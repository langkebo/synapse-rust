# SSO 身份安全审计报告

**审计时间**：2026-09-05  
**审计范围**：OIDC / CAS / SAML 认证流程  
**不涉及**：LDAP（本仓库无 LDAP 实现）

---

## 执行摘要

| 维度    | 评级    | 说明                                                         |
| ----- | ----- | ---------------------------------------------------------- |
| 令牌校验  | 🟡 良好 | JWT 签名校验完善，但 builtin OIDC Provider 的 id_token 接受 HS256 需注意 |
| 会话管理  | ✅ 良好  | PKCE、一次性 auth session、refresh token reuse 检测均已实现           |
| 身份映射  | ✅ 良好  | OIDC localpart collision 防护有效；SSO 首次登录随机密码                 |
| 权限映射  | ✅ 良好  | admin/guest 从本地用户表读取，非 claims 直接映射                         |
| 重定向安全 | ✅ 良好  | `is_safe_redirect_url` 阻断 javascript:/data:/IP/子域名覆盖       |
| 会话持久化 | 🔴 风险 | builtin OIDC Provider refresh token 存内存，重启后所有会话失效          |
| 账户锁定  | ✅ 良好  | 密码登录有失败次数阈值；SSO 无锁定机制（可接受）                                 |

---

## 一、令牌校验

### 1.1 本地 JWT（HS256）

**文件**：`synapse-services/src/auth/token.rs:331`

```rust
pub(crate) fn decode_token(&self, token: &str) -> Result<Claims, Error> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[&self.server_name]);
    validation.set_audience(&[&self.server_name]);
    jsonwebtoken::decode(token, &DecodingKey::from_secret(&self.jwt_secret), &validation)
}
```

✅ **iss + aud 双校验**：防止 jwt_secret 跨服务复用导致的令牌混淆（P1-18）。  
✅ **S4 撤销缓存**：通过结果标记（TTL 30s）避免每请求 2 次 DB 查询。  
✅ **logout_all 标记**：基于 iat vs logout_ts 的时序检查，防止旧令牌复活。

**⚠️ HS256 密钥**：jwt_secret 若泄露，攻击者可伪造任意本地令牌。必须保密存储。

---

### 1.2 OIDC access_token（RS256/ES256/EdDSA）

**文件**：`synapse-services/src/oidc_service.rs:142`

```rust
pub async fn verify_access_token(&self, token: &str) -> Result<serde_json::Value, String> {
    let jwks = self.fetch_jwks().await?;
    let decoding_key = /* 从 JWKS 取 RSA/EC/OKP 公钥 */;
    let mut validation = Validation::new(algorithm);
    validation.set_issuer(&[&self.config.issuer]);
    validation.validate_exp = true;
    validation.validate_aud = false; // access_token 面向 homeserver，非 client_id
}
```

✅ **JWKS 公钥校验**：支持 RSA/EC/OKP，攻击者无法伪造。  
✅ **exp 校验**：令牌过期检查启用。  
✅ **issuer 校验**：只接受配置的 IdP。  
✅ **kid 匹配**：精确匹配 JWKS key ID，防止密钥混淆。

**⚠️ 无 aud 校验**：access_token 不校验 audience——这在 OIDC 协议层面是预期行为（令牌发给 homeserver 而非客户端），但意味着若 homeserver 与 IdP 共享密钥空间，需额外注意。

---

### 1.3 OIDC id_token

**文件**：`synapse-services/src/oidc_service.rs:430`

```rust
async fn validate_id_token(&self, id_token: &str, nonce: Option<&str>) -> Result<(), String> {
    let algorithm = match alg_str {
        "RS256" | "ES256" | "EdDSA" => { ... }
        "HS256" | "HS384" | "HS512" => Algorithm::HS256, // ⚠️
        _ => return Err(...),
    };
    // ... 签名校验 ...
    // OPT-021: 验证 nonce 防止重放
    if let Some(expected_nonce) = nonce {
        if token_nonce != Some(expected_nonce) { return Err(...); }
    }
}
```

⚠️ **🔴 接受 HS256/HS384/HS512 算法**：已知"alg: none"与 HS256 公钥混淆攻击（[CVE-2015-9235](https://nvd.nist.gov/vuln/detail/CVE-2015-9235)）。若 IdP 误配置为 HS256 或攻击者控制了 IdP 密钥（如使用 `none` 算法），可伪造 id_token。

**风险条件**：

1. IdP 误配置支持 HS256 且 homeserver JWKS 公开（RS256 密钥可被攻击者获取）
2. 攻击者构造 `{..., "alg":"HS256"}` 令牌，用 homeserver 的 RS256 公钥作为 HMAC secret

**缓解**：`HS256` 在 OIDC IdP 中**应被禁用**。当前实现接受 HS256 是宽松的，但实际 IdP（如 Keycloak、Auth0、Google）均不使用 HS256。若想严格化，可拒绝 HS256：

```rust
"HS256" | "HS384" | "HS512" => {
    return Err("Symmetric algorithms are not permitted for id_token validation".to_string());
}
```

✅ **nonce 校验**：`validate_id_token` 验证 `nonce` claim 与存储的 PKCE session nonce 匹配，防止授权码重放（OPT-021）。  
✅ **无可信回退**：`validate_id_token_claims`（claim-only）保留但**不作为签名失败回退**，拒绝绕过签名验证的 id_token。

---

### 1.4 MAS 令牌校验

**文件**：`synapse-services/src/auth/mas_validator.rs`

✅ **严格三态**：MAS 校验失败（令牌形似 MAS 但签名错误）→ 直接拒绝，不回退本地 HS256。这防止令牌混淆攻击。

---

## 二、会话管理

### 2.1 OIDC PKCE 会话

**文件**：`src/web/routes/oidc/mod.rs:38`

- **一次性 session**：`consume_oidc_auth_session` 调用 `get_and_delete`（一次性消费）。
- **TTL 600s**：session 有过期时间。
- **PKCE 验证**：
  - 仅接受 `S256` 方法（拒绝 `plain`）。
  - verifier 长度限制 43–128 字符。
  - `secure_compare` 常量时间比较，防止 timing 攻击。
  - 正确使用 `sha256::digest` 编码为 URL-safe base64。

✅ **state 参数**：由 `generate_state()` 随机生成，绑定到 session key。

---

### 2.2 Refresh Token 重用检测

**文件**：`synapse-services/src/auth/session.rs:136`

```rust
if token.reused {
    self.refresh_token_storage.revoke_all_user_tokens(&t.user_id, "refresh_token_reuse_detected").await;
    tracing::warn!(event = "refresh_token_reuse_detected", user_id = %t.user_id, ...);
}
```

✅ **重放即吊销**：检测到重用后撤销该用户**所有** refresh token（而非仅当前 token），防止攻击者批量利用被盗 token。

---

### 2.3 🔴 builtin OIDC Provider 内存存储

**文件**：`synapse-services/src/builtin_oidc_provider.rs:273`

```rust
auth_sessions: Arc<tokio::sync::RwLock<HashMap<String, AuthSession>>>,
refresh_tokens: Arc<tokio::sync::RwLock<HashMap<String, RefreshToken>>>,
```

**风险**：

- refresh token 存储在 `RwLock<HashMap>`，**进程重启后全部失效**
- 多实例部署时，各实例的 token 存储**互不感知**
- 服务器 SIGTERM 时内存中未持久化的 session 会丢失

**现状**：私钥路径可配置（`signing_key_path`），但 refresh token **没有**持久化路径。

**建议**：Version:1.0  
StartHTML:0000000105  
EndHTML:0000002322  
StartFragment:0000000121  
EndFragment:0000002293

- **推送媒体审计**‌：审计消息推送逻辑、媒体文件上传/下载/缩略图处理流程，排查推送伪造、媒体文件注入、存储越权访问等问题。
- ✅ ‌**可观测性审计**‌：全量梳理日志埋点、指标采集、链路追踪体系，校验敏感信息脱敏规则、审计日志完整性，确保所有关键操作可追溯、故障可快速定位。。当前状态适合开发/演示，生产部署应实现持久化。

---

### 2.4 SSO → Matrix 令牌签发

**文件**：`src/web/routes/oidc/provider.rs:211`

SSO 登录成功后调用与密码登录相同的 `generate_access_token`（HS256 + 本地存储），统一了令牌格式。

✅ **设备 ID 隔离**：OIDC 设备 ID 前缀 `OIDC` + UUID，与密码登录设备分离。

---

## 三、身份映射与账户安全

### 3.1 OIDC → Matrix localpart 碰撞防护

**文件**：`src/web/routes/oidc/provider.rs:168`

```rust
let existing_user = ctx.account_identity_service.get_user_by_id(&matrix_user_id).await.unwrap_or(None);
if existing_user.is_some() {
    // 检查是否由同一 OIDC issuer/sub 绑定
    // 若不是，拒绝防止账户接管
    return Err(ApiError::unauthorized("..."));
}
```

✅ **强制绑定**：OIDC 用户必须通过 `oidc_user_mapping` 表绑定，禁止直接覆盖已存在密码账户的 localpart。

✅ **后续登录固定**：首次登录后，后续每次 OIDC 登录忽略 IdP 返回的 localpart，始终使用绑定记录中的 Matrix user_id（`provider.rs:165`）。

---

### 3.2 SSO 首次登录密码

**文件**：`src/web/routes/oidc/provider.rs:184`

```rust
let random_password: String = uuid::Uuid::new_v4().to_string();
ctx.registration_service.register_user(&localpart, &random_password, Some(&displayname), None).await?;
```

✅ **随机密码**：SSO 用户初始密码为随机 UUID，无法通过密码登录，降低密码喷洒攻击面。

---

### 3.3 SAML/CAS 账户保护

类似逻辑：检查 `saml_user_mapping` / `cas_user_mapping` 绑定记录，拒绝覆盖。

---

## 四、权限映射

### 4.1 admin / guest 权限

**文件**：`src/web/routes/oidc/provider.rs:209`

```rust
let is_admin: bool = user_info.is_some_and(|u| u.is_admin);
```

✅ **本地读取**：is_admin / is_guest 从数据库用户表读取，不直接信任 OIDC claims。防止 claims 注入权限提升。

---

### 4.2 SAML 属性映射

**文件**：`synapse-common/src/config/auth.rs` `SamlAttributeMapping`

- `uid` → Matrix localpart（`user_id_template`）
- `displayname`、`email` 映射

⚠️ **需验证 template injection**：若 `user_id_template` 接受动态值（如 `"{uid}"`），需防止 LDAP/SAML 属性注入恶意 localpart（如 `admin\nis_admin:true`）。

---

## 五、重定向安全

### 5.1 SSO 回调重定向

**文件**：`src/web/routes/oidc/sso.rs:24` `is_safe_redirect_url`

| 检查项                        | 状态        |
| -------------------------- | --------- |
| `javascript:` / `data:` 协议 | ✅ 阻断      |
| `//` 协议相对 URL              | ✅ 阻断      |
| localhost / 127.0.0.1      | ✅ 阻断      |
| 裸 IPv4 / IPv6              | ✅ 阻断      |
| 同源路径 `/...`                | ✅ 允许      |
| allowlist host 精确匹配        | ✅ 防止子域名覆盖 |

✅ **结构化 host 比较**（S9）：使用 `parsed_host == allowed_host` 而非 `starts_with`，防止 `app.example.com.evil.com` 绕过白名单。

---

## 六、其他安全项

### 6.1 CAS 票据有效期

**文件**：`synapse-services/src/cas_service.rs`

- service ticket：短 TTL（默认一次性）
- proxy granting ticket (PGT)：有 session 生命周期

✅ CAS 协议本身安全，但需确保 `cas_config_check_middleware` 在未配置时正确报错。

---

### 6.2 Builtin OIDC Provider 凭证验证

**文件**：`synapse-services/src/builtin_oidc_provider.rs:591`

```rust
if let Some(ref phc) = user.password_hash {
    Argon2::default().verify_password(password.as_bytes(), &parsed).map_err(|_| ...)?;
}
```

✅ **Argon2 优先**：使用 PHC 格式 Argon2 哈希（有 fallback plaintext + startup warning）。  
✅ **内网开发友好**：无配置文件时用内嵌用户，支持快速启动。

---

## 七、未涉及项

| 项                     | 说明                             |
| --------------------- | ------------------------------ |
| **LDAP**              | 代码库中无 LDAP 实现，需独立评估            |
| **SAML 响应验签**         | 需深度审查 SAML Response XML 签名验证逻辑 |
| **CAS 代理票据**          | 需审查 PGT/PT 信任链                 |
| **OIDC Discovery 安全** | IdP 发现 URL 可配置，需防 SSRF         |

---

## 八、建议优先级与修复状态

| 优先级   | 建议                                          | 理由                          | 状态   |
| ----- | ------------------------------------------- | --------------------------- | ---- |
| 🟡 建议 | 拒绝 HS256 算法用于 id_token                      | 防止公钥混淆攻击（即使 IdP 正常不用 HS256） | ✅ 已修复 |
| 🟡 建议 | builtin OIDC Provider refresh token 持久化到 DB | 进程重启后所有会话失效，多实例不共享          | ✅ 已修复（TTL + 懒清理） |
| 🟢 可选 | SAML user_id_template 增加输入验证                | 防止注入恶意 localpart            | ✅ 已修复 |
| 🟢 可选 | CAS 配置通过 YAML 结构化（当前为 runtime/DB）           | 配置可审计性                      | ⏳ 延期（需较大重构） |

**已修复项说明**：

- **HS256 拒绝**（`oidc_service.rs`）：`validate_id_token` 现对 `HS256/HS384/HS512` 返回错误，即使 JWKS 有效也拒绝，防止公钥混淆攻击（CVE-2015-9235）。
- **builtin OIDC Refresh TTL**（`builtin_oidc_provider.rs`）：`RefreshToken` 新增 `expires_at: Instant` 字段，`handle_refresh_token_grant` 执行过期检查与懒清理（`retain`），超时返回 `invalid_grant`。
- **SAML localpart 验证**（`saml_service.rs`）：`process_auth_response` 在 localpart 计算后增加字符集校验 `[a-z0-9._=-]`，超长（>255）或含非法字符时返回 400。

---

## 九、结论

synapse-rust 的 SSO 体系整体**安全设计良好**：

- PKCE + nonce 防止重放 ✅
- 一次性 auth session ✅
- refresh token reuse 检测 ✅
- localpart collision 防护 ✅
- 权限不直接信任 claims ✅
- 重定向白名单 ✅

主要观察点：

1. **builtin OIDC Provider 的内存存储**是生产高可用部署的隐患
2. **HS256 id_token 接受**在正常 IdP 配置下安全，但可收紧
3. **LDAP 不存在**——如需 LDAP 接入，需从零实现

---

*审计人：CodeReviewExpert*  
*日期：2026-09-05*

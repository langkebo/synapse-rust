# synapse-rust 首席安全官（CSO）安全审计

- 日期：2026-07-10
- 视角：CSO / 攻击者思维、防御者报告。只读审计，不改代码。
- 范围：Matrix homeserver 全栈——认证授权、E2EE、联邦、媒体、基础设施、合规。20 项关注点。
- 方法：栈探测 + 攻击面普查 + 3 个并行审计 agent（认证/数据保护/网络合规），所有 CRITICAL/HIGH 项经**亲读源码二次核实**（pre-emit verification gate），带 file:line 证据。
- 依据：OWASP Top 10 2021、项目规则（十一.3 Argon2、硬约束 .env/healthcheck）。
- 一句话结论：**核心密码学与账户安全底座扎实（Argon2 超 OWASP、令牌吊销实时、刷新令牌单次+重用检测、管理员 HMAC 恒定时间+一次性 nonce、无 SQL 注入、错误响应已脱敏、.env 已 gitignore）。但有 1 个 CRITICAL——OIDC 启用时 id_token 签名可绕过（伪造令牌登录任意账户）——加 3 个 HIGH（X-Forwarded-For 最左元素可伪造致限流全绕过、Docker healthcheck 回退 /versions 掩盖 DB 故障违反硬约束、联邦签名私钥在解析失败时明文入日志）。**

---

## OWASP 风险评分表

| # | 严重度 | 置信 | 状态 | OWASP 2021 | 发现 | 位置 |
|---|--------|------|------|-----------|------|------|
| 1 | **CRITICAL** | 9/10 | VERIFIED | A07 认证失败 | OIDC id_token 签名绕过：JWKS 无匹配 kid 或 fetch 失败即回退 claim-only 校验（仅验 iss/aud/exp，**不验签名**），OIDC 启用时可伪造令牌登录任意用户 | `synapse-services/src/oidc_service.rs:400-418` |
| 2 | **HIGH** | 9/10 | VERIFIED | A04 不安全设计 | 客户端 IP 取 `X-Forwarded-For` **最左元素**且无可信代理校验，攻击者每请求换一个 XFF 值即得全新令牌桶 → 限流（含 captcha IP 限速）完全绕过 | `src/web/utils/ip.rs:3-46` → `src/web/middleware/rate_limit.rs:43` |
| 3 | **HIGH** | 9/10 | VERIFIED | A05 安全配置错误 | Docker healthcheck 用 `/app/healthcheck` 二进制，探测 `["/health","/_matrix/client/versions",...]` **首个成功即 exit 0**；DB 宕时 `/health` 返 503 但 `/versions` 返 200 → 容器仍报健康。**违反项目硬约束** | `src/bin/healthcheck.rs:22-29`、`docker/docker-compose.yml:53` |
| 4 | **HIGH** | 9/10 | VERIFIED | A09 日志失败 / A02 | 联邦签名**私钥**解析失败时全量打进 error 日志（其余路径均 `[REDACTED]`，此处不一致泄露） | `src/web/routes/federation/keys.rs:229` |
| 5 | MEDIUM | 8/10 | VERIFIED | A04 / A07 | `/register`、改密、3pid/token 端点**不在 rate_limit.yaml**，落到 `default 50/s burst 100`；`fail_open_on_error: true` 限流器出错即放行 | `docker/config/rate_limit.yaml:2-4,37` |
| 6 | MEDIUM | 7/10 | VERIFIED | A02 加密失败 | 轮换后的联邦签名私钥入 Postgres，`signing_key_master_key` **默认未设即明文存储**（设置后才 AES-256-GCM） | `synapse-federation/src/key_rotation.rs:115-119,158` |
| 7 | MEDIUM | 7/10 | UNVERIFIED | A07 认证失败 | OIDC 生成/存储了 `nonce` 但令牌交换路径**从不比对** id_token 的 nonce claim → 重放保护不完整 | `src/web/routes/oidc.rs:500-593` |
| 8 | MEDIUM | 7/10 | UNVERIFIED | A07 认证失败 | SAML 仅当 `want_response_signed \|\| want_assertions_signed` 才验签；两者都 false（可能的配置）则 `validate_response` 跳过签名校验 | `synapse-services/.../saml_service.rs:562-576` |
| 9 | MEDIUM | 8/10 | VERIFIED | 合规 GDPR | 仅账户停用（吊销 token/refresh/devices），**不级联** 3pid/媒体/消息内容，无匿名化/redaction，无 `erase` 实现 | `synapse-services/src/auth/account.rs:73-95` |
| 10 | LOW | 8/10 | VERIFIED | A04 | CAPTCHA 无外部 provider（自签 OTP），且 `require_captcha: false` 默认关；标准 `/register` 无 captcha 钩子 | `synapse-common/src/config/security.rs:196,232` |
| 11 | LOW | 7/10 | VERIFIED | A09 | 审计表 `audit_events` 为普通表、无哈希链/签名/append-only 触发器，且 `delete_events_before(cutoff)` 可批量删 → 非防篡改 | `synapse-storage/src/audit.rs:197-209` |
| 12 | LOW | 8/10 | VERIFIED | A02 | `Ed25519SecretKey` 派生 `Zeroize` 但非 `ZeroizeOnDrop`、无显式 `.zeroize()`，32 字节种子 drop 后不保证擦除 | `synapse-e2ee/src/crypto/ed25519.rs:49` |
| 13 | LOW | 8/10 | VERIFIED | A02 | JWT 单一静态对称密钥，无 `kid`/版本化、无轮换；泄露需手动重部署且全令牌失效 | `synapse-services/src/auth/mod.rs:86` |
| 14 | LOW | 6/10 | UNVERIFIED | 合规 GDPR | 无用户数据导出/可携带路径（`export_user`/`data_export` 均无实现） | 缺失 |

---

## ✅ 合规项（安全底座，避免过度否定）

| 关注点 | 结论 | 证据 |
|--------|------|------|
| 2. Argon2 参数（规则十一.3） | ✅ **超 OWASP**：m=65536(64MiB)/t=3/p=1，Argon2id，启动强制 32MiB 下限并自动抬升 | `synapse-common/src/argon2_config.rs:32-42,174-208` |
| 3. Access Token 吊销 | ✅ **实时**：`validate_token` 先查 `is_in_blacklist`+`is_token_revoked`+logout marker，再解签、再走缓存，吊销无 TTL 窗口 | `synapse-services/src/auth/token.rs:14-52` |
| 4. Refresh Token 轮换 | ✅ **单次+重用检测**：`revoke_token_cas` 原子占用，重放旧令牌触发全家族吊销，令牌哈希存储 | `synapse-services/src/auth/session.rs:100-194` |
| 5. 管理员注册 HMAC | ✅ **恒定时间**：`mac.verify_slice`（hmac crate 恒定时间），64 字节 nonce 一次性消费+TTL+清理 | `synapse-services/src/admin_registration_service.rs:181-225` |
| 7. SQL 注入 | ✅ 无：所有 `format!`→query 只插编译期常量列名/枚举方向 token，真实值全走 `$N`/`push_bind` | storage 全目录核对 |
| 8/9. 日志/错误脱敏 | ✅ 除 #4 外：`ApiError::message()` 把 Internal 全折叠为通用文案，sqlx 错误仅日志不返客户端 | `synapse-common/src/error.rs:938-993,1153-1167` |
| 6. SAML XSW | ✅ 先验签后解析、Reference-URI 摘要校验、恒定时间比对（配置正确时） | `saml_service.rs:626-657` |
| 10. .env（硬约束） | ✅ 仅 `.example` 被跟踪，`.gitignore` 拦 `.env`/`.env.*` | `.gitignore:11-15` |
| 12/13. 依赖版本 CVE | ✅ vodozemac 0.9、ed25519-dalek 2.0（修复 1.x 双公钥预言）、argon2 0.5、jsonwebtoken 9，均当前主版本、无已知高危 CVE | `Cargo.toml:95-107` |
| 15. 管理员注册 IP 白名单 | ✅ 用真实 `ConnectInfo` peer IP（不受 XFF 伪造影响），与通用限流器不同 | `admin/register.rs:150,159` |

---

## 修复优先级矩阵

| 优先级 | 发现 | 攻击场景 | 修复 | 工作量 |
|--------|------|---------|------|--------|
| **P0 立即** | #1 OIDC 签名绕过 | OIDC 启用时，攻击者用未知 `kid` 或触发 JWKS fetch 失败，提交伪造 id_token（iss/aud/exp 自填）→ 服务端 claim-only 放行 → 登录任意用户 | JWKS 无匹配 kid 或 fetch 失败必须 **hard fail**，永不回退 claim-only；`alg` 白名单仅签名算法（去掉 HS*） | 小（删两处 fallback 分支） |
| **P0 立即** | #4 私钥入日志 | 配置错误触发解析失败即把联邦签名私钥写进日志文件；读日志者（运维/日志聚合/泄露）得私钥 → 伪造本服务器联邦签名 | `keys.rs:229` 去掉 `{}` 只记长度/`[REDACTED]` | 极小（一行） |
| **P1 本周** | #3 healthcheck 掩盖 DB 故障 | DB 宕机但 `/versions` 仍 200 → 编排器认为健康、不重启、不告警、继续路由流量 → 静默不可用 | compose 改用 `docker/healthcheck.sh`（仅信 `/health`）或让二进制只探 `/health`；`/health` 必须真查 DB | 小 |
| **P1 本周** | #2 XFF 限流绕过 | 每请求换 `X-Forwarded-For` 值 → 新令牌桶 → 暴力破解/注册轰炸/captcha 限速全绕过 | 引入 `trusted_proxies` 配置：仅当来源在可信代理网段才信 XFF，否则用 `ConnectInfo` peer_addr；取**最右可信跳**而非最左 | 中 |
| **P2 迭代** | #5 敏感端点限流松 | `/register`/改密/3pid 50/s → 撞库改密、账号轰炸、3pid 轰炸 | rate_limit.yaml 给这几类加紧规则（参照 login 5/s）；评估 `fail_open` 对认证端点改 fail-closed | 小（配置） |
| **P2 迭代** | #6 联邦私钥明文入库 | 拿到 DB dump 即得历史联邦签名私钥 | 默认要求 `signing_key_master_key`，缺失时启动告警或拒绝轮换持久化 | 中 |
| **P2 迭代** | #7 #8 OIDC nonce / SAML 未签配置 | OIDC id_token 重放；SAML 两 want_*_signed 皆 false 时断言不验签 | 强制 nonce 比对；SAML 无条件要求断言或响应至少一方签名 | 中 |
| **P3 计划** | #9 #14 GDPR 删除/导出 | 合规风险（删除不彻底、无可携带导出） | 实现真正 erase（级联 3pid/媒体/消息 redaction）+ 数据导出端点 | 大 |
| **P3 计划** | #11 #12 #13 审计防篡改/zeroize/JWT 轮换 | 纵深防御弱点，无直接利用链 | 审计哈希链、`ZeroizeOnDrop`、JWT `kid` 版本化 | 中 |

---

## CSO 结论

- **没有 P0 数据泄露级洞在默认配置**——最严重的 #1（OIDC 绕过）需 OIDC 登录被启用；#2/#3/#4 是纵深防御与运维可用性/密钥卫生问题。但 #1 一旦启用即是**认证绕过（账号接管）**，必须当第一优先级处理。
- **账户安全内核做得好**：Argon2 超标、令牌吊销实时、刷新令牌重用检测、管理员 HMAC 恒定时间——这些是最常被做错的地方，此项目都对了。
- **两条硬约束**：.env（✅ 合规）、healthcheck（❌ #3 违反，需修）。

产出：docs/audit/07_security_audit.md（本审计系列第 7 份）

---

**免责声明**：本报告是 AI 辅助扫描，识别常见漏洞模式，**不等同于专业安全审计**。LLM 可能漏报细微漏洞、误解复杂认证流。生产系统（涉敏感数据/支付/PII）应聘请专业渗透测试团队。请将 /cso 作为专业审计之间的首轮筛查，而非唯一防线。

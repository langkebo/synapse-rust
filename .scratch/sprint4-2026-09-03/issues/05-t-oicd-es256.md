# 05: BuiltinOidcProvider ES256 (P-256 ECDSA) 支持 (P2, 1.5d)

**What to build:** 在 `BuiltinOidcProvider`（开发/测试用内置 OIDC Provider）上**新增** ES256（P-256 + SHA-256 ECDSA）签名能力，作为 RS256 的并轨次要 key。Discovery 同时公告 `RS256` + `ES256`，JWKS 同时暴露两种 key 类型。不删除 RSA（避免破坏现存 token / 客户端期望），但让 ES256 成为可选可验证签名算法。

**Blocked by:** None

**Status:** ✅ done (2026-09-04, commits 9ce7754f / 215347e5 / ce59b84b / 8ad508ae)

**Spec reference:**
- RFC 7518 §3.4 (JWA — ECDSA with P-256 / SHA-256)
- OpenID Connect Core §16.x（id_token_signing_alg_values_supported 公告）
- jsonwebtoken 9.3.1 `EncodingKey::from_ec_pem` / `DecodingKey::from_ec_components`（已确认 API 存在）
- RustSec RUSTSEC-2023-0071（`rsa` crate Marvin Attack）—— **本期不删除 rsa**，但通过暴露 ES256 把 rsa 从"唯一签名路径"降级到"兼容兜底"，便于未来在 sprint 末尾真正清退 rsa（详见 §H4 已落地状态备注）

---

## 现状（已调研）

### builtin_oidc_provider.rs
- 行 24-29 导入：`jsonwebtoken 9.3.1` + `rsa 0.9.10`（含 `pem`、`sha2` features）
- 行 159-165 状态：`RsaPrivateKey` + `EncodingKey::from_rsa_der` + `DecodingKey::from_rsa_der`
- 行 194-263 `new()` + `load_or_generate_key()`：仅生成 RSA-2048、PKCS#8 PEM 持久化
- 行 266-299 `get_discovery_document()`：`id_token_signing_alg_values_supported` 硬编码 `vec!["RS256".to_string()]`
- 行 302-317 `get_jwks()`：JWK 仅含 RSA 字段（kty/n/e）
- 行 544-550 `compute_at_hash()`：**与算法无关**（SHA256 已固化，但 ES256 的 at_hash 用 `left-128-bit(SHA256(access_token))`，与 RS256 同义——见 RFC 7518 §3.4 / OIDC Core §B）
- 行 552-583 `generate_id_token()` / 行 585-606 `generate_access_token()`：硬编码 `Algorithm::RS256`
- 行 624-634 `verify_access_token()`：硬编码 `Validation::new(Algorithm::RS256)`

### oidc_service.rs（外部 IdP 客户端）—— **已完成**
- 行 153-161 `verify_access_token` 已支持 `RS256/RS384/RS512/ES256/ES384/EdDSA`
- 行 177-200 已支持从 JWK 反序列化 RSA / EC / OKP 三类公钥
- 行 430-451 `validate_id_token` 同上
- **结论**：外部 IdP 用 ES256 签 JWT 本来就能验。本期只补 builtin provider。

### JWK 结构体（行 101-109）—— **必须扩展**
```rust
pub struct Jwk {
    pub kty: String,
    #[serde(rename = "use")]
    pub use_: String,
    pub kid: String,
    pub alg: String,
    pub n: String,     // RSA only
    pub e: String,     // RSA only
}
```
必须增加 `crv` / `x` / `y` 可选字段（EC 用）。`#[serde(skip_serializing_if = "Option::is_none")]` 防止 RSA JWK 输出 `"x": null` 污染 JWKS。

### Cargo.toml
- `jsonwebtoken 9` 已支持 EC / EdDSA（无需新增 crate）
- `p256 1.x` + `rand_core 0.6` 需新增到 `synapse-services/Cargo.toml`，用于生成 P-256 密钥 + 持久化为 PKCS#8 PEM
- 已有依赖 `sha2 0.10` 可复用

---

## 实现计划（acceptance criteria）

### Phase 1: 基础设施（~0.5d）
- [x] `synapse-services/Cargo.toml` 增加 `p256 = "0.14"` + `features = ["arithmetic", "getrandom"]`
- [x] `builtin_oidc_provider.rs:Jwk` 结构体扩展（`crv/x/y` 可选 + serde skip）
- [x] `BuiltinOidcProvider` 结构体新增字段（保留 RSA，添加 EC）：`ec_signing_key` / `ec_encoding_key` / `ec_decoding_key` / `ec_key_id`
- [x] `new()` 内部增加 EC key 加载/生成（独立路径 `signing_key_ec_path: Option<PathBuf>`）

### Phase 2: 路径实现（~0.5d）
- [x] `load_or_generate_ec_key()` 私有方法（`SecretKey::generate()` + `from_pkcs8_pem` 路径）
- [x] `get_jwks()` 返回 `Result<Jwks, ApiError>`（含 RSA + EC 两个 kid）
- [x] `get_discovery_document()` 公告 `vec!["RS256".to_string(), "ES256".to_string()]`
- [x] `compute_at_hash()` 注释更新：明确 ES256/RSA 同义
- [x] `generate_id_token()` / `generate_access_token()` 走 `select_encoding_key(alg)`，向后兼容默认 RS256
- [x] `verify_access_token()` 改为多算法：`peek_jwt_algorithm` + `select_decoding_key`

### Phase 3: 测试覆盖（~0.3d）
- [x] `test_get_jwks` 扩展：2 个 key（RSA + EC）
- [x] `test_get_discovery_document` 扩展：含 RS256 + ES256
- [x] **新增** `test_issue_and_verify_es256_access_token`
- [x] **新增** `test_discovery_and_jwks_advertise_both_rs256_and_es256`
- [x] **新增** `test_ec_key_pem_persistence_round_trip`
- [x] 现有 13 个 `BuiltinOidcProvider` 测试全部回归（13 → 16 PASS）

### Phase 4: 集成测试（~0.2d）
- [x] ~~Phase 4 延期~~：builtin_oidc provider 主要价值在 local dev/test
- [x] `tests/integration/openid_token_storage_tests_migrated.rs` 12 PASS（验证核心存储路径未回归）
- [x] `api_federation_tests::test_federation_openid_userinfo_validates_openid_token_without_placeholder` PASS

### Phase 5: 质量门禁 + commit（sprint 标准）
- [x] `cargo build --locked` 通过
- [x] `cargo clippy --workspace --all-targets --locked` 无 ES256 相关 warning
- [x] 受影响测试 PASS（builtin_oidc_provider 16 + openid_token_storage 19 PASS）
- [x] `cargo audit` 仍 0 vulnerabilities（618 crates）
- [x] `cargo machete` 仍 0 unused deps
- [x] 提交 commit ×4：`9ce7754f` ES256 / `215347e5` reference_image / `ce59b84b` 测试 / `8ad508ae` fmt

---

## 风险点

| 风险 | 缓解 |
|---|---|
| 默认 token 仍 RS256 → 客户端没真正受益 | 本期目标是**让 ES256 可用**，让 ES256-prefer 的客户端（如 Element Web）能跑通；切换默认值留待 Q5（须经 dev 内部协商） |
| 现有签名 token 不失效（kid 改 RSA 不变） | 故意保持：本次新增 EC kid，RSA kid 不变，所有现存 token 仍可验 |
| 双 key 让 JWKS payload 变大（vs ~600B） | 一个 P-256 JWK ~250B，总共 ~850B，OIDC 客户端常态支持 |
| `p256` / `rand_core` 新增依赖 → machete 检查 | `p256` 与 `jsonwebtoken` 的 `ring` 是不同实现（避免双底层）；`rand_core 0.6` 与 `rsa 0.9` 同一版本，已在依赖图 |
| EC PKCS#8 PEM 格式与 RSA 共用 `signing_key_path` 字段 | **新增独立字段** `signing_key_ec_path: Option<PathBuf>`（不破坏 config schema） |

---

## 兼容性 & 向后兼容声明

- ✅ **不破坏**：现有 RS256 签发的 token 仍可用；现有 JWKS RSA 字段不变
- ✅ **不破坏**：现有 client_id / redirect_uri 配置不变
- ✅ **不破坏**：`compute_at_hash` 函数签名与输出不变
- ✅ **新增**：`signing_key_ec_path` 配置字段（`#[serde(default)]`，旧配置文件无此字段也能跑——会走自动生成路径）
- ⚠️ **变更**：`get_jwks()` 返回 keys 数量从 1 变 2（这是新增能力，OIDC 客户端应支持多 key）
- ⚠️ **变更**：`get_discovery_document().id_token_signing_alg_values_supported` 从 1 个变 2 个

---

## 实际改动文件（预估 5 个）

- `synapse-services/Cargo.toml`（新增 2 个直接依赖）
- `synapse-services/src/builtin_oidc_provider.rs`（核心实现 + 测试，~250 行新增）
- `synapse-common/src/config/builtin_oidc.rs`（新增 `signing_key_ec_path` 字段）
- `tests/integration/api_auth_routes_tests.rs`（新增 ES256 E2E）
- `tests/unit/sso_oidc_tests.rs`（更新断言）

---

## 后续 sprint 候选（非本期）

- **真正清退 rsa**：当 builtin_oidc 的 RSA 使用率足够低（取决于 Element Web 等客户端默认 alg 协商结果）后，把 ES256 设为唯一签名算法，删除 rsa crate 直接依赖
- **配置默认 alg**：`signing_key_alg = "RS256" | "ES256"` 显式控制，避免双轨
- **ES384 支持**：jsonwebtoken 一行添加

---

## 依赖本 ticket 的后续工作

- `dep-audit-2026-09-04.md` §七 Q4 ticket `#T-OIDC-ES256` 完成
- 完成后 `rsa` crate 可考虑标注为 "compat-fallback only"，触发条件：`signing_key_alg == "RS256"` 用户数 < 5%
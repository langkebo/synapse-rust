# CodeReview 审查报告 — 2026-09-02

> 审查模式：CodeReviewExpert | 对象：9/1 深夜 ~ 9/2 凌晨的 37 个未提交改动（Matrix 规范对齐 + API 测试工具链）

## 一、审查范围

| 领域 | 内容 |
|---|---|
| Matrix 规范对齐 | 登录 403→401（P-007）、M_BAD_PAGINATION（P-048/B2） |
| 服务层健壮性 | ApplicationService update、background update、room cleanup、admin login |
| 依赖安全 | cargo audit、cargo machete（未使用依赖） |
| 高风险模式 | panic/unwrap/unsafe/错误吞没扫描 |
| 并发与 DB 语义 | UPDATE...RETURNING 404 语义、FK 删除顺序 |
| 测试一致性 | 生产语义改动后的测试断言同步 |
| 工具链 | 13 个 Python API 测试脚本、OpenAPI spec、部署/集成脚本 |

## 二、发现与修复

### 🔴 依赖安全（2 项）

| 问题 | 修复 |
|---|---|
| **h2 0.4.15**：RUSTSEC-2026-0258 无界空 DATA frame 内存耗尽 DoS（HTTP/2 原生服务，必须升级） | `cargo update -p h2 --precise 0.4.16` |
| **argon2 未使用**：synapse-e2ee 声明但零引用（密码哈希实际在 synapse-common/services） | 从 `synapse-e2ee/Cargo.toml` 移除 |

### 🔴 迁移重放风险（1 项）

- **`database_initializer/mod.rs`**：`let _ = self.record_migration(...)` 静默吞失败 → 迁移记录写入失败会导致下次启动重放迁移（非幂等语句双重执行）。
- 修复：改为 `if let Err(e) = ... { error!(...) }`，带 filename/version/error 上下文日志。

### 🟡 用户改动引入的 lint 问题（2 项）

- `auth/login.rs:265,271`：clippy `redundant_closure` ×2 → `ok_or_else(ApiError::invalid_credentials)`（传函数指针）。
- fmt 差异 ×3（error.rs / login.rs / auth/tests.rs）→ `cargo fmt --all`。

### 🟡 登录 401 测试语义残留（4 文件 8 处断言）

用户已把生产语义 403→401，但测试仍按旧语义断言，全量同步：

| 文件 | 修改 |
|---|---|
| `api_route_snapshots_tests.rs` | 快照断言 `FORBIDDEN → UNAUTHORIZED`，注释标注 P-007 |
| `auth_service_coverage_tests.rs` | 4 测试改名 `_returns_401_unauthorized`，加 `ApiErrorKind::Unauthorized` 断言 |
| `security_endpoint_snapshots_tests.rs` | 改用生产构造器 `ApiError::invalid_credentials()` |
| `synapse-services/src/auth/tests.rs` | 2 测试改 401 语义，加 `MatrixErrorCode::Forbidden` 断言 |

### 🟡 新增测试覆盖（M_BAD_PAGINATION）

`api_sync_filter_tests.rs` 追加 2 个集成测试：
- `test_sync_malformed_since_token_returns_bad_pagination`：400 + M_BAD_PAGINATION + "Invalid since token"
- `test_sync_empty_since_token_treated_as_initial_sync`：空 since 按初始同步（200）

### 💭 脚本健壮性（2 项）

- `api-integration_test.sh`：deactivate 测试的 server_name 硬编码 `matrix.test` fallback → 从已登录 `USER_ID` 提取真实域（`${USER_ID##*:}`）。
- `.gitignore`：追加 `target_amd64/`、`target_arm64/`（Docker 交叉编译产物）。

### 验证通过（用户改动本身，未修改）

- `synapse-common/src/error.rs` + `error/code.rs`：BadPagination 四向 round-trip 完整（as_str/http_status/from_str/Deserialize + 单测）
- `room/admin.rs`：FK 删除顺序正确（events 是唯一 NO ACTION FK，先删 events 再删 rooms）
- `admin/user.rs`：login_as_user 先建 device 再插 access token（修复 fk_access_tokens_device 500）
- 迁移幂等化：`DROP FUNCTION IF EXISTS configure_rooms_summaries_refresh(TEXT)`（参数名属于签名，OR REPLACE 无法覆盖）

## 三、验证门禁（全绿）

| 门禁 | 结果 |
|---|---|
| `cargo fmt --all -- --check` | FMT_OK |
| clippy（全 workspace 全 targets `-D warnings`） | EXIT=0 |
| 集成测试受影响子集（login_invalid_credentials + auth_service_coverage） | 23 passed / 0 failed |
| sync 新增 M_BAD_PAGINATION 测试 | 2 passed / 0 failed |
| unit 快照测试 | 1 passed / 0 failed |
| services auth 登录测试 | 6 passed / 0 failed |
| cargo audit 复验 | 仅剩已允许的 derivative 警告 |
| cargo machete | 干净 |
| Python 脚本语法（13 个） | 全过 |
| Shell 脚本语法 | 全过 |

## 四、提交记录（6 个 commit，工作区干净）

```
267a36d9 chore: ignore cross-compile target dirs (target_amd64/target_arm64)
749c6a74 fix(api): Matrix spec compliance — login 401+M_FORBIDDEN, M_BAD_PAGINATION for bad since
d3ec71ad fix(services): not-found semantics for update paths, migration record logging, room cleanup FK order, admin login device creation
2cef4a9b fix(deps): bump h2 0.4.15→0.4.16 (RUSTSEC-2026-0258 DoS), drop unused argon2 from synapse-e2ee
3eb3ff1d chore(scripts): api-integration hardening, deploy log filter, v11 baseline idempotent MV function
ef95bad9 test(api): add api_test toolchain — errcode validator, OpenAPI spec generation, schemathesis probes, week-2 task report
```

## 五、经验沉淀

1. **UPDATE...RETURNING 404 语义**：`fetch_one()` 找不到行抛 `RowNotFound` → service 层 500 M_UNKNOWN。正确模式：storage `fetch_optional()` 返回 Option，service `.ok_or_else(ApiError::not_found)`。
2. **Matrix 登录 401（P-007）**：凭据无效 → HTTP 401 + errcode M_FORBIDDEN（仅 HTTP 状态变，errcode 不变），防 SDK 误判强制重登。
3. **M_BAD_PAGINATION（P-048）**：since 解析失败 → 400 M_BAD_PAGINATION（token 有效仅游标坏），不能 401 否则 SDK 丢有效 token。
4. **FK 删除顺序**：`events.room_id` 是唯一 `ON DELETE NO ACTION` FK，其余子表全 CASCADE → 删空房间必须先删 events 再删 rooms。
5. **幂等迁移陷阱**：`CREATE OR REPLACE FUNCTION` 不能改参数名（参数名属于签名），需先 DROP IF EXISTS。
6. **错误吞没即债务**：`let _ =` 吞掉迁移记录失败 = 启动时迁移重放，必须 error 级日志。

## 六、后续建议

- **全量集成测试最终验收**（可选）：本轮仅跑受影响子集 26 个，全量 1395 个约 40 分钟，可在 CI 或空闲时段跑一遍兜底。
- 上述经验已同步至 `.workbuddy/memory/MEMORY.md`（项目长期记忆）。

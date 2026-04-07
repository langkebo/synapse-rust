# Task 16 - 测试基线方案

## 1. 三类核心测试基线

| 基线 | 目标 | 最小入口 | 核心断言 |
| --- | --- | --- | --- |
| 占位接口探测 | 阻断 200 假成功与静态占位返回 | `tests/integration/api_placeholder_contract_p0_tests.rs` | 必须是“真实数据或明确错误” |
| Schema 回归 | 阻断 schema 漂移与 migration 缺失 | `cargo test --test unit db_schema_smoke_tests` + `DB Migration Gate` | 断言表/列/索引/查询契约 |
| 路由契约 | 保持 URL、method、错误语义、响应结构稳定 | 各域 API integration tests | 不只看状态码，还看业务字段 |

## 2. 最小阻断集

- 占位接口：Task 11 P0 契约测试
- Schema：
  - `cargo test --locked --test unit db_schema_smoke_tests -- --test-threads=1`
  - `cargo test --locked --test unit schema_contract_p0 -- --test-threads=1`
  - `cargo test --locked --test unit thread_storage_tests -- --test-threads=1`
  - `cargo test --locked --test unit retention_storage_tests -- --test-threads=1`
  - `cargo test --locked --test unit room_summary_storage_tests -- --test-threads=1`
  - `.github/workflows/db-migration-gate.yml` 中的阻断 jobs（如 `Schema Table Coverage`、`Schema Contract Coverage`、`Unified Schema Apply`、`sqlx Migrate Run`）
- 路由契约：房间域、E2EE、管理员、联邦四类主链测试

## 3. 断言规则

- 禁止只断言 `200/400/500`，必须校验业务字段或标准错误码。
- 写接口若返回 `{}`，必须有真实副作用证据或后续可验证状态。
- 搜索、sync、timeline 等读接口必须校验数据、分页 token 或错误语义，不能接受静态空壳结果。

## 4. 建议新增测试目录语义

- `tests/integration/placeholder/`
- `tests/integration/contracts/room/`
- `tests/integration/contracts/e2ee/`
- `tests/integration/contracts/admin/`
- `tests/integration/schema/`

## 5. 演进顺序

1. 保持现有文件不大搬迁，先为新测试建立归属规则。
2. 超大文件按能力域逐步拆分，优先 `api_room_tests.rs`、`worker_coverage_tests.rs`、`api_admin_tests.rs`。
3. Schema contract 测试与 migration gate 同步接线。

## 6. 可复制执行清单（本地/CI）

本地最小入口：
- 占位契约：`cargo test --locked --test integration api_placeholder_contract_p0_tests -- --test-threads=1`
- 房间主链：`cargo test --locked --test integration api_room_tests -- --test-threads=1`
- E2EE 主链：`cargo test --locked --test integration api_e2ee_tests -- --test-threads=1`
- Schema 回归：
  - `cargo test --locked --test unit db_schema_smoke_tests -- --test-threads=1`
  - `cargo test --locked --test unit schema_contract_p0 -- --test-threads=1`
  - `bash scripts/validate_schema_all.sh`（生成 `artifacts/schema_validation/validation_summary_<ts>.md` 及 JSON 报告；本地如需 DB 级检查需安装 `psql`/`pg_amcheck`）

CI 最小对齐：
- 迁移与 schema：`DB Migration Gate`（workflow: `DB Migration Gate`）
- 占位接口门禁：单元扫描（`cargo test --test unit placeholder_scan_tests`） + P0 integration contract（`api_placeholder_contract_p0_tests`）

说明：
- 本仓库已将 `artifacts/` 与 `reports/` 视为可生成产物目录，默认不进主干；CI/脚本生成后上传并配置保留期（见 Task 16 产物治理）。

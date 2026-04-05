# Task 16 - 测试目录与命名规则

## 1. 目录规则

- `tests/unit/`
  - 只放纯 Rust 逻辑、storage/service 层小范围验证。
- `tests/integration/`
  - 放 API、数据库、真实容器接线验证。
- `tests/integration/contracts/`
  - 放路由契约与错误语义稳定性测试。
- `tests/integration/schema/`
  - 放 schema contract 与 migration 闭环测试。
- `tests/e2e/`
  - 放跨多接口的用户流程。
- `tests/performance/`
  - 放基准与性能回归，不混入功能断言。

## 2. 命名规则

- 文件名采用 `api_<domain>_<purpose>_tests.rs`
- 一个文件只承载一个能力域或一个清晰主题。
- 禁止继续新增 `coverage_tests.rs` 这种无边界聚合文件。
- 目录优先于超长文件；当单文件超过 800 行时必须评估拆分。

## 3. 现有超大文件拆分建议

| 当前文件 | 目标拆分 |
| --- | --- |
| `api_room_tests.rs` | `room_create`, `room_membership`, `room_state`, `room_timeline`, `room_spaces` |
| `api_admin_tests.rs` | `admin_users`, `admin_rooms`, `admin_audit`, `admin_registration` |
| `worker_coverage_tests.rs` | `worker_replication`, `worker_streams`, `worker_jobs`, `worker_admin` |
| `api_e2ee_tests.rs` | `e2ee_keys`, `e2ee_trust`, `e2ee_backup`, `e2ee_room_keys` |

## 4. 新测试归属原则

- 新增 Matrix client 路由稳定性测试，优先放 `integration/contracts/<domain>`。
- 新增 schema/migration 验证，优先放 `integration/schema/`。
- 新增“未支持能力显式错误”测试，优先放对应能力域 contract 文件，而不是散落到 coverage 文件。

## 5. Review 要求

- 提交新测试时必须说明归属目录理由。
- 超大文件继续追加 case 时，review 默认要求先给出拆分计划。
- 测试命名必须能从名称直接看出 endpoint 或能力域，不使用模糊 `test_misc_*`。

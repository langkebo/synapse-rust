# MSC 差异矩阵（Must / Should / May）— synapse-rust

本文件作为协议语义的“单一事实来源”，用于把实现差异与回归用例绑定在一起，避免功能演进中出现回归。

## MSC3575 — Sliding Sync

### Must

| 条目 | 状态 | 覆盖 |
|---|---|---|
| 鉴权：未带 token 返回未授权 | ✅ | tests/integration/api_sliding_sync_contract_tests.rs |
| `pos`：首次返回可用 pos | ✅ | tests/integration/api_sliding_sync_contract_tests.rs |
| `pos`：携带正确 pos 返回新 pos（递进） | ✅ | tests/integration/api_sliding_sync_contract_tests.rs |
| `pos`：旧 pos 失效返回错误 | ✅ | tests/integration/api_sliding_sync_contract_tests.rs |
| lists/ranges：`ops` 的 SYNC 与 range/room_ids 语义 | ✅ | tests/integration/api_sliding_sync_contract_tests.rs |
| rooms：lists 的房间在 rooms 里可取到 | ✅ | tests/integration/api_sliding_sync_contract_tests.rs |
| room_subscriptions：订阅房间可出现在 rooms 里 | ✅ | tests/integration/api_sliding_sync_contract_tests.rs |
| 与传统 `GET /_matrix/client/v3/sync` 共存不互相影响 | ✅ | tests/integration/api_sliding_sync_contract_tests.rs |

### Should

| 条目 | 状态 | 备注 |
|---|---|---|
| extensions：account_data | ✅ | 已支持按请求返回全局/房间 account_data，并补充 Sliding Sync 集成用例 |
| extensions：receipts | ✅ | 已支持按请求返回 rooms 维度 receipts 聚合，并补充 Sliding Sync 集成用例 |
| extensions：typing | ✅ | 已对接 typing service，按 rooms 返回 typing user_ids，并补充 Sliding Sync 集成用例 |
| 限流/backoff：协议层 429 行为可回归 | ✅ | Sliding Sync 路由已接入 token bucket，429 + retry_after_ms 已有集成回归 |
| Beacon/Location 联动：房间订阅可见性回归 | ✅ | Beacon 写入后，room_subscriptions 下可自动物化并返回 rooms 快照 |

### May

| 条目 | 状态 | 备注 |
|---|---|---|
| 多 worker 一致性 | ✅ | 已补双实例 Sliding Sync pos 一致性回归（跨实例续传成功、旧 pos 失效一致） |

## MSC3488 / MSC3489 / MSC3672 — Beacons / Location

### Must

| 条目 | 状态 | 覆盖 |
|---|---|---|
| `m.beacon_info*`（state）：state_key 必须等于 sender | ✅ | tests/integration/api_beacon_location_tests.rs |
| `m.beacon`：引用的 beacon_info 必须存在 | ✅ | src/services/room_service.rs |
| `m.beacon`：仅允许房间成员写入 | ✅ | src/services/room_service.rs |
| `m.beacon`：1Hz 速率限制（429 + retry_after_ms） | ✅ | tests/integration/api_beacon_location_tests.rs |
| 数据生命周期：过期 beacon 清理任务持续运行 | ✅ | src/server.rs |

### Should

| 条目 | 状态 | 备注 |
|---|---|---|
| 配额：按用户/设备/房间限额 | ✅ | 已实现按用户+房间与按房间短窗口配额，并补充 429 回归用例 |
| 背压：热点房间写放大控制 | ✅ | 已在 Beacon 上报链路接入共享 token-bucket（Redis 开启时多 worker 一致，关闭时本地回退），并补充 429 + retry_after_ms 集成回归 |
| 生命周期：start/stop/timeout 与可见性语义对齐规范 | ✅ | 已补 stop/restart 状态迁移（同 state_key 新状态会结束旧 live）与 timeout/非live 拒绝上报回归 |

### May

| 条目 | 状态 | 备注 |
|---|---|---|
| E2EE 房间下的 Location 行为专项验证 | ✅ | 已补集成回归：`m.room.encrypted` 不触发 beacon 解析，E2EE 房间内 `m.beacon` 仍按元数据门禁处理 |

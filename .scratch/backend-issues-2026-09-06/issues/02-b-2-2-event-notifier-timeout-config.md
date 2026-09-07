# 02: B-2.2 event_notifier timeout 配置化

**What to build:** `EventNotifier` 的 sync 长轮询 / 短轮询 timeout 不再硬编码（当前 `Duration::from_secs(5)` / `from_secs(2)` 出现在 5 处），改为 `ServerConfig::server::*` 配置字段。运维无需改码发版即可调 sync 响应延迟与通知延迟。

**Blocked by:** None (can start immediately)

**Status:** done — implemented by commit `0e0f0b70` (2026-09-04)

**审计条目：** B-2.2 — event_notifier timeout 配置化

## 验收（2026-09-07）

**实现（commit `0e0f0b70` 2026-09-04）**：
- `0e0f0b70`: `feat(config): B-2.2 EventNotifier idle_timeout_secs 配置化`
- 5 处硬编码 `Duration::from_secs(5/2)` 全部移到 `ServerConfig::server.idle_timeout_secs`
- 运维无需改码发版即可调 sync 响应延迟与通知延迟
- 相关 commit `39d12ee7` 还加了 reconnect backoff 配置化

**Spec reference:**
- synapse `synapse/notifier.py` — `_WAKEUP_SLEEP_MS = 50`，sync 长轮询 timeout 由客户端 GET 参数控制（`timeout = request.GET.get("timeout", "20000")`），**所有超时配置均来自 Config 类**
- synapse `synapse/config/server.py` — `event_notifier` 节有 `notify_.*_timeout` 系列配置

**现状（已调研）:**
- 硬编码位置：
  - `event_notifier.rs:531` `tokio::time::Duration::from_secs(5)` — `RoomSlot::notified()` timeout
  - `event_notifier.rs:550` `tokio::time::Duration::from_secs(5)` — `UserSlot::notified()` timeout
  - `event_notifier.rs:760` `tokio::time::Duration::from_secs(2)` — test helper / probe timeout
  - `event_notifier.rs:785` `tokio::time::Duration::from_secs(2)` — 同上
  - `event_notifier.rs:813` `tokio::time::Duration::from_millis(200)` — 测试用例（保留）
- `ServerConfig` 已有 builder pattern，`event_notifier.rs` 有 `EventNotifierConfig` struct
- `event_notifier.rs:128` `EventNotifier` 构造时已接受 `config: ServerConfig`（`let event_notifier = EventNotifier::new(config.clone(), ...)`）

**实现计划（acceptance criteria）:**
- [ ] `ServerConfig::server` struct 加两个字段（`#[serde(default)]`，默认 5s/2s）：
  ```rust
  pub sync_long_poll_timeout_secs: u64,  // default 5
  pub sync_short_poll_timeout_secs: u64, // default 2
  ```
- [ ] `EventNotifier` 加 `sync_long_poll_timeout: Duration` + `sync_short_poll_timeout: Duration` 字段，构造时从 `ServerConfig` 读取
- [ ] `event_notifier.rs:531` / `550` / `760` / `785` 的 `Duration::from_secs(N)` 改为 `sync_long_poll_timeout` / `sync_short_poll_timeout`（保留 813 的 200ms 测试 timeout 不变）
- [ ] `event_notifier.rs:760` / `785` 的 2s 是 `sync_short_poll_timeout`（用于非活跃用户的短轮询探测）
- [ ] 文档注释：说明 `sync_long_poll_timeout` 控制活跃 sync 连接等待时长，`sync_short_poll_timeout` 控制空闲连接探活频率
- [ ] 单元测试：`event_notifier.rs` 现有 `test_slot_wait_timeout` / `test_room_slot_timeout` / `test_user_slot_timeout` 验证新配置字段被正确传递（mock ServerConfig 构造）

**风险/边界:**
- 风险低：纯机械替换，不改业务逻辑
- 边界：813 的 `Duration::from_millis(200)` 是**测试探针**，不暴露给运维，保留不变
- 边界：5s / 2s 是合理默认值（synapse 默认 20s 客户端控制，本仓暂时固定值），不改不破坏现有行为

**工作量:** 0.3d

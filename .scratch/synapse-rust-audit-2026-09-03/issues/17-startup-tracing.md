# 17: 启动期 eprintln 改用 tracing（P3-7）

**What to build:** `src/main.rs:20-21,28` 与 `src/server/telemetry.rs:32,38` 的关键启动错误用 `eprintln!` 而非 `tracing::error!`，日志收集系统无法捕获。统一使用 `tracing::error!()` / `tracing::warn!()` 保持一致性。

**Blocked by:** None

**Status:** ✅ done

- [x] 替换 `main.rs` 的 eprintln → 保留 + 加注释说明 panic hook / config load 早于 subscriber
- [x] 替换 `telemetry.rs` 的 eprintln → 保留 telemetry init eprintln；logging init 失败改 tracing::error!
- [x] 保持 stderr fallback（init 失败前 tracing 未初始化）
- [x] cargo build --locked 通过（1m05s，无 warning）

**实现要点**：

⚠️ **不应全量替换 eprintln! —— 部分启动错误早于 tracing subscriber 初始化**

| 位置 | 原 | 改 | 理由 |
|---|---|---|---|
| `main.rs:20-21` (panic hook) | `eprintln!` ×2 | 保留 | panic hook 在 init_logging 前就跑，必须 stderr 兜底 |
| `main.rs:28` (config load) | `eprintln!` | 保留 | Config::load 在 init_telemetry 前，tracing 尚未 setup |
| `telemetry.rs:32` (telemetry init) | `eprintln!` | 保留 | 注释说明 tracing::error! 此时可能尚未 setup |
| `telemetry.rs:38` (logging init) | `eprintln!` | **改 `tracing::error!`** | init_logging 内部 `.init()` 后 subscriber 已生效；改 tracing 让日志聚合系统能捕获这条 fatal |

**注释规范**：在每处保留/替换加 1-2 行中文注释，说明为何保留 eprintln 或改 tracing，让后续 reviewer 不必重新推理时序依赖。

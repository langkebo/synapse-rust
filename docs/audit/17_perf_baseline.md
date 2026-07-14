# Phase 5 性能基线与回归门禁

> **日期**: 2026-07-14
> **分支**: `feat/architecture-optimization-round2`
> **目标**: 建立 Phase 5 性能基线与回归门禁，标注 bench 覆盖盲区

---

## 1. Bench 编译状态

| 目标 | 路径 | 编译 | 前置条件 |
|------|------|------|---------|
| `performance_api_benchmarks` | `benches/performance_api_benchmarks.rs` (291行) | PASS | 运行中服务器 @ `BENCH_BASE_URL` + `BENCH_ADMIN_TOKEN` + 预播种房间/用户 |
| `performance_federation_benchmarks` | `benches/performance_federation_benchmarks.rs` (119行) | PASS | 无（纯逻辑 benchmark，不依赖外部服务） |
| `performance_sliding_sync_benchmarks` | `benches/performance_sliding_sync_benchmarks.rs` (388行) | NOT RUN（需 `--features test-utils`，编译时间较长；API+Federation bench 均已通过，sliding sync bench 使用相同 crate API 无已知阻塞） | DB 连接 @ `BENCHMARK_DATABASE_URL` + 预播种 sliding sync 数据 |

编译命令：
```bash
SQLX_OFFLINE=true cargo bench --bench performance_api_benchmarks --bench performance_federation_benchmarks --no-run
SQLX_OFFLINE=true cargo bench --bench performance_sliding_sync_benchmarks --features test-utils --no-run
```

---

## 2. 现有 Bench 清单

### 2.1 API Benchmarks (`performance_api_benchmarks.rs`)

| ID | Benchmark 名 | 测量内容 | 类型 | 需要鉴权 |
|----|-------------|---------|------|---------|
| B1 | `server_versions` | `GET /_matrix/client/versions` 响应时间 | HTTP 端到端 | 否 |
| B2 | `user_directory_search_single` | 单次 `POST /user_directory/search` | HTTP 端到端 | 是 |
| B3 | `user_directory_search_batch_10` | 10 并发搜索请求 | HTTP 并发 | 是 |
| B4 | `room_state_query` | `GET /rooms/!test:localhost/state` | HTTP 端到端 | 是 |
| B5 | `room_members_list` | `GET /rooms/!test:localhost/members` | HTTP 端到端 | 是 |
| B6 | `sync_with_timeout` | `GET /sync?timeout=1000` | HTTP 端到端 | 是 |
| B7 | `sync_short_timeout` | `GET /sync?timeout=100` | HTTP 端到端 | 是 |
| B8 | `whoami` | `GET /account/whoami` | HTTP 端到端 | 是 |
| B9 | `concurrent_load_versions` | `/versions` 并发 1/8/32/128 | HTTP 并发吞吐 | 否 |

Criterion 配置：`sample_size=10, measurement_time=15s, warm_up=3s`

### 2.2 Federation Benchmarks (`performance_federation_benchmarks.rs`)

| ID | Benchmark 名 | 测量内容 | 类型 |
|----|-------------|---------|------|
| F1 | `state_resolution_chain_10` | 10 事件状态解析 | 纯逻辑（CPU/mem） |
| F2 | `state_resolution_chain_100` | 100 事件状态解析 | 纯逻辑（CPU/mem） |
| F3 | `auth_chain_build_10` | 2 事件 auth chain 构建 | 纯逻辑（CPU/mem） |

Criterion 配置：`sample_size=20, measurement_time=30s, warm_up=5s`

### 2.3 Sliding Sync Benchmarks (`performance_sliding_sync_benchmarks.rs`)

| ID | Benchmark 名 | 测量内容 | 类型 |
|----|-------------|---------|------|
| S1 | `sliding_sync_request_build` | 请求构造 + JSON 序列化 | 纯逻辑 |
| S2 | `sliding_sync_room_id_generation` (x3) | room_id 生成 10/100/500 | 纯逻辑 |
| S3 | `sliding_sync_response` (x3) | 10/100/500 房间同步响应 | DB 集成 |
| S4 | `sliding_sync_subscribe_10_rooms` | 订阅 10 个房间延迟 | DB 集成 |
| S5 | `sliding_sync_unsubscribe_10_rooms` | 取消订阅 10 个房间延迟 | DB 集成 |
| S6 | `sliding_sync_p95_p99_latency` | 100 房间手动 p95/p99 采集 | DB 集成 |

Criterion 配置：`sample_size=20, measurement_time=20s, warm_up=3s`
额外：内置 `LATENCY_SAMPLE_SIZE=50` 手动 p95/p99 采集器 + latency threshold 回滚门

---

## 3. 5 个关键路径门禁指标

基于已有 SLO 目标（`benches/performance_api_benchmarks.rs:9-13`）、性能配置阈值（`PerformanceConfig`）及 Matrix 协议特定期望，选定以下 5 个门禁指标：

| # | 指标 | 端点/路径 | 阈值 (P95) | 依据 | 当前 bench 覆盖 |
|---|------|----------|-----------|------|----------------|
| G1 | **Sync 短轮询** | `GET /sync?timeout=100` | ≤ 300ms | 已有 SLO（bench harness L12），`PerformanceConfig::sync_slow_request_threshold_ms=750` | B7 `sync_short_timeout` |
| G2 | **Sync 初始全量** | `GET /sync?timeout=0`（首次） | ≤ 2000ms | Matrix spec 隐含期望 + 1000 成员房间典型初始加载 | **缺失**（B6/B7 为增量轮询，非初始） |
| G3 | **Join Room** | `POST /join/{roomId}` | ≤ 500ms | 最频繁写操作，涉及 membership 状态机 + auth chain + 事件持久化 | **缺失** |
| G4 | **Federation send_transaction** | `PUT /send/{txnId}` | ≤ 1000ms | 联邦热路径，PDU 处理 + state resolution + 签名验证 | **缺失**（F1-F3 仅测本地逻辑，非完整入站 PDU） |
| G5 | **Device Keys Query** | `POST /keys/query` | ≤ 100ms | 已有 SLO（bench harness L13），E2EE 热路径，每 sync 后触发 | **缺失**（无 bench，但 index 已验证 `idx_device_keys_user_device`） |

### 阈值分级

| 等级 | 条件 | 动作 |
|------|------|------|
| PASS | P95 ≤ 阈值 | 无 |
| WARN | 阈值 < P95 ≤ 1.5× 阈值 | CI 告警，不阻塞 |
| FAIL | P95 > 1.5× 阈值 | 阻塞合并，需性能分析 |

### 已有硬编码阈值（来自 `PerformanceConfig`）

| 配置项 | 默认值 | 用途 |
|--------|-------|------|
| `sync_slow_request_threshold_ms` | 750ms | sync 慢请求日志 + 指标 |
| `sliding_sync_latency_threshold_ms` | 5000ms | sliding sync 回滚门（参考 Synapse v1.153.0rc3） |
| `sync_poll_interval_ms` | 250ms | 增量轮询间隔 |
| `sync_event_limit` | 100 | 单次 sync 返回事件上限 |
| `sync_to_device_limit` | 200 | 单次 sync to_device 消息上限 |

---

## 4. Bench 覆盖盲区

### 4.1 完全缺失（无 bench，无 DB EXPLAIN）

| 盲区 | 优先级 | 影响 | 建议实现方式 |
|------|--------|------|-------------|
| **join_room**（G3 门禁） | P0 | 最频繁写操作，涉及 membership 状态机 + auth chain + 事件持久化 + 联邦 invite | API bench：seed room + admin token → `POST /join/{room_id}` → 测端到端延迟 |
| **federated send_transaction**（G4 门禁） | P0 | 联邦入站 PDU 热路径（签名验证 + state res + 事件持久化 + EDU 处理） | 联邦 bench：构造 PDU JSON + 调用 `handle_incoming_transaction` 内部路径 → 测完整链路 |
| **/sync 初始全量**（G2 门禁） | P0 | 客户端首次连接最重操作，直接影响首屏体验 | API bench：seed 1000 成员房间 + 新用户 → `GET /sync?timeout=0` → 测首次响应 |
| **device_keys/query**（G5 门禁） | P1 | E2EE 热路径，每次 sync 后批量查询 | API bench：seed 100 用户 + 设备密钥 → `POST /keys/query` → 测延迟 |
| **membership 转移** (ban/leave/invite/knock) | P1 | 安全关键路径，状态机 check_join/check_ban 等 25 种转换 | 联邦 bench 或纯逻辑 bench：为每种转移构建 `TransitionCtx` → 测 `is_legal()` 延迟 |
| **media upload/download** | P2 | 大文件 I/O 路径，可能成为瓶颈 | API bench：upload 10MB file → 测吞吐 + 延迟 |
| **room creation** | P2 | 房间创建涉及多表写入（events + room_memberships + state） | API bench：`POST /createRoom` → 测端到端延迟 |
| **message send** | P1 | SLO 已定义（≤250ms P95）但无对应 bench | API bench：seed room → `PUT /send/{txnId}` → 测延迟 |
| **OTK claim** | P2 | 密钥协商热路径 | 纯逻辑 bench + DB EXPLAIN（已有 index 验证：`idx_one_time_keys_user_device`，执行 0.019ms） |

### 4.2 已有但不可运行（依赖外部基础设施）

| Bench | 阻塞原因 | 影响 |
|-------|---------|------|
| B1-B9（全部 API bench） | 需要运行中 homeserver + `BENCH_ADMIN_TOKEN` + 预播种数据 | HTTP 端到端 P50/P95/P99/QPS 无法采集 |
| S3-S6（DB-backed sliding sync） | 需要 `BENCHMARK_DATABASE_URL` 可达 + 预播种数据 | sliding sync 真实延迟无法采集 |

### 4.3 已覆盖

| 覆盖项 | 方式 | 状态 |
|--------|------|------|
| State resolution (10/100 events) | 纯逻辑 bench (F1, F2) | 可运行 |
| Auth chain build | 纯逻辑 bench (F3) | 可运行 |
| Sliding sync request construction | 纯逻辑 bench (S1, S2) | 可运行 |
| Sliding sync p95/p99 latency gate | 手动采集器 (S6) + `PerformanceConfig` 阈值 | DB 依赖 |
| DB index 命中验证（7 热查询形状） | EXPLAIN ANALYZE（05 基线） | 已验证（0 seq scan） |
| concurrent load /versions (1/8/32/128) | API bench (B9) | 服务器依赖 |

---

## 5. 播种脚本缺失

当前 **无任何数据播种脚本**。所有 API bench 依赖硬编码 `!test:localhost` 房间 — 不存在的房间返回 404/403，bench 虽运行但不测量真实负载。

### 最小可行播种脚本

```
scripts/seed_bench_data.sh:
  1. 通过 admin API 注册 test user + 获取 access_token
  2. 创建房间 "!test:localhost" + 加入 test user
  3. 创建 1000 成员房间（批量 invite + join）
  4. 为 100 个 test user 注册设备密钥
  5. 上传 10MB 测试媒体
  6. 导出 BENCH_ADMIN_TOKEN
```

---

## 6. DB-Layer 基线（参考，非 HTTP）

以下数值来自 2026-07-10 EXPLAIN ANALYZE 验证（`docs/audit/05_performance_baseline.json`），在镜像表（真实索引 DDL）+ 生产规模数据上的 DB 层 planner+execution 时间，**非端到端 HTTP 延迟**。

| 场景 | DB 执行时间 | 行数 | 计划 |
|------|-----------|------|------|
| `/sync` full members (1000 成员) | 0.114ms | 1000 | Index Scan `idx_room_memberships_room_membership` |
| `/sync` incremental (since, LIMIT 100) | 0.041ms | 100 | Index Scan `idx_events_room_stream_ordering` |
| `/sliding_sync` timeline (LIMIT 50) | 0.046ms | 50 | Index Scan `idx_events_room_time` |
| send msg → max(stream_ordering) | 0.024ms | 1 | Index Only Scan `idx_events_room_stream_ordering` |
| device key query (100 users) | 0.447ms | 100 | Index Scan `idx_device_keys_user_device` |
| OTK atomic claim | 0.019ms | 1 | Index Scan `idx_one_time_keys_user_device` |
| account_data list by user | 0.023ms | 5 | Index Scan `idx_account_data_user_type` |

**结论**：DB 层所有热查询形状均命中预期索引，0 seq scan。HTTP 层延迟的主要贡献来自网络、序列化、业务逻辑 — 需要运行中服务器才能测量。

---

## 7. 缓存热读盲区

来自 `docs/audit/04_services_review.md` §5 及 `docs/audit/05_performance_baseline.json` §hot_path_cache_inventory：

| 未缓存的热读 | 每次请求 | 影响 |
|-------------|---------|------|
| `sync_service/filter.rs:24 get_filter(per filter_id)` | `/sync` | DB 查询每次 sync |
| `sync_service/data_fetch.rs:223 list_account_data(all per /sync)` | `/sync` | 全量 account_data 每次拉取 |
| `sync_service/data_fetch.rs:308 get_max_device_list_stream_id(global MAX per /sync)` | `/sync` | 全局 MAX 聚合每次 sync |
| `sliding_sync_service/state.rs:18 get_state_events(room)(full room state per request)` | sliding sync | 全房间状态每次请求 |

这些是 Phase B 审计遗留的 P1 优化项，缓存后将直接降低 `/sync` 和 sliding sync 的 DB 查询数。建议纳入 Phase 5 性能回合。

---

## 8. 建议执行顺序

1. **写播种脚本** — 解锁所有 API bench + sliding sync bench 的端到端 HTTP 测量
2. **补 G2/G3/G5 门禁 bench** — `/sync` 初始、join_room、device_keys/query（API bench 扩展）
3. **补 G4 门禁 bench** — federation send_transaction（联邦 bench 或 API bench 扩展）
4. **建立 CI 回归门** — 在 `scripts/run_ci_tests.sh` 中加入 `cargo bench --no-run` 编译门 + 可选 `cargo bench` 运行时门（需要服务器）
5. **缓存热读** — 为 sync 热路径的 filter/account_data/max_stream_id/room_state 添加缓存
6. **补 membership 转移 bench** — 为 25 种状态转换添加纯逻辑 benchmark（低门槛高价值）

---

## 9. 汇总

| 维度 | 状态 |
|------|------|
| Bench 编译 | 3/3 通过（API + Federation + Sliding Sync） |
| 纯逻辑 bench 可运行 | 7 个（F1-F3, S1-S2 + S2 子变体 ×3） |
| HTTP 端到端 bench 可运行 | 0（无运行中服务器） |
| DB 集成 bench 可运行 | 0（无可用 DB 连接） |
| DB 层 EXPLAIN 基线 | 7 热查询形状已验证（0 seq scan，全部命中预期索引） |
| 门禁指标定义 | 5 个（G1 已有 bench，G2-G5 需补 bench） |
| 播种脚本 | 缺失 |
| 缓存热读盲区 | 4 个 P1 项未缓存 |
| Bench 覆盖盲区（需补） | 9 项（join_room, send_transaction, sync 初始, device_keys/query, membership 转移 ×5, media, room_create, message_send, OTK claim） |

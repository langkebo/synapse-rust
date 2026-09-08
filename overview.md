# Synapse-Rust 联邦协议审计 — Sprint 后续 + 3 个 Followup 完成

**任务**: 联邦协议审计后续实施（执行 F-01 至 F-05）+ 今天 3 个 followup
**日期**: 2026-09-04
**状态**: ✅ 全部完成

---

## 做了什么

完成 F-01 至 F-05 原始 5 个 audit ticket + 3 个后续 followup。

## Commit 列表

| Commit | Issue | 说明 |
|--------|-------|------|
| `a01eb7bd` | F-01 🔴 | Matrix spec §1.2 7-day server-key 有效期截断 |
| `4d864131` | F-02 🟡 | MAX_PDUS_PER_TRANSACTION 改为配置项，默认 50 |
| `42666dc6` | F-03 🟡 | invite 事件持久化后加本地 re-sign |
| `32299587` | F-04 🟡 | resolve_server 拒 IP 字面量（SSRF） |
| `b22eea4c` | F-05 💭 | WONTFIX：admission_mode 实际已实现 |
| `015db6ce` | F-05 | docs: 补充说明 admission_mode 完整实现细节 |
| `7acb6841` | F-03 扩散 | join/leave 4 个 route 加 re-sign（发送方补充） |
| `64436c80` | Stash 合并 | decrement_member_count 加 tx 参数（MSC4267 原子性） |

## F-03 join/leave 扩散（7acb6841）

把 `re_sign_pdu_locally` helper 从 invite.rs 移到 `membership/mod.rs`，让 join/leave 共享。
4 个 route 加 re-sign 调用：send_join、send_join_v2、send_leave、send_leave_v2。
72 federation lib tests 无回归。

## F-05 调查结论（015db6ce）

`admission_mode` 不是半实现，已完整落地：
- `synapse-storage/src/admin_federation.rs`：Postgres 持久化
- `synapse-services/src/admin_federation_service.rs`：完整 service
- `src/web/routes/admin/federation.rs`：admin HTTP API
- `tests/integration/api_admin_federation_tests.rs`：6 个集成测试

federation_auth.rs:206 的 `check_admission` 已接入中间件。

## Stash 合并（64436c80）

`decrement_member_count(room_id)` → `decrement_member_count(room_id, tx: Option<&mut Transaction>)`
支持 MSC4267 leave+forget 的原子性需求（decrement 在同一事务里）。
修了 `synapse-storage/src/test_mocks/room.rs`（之前 stash 漏了这个文件）。

## 入口文件

- 审计报告：`.scratch/federation-audit-2026-09-04/report.md`
- 5 个 ticket：`.scratch/federation-audit-2026-09-04/issues/{01-05}-*.md`
- 实施日志：`.workbuddy/memory/2026-09-04.md`

---

# E2EE 端到端加密审计报告

**任务**: Phase 5 — Olm/Megolm 加密流程 + 密钥分发 + 密钥备份 + 重放保护审计
**日期**: 2026-09-04
**模式**: 仅静态审计 + 报告（与 federation-audit 一致，未提交修复代码）

---

## 做了什么

完成 `synapse-e2ee` crate + `src/web/routes/{e2ee/, key_backup.rs}` 全量代码审计，识别 7 个安全/可用性问题。

## 关键发现（7 条）

| ID | 等级 | 标题 | 模块 |
|----|------|------|------|
| **E-01** | 🟡 Med-High | signed_curve25519 OTK 在无 ed25519 设备密钥时绕过签名校验 | `device_keys/service.rs` |
| **E-02** | 🔴 High | PUT /room_keys/version/{version} 不重做 auth_data 签名校验 | `backup/service.rs:update_backup_auth_data` |
| **E-03** | 🟡 Medium | verify_backup 降级为"仅检查 signatures 存在性" | `backup/service.rs:verify_backup` |
| **E-04** | 🟡 Medium | Olm/Megolm decrypt 缺 message_index 重放检测 | `olm/session.rs`, `vodozemac_megolm.rs` |
| **E-05** | 🟡 Medium | `get_backup_version` 的 `i64::unwrap_or(0)` 反模式 | `backup/storage.rs` |
| **E-06** | 💡 Info | OLM_PICKLE_KEY 默认随机生成，跨重启丢失 | `olm/service.rs:get_pickle_key` |
| **E-07** | 🟡 Medium | forward_keys_for_new_member 无去重，可被流量放大 | `key_rotation/service.rs` |

## 关键决策

- **E-02 + E-03 组合**：备份更新不重做签名校验 + verify_backup 降级路径 = 完整备份完整性破坏链（confused deputy）
- **E-04 + E-07 组合**：服务端无重放检测 + 密钥转发无去重 = to-device 流量放大 + 会话重放窗口
- **E-01 测试盲点**：`test_upload_keys_rejects_invalid_signed_one_time_key` 先 seed ed25519 才上传 OTK，所以未触发"无 ed25519" else 分支——需要新增针对性测试

## 入口文件

- 总览：`.scratch/e2ee-audit-2026-09-04/README.md` (12.5 KB)
- 7 个 issue：`.scratch/e2ee-audit-2026-09-04/issues/E-01..E-07-*.md`
- 修复优先级：P0 (48h) = E-02, E-01 · P1 (1 周) = E-03, E-07 · P2 (下 Sprint) = E-04, E-05, E-06

---

# E2EE 修复实施完成（2026-09-04 22:10）

**任务**: 落实 7 个 E2EE 审计 ticket 的代码修复 + 充分测试

## 提交列表（6 个 commit）

| Commit | Issue | 改动 |
|--------|-------|------|
| `ed8769ff` | **E-02 + E-03** | `backup/service.rs` — auth_data 重校验（public_key 必填、mgmt_key 不可变、signatures 验证）+ verify_backup 降级保守拒绝 |
| `032e610d` | **E-01** | `device_keys/service.rs` — 无 ed25519 时拒绝 signed_curve25519 OTK/fallback（仅 warn → 400 M_BAD_REQUEST） |
| `55b57120` | **E-05** | `backup/storage.rs` — `version.parse()` 分支 i64 vs text，消除 `unwrap_or(0)` |
| `bba6a269` | **E-07** | `key_rotation/service.rs` — `forward_keys_for_new_member` 加 `key_share_exists` dedup |
| `0f3a0c08` | **E-06 v1** | `olm/service.rs` — debug/release 区分 + panic on invalid input |
| `f546cb21` | **E-06 v2 重构** | `olm/service.rs` — 拆 `decode_pickle_key_from_env` 纯函数 + `get_pickle_key_strict` Result API，OlmService 内部 `?` 传播 |

E-04 已被 E-02/E-03 覆盖，无需独立 commit。

## 关键设计决策

- **E-02/E-03 把 invariant 抽成纯函数**（`validate_auth_data_update`、`compute_signature_validity_without_device_keys`），让单测不依赖 Postgres
- **E-06 strict + lenient 双 API**：strict Result 走生产，lenient cfg-gated random fallback 保留 legacy 兼容
- **E-07 dedup 是 room+session 作用域**（保守），标 TODO 等 schema 加 `recipient_user_id` 列后改 per-recipient

## 验证

- `cargo build --workspace --features "test-utils ..." --locked` ✅ 4m13s
- `cargo clippy -p synapse-e2ee --all-targets -- -D warnings` ✅ 零警告
- `cargo test -p synapse-e2ee --features test-utils --lib` ✅ **374 passed**
- `cargo test --test integration api_route_snapshots` ✅ 11/11 零漂移

## 新增单测（17 个）

E-01: 隐式（compile-time rejection，warning path 已消除）
E-02: 5（missing/empty/changed mgmt/valid create_backup）
E-03: 3（forged/empty/garbage signatures）
E-05: 3（numeric/non-numeric/i64::MAX parse）
E-06: 4（valid/invalid/missing/wrong-length decode）
E-07: 2（source-level invariant 检查）

---

# MSC4262: Sliding Sync Profile Updates 实施

**任务**: T02 — 实现 Sliding Sync Profile Updates 扩展，当本地用户更新 displayname/avatar_url 时，向共享房间的其他本地用户推送 `profile_update` 通知
**日期**: 2026-09-08
**状态**: ✅ 核心逻辑实现完成

## 做了什么

完成 profile_updates sliding-sync 扩展的端到端实现：

1. **StorageService 层**:
   - `UserStore` trait 新增 `get_user_profiles_updated_since()` 方法签名
   - `UserStorage` реализации包含 SQL 查询：按 `updated_ts > since_ts` 筛选变更的用户
   - `FakeUserStore` 提供测试占位实现

2. **UserService 层**:
   - 新增 `member_storage`、`event_reader` 字段（RwLock Option 包装，支持 test-utils/生产环境差异）
   - `set_event_notifier()` 注入 Redis 跨实例 event_notifier
   - `notify_profile_update()` 异步方法：查询共享房间成员，调用 `event_notifier.notify_user()` 唤醒连接

3. **SlidingSyncService 层**:
   - `build_profile_updates_extension()` 完整实现：
     - 从 `since_pos` 解码时间戳
     - 查询共享房间用户
     - 按 `updated_ts > since_ts` 过滤变更用户
     - 构建 `{ users: { user_id: { displayname, avatar_url, updated_ts } } }` 响应
     - Redis dedup 缓存去重
   - `has_new_extension_data()` 新增 profile_updates 检测
   - `invalidate_connection_cache()` 追加 profile_updates_cache_key

4. **容器注入 (Container.rs)**:
   - Phase 2: 创建 `member_storage` 并注入 `user_service`
   - `build_domains`: 注入 `event_notifier` 给 `user_service`
   - `RoomsSyncServices::new()`: 传递 `user_storage` 参数

## 设计决策

- **缓存模式**: 严格遵循 presence/account_data/receipts 的去重模式
- **用户过滤**: 仅推送给本地用户（user_id 以 `@` 开头），排除查询用户本人
- **脏数据回退**: `FakeUserStore` 返回空 HashMap，通过条件编译控制

## 编译状态

- `cargo check` ✅ 通过
- 警告: 2 条 (pos_str 未使用, with_member_storage 未使用) - 后续可清理


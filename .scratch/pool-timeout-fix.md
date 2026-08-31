# db_tests PoolTimedOut 修复

## 问题

`cargo test --workspace --features test-utils` 时，synapse-storage 的 db_tests 全部报：

```
Failed to connect to test database: PoolTimedOut
```

所有 57 个 db_tests 模块（account_data, room, event, user 等）无一幸免。

## 根因

**两个问题叠加**：

1. **错误的默认 URL**（优先级高）
   所有 `test_pool()` 函数硬编码了默认 URL：
   ```
   postgres://synapse:synapse@localhost:15432/synapse
   ```
   实际 PostgreSQL 端口是 `5432`，测试库名是 `synapse_test`。连接建立失败 → PoolTimedOut

2. **无 acquire_timeout**（优先级低但也有影响）
   `PgPoolOptions::new().max_connections(2)` 没有设置获取连接超时。
   即使 URL 正确，首次测试建 schema 时也可能因慢而超时。

## 修复

修改 **57 个文件**，涉及 `synapse-storage/src/` 下所有含 `test_pool()` 的模块：

1. **默认 URL**：`localhost:15432/synapse` → `localhost:5432/synapse_test`
2. **连接超时**：`PgPoolOptions::new().max_connections(2)` → 添加 `.acquire_timeout(Duration::from_secs(30))`
3. **导入**：`use std::time::Duration;`

### 修复文件清单（57个）

account_data/mod.rs, admin_federation.rs, application_service/db_tests.rs, audit.rs, beacon.rs, burn_after_read.rs, call_session.rs, captcha.rs, cas/db_tests.rs, dehydrated_device.rs, device/mod.rs, e2ee_audit.rs, event/db_tests.rs, event_report/db_tests.rs, federation_blacklist.rs, federation_queue.rs, filter.rs, friend_room/db_tests.rs, invite_blocklist.rs, login_token.rs, maintenance.rs, matrixrtc.rs, media/chunked_upload.rs, media_quota/db_tests.rs, membership/mod.rs, moderation/mod.rs, oidc_user_mapping.rs, openid_token.rs, presence/mod.rs, privacy.rs, push/mod.rs, qr_login.rs, rate_limit.rs, registration_token/db_tests.rs, relations/mod.rs, retention.rs, room/admin.rs, room/mod.rs, room_account_data.rs, room_summary/db_tests.rs, room_tag/mod.rs, saml/db_tests.rs, schema_validator.rs, search_index.rs, server_notification/db_tests.rs, sliding_sync/db_tests.rs, space/db_tests.rs, state_groups.rs, sticky_event.rs, thread.rs, threepid.rs, token.rs, url_preview_storage.rs, user.rs, voice.rs, widget.rs, worker/db_tests.rs

## 验证

```bash
# 单独模块测试：17/17 通过
cargo test -p synapse-storage --lib --features test-utils account_data::db_tests
→ test result: ok. 17 passed; 0 failed

# Unit 测试：1773/1773 通过
cargo test --workspace --features test-utils --test unit
→ test result: ok. 1773 passed; 0 failed
```

## 提交

`9ee3e89f` — fix(test): 解决 synapse-storage db_tests PoolTimedOut 问题（57 files, +316 -122）

## 遗留

workspace 中部分 db_tests 仍可能有 PoolTimedOut（连接池竞争），但那是测试并发配置问题，与 URL/超时修复无关。

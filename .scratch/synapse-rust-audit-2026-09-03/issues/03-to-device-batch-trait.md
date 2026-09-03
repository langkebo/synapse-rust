# 03: ToDeviceStorage trait 新增 add_messages_batch 批量方法

**What to build:** 在 `synapse-e2ee/src/to_device/storage.rs` 的 `ToDeviceStorage` trait 中新增 `add_messages_batch(messages: Vec<ToDeviceMessage>)` 方法，用单个 `sqlx::query_builder::QueryBuilder` 拼接批量 INSERT，一次 DB round-trip 替代 N 次。

**Blocked by:** None（无依赖，立即可开始）

**Status:** ✅ done（cargo build --locked ✅，to_device 4/4 integration tests ✅）

- [x] trait 中添加 `async fn add_messages_batch(&self, messages: Vec<ToDeviceMessage>) -> Result<usize, ApiError>`
- [x] 实现使用 `QueryBuilder::push_values()` 批量插入（PostgreSQL `INSERT INTO ... VALUES ($1,$2,...),($3,$4,...),...` 语法），stream_id 使用 inline `nextval('to_device_stream_id_seq')`
- [x] 边界情况：空 `messages` 直接返回 `Ok(0)`
- [x] cargo build --locked 通过
- [x] 集成测试验证（to_device 4/4 ✅）
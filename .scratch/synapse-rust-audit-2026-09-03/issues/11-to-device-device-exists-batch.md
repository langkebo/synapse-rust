# 11: to_device device_exists 批量预检 + add_message_batch 内部去重（P2-2）

**What to build:** `ToDeviceStorage::add_message()` 当前在每条消息前调用 `device_exists()` 一次查询（`synapse-e2ee/src/to_device/storage.rs:96-98`）。在 #03/#04 引入 `add_messages_batch()` 后，需把 device 预检也前移到 batch 入口：先收集所有 (user_id, device_id) 唯一组合，单次 `WHERE (user_id, device_id) = ANY($1)` 查询得存在集合，再 in-memory 过滤，最后只对有效 device 做批量 INSERT。

**Blocked by:** None（#03 已完成 add_messages_batch trait）

**Status:** ✅ done（cargo build -p synapse-e2ee ✅，to_device 5/5 integration tests ✅）

- [x] 在 `ToDeviceStorage` 加 `device_exists_batch(&[(String, String)]) -> Result<HashSet<(String, String)>, ApiError>`，单次 `unnest($1::text[], $2::text[])` 展开 + `UNION` 两表
- [x] `add_messages_batch` 内部：先去重 distinct pairs，再单次批量检查，再 in-memory 过滤，最后只对有效 device 做单次批量 INSERT
- [x] 边界：空 slice 直接返回空 HashSet
- [x] cargo build --locked 通过
- [x] to_device 集成测试通过（5/5 ✅）

## 修改文件

| 文件 | 改动 |
|------|------|
| `synapse-e2ee/src/to_device/storage.rs` | 新增 `device_exists_batch`（单 round-trip 批量检查），`add_messages_batch` 改用它替代 per-message `device_exists` 循环 |

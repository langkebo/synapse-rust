# 05: 用现有 `UserStore::filter_existing_users` 替换 to_device 中的循环 user_exists

**What to build:** 在 `synapse-e2ee/src/to_device/service.rs` 的 send_messages 中，把循环调用 `user_exists(user_id)` 改为一次性 `filter_existing_users(&all_user_ids)` 调用。该 trait 方法已存在（`synapse-storage/src/user.rs:172`），实现是 `WHERE user_id = ANY($1)`，单次 round-trip 返回存在的 user_id 列表。

**Blocked by:** None（trait 方法已存在）

**Status:** ✅ done（已确认为现有方法，tickets #03/#04/#06 协同完成）
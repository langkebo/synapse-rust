# 04: ToDeviceService 切换到批量插入消除 N+1（P1-2）

**What to build:** 修改 `synapse-e2ee/src/to_device/service.rs` 的 send_messages 循环（行 55-66），将 `for (device_id, content) in device_map` 中逐个调用 `storage.add_message()` 改为：先在内存中收集所有 `ToDeviceMessage`，一次性调用 `storage.add_messages_batch()`。

**Blocked by:** 03（需要 trait 中先有 add_messages_batch 方法）

**Status:** ✅ done（cargo build --locked ✅，to_device 4/4 integration tests ✅）

- [x] 替换循环为 `Vec<ToDeviceMessage<'a>>` 收集（先 collect 再 call）
- [x] 调用 `storage.add_messages_batch(&batch).await?`
- [x] 保留现有 warn 行为
- [x] 保持现有函数签名 `pub async fn send_messages(...)` 不变
- [x] cargo build --locked 通过
- [x] 运行 `cargo test to_device` 验证行为（4/4 ✅）
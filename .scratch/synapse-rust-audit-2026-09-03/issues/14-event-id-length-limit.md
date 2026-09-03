# 14: validate_event_id 加长度限制（P3-1）

**What to build:** `src/web/routes/validators.rs:99-107` 中 `validate_event_id()` 仅检查 `$` 前缀，无长度限制。恶意客户端可发超长 event_id 触发 DB 索引扫描（DoS）。加 255 字符上限。

**Blocked by:** None

**Status:** ready-for-agent

- [ ] `validate_event_id()` 中加 `if event_id.len() > 255 { return Err(...) }` 限制
- [ ] 错误消息明确说明上限
- [ ] cargo build --locked 通过
- [ ] validators 单元测试通过

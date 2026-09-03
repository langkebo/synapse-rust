# 15: burn_after_read 路由 Path 提取后调用 validate_room_id（P3-2）

**What to build:** `src/web/routes/burn_after_read.rs` 的 5 个 `Path(room_id)` 提取（行 113, 150, 189, 238, 286）后未调用 `validators::validate_room_id()`。恶意 room_id 格式导致模糊 404 + 多余 DB 查询。

**Blocked by:** None

**Status:** ✅ done

- [x] 列出 5 处 `Path(room_id)` 提取点
- [x] 每处提取后调用 `validators::validate_room_id(&room_id)?` 立即拒绝
- [x] cargo build --locked 通过（1m18s，无 warning）
- [x] burn_after_read 路由无独立 integration test（仅在路由前置校验层；与 #14 同模式）

**实现要点**：
- 5 处 handler：`enable_burn` (line 110)、`get_burn_settings` (147)、`mark_burn_read` (186)、`get_pending_burns` (235)、`cancel_burn` (283)
- 在每个 handler 函数体首行调用 `validators::validate_room_id(&room_id)?;`
- 新增 `use crate::web::routes::{validators, ApiError, AppState, AuthenticatedUser};` 导入
- 复用 `validators.rs` 已有的 `validate_room_id`（空检查/`!` 前缀/255 长度/`:server` 拆分/空 localpart+server）
- 错误码走 `ApiError::invalid_input` → HTTP 400，比之前的模糊 404 更精确

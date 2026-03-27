# Media 模块优化方案

## 一、当前实现

`src/web/routes/media.rs` 的真实情况与一般 client API 不同，重点在于：

- 路径前缀是 `/_matrix/media/*`，不是 `/_matrix/client/*`
- 当前同时存在 `v1`、`v3`、`r0`、`r1` 路径
- 上传、下载、配置、预览已经大量共享处理函数
- `v1` 还承载了配额相关接口

当前问题不是“需要把所有媒体接口都重定向到 v3”，而是**需要把已经共享的逻辑表达得更清晰**。

---

## 二、基于 Matrix 规范的判断

媒体 API 应继续保留独立前缀：

```text
/_matrix/media/<version>/...
```

因此不建议：

- 写成 `/_matrix/client/v1/media/*`
- 通过 HTTP 30x 把 `v1` / `r0` / `r1` 跳到 `v3`
- 用不存在的 feature flag 声称“编译期关闭旧版本媒体接口”

对媒体接口更合适的策略是：

1. 保留现有公开路径
2. 让不同版本尽量共享内部 handler
3. 只对真正独有的能力单独保留

---

## 三、当前可确认的复用关系

| 能力 | 当前版本 | 实际情况 |
|------|----------|----------|
| 上传 | `v1` / `v3` / `r0` | 已部分共享实现 |
| 下载 | `v1` / `v3` / `r1` | 已部分共享实现 |
| 配置 | `v1` / `v3` / `r0` | 已共享 `media_config` |
| URL 预览 | `v1` / `v3` | 已共享 `preview_url` |
| 配额 | `v1` | 当前为 v1 独有扩展 |
| 删除 | `v1` / `v3` | 已共享 `delete_media` |
| 缩略图 | `v3` | 当前只在 v3 暴露 |

这说明 media 模块已经具备“内部复用”的基础，不需要再引入激进的版本收口。

---

## 四、推荐优化方向

### 4.1 提炼公共 helper，而不是做外部重定向

可以进一步把上传、下载的公共逻辑下沉为内部辅助函数：

```rust
async fn upload_media_common(
    state: &AppState,
    user_id: &str,
    headers: &HeaderMap,
    params: &Value,
    body: &[u8],
) -> Result<Value, ApiError> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream");

    state
        .services
        .media_service
        .upload_media(user_id, body, content_type, params.get("filename").and_then(|v| v.as_str()))
        .await
}
```

这样可以：

- 继续保留 `upload_media_v1`、`upload_media_v3` 的外部路径差异
- 避免重复的 body / content-type 处理
- 在不改变 API 行为的前提下减少重复代码

### 4.2 按版本前缀拆分阅读结构

若要继续整理路由定义，可以按版本分块，而不是按“全部指向 v3”思路组织：

```rust
Router::new()
    .route("/_matrix/media/v3/upload", post(upload_media_v3))
    .route("/_matrix/media/v1/upload", post(upload_media_v1))
    .route("/_matrix/media/r0/upload", post(upload_media_v3))
```

这里的重点是“复用同一个 handler”，不是“让客户端跳转”。

---

## 五、配额接口的处理建议

`quota/check`、`quota/stats`、`quota/alerts` 当前只在 `v1` 下存在。

这意味着：

- 不能把它们简单写成“v1 冗余”
- 也不能直接声称“应该迁到 v3”

更准确的建议是：

1. 先确认这些接口是否为本项目自定义扩展
2. 若要给新客户端统一体验，可**新增** v3 别名
3. 在确认客户端迁移完成前，继续保留 v1 路径

---

## 六、不建议的方案

- 不建议把旧版媒体写成 HTTP 30x → v3
- 不建议把 media API 改写为 client API 风格路径
- 不建议仅根据路由数量推导“可以直接减少 33%”
- 不建议文档中写入当前不存在的 `compat-media-*` feature

---

## 七、实施优先级

| 项目 | 优先级 | 建议 |
|------|--------|------|
| 抽公共上传/下载 helper | 高 | 可立即落地 |
| 整理版本分组写法 | 中 | 提升可读性 |
| 为 quota 增加 v3 别名 | 中 | 先确认产品需求 |
| 删除旧版本路径 | 低 | 当前不建议 |

---

## 八、最终结论

Media 模块应采用的不是“版本重定向方案”，而是：

1. **保留 `/_matrix/media/*` 真实路径结构**
2. **继续通过共享 handler / helper 做内部复用**
3. **把 v1 配额接口视为现有扩展能力，而不是冗余路径**
4. **仅在确认需求后再新增 v3 别名，不主动删除旧路径**

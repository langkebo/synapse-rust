# synapse-rust API 优化总方案

> 版本: 2.0
> 日期: 2026-03-27
> 目标: 在不破坏 Matrix 兼容性的前提下，减少重复路由与重复实现

---

## 一、基于当前后端实现的判断

本次重新评估以 `src/web/routes` 的实际代码为准，核心结论如下：

1. **当前项目已经有成熟的多版本复用写法**  
   `space.rs` 与 `key_backup.rs` 使用 `Router::nest()` 把同一组子路由挂到 `v1/r0/v3`，这比把版本写成 `{version}` 路径参数更符合现有代码风格。

2. **Matrix Client-Server API 不适合通过 HTTP 30x 做常规版本兼容**  
   尤其是 `POST`、`PUT`、`DELETE` 类请求，重定向会引入客户端差异、签名上下文变化和额外兼容风险。更稳妥的方式是**内部复用 handler 或 service**。

3. **当前仓库没有文档中假设的兼容 feature flag**  
   `Cargo.toml` 只有 `server` feature，没有 `compat-r0`、`compat-v1` 之类的编译开关，因此文档只能写“可新增运行时配置”，不能写成“当前已有机制”。

4. **admin v2 目前不是全面替代 v1**  
   当前仅在 `admin/user.rs` 中存在有限的 `/_synapse/admin/v2/users*` 路由，不能把 v1 全面迁移到 v2 视为既成事实。

---

## 二、优化原则

### 2.1 总体原则

- 不删除现有公开 API 路径
- 不改写 Matrix 规范中的版本前缀语义
- 先减少重复注册，再讨论接口收口
- 先抽公共 service / handler，再评估是否值得收敛路由

### 2.2 推荐版本复用模式

```rust
fn create_account_data_router(state: AppState) -> Router<AppState> {
    let router = Router::new()
        .route("/user/{user_id}/account_data/", get(list_account_data))
        .route(
            "/user/{user_id}/account_data/{type}",
            get(get_account_data).put(set_account_data),
        )
        .route(
            "/user/{user_id}/filter",
            put(create_filter).post(create_filter),
        )
        .route("/user/{user_id}/filter/{filter_id}", get(get_filter));

    Router::new()
        .nest("/_matrix/client/r0", router.clone())
        .nest("/_matrix/client/v3", router)
        .with_state(state)
}
```

这个模式的优点：

- 路径保持与规范一致
- 处理函数签名无需增加 `version` 参数
- 与项目已有 `space.rs`、`key_backup.rs` 写法一致
- 便于逐模块迁移，不需要一次性重构全站路由

---

## 三、模块评估结论

### 3.1 适合优先优化的模块

| 模块 | 当前情况 | 推荐动作 |
|------|----------|----------|
| `account_data` | r0 / v3 完全同构 | 提取子路由后用 `nest()` 复用 |
| `device` | r0 / v3 完全同构 | 提取子路由后用 `nest()` 复用 |
| `e2ee_routes` | 多数 keys 路由 r0 / v3 同构 | 提取 `keys`、`sendToDevice` 子路由 |
| `media` | 已经大量共享 handler | 继续合并实现，不做 HTTP 重定向 |

### 3.2 需要保守处理的模块

| 模块 | 原因 | 推荐动作 |
|------|------|----------|
| `friend_room` | `friends` 与 `friendships` 并存，别名差异明显 | 先复用内部函数，不强推统一 URL |
| `search` | threads 与 `thread` 模块只部分重叠 | 先抽 service，再评估是否保留双入口 |
| `room_summary` | `summary/sync` 与 `unread/clear` 仍走独立 `room_summary_service` | 暂不删除，只整理重复注册方式 |
| `admin` | v2 覆盖范围有限 | 保持 v1 主体，逐步补齐 v2 |

---

## 四、与 Matrix 规范的对齐

### 4.1 应保留的版本入口

| 范围 | 建议 |
|------|------|
| `/_matrix/client/v3/*` | 作为当前主实现 |
| `/_matrix/client/r0/*` | 对已有客户端继续保留兼容入口 |
| `/_matrix/client/v1/*` | 保留规范或当前实现仍在使用的端点 |
| `/_matrix/media/*` | 继续按媒体 API 独立前缀维护，不并入 client 前缀 |
| `/_synapse/admin/v1/*` | 继续作为当前主 admin 入口 |
| `/_synapse/admin/v2/users*` | 作为已存在但覆盖有限的增量能力 |

### 4.2 不建议的做法

- 不建议把所有 `/_matrix/client/v1|r0|v3/*` 强行改成 `/_matrix/client/{version}/*`
- 不建议把旧版本公开路径改成 HTTP 30x 跳转
- 不建议在文档中假设“r0 全部废弃”“v1 全部可删”
- 不建议把 `room_summary` 的自定义接口直接等同于 `/sync`

---

## 五、建议的落地顺序

### 5.1 第一阶段：低风险收敛

1. `account_data`
2. `device`
3. `e2ee_routes` 中完全同构的 keys 端点
4. `media` 中已经共享 handler 的上传、下载、配置端点

### 5.2 第二阶段：中风险抽象

1. `friend_room` 内部辅助函数抽取
2. `search` 中搜索、context、hierarchy 的子路由复用
3. `threads` 查询逻辑与 `thread` service 的能力对齐

### 5.3 第三阶段：高风险治理

1. `room_summary` 与主同步逻辑的边界梳理
2. admin v1 / v2 能力差距梳理
3. 访问统计、弃用日志、运行时兼容开关设计

---

## 六、实施建议

### 6.1 推荐修改方式

- 优先把重复路径提炼成子 `Router`
- 优先把重复逻辑下沉到 service 层
- 用 `nest()` 复制到多个版本前缀
- 保留原始 handler 的请求/响应结构

### 6.2 推荐验证项

- r0 与 v3 是否继续返回同样结构
- v1 特有端点是否仍按规范保留
- 媒体 API 是否仍使用 `/_matrix/media/*`
- admin v1 / v2 是否没有被文档误写为“已完全迁移”

---

## 七、风险评估

| 优化项 | 风险等级 | 主要风险 | 建议 |
|--------|----------|----------|------|
| 子路由 + `nest()` 复用 | 低 | 路由前缀拼接错误 | 先改低耦合模块 |
| service 抽取 | 中 | 行为在多个入口下不一致 | 先补回归测试 |
| search / thread 收口 | 中 | 返回结构和筛选语义不同 | 先比对响应模型 |
| room_summary 收口 | 高 | 误删现有产品能力 | 只做保守整理 |
| admin v1 / v2 整理 | 高 | 运维接口兼容性 | 先补齐覆盖范围再迁移 |

---

## 八、最终建议

这份方案的重点不是“把旧版本 URL 全删掉”，而是：

1. **保留 Matrix 规范路径**
2. **复用内部实现**
3. **以 `nest()` 替代大范围 `{version}` 参数化**
4. **以实际代码覆盖范围为依据推进，而不是按理想化路线一次性合并**

---

## 九、预期收益

1. **代码量减少** - 估计减少 30% 重复代码
2. **维护成本降低** - 统一实现减少 bug 修复成本
3. **结构清晰** - 模块边界与版本策略更容易维护
4. **文档简化** - API 文档更清晰

---

## 十、附录：示意性变更清单

### A. 适合做子路由复用的场景

```rust
let account_data_router = Router::new()
    .route("/user/{user_id}/account_data/", get(list_account_data))
    .route(
        "/user/{user_id}/account_data/{type}",
        get(get_account_data).put(set_account_data),
    );

Router::new()
    .nest("/_matrix/client/r0", account_data_router.clone())
    .nest("/_matrix/client/v3", account_data_router);
```

### B. 适合下沉到 service 的场景

```rust
async fn upload_media_v1(...) -> Result<Json<Value>, ApiError> {
    upload_media_common(...).await
}

async fn upload_media_v3(...) -> Result<Json<Value>, ApiError> {
    upload_media_common(...).await
}
```

### C. 需要补充的兼容复用层

```rust
let compat_router = Router::new()
    .route("/account_data", get(get_account_data))
    .route("/search", post(search));

Router::new()
    .nest("/_matrix/client/r0", compat_router.clone())
    .nest("/_matrix/client/v3", compat_router);
```

这里的重点是**保留旧路径并共享内部实现**，而不是新增 HTTP 重定向中间件。

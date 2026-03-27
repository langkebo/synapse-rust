# synapse-rust API 优化方案

> 版本: 2.0
> 日期: 2026-03-27
> 范围: `/Users/ljf/Desktop/hu/synapse-rust/src/web/routes`
> 目标: 在不破坏兼容性的前提下减少重复路由定义，统一实现入口，降低后续维护成本

---

## 一、结论摘要

基于当前后端代码的实际结构，原方案的总体方向是正确的，但需要做三类修正：

1. **“重定向”需要改为“别名路由/共享子路由”**  
   当前重复端点大量包含 `POST`、`PUT`、`DELETE`。对 Matrix Client API 直接做 HTTP 30x 重定向并不稳妥，容易影响客户端、认证头、请求体与 SDK 兼容性。更适合的方式是：
   - 保留旧路径
   - 让旧路径与新路径绑定到同一个 handler
   - 或者先构造不带版本前缀的子路由，再通过 `nest()` 同时挂到 `r0`、`v1`、`v3`

2. **admin API 不能简单地“以 v2 替代 v1”**  
   当前代码里 `/_synapse/admin/v2/*` 只在 `admin/user.rs` 中出现少量用户接口，绝大多数管理接口仍是 `v1`。因此“保留 admin v2、废弃 admin v1”不符合现状。

3. **部分模块不是“完全重复”，而是“局部重叠”**  
   尤其是 `search` 与 `thread`、`room_summary` 与 `sync`。这两组更适合抽服务层共用逻辑，而不是直接删除某个端点。

综合评估：

| 方向 | 结论 |
|------|------|
| account_data / device / e2ee / media 去重 | **高可行** |
| friend_room 统一实现 | **中高可行** |
| search 与 thread 直接合并 | **中低可行** |
| room_summary 删除 sync/unread 端点 | **低可行** |
| admin v1 全量替换为 v2 | **不可直接执行** |
| federation 保持现状 | **应维持不动** |

---

## 二、代码现状核查

### 2.1 路由入口现状

项目主路由入口位于 `src/web/routes/mod.rs`，通过 `.merge(...)` 方式注册多个业务子路由。当前已经存在明显的“多版本并行暴露”模式，而不是统一版本前缀网关模式。

### 2.2 版本分布现状

基于 `src/web/routes` 下源码字符串的粗略统计：

| 路径前缀 | 粗略出现次数 | 说明 |
|---------|-------------|------|
| `/_matrix/client/r0/` | 191 | 兼容路径仍大量存在 |
| `/_matrix/client/v3/` | 192 | 当前客户端主路径 |
| `/_matrix/client/v1/` | 104 | 不是纯兼容壳，部分为主实现 |
| `/_synapse/admin/v2/` | 3 | 仅用户管理少量接口使用 |

说明：

- 这些数字反映的是**源码中路由字面量出现次数**，不是精确端点总数。
- 原文中的 `656`、`~400` 目前缺乏代码生成或自动统计依据，不能作为实施基线。

### 2.3 已存在的推荐模式

项目内部已经出现两种更适合复用的模式：

#### 模式 A：同一个 handler 绑定多个版本路径

适用于 `account_data`、`device`、`e2ee_routes`、`friend_room` 这类已经共用 handler 的模块。

#### 模式 B：构造子路由后用 `nest()` 同时挂载多个版本

`space.rs` 与 `key_backup.rs` 已经使用了这种模式，说明该仓库对这种写法是接受且一致的。

```rust
let router = Router::new().route("/room_keys/version", get(handler));

Router::new()
    .nest("/_matrix/client/r0", router.clone())
    .nest("/_matrix/client/v3", router)
```

这比引入额外“版本重定向中间件”更符合当前代码风格。

---

## 三、对原方案逐项评估

### 3.1 account_data

**现状**

- 文件：`src/web/routes/account_data.rs`
- `r0` 与 `v3` 已经直接共用同一批 handler
- `account_data`、`room account_data`、`filter`、`openid request_token` 都属于明显的别名重复

**评估**

- **可行性：高**
- 可以继续优化为“去掉重复 `.route()` 定义，改成子路由 + `nest()`”
- 不需要 HTTP 重定向

**建议**

- 提取不带版本前缀的 `account_router`
- 同时挂载到 `/_matrix/client/r0` 与 `/_matrix/client/v3`
- `openid` 也纳入统一子路由

### 3.2 device

**现状**

- 文件：`src/web/routes/device.rs`
- `r0` / `v3` 的 `devices`、`delete_devices`、`devices/{device_id}`、`keys/device_list_updates` 均共用 handler

**评估**

- **可行性：高**
- 属于最适合先做的去重模块之一

**建议**

- 直接按 `key_backup.rs` 的模式重构
- 优先级可与 `account_data` 同级

### 3.3 e2ee_routes

**现状**

- 文件：`src/web/routes/e2ee_routes.rs`
- `r0` 与 `v3` 的基础密钥接口大量共用 handler
- 但设备信任、安全摘要、安全备份等新能力只暴露在 `v3`

**评估**

- **可行性：中高**
- 能统一的是“共有基础能力”，不能把所有 `v3` 新接口机械下沉到 `r0`

**建议**

- 先提炼“r0/v3 共用部分”子路由
- 将仅 `v3` 能力保留在单独的 `v3_only_router`
- 与 `key_backup.rs` 一起看，避免 E2EE 路由拆分后又形成新的重复边界

### 3.4 friend_room

**现状**

- 文件：`src/web/routes/friend_room.rs`
- 存在 `v3`、`v1`、`r0` 三种路径
- 但并非完全同构：
  - `v3` 目前只有 `/_matrix/client/v3/friends`
  - `v1` / `r0` 的请求、状态、分组等接口更完整
  - `r0` 同时存在 `friendships` 与 `friends/*` 两套历史命名

**评估**

- **可行性：中高**
- 可以统一实现，但不能简单写成“只保留 v3、其余全部重定向”

**建议**

- 先统一 handler 层
- 再梳理出三类路径：
  1. `v3` 当前已支持的稳定接口
  2. `v1/r0` 的功能别名
  3. `r0` 独有历史命名（如 `friendships`）
- 第二阶段再决定是否给 `v3` 补齐缺失功能，而不是立即删除 `v1/r0`

### 3.5 media

**现状**

- 文件：`src/web/routes/media.rs`
- 使用的是 `/_matrix/media/*`，不是 `/_matrix/client/v1/media/*`
- 当前同时存在 `v1`、`v3`、`r0`、`r1`
- 多数接口共用逻辑，但 `upload` 存在 `upload_media_v1` 与 `upload_media_v3` 两个入口

**评估**

- **可行性：中高**
- 原方案中的路径示例需要更正
- 不能直接按 client API 的版本映射思路处理 media API

**建议**

- 先做“下载/预览/config/delete”这类明显同构路由的结构去重
- 再单独评估 `upload_media_v1` 与 `upload_media_v3` 是否可以进一步合并
- 保留 `r0`、`r1` 兼容层，先共享 handler，不做 30x

### 3.6 search

**现状**

- 文件：`src/web/routes/search.rs`
- `search`、`search_recipients`、`search_rooms` 的 `r0/v3` 路由是明显重复
- 但 `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads` 与 `thread.rs` 中 `/_matrix/client/v1/rooms/{room_id}/threads` 不是简单等价

**评估**

- **可行性：分层处理**
- `search*` 的版本去重：**高可行**
- `threads` 端点直接删除：**中低可行**

**原因**

- search 模块中的 threads 带有 `user/{user_id}` 维度
- thread 模块提供的是更完整的线程管理域模型
- 二者更像“部分结果重叠”，不是完全重复路径

**建议**

- 先只处理 `search`、`search_recipients`、`search_rooms` 的版本别名去重
- 将 `get_threads` 内部逻辑改为复用 `thread_service`
- 不要在第一阶段删除 search 中的 threads 路由

### 3.7 room_summary

**现状**

- 文件：`src/web/routes/room_summary.rs`
- `v3` 路由很完整，`r0` 只覆盖了部分读接口
- 还有 `/_synapse/room_summary/v1/*` 的内部/管理型路由
- `summary/sync` 与 `summary/unread/clear` 虽然与主 `sync` 领域相关，但当前是独立 handler 和独立服务调用

**评估**

- **可行性：中低**
- `r0/v3` 的纯只读别名部分可以去重
- `summary/sync`、`summary/unread/clear` 不建议直接删

**原因**

- 代码上它们并不是简单转发到主 `sync` 模块
- 删除会改变调用语义与客户端接入点

**建议**

- 第一阶段只合并重复读接口的路由定义
- 第二阶段再评估是否抽取共用 service 方法
- 暂不删除 `summary/sync` 与 `summary/unread/clear`

### 3.8 admin API

**现状**

- `/_synapse/admin/v1/*` 在多个文件中大量存在
- `/_synapse/admin/v2/*` 当前只在 `src/web/routes/admin/user.rs` 中覆盖少量用户接口

**评估**

- **不可按原方案执行**
- 当前不能把“保留 admin v2”作为整体策略结论

**建议**

- 文档层面改为：
  - 保留 admin v1 作为当前主版本
  - 将 admin v2 视为局部演进版本
  - 后续如要统一，必须先补齐 v2 的领域覆盖面

### 3.9 federation API

**现状**

- `src/web/routes/federation.rs` 覆盖 `v1` 与 `v2`
- 与 Matrix 联邦协议强相关

**评估**

- **应保持现状**
- 不建议纳入本轮“版本收敛”改造

---

## 四、修订后的优化原则

### 4.1 保留路径兼容，先减少代码重复

本轮优化目标应是：

- 减少重复的 `.route()` 声明
- 保留现有 URL 兼容面
- 不主动减少对外暴露的旧版本路径

### 4.2 优先做“共享实现”，暂缓做“协议级废弃”

优先级建议：

1. 共享 handler
2. 抽子路由并 `nest()`
3. 提取 service 层共用逻辑
4. 增加弃用日志与观测
5. 在有统计依据后再谈删除路径

### 4.3 不以 HTTP 30x 作为主要兼容方案

对于 Matrix API，推荐使用：

- 路由别名
- 内部复用
- 子路由挂载

不推荐默认采用：

- `301`
- `302`
- `307`
- `308`

除非已经验证相关 SDK、反向代理与认证流程均可稳定处理。

### 4.4 基于真实代码结构设计方案

后续文档与实施必须以实际文件为准：

- `src/web/routes/account_data.rs`
- `src/web/routes/device.rs`
- `src/web/routes/e2ee_routes.rs`
- `src/web/routes/friend_room.rs`
- `src/web/routes/media.rs`
- `src/web/routes/search.rs`
- `src/web/routes/room_summary.rs`
- `src/web/routes/thread.rs`
- `src/web/routes/admin/*.rs`
- `src/web/routes/federation.rs`

---

## 五、推荐落地方案

### 5.1 第一阶段：低风险结构去重

目标：**不改对外语义，只减少重复注册代码**

适用模块：

- `account_data`
- `device`
- `search` 中的 `search/search_recipients/search_rooms`
- `media` 中的显性同构路由

示例模式：

```rust
fn create_account_data_router(state: AppState) -> Router<AppState> {
    let router = Router::new()
        .route("/user/{user_id}/account_data/", get(list_account_data))
        .route("/user/{user_id}/account_data/{type}", get(get_account_data).put(set_account_data))
        .route("/user/{user_id}/filter", put(create_filter).post(create_filter))
        .route("/user/{user_id}/filter/{filter_id}", get(get_filter))
        .route("/user/{user_id}/openid/request_token", get(get_openid_token));

    Router::new()
        .nest("/_matrix/client/r0", router.clone())
        .nest("/_matrix/client/v3", router)
        .with_state(state)
}
```

### 5.2 第二阶段：中风险域内统一

目标：**按业务域抽象共用服务，而不是只做路由层收缩**

适用模块：

- `e2ee_routes`
- `friend_room`
- `room_summary`
- `search` 与 `thread` 的共用服务抽取

建议：

- 先提炼 service 复用点
- 再减少 route handler 内部重复
- 最后才决定是否缩减某些别名路径

### 5.3 第三阶段：兼容策略收敛

目标：**在有调用观测与回归数据后，再决定是否废弃部分旧路径**

前置条件：

- 有访问日志或埋点统计版本使用情况
- 有 SDK / 客户端兼容回归
- 有端到端测试覆盖关键业务路径

只有满足以上条件，才考虑：

- 某些 `r0` 兼容路径转为软弃用
- 某些 `v1` 别名路径文档级标记 deprecated

---

## 六、修订后的版本策略

| 版本 | 当前定位 | 修订建议 |
|------|----------|----------|
| client v3 | 主客户端版本 | 作为首选实现入口 |
| client r0 | 兼容版本 | 保留路径，内部共享实现 |
| client v1 | 混合状态 | 只对确有主实现价值的模块保留 |
| media v1/v3/r0/r1 | 媒体协议兼容层 | 保留路径，按接口逐步统一 |
| federation v1/v2 | 联邦协议 | 维持现状 |
| admin v1 | 当前主管理版本 | 明确保留 |
| admin v2 | 局部演进版本 | 继续扩展，不替代整个 v1 |

---

## 七、修订后的实施清单

### 7.1 建议优先处理的文件

| 优先级 | 文件 | 动作 |
|-------|------|------|
| P0 | `src/web/routes/account_data.rs` | 改为子路由 + `nest()` |
| P0 | `src/web/routes/device.rs` | 改为子路由 + `nest()` |
| P1 | `src/web/routes/search.rs` | 仅收敛 `search*` 的版本重复 |
| P1 | `src/web/routes/media.rs` | 分离同构接口与差异接口 |
| P1 | `src/web/routes/e2ee_routes.rs` | 分拆共用路由与 `v3_only` 路由 |
| P2 | `src/web/routes/friend_room.rs` | 先统一 handler 再整理版本矩阵 |
| P2 | `src/web/routes/room_summary.rs` | 只处理纯重复读接口 |

### 7.2 本轮不建议直接做的事项

- 不直接删除 `search` 中的 threads 路由
- 不直接删除 `room_summary/sync`
- 不直接删除 `room_summary/unread/clear`
- 不将 admin v1 批量重写到 admin v2
- 不将所有旧版本路径统一改成 HTTP 重定向

---

## 八、兼容与配置建议

### 8.1 feature flag 不是当前首选

当前 `Cargo.toml` 的 `[features]` 仅包含基础服务特性，没有现成的 `compat-r0` 或 `compat-v1` 体系。  
如果未来确实需要“编译期裁剪兼容路由”，可以追加 feature，但这不适合作为第一阶段的主要方案。

### 8.2 更适合的控制方式

如需逐步收敛兼容接口，建议优先考虑：

- 运行时配置开关
- 弃用日志
- 指标统计
- 文档标记 deprecated

在当前阶段，**共享实现 > 配置关闭 > 编译期开关**。

---

## 九、测试与验证计划

### 9.1 路由级验证

- 为重构后的 router 补充路由存在性测试
- 重点验证 `r0`、`v1`、`v3` 是否仍正确映射到相同 handler
- 对 `space.rs`、`key_backup.rs` 已采用的 `nest()` 模式进行复用式测试

### 9.2 行为级回归

- account data 读写
- device 查询、删除、更新
- E2EE keys upload/query/claim/sendToDevice
- media upload/download/config
- friend request / group 相关接口
- room summary 的读操作与 unread 清理

### 9.3 兼容性验证

- Matrix SDK 基础流程
- Element 客户端登录、同步、媒体下载
- 若存在内部移动端或 Web 客户端，需要额外回归 `r0` 兼容路径

---

## 十、风险评估

| 优化项 | 风险等级 | 主要风险 | 缓解措施 |
|--------|----------|----------|----------|
| account_data / device 路由去重 | 低 | 路径拼接错误 | 使用 `nest()` 模式，补路由测试 |
| e2ee_routes 结构拆分 | 中 | `v3` 专属能力误被合并 | 分离 `shared` 与 `v3_only` 子路由 |
| friend_room 统一 | 中 | 历史路径矩阵复杂 | 先梳理接口矩阵，再做结构收敛 |
| search 与 thread 服务抽取 | 中 | 语义误判导致行为变化 | 不删路径，只做内部复用 |
| room_summary 收敛 | 中高 | 与客户端行为耦合 | 第一阶段只去重不删接口 |
| admin 版本统一 | 高 | 现有 v2 覆盖不全 | 暂缓，先补齐领域覆盖 |

---

## 十一、预期收益

1. **降低重复注册成本**：减少多版本 `.route()` 的手工维护
2. **降低回归风险**：通过共享实现替代路径重定向，避免兼容性问题
3. **提升结构清晰度**：区分“版本别名”与“真正的版本差异”
4. **为后续废弃策略打基础**：先具备观测与测试能力，再讨论删旧版本

---

## 十二、最终建议

本项目的 API 优化应采用以下主线：

1. **不以删除端点为目标，而以统一实现为目标**
2. **不以 30x 重定向为主，而以别名路由和 `nest()` 复用为主**
3. **不假设 admin v2 已成熟，而是承认当前 admin v1 仍是主版本**
4. **对 search/thread、room_summary/sync 采用“服务抽取优先”的保守策略**
5. **先做 P0/P1 低风险收益项，再决定是否推进协议级收敛**

换言之，原方案的方向可以保留，但实施方式应从：

- “统一重定向”
- “直接删除重复端点”
- “默认 admin v2 主版本”

修订为：

- “共享子路由”
- “保留路径、收敛实现”
- “按真实覆盖度推进版本演进”

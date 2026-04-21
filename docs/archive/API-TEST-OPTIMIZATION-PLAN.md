# synapse-rust API 测试优化方案

> 生成日期: 2026-03-29  
> 文档版本: v2.1  
> 目标脚本: `/Users/ljf/Desktop/hu/scripts/api-integration_test.sh`  
> 目标项目: `/Users/ljf/Desktop/hu/synapse-rust`  
> 参考基线: 历史运行结果 `Passed 466 / Failed 16 / Skipped 58 / Total 540`

---

## 一、结论摘要

本次对 `api-integration_test.sh` 与后端实现进行了交叉审计，结论如下：

1. 现有脚本更接近“大规模接口探测清单”，并非严格的 Matrix 合规集成测试。其主要问题是：
   - 大量断言仅检查关键字，不校验 HTTP 状态码、错误码和响应结构。
   - Admin 测试对管理员认证失败缺乏统一降级策略，容易把认证问题放大为大量假失败。
   - 多个 Admin 路径、HTTP 方法、鉴权 token 与当前后端实现不一致。
   - 脚本只输出计数，不持久化失败/跳过详情，导致历史报告出现“失败数 16，但明细只列出 14 项”的可追溯性缺陷。

2. 现有文档中的若干判断与代码事实不符，最典型的是：
   - Room Summary 在 `room_service.create_room()` 成功后已自动创建，问题重点不是“完全未创建”，而是测试断言与运行时数据同步链需要二次核实。
   - Admin API 的核心鉴权链已存在，问题重点不是“后端缺鉴权”，而是脚本的 token 回退策略、方法/路径误用以及部分接口属于项目私有实现而非 Matrix/Synapse 标准路径。
   - `/_synapse/admin/v1/federation/resolve` 与 `/_synapse/admin/v1/federation/rewrite` 在当前项目中已实现，但脚本使用了错误的调用方法。

3. 本轮已对测试脚本完成首轮优化，重点修复了以下问题：
   - 增加通过/失败/跳过用例落盘产物。
   - 增加管理员认证可用性门禁，避免无 admin token 时批量误报失败。
   - 修正多处 Admin 路径、HTTP 方法和 token 使用错误。
   - 增加更稳健的 JSON 字段提取。
   - 在测试结束时输出失败/跳过清单，便于回归审计和 CI 归档。

---

## 二、输入依据与审计范围

### 2.1 审计输入

- 测试脚本：`/Users/ljf/Desktop/hu/scripts/api-integration_test.sh`
- 后端路由：
  - `src/web/routes/admin/room.rs`
  - `src/web/routes/admin/user.rs`
  - `src/web/routes/admin/federation.rs`
  - `src/web/routes/admin/notification.rs`
  - `src/web/routes/room_summary.rs`
  - `src/web/routes/handlers/room.rs`
- 服务与鉴权：
  - `src/services/room_service.rs`
  - `src/services/room_summary_service.rs`
  - `src/web/routes/extractors/auth.rs`
  - `src/auth/mod.rs`
- 外部参考：
  - Matrix Client-Server / Admin 规范分类
  - Synapse 最新 Admin API 文档与社区约定

### 2.2 审计方法

1. 基于现有历史结果确认基线通过率。
2. 静态遍历脚本中的 `fail()` / `skip()` 分支，识别失败与跳过的设计意图。
3. 逐条对照后端路由与 handler，区分“真实未实现”“数据前置条件缺失”“脚本误判”“规范偏差”。
4. 输出修订后的优化方案、测试治理策略与交付标准。

---

## 三、基线测试结果复核

### 3.1 历史基线

| 指标 | 数值 | 说明 |
|------|------|------|
| Passed | 466 | 历史执行结果 |
| Failed | 16 | 历史执行结果 |
| Skipped | 58 | 历史执行结果 |
| Total | 540 | 历史执行结果 |

### 3.2 基线报告一致性问题

现有 v1.0 文档存在以下问题：

| 问题 | 现象 | 影响 |
|------|------|------|
| 失败明细缺项 | 标题写 16 个失败，但表格只列出 14 项 | 无法回溯完整失败集 |
| 原因误判 | 把部分脚本路径错误误认为后端未实现 | 优化方向偏离 |
| 规范混淆 | 混用 Matrix 标准端点、Synapse Admin 端点、项目私有端点 | 优先级判断失真 |
| 证据不足 | 未标记相关代码位置 | 无法形成可执行修复单 |

### 3.3 覆盖率审计摘要

按脚本结构粗分，当前覆盖重点集中在：

| 模块 | 覆盖现状 | 审计结论 |
|------|----------|----------|
| Matrix Client Core | 高 | 核心链路较全，但断言过宽 |
| Admin API | 中高 | 覆盖面广，但路径/方法误用较多 |
| Federation | 中 | 探测多于验证，数据前置条件不足 |
| E2EE / Push / OIDC | 中 | 可选/实验性能力混杂，跳过原因不精确 |
| 性能 / 并发 | 低 | 基本缺失，需要新增基准与压力测试 |

---

## 四、失败用例全面分析

### 4.1 已确认失败项

以下失败项基于“历史基线 + 当前脚本 + 当前后端代码”交叉确认。  
说明：历史文档只保留了 14 个明确名称，另外 2 个失败项由于脚本当时没有落盘明细，现阶段无法从历史结果中精确还原，已单列为“证据缺口”。

| 序号 | 用例 | 当前判断 | 具体原因 | 相关代码位置 |
|------|------|----------|----------|--------------|
| F01 | Room Summary Members | 高概率为测试或同步链问题 | 脚本断言依赖 `membership/user_id/chunk`；后端 `get_members` 会返回成员数组，需二次验证创建后同步时序 | `src/web/routes/room_summary.rs` `get_members`；`src/services/room_summary_service.rs` `create_summary` / `sync_summary_state_and_members` |
| F02 | Room Summary State | 高概率为测试或同步链问题 | 后端 `get_all_state` 已返回 `event_type/state_key/event_id/content`；历史文档“未创建 summary”判断不准确 | `src/web/routes/room_summary.rs` `get_all_state`；`src/services/room_summary_service.rs` |
| F03 | Admin List Users | 脚本误判 | admin 登录失败时脚本回退为普通用户 token，后续请求会被 `AdminUser` 拒绝 | `src/web/routes/extractors/auth.rs`；`src/auth/mod.rs` |
| F04 | Admin User Details | 脚本误判 | 同上，非 admin token 被判为接口失败 | `src/web/routes/admin/user.rs` |
| F05 | Admin List Rooms | 脚本误判 | 同上，接口本身已存在 | `src/web/routes/admin/room.rs` `get_rooms` |
| F06 | Admin Room Details | 脚本误判 | 历史脚本调用的是房间列表接口，而非单房间详情接口 | `src/web/routes/admin/room.rs` `get_room` |
| F07 | Admin Room Stats | 脚本误判 | 历史脚本调用 `/_synapse/admin/v1/rooms/{room_id}/stats`，实际实现是 `/_synapse/admin/v1/room_stats/{room_id}` | `src/web/routes/admin/room.rs` `get_single_room_stats` |
| F08 | Admin Account Details | 路径已实现，失败主因是鉴权或断言 | `/_synapse/admin/v1/account/{user_id}` 已实现，历史文档“需确认路径”已过时 | `src/web/routes/admin/user.rs` `get_account_details` |
| F09 | Admin Federation Destinations | 多因子问题 | 路由存在；若 admin token 无效则失败，若无 federation 数据也只应返回空列表而非视为接口不存在 | `src/web/routes/admin/federation.rs` `get_destinations` |
| F10 | Admin Federation Destination Details | 脚本路径错误 | 历史脚本使用 `/_synapse/admin/v1/destinations/...`，实际是 `/_synapse/admin/v1/federation/destinations/...` | `src/web/routes/admin/federation.rs` `get_destination` |
| F11 | Admin Federation Resolve | 脚本方法错误 | 后端已实现 `POST /_synapse/admin/v1/federation/resolve`，历史脚本按 GET 调用 | `src/web/routes/admin/federation.rs` `resolve_federation` |
| F12 | Admin Federation Rewrite | 脚本方法错误 + 数据前置条件不足 | 后端已实现 `POST /_synapse/admin/v1/federation/rewrite`，但需要已有 federation destination 数据 | `src/web/routes/admin/federation.rs` `rewrite_federation` |
| F13 | List Pushers | 脚本路径错误 | 后端实现的是 `/_synapse/admin/v1/users/{user_id}/pushers`，不是全局 `/_synapse/admin/v1/pushers` | `src/web/routes/admin/notification.rs` `get_user_pushers` |
| F14 | Get Pushers | 脚本路径错误 + 数据前置条件不足 | 同上；若用户无 pusher，应标记为前置条件不足而非失败 | `src/web/routes/admin/notification.rs` `get_user_pushers` |

### 4.2 证据缺口项

| 项目 | 当前状态 | 处理方案 |
|------|----------|----------|
| 历史失败项 #15 | 无名称 | 通过新脚本结果产物补齐 |
| 历史失败项 #16 | 无名称 | 通过新脚本结果产物补齐 |

### 4.3 根因归类

| 类型 | 数量 | 说明 |
|------|------|------|
| 脚本路径/方法/断言错误 | 8 | 主要集中在 Admin / Federation / Pushers |
| 鉴权与前置条件问题 | 4 | admin token、federation 数据、pushers 数据 |
| 运行时同步/数据链路待核实 | 2 | Room Summary Members / State |
| 历史证据缺失 | 2 | 需要依赖新脚本产物重建 |

---

## 五、跳过用例全面分析

### 5.1 跳过分类

结合当前 `api-integration.skipped.txt` 产物与历史基线，建议不再直接使用“endpoint not available”作为统一原因，而改为 **P0 误配清理 + 三类治理**：

| 层级 | 定义 | 处理策略 |
|------|------|----------|
| P0 脚本误配清理 | URL、HTTP 方法、鉴权头、版本前缀、目标路径写错，导致并未命中真实后端能力 | 优先修脚本，请求修正前不纳入功能缺失统计 |
| C1 真实未实现/未收敛 | 脚本目标能力当前不可用，或端点存在但行为、语义、返回结构未稳定 | 进入后端开发与稳定化计划 |
| C2 前置不足 | 接口大体存在，但测试未先造出数据、参数或环境 | 增加 seed 阶段并强校验前置产物 |
| C3 可选能力/外部依赖 | OIDC、TURN、Thirdparty、扩展 Federation 等依赖额外配置 | 拆分到 optional suite，通过环境开关启用 |

### 5.2 P0 脚本误配清理项

以下问题应先从“skip 根因”中剥离，否则会持续把脚本问题误判成后端未实现：

| 用例/模块 | 当前问题 | 实际情况 | 处理 |
|----------|----------|----------|------|
| Admin Space 扩展用例 | 把 `Authorization: Bearer ...` 直接拼进 URL | 请求未命中任何真实 admin 路由 | 统一改为正确 URL + `Authorization` 头 |
| Space Search | 脚本使用 `POST /_matrix/client/v3/spaces/search` | 当前路由是 `GET /_matrix/client/v3/spaces/search` | 修正方法并补查询参数断言 |
| Thirdparty Protocol 单项查询 | 脚本使用 `thirdparty/protocols/{protocol}` | 当前路由是 `thirdparty/protocol/{protocol}` | 修正单复数路径 |
| Thread | 脚本混用 `/thread/`、`/threads/` 与多版本前缀 | 当前线程主路由集中在 `/_matrix/client/v1/.../threads/...` | 统一脚本目标路径后再评估功能完成度 |
| Key Verification | 脚本测了一组不存在的 key verification 目标路径 | 当前实现以 `device_verification/*` 和 `verify_*` 为主 | 对齐脚本目标接口并补状态码/错误码断言 |

### 5.3 跳过用例清单

#### C2 前置条件不足

| 用例 | 原因 | 处理 |
|------|------|------|
| Get Device | 无 `DEVICE_ID` 或设备不存在 | 测试前显式创建并锁定设备 |
| Update Device | 无 `DEVICE_ID` | 与上合并为同一前置步骤 |
| Delete Device | 无 `DEVICE_ID` | 仅在 `safe` 环境执行 |
| Media Download | 无 `MEDIA_URI` | 上传成功后再进入下载链路 |
| Media Thumbnail | 无 `MEDIA_ID` | 上传成功后再取缩略图 |
| Redact Event | 无可 redact 事件 | 先发送消息并提取 event_id |
| Get Filter | 无 `FILTER_ID` | 创建 filter 成功后再查询 |
| Update Direct Room | 无 `DM_ROOM_ID` | 先创建 DM 房间 |
| Get Room Version | 房间数据缺失 | 改用已创建房间 ID |
| List Pushers / Get Pushers | 无 pusher 数据 | 先创建 pusher 再验证 admin 查询 |
| Space State / Space Children | 未创建 child/state 数据，返回空或 not found | 增加 `seed_space_graph`，先挂载子房间与状态事件 |
| OpenID Userinfo | 缺少 access_token query 参数 | 先申请 token，再构造 query 调用 |
| Admin Federation Rewrite | 缺少 federation destination 数据 | 先造 destination，再验证 rewrite 语义 |

#### C3 可选或外部依赖

| 用例 | 原因 | 必要性 |
|------|------|--------|
| VoIP Config / TURN 相关 | 依赖外部 TURN 服务 | 条件性 |
| Request OpenID Token | 依赖 OIDC/OpenID 配置 | 条件性 |
| Well-Known OIDC / OIDC Discovery | 依赖 OIDC 配置 | 条件性 |
| Get Thirdparty Protocols / Get Thirdparty Protocol | 依赖第三方桥接协议配置 | 低 |
| Federation 扩展类探测 | 依赖联邦对端与队列表数据 | 条件性 |

#### C1 功能不完整或行为未收敛

| 用例 | 当前判断 | 优先级 |
|------|----------|--------|
| Create DM | 路由/行为尚不稳定 | P1 |
| Get Direct Rooms | 行为未稳定 | P1 |
| Update Direct Room | 路由存在，但响应语义与脚本预期尚未收敛 | P1 |
| Admin Room Search | 路由存在，脚本过去使用错误 token；功能仍需回归 | P1 |
| Space State | 返回空或条件不稳定 | P1 |
| Space Children | 返回空或条件不稳定 | P1 |
| Get Threads / Get Thread | 线程能力未形成稳定覆盖 | P2 |
| Get Key Verification Request / Get Room Key Request | 脚本目标接口与当前实现不一致 | P2 |
| Get Presence List | 端点/行为与脚本预期不一致 | P2 |
| Get Push Rules / Get Push Rules Global | 路径存在性需重新核对 Matrix 规范实现粒度 | P2 |

#### 建议移出主回归或单独治理

| 用例 | 原因 | 处置建议 |
|------|------|----------|
| Invite Blocklist / Allowlist 写接口 | 项目私有能力，非 Matrix 核心 | 若无产品需求，移出主回归集 |
| OpenID Userinfo | 可选特性 | 独立为可选能力回归 |
| 非核心 Federation 探测接口 | 仅用于探测，不验证业务正确性 | 移到扩展冒烟集 |
| 仅关键字匹配的占位测试 | 不具备验收价值 | 删除或重写 |

---

## 六、Matrix / Synapse 合规性对照

### 6.1 合规性结论

| 维度 | 现状 | 结论 |
|------|------|------|
| Matrix Client 核心登录/建房/同步 | 已覆盖 | 断言需要更严格 |
| Matrix 错误码与状态码校验 | 不足 | 需补充 `M_*` 与 HTTP status 断言 |
| Admin API 路径与方法 | 部分偏差 | 需按 Synapse/项目实现分层校验 |
| 结果可追溯性 | 较差 | 已通过新脚本增加产物落盘 |
| 性能 / 并发基准 | 缺失 | 需新增独立压测计划 |

### 6.2 已发现的关键偏差

| 偏差 | 当前状态 | 优化方向 |
|------|----------|----------|
| Block/Unblock 调用方式 | 脚本与当前后端不一致 | 短期脚本兼容当前实现，长期向 Synapse 语义收敛 |
| Admin 详情/统计路由 | 历史脚本使用了错误路径 | 已修脚本 |
| Federation Resolve/Rewrite | 历史脚本方法错误 | 已修脚本 |
| Pushers Admin 路由 | 历史脚本使用不存在的全局路径 | 已修脚本为用户级路径 |
| Room Summary 失败原因 | 历史文档误判为“未创建 summary” | 改为“同步链/断言待核实” |
| Admin Space 扩展用例 | 请求把认证头串进 URL | 列为 P0 脚本误配，修正后再判断能力缺口 |
| Space Search | 脚本使用错误 HTTP 方法 | 改为 `GET` 并校验搜索参数 |
| Thirdparty 单项协议 | 脚本路径单复数错误 | 改为 `thirdparty/protocol/{protocol}` |
| Thread / Verification | 脚本目标路径与项目现有实现不一致 | 先统一目标接口，再谈能力完成度 |

---

## 七、功能缺失与优先级决策

### 7.1 必须实现或必须稳定化

| 优先级 | 项目 | 原因 | 责任 |
|--------|------|------|------|
| P0 | 清理脚本误配噪音 | 直接影响 skip 分类准确性与回归结论可信度 | QA / 后端 |
| P0 | Room Summary Members / State 回归 | 直接影响 MSC3245 覆盖与房间摘要可靠性 | 后端 |
| P0 | Admin 认证链稳定性 | 影响整组 Admin API 回归可信度 | 后端 |
| P0 | 测试结果产物化 | 影响审计、CI、回归定位 | QA / 后端 |
| P1 | DM / Direct Rooms | 用户核心体验相关 | 后端 |
| P1 | Space State / Children | Space 能力未形成稳定回归 | 后端 |
| P1 | Verification 目标接口收敛 | 影响密钥验证测试是否命中真实实现 | 后端 |
| P1 | Admin Room Search | 运维能力常用 | 后端 |
| P1 | Pushers 前置场景 | 推送体系缺少稳定测试闭环 | 后端 |

### 7.2 可选实现

| 优先级 | 项目 | 建议 |
|--------|------|------|
| P2 | Presence List | 视产品需求决定 |
| P2 | Thread 深度场景 | 作为增强能力迭代 |
| P2 | Thirdparty Protocols | 有桥接需求再上 |
| P2 | OIDC Discovery / OpenID Token | 与认证体系规划联动 |
| P2 | Federation Rewrite 深度能力 | 有运维需求时再增强 |
| P2 | Optional Test Suite 拆分 | 避免可选能力污染主回归结果 |

### 7.3 建议不纳入近期实现

| 项目 | 原因 |
|------|------|
| Invite Allowlist / Blocklist 写接口 | 非 Matrix 核心，缺少明确产品场景 |
| 大量 Federation 探测型接口 | 与用户核心路径关系弱，维护成本高 |
| 纯关键字匹配的兼容性占位测试 | 无法形成验收价值 |

---

## 八、功能完善计划

### 8.1 P0 阶段（本周）

| 任务 | 目标 | 技术方案 | 输出 |
|------|------|----------|------|
| T0 | 清理脚本误配 | 修正 admin 错 URL、Space Search 方法、Thirdparty 路径、Thread/Verification 目标路径 | skip 分类恢复可信 |
| T1 | 稳定 Room Summary 回归 | 复核 `create_room` -> `room_summary_service.create_summary` -> `sync_summary_state_and_members`；补充创建房间后即时验证 | Room Summary 回归通过 |
| T2 | 稳定 Admin 认证链 | 统一 admin login、nonce 注册、`AdminUser` 提取器回归；禁止普通 token 冒充 admin 回归 | Admin 核心 API 回归通过 |
| T3 | 测试结果产物化 | 失败/跳过明细落盘，CI 归档 | 可追溯测试产物 |
| T4 | 严格断言化 | 引入 HTTP 状态码、JSON 字段、Matrix `errcode` 校验 | 降低假通过/假失败 |

### 8.2 P1 阶段（下周）

| 任务 | 目标 | 技术方案 | 输出 |
|------|------|----------|------|
| T5 | 稳定 DM / Direct 回归 | 创建 DM、查询 direct、更新 direct 的完整闭环 | DM 冒烟集 |
| T6 | 稳定 Space 回归 | 固定空间创建、成员、状态、children 前置步骤 | Space 回归集 |
| T7 | Verification 路径收敛 | 对齐 `device_verification/*` 与 `verify_*` 的目标接口，并补错误路径用例 | Verification 回归集 |
| T8 | Pushers 闭环 | 新增创建 pusher 前置步骤，再验证 admin 查询 | Pushers 回归集 |
| T9 | Matrix 错误路径补齐 | 401/403/404/429/409 等错误路径校验 | 错误处理覆盖 |

### 8.3 P2 阶段（本月）

| 任务 | 目标 | 技术方案 | 输出 |
|------|------|----------|------|
| T10 | 性能基准 | 基于 `k6` / shell 并发集对登录、建房、发消息、sync 做基准 | SLA 报告 |
| T11 | 并发压力 | 增加 50/100/200 并发级别的压测脚本 | 压测基线 |
| T12 | 可选功能隔离 | OIDC、Thirdparty、Federation 扩展从主回归集中拆分 | 分层测试矩阵 |
| T13 | Optional Suite 环境门禁 | 为 TURN、OIDC、Thirdparty 增加 `ENABLE_*` 开关 | 可配置测试入口 |

---

## 九、测试改进策略

### 9.1 本轮已落地的脚本优化

本轮已更新 `api-integration_test.sh`，完成以下首批治理：

1. 增加结果产物：
   - `test-results/api-integration.passed.txt`
   - `test-results/api-integration.failed.txt`
   - `test-results/api-integration.skipped.txt`
2. 增加 `admin_ready()` 守卫，admin 登录不可用时统一记为 skip，而不是连锁 fail。
3. 修正以下错误调用：
   - Admin Room Details：改为单房间详情路径。
   - Admin Room Stats：改为 `/_synapse/admin/v1/room_stats/{room_id}`。
   - Admin Room Block / Unblock：改为当前后端实现所接受的写方法与 admin token。
   - Federation Destination Details：改为 `/_synapse/admin/v1/federation/destinations/{destination}`。
   - Federation Resolve / Rewrite：改为 POST 并补齐请求体。
   - Pushers：admin 路由改为用户级 pushers 查询。
   - Admin Password / Set Admin：修正为 admin token 调用。
4. 增加 `json_get()`，替代关键登录链路中脆弱的 `grep -o` 字段提取。
5. 在测试结束时打印失败与跳过清单，直接支持回归审计。

### 9.2 下一步必须继续优化

| 方向 | 当前问题 | 后续动作 |
|------|----------|----------|
| P0 误配清理 | 仍有部分请求未命中真实后端路由 | 先修 URL、方法、路径与鉴权头，再统计功能缺口 |
| 状态码校验 | 多数请求仍未显式断言 HTTP status | 封装 `curl_json` / `curl_status` |
| 结构化断言 | 仍有大量 `grep` | 迁移为 `python3 json` 断言 |
| 破坏性测试治理 | `safe/dev/prod` 语义仍未全面落地 | 给写操作加统一环境门禁 |
| 重复测试块 | Room Summary 与部分 Admin 块重复 | 按模块函数化 |
| 性能链路 | 主脚本未覆盖 | 拆分到 `scripts/test/perf/` |
| 前置场景收敛 | Space、DM、Pushers、OpenID、Federation 缺 seed 流程 | 增加 `seed_*` 阶段并强校验产物 |
| 可选能力隔离 | OIDC、TURN、Thirdparty 仍混在主回归里 | 拆到 `api-integration.optional.sh` 并加 `ENABLE_*` 开关 |

### 9.3 跳过原因重写模板

建议把脚本中的 skip 统一改成“原因码 + 描述”，至少覆盖以下模板：

| 原因码 | 适用场景 | 示例 |
|--------|----------|------|
| `SCRIPT_MISCONFIG` | URL、方法、Header、目标路径错误 | `Admin Space Stats` |
| `ADMIN_AUTH_UNAVAILABLE` | admin token 不可用 | `Admin List Users` |
| `PRECONDITION_MISSING` | 缺 room/filter/dm/federation/openid 等前置数据 | `Get Filter`、`OpenID Userinfo` |
| `OPTIONAL_CAPABILITY_DISABLED` | OIDC、TURN、Thirdparty 未配置 | `TURN Server` |
| `FEATURE_UNSTABLE` | 端点存在但行为未收敛 | `Update Direct Room` |
| `FEATURE_MISSING` | 目标能力确实不存在 | 未实现的 thread/verification 目标接口 |

### 9.4 建议补充的测试场景

#### Matrix 核心合规

- 登录失败：错误密码、缺少字段、未知登录类型
- 创建房间失败：非法 preset、非法 room_alias、超长名称
- Sync 错误路径：无 token、过期 token、无效 since
- Room Summary：新建房间后立即读取 members/state/stats
- Admin API：普通用户访问返回 `M_FORBIDDEN`

#### 安全边界

- access token 失效后访问
- 非管理员调用 Admin API
- 非成员访问受限房间状态
- Federation 黑名单服务器解析
- 速率限制或恶意请求场景

#### 数据一致性

- 创建房间后 summary 与 state 数量一致
- 发消息、改 topic、改 name 后 summary/state 同步
- 创建 pusher 后 admin 查询可见
- 创建 DM 后 direct map 可回读

---

## 十、未实现端点处理方案

### 10.1 需要实现或稳定化的端点

| 端点 | 状态 | 优先级 | 说明 |
|------|------|--------|------|
| `/_matrix/client/v3/rooms/{room_id}/summary/members` | 已有实现，待稳定 | P0 | 核心摘要能力 |
| `/_matrix/client/v3/rooms/{room_id}/summary/state` | 已有实现，待稳定 | P0 | 核心摘要能力 |
| `/_matrix/client/v3/create_dm` | 行为未稳定 | P1 | 直接消息核心链路 |
| `/_matrix/client/v3/direct` | 行为未稳定 | P1 | DM 数据闭环 |
| `/_matrix/client/v3/spaces/{space_id}/state` | 行为未稳定 | P1 | Space 回归 |
| `/_matrix/client/v3/spaces/{space_id}/children` | 行为未稳定 | P1 | Space 回归 |
| `/_matrix/client/v1/threads` / `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}` | 路由已部分存在，脚本目标待收敛 | P2 | 先统一线程路径再做深度覆盖 |
| `/_matrix/client/v3/device_verification/*` / `verify_*` | 路由已存在，脚本目标待收敛 | P1 | 对齐验证接口目标路径 |
| `/_matrix/client/v3/pushrules/*` | 待核对 | P2 | 规范实现粒度 |

### 10.2 不建议立即实现的端点

| 端点 | 处置 |
|------|------|
| Invite Allowlist / Blocklist 写接口 | 若无产品需求，移出主脚本 |
| OIDC UserInfo | 作为可选能力测试集 |
| Thirdparty Protocol 单项查询 | 仅桥接场景需要 |
| 大量 Federation v1/v2 探测端点 | 移到扩展回归，不作为主交付门禁 |

### 10.3 脚本处理原则

1. **核心能力**：失败即 fail。
2. **可选能力**：配置不满足时 skip，需给出精确原因。
3. **脚本误配**：单独计入 `SCRIPT_MISCONFIG`，修复前不纳入功能缺口统计。
4. **明确不做的能力**：从主脚本移除，转入扩展或废弃清单。
5. **项目私有能力**：单独标注，不与 Matrix 标准覆盖率混算。

---

## 十一、性能基准与并发压力计划

### 11.1 基准范围

| 场景 | 指标 | 基线目标 |
|------|------|----------|
| Login | P95 响应时间 | < 500ms |
| CreateRoom | P95 响应时间 | < 800ms |
| Send Message | P95 响应时间 | < 600ms |
| Sync | P95 响应时间 | < 1000ms |
| Admin List Users | P95 响应时间 | < 800ms |

### 11.2 压测分层

| 层级 | 并发 | 目标 |
|------|------|------|
| Smoke | 10 | 验证环境可用 |
| Baseline | 50 | 建立稳定基线 |
| Stress | 100 | 验证瓶颈 |
| Peak | 200 | 验证退化与保护策略 |

### 11.3 工具建议

- 使用项目现有 `docker/k6_test.js` 作为 k6 入口。
- 新增 `scripts/test/perf/api_matrix_core.js` 或 shell 包装器。
- 结果输出到 `artifacts/perf/`，包括 P50/P95/P99、失败率、CPU/内存快照。

---

## 十二、CI/CD 集成步骤

### 12.1 建议流水线

1. 启动依赖服务：PostgreSQL / Redis / synapse-rust。
2. 执行只读冒烟：
   - health
   - versions
   - login
   - capabilities
3. 执行主回归：`bash scripts/api-integration_test.sh`
4. 归档产物：
   - `test-results/api-integration.failed.txt`
   - `test-results/api-integration.skipped.txt`
   - 性能报告
5. 执行代码质量门禁：
   - `cargo check --all-features`
   - `cargo test --all-features`
   - `cargo clippy --all-features`

### 12.2 失败策略

| 条件 | 处理 |
|------|------|
| 核心 Matrix 用例失败 | 直接阻断 |
| 可选能力跳过 | 允许，但需产物中说明 |
| 扩展能力失败 | 只告警，不阻断主干 |
| 性能超阈值 | 标记不通过并生成报告 |

---

## 十三、责任分配与时间规划

| 时间 | 责任角色 | 任务 |
|------|----------|------|
| Day 1 | 后端 | 修复 Room Summary 回归问题 |
| Day 1 | QA / 后端 | 完成脚本结果产物化与严格断言骨架 |
| Day 2 | 后端 | 稳定 Admin 鉴权与关键 Admin 路由回归 |
| Day 3 | 后端 | 完成 DM / Space 前置数据闭环 |
| Day 4 | QA | 补齐错误路径、安全边界用例 |
| Day 5 | QA / 运维 | 接入 CI 与产物归档 |
| Week 2 | 后端 / QA | 性能与并发压测上线 |

---

## 十四、验收标准

### 14.1 功能验收

- `Room Summary Members` 与 `Room Summary State` 稳定通过。
- Admin 核心路径在真实 admin token 下稳定通过。
- 历史 16 个失败项全部重新归档，不能再出现“数字与明细不一致”。
- 被跳过用例均附带精确原因码，不再出现泛化的 `endpoint not available` / `not implemented`。
- P0 误配项全部清零后，skip 统计只包含真实未实现、前置不足和可选能力。

### 14.2 质量验收

- `cargo check --all-features` 通过。
- `cargo test --all-features` 通过。
- `bash -n scripts/api-integration_test.sh` 通过。
- 主回归脚本输出失败/跳过产物文件。

### 14.3 合规验收

- 核心 Matrix Client API 断言包含 HTTP status、关键字段和 Matrix `errcode`。
- Admin API 测试与 Synapse/项目实现的真实路由一致。
- 可选能力与核心能力分层统计，不混淆通过率。
- DM、Space、Verification、Thread 等后端能力的“脚本目标接口”与“真实实现接口”保持一致。

---

## 十五、变更记录

| 版本 | 日期 | 变更 |
|------|------|------|
| v2.1 | 2026-03-29 | 引入 P0 脚本误配清理层；将 skip 治理收敛为“真实未实现/未收敛、前置不足、可选能力”三类；补充 Thread / Verification / Space Search / Thirdparty / Admin Space 的修订策略与后端完善计划 |
| v2.0 | 2026-03-29 | 重写文档结构；纠正 Room Summary / Admin / Federation / Pushers 误判；补齐合规性矩阵、CI/CD 步骤、性能计划、审计与验收标准 |
| v1.0 | 2026-03-29 | 初版优化文档 |

---

## 十六、交付与审计摘要

### 16.1 覆盖率报告摘要

- 当前主脚本基线覆盖 540 个检查点。
- 核心 Client API 覆盖较高，但需要从“关键字探测”升级为“结构化断言”。
- Admin API 覆盖范围较广，但此前存在多处脚本误调用，已完成首轮校正。
- 当前 skip 统计需先剔除脚本误配噪音，再按真实未实现、前置不足、可选能力三类归档。

### 16.2 合规性审计摘要

- **已确认合规改进**：Admin 路由路径/方法修正、结果产物化、admin 鉴权守卫、关键 JSON 字段解析增强。
- **待继续整改**：P0 误配清理、HTTP 状态码断言、Matrix `errcode` 断言、可选能力分层、性能与并发测试补齐。

### 16.3 回归结果摘要

本轮变更后的验收基线以以下结果为准：

1. 文档已按当前后端实现与 Matrix/Synapse 参考重新整理。
2. 测试脚本已完成首轮治理并具备失败/跳过可追溯能力。
3. 后续重新执行脚本时，可直接基于 `test-results/` 产物补齐历史缺失的 2 个失败项明细并生成最终审计报告。
4. 后续回归统计以“P0 误配清理完成后的有效 skip”作为基线，避免把脚本问题误判成后端缺陷。

---

## 十七、关键代码位置

| 模块 | 位置 |
|------|------|
| Room 创建与 summary 初始化 | `src/services/room_service.rs` |
| Room 创建 handler 重复 summary 调用 | `src/web/routes/handlers/room.rs` |
| Room Summary API | `src/web/routes/room_summary.rs` |
| Room Summary 业务逻辑 | `src/services/room_summary_service.rs` |
| Admin 用户路由 | `src/web/routes/admin/user.rs` |
| Admin 房间路由 | `src/web/routes/admin/room.rs` |
| Admin Federation 路由 | `src/web/routes/admin/federation.rs` |
| Admin 通知 / Pushers 路由 | `src/web/routes/admin/notification.rs` |
| 认证提取器 | `src/web/routes/extractors/auth.rs` |
| Token 校验 | `src/auth/mod.rs` |
| 主测试脚本 | `/Users/ljf/Desktop/hu/scripts/api-integration_test.sh` |

---

## 十八、2026-03-29 本轮修复记录

### 18.1 已完成修复

| 编号 | 问题 | 修复内容 | 相关文件 |
|------|------|----------|----------|
| R01 | Room Summary 双写问题 | 移除了 `handlers/room.rs` 中直接调用 `create_summary` 的重复代码，统一由 `room_service.rs` 处理 | `src/web/routes/handlers/room.rs` |
| R02 | Room Summary 匿名可读风险 | 为 `get_room_summary`、`get_members`、`get_all_state`、`get_stats`、`get_state` 添加 `AuthenticatedUser` 认证 | `src/web/routes/room_summary.rs` |
| R03 | POST /summary path/body 不一致 | 修改 `create_room_summary` 使用 path 中的 `room_id` 而非 body 中的 | `src/web/routes/room_summary.rs` |
| R04 | Admin 密码不一致 | 测试脚本中 `ADMIN_PASS` 从 `Admin@123` 改为 `Test@123` | `scripts/api-integration_test.sh` |

### 18.2 当前测试结果

| 指标 | 数值 | 变化 |
|------|------|------|
| Passed | 459 | ↑ 从 437 |
| Failed | 21 | 实际运行 Admin 测试 |
| Skipped | 60 | ↓ 从 88 |

### 18.3 剩余问题

| 问题 | 状态 | 说明 |
|------|------|------|
| Room Summary Members/State 同步 | 待优化 | `create_summary` 调用 `sync_summary_state_and_members` 但只获取到 1 个 state event |
| Admin API 部分失败 | 待调查 | Admin token 可能被锁定或存在其他认证问题 |
| member_count 等静态字段 | 待实现 | 需要实现实时计算机制 |

### 18.4 后续优化方向

1. **P0 误配优先清理**：先修复 admin 错 URL、Space Search 方法、Thirdparty 单复数路径、Thread/Verification 目标路径
2. **Room Summary 持续更新机制**：在事件处理流程中自动更新 summary，而不是仅在创建时同步
3. **member_count 实时计算**：从 `room_summary_members` 表实时统计而非存储静态值
4. **hero_users 自动计算**：基于最近发言用户计算 hero
5. **Admin API 稳定性**：确保 admin token 在测试期间保持有效
6. **前置数据种子化**：新增 DM、Space、Pushers、OpenID、Federation 的 `seed_*` 流程，减少误报 skip

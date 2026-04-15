# synapse-rust 后端契约缺陷梳理与增量整改方案

## 1. 目标与范围

- 目标：系统性梳理 `matrix-js-sdk/docs/api-contract` 中记录的 `synapse-rust` 后端问题，并以 `element-hq/synapse` 官方实现与 Matrix Client-Server / Media API 为基线，制定可执行、可回滚、可测试的增量整改方案。
- 原则：
  - 最小改动：优先复用现有 service / storage / route / test 基座，禁止平行重复开发。
  - 契约驱动：先修复“文档已声明但后端未兑现”的接口与字段，再补性能和兼容细节。
  - 清理占位：对无明确业务价值、无 SDK 使用、无 Synapse 基线的占位接口，优先下线路由并删除桩代码。
  - 先止血后增强：优先处理安全漏洞、`M_UNRECOGNIZED`、错误契约和高频同步链路，再处理扩展能力。

## 2. 基线结论

- 已补齐的核心能力：
  - `/sync` 已接入更多 filter 语义与 lazy-load 相关修复。
  - Sliding Sync 已补齐核心列表过滤、房间组装、单 range 增量 `ops`、`e2ee` extension、`to_device` extension。
  - `to_device` 已支持扩展内独立 `since` / `limit`，并返回 `events` 与 `next_batch`。
- 当前主要差距已从“主链路完全缺失”转向：
  - Sliding Sync 细粒度 MSC3886 兼容项。
  - 已挂载占位接口仍返回 `M_UNRECOGNIZED`。
  - 少量路由仍是模拟成功、空实现或忽略关键路径参数。
  - 个别路由存在权限面过宽问题，已属于安全缺陷而非单纯契约落差。

## 3. 问题分类总览

### 3.1 功能缺失

| ID | 模块 | 问题 | 证据 |
| --- | --- | --- | --- |
| SS-01 | Sliding Sync | 房间对象缺少 `heroes`、`invite_state`、`joined_count`、`invited_count` 等契约字段 | `src/services/sliding_sync_service.rs` 的 `room_to_json()` / `build_room_json()` 当前仅填充基础字段 |
| SS-02 | Sliding Sync | `timeout` / `clientTimeout` 已入参但未用于长轮询等待 | `src/storage/sliding_sync.rs` 定义字段；`src/services/sliding_sync_service.rs` 未消费 |
| SS-03 | Sliding Sync | 多 range 增量请求无法生成差量 `ops`，回退为整段 `SYNC` | `build_incremental_ops()` 仅支持 `previous.ranges.len() == 1 && current.len() == 1` |
| SS-04 | Sliding Sync | `room_types` / `not_room_types` / `tags` / `not_tags` 已建模但未下推到查询 | `src/storage/sliding_sync.rs` `push_room_filters()` 未处理这些字段 |
| RM-01 | Room | `GET /rooms/{room_id}/initialSync` 已挂载但未实现 | `src/web/routes/handlers/room.rs` `room_initial_sync()` 返回 `M_UNRECOGNIZED` |
| TP-01 | Thirdparty | `get_location` / `get_user` 仅返回空数组，未基于协议和查询参数执行查询 | `src/web/routes/thirdparty.rs` |

### 3.2 占位桩代码

| ID | 模块 | 问题 | 证据 |
| --- | --- | --- | --- |
| VC-01 | Voice | `convert` 为模拟成功，`converted_content = null` | `src/web/routes/voice.rs` |
| VC-02 | Voice | `optimize` 为模拟成功，`optimized_content = null` | `src/web/routes/voice.rs` |
| RM-02 | Room | 11 个房间扩展端点为纯占位，前置校验后统一 `M_UNRECOGNIZED` | `room.md` 与 `src/web/routes/handlers/room.rs` 对应实现 |

### 3.3 返回 `M_UNRECOGNIZED`

| ID | 模块 | 范围 | 说明 |
| --- | --- | --- | --- |
| MU-01 | Room | `initialSync`、`fragments`、`service_types`、`event_perspective`、`reduced_events`、`rendered`、`translate`、`convert`、`vault_data`、`external_ids`、`device` | 契约已记录为已挂载未支持，占位返回 `M_UNRECOGNIZED` |
| MU-02 | Voice | `transcription` 当前测试预期为显式不支持错误 | 属于兼容策略问题，后续需统一为真实实现或文档化移除 |

### 3.4 性能瓶颈

| ID | 模块 | 问题 | 影响 |
| --- | --- | --- | --- |
| PF-01 | Sliding Sync | 忽略 `timeout`，客户端只能高频轮询 | 放大请求频率、数据库扫描与缓存 miss |
| PF-02 | Sliding Sync | 多 range 回退整段 `SYNC` | 房间量大时响应体膨胀，列表抖动明显 |
| PF-03 | Thirdparty | 当前虽然返回空，但补实现若不做协议过滤与 limit 将放大查询成本 | 需要先设计受限查询路径 |

### 3.5 安全漏洞

| ID | 模块 | 问题 | 风险等级 | 证据 |
| --- | --- | --- | --- | --- |
| SEC-01 | Widget | 多个 `v1` widget CRUD / 查询接口未显式要求 `AuthenticatedUser` | P0 | `src/web/routes/widget.rs` 的 `get_widget()`、`update_widget()`、`delete_widget()`、`get_room_widgets()`、`get_widget_config()`、`get_widget_session()`、`get_widget_sessions()`、`terminate_widget_session()` 等 |
| SEC-02 | Widget | 即便接口补认证，当前仍缺少“仅 room 成员/创建者/授权者可读写”的对象级鉴权闭环 | P0 | 路由侧大多直接调用 `widget_service`，未在 handler 层做用户-房间-资源校验 |
| SEC-03 | Media | `PUT /upload/{server_name}/{media_id}` 忽略路径参数，无法保证具名上传语义 | P1 | `upload_media_with_id()` 将路径参数丢弃 |

## 4. 契约文档映射

| 契约文档 | 记录问题 | 对应整改项 |
| --- | --- | --- |
| `sync.md` | Sliding Sync 房间字段缺失、`timeout` 契约未兑现、过滤字段未完全生效 | `SS-01` `SS-02` `SS-03` `SS-04` |
| `room.md` | 12 个房间扩展端点为已挂载占位接口，返回 `M_UNRECOGNIZED` | `RM-01` `RM-02` `MU-01` |
| `voice.md` | `convert` / `optimize` 为模拟成功 | `VC-01` `VC-02` |
| `media.md` | 具名上传忽略 `server_name/media_id` | `SEC-03` |
| `widget.md` | session 创建要求 body 重复传 `widget_id`；部分接口按实际为公开 | `SEC-01` `SEC-02` `WG-01` |
| `CHANGELOG.md` | 已记录房间占位接口、Sync/Sliding Sync 已修复边界与剩余缺口迁移情况 | 作为本轮修复后的清理记录入口 |

## 5. 优先级与影响面

### P0

| ID | 问题 | 影响面 | 处理策略 |
| --- | --- | --- | --- |
| SEC-01 | Widget 路由未认证 | 任意未登录访问潜在读取/修改 widget 与 session 资源 | 立即补认证并加回归测试 |
| SEC-02 | Widget 缺少对象级鉴权 | 已登录非授权用户可能越权访问 room widget 资源 | 在现有 service 上补最小鉴权判断 |
| MU-01 | Room 占位接口持续返回 `M_UNRECOGNIZED` | 前端契约不稳定，回归时误判成功挂载接口 | 标准接口实现，私有无用接口下线 |

### P1

| ID | 问题 | 影响面 | 处理策略 |
| --- | --- | --- | --- |
| SS-01 | Sliding Sync 房间字段缺失 | 多客户端房间摘要、邀请态渲染不完整 | 补 room payload 组装 |
| SS-02 | Sliding Sync 忽略 timeout | 高并发轮询放大性能压力 | 复用 `/sync` 等待模型实现短长轮询 |
| SS-04 | Sliding Sync 过滤条件不完整 | SDK 过滤结果与契约不一致 | 在 storage 查询层增量下推 |
| SEC-03 | Media 具名上传语义缺失 | 内容寻址、幂等导入、测试契约不稳定 | 最小改动支持指定 ID 落盘 |
| VC-01 / VC-02 | Voice 模拟成功 | 业务误判转码成功，后续链路空内容 | 改为真实实现或显式“不支持”策略 |

### P2

| ID | 问题 | 影响面 | 处理策略 |
| --- | --- | --- | --- |
| SS-03 | 多 range `ops` 未增量化 | 大列表同步效率差，但可用性尚存 | 在现有 diff 算法上扩展到多窗口 |
| TP-01 | Thirdparty 空实现 | bridge/目录类能力不完整 | 先补最小可用查询，后续再扩协议 |
| WG-01 | create session 要求 body 重复 `widget_id` | SDK 适配噪音与重复参数 | 以 path 为准，body 字段改为可选或删除 |

## 6. 最小改动修复方案

### 6.1 Sliding Sync

#### SS-01 房间字段补齐

- 修改点：
  - `src/services/sliding_sync_service.rs`
  - 如有必要，补少量查询辅助方法到 `member_storage` / `event_storage`
- 修复步骤：
  - 在 `build_room_json()` 中补 `heroes`、`joined_count`、`invited_count`、`invite_state`。
  - 优先复用现有 `state_events`、成员表与邀请态事件，不新增独立聚合表。
  - `heroes` 先按 Synapse 常见语义，基于房间成员摘要挑选除当前用户外的有限用户列表。
  - `invite_state` 仅在邀请态房间填充，普通 join 房间返回空或省略。
- 测试：
  - 在 `tests/integration/api_sliding_sync_contract_tests.rs` 新增房间摘要字段断言。
  - 增加邀请房间场景，验证 `invite_state` 与 `invited_count`。

#### SS-02 timeout / clientTimeout 真正生效

- 修改点：
  - `src/web/routes/sliding_sync.rs`
  - `src/services/sliding_sync_service.rs`
- 修复步骤：
  - 保持 `SlidingSyncRequest.timeout` 为服务端等待时长。
  - `clientTimeout` 若继续保留，仅作为兼容字段透传到等待上限裁剪，不改变服务端主逻辑。
  - 复用现有 `/sync` 的等待模型、缓存或事件推进条件，不新建第二套轮询框架。
  - 为 `timeout = 0` 保留立即返回语义。
- 测试：
  - 补“无新数据时短等待后返回”的集成测试。
  - 补“等待期间有新事件写入则提前返回”的集成测试。

#### SS-03 多 range 增量 ops

- 修改点：
  - `src/services/sliding_sync_service.rs`
- 修复步骤：
  - 将 `build_incremental_ops()` 从“单窗口 diff”扩展为“按 range 分片 diff”。
  - 保持已有单 range 逻辑不回归，先抽出单 range diff helper，再在外层聚合多 range 结果。
  - 若某个 range 无法安全 diff，仅回退该 range 为 `SYNC`，避免整列表降级。
- 测试：
  - 补多 range 初次请求 + 增量请求场景。
  - 补某一 range 插入、另一 range 删除的混合场景。

#### SS-04 过滤字段补齐

- 修改点：
  - `src/storage/sliding_sync.rs`
  - 如缺少底层字段，再最小补 schema / materialization
- 修复步骤：
  - 在 `push_room_filters()` 增加 `room_types`、`not_room_types`、`tags`、`not_tags`。
  - 优先复用已有房间 state/account data 持久化表，不新增大而全的缓存表。
  - 如果 tag 当前无持久化来源，仅补“当前用户房间 account_data 的 tag 查询”所需最小链路。
- 测试：
  - 新增每类 filter 的正反例。
  - 增加组合过滤测试，确认与 `is_dm` / `room_name_like` 叠加时结果正确。

### 6.2 Room 占位接口

#### RM-01 保留并实现 `initialSync`

- 原因：
  - 该接口属于标准 Matrix 房间接口，虽然已过时，但不应长期以 `M_UNRECOGNIZED` 占位。
- 修改点：
  - `src/web/routes/handlers/room.rs`
  - 复用已有 `get_room_members()`、`get_room_state`、`get_room_messages()` 或 sync 辅助逻辑
- 修复步骤：
  - 用现有房间状态、时间线和成员接口拼装最小 `initialSync` 响应。
  - 不单独引入全新 service；以兼容层方式组装 JSON。
  - 明确文档标注“兼容旧接口，建议使用 `/sync`”。
- 测试：
  - 在 `tests/integration/api_room_placeholder_contract_tests.rs` 中把 `initialSync` 从 `M_UNRECOGNIZED` 断言改为结构断言。

#### RM-02 清理无业务价值占位接口

- 建议删除对象：
  - `fragments`
  - `service_types`
  - `event_perspective`
  - `reduced_events`
  - `rendered`
  - `translate`
  - `convert`
  - `vault_data` `GET/PUT`
  - `external_ids`
  - `device`
- 删除原则：
  - 无 Synapse 标准基线。
  - 无契约方真实消费。
  - 当前仅制造“接口已挂载但不可用”的假象。
- 修改点：
  - 从路由装配处取消挂载。
  - 删除对应 handler 与死代码。
  - 同步更新 `room.md`、`CHANGELOG.md`、相关 placeholder tests。
- 清理记录：
  - 在 `CHANGELOG.md` 增补“移除无实际用途房间占位接口”条目。
  - 在修复 PR 描述中附被删除路由清单、原因、替代路径和回滚方式。
- 测试：
  - 现有 `api_room_placeholder_contract_tests.rs` 调整为：
    - `initialSync` 断言功能可用。
    - 已删除私有占位路由断言为 `404` 或不再在契约中暴露。

### 6.3 Voice

#### VC-01 / VC-02 转码与优化

- 建议优先级：
  - 若近期没有真实 FFmpeg 接入计划，先不要继续返回“成功但结果为空”。
- 最小改动方案：
  - 方案 A：接入现有 `voice_service`，把 `convert` / `optimize` 改成真正的异步任务入口。
  - 方案 B：若暂不落地转码，改为显式 `501` / `M_NOT_SUPPORTED` 风格错误，并在文档中标注未启用条件。
- 不建议：
  - 继续返回 `200 success + null content`。
- 测试：
  - 若走方案 A，新增成功产物断言、格式校验、失败路径回滚测试。
  - 若走方案 B，把 `voice_routes_tests.rs` 从“成功”断言改为“明确不支持”断言，并同步更新 `voice.md`。

### 6.4 Media

#### SEC-03 具名上传真正使用路径参数

- 修改点：
  - `src/web/routes/media.rs`
  - `media_service` 底层存储接口
- 修复步骤：
  - `upload_media_with_id()` 传递 `server_name` / `media_id` 到公共上传逻辑。
  - 增加服务器名校验，拒绝伪造本机外 server name。
  - 若目标 `media_id` 已存在，按规范返回冲突或幂等覆盖策略，但必须固定一种行为并写入契约。
- 测试：
  - 在 `tests/integration/api_media_routes_tests.rs` 中补：
    - `PUT` 后下载路径必须命中指定 `media_id`。
    - 重复 `media_id` 的冲突或幂等行为。

### 6.5 Thirdparty

#### TP-01 location / user 最小可用实现

- 修改点：
  - `src/web/routes/thirdparty.rs`
  - 如已有 bridge/provider 存储则复用，没有则先做受限静态/配置驱动实现
- 修复步骤：
  - 保留 `protocol` 白名单校验。
  - 对 `search`、`alias`、`userid`、`nickname` 提供最小过滤能力。
  - 未配置任何 provider 时，返回空列表可以保留，但需区分“协议不支持”和“无匹配结果”。
- 测试：
  - 协议存在但无数据时返回 `200 []`。
  - 协议不存在返回 `404`。
  - 有样例数据时按查询参数命中过滤结果。

### 6.6 Widget

#### SEC-01 / SEC-02 补认证与对象级鉴权

- 修改点：
  - `src/web/routes/widget.rs`
  - 必要时在 `widget_service` 增加 `assert_widget_access(user_id, widget_id, action)` 辅助方法
- 修复步骤：
  - 给所有读写路由注入 `AuthenticatedUser`。
  - 基于 `widget.room_id` 与房间成员关系、创建者、权限记录做最小对象鉴权。
  - `session` 相关接口也必须校验“session 属于当前用户可访问的 widget”。
  - 保持 `capabilities` / `send` 现有鉴权逻辑不回归。
- 测试：
  - `tests/integration/api_widget_tests.rs` 新增：
    - 未登录访问返回 `401`。
    - 非 room 成员访问他人 widget 返回 `403`。
    - 创建者或房间成员访问成功。

#### WG-01 session 创建去除 body 中重复 `widget_id`

- 修改点：
  - `src/web/routes/widget.rs`
  - request DTO
- 修复步骤：
  - 以 path 中 `widget_id` 为单一真实来源。
  - body 中若保留 `widget_id`，仅用于兼容校验，随后废弃。
- 测试：
  - `POST /widgets/{widget_id}/sessions` 仅传 `device_id` 也应成功。
  - 若 body 中传不同 `widget_id`，返回 `400` 明确错误。

## 7. 测试落地矩阵

| 修复项 | 单元测试 | 集成测试 | 回归重点 |
| --- | --- | --- | --- |
| SS-01 | `sliding_sync_service` payload 组装 | `api_sliding_sync_contract_tests.rs` | 邀请房间、heroes 生成 |
| SS-02 | 等待条件判定 helper | `api_sliding_sync_contract_tests.rs` | timeout=0、提前唤醒 |
| SS-03 | 多 range diff helper | `api_sliding_sync_contract_tests.rs` | 混合 INSERT/DELETE/SYNC |
| SS-04 | `SlidingSyncStorage::push_room_filters` | `api_sliding_sync_contract_tests.rs` | 组合过滤 |
| RM-01 | 可选提取 JSON 组装 helper | `api_room_placeholder_contract_tests.rs` | `initialSync` 结构完整性 |
| RM-02 | 无需新增单测 | `api_room_placeholder_contract_tests.rs` | 已删路由不可访问 |
| VC-01 / VC-02 | `voice_service` 或错误分支 | `voice_routes_tests.rs` | 不再出现“成功但 null” |
| SEC-03 | 媒体 ID 落盘逻辑 | `api_media_routes_tests.rs` | 指定 ID 下载一致性 |
| TP-01 | 协议查询 helper | 新增 `api_protocol_alignment_tests.rs` 或专用 thirdparty 测试 | 协议存在/不存在/命中 |
| SEC-01 / SEC-02 / WG-01 | `widget_service` 鉴权 helper | `api_widget_tests.rs` | 401/403/200 分层 |

## 8. 交付物清单

### 必交付

- 问题跟踪表：本文件第 3 至第 7 节可直接作为初始版本。
- 优化后的代码：按模块拆分提交，建议 `widget`、`room placeholders`、`sliding sync`、`media/voice/thirdparty` 分批提交。
- 测试报告：
  - `cargo test --test integration ...`
  - `cargo test --test unit ...`
  - 必要时补 `nextest` 或性能 smoke。
- 部署验证记录：
  - 修复项对应接口请求样例。
  - 修复前后返回差异。
  - 关键日志截图或命令输出。
- 回滚方案：
  - 路由删除类改动：保留一版兼容开关或可逆提交。
  - schema 改动：配套 `.undo.sql`。
  - 行为变更：保留契约文档版本记录与客户端兼容说明。

### 建议提交批次

- 批次 1：`widget` 安全修复。
- 批次 2：`initialSync` 实现 + 房间占位路由清理。
- 批次 3：Sliding Sync 字段 / timeout / filters。
- 批次 4：media 具名上传。
- 批次 5：voice 与 thirdparty。

## 9. 部署验证模板

### 验证步骤

1. 启动修复后的服务与数据库。
2. 针对每个修复项执行契约请求样例。
3. 校验不再出现 `M_UNRECOGNIZED`。
4. 对比 `matrix-js-sdk/docs/api-contract` 中记录的响应字段。
5. 跑对应单元测试、集成测试与最小手工冒烟。

### 重点验证清单

- Sliding Sync：
  - 多次增量请求是否返回稳定 `pos` 与 `ops`
  - `timeout` 是否有效
  - `heroes` / `joined_count` / `invite_state` 是否齐全
- Room：
  - `initialSync` 可用
  - 已删除占位路由不再误导客户端
- Widget：
  - 未认证为 `401`
  - 越权访问为 `403`
- Media：
  - `PUT /upload/{server_name}/{media_id}` 下载内容与指定 ID 一致
- Voice：
  - 不再出现“成功但空内容”

## 10. 回滚方案

- 代码回滚：
  - 每一批修复独立提交，出现问题可按批次回滚。
- 路由清理回滚：
  - 若客户端仍依赖旧占位路径，可临时恢复路由但返回明确 `404/410` 风格错误，不恢复 `M_UNRECOGNIZED`。
- 数据回滚：
  - 若新增 schema，必须同步提供 `.undo.sql`。
- 行为回滚：
  - 对 `voice`、`widget`、`media` 这类响应语义变化较大的接口，发布时保留变更说明。

## 11. 最终验收标准

- 所有契约内保留接口不再返回 `M_UNRECOGNIZED`。
- 无实际用途的占位接口已从路由和代码中删除，并在 `CHANGELOG.md` 留存清理记录。
- Sliding Sync 响应字段与 `sync.md` 契约一致，至少补齐当前缺失字段与 timeout 行为。
- `widget` 路由完成认证与对象级鉴权闭环。
- `media` 具名上传语义与契约一致。
- `voice` 不再返回“成功但空内容”的伪成功响应。
- 每项修复均有对应单元测试或集成测试，且测试报告可复现。
- 部署验证与回滚步骤完整可执行。

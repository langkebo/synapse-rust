# synapse-rust 安全审计与代码质量改进建议

更新时间: 2026-04-14 (v2)

本文档基于本轮对 `synapse-rust` 代码的再次复核整理，目标不是重复罗列问题，而是把结论沉淀为一份可直接指导修复排期、架构整改和回归验证的执行文档。

## 1. 执行摘要

### 1.1 本轮代码复核结论

以下问题已通过代码直接确认，且对安全边界或协议正确性有实质影响：

| 编号 | 风险 | 复核结论 | 核心影响 |
| --- | --- | --- | --- |
| V-01 | Critical | ✅ 已修复 | `room_key_distribution` 已添加房间成员校验 |
| V-02 | Critical | ✅ 已修复 | `m.room.power_levels` 已接入真实解析，动态阈值全面覆盖 |
| V-03 | Critical | ✅ 已修复 | 联邦 TLS 证书校验已恢复，`danger_accept_invalid_certs` 已移除 |
| V-04 | Critical | ✅ 已修复 | 关键状态事件已按类型收紧授权，`get_required_state_event_power_level` 优先从 `events` 键读取 |
| V-05 | Critical | ✅ 已修复 | `m.login.dummy` 已被拒绝，仅允许 `m.login.password` |
| V-06 | Critical | ✅ 已修复 | 密钥轮换接口已改用 `AdminUser` 提取器 |
| V-07 | High | ✅ 已修复 | 设备列表按共享房间过滤，`last_seen_ip` 全面移除（含 SQL SELECT 和序列化） |
| V-08 | High | ✅ 已修复 | `join_room()` 已校验 `join_rule`，`create_room()` visibility 与 join_rule 一致性已修复 |
| V-09 | High | ✅ 已修复 | 仅 `super_admin` 可修改管理员权限 |
| V-10 | High | ✅ 已修复 | 联邦入站签名验证已补齐 |
| V-11 | High | ✅ 已修复 | `register_internal` 管理员参数已收敛 |
| V-12 | High | ✅ 已修复 | 设备归属验证已添加 |
| V-13 | Medium | ✅ 已修复 | `refresh_token()` 已检查停用状态 |
| V-14 | Medium | ✅ 已修复 | Access Token 已改为哈希存储 |
| V-15 | Medium | ✅ 已修复 | `invite_user()`/`ban_user()` 已验证操作者权限，动态阈值全面覆盖 |
| V-16 | Medium | ✅ 已修复 | `AuthorizationService` 已显式覆盖所有资源类型 |

### 1.2 v2 新增修复项

| 编号 | 风险 | 修复说明 |
| --- | --- | --- |
| V-17 | Medium | `can_ban_user()` 从硬编码 50 改为动态读取 `ban` 阈值 |
| V-18 | Medium | `verify_room_moderator()` 从硬编码 50 改为动态读取 `state_default` 阈值 |
| V-19 | Medium | `get_required_state_event_power_level()` 优先从 `events` 键读取，硬编码 100 仅作兜底 |
| V-20 | Medium | 联邦 `DeviceInfo.last_seen_ip` 添加 `#[serde(skip)]` 防止序列化泄露 |
| V-21 | Low | SQL SELECT 不再查询 `last_seen_ip`，从数据源头消除泄露 |
| V-22 | Low | `create_room()` 当 `visibility = "public"` 时自动设置 `join_rule = "public"` |
| V-23 | Low | `hash_token()` 从 3 处重复实现合并到 `common/crypto.rs` |
| V-24 | Low | `generate_token()`/`generate_family_id()` 合并到 `common::crypto::generate_token` |
| V-25 | Low | 删除 4 个死文件: `search.rs`, `thread.rs`, `query_cache.rs`, `slow_query_logger.rs` |
| V-26 | Low | `filter_users_with_shared_rooms()` 从 2 处重复实现合并到 `response_helpers.rs` |

### 1.2 风险分层建议

- P0: 立即阻断可直接导致账号接管、E2EE 失效、联邦中间人攻击、房间权限全面失守的问题。
- P1: 修复房间状态事件授权、加入规则、联邦入站签名校验等“协议核心链路”问题。
- P2: 收敛数据泄露、角色提升、设备归属校验和令牌安全问题。
- P3: 清理桩实现、统一授权框架、补齐关键占位功能和回归测试矩阵。

## 2. 已复核的高优先级问题

### V-01 E2EE 房间密钥泄露

- 位置: `src/web/routes/e2ee_routes.rs`
- 现象: `room_key_distribution` 仅校验房间是否存在，然后直接返回 `session_key`。
- 问题本质: 缺少“请求用户是否为房间成员”的校验。
- 影响: 任意已认证用户只要知道 `room_id` 就可能获取 Megolm 会话密钥，直接破坏端到端加密。
- 结论: 这是应立即修复的阻断级漏洞。

### V-02 Power Level 未按状态事件解析

- 位置: `src/auth/mod.rs`、`src/auth/authorization.rs`
- 原始现象: `get_user_power_level()` 只对房间创建者返回 `100`，其他成员统一返回 `0`。
- 问题本质: 当时完全未读取 `m.room.power_levels` 状态事件，也未解析 `users`、`users_default` 等关键字段。
- 修复状态: 已完成。当前实现会优先读取房间最新 `m.room.power_levels` 状态事件，依次解析 `users[user_id]`、`users_default`，仅在缺失状态事件时才回退到创建者 `100` / 普通成员 `0`。
- 回归验证: 已新增“显式授予 100 权限的非创建者可以执行 kick”集成测试，确保授权结果不再被硬编码默认值覆盖。
- 结论: V-02 已从“协议核心失效”降为“已修复并需继续扩展事件级授权”的后续整改项。

### V-03 联邦 TLS 校验被禁用

- 位置: `src/federation/client.rs`
- 现象: 两处使用 `danger_accept_invalid_certs(true)`。
- 问题本质: 主动关闭证书校验，相当于把联邦通信暴露给中间人攻击。
- 影响: 攻击者可拦截和篡改联邦请求、服务器密钥交换和事件传输。
- 结论: 在生产环境属于不可接受配置，应立即移除并改为显式可控的调试开关或白名单模式。

### V-04 状态事件发送无类型授权

- 位置: `src/services/room_service.rs`
- 现象: `send_message()` 仅基于成员身份执行发送，不区分普通消息事件与关键状态事件。
- 问题本质: 缺少针对 `m.room.power_levels`、`m.room.join_rules`、`m.room.history_visibility` 等状态事件的单独授权校验。
- 影响: 普通成员可能伪造关键状态事件，从而抬升自身权限或改变房间行为。
- 修复进展: 已在 `src/web/routes/handlers/room.rs` 对状态事件写入主路径增加统一入口校验，并切换为按房间 `m.room.power_levels` 的 `events[event_type]` / `state_default` 动态判定所需权限；普通成员现在不能再直接写入 `m.room.power_levels`、`m.room.join_rules`，也不能继续利用未列入旧白名单的 `m.room.topic` 等状态事件绕过授权；此前未做成员校验的空 `state_key` 写入路径也已补上成员检查。联邦 `/_matrix/federation/v1/send/{txn_id}` 入口也已补上 `auth.origin` 与 body/pdu `origin` 一致性校验，并对非 `m.room.member` 状态事件复用同一套动态 power-level 授权，减少远端服务器直接灌入关键状态事件的风险。
- 当前缺口: 这仍不是完整的 Matrix `event_auth`，更多状态事件类型、内容级别校验，以及 service / 联邦入口的一致性授权仍需继续收口；联邦入站目前仍偏向“最小防线”，尚未实现完整的 auth chain / state resolution 级验证。
- 回归验证: 已新增“普通成员写入 `m.room.join_rules` / `m.room.power_levels` / `m.room.topic` 返回 `403`”以及“联邦 `send_transaction` 中普通成员状态事件被拒绝”的集成测试。
- 结论: V-04 已从“完全无事件类型授权”推进到“客户端主路径和联邦事务主路径都已收紧”的阶段，但仍属于需持续整改的协议核心问题。

### V-05 修改密码支持 `m.login.dummy`

- 位置: `src/web/routes/account_compat.rs`
- 现象: `change_password_uia` 在 `auth.type == "m.login.dummy"` 时直接修改密码。
- 问题本质: 将占位型 UIA 流程当成了高风险账户操作的真实认证因子。
- 影响: 只要攻击者拿到有效 access token，即可不验证旧密码直接改密并接管账户。
- 结论: 应只允许 `m.login.password` 或更强 UIA 流程用于密码修改。

### V-06 密钥轮换接口缺少管理员约束

- 位置: `src/web/routes/key_rotation.rs`
- 现象: `rotate_keys()`、`configure_key_rotation()` 仅要求 `AuthenticatedUser`。
- 问题本质: 管理级操作暴露给普通用户。
- 影响: 任意用户可写入 `key_rotation_history` 或伪造配置修改，破坏运维边界与审计可信度。
- 结论: 应改为管理员接口，至少要求 `is_admin` 或更细粒度角色。

### V-07 设备列表接口可枚举任意用户设备

- 位置: `src/web/routes/device.rs`、`src/web/routes/e2ee_routes.rs`
- 现象: 请求体中的 `users` 可包含任意目标用户，且调用者上下文未用于范围收敛。
- 问题本质: 缺少“共享房间”或“目标即本人”约束。
- 影响: 设备显示名、最后活跃时间、最后登录 IP 等元数据可被任意查询。
- 结论: 这是典型 IDOR / 隐私泄露问题。

### V-08 加入房间未验证 `join_rule`

- 位置: `src/services/room_service.rs`
- 现象: `join_room()` 仅检查房间和用户存在，然后直接写入 `join` membership。
- 问题本质: 忽略 `m.room.join_rules` 和邀请状态。
- 影响: 私有房、invite-only 房间可被直接加入。
- 修复状态: 已完成。当前实现会优先读取最新 `m.room.join_rules` 状态事件，查不到时再回退房间元数据；`public` 房间允许直接加入，非 `public` 房间必须已有 `invite` membership，`ban` 状态则显式拒绝。
- 回归验证: 已新增“invite-only 房间中，未受邀用户调用 `join` 返回 `403`”的集成测试；同时现有受邀后加入的回归链路继续通过。
- 当前边界: 客户端主加入路径和联邦 `send_join` / `send_join_v2` 现已统一到基础的 `public` / `invite` / `ban` 判定；`knock` / `restricted` 等更复杂的 Matrix 加入语义仍未完整实现，但至少不会再以“无条件直接加入”的方式绕过房间边界。
- 结论: V-08 已从“可直接绕过房间边界”降为“基础准入已修复，复杂协议语义待补齐”。

## 3. 本轮新增确认的中风险问题

### V-15 `invite_user()` / `ban_user()` 不校验操作者权限

- 位置: `src/services/room_service.rs`
- 现象: 只校验房间与目标用户存在，不校验发起者是否具备对应 power level。
- 影响: 任何可触达该 service 的调用链都可能让普通成员执行邀请或封禁。
- 修复进展: 已将 `AuthService` 注入 `RoomService`，`invite_user()` / `ban_user()` 现在会在 service 层自行读取操作者身份并执行 `can_invite_user()` / `can_ban_user()` 校验；客户端邀请路由也补上了统一授权前置检查，避免“仅靠路由层约束”的脆弱边界。
- 回归验证: 已新增“非房间成员调用 `/rooms/{roomId}/invite` 返回 `403`”的集成测试，同时保留现有正常邀请成功链路。
- 结论: 房间邀请/封禁主调用链已从“service 可直接误用”收敛为“service 自带权限校验”；后续仍可继续扩展到更多旁路调用者的统一复用。

### V-16 统一授权服务存在静默放行分支

- 位置: `src/auth/authorization.rs`
- 现象: 历史上 `check_resource_access()` 对 `Event` / `AccountData` 缺少实质性约束，容易形成“调用了统一授权但实际上仍被放行”的假安全感。
- 修复进展: `AccountData` 现已按“仅本人可读写”处理；`Event` 的通用 `Write` / `Redact` 入口也已改为安全默认拒绝，只有管理员可直接通过这条泛化授权路径修改事件，避免未来接线时误把该门面当作可直接放行的写权限。
- 当前边界: 具体的房间事件写入、redaction、kick/ban 等仍主要依赖更细粒度的房间 power-level 校验；统一授权服务后续仍建议继续吸收这些专用规则，减少重复实现。

### 旧版桩路由与完整实现并存

- 位置: `src/web/routes/handlers/auth.rs`、`src/web/routes/handlers/user.rs`
- 现象: 文件中仍包含 `login/register/logout/logout_all` 与 profile 相关桩实现，返回硬编码值或 `not implemented`。
- 风险: 一旦路由接线错误、模块重构或回归时引用旧处理器，将导致认证和用户资料接口行为漂移。
- 结论: 该问题更偏架构卫生，但对安全边界的长期稳定性影响很大。

## 4. 待继续复核或补证的问题

以下问题在原始审计清单中风险较高，建议在下一轮中继续做代码级确认并补充 PoC 或回归用例：

- V-09: 低权限管理员可修改 `is_admin`，存在角色提升风险。已限制为仅 `super_admin` 可修改 `is_admin` / `user_type` 等管理员权限字段，并补充回归测试覆盖 `set_admin` 与 `v2 users` 更新接口。
- V-10: 联邦入站签名验证缺失或不完整。已补齐请求级 `X-Matrix` 验签后的 `origin` 透传，并在 `send_join/send_leave` 强制校验 `origin`、`sender` 域、`room_id`、`event_id`、`state_key` 与 membership 一致性；`send_join` / `send_join_v2` 也已对齐 `join_rule` / invite / ban 判定，不再允许未受邀用户通过联邦 join 直接进入 invite-only 房间；本轮继续补上 `make_join` / `make_leave` 的 `auth.origin` 与路径 `user_id` 域绑定，并收紧 `knock` / `thirdparty/invite` / `invite` / `invite_v2` 以及 `exchange_third_party_invite` 的 sender 域、`origin`、路径与 membership 事件一致性，防止已验签服务器替其他域用户伪造 membership 相关请求；同时对 `get_state` / `get_state_ids` / `get_event_auth` / `get_room_auth` / `get_room_members` / `get_joined_room_members` / `get_event` / `get_room_event` / `get_missing_events` / `timestamp_to_event` / `backfill` 增加“请求服务器在房间内有 joined 成员”校验，并将 `get_room_event`、`hierarchy` 从公开路由移入联邦鉴权链；联邦 `get_event` / `get_room_event` 现对齐事件检索响应外壳，返回 `origin`、`origin_server_ts` 与单元素 `pdus`，不再直接回裸事件对象；`get_state` / `get_state_ids` / `backfill` 现也已进一步对齐规范外壳和最小披露边界，其中 `get_state` 返回 `pdus` + `auth_chain`、`get_state_ids` 返回 `pdu_ids` + `auth_chain_ids`，`backfill` 改为按查询串解析重复 `v=` 与 `limit`，并稳定排序事件输出，不再携带伪造 `prev_events`、非规范 `limit` 或多余房间内部元数据；联邦 `hierarchy` 现进一步复用真实的 space hierarchy 分页结果，返回 `rooms` / `next_batch` 而不是旧的占位 `children` / `public` 顶层摘要；`get_joining_rules` 也改为真实返回 `m.room.join_rules` 状态中的 `join_rule` / `allow`，且仅对 `public` 房间向未参与服务器开放；`query/directory/room` 对私有房间新增共享房间前置条件，并进一步裁剪为仅返回 `room_id` 与 `servers`，不再附带 `name`、`topic`、`guest_can_join`、`world_readable` 等额外房间摘要；`query/directory` 仅回答本域 alias，且私有房间 alias 仅对已参与服务器开放，`query/profile` 仅允许回答本域用户资料，并对齐规范支持 `?user_id=...&field=...` 形态，仅返回 `displayname` / `avatar_url` 请求字段，不再向远端附带 `user_id`；联邦 `publicRooms` 的 `GET` / `POST` 现统一裁剪为公开目录必需字段，不再把 `creator_user_id`、`room_version`、`history_visibility`、`created_ts`、`is_spotlight`、`is_flagged` 等内部房间元数据直接暴露给远端；联邦媒体 `media/download` / `media/thumbnail` 现要求路径 `server_name` 必须为本域，且 thumbnail 查询参数支持字符串数值解析并对尺寸施加上限，防止跨域命名空间混淆和超大缩略图请求放大资源消耗；原先返回伪造成功数据的 `query/destination`、`query/auth` / `event_auth` 已改为显式 `404`，无后端实现的废弃 `groups` 与 `key/clone` 入口已直接移除，避免向远端暴露误导性兼容接口；另外把联邦 `user/devices` 与 `user/keys/*` 收口到规范语义，只暴露设备公钥、签名、`stream_id` 与可选的交叉签名主键，不再向远端返回 `device_display_name`、`last_seen_ip`、`last_seen_ts` 等本地设备元数据，也不再保留非规范 `keys/*`/`keys/upload` 写入口。
- V-11: `register_internal()` 的 `admin` 参数设计存在后门式误用风险。已将 `AuthService` 收敛为显式的普通注册 / 管理员注册入口，移除通用注册接口上的 `admin` 布尔能力，并补充公网注册传入 `admin=true` 仍不会落库为管理员的回归测试。
- V-12: `get_or_create_device_id()` 可能未验证设备归属。登录时若客户端显式提供 `device_id`，现已强制校验该设备是否属于当前用户；跨用户复用他人设备 ID 将返回拒绝，并补充回归测试。
- V-13: refresh token 是否检查停用状态。已确认 `refresh_token()` 在换发新 token 前会查询用户停用状态；停用账号刷新时返回 `M_USER_DEACTIVATED`，并补充生命周期回归测试覆盖。
- V-14: access token 是否明文存储。已改为仅在 `access_tokens` 中持久化 `token_hash`，新增迁移回填旧数据哈希并清空历史明文 `token`；鉴权、撤销、黑名单与 schema 契约均切换为按哈希工作。
- V-17 ~ V-25: 管理员缓存时效、路径匹配、redaction、事件认证链、授权策略一致性、成员计数、级联删除、事件哈希等问题。

建议后续对每个条目补充三类证据：

- 触发路径: 路由到 service 到 storage 的完整调用链。
- 可利用性: 是否能被普通用户或低权限管理员稳定触发。
- 修复闭环: 是否已有对应测试覆盖。

## 5. 占位与未实现功能清单

### 5.1 关键占位功能

| 功能 | 位置 | 当前状态 | 影响 |
| --- | --- | --- | --- |
| E2EE 密钥请求履行 | `key_request/service.rs` | `fulfill_request()` 未实现 | 无法恢复已加密消息 |
| OIDC JWKS | `builtin_oidc_provider.rs` | 使用伪密钥/占位逻辑 | 生产不可用 |
| Power Level 读取 | `src/auth/mod.rs`、`src/auth/authorization.rs` | 已实现 `m.room.power_levels` 基础解析 | 仍需继续扩展到事件级授权模型 |
| 设备二维码验证 | `verification/service.rs` | 关键字段为空 | 跨设备验证不可用 |

### 5.2 管理接口占位实现

以下接口目前更接近“契约占位”而非真实管理功能，应在文档和 API 层明确标记，避免被误当作可用特性：

- `admin/server.rs` 中的 `restart_server`
- `admin/server.rs` 中的 `purge_media_cache`
- `admin/server.rs` 中的 `run_background_updates`
- `admin/server.rs` 中的 `enable_background_updates`
- `admin/server.rs` 中的 `get_experimental_features`
- `admin/server.rs` 中的 `get_backups`
- `src/web/routes/key_rotation.rs` 中的 `configure_key_rotation`

### 5.3 旧版桩实现文件

| 文件 | 状态 |
| --- | --- |
| `src/web/routes/handlers/auth.rs` | ✅ 已删除 |
| `src/web/routes/handlers/user.rs` | ✅ 已删除 |
| `src/web/routes/search.rs` | ✅ 已删除（死包装文件） |
| `src/web/routes/thread.rs` | ✅ 已删除（死包装文件） |
| `src/web/routes/admin/query_cache.rs` | ✅ 已删除（与 cache/query_cache.rs 重复） |
| `src/web/routes/admin/slow_query_logger.rs` | ✅ 已删除（与 storage/performance.rs 重复） |

## 6. 对照 Synapse 的架构改进建议

本项目当前最大的问题，不是“缺少几个 if 判断”，而是协议安全逻辑没有像 Synapse 那样沉淀到稳定、集中的核心链路中。

### 6.1 用事件授权替代散落式路由授权

Synapse 的核心思路是：

- 房间事件授权依赖状态事件和事件认证规则，而不是让每个路由自行拼装权限判断。
- `m.room.power_levels`、`m.room.join_rules`、membership、redaction 等逻辑进入统一事件鉴权链路。
- 即使调用来源变化，只要最终走到事件构建 / 持久化，授权模型仍然一致。

本项目建议：

- 建立 `EventAuthorizationService` 或扩展现有 `AuthorizationService`，作为所有房间状态事件写入前的唯一授权入口。
- 将 `send_message()`、`invite_user()`、`ban_user()`、`join_room()` 等 service 全部切换到统一授权能力，而不是在每个函数中各写一套条件。
- 对 `Event`、`AccountData`、`RoomState` 资源做显式建模，不允许 `_ => {}` 这种静默跳过分支。

### 6.2 把 Power Level 解析做成可复用基础设施

Synapse 的最佳实践不是“在需要的地方顺手读一下 JSON”，而是：

- 统一解析当前状态中的 `m.room.power_levels`。
- 提供 `get_user_power_level()`、`get_required_event_level()`、`can_send_state_event()`、`can_invite()`、`can_ban()` 等稳定接口。
- 所有房间治理动作复用同一份结果。

本项目建议：

- 新增 `RoomPowerLevels` 解析模块，负责读取和缓存房间 power level 状态。
- 明确默认值策略，符合 Matrix 规范。
- 对房间创建者 fallback 逻辑加上“仅当房间尚无 power level 状态时”才生效。

### 6.3 联邦安全默认拒绝，调试开关显式隔离

Synapse 的联邦设计强调两点：

- 入站请求必须验签，不能只做出站签名。
- TLS 校验默认开启，任何降低安全性的配置都应明确、可审计、最好仅用于开发环境。

本项目建议：

- 删除生产代码中的 `danger_accept_invalid_certs(true)`。
- 若确需开发调试支持，使用显式配置项，例如 `allow_invalid_federation_tls_for_tests_only = true`，并限制在 debug / test 构建使用。
- 将入站联邦签名验证纳入单独 middleware 或 federation pipeline，而不是散落在各个 handler。

### 6.4 客户端接口与管理接口彻底隔离

当前项目里客户端 API、兼容 API、管理 API、历史桩实现交织，长期看会带来三类问题：

- 路由冲突或误注册。
- 安全假设不一致。
- 维护者难以判断哪个实现才是“唯一真实入口”。

建议结构：

- `client/` 只放 Matrix Client-Server API。
- `admin/` 只放管理接口，并使用独立认证与角色模型。
- `federation/` 只放 Server-Server API。
- `deprecated/` 或测试专用模块存放历史兼容或占位代码，默认构建不参与路由注册。

### 6.5 用回归测试守住安全边界

建议新增或补齐以下测试：

- 非房间成员请求 `room_key_distribution` 返回 `403`。
- 使用 `m.login.dummy` 修改密码返回 `401/403`。
- invite-only 房间无邀请用户 `join_room` 返回 `403`。
- 普通成员发送 `m.room.power_levels`、`m.room.join_rules` 等状态事件返回 `403`。
- 被授予高权限的非创建者执行 `kick/ban/redact` 时，应按 `m.room.power_levels` 真实结果判定，而不是退回硬编码默认值。
- 普通成员调用密钥轮换管理接口返回 `403`。
- 设备列表查询非共享房间用户返回 `403` 或过滤结果。

## 7. 修复优先级路线图

### P0: 立即修复

| 优先级 | 编号 | 修复项 | 状态 |
| --- | --- | --- | --- |
| 1 | V-01 | `room_key_distribution` 加入房间成员校验 | ✅ 已修复 |
| 2 | V-05 | 修改密码仅允许强认证 UIA 流程 | ✅ 已修复 |
| 3 | V-03 | 恢复联邦 TLS 校验 | ✅ 已修复 |
| 4 | V-06 | 密钥轮换接口增加管理员权限验证 | ✅ 已修复 |

### P1: 协议核心修复

| 优先级 | 编号 | 修复项 | 状态 |
| --- | --- | --- | --- |
| 5 | V-02 | 实现 `m.room.power_levels` 真实解析 | ✅ 已修复 |
| 6 | V-04 | 统一事件类型权限校验 | ✅ 已修复 |
| 7 | V-08 | `join_room` 校验 `join_rule` / invite 状态 | ✅ 已修复 |
| 8 | V-10 | 联邦入站签名验证 | ✅ 已修复 |

### P2: 越权与数据泄露收敛

| 优先级 | 编号 | 修复项 | 状态 |
| --- | --- | --- | --- |
| 9 | V-07 | 设备列表接口按共享房间范围过滤 | ✅ 已修复 |
| 10 | V-09 | 管理员角色等级与目标约束 | ✅ 已修复 |
| 11 | V-12 | 设备归属验证 | ✅ 已修复 |
| 12 | V-13 | refresh token 加入停用状态校验 | ✅ 已修复 |
| 13 | V-14 | access token 哈希存储 | ✅ 已修复 |
| 14 | V-15 | `invite_user` / `ban_user` service 层强制权限验证 | ✅ 已修复 |
| 15 | V-16 | `AuthorizationService` 显式覆盖所有资源类型 | ✅ 已修复 |
| 16 | V-17~V-26 | 动态阈值、IP泄露封堵、代码合并 | ✅ 已修复 |

### P3: 架构治理与代码卫生

| 优先级 | 项目 | 说明 |
| --- | --- | --- |
| 17 | AuthorizationService 死代码清理 | authorization.rs 中 ~735 行代码从未被调用 |
| 18 | 统一授权框架 | 收敛到 service / event auth 中心 |
| 19 | 补齐关键占位功能 | E2EE 密钥请求、OIDC、设备验证等 |
| 20 | 回归测试矩阵 | 为所有高风险缺陷建立负向测试 |
| 21 | storage/models/ 目录清理 | 与专用存储模块重复，仅 3 处引用 |
| 22 | PaginationQuery 重复定义合并 | 应使用 extractors/pagination.rs |
| 23 | register/register_admin 合并 | 两个方法仅差一个 bool 参数 |

## 8. 推荐实施顺序

建议按以下顺序推进，避免“先修外围、后返工核心”：

1. 先修 P0，立即阻断账号接管、E2EE 泄露和联邦 MITM。
2. 再修 P1，把权限模型和事件授权主链路建立起来。
3. 随后处理 P2，把剩余越权、隐私泄露和令牌安全问题收口。
4. 最后做 P3，把桩实现、模块边界和测试基线整理干净。

## 9. 参考资料

以下 Synapse 资料可作为整改时的实现对照：

- `https://github.com/element-hq/synapse/blob/develop/synapse/event_auth.py`
- `https://github.com/element-hq/synapse/blob/develop/synapse/federation/federation_base.py`
- `https://github.com/element-hq/synapse`

建议在具体落地时重点学习三类能力：

- 事件认证和状态解析的集中化设计。
- 联邦入站验签与 TLS 安全默认值。
- 管理接口、客户端接口、联邦接口的职责隔离与测试策略。

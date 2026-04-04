# synapse-rust 占位实现审计表

> 日期：2026-04-04  
> 范围：`synapse-rust` 全项目路由、服务、存储层中对外暴露的占位实现、简化实现、未接通能力  
> 目标：区分“功能已实现但端点未接通 / 功能未实现 / 低优先级可延后”，为后续拆任务和排期提供依据

---

## 一、判定口径

- 不把所有 `Json({})` 都视为占位实现；Matrix 协议中大量 `PUT/DELETE` 空成功响应属于正常行为。
- 只有以下情况计入本表：
  - 返回固定假数据或 placeholder。
  - 路由未复用已经存在的 `service/storage` 能力。
  - 功能表面可用，但关键链路返回空内容或假成功。
  - 代码中存在预留实现，但当前未接通，且会误导能力判断。

---

## 二、审计统计

- 审计项总数：16 条
- 归并后的模块级问题：14 个
- 功能已实现但端点未接通/实现简化：5 条
- 功能未实现：6 条
- 低优先级可延后：5 条

---

## 三、Markdown 审计表

| 模块 | 端点 | 当前行为 | 已有能力 | 结论 | 优先级 | 改造建议 |
| --- | --- | --- | --- | --- | --- | --- |
| 联邦媒体 | `GET /_matrix/federation/v1/media/download/{server_name}/{media_id}`<br>`GET /_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}` | 返回简化 JSON 元数据，不返回真实媒体二进制/缩略图 | 已有 `media_service.download_media()`、`media_service.get_thumbnail()`，普通媒体路由已接通 | 功能已实现但端点未接通 | P0 | 直接复用媒体主链路，返回与客户端媒体下载一致的响应头和二进制内容 |
| 联邦目录别名查询 | `GET /_matrix/federation/v1/query/directory` | 固定拼接 `example.com` 与伪造 `room_id` | 已有 `room_storage.get_room_by_alias()`、客户端目录查询路由 | 功能已实现但端点未接通 | P0 | 改为走真实 alias 解析，返回实际 `room_id` 与可联邦 server 列表 |
| Typing | `PUT /_matrix/client/v3/rooms/{room_id}/typing/{user_id}`<br>`GET /_matrix/client/v3/rooms/{room_id}/typing`<br>`POST /_matrix/client/v3/rooms/typing` | 路由层自己维护 `OnceLock<HashMap>`，绕过服务层 | 容器中已有 `typing_service`，且 service 自带过期清理能力 | 功能已实现但端点未接通 | P0 | 路由统一切到 `typing_service`；后续若需要多实例，再将 service 替换为 Redis/DB 实现 |
| 线程全局未读 | `GET /_matrix/client/v1/threads/unread` | 固定返回空 `threads` | `thread_service.get_unread_threads(user_id, None)` 已支持全局查询 | 功能已实现但端点未接通 | P0 | 将全局未读接口接到现有 `thread_service`，保留房间级与全局级一致的数据结构 |
| 语音消息读取 | `GET /_matrix/client/v3/voice/messages/{message_id}` | 外层接口能返回 200，但内部读取链路固定返回空字节内容 | 语音上传已真实落盘，数据库中已有语音元数据存储 | 功能已实现但端点未接通 | P0 | 补文件回读逻辑，按 `event_id` 反查落盘文件并返回真实音频内容与类型 |
| 联邦远端服务器密钥 | `GET /_matrix/key/v2/query/{server_name}/{key_id}` | 本机以外 server 直接返回 `remote_key_placeholder` | 仅有本机 `server_key/resolve_server_keys`；未见远端 key 拉取或缓存能力 | 功能未实现 | P1 | 新增远端 server key 获取、缓存、过期刷新与失败处理逻辑；短期做不到时应返回明确未实现错误而非 placeholder |
| 联邦 OpenID 用户信息 | `GET /_matrix/federation/v1/openid/userinfo` | 只校验 query 里有 token，固定返回 `sub=user_id:example.com` | 未见真实 token 校验与 user 解析能力 | 功能未实现 | P1 | 接入 access token 校验链路，根据 token 解析真实用户并返回合法 `sub` |
| 语音转写 | `POST /_matrix/client/v3/voice/transcription` | 明确返回 `not yet implemented` placeholder 文本 | 语音上传/统计能力已存在，但无 ASR service | 功能未实现 | P1 | 若近期不上 ASR，改为明确能力关闭；若要上线，新增转写服务接口、异步任务与状态字段 |
| 线程全局列表/创建 | `GET /_matrix/client/v1/threads`<br>`POST /_matrix/client/v1/threads` | 直接返回 not implemented | 房间级线程 `create/list/get/search` 已较完整 | 功能未实现 | P1 | 设计全局聚合查询与创建语义；若协议上不准备支持，应移除入口或显式标为未支持 |
| 线程全局订阅列表 | `GET /_matrix/client/v1/threads/subscribed` | 固定返回空 `threads/subscribed` | 已有订阅存储与单线程订阅能力，但缺少全局枚举方法 | 功能未实现 | P1 | 在 `thread_storage/thread_service` 增加按用户列举订阅线程的方法，并接通全局接口 |
| 推送签名 | APNS / WebPush 下游推送签名生成 | JWT/VAPID 仍生成 `signature_placeholder`，无法形成真实签名 | Provider 框架、HTTP 发送逻辑已存在 | 功能未实现 | P1 | 接入真实 ECDSA 签名流程，补密钥读取、签名失败处理与集成验证 |
| 第三方桥接查询 | `/_matrix/client/v3/thirdparty/protocols`<br>`/_matrix/client/v3/thirdparty/protocol/{protocol}`<br>`/_matrix/client/v3/thirdparty/location*`<br>`/_matrix/client/v3/thirdparty/user*` | 大量硬编码 IRC 示例；v3 查询端点直接返回空数组 | 未见 thirdparty 专门 service/storage | 低优先级可延后 | P2 | 若项目不计划支持桥接，保留最小占位并在文档标明；若计划支持，需单独建 thirdparty 域能力 |
| 联邦第三方邀请交换 | `PUT /_matrix/federation/v1/exchange_third_party_invite/{room_id}` | 只做参数校验后返回 `processed` | 未见 3pid invite 实际处理链路 | 低优先级可延后 | P2 | 未纳入主链路前建议显式返回未支持；后续若要做桥接/3pid，再配套实现完整邀请流 |
| 客户端配置 | `GET /_matrix/client/v1/config/client` | 永远返回空对象 | 未见 client config 专门 service/storage | 低优先级可延后 | P2 | 明确是否需要此接口；若无实际业务用途，可继续空实现但需在能力矩阵中标注 |
| E2EE 设备变更查询死代码 | `DeviceKeyService::get_key_changes()` | service 内部 `left` 固定为 `vec![]` | 外部真实路由已直接用 SQL 返回 `changed/left`，未走该 service | 低优先级可延后 | P2 | 清理死代码或让路由统一走 service，避免后续误判能力缺口 |
| E2EE 密钥请求履约死代码 | `KeyRequestService::fulfill_request()` | 返回 `session_key_placeholder` | 当前未搜到对外路由或调用链接通该方法 | 低优先级可延后 | P2 | 若短期不用，标记未接通并补注释；若准备启用，需打通 Megolm session key 的真实提取与分享 |

---

## 四、建议拆任务顺序

### 4.1 第一批：只差接线，优先止血（P0）

1. 联邦媒体下载/缩略图复用 `media_service`
2. 联邦目录别名查询改走 `room_service/room_storage`
3. Typing 路由统一复用 `typing_service`
4. 线程全局未读改接 `thread_service`
5. 语音消息读取修正文件回读链路

### 4.2 第二批：补真实能力（P1）

1. 联邦远端 server key 查询
2. 联邦 OpenID userinfo
3. 语音转写
4. 线程全局列表/创建/订阅接口
5. APNS / WebPush 真实签名

### 4.3 第三批：能力边界与代码清理（P2）

1. 第三方桥接与 3pid 邀请能力边界说明
2. `config/client` 是否保留空实现的口径统一
3. E2EE 预留/死代码清理，避免后续审计误报

---

## 五、建议的执行原则

- 能立即复用已有 `service/storage` 的项，优先按“少改路由、多复用现有能力”处理。
- 做不到真实实现的接口，不要继续返回假成功；应改成明确的未支持或未实现响应。
- 文档口径要与代码状态一致，避免把“返回了 200”误写成“功能已完成”。


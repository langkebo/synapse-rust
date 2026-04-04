# SDK 全面审核评估检查清单

## API 清单基准

- [x] 已从后端 API 集成测试文件提取所有被测试的 API 端点
- [x] 每个 API 已记录端点路径、HTTP 方法、请求参数、响应字段
- [x] API 已按功能模块分类组织
- [x] 已输出结构化的 API 清单文档

## 功能完整性审核

### 认证与账户模块
- [x] 登录 API (POST /_matrix/client/v3/login) 封装状态已确认
- [x] 注册 API (POST /_matrix/client/v3/register) 封装状态已确认
- [x] 注销 API (POST /_matrix/client/v3/logout) 封装状态已确认
- [x] Token 刷新 API (POST /_matrix/client/v3/refresh) 封装状态已确认
- [x] WhoAmI API (GET /_matrix/client/v3/account/whoami) 封装状态已确认
- [x] 3PID API 封装状态已确认
- [x] OpenID Token API 封装状态已确认

### 房间管理模块
- [x] 创建房间 API (POST /_matrix/client/v3/createRoom) 封装状态已确认
- [x] 加入房间 API (POST /_matrix/client/v3/join/{roomId}) 封装状态已确认
- [x] 离开房间 API (POST /_matrix/client/v3/rooms/{roomId}/leave) 封装状态已确认
- [x] 邀请用户 API (POST /_matrix/client/v3/rooms/{roomId}/invite) 封装状态已确认
- [x] 踢出用户 API (POST /_matrix/client/v3/rooms/{roomId}/kick) 封装状态已确认
- [x] 封禁用户 API (POST /_matrix/client/v3/rooms/{roomId}/ban) 封装状态已确认
- [x] 房间状态 API (GET /_matrix/client/v3/rooms/{roomId}/state) 封装状态已确认
- [x] 房间成员 API (GET /_matrix/client/v3/rooms/{roomId}/members) 封装状态已确认
- [x] 房间别名 API 封装状态已确认
- [x] 公共房间 API (GET /_matrix/client/v3/publicRooms) 封装状态已确认

### 消息与事件模块
- [x] 发送消息 API (PUT /_matrix/client/v3/rooms/{roomId}/send/{eventType}/{txnId}) 封装状态已确认
- [x] 获取消息 API (GET /_matrix/client/v3/rooms/{roomId}/messages) 封装状态已确认
- [x] 事件上下文 API (GET /_matrix/client/v3/rooms/{roomId}/context/{eventId}) 封装状态已确认
- [x] 事件关系 API (GET /_matrix/client/v3/rooms/{roomId}/relations/{eventId}) 封装状态已确认
- [x] 撤回事件 API (PUT /_matrix/client/v3/rooms/{roomId}/redact/{eventId}/{txnId}) 封装状态已确认
- [x] 搜索 API (POST /_matrix/client/v3/search) 封装状态已确认

### 用户资料模块
- [x] 获取用户资料 API (GET /_matrix/client/v3/profile/{userId}) 封装状态已确认
- [x] 设置显示名 API (PUT /_matrix/client/v3/profile/{userId}/displayname) 封装状态已确认
- [x] 设置头像 API (PUT /_matrix/client/v3/profile/{userId}/avatar_url) 封装状态已确认
- [x] 用户目录搜索 API (POST /_matrix/client/v3/user_directory/search) 封装状态已确认

### 媒体模块
- [x] 媒体上传 API (POST /_matrix/media/v3/upload) 封装状态已确认
- [x] 媒体下载 API (GET /_matrix/media/v3/download/{serverName}/{mediaId}) 封装状态已确认
- [x] 媒体缩略图 API (GET /_matrix/media/v3/thumbnail/{serverName}/{mediaId}) 封装状态已确认
- [x] 媒体配置 API (GET /_matrix/media/v3/config) 封装状态已确认

### 设备管理模块
- [x] 列出设备 API (GET /_matrix/client/v3/devices) 封装状态已确认
- [x] 获取设备 API (GET /_matrix/client/v3/devices/{deviceId}) 封装状态已确认
- [x] 更新设备 API (PUT /_matrix/client/v3/devices/{deviceId}) 封装状态已确认
- [x] 删除设备 API (DELETE /_matrix/client/v3/devices/{deviceId}) 封装状态已确认

### E2EE 密钥模块
- [x] 上传密钥 API (POST /_matrix/client/v3/keys/upload) 封装状态已确认
- [x] 查询密钥 API (POST /_matrix/client/v3/keys/query) 封装状态已确认
- [x] 领取密钥 API (POST /_matrix/client/v3/keys/claim) 封装状态已确认
- [x] 密钥变更 API (GET /_matrix/client/v3/keys/changes) 封装状态已确认
- [x] 密钥备份 API 封装状态已确认
- [x] 跨设备签名 API 封装状态已确认

### 管理员 API 模块
- [x] 用户管理 API 封装状态已确认 (GET/PUT/POST /_synapse/admin/v1/users/*)
- [x] 房间管理 API 封装状态已确认 (GET/DELETE /_synapse/admin/v1/rooms/*)
- [x] 服务器统计 API 封装状态已确认 (GET /_synapse/admin/v1/statistics)
- [x] 联邦管理 API 封装状态已确认 (GET /_synapse/admin/v1/federation/*)
- [x] 注册令牌 API 封装状态已确认 (GET /_synapse/admin/v1/registration_tokens)
- [x] 后台更新 API 封装状态已确认 (GET /_synapse/admin/v1/background_updates)
- [x] 事件报告 API 封装状态已确认 (GET /_synapse/admin/v1/event_reports)
- [x] 服务器通知 API 封装状态已确认 (POST /_synapse/admin/v1/send_server_notice)

### Space API 模块
- [x] 创建 Space API 封装状态已确认
- [x] 获取 Space 层级 API (GET /_matrix/client/v3/spaces/{spaceId}/hierarchy) 封装状态已确认
- [x] 获取 Space 子房间 API (GET /_matrix/client/v3/spaces/{spaceId}/children) 封装状态已确认
- [x] 添加子房间 API (PUT /_matrix/client/v3/spaces/{spaceId}/children/{roomId}) 封装状态已确认
- [x] 公共 Space API (GET /_matrix/client/v3/spaces/public) 封装状态已确认
- [x] 用户 Space API (GET /_matrix/client/v3/spaces/user) 封装状态已确认

### Thread API 模块
- [x] 获取线程 API (GET /_matrix/client/v1/rooms/{roomId}/threads) 封装状态已确认
- [x] 获取线程事件 API 封装状态已确认

### DM API 模块
- [x] 创建 DM API (POST /_matrix/client/v3/create_dm) 封装状态已确认
- [x] 获取 DM 列表 API (GET /_matrix/client/v3/direct) 封装状态已确认
- [x] m.direct account data 处理逻辑已验证

### Push API 模块
- [x] 获取推送规则 API (GET /_matrix/client/v3/pushrules) 封装状态已确认
- [x] 设置推送规则 API 封装状态已确认
- [x] 推送器管理 API (POST /_matrix/client/v3/pushers/set) 封装状态已确认
- [x] 通知 API 封装状态已确认

### Presence API 模块
- [x] 获取在线状态 API (GET /_matrix/client/v3/presence/{userId}/status) 封装状态已确认
- [x] 设置在线状态 API (PUT /_matrix/client/v3/presence/{userId}/status) 封装状态已确认
- [x] 在线状态列表 API (GET /_matrix/client/v3/presence/list/{userId}) 封装状态已确认

### 联邦 API 模块
- [x] 联邦版本 API (GET /_matrix/federation/v1/version) 封装状态已确认
- [x] 联邦公钥 API 封装状态已确认
- [x] 联邦事件 API 封装状态已确认

### 其他扩展 API 模块
- [x] 房间摘要 API (GET /_matrix/client/v3/rooms/{roomId}/summary) 封装状态已确认
- [x] 房间统计 API (GET /_matrix/client/v3/rooms/{roomId}/summary/stats) 封装状态已确认
- [x] 好友 API (GET /_matrix/client/v1/friends) 封装状态已确认
- [x] 好友请求 API (POST /_matrix/client/v1/friends/request) 封装状态已确认
- [x] 邀请黑名单 API 封装状态已确认
- [x] 邀请白名单 API 封装状态已确认
- [x] 房间保留策略 API 封装状态已确认
- [x] 阅读标记 API (POST /_matrix/client/v3/rooms/{roomId}/read_markers) 封装状态已确认
- [x] 输入状态 API (PUT /_matrix/client/v3/rooms/{roomId}/typing/{userId}) 封装状态已确认
- [x] VoIP 配置 API (GET /_matrix/client/v3/voip/config) 封装状态已确认
- [x] Well-Known API 封装状态已确认
- [x] 服务器版本 API (GET /_matrix/client/versions) 封装状态已确认
- [x] 能力 API (GET /_matrix/client/v3/capabilities) 封装状态已确认
- [x] 过滤器 API 封装状态已确认
- [x] 账户数据 API 封装状态已确认
- [x] 房间标签 API 封装状态已确认
- [x] 同步 API (GET /_matrix/client/v3/sync) 封装状态已确认
- [x] SendToDevice API 封装状态已确认

## API 封装准确性审核

- [x] URL 路径构造已验证无重复前缀问题
- [x] HTTP 方法使用已验证与后端一致
- [x] 请求参数（query、body、path）已验证与后端一致
- [x] 响应数据解析已验证正确
- [x] 路径参数编码已验证正确

## 类型定义审核

- [x] 每个 API 响应有对应的 TypeScript 接口
- [x] 接口字段与后端响应一致
- [x] 字段类型正确
- [x] 可选字段标记正确
- [x] 缺失字段已识别

## 错误处理机制审核

- [x] SDK 实现了错误分类体系（AuthError, NotFoundError, RetryableError, ApiError）
- [x] 错误被正确传播给调用方
- [x] 不存在吞掉错误返回默认值的情况
- [x] 错误对象包含足够的信息（错误码、HTTP 状态码、原始错误）

## 文档完整性审核

- [x] 每个公开方法有 JSDoc 注释
- [x] 注释包含方法描述、参数说明、返回值说明、异常说明
- [x] 关键方法有使用示例
- [x] 示例代码可运行

## 问题分级与报告

- [x] 发现的问题已按 P0/P1/P2/P3 分级
- [x] 问题清单已按模块整理
- [x] 每个问题有详细信息（位置、描述、影响、建议修复方案）
- [x] 各模块封装覆盖度已统计
- [x] 完整审核报告已生成

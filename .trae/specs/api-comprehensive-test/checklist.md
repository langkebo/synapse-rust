# API 全面系统性测试检查清单

## 测试环境准备

- [ ] 检查服务运行状态
- [ ] 验证测试账户可用性
- [ ] 验证测试房间可用性

## API 模块测试

### 基础服务 API

- [ ] 健康检查端点 (`/health`)
- [ ] 版本信息端点 (`/_matrix/client/v3/versions`)
- [ ] 客户端能力端点 (`/_matrix/client/v3/capabilities`)
- [ ] 服务器发现端点 (`/.well-known/matrix/server`)

### 用户认证 API

- [ ] 登录流程端点 (`/_matrix/client/v3/login`)
- [ ] 注册流程端点 (`/_matrix/client/v3/register`)
- [ ] 令牌刷新端点 (`/_matrix/client/v3/refresh`)
- [ ] 登出端点 (`/_matrix/client/v3/logout`)
- [ ] 当前用户端点 (`/_matrix/client/v3/account/whoami`)

### 账户管理 API

- [ ] 用户资料端点 (`/_matrix/client/v3/profile/{userId}`)
- [ ] 第三方ID端点 (`/_matrix/client/v3/account/3pid`)
- [ ] 账户数据端点 (`/_matrix/client/v3/user/{userId}/account_data`)

### 房间管理 API

- [ ] 创建房间端点 (`/_matrix/client/v3/createRoom`)
- [ ] 加入房间端点 (`/_matrix/client/v3/rooms/{roomId}/join`)
- [ ] 离开房间端点 (`/_matrix/client/v3/rooms/{roomId}/leave`)
- [ ] 房间成员端点 (`/_matrix/client/v3/rooms/{roomId}/members`)
- [ ] 公开房间端点 (`/_matrix/client/v3/publicRooms`)

### 消息发送 API

- [ ] 发送消息端点 (`/_matrix/client/v3/rooms/{roomId}/send/m.room.message/{txnId}`)
- [ ] 获取消息端点 (`/_matrix/client/v3/rooms/{roomId}/messages`)
- [ ] 已读标记端点 (`/_matrix/client/v3/rooms/{roomId}/receipt/m.read/{eventId}`)

### 设备管理 API

- [ ] 设备列表端点 (`/_matrix/client/v3/devices`)
- [ ] 设备删除端点 (`/_matrix/client/v3/devices/{deviceId}`)

### 推送通知 API

- [ ] 推送规则端点 (`/_matrix/client/v3/pushrules`)
- [ ] 通知列表端点 (`/_matrix/client/v3/notifications`)

### E2EE 加密 API

- [ ] 密钥上传端点 (`/_matrix/client/v3/keys/upload`)
- [ ] 密钥查询端点 (`/_matrix/client/v3/keys/query`)
- [ ] 密钥备份端点 (`/_matrix/client/v3/room_keys/version`)

### 媒体服务 API

- [ ] 媒体上传端点 (`/_matrix/media/v3/upload`)
- [ ] 媒体下载端点 (`/_matrix/media/v3/download/{serverName}/{mediaId}`)
- [ ] 媒体配置端点 (`/_matrix/media/v3/config`)

### 好友系统 API

- [ ] 好友列表端点 (`/_matrix/client/v1/friends`)
- [ ] 好友请求端点 (`/_matrix/client/v1/friend/request`)
- [ ] 好友操作端点 (`/_matrix/client/v1/friend/{userId}`)

### 同步 API

- [ ] 同步端点 (`/_matrix/client/v3/sync`)
- [ ] 过滤端点 (`/_matrix/client/v3/user/{userId}/filter`)

### VoIP 服务 API

- [ ] TURN服务器端点 (`/_matrix/client/v3/voip/turnServer`)
- [ ] VoIP配置端点 (`/_matrix/client/v3/voip/config`)

### 搜索服务 API

- [ ] 消息搜索端点 (`/_matrix/client/v3/search`)
- [ ] 用户搜索端点 (`/_matrix/client/v3/user_directory/search`)

### 管理后台 API

- [ ] 服务器版本端点 (`/_synapse/admin/v1/server_version`)
- [ ] 用户管理端点 (`/_synapse/admin/v1/users`)
- [ ] 房间管理端点 (`/_synapse/admin/v1/rooms`)
- [ ] 服务器状态端点 (`/_synapse/admin/v1/server_status`)

### 联邦 API

- [ ] 联邦版本端点 (`/_matrix/federation/v1/version`)
- [ ] 联邦密钥端点 (`/_matrix/key/v2/server`)

### Space 空间 API

- [ ] 公开空间端点 (`/_matrix/client/v1/spaces/public`)

### Thread 线程 API

- [ ] 线程列表端点 (`/_matrix/client/v1/threads`)
- [ ] 线程消息端点 (`/_matrix/client/v1/rooms/{roomId}/threads`)

## 问题修复与报告生成

- [ ] 记录所有发现的问题到 api-error.md
- [ ] 分析问题根因
- [ ] 提出修复方案
- [ ] 验证修复效果

## 最终报告生成

- [ ] 统计测试覆盖率
- [ ] 分析问题类别
- [ ] 生成最终测试报告

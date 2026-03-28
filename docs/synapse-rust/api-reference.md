# synapse-rust API 参考文档

> 生成时间: 2026-03-28
> 代码版本: 基于实际代码扫描

---

## 统计概览

| 项目 | 结果 |
|------|------|
| **API 端点总数** | **293** |
| **HTTP 方法** | GET, POST, PUT, DELETE, PATCH |

---

## API 分类统计

| 分类 | 路径前缀 | 端点数量 |
|------|----------|----------|
| Matrix Client API | `/_matrix/client/v3`, `/_matrix/client/r0`, `/_matrix/client/v1` | ~100 |
| Matrix Federation API | `/_matrix/federation/v1`, `/_matrix/federation/v2` | ~50 |
| Synapse Admin API | `/_synapse/admin/v1`, `/_synapse/admin/v2` | ~80 |
| Synapse Worker API | `/_synapse/worker/v1` | ~15 |
| Custom API | `/`, `/spaces/`, `/connections/` | ~50 |

---

## Matrix Client API

### Client API v3

#### Account

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v3/account/guest` | 获取访客账号信息 |
| GET | `/_matrix/client/v3/capabilities` | 获取客户端能力 |
| POST | `/_matrix/client/v3/createRoom` | 创建房间 |
| POST | `/_matrix/client/v3/create_dm` | 创建直接消息房间 |
| GET | `/_matrix/client/v3/direct` | 获取直接消息房间列表 |
| PUT | `/_matrix/client/v3/direct/{room_id}` | 更新直接消息房间 |
| POST | `/_matrix/client/v3/joined_rooms` | 获取已加入房间列表 |
| GET | `/_matrix/client/v3/my_rooms` | 获取我的房间 |
| POST | `/_matrix/client/v3/register/guest` | 注册访客 |
| POST | `/_matrix/client/v3/refresh` | 刷新令牌 |
| GET | `/_matrix/client/v3/sync` | 同步数据 |
| POST | `/_matrix/client/v3/sync` | 同步数据 (POST) |
| GET | `/_matrix/client/v3/versions` | 获取版本 |
| GET | `/_matrix/client/versions` | 获取客户端版本 |

#### Presence

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_matrix/client/v3/presence/list` | 更新在线状态列表 |

#### Rooms

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v3/rooms/{room_id}` | 获取房间信息 |
| POST | `/_matrix/client/v3/rooms/{room_id}/ban` | 封禁用户 |
| POST | `/_matrix/client/v3/rooms/{room_id}/dm` | 获取 DM 信息 |
| POST | `/_matrix/client/v3/rooms/{room_id}/invite` | 邀请用户 |
| POST | `/_matrix/client/v3/rooms/{room_id}/join` | 加入房间 |
| GET | `/_matrix/client/v3/rooms/{room_id}/joined_members` | 获取成员列表 |
| POST | `/_matrix/client/v3/rooms/{room_id}/kick` | 踢出用户 |
| POST | `/_matrix/client/v3/rooms/{room_id}/leave` | 离开房间 |
| GET | `/_matrix/client/v3/rooms/{room_id}/members` | 获取房间成员 |
| GET | `/_matrix/client/v3/rooms/{room_id}/messages` | 获取房间消息 |
| POST | `/_matrix/client/v3/rooms/{room_id}/report` | 举报房间 |
| GET | `/_matrix/client/v3/rooms/{room_id}/state` | 获取房间状态 |
| POST | `/_matrix/client/v3/rooms/{room_id}/unban` | 解除封禁 |
| POST | `/_matrix/client/v3/rooms/typing` | 发送 typing 通知 |

#### Room Summary

| 方法 | 路径 | 描述 |
|------|------|------|
| DELETE | `/_matrix/client/v3/rooms/{room_id}/summary` | 删除房间摘要 |
| GET | `/_matrix/client/v3/rooms/{room_id}/summary` | 获取房间摘要 |
| POST | `/_matrix/client/v3/rooms/{room_id}/summary` | 创建房间摘要 |
| PUT | `/_matrix/client/v3/rooms/{room_id}/summary` | 更新房间摘要 |
| GET | `/_matrix/client/v3/rooms/{room_id}/summary/members` | 获取摘要成员 |
| POST | `/_matrix/client/v3/rooms/{room_id}/summary/members` | 更新摘要成员 |
| GET | `/_matrix/client/v3/rooms/{room_id}/summary/state` | 获取摘要状态 |
| GET | `/_matrix/client/v3/rooms/{room_id}/summary/stats` | 获取摘要统计 |
| POST | `/_matrix/client/v3/rooms/{room_id}/summary/sync` | 同步摘要 |
| POST | `/_matrix/client/v3/rooms/{room_id}/summary/unread/clear` | 清除未读 |

#### Push

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v3/pushrules/` | 获取推送规则 |
| GET | `/_matrix/client/v3/pushrules/{scope}` | 获取指定范围推送规则 |
| GET | `/_matrix/client/v3/pushrules/{scope}/{kind}` | 获取推送规则详情 |

#### Thirdparty

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v3/thirdparty/user` | 第三方用户查询 |

### Client API r0

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/r0/capabilities` | 获取客户端能力 |
| POST | `/_matrix/client/r0/create_dm` | 创建直接消息房间 |
| GET | `/_matrix/client/r0/direct` | 获取直接消息房间列表 |
| PUT | `/_matrix/client/r0/direct/{room_id}` | 更新直接消息房间 |
| GET | `/_matrix/client/r0/friends/groups` | 获取好友分组 |
| GET | `/_matrix/client/r0/friendships` | 获取好友列表 |
| POST | `/_matrix/client/r0/friendships` | 添加好友 |
| GET | `/_matrix/client/r0/push/devices` | 获取推送设备 |
| POST | `/_matrix/client/r0/push/devices` | 注册推送设备 |
| GET | `/_matrix/client/r0/push/rules` | 获取推送规则 |
| POST | `/_matrix/client/r0/push/rules` | 设置推送规则 |
| POST | `/_matrix/client/r0/push/send` | 发送推送 |
| GET | `/_matrix/client/r0/version` | 获取版本 |

### Client API v1

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v1/friends` | 获取好友列表 |
| POST | `/_matrix/client/v1/friends` | 添加好友 |
| GET | `/_matrix/client/v1/friends/groups` | 获取好友分组 |
| GET | `/_matrix/client/v1/threads` | 获取线程列表 |
| POST | `/_matrix/client/v1/threads` | 创建线程 |
| GET | `/_matrix/client/v1/user/burn/stats` | 获取燃烧统计 |
| POST | `/_matrix/client/v1/widgets` | 创建小组件 |
| GET | `/_matrix/client/v1/widgets/{widget_id}` | 获取小组件信息 |
| PUT | `/_matrix/client/v1/widgets/{widget_id}` | 更新小组件 |
| POST | `/_matrix/client/v1/keys/rotation/rotate` | 轮换密钥 |
| POST | `/_matrix/client/v1/rendezvous` | 创建约会 |

### OIDC

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/r0/oidc/authorize` | OIDC 授权 |
| GET | `/_matrix/client/r0/oidc/callback` | OIDC 回调 |
| POST | `/_matrix/client/r0/oidc/logout` | OIDC 登出 |
| POST | `/_matrix/client/r0/oidc/register` | OIDC 注册 |
| POST | `/_matrix/client/r0/oidc/token` | 获取 OIDC 令牌 |
| GET | `/_matrix/client/r0/oidc/userinfo` | 获取 OIDC 用户信息 |
| GET | `/_matrix/client/v3/oidc/authorize` | OIDC 授权 |
| GET | `/_matrix/client/v3/oidc/callback` | OIDC 回调 |
| POST | `/_matrix/client/v3/oidc/login` | OIDC 登录 |
| POST | `/_matrix/client/v3/oidc/logout` | OIDC 登出 |
| POST | `/_matrix/client/v3/oidc/register` | OIDC 注册 |
| POST | `/_matrix/client/v3/oidc/token` | 获取 OIDC 令牌 |
| GET | `/_matrix/client/v3/oidc/userinfo` | 获取 OIDC 用户信息 |

### SAML

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/r0/logout/saml` | SAML 登出 |
| GET | `/_matrix/client/r0/saml/metadata` | SAML 元数据 |
| GET | `/_matrix/client/r0/saml/sp_metadata` | SP 元数据 |

---

## Matrix Federation API

### Federation v1

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/federation/v1` | 联邦发现 |
| GET | `/_matrix/federation/v1/backfill/{room_id}` | 获取房间历史 |
| GET | `/_matrix/federation/v1/event_auth` | 获取事件认证 |
| GET | `/_matrix/federation/v1/event/{event_id}` | 获取事件 |
| POST | `/_matrix/federation/v1/keys/claim` | 声明密钥 |
| POST | `/_matrix/federation/v1/keys/query` | 查询密钥 |
| POST | `/_matrix/federation/v1/keys/upload` | 上传密钥 |
| GET | `/_matrix/federation/v1/publicRooms` | 获取公共房间 |
| GET | `/_matrix/federation/v1/query/auth` | 查询认证 |
| GET | `/_matrix/federation/v1/state/{room_id}` | 获取房间状态 |
| GET | `/_matrix/federation/v1/version` | 获取联邦版本 |

### Federation v2

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/federation/v2/server` | 获取服务器密钥 |
| POST | `/_matrix/federation/v2/key/clone` | 克隆密钥 |

### Key Server

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/key/v2/server` | 获取服务器密钥 |

### Server

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/server_version` | 获取服务器版本 |

### App Service

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_matrix/app/v1/ping` | 应用服务 ping |

---

## Synapse Admin API

### Admin v1

#### Account

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v1/account/{user_id}` | 管理员操作账号 |
| GET | `/_synapse/admin/v1/whois/{user_id}` | 获取用户 WHOIS |

#### Appservices

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/appservices` | 获取应用服务列表 |
| POST | `/_synapse/admin/v1/appservices` | 注册应用服务 |
| GET | `/_synapse/admin/v1/appservices/query/user` | 查询应用服务用户 |

#### Background Updates

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v1/background_updates` | 触发后台更新 |

#### Backups

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/backups` | 获取备份列表 |

#### Captcha

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v1/captcha/cleanup` | 清理验证码 |

#### Config

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/config` | 获取配置信息 |

#### Event Reports

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/event_reports` | 获取事件举报列表 |
| POST | `/_synapse/admin/v1/event_reports` | 提交事件举报 |
| GET | `/_synapse/admin/v1/event_reports/{id}` | 获取举报详情 |
| PUT | `/_synapse/admin/v1/event_reports/{id}` | 更新举报状态 |
| GET | `/_synapse/admin/v1/event_reports/count` | 获取举报数量 |
| GET | `/_synapse/admin/v1/event_reports/stats` | 获取举报统计 |

#### Health

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/health` | 健康检查 |
| GET | `/_synapse/admin/v1/status` | 获取状态 |

#### Media

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/media` | 获取媒体列表 |
| DELETE | `/_synapse/admin/v1/media/{media_id}` | 删除媒体 |
| GET | `/_synapse/admin/v1/media/{media_id}` | 获取媒体信息 |
| GET | `/_synapse/admin/v1/media/quota` | 获取配额信息 |

#### Modules

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/modules` | 获取模块列表 |
| POST | `/_synapse/admin/v1/modules` | 注册模块 |
| GET | `/_synapse/admin/v1/modules/{module_name}` | 获取模块信息 |
| POST | `/_synapse/admin/v1/modules/check_spam` | 检查垃圾信息 |

#### Notifications

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/notifications` | 获取通知列表 |

#### Purge

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v1/purge_history` | 清除历史 |
| POST | `/_synapse/admin/v1/purge_room` | 清除房间 |

#### Push

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v1/push/cleanup` | 清理推送 |
| POST | `/_synapse/admin/v1/push/process` | 处理推送 |

#### Register

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v1/register` | 注册用户 |
| GET | `/_synapse/admin/v1/register/nonce` | 获取注册 nonce |

#### Reports

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/reports` | 获取举报列表 |
| GET | `/_synapse/admin/v1/reports/{report_id}` | 获取举报详情 |

#### Restart

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v1/restart` | 重启服务 |

#### Retention

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v1/retention/run` | 运行保留策略 |

#### Room Admin

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/rooms` | 获取房间列表 |
| GET | `/_synapse/admin/v1/rooms/{room_id}` | 获取房间详情 |
| DELETE | `/_synapse/admin/v1/rooms/{room_id}` | 删除房间 |
| POST | `/_synapse/admin/v1/rooms/{room_id}/block` | 封禁房间 |

#### Room Stats

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/room_stats` | 获取房间统计 |

#### Server Notices

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/server_notices` | 获取服务器通知 |

#### Server Version

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/server_version` | 获取服务器版本 |

#### Shutdown

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v1/shutdown_room` | 关闭房间 |

#### Spaces

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/spaces` | 获取空间列表 |
| GET | `/_synapse/admin/v1/spaces/{space_id}` | 获取空间详情 |
| DELETE | `/_synapse/admin/v1/spaces/{space_id}` | 删除空间 |

#### Statistics

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/statistics` | 获取统计信息 |
| GET | `/_synapse/admin/v1/user_stats` | 获取用户统计 |

#### Telemetry

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/telemetry/health` | 获取遥测健康 |
| GET | `/_synapse/admin/v1/telemetry/status` | 获取遥测状态 |

#### User Admin

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v1/users` | 获取用户列表 |
| DELETE | `/_synapse/admin/v1/users/{user_id}` | 删除用户 |
| GET | `/_synapse/admin/v1/users/{user_id}` | 获取用户详情 |
| PUT | `/_synapse/admin/v1/users/{user_id}/admin` | 设置管理员 |
| POST | `/_synapse/admin/v1/users/batch` | 批量创建用户 |

### Admin v2

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v2/users` | 获取用户列表 |
| GET | `/_synapse/admin/v2/users/{user_id}` | 获取用户详情 |

---

## Synapse Worker API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/worker/v1/events` | 获取事件列表 |
| POST | `/_synapse/worker/v1/register` | 注册 worker |
| GET | `/_synapse/worker/v1/select/{task_type}` | 选择任务 |
| GET | `/_synapse/worker/v1/statistics` | 获取统计信息 |
| GET | `/_synapse/worker/v1/tasks` | 获取任务列表 |
| POST | `/_synapse/worker/v1/tasks` | 创建任务 |
| POST | `/_synapse/worker/v1/tasks/{task_id}/fail` | 标记任务失败 |
| GET | `/_synapse/worker/v1/workers` | 获取 worker 列表 |
| GET | `/_synapse/worker/v1/workers/{worker_id}` | 获取 worker 详情 |

---

## Custom API

### Well-Known

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/.well-known/jwks.json` | 获取 JWKS |
| GET | `/.well-known/matrix/client` | 客户端发现 |
| GET | `/.well-known/matrix/server` | 服务器发现 |
| GET | `/.well-known/matrix/support` | 支持信息 |
| GET | `/.well-known/openid-configuration` | OpenID 配置 |

### Root

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/` | 根路径 |
| GET | `/capabilities` | 服务能力 |
| GET | `/config` | 服务配置 |
| GET | `/health` | 健康检查 |

### Account

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/account/3pid` | 获取 3pid 列表 |
| POST | `/account/3pid/add` | 添加 3pid |
| POST | `/account/3pid/bind` | 绑定 3pid |
| POST | `/account/3pid/delete` | 删除 3pid |
| POST | `/account/3pid/unbind` | 解绑 3pid |
| POST | `/account/deactivate` | 停用账号 |
| POST | `/account/password` | 修改密码 |
| GET | `/account/profile/{user_id}` | 获取用户资料 |
| PUT | `/account/profile/{user_id}/avatar_url` | 更新头像 |
| GET | `/account/whoami` | 获取当前用户 |

### Admin Services

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/admin/services` | 获取服务列表 |
| POST | `/admin/services` | 创建服务 |
| DELETE | `/admin/services/{service_id}` | 删除服务 |

### Authentication

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/delete_devices` | 删除设备 |
| GET | `/devices` | 获取设备列表 |
| GET | `/login` | 获取登录信息 |
| GET | `/logout` | 登出 |
| POST | `/logout` | 登出 |
| POST | `/logout/all` | 登出所有设备 |
| POST | `/refresh` | 刷新令牌 |
| GET | `/register` | 获取注册信息 |
| GET | `/register/available` | 检查用户名可用 |
| POST | `/register/email/submitToken` | 提交注册令牌 |

### Directory

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/directory/room/{room_id}/alias` | 获取房间别名 |

### Events

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/events` | 获取事件 |

### Invites & Join

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/invite/{room_id}` | 邀请用户 |
| POST | `/join/{room_id_or_alias}` | 加入房间 |
| POST | `/knock/{room_id_or_alias}` | 敲房间 |
| GET | `/joined_rooms` | 获取已加入房间 |

### Keys

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/keys/changes` | 获取密钥变化 |
| POST | `/keys/claim` | 声明密钥 |
| POST | `/keys/device_list_updates` | 更新设备列表 |
| POST | `/keys/device_signing/upload` | 上传设备签名 |
| POST | `/keys/device_signing/verify_done` | 完成验证 |
| POST | `/keys/device_signing/verify_mac` | MAC 验证 |
| POST | `/keys/qr_code/scan` | 扫描二维码 |
| GET | `/keys/qr_code/show` | 显示二维码 |
| POST | `/keys/query` | 查询密钥 |
| POST | `/keys/signatures/upload` | 上传签名 |
| POST | `/keys/upload` | 上传密钥 |

### Media

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/download/{server_name}/{media_id}` | 下载媒体 |
| GET | `/media/config` | 获取媒体配置 |
| POST | `/delete/{server_name}/{media_id}` | 删除媒体 |
| POST | `/thumbnail/{server_name}/{media_id}` | 获取缩略图 |
| POST | `/upload` | 上传媒体 |

### Notifications

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/notifications` | 获取通知列表 |

### Profile

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/profile/{user_id}` | 获取用户资料 |

### Pushers

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/pushers` | 获取推送器 |
| POST | `/pushers/set` | 设置推送器 |

### Pushrules

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/pushrules` | 获取推送规则 |
| GET | `/pushrules/{scope}` | 获取指定范围规则 |
| GET | `/pushrules/{scope}/{kind}` | 获取规则详情 |

### Room Keys

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/room_keys/{version}/keys` | 存储房间密钥 |
| POST | `/room_keys/batch_recover` | 批量恢复 |
| GET | `/room_keys/export` | 导出房间密钥 |
| GET | `/room_keys/export/{version}` | 导出指定版本 |
| POST | `/room_keys/import` | 导入房间密钥 |
| POST | `/room_keys/import/{version}` | 导入指定版本 |
| POST | `/room_keys/recover` | 恢复房间密钥 |
| GET | `/room_keys/verify/{version}` | 验证房间密钥 |

### Rooms

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/rooms/{room_id}` | 获取房间信息 |
| POST | `/_matrix/client/v3/rooms/{room_id}/ban` | 封禁用户 |
| POST | `/_matrix/client/v3/rooms/{room_id}/invite` | 邀请用户 |
| POST | `/_matrix/client/v3/rooms/{room_id}/join` | 加入房间 |
| POST | `/_matrix/client/v3/rooms/{room_id}/kick` | 踢出用户 |
| POST | `/_matrix/client/v3/rooms/{room_id}/leave` | 离开房间 |
| GET | `/rooms/{room_id}/aliases` | 获取房间别名 |
| GET | `/rooms/{room_id}/event/{event_id}` | 获取事件 |
| POST | `/rooms/{room_id}/forget` | 忘记房间 |
| GET | `/rooms/{room_id}/hierarchy` | 获取房间层级 |
| GET | `/rooms/{room_id}/initialSync` | 初始同步 |
| GET | `/rooms/{room_id}/members` | 获取成员 |
| GET | `/rooms/{room_id}/messages` | 获取消息 |
| POST | `/rooms/{room_id}/report` | 举报房间 |
| POST | `/rooms/{room_id}/report/{event_id}` | 举报事件 |
| GET | `/rooms/{room_id}/state` | 获取状态 |
| POST | `/rooms/{room_id}/unban` | 解除封禁 |
| POST | `/rooms/{room_id}/upgrade` | 升级房间 |

### Search

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/search` | 搜索 |
| POST | `/search_recipients` | 搜索收件人 |
| POST | `/search_rooms` | 搜索房间 |

### Spaces

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/spaces` | 创建空间 |
| DELETE | `/spaces/{space_id}` | 删除空间 |
| GET | `/spaces/{space_id}` | 获取空间信息 |
| PUT | `/spaces/{space_id}` | 更新空间 |
| GET | `/spaces/{space_id}/children` | 获取子空间 |
| POST | `/spaces/{space_id}/children` | 添加子空间 |
| GET | `/spaces/{space_id}/hierarchy` | 获取空间层级 |
| POST | `/spaces/{space_id}/invite` | 邀请到空间 |
| POST | `/spaces/{space_id}/join` | 加入空间 |
| POST | `/spaces/{space_id}/leave` | 离开空间 |
| GET | `/spaces/{space_id}/members` | 获取空间成员 |
| GET | `/spaces/{space_id}/rooms` | 获取空间房间 |
| GET | `/spaces/{space_id}/state` | 获取空间状态 |
| GET | `/spaces/{space_id}/summary` | 获取空间摘要 |
| GET | `/spaces/{space_id}/tree_path` | 获取树路径 |
| GET | `/spaces/public` | 获取公共空间 |
| GET | `/spaces/room/{room_id}` | 获取房间所在空间 |
| GET | `/spaces/room/{room_id}/parents` | 获取父空间 |
| GET | `/spaces/search` | 搜索空间 |
| GET | `/spaces/statistics` | 获取空间统计 |
| GET | `/spaces/user` | 获取用户空间 |

### Sync

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/sync` | 同步数据 |

### Thirdparty

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/thirdparty/location/{protocol}` | 第三方位置协议 |
| GET | `/thirdparty/protocol/{protocol}` | 第三方协议 |
| GET | `/thirdparty/protocols` | 第三方协议列表 |
| GET | `/thirdparty/user/{protocol}` | 第三方用户协议 |

### User Directory

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/user_directory/list` | 更新用户目录列表 |
| POST | `/user_directory/search` | 搜索用户目录 |

### User Account Data

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/user/{user_id}/account_data/` | 获取用户账户数据 |
| GET | `/user/{user_id}/filter/{filter_id}` | 获取过滤器 |
| GET | `/user/{user_id}/rooms` | 获取用户房间 |
| GET | `/user/{user_id}/rooms/{room_id}/tags` | 获取房间标签 |
| PUT | `/user/{user_id}/rooms/{room_id}/tags/{tag}` | 更新房间标签 |

### VoIP

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/voip/config` | 获取 VoIP 配置 |
| GET | `/voip/turnServer/guest` | 获取访客 TURN 服务器 |

### WebSocket

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/ws` | WebSocket 连接 |

### MCP (Model Context Protocol)

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/mcp/tools` | 获取 MCP 工具列表 |
| POST | `/mcp/tools/call` | 调用 MCP 工具 |

### Connections

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/connections` | 获取连接列表 |

### Rendezvous

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/proxy` | 代理请求 |
| GET | `/proxyValidate` | 验证代理 |
| GET | `/serviceValidate` | 验证服务 |
| GET | `/p3/serviceValidate` | P3 服务验证 |

### Quota

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/quota/alerts` | 获取配额告警 |
| GET | `/quota/check` | 检查配额 |
| GET | `/quota/stats` | 获取配额统计 |

### URL Preview

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/preview_url` | 预览 URL |

### Voice

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/r0/voice/config` | 获取语音配置 |

---

## 端点统计 (293)

### 按 HTTP 方法

| 方法 | 数量 |
|------|------|
| GET | ~160 |
| POST | ~95 |
| PUT | ~20 |
| DELETE | ~15 |
| PATCH | ~3 |

### 按模块

| 模块 | 数量 |
|------|------|
| Matrix Client v3 | ~50 |
| Matrix Client r0 | ~25 |
| Matrix Client v1 | ~15 |
| Matrix Federation | ~50 |
| Synapse Admin v1 | ~75 |
| Synapse Admin v2 | ~5 |
| Synapse Worker | ~15 |
| Custom API | ~60 |

---

*文档更新时间: 2026-03-28*
*基于代码扫描自动生成*

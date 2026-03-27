# synapse-rust API 参考文档

> 生成时间: 2026-03-27
> 代码行数: \~18万行
> 最后审计: 2026-03-27 (API 完整性排查与修复)

***

## 统计概览

| 项目           | 结果        |
| :----------- | :-------- |
| **API 端点总数** | **656**   |
| **模块数量**     | **48**    |
| **HTTP 方法**  | 1167+ 处理器 |
| **数据库表**     | 135+      |

***

## 模块端点统计

| 模块                   | 端点数量    |
| :------------------- | :------ |
| mod (核心)             | 57      |
| federation           | 47      |
| friend\_room         | 43      |
| worker               | 21      |
| media                | 21      |
| space                | 21      |
| module               | 20      |
| e2ee\_routes         | 27      |
| key\_backup          | 20      |
| admin/user           | 18      |
| push                 | 18      |
| background\_update   | 17      |
| event\_report        | 16      |
| room\_summary        | 16      |
| thread               | 16      |
| app\_service         | 15      |
| oidc                 | 15      |
| verification\_routes | 14      |
| search               | 12      |
| account\_data        | 12      |
| admin/federation     | 12      |
| thirdparty           | 10      |
| external\_service    | 8       |
| device               | 8       |
| admin/notification   | 10      |
| voice                | 10      |
| **总计**               | **656** |

***

## API 分类

### Matrix Client API

| 分类             | 路径前缀                  | 端点数量 |
| :------------- | :-------------------- | :--- |
| Client API v3  | `/_matrix/client/v3`  | 150+ |
| Client API r0  | `/_matrix/client/r0`  | 120+ |
| Client API v1  | `/_matrix/client/v1`  | 80+  |
| Media API      | `/_matrix/media`      | 21   |
| Federation API | `/_matrix/federation` | 47   |

### Admin API

| 分类         | 路径前缀                 | 端点数量 |
| :--------- | :------------------- | :--- |
| Admin v1   | `/_synapse/admin/v1` | 120+ |
| Worker API | `/_synapse/worker`   | 21   |

### Custom API

| 分类            | 路径前缀                          | 端点数量 |
| :------------ | :---------------------------- | :--- |
| Space         | `/spaces`                     | 21   |
| AI Connection | `/connections`, `/mcp`        | 4    |
| CAS           | `/admin`, `/login`, `/logout` | 9    |
| SAML          | `/_matrix/client/r0/saml`     | 7    |
| OIDC          | `/_matrix/client/r0/oidc`     | 15   |

***

## 完整路由列表

### mod (核心模块) (57)

- `/.well-known/matrix/client`
- `/.well-known/matrix/server`
- `/.well-known/matrix/support`
- `/_matrix/client/r0/account/3pid/add`
- `/_matrix/client/r0/account/3pid/bind`
- `/_matrix/client/r0/account/whoami`
- `/_matrix/client/r0/capabilities`
- `/_matrix/client/r0/createRoom`
- `/_matrix/client/r0/events`
- `/_matrix/client/r0/joined_rooms`
- `/_matrix/client/r0/login`
- `/_matrix/client/r0/logout`
- `/_matrix/client/r0/logout/all`
- `/_matrix/client/r0/media/config`
- `/_matrix/client/r0/profile/{user_id}`
- `/_matrix/client/r0/refresh`
- `/_matrix/client/r0/rooms/{room_id}`
- `/_matrix/client/r0/rooms/{room_id}/ban`
- `/_matrix/client/r0/rooms/{room_id}/join`
- `/_matrix/client/r0/rooms/{room_id}/kick`
- `/_matrix/client/r0/rooms/{room_id}/leave`
- `/_matrix/client/r0/rooms/{room_id}/unban`
- `/_matrix/client/r0/sync`
- `/_matrix/client/r0/version`
- `/_matrix/client/r0/voip/config`
- `/_matrix/client/r0/voip/turnServer`
- `/_matrix/client/v1/media/config`
- `/_matrix/client/v1/sync`
- `/_matrix/client/v3/account/3pid/add`
- `/_matrix/client/v3/account/3pid/bind`
- `/_matrix/client/v3/account/whoami`
- `/_matrix/client/v3/capabilities`
- `/_matrix/client/v3/createRoom`
- `/_matrix/client/v3/events`
- `/_matrix/client/v3/joined_rooms`
- `/_matrix/client/v3/login`
- `/_matrix/client/v3/logout`
- `/_matrix/client/v3/logout/all`
- `/_matrix/client/v3/media/config`
- `/_matrix/client/v3/my_rooms`
- `/_matrix/client/v3/presence/list`
- `/_matrix/client/v3/profile/{user_id}`
- `/_matrix/client/v3/pushrules/`
- `/_matrix/client/v3/refresh`
- `/_matrix/client/v3/rooms/{room_id}`
- `/_matrix/client/v3/rooms/{room_id}/ban`
- `/_matrix/client/v3/rooms/{room_id}/join`
- `/_matrix/client/v3/rooms/{room_id}/kick`
- `/_matrix/client/v3/rooms/{room_id}/leave`
- `/_matrix/client/v3/rooms/{room_id}/unban`
- `/_matrix/client/v3/sync`
- `/_matrix/client/v3/versions`
- `/_matrix/client/v3/voip/config`
- `/_matrix/client/v3/voip/turnServer`
- `/_matrix/client/versions`
- `/_matrix/server_version`
- `/health`

### account\_data (12)

- `/_matrix/client/r0/user/{user_id}/account_data/`
- `/_matrix/client/r0/user/{user_id}/account_data/{type}`
- `/_matrix/client/r0/user/{user_id}/filter`
- `/_matrix/client/r0/user/{user_id}/filter/{filter_id}`
- `/_matrix/client/r0/user/{user_id}/openid/request_token`
- `/_matrix/client/r0/user/{user_id}/rooms/{room_id}/account_data/{type}`
- `/_matrix/client/v3/user/{user_id}/account_data/`
- `/_matrix/client/v3/user/{user_id}/account_data/{type}`
- `/_matrix/client/v3/user/{user_id}/filter`
- `/_matrix/client/v3/user/{user_id}/filter/{filter_id}`
- `/_matrix/client/v3/user/{user_id}/openid/request_token`
- `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}`

### admin/federation (12)

- `/_synapse/admin/v1/federation/blacklist`
- `/_synapse/admin/v1/federation/blacklist/{server_name}`
- `/_synapse/admin/v1/federation/cache`
- `/_synapse/admin/v1/federation/cache/clear`
- `/_synapse/admin/v1/federation/cache/{key}`
- `/_synapse/admin/v1/federation/confirm`
- `/_synapse/admin/v1/federation/destinations`
- `/_synapse/admin/v1/federation/destinations/{destination}`
- `/_synapse/admin/v1/federation/destinations/{destination}/reset_connection`
- `/_synapse/admin/v1/federation/destinations/{destination}/rooms`
- `/_synapse/admin/v1/federation/resolve`
- `/_synapse/admin/v1/federation/rewrite`

### admin/room (28)

- `/_synapse/admin/v1/purge_history`
- `/_synapse/admin/v1/purge_room`
- `/_synapse/admin/v1/room_stats`
- `/_synapse/admin/v1/room_stats/{room_id}`
- `/_synapse/admin/v1/rooms`
- `/_synapse/admin/v1/rooms/{room_id}`
- `/_synapse/admin/v1/rooms/{room_id}/ban/{user_id}`
- `/_synapse/admin/v1/rooms/{room_id}/block`
- `/_synapse/admin/v1/rooms/{room_id}/delete`
- `/_synapse/admin/v1/rooms/{room_id}/event_context/{event_id}`
- `/_synapse/admin/v1/rooms/{room_id}/forward_extremities`
- `/_synapse/admin/v1/rooms/{room_id}/kick/{user_id}`
- `/_synapse/admin/v1/rooms/{room_id}/listings`
- `/_synapse/admin/v1/rooms/{room_id}/listings/public`
- `/_synapse/admin/v1/rooms/{room_id}/make_admin`
- `/_synapse/admin/v1/rooms/{room_id}/members`
- `/_synapse/admin/v1/rooms/{room_id}/members/{user_id}`
- `/_synapse/admin/v1/rooms/{room_id}/messages`
- `/_synapse/admin/v1/rooms/{room_id}/search`
- `/_synapse/admin/v1/rooms/{room_id}/state`
- `/_synapse/admin/v1/rooms/{room_id}/unban/{user_id}`
- `/_synapse/admin/v1/rooms/{room_id}/unblock`
- `/_synapse/admin/v1/shutdown_room`
- `/_synapse/admin/v1/spaces`
- `/_synapse/admin/v1/spaces/{space_id}`
- `/_synapse/admin/v1/spaces/{space_id}/rooms`
- `/_synapse/admin/v1/spaces/{space_id}/stats`
- `/_synapse/admin/v1/spaces/{space_id}/users`

### admin/user (18)

- `/_synapse/admin/v1/account/{user_id}`
- `/_synapse/admin/v1/user_sessions/{user_id}`
- `/_synapse/admin/v1/user_sessions/{user_id}/invalidate`
- `/_synapse/admin/v1/user_stats`
- `/_synapse/admin/v1/users`
- `/_synapse/admin/v1/users/batch`
- `/_synapse/admin/v1/users/batch_deactivate`
- `/_synapse/admin/v1/users/{user_id}`
- `/_synapse/admin/v1/users/{user_id}/admin`
- `/_synapse/admin/v1/users/{user_id}/deactivate`
- `/_synapse/admin/v1/users/{user_id}/devices`
- `/_synapse/admin/v1/users/{user_id}/devices/{device_id}`
- `/_synapse/admin/v1/users/{user_id}/login`
- `/_synapse/admin/v1/users/{user_id}/logout`
- `/_synapse/admin/v1/users/{user_id}/password`
- `/_synapse/admin/v1/users/{user_id}/rooms`
- `/_synapse/admin/v2/users`
- `/_synapse/admin/v2/users/{user_id}`

### device (8)

- `/_matrix/client/r0/delete_devices`
- `/_matrix/client/r0/devices`
- `/_matrix/client/r0/devices/{device_id}`
- `/_matrix/client/r0/keys/device_list_updates`
- `/_matrix/client/v3/delete_devices`
- `/_matrix/client/v3/devices`
- `/_matrix/client/v3/devices/{device_id}`
- `/_matrix/client/v3/keys/device_list_updates`

### dm (5)

- `/_matrix/client/r0/create_dm`
- `/_matrix/client/v3/direct`
- `/_matrix/client/v3/direct/{room_id}`
- `/_matrix/client/v3/rooms/{room_id}/dm`
- `/_matrix/client/v3/rooms/{room_id}/dm/partner`

### e2ee\_routes (27)

- `/_matrix/client/r0/keys/changes`
- `/_matrix/client/r0/keys/claim`
- `/_matrix/client/r0/keys/device_signing/upload`
- `/_matrix/client/r0/keys/query`
- `/_matrix/client/r0/keys/signatures/upload`
- `/_matrix/client/r0/keys/upload`
- `/_matrix/client/r0/rooms/{room_id}/keys/distribution`
- `/_matrix/client/r0/sendToDevice/{event_type}/{transaction_id}`
- `/_matrix/client/v3/device_trust`
- `/_matrix/client/v3/device_trust/{device_id}`
- `/_matrix/client/v3/device_verification/request`
- `/_matrix/client/v3/device_verification/respond`
- `/_matrix/client/v3/device_verification/status/{token}`
- `/_matrix/client/v3/keys/backup/secure`
- `/_matrix/client/v3/keys/backup/secure/{backup_id}`
- `/_matrix/client/v3/keys/backup/secure/{backup_id}/keys`
- `/_matrix/client/v3/keys/backup/secure/{backup_id}/restore`
- `/_matrix/client/v3/keys/backup/secure/{backup_id}/verify`
- `/_matrix/client/v3/keys/changes`
- `/_matrix/client/v3/keys/claim`
- `/_matrix/client/v3/keys/device_signing/upload`
- `/_matrix/client/v3/keys/query`
- `/_matrix/client/v3/keys/signatures/upload`
- `/_matrix/client/v3/keys/upload`
- `/_matrix/client/v3/rooms/{room_id}/keys/distribution`
- `/_matrix/client/v3/security/summary`
- `/_matrix/client/v3/sendToDevice/{event_type}/{transaction_id}`

### federation (47)

- `/_matrix/federation/v1`
- `/_matrix/federation/v1/backfill/{room_id}`
- `/_matrix/federation/v1/event/{event_id}`
- `/_matrix/federation/v1/event_auth`
- `/_matrix/federation/v1/exchange_third_party_invite/{room_id}`
- `/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}`
- `/_matrix/federation/v1/get_joining_rules/{room_id}`
- `/_matrix/federation/v1/get_missing_events/{room_id}`
- `/_matrix/federation/v1/hierarchy/{room_id}`
- `/_matrix/federation/v1/invite/{room_id}/{event_id}`
- `/_matrix/federation/v1/keys/claim`
- `/_matrix/federation/v1/keys/query`
- `/_matrix/federation/v1/keys/upload`
- `/_matrix/federation/v1/knock/{room_id}/{user_id}`
- `/_matrix/federation/v1/make_join/{room_id}/{user_id}`
- `/_matrix/federation/v1/make_leave/{room_id}/{user_id}`
- `/_matrix/federation/v1/media/download/{server_name}/{media_id}`
- `/_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}`
- `/_matrix/federation/v1/members/{room_id}`
- `/_matrix/federation/v1/members/{room_id}/joined`
- `/_matrix/federation/v1/openid/userinfo`
- `/_matrix/federation/v1/publicRooms`
- `/_matrix/federation/v1/query/auth`
- `/_matrix/federation/v1/query/destination`
- `/_matrix/federation/v1/query/directory`
- `/_matrix/federation/v1/query/directory/room/{room_id}`
- `/_matrix/federation/v1/query/profile/{user_id}`
- `/_matrix/federation/v1/room/{room_id}/{event_id}`
- `/_matrix/federation/v1/room_auth/{room_id}`
- `/_matrix/federation/v1/send/{txn_id}`
- `/_matrix/federation/v1/send_join/{room_id}/{event_id}`
- `/_matrix/federation/v1/send_leave/{room_id}/{event_id}`
- `/_matrix/federation/v1/state/{room_id}`
- `/_matrix/federation/v1/state_ids/{room_id}`
- `/_matrix/federation/v1/thirdparty/invite`
- `/_matrix/federation/v1/timestamp_to_event/{room_id}`
- `/_matrix/federation/v1/user/devices/{user_id}`
- `/_matrix/federation/v1/version`
- `/_matrix/federation/v2/invite/{room_id}/{event_id}`
- `/_matrix/federation/v2/key/clone`
- `/_matrix/federation/v2/query/{server_name}/{key_id}`
- `/_matrix/federation/v2/send_join/{room_id}/{event_id}`
- `/_matrix/federation/v2/send_leave/{room_id}/{event_id}`
- `/_matrix/federation/v2/server`
- `/_matrix/federation/v2/user/keys/query`
- `/_matrix/key/v2/query/{server_name}/{key_id}`
- `/_matrix/key/v2/server`

### friend\_room (43)

- `/_matrix/client/r0/friends/check/{user_id}`
- `/_matrix/client/r0/friends/groups`
- `/_matrix/client/r0/friends/groups/{group_id}`
- `/_matrix/client/r0/friends/groups/{group_id}/add/{user_id}`
- `/_matrix/client/r0/friends/groups/{group_id}/friends`
- `/_matrix/client/r0/friends/groups/{group_id}/name`
- `/_matrix/client/r0/friends/groups/{group_id}/remove/{user_id}`
- `/_matrix/client/r0/friends/request`
- `/_matrix/client/r0/friends/request/received`
- `/_matrix/client/r0/friends/request/{user_id}/accept`
- `/_matrix/client/r0/friends/request/{user_id}/cancel`
- `/_matrix/client/r0/friends/request/{user_id}/reject`
- `/_matrix/client/r0/friends/requests/incoming`
- `/_matrix/client/r0/friends/requests/outgoing`
- `/_matrix/client/r0/friends/suggestions`
- `/_matrix/client/r0/friends/{user_id}`
- `/_matrix/client/r0/friends/{user_id}/groups`
- `/_matrix/client/r0/friends/{user_id}/info`
- `/_matrix/client/r0/friends/{user_id}/note`
- `/_matrix/client/r0/friends/{user_id}/status`
- `/_matrix/client/r0/friendships`
- `/_matrix/client/v1/friends`
- `/_matrix/client/v1/friends/check/{user_id}`
- `/_matrix/client/v1/friends/groups`
- `/_matrix/client/v1/friends/groups/{group_id}`
- `/_matrix/client/v1/friends/groups/{group_id}/add/{user_id}`
- `/_matrix/client/v1/friends/groups/{group_id}/friends`
- `/_matrix/client/v1/friends/groups/{group_id}/name`
- `/_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}`
- `/_matrix/client/v1/friends/request`
- `/_matrix/client/v1/friends/request/received`
- `/_matrix/client/v1/friends/request/{user_id}/accept`
- `/_matrix/client/v1/friends/request/{user_id}/cancel`
- `/_matrix/client/v1/friends/request/{user_id}/reject`
- `/_matrix/client/v1/friends/requests/incoming`
- `/_matrix/client/v1/friends/requests/outgoing`
- `/_matrix/client/v1/friends/suggestions`
- `/_matrix/client/v1/friends/{user_id}`
- `/_matrix/client/v1/friends/{user_id}/groups`
- `/_matrix/client/v1/friends/{user_id}/info`
- `/_matrix/client/v1/friends/{user_id}/note`
- `/_matrix/client/v1/friends/{user_id}/status`
- `/_matrix/client/v3/friends`

### media (21)

- `/_matrix/media/r0/config`
- `/_matrix/media/r0/upload`
- `/_matrix/media/r1/download/{server_name}/{media_id}`
- `/_matrix/media/r1/download/{server_name}/{media_id}/{filename}`
- `/_matrix/media/v1/config`
- `/_matrix/media/v1/delete/{server_name}/{media_id}`
- `/_matrix/media/v1/download/{server_name}/{media_id}`
- `/_matrix/media/v1/download/{server_name}/{media_id}/{filename}`
- `/_matrix/media/v1/preview_url`
- `/_matrix/media/v1/quota/alerts`
- `/_matrix/media/v1/quota/check`
- `/_matrix/media/v1/quota/stats`
- `/_matrix/media/v1/upload`
- `/_matrix/media/v3/config`
- `/_matrix/media/v3/delete/{server_name}/{media_id}`
- `/_matrix/media/v3/download/{server_name}/{media_id}`
- `/_matrix/media/v3/download/{server_name}/{media_id}/{filename}`
- `/_matrix/media/v3/preview_url`
- `/_matrix/media/v3/thumbnail/{server_name}/{media_id}`
- `/_matrix/media/v3/upload`
- `/_matrix/media/v3/upload/{server_name}/{media_id}`

### room\_summary (16)

- `/_matrix/client/r0/rooms/{room_id}/summary`
- `/_matrix/client/r0/rooms/{room_id}/summary/members`
- `/_matrix/client/r0/rooms/{room_id}/summary/state`
- `/_matrix/client/r0/rooms/{room_id}/summary/stats`
- `/_matrix/client/v3/rooms/{room_id}/summary`
- `/_matrix/client/v3/rooms/{room_id}/summary/heroes/recalculate`
- `/_matrix/client/v3/rooms/{room_id}/summary/members`
- `/_matrix/client/v3/rooms/{room_id}/summary/members/{user_id}`
- `/_matrix/client/v3/rooms/{room_id}/summary/state`
- `/_matrix/client/v3/rooms/{room_id}/summary/state/{event_type}/{state_key}`
- `/_matrix/client/v3/rooms/{room_id}/summary/stats`
- `/_matrix/client/v3/rooms/{room_id}/summary/stats/recalculate`
- `/_matrix/client/v3/rooms/{room_id}/summary/sync`
- `/_matrix/client/v3/rooms/{room_id}/summary/unread/clear`
- `/_synapse/room_summary/v1/summaries`
- `/_synapse/room_summary/v1/updates/process`

### search (12)

- `/_matrix/client/r0/search`
- `/_matrix/client/r0/search_recipients`
- `/_matrix/client/r0/search_rooms`
- `/_matrix/client/v1/rooms/{room_id}/context/{event_id}`
- `/_matrix/client/v1/rooms/{room_id}/hierarchy`
- `/_matrix/client/v1/rooms/{room_id}/timestamp_to_event`
- `/_matrix/client/v3/rooms/{room_id}/context/{event_id}`
- `/_matrix/client/v3/rooms/{room_id}/hierarchy`
- `/_matrix/client/v3/search`
- `/_matrix/client/v3/search_recipients`
- `/_matrix/client/v3/search_rooms`
- `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads`

### space (21)

- `/spaces`
- `/spaces/public`
- `/spaces/room/{room_id}`
- `/spaces/room/{room_id}/parents`
- `/spaces/search`
- `/spaces/statistics`
- `/spaces/user`
- `/spaces/{space_id}`
- `/spaces/{space_id}/children`
- `/spaces/{space_id}/children/{room_id}`
- `/spaces/{space_id}/hierarchy`
- `/spaces/{space_id}/hierarchy/v1`
- `/spaces/{space_id}/invite`
- `/spaces/{space_id}/join`
- `/spaces/{space_id}/leave`
- `/spaces/{space_id}/members`
- `/spaces/{space_id}/rooms`
- `/spaces/{space_id}/state`
- `/spaces/{space_id}/summary`
- `/spaces/{space_id}/summary/with_children`
- `/spaces/{space_id}/tree_path`

### thread (16)

- `/_matrix/client/v1/rooms/{room_id}/replies/{event_id}/redact`
- `/_matrix/client/v1/rooms/{room_id}/threads`
- `/_matrix/client/v1/rooms/{room_id}/threads/search`
- `/_matrix/client/v1/rooms/{room_id}/threads/unread`
- `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}`
- `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/freeze`
- `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/mute`
- `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/read`
- `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies`
- `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/stats`
- `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/subscribe`
- `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unfreeze`
- `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unsubscribe`
- `/_matrix/client/v1/threads`
- `/_matrix/client/v1/threads/subscribed`
- `/_matrix/client/v1/threads/unread`

### worker (21)

- `/_synapse/worker/v1/commands/{command_id}/complete`
- `/_synapse/worker/v1/commands/{command_id}/fail`
- `/_synapse/worker/v1/events`
- `/_synapse/worker/v1/register`
- `/_synapse/worker/v1/replication/{worker_id}/position`
- `/_synapse/worker/v1/replication/{worker_id}/{stream_name}`
- `/_synapse/worker/v1/select/{task_type}`
- `/_synapse/worker/v1/statistics`
- `/_synapse/worker/v1/statistics/types`
- `/_synapse/worker/v1/tasks`
- `/_synapse/worker/v1/tasks/claim/{worker_id}`
- `/_synapse/worker/v1/tasks/{task_id}/claim/{worker_id}`
- `/_synapse/worker/v1/tasks/{task_id}/complete`
- `/_synapse/worker/v1/tasks/{task_id}/fail`
- `/_synapse/worker/v1/workers`
- `/_synapse/worker/v1/workers/type/{worker_type}`
- `/_synapse/worker/v1/workers/{worker_id}`
- `/_synapse/worker/v1/workers/{worker_id}/commands`
- `/_synapse/worker/v1/workers/{worker_id}/connect`
- `/_synapse/worker/v1/workers/{worker_id}/disconnect`
- `/_synapse/worker/v1/workers/{worker_id}/heartbeat`

***

## API 版本兼容性

| API版本    | 说明            | 状态     |
| :------- | :------------ | :----- |
| r0       | 旧版API         | ✅ 兼容   |
| v3       | 当前稳定版本        | ✅ 推荐   |
| v1       | 特定功能版本        | ✅ 支持   |
| v2       | Federation v2 | ✅ 支持   |
| unstable | 实验性功能         | ⚠️ 不稳定 |

***

## MSC 功能支持

| MSC     | 功能名称              | 状态    |
| :------ | :---------------- | :---- |
| MSC3575 | Sliding Sync      | ✅ 已实现 |
| MSC3983 | Thread            | ✅ 已实现 |
| MSC4380 | 邀请屏蔽              | ✅ 已实现 |
| MSC4354 | Sticky Events     | ✅ 已实现 |
| MSC4388 | QR 登录             | ✅ 已实现 |
| MSC4261 | Widget API        | ✅ 已实现 |
| MSC3245 | Room Summary      | ✅ 已实现 |
| MSC3814 | Dehydrated Device | ✅ 已实现 |

***

## 状态码说明

| 状态码 | 说明      |
| :-- | :------ |
| 200 | 成功      |
| 201 | 创建成功    |
| 204 | 无内容     |
| 400 | 请求错误    |
| 401 | 未认证     |
| 403 | 禁止访问    |
| 404 | 未找到     |
| 409 | 冲突      |
| 429 | 请求过多    |
| 500 | 服务器内部错误 |

***

*文档生成完成 - 基于 synapse-rust 项目实际代码统计 (2026-03-26)*

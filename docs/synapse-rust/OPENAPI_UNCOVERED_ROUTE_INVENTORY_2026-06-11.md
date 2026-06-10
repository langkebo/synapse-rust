# OpenAPI 未覆盖路由清单

- 日期: `2026-06-11`
- 当前 OpenAPI 注解数: `364`
- 当前 OpenAPI 已文档化唯一路径数: `308`
- 声明路由路径数: `283`
- 未覆盖路径总数: `123`
- 标准兼容: `0`
- unstable MSC: `0`
- 私有扩展: `123`

> 分类规则:
> - `标准兼容`: Matrix 稳定 API、兼容版本路径、AppService/Federation/Key API 等非私有扩展路径。
> - `unstable MSC`: `/_matrix/client/unstable/*` 路径。
> - `私有扩展`: 朋友关系、语音、阅后即焚、线程、组件、私有 appservice/external_services 等项目扩展路径。

## 标准兼容
- 数量: `0`

## Unstable MSC
- 数量: `0`

## 私有扩展
- 数量: `123`
- 分组 `/_matrix/admin/v1`: `3`

```text
/_matrix/admin/v1/external_services
/_matrix/admin/v1/external_services/health
/_matrix/admin/v1/external_services/{as_id}
```

- 分组 `/_matrix/client/r0/create_dm`: `1`

```text
/_matrix/client/r0/create_dm
```

- 分组 `/_matrix/client/r0/direct`: `2`

```text
/_matrix/client/r0/direct
/_matrix/client/r0/direct/{room_id}
```

- 分组 `/_matrix/client/r0/friends`: `23`

```text
/_matrix/client/r0/friends/check/{user_id}
/_matrix/client/r0/friends/dm/{user_id}
/_matrix/client/r0/friends/groups
/_matrix/client/r0/friends/groups/{group_id}
/_matrix/client/r0/friends/groups/{group_id}/add/{user_id}
/_matrix/client/r0/friends/groups/{group_id}/friends
/_matrix/client/r0/friends/groups/{group_id}/name
/_matrix/client/r0/friends/groups/{group_id}/remove/{user_id}
/_matrix/client/r0/friends/request
/_matrix/client/r0/friends/request/received
/_matrix/client/r0/friends/request/{user_id}/accept
/_matrix/client/r0/friends/request/{user_id}/cancel
/_matrix/client/r0/friends/request/{user_id}/reject
/_matrix/client/r0/friends/requests/incoming
/_matrix/client/r0/friends/requests/outgoing
/_matrix/client/r0/friends/search
/_matrix/client/r0/friends/suggestions
/_matrix/client/r0/friends/{user_id}
/_matrix/client/r0/friends/{user_id}/displayname
/_matrix/client/r0/friends/{user_id}/groups
/_matrix/client/r0/friends/{user_id}/info
/_matrix/client/r0/friends/{user_id}/note
/_matrix/client/r0/friends/{user_id}/status
```

- 分组 `/_matrix/client/r0/friendships`: `1`

```text
/_matrix/client/r0/friendships
```

- 分组 `/_matrix/client/r0/voice`: `2`

```text
/_matrix/client/r0/voice/config
/_matrix/client/r0/voice/upload
```

- 分组 `/_matrix/client/v1/external_services`: `2`

```text
/_matrix/client/v1/external_services/health
/_matrix/client/v1/external_services/{service_id}
```

- 分组 `/_matrix/client/v1/friends`: `24`

```text
/_matrix/client/v1/friends
/_matrix/client/v1/friends/check/{user_id}
/_matrix/client/v1/friends/dm/{user_id}
/_matrix/client/v1/friends/groups
/_matrix/client/v1/friends/groups/{group_id}
/_matrix/client/v1/friends/groups/{group_id}/add/{user_id}
/_matrix/client/v1/friends/groups/{group_id}/friends
/_matrix/client/v1/friends/groups/{group_id}/name
/_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}
/_matrix/client/v1/friends/request
/_matrix/client/v1/friends/request/received
/_matrix/client/v1/friends/request/{user_id}/accept
/_matrix/client/v1/friends/request/{user_id}/cancel
/_matrix/client/v1/friends/request/{user_id}/reject
/_matrix/client/v1/friends/requests/incoming
/_matrix/client/v1/friends/requests/outgoing
/_matrix/client/v1/friends/search
/_matrix/client/v1/friends/suggestions
/_matrix/client/v1/friends/{user_id}
/_matrix/client/v1/friends/{user_id}/displayname
/_matrix/client/v1/friends/{user_id}/groups
/_matrix/client/v1/friends/{user_id}/info
/_matrix/client/v1/friends/{user_id}/note
/_matrix/client/v1/friends/{user_id}/status
```

- 分组 `/_matrix/client/v1/rooms`: `18`

```text
/_matrix/client/v1/rooms/create_private
/_matrix/client/v1/rooms/{room_id}/burn
/_matrix/client/v1/rooms/{room_id}/burn/pending
/_matrix/client/v1/rooms/{room_id}/burn/{event_id}
/_matrix/client/v1/rooms/{room_id}/threads
/_matrix/client/v1/rooms/{room_id}/threads/search
/_matrix/client/v1/rooms/{room_id}/threads/unread
/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}
/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/freeze
/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/mute
/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/read
/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies
/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/stats
/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/subscribe
/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unfreeze
/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unsubscribe
/_matrix/client/v1/rooms/{room_id}/widgets
/_matrix/client/v1/rooms/{room_id}/widgets/jitsi/config
```

- 分组 `/_matrix/client/v1/threads`: `3`

```text
/_matrix/client/v1/threads
/_matrix/client/v1/threads/subscribed
/_matrix/client/v1/threads/unread
```

- 分组 `/_matrix/client/v1/user`: `3`

```text
/_matrix/client/v1/user/burn/config
/_matrix/client/v1/user/burn/stats
/_matrix/client/v1/user/{user_id}/appservice
```

- 分组 `/_matrix/client/v1/voice`: `5`

```text
/_matrix/client/v1/voice/config
/_matrix/client/v1/voice/room/{room_id}/stats
/_matrix/client/v1/voice/stats
/_matrix/client/v1/voice/upload
/_matrix/client/v1/voice/user/{user_id}/stats
```

- 分组 `/_matrix/client/v1/widgets`: `7`

```text
/_matrix/client/v1/widgets
/_matrix/client/v1/widgets/sessions/{session_id}
/_matrix/client/v1/widgets/{widget_id}
/_matrix/client/v1/widgets/{widget_id}/config
/_matrix/client/v1/widgets/{widget_id}/permissions
/_matrix/client/v1/widgets/{widget_id}/permissions/{user_id}
/_matrix/client/v1/widgets/{widget_id}/sessions
```

- 分组 `/_matrix/client/v3/appservice`: `2`

```text
/_matrix/client/v3/appservice/alias
/_matrix/client/v3/appservice/user
```

- 分组 `/_matrix/client/v3/create_dm`: `1`

```text
/_matrix/client/v3/create_dm
```

- 分组 `/_matrix/client/v3/friends`: `5`

```text
/_matrix/client/v3/friends
/_matrix/client/v3/friends/check/{user_id}
/_matrix/client/v3/friends/requests/incoming
/_matrix/client/v3/friends/requests/outgoing
/_matrix/client/v3/friends/search
```

- 分组 `/_matrix/client/v3/rooms`: `6`

```text
/_matrix/client/v3/rooms/create_private
/_matrix/client/v3/rooms/{room_id}/burn
/_matrix/client/v3/rooms/{room_id}/burn/pending
/_matrix/client/v3/rooms/{room_id}/burn/{event_id}
/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities
/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/send
```

- 分组 `/_matrix/client/v3/user`: `3`

```text
/_matrix/client/v3/user/burn/config
/_matrix/client/v3/user/burn/stats
/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads
```

- 分组 `/_matrix/client/v3/voice`: `11`

```text
/_matrix/client/v3/voice/config
/_matrix/client/v3/voice/room/{room_id}
/_matrix/client/v3/voice/room/{room_id}/stats
/_matrix/client/v3/voice/stats
/_matrix/client/v3/voice/upload
/_matrix/client/v3/voice/user/{user_id}
/_matrix/client/v3/voice/user/{user_id}/stats
/_matrix/client/v3/voice/{media_id}
/_matrix/client/v3/voice/{media_id}/convert
/_matrix/client/v3/voice/{media_id}/optimize
/_matrix/client/v3/voice/{media_id}/transcription
```

- 分组 `/_matrix/client/v3/widgets`: `1`

```text
/_matrix/client/v3/widgets/create
```


## 后续补齐建议
- 优先补 `标准兼容` 中体量小、返回稳定的路径，如 `r0/v1` 兼容认证路径、`AppService`、`captcha r0`、`push notification r0`。
- 其次补 `unstable MSC`，按 MSC 编号逐组推进，避免和实验性实现脱节。
- `私有扩展` 建议最后补，先按模块拆分为 `friends / voice / thread / widget / burn / external_services` 六类。

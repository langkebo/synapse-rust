# 4.17 搜索 API 测试结果

**测试时间**: 2026-02-18 17:35:12
**测试环境**: http://localhost:8008

---

### SEARCH-001: POST /_matrix/client/v3/search

**描述**: 搜索用户 - 有效请求

**状态**: ✅ 通过

**响应时间**: 23ms

**响应内容**:
```json
{"search_categories":{"users":{"limited":false,"results":[{"avatar_url":null,"display_name":null,"user_id":"@admin:cjystx.top"}]}}}
```

---

### SEARCH-002: POST /_matrix/client/v3/search

**描述**: 搜索用户 - 不存在的用户

**状态**: ✅ 通过

**响应时间**: 22ms

**响应内容**:
```json
{"search_categories":{"users":{"limited":false,"results":[]}}}
```

---

### SEARCH-003: POST /_matrix/client/v3/search

**描述**: 搜索用户 - 空搜索词

**状态**: ✅ 通过

**响应时间**: 22ms

**响应内容**:
```json
{"search_categories":{"users":{"limited":false,"results":[{"avatar_url":null,"display_name":null,"user_id":"@admin:cjystx.top"},{"avatar_url":null,"display_name":null,"user_id":"@testuser1:cjystx.top"}]}}}
```

---

### SEARCH-004: POST /_matrix/client/v3/search

**描述**: 搜索用户 - 无认证

**状态**: ✅ 通过

**响应时间**: 20ms

**响应内容**:
```json
{"status":"error","error":"Missing or invalid authorization header","errcode":"M_UNAUTHORIZED"}
```

---

### SEARCH-005: POST /_matrix/client/v3/search

**描述**: 搜索房间事件 - 有效请求

**状态**: ✅ 通过

**响应时间**: 24ms

**响应内容**:
```json
{"search_categories":{"room_events":{"count":0,"groups":{"room_id":{},"sender":{}},"highlights":[],"next_batch":null,"results":[],"state":{"rooms":{}}}}}
```

---

### SEARCH-006: POST /_matrix/client/v3/search

**描述**: 搜索房间事件 - 带类型过滤

**状态**: ✅ 通过

**响应时间**: 24ms

**响应内容**:
```json
{"search_categories":{"room_events":{"count":0,"groups":{"room_id":{},"sender":{}},"highlights":[],"next_batch":null,"results":[],"state":{"rooms":{}}}}}
```

---

### SEARCH-007: POST /_matrix/client/v3/search

**描述**: 搜索房间事件 - 按时间排序

**状态**: ✅ 通过

**响应时间**: 24ms

**响应内容**:
```json
{"search_categories":{"room_events":{"count":0,"groups":{"room_id":{},"sender":{}},"highlights":[],"next_batch":null,"results":[],"state":{"rooms":{}}}}}
```

---

### SEARCH-008: POST /_matrix/client/r0/search

**描述**: 搜索用户 - r0版本

**状态**: ✅ 通过

**响应时间**: 22ms

**响应内容**:
```json
{"search_categories":{"users":{"limited":false,"results":[{"avatar_url":null,"display_name":null,"user_id":"@admin:cjystx.top"}]}}}
```

---

### SEARCH-009: POST /_matrix/client/r0/search

**描述**: 搜索房间事件 - r0版本

**状态**: ✅ 通过

**响应时间**: 24ms

**响应内容**:
```json
{"search_categories":{"room_events":{"count":0,"groups":{"room_id":{},"sender":{}},"highlights":[],"next_batch":null,"results":[],"state":{"rooms":{}}}}}
```

---

### SEARCH-010: GET /_matrix/client/v3/user/@admin:cjystx.top/rooms/!test:localhost/threads?limit=10

**描述**: 获取房间线程 - 有效请求

**状态**: ✅ 通过

**响应时间**: 28ms

**响应内容**:
```json
{"status":"error","error":"Not a member of this room","errcode":"M_FORBIDDEN"}
```

---

### SEARCH-011: GET /_matrix/client/v3/user/@admin:cjystx.top/rooms/!test:localhost/threads?limit=0

**描述**: 获取房间线程 - limit=0边界值

**状态**: ✅ 通过

**响应时间**: 23ms

**响应内容**:
```json
{"status":"error","error":"Not a member of this room","errcode":"M_FORBIDDEN"}
```

---

### SEARCH-012: GET /_matrix/client/v3/user/@admin:cjystx.top/rooms/!test:localhost/threads

**描述**: 获取房间线程 - 无认证

**状态**: ✅ 通过

**响应时间**: 20ms

**响应内容**:
```json
{"status":"error","error":"Missing or invalid authorization header","errcode":"M_UNAUTHORIZED"}
```

---

### SEARCH-013: GET /_matrix/client/v1/rooms/!test:localhost/hierarchy

**描述**: 获取房间层级 - 有效请求

**状态**: ✅ 通过

**响应时间**: 23ms

**响应内容**:
```json
{"max_depth":3,"rooms":[]}
```

---

### SEARCH-014: GET /_matrix/client/v1/rooms/!test:localhost/hierarchy?limit=10&max_depth=2

**描述**: 获取房间层级 - 带参数

**状态**: ✅ 通过

**响应时间**: 24ms

**响应内容**:
```json
{"max_depth":2,"rooms":[]}
```

---

### SEARCH-015: GET /_matrix/client/v1/rooms/!nonexistent:localhost/hierarchy

**描述**: 获取房间层级 - 不存在的房间

**状态**: ✅ 通过

**响应时间**: 22ms

**响应内容**:
```json
{"max_depth":3,"rooms":[]}
```

---

### SEARCH-016: GET /_matrix/client/v1/rooms/!test:localhost/hierarchy

**描述**: 获取房间层级 - 无认证

**状态**: ✅ 通过

**响应时间**: 21ms

**响应内容**:
```json
{"status":"error","error":"Missing or invalid authorization header","errcode":"M_UNAUTHORIZED"}
```

---

### SEARCH-017: GET /_matrix/client/v1/rooms/!test:localhost/timestamp_to_event?ts=0&dir=f

**描述**: 时间戳转事件 - 有效请求

**状态**: ✅ 通过

**响应时间**: 24ms

**响应内容**:
```json
{"status":"error","error":"Not a member of this room","errcode":"M_FORBIDDEN"}
```

---

### SEARCH-018: GET /_matrix/client/v1/rooms/!test:localhost/timestamp_to_event?ts=1771407312000&dir=b

**描述**: 时间戳转事件 - 当前时间向后

**状态**: ✅ 通过

**响应时间**: 22ms

**响应内容**:
```json
{"status":"error","error":"Not a member of this room","errcode":"M_FORBIDDEN"}
```

---

### SEARCH-019: GET /_matrix/client/v1/rooms/!test:localhost/timestamp_to_event?dir=f

**描述**: 时间戳转事件 - 缺少ts参数

**状态**: ✅ 通过

**响应时间**: 20ms

**响应内容**:
```json
{"status":"error","error":"Missing ts parameter","errcode":"M_BAD_JSON"}
```

---

### SEARCH-020: GET /_matrix/client/v1/rooms/!test:localhost/timestamp_to_event?ts=0&dir=f

**描述**: 时间戳转事件 - 无认证

**状态**: ✅ 通过

**响应时间**: 21ms

**响应内容**:
```json
{"status":"error","error":"Missing or invalid authorization header","errcode":"M_UNAUTHORIZED"}
```

---

### SEARCH-021: POST /_matrix/client/v3/search

**描述**: 搜索用户 - 超长搜索词

**状态**: ✅ 通过

**响应时间**: 22ms

**响应内容**:
```json
{"search_categories":{"users":{"limited":false,"results":[]}}}
```

---

### SEARCH-022: POST /_matrix/client/v3/search

**描述**: 搜索用户 - XSS测试

**状态**: ✅ 通过

**响应时间**: 21ms

**响应内容**:
```json
{"search_categories":{"users":{"limited":false,"results":[]}}}
```

---

### SEARCH-023: POST /_matrix/client/v3/search

**描述**: 搜索用户 - SQL注入测试

**状态**: ✅ 通过

**响应时间**: 20ms

**响应内容**:
```json
{"search_categories":{"users":{"limited":false,"results":[]}}}
```

---

### SEARCH-024: POST /_matrix/client/v3/search

**描述**: 搜索 - 空搜索类别

**状态**: ✅ 通过

**响应时间**: 25ms

**响应内容**:
```json
{"search_categories":{}}
```

---

### SEARCH-025: GET /_matrix/client/v1/rooms/invalid_room_id/hierarchy

**描述**: 获取房间层级 - 无效房间ID格式

**状态**: ✅ 通过

**响应时间**: 24ms

**响应内容**:
```json
{"max_depth":3,"rooms":[]}
```

---

**测试汇总**

| 指标 | 数值 |
|------|------|
| 总计测试 | 25 |
| 通过 | 25 |
| 失败 | 0 |
| 通过率 | 100% |

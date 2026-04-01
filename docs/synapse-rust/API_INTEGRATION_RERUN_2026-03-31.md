# API 集成测试复测诊断报告

> 日期: 2026-03-31
> 执行人: Trae IDE Agent
> 目标环境: Docker Compose 本地后端
> 服务地址: `http://localhost:28008`
> 执行脚本: `/Users/ljf/Desktop/hu/synapse-rust/scripts/test/api-integration_test.sh`
> 执行参数: `TEST_ENV=dev SERVER_URL=http://localhost:28008`

## 1. 执行摘要

- 后端服务已通过 Docker Compose 成功启动
- 健康检查、版本接口、Admin Nonce 接口均返回 200
- 管理员账号 `admin` 已验证可登录，用户 ID 为 `@admin:cjystx.top`
- `/_synapse/admin/v1/users?from=0&limit=1` 已验证可访问
- API 集成脚本执行结果: 447 通过 / 32 失败 / 61 跳过
- 原始脚本退出码: `1`

## 2. 启动与账号校验

### 2.1 服务校验

- `GET /health` → 200
- `GET /_matrix/client/versions` → 200
- `GET /_synapse/admin/v1/register/nonce` → 200

### 2.2 管理员校验

- 登录账号: `admin`
- 登录密码: 使用脚本当前默认值 `Wzc@9890951`
- 登录结果: 成功获取 access token
- Admin 接口校验: `GET /_synapse/admin/v1/users?from=0&limit=1` 成功返回用户列表

结论:

- 本轮失败不是由服务未启动、管理员账号失效或 Admin 权限缺失导致

## 3. 原始产物

- 标准输出: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-064855/stdout.log`
- 通过清单: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-064855/api-integration.passed.txt`
- 失败清单: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-064855/api-integration.failed.txt`
- 跳过清单: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-064855/api-integration.skipped.txt`

## 4. 失败归因

### 4.1 P0: `rooms.member_count` 契约漂移

代表复现:

```json
{
  "errcode": "M_UNKNOWN",
  "error": "Internal error: Failed to create room: Internal error: Failed to create room: error returned from database: column \"member_count\" of relation \"rooms\" does not exist"
}
```

直接影响:

- `Create Test Room`
- `Create Second Room`
- `Get Room State`
- `Send Message`
- `Room Messages`
- `Joined Members`
- 多个 `Room Summary*`
- 多个 `Admin Room*`

代码与 schema 对照:

- 统一 schema 已明确移除 `rooms.member_count`，改由 `room_summaries.member_count` 承载，见 [00000000_unified_schema_v6.sql:L175-L193](file:///Users/ljf/Desktop/hu/synapse-rust/migrations/00000000_unified_schema_v6.sql#L175-L193)
- 当前建房与取房逻辑仍直接访问 `rooms.member_count`，见 [room.rs:L83-L96](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/room.rs#L83-L96) 与 [room.rs:L117-L143](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/room.rs#L117-L143)

结论:

- 这是 Matrix Client-Server 核心房间链路阻断问题

### 4.2 P0: `rooms.encryption` 契约漂移

代表复现:

```json
{
  "errcode": "M_UNKNOWN",
  "error": "Internal error: Failed to get public rooms: error returned from database: column \"encryption\" does not exist"
}
```

管理员接口复现:

```json
{
  "errcode": "M_UNKNOWN",
  "error": "Internal error: Database error: error returned from database: column r.encryption does not exist"
}
```

直接影响:

- `Public Rooms`
- `Admin List Rooms`
- `Admin Room Details`
- `Admin Get Room`

代码与 schema 对照:

- `RoomStorage::get_public_rooms` 仍查询 `encryption` 与 `member_count`，见 [room.rs:L214-L253](file:///Users/ljf/Desktop/hu/synapse-rust/src/storage/room.rs#L214-L253)
- 公共房间路由依赖 `room_storage.get_public_rooms`，见 [directory.rs:L120-L154](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/directory.rs#L120-L154)

结论:

- 这是 Matrix 公共目录能力与 Synapse Admin 房间视图的双重兼容回归

### 4.3 P1: `registration_tokens.uses_allowed` 契约漂移

代表复现:

```json
{
  "errcode": "M_UNKNOWN",
  "error": "Internal error: Database error: error returned from database: column \"uses_allowed\" does not exist"
}
```

代码与 schema 对照:

- 统一 schema 当前字段为 `max_uses`、`uses_count`、`expires_at`，见 [00000000_unified_schema_v6.sql:L1906-L1931](file:///Users/ljf/Desktop/hu/synapse-rust/migrations/00000000_unified_schema_v6.sql#L1906-L1931)
- Admin Token 路由仍直接读写 `uses_allowed`、`pending`、`completed`、`expiry_time`，见 [token.rs:L67-L200](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/admin/token.rs#L67-L200)

结论:

- 这是 Synapse Admin 兼容层字段映射未对齐导致的问题

### 4.4 失败并非 32 个独立缺陷

- 32 个失败集中在 3 组主因
- `Room Summary`、`Admin List Rooms`、`Admin Room Details` 等用例在脚本中重复出现
- 当前失败统计包含大量由建房失败触发的级联失败

## 5. 跳过项分析

61 个跳过项可分为六类:

| 分类 | 数量 | 典型项 | 说明 |
|------|------|--------|------|
| `endpoint not available` | 49 | `OpenID Userinfo`、`Admin Sessions`、`Federation Backfill` | 路由未接入、脚本路径未命中，或能力未开放 |
| `not implemented` | 6 | `List Presences`、`Create DM`、`Admin Room Search` | 脚本显式按未实现处理 |
| 前置数据缺失 | 2 | `Redact Event (no event to redact)`、`Update Direct Room (no DM room)` | 上游动作未产生所需数据 |
| schema 级联 | 1 | `Get Room Version (room not found - schema issue)` | 建房失败后的保护性跳过 |
| 外部依赖 | 1 | `Admin Federation Rewrite (requires federation destination data)` | 需要联邦上下文 |
| 行为或断言差异 | 2 | `Refresh Token (endpoint behavior)`、`Space State (endpoint returns empty or not found)` | 接口语义与脚本判定不完全一致 |

结论:

- 跳过不等于合规
- 当前脚本需要把“未实现”“需配置”“前置失败”“断言差异”拆开统计

## 6. Matrix / Synapse 合规差异

### 6.1 Matrix Client-Server 核心能力差异

- `createRoom` 失败，说明房间生命周期入口已受损
- `publicRooms` 失败，说明公共房间目录能力不可用
- 多个 `room summary` 失败，说明房间发现与摘要能力受损

这类问题影响 Matrix 客户端基础能力，不应视为脚本噪音。

### 6.2 Synapse Admin 兼容差异

- `registration_tokens` 仍按旧字段模型实现
- 部分 Admin Room 查询仍隐式依赖旧 `rooms` 列结构

这类问题主要影响 Synapse Admin API 兼容和运维可用性。

### 6.3 脚本通过率不能直接代表合规度

- 当前统计混合了 Matrix Client API、Synapse Admin API、外部依赖型接口
- 同时存在“假通过”“级联失败”“能力未接入”三类噪音

更准确的结论应是:

- 当前服务具备较高覆盖面的 API 壳层
- 但 Matrix 核心房间链路与 Synapse Admin 兼容层存在明确结构性回归

## 7. 脚本断言问题

### 7.1 过宽断言

- `Media Download` 与 `Media Thumbnail` 使用 `grep -q ""`，几乎任意非空输出都会通过，见 [api-integration_test.sh:L433-L445](file:///Users/ljf/Desktop/hu/synapse-rust/scripts/test/api-integration_test.sh#L433-L445)

### 7.2 仅用关键字判断成功

- `Get Filter` 只匹配 `room|filter`，无法判断 HTTP 状态与响应结构，见 [api-integration_test.sh:L833-L838](file:///Users/ljf/Desktop/hu/synapse-rust/scripts/test/api-integration_test.sh#L833-L838)
- `Request OpenID Token` 只匹配 `access_token|token`，未区分真实成功与兼容错误，见 [api-integration_test.sh:L853-L856](file:///Users/ljf/Desktop/hu/synapse-rust/scripts/test/api-integration_test.sh#L853-L856)
- `Refresh Token` 失败后直接按 `endpoint behavior` 跳过，缺少错误码分类，见 [api-integration_test.sh:L1421-L1426](file:///Users/ljf/Desktop/hu/synapse-rust/scripts/test/api-integration_test.sh#L1421-L1426)

### 7.3 假通过案例

- `Upload Keys`、`Create Key Backup`、`Get Key Backup` 等接口返回数据库错误文本，但脚本仍按关键字判为通过，本轮日志中已观察到多次 `M_UNKNOWN` 被记为 PASS

结论:

- 当前脚本更接近“冒烟+关键字巡检”，还不是严格意义上的合规测试

## 8. 问题清单

| 优先级 | 问题 | 影响范围 | 建议 |
|--------|------|----------|------|
| P0 | `rooms.member_count` 契约漂移 | 建房、房间状态、消息、摘要、Admin Room | 统一切换到 `room_summaries.member_count` 或补兼容层 |
| P0 | `rooms.encryption` 契约漂移 | `publicRooms`、Admin Room 列表与详情 | 改从状态事件或 `room_summaries.is_encrypted` 读取 |
| P1 | `registration_tokens.uses_allowed` 契约漂移 | Admin 注册令牌接口 | 在 Admin 层做字段映射，统一到当前 schema |
| P1 | 级联失败放大统计 | 报告可读性与优先级判断 | 在主前置失败后折叠后续依赖用例 |
| P1 | 断言过宽导致假通过 | 测试可信度 | 统一校验 HTTP 状态码、`errcode`、响应 schema |
| P2 | 跳过项分类粗糙 | 合规分析失真 | 为 skip 增加结构化原因标签 |

## 9. 优化交付物

### 9.1 已交付

- 后端启动与管理员账号校验结果
- 本轮原始测试产物目录
- 失败根因分析
- 跳过项分类
- Matrix / Synapse 合规差异说明
- 优先级问题清单

### 9.2 建议后续交付顺序

1. 修复 `RoomStorage` 与统一 schema 的字段契约
2. 修复 `publicRooms` 与 `Admin Room` 的读取链路
3. 修复 `registration_tokens` Admin 路由字段映射
4. 为集成测试脚本增加 HTTP 状态码与错误码断言
5. 将测试报告拆分为 Matrix Core / Synapse Admin / Optional Features 三个维度

## 10. 修复后复测结果

在完成契约漂移修复并重新构建 Docker 镜像后，API 集成脚本已实现 0 失败退出。

### 10.1 复测结果

- 486 passed / 0 failed / 54 skipped
- 脚本退出码: `0`

### 10.2 原始产物

- 标准输出: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-074802/stdout.log`
- 通过清单: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-074802/api-integration.passed.txt`
- 失败清单: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-074802/api-integration.failed.txt`
- 跳过清单: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-074802/api-integration.skipped.txt`

### 10.3 生效修复点

- `rooms.member_count` / `rooms.encryption`：运行时代码不再依赖 `rooms` 冗余列，改为从 `room_summaries` 推导
- `registration_tokens`：Admin 路由字段映射对齐现行表结构并保持对外兼容字段
- `events.processed_ts`：运行查询改为使用数据库现有列 `processed_at`

## 11. 脚本增强后复测结果

在增强脚本断言（避免假通过）并修复因此暴露的 E2EE 与媒体用例后，API 集成脚本仍保持 0 失败退出，同时通过数增加。

### 11.1 复测结果

- 487 passed / 0 failed / 53 skipped
- 脚本退出码: `0`

### 11.2 原始产物

- 标准输出: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-081249/stdout.log`
- 通过清单: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-081249/api-integration.passed.txt`
- 失败清单: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-081249/api-integration.failed.txt`
- 跳过清单: `/Users/ljf/Desktop/hu/synapse-rust/test-results/api-integration-20260331-081249/api-integration.skipped.txt`

### 11.3 生效优化点

- 修复 `Media Download/Thumbnail`：纠正 `mxc://` 解析并上传真实 PNG 数据，避免误判与无效用例
- 修复 `E2EE Keys`：用 HTTP 状态码+JSON 字段校验替代 `curl && pass`，并修复后端 device_keys 写入契约漂移
- 修复 `Refresh Token`：使用登录返回的真实 `refresh_token` 做闭环验证，提升覆盖率

### 11.4 最新跳过项复核

基于最新产物 `api-integration.skipped.txt`，53 个跳过项中已有一部分可明确判定为“脚本问题”而非“后端未实现”。

#### 11.4.1 脚本误配导致的代表项

- `Get Presence List`：后端已实现 `GET /_matrix/client/v3/presence/list/{user_id}`，脚本却仍调用 `POST /_matrix/client/v3/presence/list`
- `Get Thread`：脚本直接将 `THREAD_ID` 设为 `ROOM_ID`，未先制造真实线程上下文
- `Server Key Query`：脚本缺少 `key_id` 路径段
- `Friend Request` / `Incoming Friend Requests`：后端同时暴露了 `v3` 与 `v1/r0` 路径，当前跳过更可能来自脚本变量/断言或前置数据问题，而不是“后端未实现”
- `Admin User Tokens`：脚本请求 `/users/{user_id}/login`，真实接口是 `/users/{user_id}/tokens`
- `Admin Rate Limit`：脚本请求 `ratelimit`，真实接口是 `rate_limit`
- `Admin Media`：脚本请求 `/media/stats`，而当前后端暴露的是 `/_synapse/admin/v1/media` 与 `/_synapse/admin/v1/media/quota`

#### 11.4.2 依赖测试种子或上下文的代表项

- `Space State`：依赖稳定的 space 状态事件与成员上下文
- `Federation State` / `Federation Backfill` / `OpenID Userinfo`：需要联邦认证或专用上下文，不能用普通客户端 Bearer Token 直接判定
- `Admin Federation Rewrite`：需要存在联邦目的服务器数据

#### 11.4.3 当前确属实现缺口的代表项

- `/_synapse/admin/v1/devices`
- `/_synapse/admin/v1/auth`
- `/_synapse/admin/v1/capabilities`
- `/_synapse/admin/v1/rooms/{room_id}/shares`
- `/_synapse/admin/v1/users/count`
- `/_synapse/admin/v1/rooms/count`
- `/_synapse/admin/v1/rooms/{room_id}/pending_joins`

#### 11.4.4 结论

- 最新结果已证明主链路无失败，但跳过项仍混杂了脚本误配、缺少上下文和真实实现缺口
- 后续优化应优先清理脚本误配项，再决定是否补齐剩余 Admin 扩展端点

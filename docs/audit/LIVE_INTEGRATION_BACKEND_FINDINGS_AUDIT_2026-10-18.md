# 联调报告「后端问题」复核（2026-10-18）

> 复核对象：`matrix-js-sdk/artifacts/live-integration-test-report-2026-10-17.md` §3 的
> 后端条目 **I-01…I-05**，以及 **I-07** 的后端一侧。
> 环境：`https://matrix.test`（本机部署，`synapse-app` / `synapse-postgres` 运行中）。
> 后端基线：`deecdb603`（报告基于 `2946e19eb`，两者之间只有两笔文档提交，代码未变）。
> 方法：**代码定位 + 活体探测 + DB 对拍**，不采信报告结论（含其"已核实无问题"清单）。
>
> 结论：**5 条中 4 条成立、1 条部分成立**（I-03 的状态码与 errcode 错配成立，但"不限流"
> **不成立**——实测第 6 次即 429）；另有 2 条报告未提、与 I-01/I-02 同源的事实。

## 1. 判定总表

| # | 报告结论 | 复核判定 | 关键证据（本轮实测） |
| --- | --- | --- | --- |
| **I-01** | 投影表 `room_summary_members` 未随 membership 维护 | ✅ **成立，且比报告更严重**（双向都错，另有 kick/ban 缺口） | 活体：join 后投影无行、`/summaries` 看不到；leave 后 `/summaries` **仍返回**该房间。DB：多余 71/49/14 条（三个用户），**缺失** 3 条 |
| **I-02** | 裸 `Json<T>` 的错误响应缺 `errcode` | ✅ **成立，规模被低估约 4 倍** | 裸 `Json` **254** 处（报告 ≥60）/ `MatrixJson` **22** 处（报告 17）；活体 415/400/400/422 全部 `text/plain` |
| **I-03** | 登录失败 `401 + M_FORBIDDEN` 且**连续失败不限流** | ⚠️ **部分成立**：401 成立（且被测试钉死）；**"不限流"不成立** | 实测第 1–5 次 401，**第 6 次 429 `retry_after_ms=300000`**；配置 `login_failure_lockout_threshold: 5` |
| **I-04** | `createRoom` 对字段类型错误静默接受 | ✅ 成立 | `{"name":123}` → **200 建房**，`m.room.name` 查询 `M_NOT_FOUND` |
| **I-05** | `INNER JOIN` 把"投影缺失"放大成静默不可见 | ✅ 成立（与 I-01 同源） | `repository.rs:237`；活体已证"投影无行 ⇒ 房间永不出现"，无告警 |
| **I-07**（后端侧） | 线程创建接受 `content` 但丢弃 | ✅ 成立 | `thread.rs:24-26` 映射到 `_content`；活体传 `content.name` → 200 建线程，详情无 `content`/`name` |

## 2. 逐条复核

### I-01 成立；错的是**双向**，且 kick/ban 也没有维护

**代码事实**

| 路径 | 是否写 `room_summary_members` | 位置 |
| --- | --- | --- |
| 本地 `join_room` | ❌ **不写** | `room/membership/actions.rs:137-155` 只有 `member_storage.add_member`（= `room_memberships`）与 `increment_member_count`（= `room_summaries.member_count`） |
| 本地 `leave_room` | ❌ 不删/不改 | `actions.rs:231-244` 调 `remove_member`，而它只是 `UPDATE room_memberships SET membership='leave'`（`membership/mod.rs:271-290`） |
| 本地 `invite_user` | ✅ 写（best-effort，不在事务内） | `room/membership/moderation.rs:72-87` |
| 入站联邦 invite | ✅ 写（best-effort） | `moderation.rs:149-166` |
| 联邦 join / leave | ✅ 写 | `routes/federation/membership/{join,leave}.rs` |
| **`kick` / `ban` / `unban`** | ❌ **全无** | `moderation.rs` 内 `room_summary` 仅出现在上面两处 invite |

**报告的一处误读**：`actions.rs:127` 的注释写着 "bind the membership write and the summary
refresh in one transaction"，但它指的是 `increment_member_count`（**聚合行**的计数），
**不是投影行**。所以就"投影行"而言，`join_room` 确实一次都没写——报告这一条的判断是对的，
只是注释本身也在误导。

**活体复现（本轮新建房间 `!LaplDuMG…`，全程可重放）**

| 步骤 | 结果 |
| --- | --- |
| `testuser1` 建 `public_chat` 房间 | 200 |
| `sdk_testuser2` join | 200，`room_memberships` 出现 `sdk_testuser2 → join` |
| 同房间的 `room_summary_members` | **只有 `testuser1`**（join 未写投影） |
| `sdk_testuser2` 的 `/summaries` | 返回 34 个房间，**不含**刚加入的这个 |
| `testuser1` leave | 200，`room_memberships` 变 `leave` |
| `testuser1` 的 `/summaries` | 返回 77 个房间，**仍包含**已离开的这个 |

**DB 全量对拍**（`room_summary_members` 8069 行）

| 方向 | 用户 | 条数 |
| --- | --- | --- |
| 投影有、真实无（leave 残留） | `@testuser1` / `@sdk_testuser` / `@testuser2` | **71 / 49 / 14** |
| 真实有、投影无（join 未写） | `@sdk_testuser2`（含本轮造的 1 条）/ `@batchb-user2` | **2 / 1** |

⇒ 报告只统计了 leave 方向（`@sdk_testuser` 49 条），**join 方向同样成立**，且历史数据里
已经存在 3 条"加入但投影没有"的行（不是本轮造成的新现象）。

### I-02 成立；规模比报告大 4 倍，且只在**已认证**请求上暴露

**代码事实**：`routes/extractors/json.rs` 的 `MatrixJson` 已把 `JsonRejection` 正确映射
（`JsonDataError → 400 M_BAD_JSON`；`JsonSyntaxError` / `MissingJsonContentType → 400 M_NOT_JSON`），
但**没有任何全局兜底**——`JsonRejection` 在整个 `synapse-web` 里只在这一个文件出现。

规模实测（`synapse-web/src/routes/**` 的提取器签名）：

| 项 | 实测 | 报告 |
| --- | --- | --- |
| 裸 `Json(x): Json<T>` 提取器 | **254 处 / 82 个文件** | ≥60 |
| `MatrixJson<…>` | **22 处** | 17 |

**活体（携带 `@sdk_testuser2` 的 token）**

| 请求 | 状态码 | Content-Type | Body |
| --- | --- | --- | --- |
| `POST /createRoom`，`Content-Type: text/plain` | **415** | `text/plain` | `Expected request with \`Content-Type: application/json\`` |
| `POST /createRoom`，坏 JSON | **400** | `text/plain` | `Failed to parse the request body as JSON: key must be a string …` |
| `POST /createRoom`，空 body | **400** | `text/plain` | `… 解析到结尾时值不完整 …` |
| `POST /_synapse/room_summary/v1/summaries/batch`，未知字段 | **422** | `text/plain` | `… unknown field \`room_ids\`, expected \`rooms\` or \`suggested_only\``（反序列化失败原文） |
| 对照：`POST /login`，坏 JSON | 400 | `application/json` | `{"errcode":"M_NOT_JSON", …}` ✅ |

**补充一个报告没写的边界**：**无 token** 时三个请求都先返回 401（提取器排在 auth 之后），
所以这个缺陷只影响"认证通过 + body 不合法"的组合——而那正是客户端最常踩的路径
（例如把对象序列化成字符串、或用了错误的字段名）。

### I-03 部分成立：「401 + M_FORBIDDEN」成立，「不限流」不成立

**成立的部分**：实测 1–5 次均为 `401 {"errcode":"M_FORBIDDEN","error":"Invalid credentials"}`。
而 Matrix 规范（`matrix-spec/data/api/client-server/login.yaml`）对该端点明确规定：
**403 `M_FORBIDDEN`** —— "The login attempt failed"（另有 400 / 429）。
⇒ 状态码与规范不符：401 在客户端语义里是"凭证/token 失效"，会让通用 401 处理误伤登录接口。

**不成立的部分（需要更正登记）**：报告称"连续 3 次失败**不限流**（无 429/retry_after_ms）"。
实测**第 6 次**即：

```text
429 {"errcode":"M_LIMIT_EXCEEDED","error":"Rate limited","retry_after_ms":300000}
```

配置也印证：`docker/config/homeserver.yaml` 的 `security.login_failure_lockout_threshold: 5`
（按 `(ip, username)` 对）、`login_lockout_max_ip_attempts: 30`（按 IP 聚合）、
`login_lockout_duration_seconds: 300`。实现见 `synapse-web/src/utils/login_lockout.rs`。
报告只跑了 3 次（< 阈值 5），所以看不到退避——**这是取样不足，不是产品缺陷**。

> 另外要提醒：本地 dev 配置把 `127.0.0.1/32`、`::1/128` 放进了
> `login_lockout_ip_allowlist`。该名单**只豁免锁定、不豁免密码校验**，所以本轮仍能触发
> `(ip, username)` 维度的锁定；但如果将来只按 IP 聚合判定，本机联调会永远看不到锁定。

**比报告更严重的一点：错行为被单元测试钉死了。**

- `synapse-services/src/auth/login.rs:53` 注释："P-007 fix: Matrix spec requires HTTP 401 + M_FORBIDDEN"——与规范原文**相反**；
- `synapse-services/src/auth/tests.rs:742-748` 断言 `kind == Unauthorized` **且** `code == Forbidden`；
- 同一文件 `:767`、`:779` 还断言停用用户/无密码用户也是 401。

⇒ 改状态码必须同步改这三处断言与注释，否则"修了反而红"。这也是该缺陷能长期存活的原因
（守卫镜像了实现，而不是镜像规范）。

### I-04 成立；与 I-02 同一根因（缺强类型 body）

`create.rs:79` 是 `Json(body): Json<Value>`，`:111` 用 `body.get("name").and_then(|v| v.as_str())`
取值，非字符串直接跳过。活体：`{"name":123,"preset":"private_chat"}` → **200** 建房，
随后 `GET /rooms/{id}/state/m.room.name` → `404 M_NOT_FOUND`（确认未落库）。

同源但报告未提：**未知字段同样被静默接受**（不返回 `M_BAD_JSON`）。根因一致——
`Json<Value>` 既不做类型校验，也不做未知字段拒绝。

### I-05 成立（与 I-01 同源，不是独立缺陷）

`room_summary/repository.rs:237` 是 `INNER JOIN room_summary_members rsm`，
所以"投影没有行"等于"房间永远不出现"，且无告警、无回退——本轮的 `/summaries` 活体
（join 后返回 34 个房间但不含新房间）正是这个放大效应。

报告建议的"`LEFT JOIN` + 告警"只能在**暂不修 I-01** 时作为可观测手段；
不建议用它替代修复（会绕过 summary 既定的聚合口径）。

### I-07 后端侧成立：`content` 接受即丢弃

`handlers/thread.rs:20-28` 的 `CreateThreadBody` 把 `content` 映射到 `_content`（下划线前缀
⇒ serde 接受但从不读取），类型还是 `Value`（传错也不报）。活体：带
`content: {"name":"我的线程标题"}` 建线程 → 200，`thread_id` 正常返回，但
`GET /_matrix/client/v1/rooms/{room}/threads/{id}` 的 `root` 对象里**没有** `content`/`name`。

这是"SDK 暴露了参数、后端接受后丢弃"的经典形态：**门禁抓不到**（键集合法、无 4xx）。
定性上更偏"契约未定义"而非"后端 bug"——要么后端落库，要么 SDK 移除/报错，两者选一。

## 3. 报告未提，但复核时确认的两点

1. **`createRoom` 未知字段静默**（见 I-04 末段）——与 I-02 同根因，修强类型 body 时一起解决。
2. **不要用 `/rooms/{id}/info` 判断 `createRoom` 是否落了 `name`/`topic`**：该端点不返回这两个
   键（返回 `name: None`），但 `m.room.topic` / `m.room.name` 状态事件是正常写入的
   （本轮实测 `GET /rooms/{id}/state/m.room.topic` → `{"topic":"i04-probe"}`）。
   ⇒ 这是报告 §7.1 那条方法学教训的又一个实例：**判"没写入"必须查状态事件或 DB，不能查聚合视图**。

## 4. 修复建议（按优先级，附判据）

| # | 动作 | 判据 |
| --- | --- | --- |
| **1** | **I-01**：`RoomSummaryStoreApi` 补 `remove_member`（实现已有，`repository.rs:472`，只是没进 trait）+ 事务版；`join` / `leave` / `kick` / `ban` 在同一事务内写/删投影（复用 `add_member_in_tx`）；提供一次性回填（`INSERT … SELECT` + 删除 `membership='leave'` 的行） | 活体：join → `/summaries` 可见；leave → 不可见。DB：`投影(join∪invite) EXCEPT 真实` 与 `真实 EXCEPT 投影` 两向均为 0 |
| **2** | **I-02**：把 254 处提取器换成 `MatrixJson`（脚本化替换 + 编译期兜底），或先加一个 `map_response` 兜住 `text/plain` 的 400/415/422；再加契约测试（对每个含 body 的端点发一次 `text/plain`，断言 `content-type: application/json` 且 `errcode` 非空） | 契约测试常驻；抽查 415/400/422 三态均返回 Matrix 错误体 |
| **3** | **I-03**：`ApiError::invalid_credentials` 改为 **403 M_FORBIDDEN**（与规范/上游同族）；同步改 `login.rs:53` 注释与 `auth/tests.rs:742/767/779` 三处断言。**不要动锁定逻辑**（它是对的） | 实测 403 + `M_FORBIDDEN`；第 6 次仍 429 |
| **4** | **I-04**：`CreateRoomBody` 强类型（至少 `name` / `topic` / `preset` / `visibility` / `invite` 五个高频键），未知字段按 spec 拒绝 | `{"name":123}` → 400 `M_BAD_JSON`；原本合法的请求仍 200 |
| **5** | **I-05**：随 I-01 消解；若要过渡，加"投影缺失条数"指标而非改 JOIN | 指标可查、能告警 |

## 5. 复现命令（最小集）

```bash
B=https://matrix.test
# ① I-02（需先拿一个有效 token）
TOK=$(curl -s -X POST $B/_matrix/client/v3/login -H 'Content-Type: application/json' \
  -d '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"sdk_testuser2"},"password":"Test@123"}' \
  | python3 -c 'import json,sys;print(json.load(sys.stdin)["access_token"])')
curl -si -X POST $B/_matrix/client/v3/createRoom -H "Authorization: Bearer $TOK" \
  -H 'Content-Type: text/plain' --data-binary x | head -5          # → 415 text/plain
curl -si -X POST $B/_synapse/room_summary/v1/summaries/batch -H "Authorization: Bearer $TOK" \
  -H 'Content-Type: application/json' --data-binary '{"room_ids":[]}' | head -5  # → 422 text/plain

# ② I-04
curl -s -X POST $B/_matrix/client/v3/createRoom -H "Authorization: Bearer $TOK" \
  -H 'Content-Type: application/json' --data-binary '{"name":123,"preset":"private_chat"}'   # → 200

# ③ I-03（第 6 次才 429；1–5 次为 401）
for i in $(seq 1 6); do curl -s -o /dev/null -w "$i:%{http_code} " -X POST $B/_matrix/client/v3/login \
  -H 'Content-Type: application/json' \
  -d '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"sdk_testuser"},"password":"WRONG"}'; done; echo

# ④ I-01（DB 对拍；两向都必须为 0 才算修好）
psql "$DATABASE_URL" -tAc "
WITH s AS (SELECT user_id, room_id FROM room_summary_members WHERE membership IN ('join','invite')),
     m AS (SELECT user_id, room_id FROM room_memberships      WHERE membership IN ('join','invite'))
SELECT (SELECT count(*) FROM (SELECT room_id,user_id FROM s EXCEPT SELECT room_id,user_id FROM m) a) AS stale_in_projection,
       (SELECT count(*) FROM (SELECT room_id,user_id FROM m EXCEPT SELECT room_id,user_id FROM s) b) AS missing_in_projection;"
```

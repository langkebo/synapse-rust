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

## 6. 执行记录（2026-10-18，按 §4 优先级落地）

> 状态：**I-01…I-05、I-07 全部已改代码并补回归用例**（I-05 随 I-01 消解）；
> **测试/调试阶段限流与登录锁定整体关闭**（见 §6.5），正式部署前需显式恢复。

### 6.1 I-01 投影：同事务写入 + 一次性回填

| 动作 | 位置 |
| --- | --- |
| `join` 落投影（有事务走 `add_member_in_tx`，无 pool 的测试环境降级 best-effort） | `room/membership/actions.rs`（`join_room`） |
| `leave` / `leave+forget` 删投影（同一事务） | `actions.rs::remove_summary_member_projection` |
| `kick` / `ban` / `unban` 删投影 | `room/membership/moderation.rs` |
| 存储层事务版删除 + 同事务重算聚合计数 | `synapse-storage/src/room_summary/repository.rs::remove_member_in_tx`，trait 见 `api.rs` |
| 存量漂移一次性回填（补缺失 / 删残留 / 重算计数，三段独立 DML，幂等） | `migrations/00000000_unified_schema_v12.sql` 末尾 |

**基线指纹**：`16a162d3c01f87ce` → **`639e79110cb63fd3`**（236554 → 239515 字节）。
按 R10 第①条**先用改动前的实际常量自检哈希实现**：对 `HEAD:migrations/…v12.sql`（236554 字节）复算得
`16a162d3c01f87ce`，逐字节吻合后才取新值。

### 6.2 I-02 裸 `Json<T>` → `MatrixJson`

- 提取器位置 **289 处**已全部改为 `MatrixJson<…>`，`synapse-web/src/routes/**` 里**裸 `Json<` 提取器为 0**
  （统计口径：`: Json<` / `Option<Json<`，排除返回类型 `Result<Json<…>, _>`）。
- `extractors/json.rs` 的 `map_json_rejection` 同时驱动 `FromRequest` 与新增的
  `OptionalFromRequest`（保持 Axum "无 Content-Type ⇒ None" 语义）。
- **新增常驻契约测试** `tests/unit/matrix_json_contract_tests.rs`：对最小 router 打四种 body，
  断言 `content-type: application/json` + `errcode` 非空：

| 请求 | 修复前 | 修复后（本测试钉住） |
| --- | --- | --- |
| `Content-Type: text/plain` | 415 `text/plain` | 400 `M_NOT_JSON` |
| `Content-Type: application/json` + 坏 JSON | 400 `text/plain` | 400 `M_NOT_JSON` |
| 类型错误 / 未知字段 | 422 `text/plain` | 400 `M_BAD_JSON` |
| 合法 body | 200 | 200（未变） |

### 6.3 I-03 登录失败：403 同族 + 渐进退避

- `ApiError::invalid_credentials()` → **403 `M_FORBIDDEN`**（`STATUS_DIVERGENCE_WHITELIST` 中
  `(Unauthorized, Forbidden)` 条目同步删除）；`login.rs` 注释与 `auth/tests.rs` 四处断言同步改写。
- **渐进退避**：`retry_after_ms = min(base × 2^(档位-1), login_lockout_max_duration_seconds × 1000)`，
  新增配置 `security.login_lockout_max_duration_seconds`（默认 3600）。档位 = 连续失败次数 − 阈值
  （**下限 1**，即刚达阈值时仍是配置的锁定窗口），已锁定状态下每重试一次升一档，
  因此持续爆破的代价指数上升而不是每 5 分钟送 5 次免费尝试。
- 新用例：`lockout_backoff_escalates_then_caps`（1s→2s→…→封顶 4s，单调不减）。

### 6.4 「永久锁」的真正根因（比 §4 的 TTL 条目更深一层）

排查 P1 的「`CacheManager::set` 的 TTL」时发现**三个叠加缺陷**，只改第一个不足以修好：

| # | 缺陷 | 后果 | 修复 |
| --- | --- | --- | --- |
| a | `set` / `set_checked` 用 `local.set_raw`，忽略调用方 TTL | L1 一律按 builder TTL（**7200s**）过期 | 改 `set_raw_with_ttl`（`local_write`，`ttl=0` 保留"不过期"语义） |
| b | **moka 以 `RemovalCause::Replaced` 回调驱逐监听器**，而监听器无条件删 deadline | **覆盖写一次就丢掉 per-key TTL** ⇒ 300 秒锁定在 L1 实际存活 2 小时；每次失败又重写，等于永不过期 | 监听器跳过 `Replaced`（`drop_deadline_unless_replaced`），通用缓存与四个命名空间都改 |
| c | L1 从 Redis 回填时用 `set_raw`（builder TTL） | 一条只剩 60 秒的 L2 条目会在 L1 活 7200 秒 | 新增 `RedisCache::get_with_ttl` / `get_checked_with_ttl` / `get_batch_with_ttl`（`GET`+`PTTL` 同一次 pipeline），回填按剩余寿命写 |
| d | `set_not_found` / `set_batch_serialized` 同样忽略 TTL | 负缓存/批量写最多多活 40 倍 | 统一走 `local_write` |

**计数器改固定窗口**：新增 `CacheManager::increment_counter`（L1 侧 `increment_with_fixed_window`，
L2 侧 Lua `INCR` + 仅在 `count == 1` 时 `EXPIRE`，并返回 `PTTL` 供 L1 同步）。
`record_login_failure` 从 `get`+`set(ttl)` 改为它 ⇒ 窗口从**首次失败**起算、不再随重试顺延，
攻击者无法靠"到期前再试一次"把受害者永久锁死。新用例
`lockout_window_expires_even_under_continued_attempts` 用 1 秒窗口实测：锁定期间再打 4 次，1.1 秒后必须放行。

**回归守卫**（`synapse-cache`，93 个用例全绿）：
`set_honours_per_write_ttl_for_local_tier`、`overwriting_a_key_keeps_its_per_key_ttl`（钉住缺陷 b）、
`set_not_found_honours_per_write_ttl`、`set_batch_serialized_honours_per_entry_ttl`、
`increment_counter_is_a_fixed_window`。

### 6.5 测试阶段的开关（重要）

联调/压测阶段 429 会把真实缺陷伪装成"偶发失败"，因此**两条防护都显式关闭**，且都在启动时告警一次：

| 开关 | 位置 | 关闭后的行为 | 恢复 |
| --- | --- | --- | --- |
| `rate_limit.enabled: false` | `docker/config/rate_limit.yaml`（生效值）+ `docker/config/homeserver.yaml`（回退值） | 中间件对每个请求直接放行，`default`/`endpoints`/`sync` 全部不生效 | 两处改回 `true` |
| `security.login_lockout_enabled: false` | `docker/config/homeserver.yaml` | 不判定也不计数（`check_login_lockout` 直接放行） | 改回 `true`（默认值即 true） |

代码侧告警：`middleware/rate_limit.rs` 与 `utils/login_lockout.rs` 各用 `AtomicBool` 打一条
`WARN ... DISABLED`（只打一次，避免刷日志）。**为什么必须留告警**：「没有 429」与「限流正常工作」
在客户端看来完全一样，带着 `false` 上线不会被任何测试发现。

### 6.6 P2 部署侧：`trust_forwarded` 落地

- 之前两个 compose 栈都**没有固定子网**（docker 自动分配），而 `trusted_proxies` 必须写死 CIDR
  ⇒ 已给 `docker/docker-compose.yml` 与 `docker/deploy/docker-compose.yml` 的 `ipam` 加上
  `${SYNAPSE_NETWORK_SUBNET:-192.168.97.0/24}`（与本机现有网络一致，无需重建）。
- `rate_limit.trust_forwarded: true` + `trusted_proxies: ["192.168.97.0/24"]` 同时写入
  `docker/config/rate_limit.yaml`（中间件）与 `docker/config/homeserver.yaml`（登录锁定读取的那半）。
- `docker/deploy/.env.example` 补 `SYNAPSE_NETWORK_SUBNET`，并写明"与 YAML 的 trusted_proxies 必须一致"。
- 顺带纠正一处误判：`synapse-nginx`（0.0.0.0:443 那个）每个代理 location 都设了
  `X-Forwarded-For $proxy_add_x_forwarded_for`，**转发头链路本身是完整的**；
  缺的只是服务端"信任该子网"的声明。

> ⚠️ 生效前提：`trusted_proxies` 只在 `trust_forwarded: true` **且**对端确实落在该 CIDR 时生效；
> 直连（例如 dev 栈 127.0.0.1:8008）仍用 TCP 对端地址，这是刻意的（防 XFF 伪造换桶）。

### 6.7 判据（复现）

```bash
# 1) 投影两向为 0
psql "$DATABASE_URL" -tAc "WITH s AS (SELECT user_id,room_id FROM room_summary_members WHERE membership IN ('join','invite')),
  m AS (SELECT user_id,room_id FROM room_memberships WHERE membership IN ('join','invite'))
SELECT (SELECT count(*) FROM (SELECT room_id,user_id FROM s EXCEPT SELECT room_id,user_id FROM m) a) AS stale,
       (SELECT count(*) FROM (SELECT room_id,user_id FROM m EXCEPT SELECT room_id,user_id FROM s) b) AS missing;"

# 2) body 解析失败一律 Matrix 错误体（关掉限流/锁定后仍成立）
curl -si -X POST $B/_matrix/client/v3/createRoom -H "Authorization: Bearer $TOK" \
  -H 'Content-Type: text/plain' --data-binary x | head -3          # 400 + application/json + M_NOT_JSON

# 3) 登录失败 403（恢复锁定后）
curl -s -X POST $B/_matrix/client/v3/login -H 'Content-Type: application/json' \
  -d '{"type":"m.login.password","identifier":{"type":"m.id.user","user":"x"},"password":"WRONG"}'
```

### 6.8 I-04 强类型 body（本轮补齐）

`synapse-web/src/routes/handlers/room/management/create.rs` 的 `CreateRoomBody` 已从
`Json<Value>` 改为强类型 `#[derive(Deserialize)] #[serde(deny_unknown_fields)]` 结构体：

| 键 | 类型 | 说明 |
| --- | --- | --- |
| `name` / `topic` / `preset` / `visibility` / `room_alias_name` / `room_type` | `Option<String>` | 高频键，类型错误即反序列化失败 |
| `invite` | `Option<Vec<InviteEntry>>` | 支持纯字符串与 `{"user_id","reason"}` 两种形态 |
| `is_direct` / `ignore_duplicate_name` | `Option<bool>` | |
| `creation_content` / `power_level_content_override` | `Option<Map<String, Value>>` | 自由形态但必须是对象 |
| `initial_state` / `invite_3pid` | `Option<Vec<Value>>` | 未实现的三方邀请显式拒绝 |

- 错误路径：类型错误 / 未知字段 → `MatrixJson` 的 `JsonDataError` → **400 `M_BAD_JSON`**。
- 回归用例 10 个（`create.rs` 测试模块）：`{"name":123}` 400、未知键 400、合法请求 200、
  `invite` 两种形态、`topic` 落库等。

### 6.9 I-07 线程 `content` 显式拒绝（本轮补齐）

`handlers/thread.rs` 的 `reject_unsupported_thread_content`：

| 输入 | 结果 |
| --- | --- |
| 缺省 / `null` / `{}` | 200（无可丢弃内容） |
| 任意非空对象（如 `{"name":"标题"}`） | **400 `M_INVALID_PARAM`** |

线程根的 content 由 `root_event_id` 指向的既有事件定义，`thread_roots` 表也无处存放客户端
传入的 content，因此按"要么落库、要么拒绝"选后者。新增 3 个用例 + 既有 2 个用例全绿；
`docker/deploy/api-integration_test.sh`、`scripts/test/api-integration_test.sh` 与
`docs/synapse-rust/ROUTE_CONTRACT.md` 附录同步为"非空 content → 400 `M_INVALID_PARAM`"。

### 6.10 I-01 单元测试（本轮补齐）

| 用例 | 位置 | 断言 |
| --- | --- | --- |
| `join_upserts_room_summary_projection_from_invite` | `room/membership/actions.rs` | 预置 `invite` 投影行后 join ⇒ 投影变 `join` |
| `leave_removes_room_summary_projection` | 同上 | leave ⇒ 投影行消失 |
| `leave_and_forget_removes_room_summary_projection` | 同上 | leave+forget ⇒ 投影行消失 |
| `ban_user_removes_summary_projection` | `room/membership/moderation.rs` | ban ⇒ 投影行消失 |
| `kick_user_removes_summary_projection` | 同上 | kick ⇒ 投影行消失 |

`cargo test -p synapse-services --lib --all-features room::membership` → **102 passed / 0 failed**。

### 6.11 本轮未做

- 全量集成套件（~2h）与 live 联邦互操作（需双主机名 + 互信 CA）。
- 活体复验（`--all` 一键部署后按 §5 探测）：待部署完成后执行，结果回填本节。

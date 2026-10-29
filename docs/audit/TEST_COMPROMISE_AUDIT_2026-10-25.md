# 测试降标审计：被压缩 / 跳过 / 降低标准的测试（2026-10-25）

> **审计问题**：是否存在"为给产品缺陷让路或赶进度，测试环节被压缩、跳过或降低标准"。
> **审计口径**：只认**能复现**的判据 —— 每条结论给出 `文件:行` 与实跑命令；不能复现的一律进 §5（已排除）。
> **与前几轮的关系**：C-7 那轮已确认过一次"测试替产品缺陷让路"的**已发生实例**（`api_widget_tests`
> 的 PL helper 为绕开 403 而只写非创建者，产品缺陷直到 `7d58ac648` 才修）。本文件回答的是
> **现在还剩哪些同类形态**，以及哪些验收条件被**制度性**地降低了。

## 0 结论

有，而且集中在四处。最严重的一条不是"少写了测试"，而是**测试基础设施自己 fail-open** ——
在数据库不可达时，一批**安全边界用例会静默报 PASS，一条断言都不跑**，且**耗时比真跑更长**
（30 s vs 6 s），所以"跑得快 = 没跑"这个常用判据在这里是反的。

另有一条**同根因**（没有任何测试确认那段代码在跑）的独立发现：refresh token 重放检测里
"检测到重用即撤销全族"这条能力，**在真实路径上三层判断互相阻断、一次都没生效过**（§2.5），
现已接通。它不是覆盖率数字能反映的问题，所以单列。

| 等级 | 条目 | 受影响功能 | 状态 |
|---|---|---|---|
| **R1** | `setup_fresh_test_app_with_config` fail-open ⇒ 9 条安全边界用例可静默假绿 | 语音上传体积上限（含 CVE-2026-102592 缓解）、媒体 2 MiB 边界、worker surface gate 窄化授权 | **✅ 已修复**（入口改 fail-closed + 9 处换 `skip_or_fail_without_db`；**9/9 变异自证**，双向验证见 §2.4） |
| **R1** | refresh token 重放「撤销全族」不可达：`validate_token` 在入口就拒已撤销 token，第 1 层参数传错恒空转，第 2 层把重放判成良性并发 | 会话安全（RFC 6819 / OAuth Security BCP） | **✅ 已接通**：抽 `load_token_inner` + 5 s 并发窗口（两层都加）+ mock 有状态化；双向变异（窗口 0 / MAX）各自红（§2.5） |
| **R2** | 覆盖率门禁不在 PR 车道；新文件达标线 30%；已入库文件无绝对下限 | 全仓覆盖率回归（事后才发现） | **🟡 已收紧**：达标线 30 → **50**（§3.1.1 双向验证）；**基线自毁缺陷已修**（一次不完整 lcov 曾把 608 条砍成 1 条，§3.1.1）；0% 条目**分母已订正为 32**（§7.5）—— 另 22 个 `test_mocks/` + 5 个二进制入口**本就在门禁的两张排除清单里**，不计入：**10 个陈旧（全部已本地实测非零）/ 4 个真 0（已实测）/ 3 个 `feature` 关（已补 feature 集）/ 5 个无实质可测行 / 2 个跨 crate 转发层（已补 `&dyn` 契约测试，实测 28.7% / 14.6%）/ 8 个本轮已消除（补测试 6 + 删死代码 2）**；**"等 CI 的 diff"已被证伪**：覆盖率车道在最近 30 次运行里全部 `skipped`（`needs: [integration-test]`，而 `test` 恒红），`auto-ratchet` 自动提交史为 **0 条** ⇒ 基线里的 0.0 是 bootstrap（2026-09-19）的**冻结值**，不是当期读数（§7.5 G）。**三条修复（2026-10-27）**：①`coverage` 的 `needs` 去掉 `integration-test`（§7.5 H，否则永远不发车）；②`STORAGE_FEATURES` 补 `friends,widgets,burn-after-read`（§7.5 F，71 条用例全绿）；③**20 条冻结地板抬到实测下限**（§7.5 I，608→608 条、0 新增、0 降低） |
| **R2** | 性能验收只打印不断言（`appservice_scheduler_perf_tests` 全 `#[ignore]`） | appservice 调度器吞吐 | **🟡 已补 1 条**：`get_statistics` 每行成本折算往返次数，进 CI（§3.2.1）；其余仍 ignore |
| **R2** | `cargo test --doc` 是编译门禁，不是执行覆盖（4 个 doc test 全 `#[ignore]`） | 文档示例 | 已知并登记 |
| **R3** | 572 处 `else { return; }` 静默跳过**残留模式**（当前恒 `Some`，但 R1 已证明该模式会复活） | 全 integration 面 | **✅ 已止损**：7 处 fail-open 夹具改 panic + 守卫禁新增（§4.1.1）；调用点本身保留为死代码 |
| **R3** | 测试替身与生产 crate 未做特性隔离（MAINT-01） | unit 层 mock 语义 | **🟡 已设棘轮**：实测 106 处「忽略参数却假装成功」（24.7%），上限**只许下降**（`check_mock_stub_drift.py`，已接 preflight）；**抽样 6 处无一处假绿**、另有 2 处是死代码（§4.2.1 / §4.2.2） |
| **R3** | 回归范围按"改了哪个文件"选，而非"影响哪些契约" | 跨路径契约（ARCH-02 等价用例曾静默腐烂） | **✅ 已建机制**：`test_impact_map` + 提交前提示（§4.3.1） |

**没有发现**的（同样重要，见 §5）：无裸 `#[ignore]`、无"形状测试冒充覆盖"、无生产代码
`cfg!(test)` 短路、by-design unsupported 有双向门禁、棘轮上调有逐条归因。

## 1 排查方法（可复现）

```bash
# ① 静默跳过 vs fail-closed 的分布
python3 - <<'PY'   # 见 §3.1，统计 let Some(..) else { return; }
PY

# ② 跳过用例是否有覆盖车道
python3 scripts/quality/analyze_skipped_tests.py

# ③ 形状测试是否冒充覆盖
python3 scripts/ci/check_shape_only_tests.py

# ④ 覆盖率门禁的触发条件与达标线
grep -n -A6 '^  coverage:' .github/workflows/ci.yml
grep -nE 'global-floor|new-file-floor|core-threshold' scripts/ci/run_coverage.sh scripts/check_file_coverage.py

# ⑤ 假绿复现（R1 的决定性实验，见 §2.1）
CI=1 TEST_DATABASE_URL='postgresql://synapse:x@127.0.0.1:59999/nope' \
  cargo nextest run --test integration --test-threads 1 \
  --features 'test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,widgets' \
  -E 'test(test_voice_upload_accepts_body_above_the_axum_default_limit)'
```

## 2 R1：安全边界用例可静默假绿（活缺陷）

### 2.1 现象与实跑证据

`tests/integration/mod.rs:650` 的 `setup_fresh_test_app_with_config` 用
`prepare_shared_test_pool().await.ok()?` 把基础设施故障折叠成 `None`，而它的调用点全部是
`let Some(..) = ... else { return; }` ⇒ libtest 记为 `ok`。

同一台机器、同一个不可达 URL，两种 setup 入口给出相反结论：

| 用例 | setup 入口 | 耗时 | 结果 |
|---|---|---|---|
| `voice_routes_tests::test_voice_upload_accepts_body_above_the_axum_default_limit` | `with_config`（`.ok()?`） | **30.036 s** | **PASS**（0 断言） |
| `api_friend_routes_tests::friend_endpoints_require_authentication` | `TestContext::new`（panic） | 30.439 s | **FAIL** + `[integration] No reachable test database` |

对照真库（`localhost:55432`）同 4 条：

```text
PASS [17.767s] test_voice_upload_accepts_body_above_the_axum_default_limit
PASS [ 6.024s] test_voice_upload_body_limit_is_content_type_case_insensitive
PASS [ 6.663s] test_voice_upload_rejects_body_above_the_configured_limit
PASS [ 4.148s] test_voice_upload_streamed_body_without_content_length_is_still_rejected
Summary: 7 tests run: 7 passed
```

⇒ 定性是**间歇性假绿**：正常有库时真跑真断言；DB 抖动 / 模板建池失败时**静默变绿**。
危险点在于**假绿比真跑还慢**（30 s vs 6 s），任何"耗时异常短"的巡检都发现不了它。

### 2.2 受影响清单（9 条，全部为安全边界）

| 文件:行 | 用例 | 该用例守的边界 |
|---|---|---|
| `voice_routes_tests.rs:599` | `test_voice_upload_accepts_body_above_the_axum_default_limit` | 2.5 MiB 必须被接受（axum 默认 2 MiB 不能成为事实上限） |
| `voice_routes_tests.rs:622` | `test_voice_upload_rejects_body_above_the_configured_limit` | 超 4 MiB 必须拒绝 |
| `voice_routes_tests.rs:649` | `test_voice_upload_body_limit_is_content_type_case_insensitive` | **CVE-2026-102592 对应项**：`Multipart/Form-Data` 大小写变体不得绕过上限 |
| `voice_routes_tests.rs:676` | `test_voice_upload_streamed_body_without_content_length_is_still_rejected` | 无 `Content-Length` 的流式体仍须受限 |
| `api_media_routes_tests.rs:123` | `test_media_upload_with_id_accepts_over_2mb_body` | 媒体 2 MiB 边界（接受侧） |
| `api_media_routes_tests.rs:177` | `test_media_preview_and_delete_boundaries` | 预览/删除边界 |
| `api_media_routes_tests.rs:298` | `test_client_v1_authenticated_media_thumbnail_and_preview_routes_work` | v1 认证缩略图/预览 |
| `api_worker_surface_gate_tests.rs:51` | `a_narrowed_instance_serves_its_own_prefix_and_refuses_the_rest` | W-F 窄化授权：非属主实例必须 404 |
| `api_worker_surface_gate_tests.rs:89` | `without_the_switch_the_same_paths_are_not_refused` | 同上（开关关闭时的反向断言） |

**为什么偏偏是它们**：这 9 条都要改 `ServiceContainer` 配置（如 `server.max_upload_size`），
唯一能改配置的入口就是 `with_config` —— 而它恰好是 fail-open 的那一个。
即**测试基础设施的缺陷，把最关键的安全测试降级了**。

### 2.3 补齐项

| 优先级 | 动作 | 判据 |
|---|---|---|
| **P0 ✅ 已完成** | `setup_fresh_test_app_with_config` 内部先 `require_test_database().await`（与 `TestContext` 同口径），并把 `.ok()?` 改成 `Err` ⇒ `panic!` | 重跑 §1 ⑤ 的假绿实验 ⇒ 已 **FAIL**；真库下 9 条仍 **9 passed**（见 §2.4） |
| **P0 ✅ 已完成** | 9 个 `else { return; }` 换成 `else { skip_or_fail_without_db(); return; }`（双保险） | 同上 |
| ~~P1~~ **✅ 9/9 已完成** | 9 条安全用例逐条变异自证，覆盖 6 种不同机制（业务判据 / 路由门禁 / 全局 layer / 传输层+业务层纵深 / serde 参数语义 / 认证提取器）。其中 ⑦ 第一次用的变异（`deny_unknown_fields`）**无效**，换 `ts` 必填才红（§2.4） | 见 §2.4 |
| ~~P2~~ **✅ 已达成** | 全仓 `with_config` 族 helper 内 `return None` 改 fail-closed | `check_test_setup_fail_closed.py` 扫 **74 个**夹具型函数报 **OK**；`grep "return None" tests/integration/mod.rs` 仅剩 1 处、在**注释**里（"2026-10-18：数据库不可达时 panic，不再 return None"） |

### 2.4 修复与双向自证（2026-10-25）

改动面只有 4 个文件共 +28/−1：

- `tests/integration/mod.rs`：入口先 `require_test_database().await`，再把 `.ok()?` 换成
  `Err` ⇒ `panic!`（与 `TestContext::build` 同口径）；`Option` 返回值保留（兼容调用点），但恒 `Some`。
- 9 个调用点：`else { return; }` ⇒ `else { skip_or_fail_without_db(); return; }`
  （`let ... else` 要求分支发散，而该辅助函数签名是 `()`，所以按仓库既有 42 处的写法补 `return;`）。

| 验证 | 修复前 | 修复后 |
|---|---|---|
| 假绿实验（`CI=1` + 不可达 URL） | **PASS 30.036 s**（0 断言） | **FAIL 30.029 s** + `[integration] No reachable test database … refusing to run` |
| 真库 9 条 | — | **9 passed**（1.0–4.8 s，逐条真跑） |
| 变异自证 ①（`voice_multipart_body_limit` 退回 axum 默认 2 MiB） | — | **红**：`2.5 MiB 语音必须被接受：/voice/config 宣告的上限是 4194304 字节，响应 {"errcode":"M_BAD_JSON",…}`；还原后 7 passed |
| 变异自证 ②（`worker_surface_middleware` 的 `role_owns_path` 恒真） | — | **红**：`/_matrix/client/v3/media/config must be refused for a synchrotron: {"errcode":"M_UNAUTHORIZED",…}`，`left: 401 / right: 404` |
| 变异自证 ③（全局 `RequestBodyLimitLayer` 抬到 `usize::MAX`） | — | **红**：`大小写变体不得绕过请求体上限…响应 {"errcode":"M_BAD_JSON","error":"Voice message too large. Max size is 524288 bytes"}`，`left: 400 / right: 413` |
| 变异自证 ④（voice 层 `usize::MAX` + 全局层 `usize::MAX`，两层传输上限同时取消） | — | **红**：`超过 server.max_upload_size 的请求体必须在传输层被拒`，`left: 400 / right: 413` —— 只剩业务判据，它给不了 413 |
| 变异自证 ⑤（④ + 业务判据 `if false`，三层全关） | — | **红**：panic @ `voice_routes_tests.rs:691`（`流式超限 body 绝不能成功`）—— 544 KiB 流式体变成 200 |
| 变异自证 ⑥（MSC2246 PUT 上传的 `DefaultBodyLimit` 退回 2 MiB） | — | **红**：`S27 回归：3MB 的 MSC2246 上传被 413 拒绝`，`left: 413 / right: 413`（`assert_ne` 两侧同值） |
| 变异自证 ⑦（`PreviewUrlParams.ts` 由 `Option<i64>` 改必填 `i64`） | — | **红**：`left: 400 / right: 200` —— 不带 `ts` 的 v1 preview 被 serde 拒成 400 |
| 变异自证 ⑧（v1 缩略图的 `AuthenticatedUser` 降级为 `OptionalAuthenticatedUser`） | — | **红**：匿名缩略图 `left: 200 / right: 401` |
| 变异自证 ⑨（`if config.worker.surface_gate` 改 `if true`，无视开关） | — | **红**：`/_matrix/client/v3/media/config must not be 404 when worker.surface_gate is off`（这正是对照组要防的"空洞否定测试"） |

九条变异（9/9）各自钉住了不同的东西。三条给出了**额外信息**：

- ② 的 401 vs 404 证明那个 404 **确实由 gate 返回**——门禁失效后请求会穿透到 handler 变成 401，
  而不是被别的层顺手拒掉。这类"404 到底是谁给的"的歧义，只有变异能分清。
- ③ 去掉全局层后请求**没有变成 200**，而是被 voice 自己的业务判据兜住（`Voice message too large`
  → 400）。⇒ 请求体上限是**纵深防御**（全局 layer + 业务判据），不是单点。测试钉的是
  "必须是 **413**（规范状态码）"，比"必须被拒"更严格 —— 这一条顺带否掉了"这个 CVE 缓解只有单层
  保护"的猜想。
- ④/⑤ 把这条纵深数清了：**三层**（全局 `RequestBodyLimitLayer` / voice 路由 `DefaultBodyLimit` /
  handler 的业务判据 `content.len() > max_size`）。④ 关掉前两层 ⇒ 仍被拒但状态码掉成 400；
  ⑤ 连业务判据也关掉 ⇒ 544 KiB 流式体**成功**（`status == 200`）。⇒ **413 这个状态码只能由
  传输层提供**，业务判据给不出；而"能拒住"最薄的一层是业务判据。

**一条无效变异（值得记）**：⑦ 先试的是给 `PreviewUrlParams` 加 `#[serde(deny_unknown_fields)]`，
测试**照样 PASS** —— 因为 `ts` 是**已声明字段**，不属于 unknown field，这个变异什么都没改变。
它同时证明那段测试注释写错了（原注释称"加 `deny_unknown_fields` 会先红"，还称"结构体故意不声明
`ts`"，而实际声明了 `ts: Option<i64>` + `#[expect(dead_code)]`）。注释已就地订正，并把有效变异
（`ts` 改必填）与实测值写了进去。**"变异能让测试变红"本身要先验证**：无效变异会伪装成
"测试没抓到"，与真的漏测完全同形。

### 2.5 「检测到重用即撤销全族」曾不可达，现已接通（含一次实施中发现的误杀）

本来要给「重放检测」补测试（它是 S-9 / A+ W1 的 fail-closed 改造点，全文件零测试）。
读完整调用链后，结论比"没有防护"更精确：**有三层判断，前两层让第三层到不了，第三层又会把重放判成良性。**

调用链：`refresh_access_token`（110）→ `validate_token`（117）→ 前置重放检查（130–179）
→ `rotate_token_atomic` CAS（186）→ CAS-miss 重放处理（206–254）。

#### 第 0 层 `validate_token`：把重放挡在最前面，也把族撤销的可能挡掉了

```rust
// validate_token:94
if token_record.is_revoked {
    return Err(ApiError::unauthorized("Token has been revoked"));
}
```

一次正常轮换会把旧 token 置为 revoked（`rotate_token_atomic` 的 CAS 目标就是 `is_revoked = false`）。
于是**串行重放**（先正常 refresh，再用同一个旧 token）在第 117 行就返回
`401 "Token has been revoked"` —— **根本到不了后面两段重放处理**。

#### 第 1 层 前置检查（130–179）：参数传错，恒空转

```rust
let family_id = match self.storage.get_rotations(&old_token.user_id).await { … }   // ← 传 user_id
let rotations = self.storage.get_rotations(&family_id).await?;                     // ← 刚新建的 family
if let Some(last_rotation) = rotations.first() { /* 重放处理 */ }                  // ← 恒 None
```

`get_rotations(family_id)` 的 SQL 是 `FROM refresh_token_rotations WHERE family_id = $1`；而第 130 行传的是
`@alice:example.com`，family_id 由 `generate_token(16)` 生成 ⇒ 恒 0 行 ⇒ 恒走 `_` 分支**新建一个 family**
⇒ 第 155 行查这个刚建的族（还没有任何 rotation）⇒ `first()` 恒 `None` ⇒ 这段重放处理从未执行。
**副作用**：每次 refresh 都新建一族，`refresh_token_families` 单调增长。

#### 第 2 层 CAS-miss（206–254）：只在并发竞态下可达，且会把重放判成「良性」

`rotate_token_atomic` 返回 `None` 表示 CAS 失败（token 已被撤销）。此时按 `revoked_reason` 分流：

| `revoked_reason` | 处理 | 语义 |
|---|---|---|
| `Some("Rotated")` | 401 `"has already been used"`，**不撤销族** | 判为并发 refresh / 网络重试 |
| 其它 / `None` | `mark_family_compromised` + `revoke_all_user_tokens` + 401 `"All tokens revoked"` | 判为真重放 |

而**正常轮换写的 reason 正是 `"Rotated"`**（第 202 行）。所以能走到这一层的重放（只可能是并发竞态），
也会命中"良性"那一行 ⇒ 不撤族。

#### 净效果

- **能挡住**：任何重放的 refresh token 都换不到新 token（第 0 层就 401）——这一点是好的；
- **挡不住**：**泄露族不被撤销、用户不被强制登出、没有告警**。若攻击者抢在受害者之前用窃取的 token
  换到过新 token，受害者的下一次 refresh 只会得到"已被撤销"，双方都不被告警，泄露无法止损。
  这违反 OAuth 2.0 Security BCP / RFC 6819 对 refresh token rotation 的要求（检测到重用即撤销整族）；
- **附带**：families 表随 refresh 次数单调增长，且每族只有一条 rotation。

#### 为什么一直没被发现

`refresh_access_token` **没有任何测试调用**（全仓 grep 只命中定义处）；`replay` 在文件里只出现在注释。
这正是 R1 的形态：**安全能力缺失不是因为难写测试，是因为没人确认它在跑**。

#### 已接通（2026-10-26，含一次实施中发现的误杀）

四处改动：

1. **`get_family_id(user_id, device_id)`**（trait +1、真实 SQL
   `WHERE user_id = $1 AND device_id IS NOT DISTINCT FROM $2`、mock +1，`.sqlx` +1）。
2. **`validate_token` 抽出 `load_token_inner(token, reject_revoked)`**：`validate_token` 保持原语义
   （公开 API + 7 条既有测试不动），refresh 走 `reject_revoked = false` —— 即**允许看到"已撤销"这个事实**，
   否则 CAS-miss 分支永远到不了。黑名单 / 不存在 / 过期仍然一律拒。
3. **第 1 层（前置检查）加并发窗口**：`old_token_hash != latest.new_token_hash` 时，
   若距该次轮换 ≤ `CONCURRENT_REFRESH_WINDOW_MS`（5s）⇒ 只拒本次；超出 ⇒ 撤族。
4. **第 2 层（CAS-miss）同样按窗口分流**（原来只按 `revoked_reason == "Rotated"` 判良性，
   而正常轮换写的正是 `"Rotated"` ⇒ 重放会被当成并发）。

**实施中发现的新问题（差点引入误杀）**：第 1 层一旦可达，它**无条件**撤族。而客户端并发刷新 /
网络重试会在毫秒级内重发旧 token，`old_token_hash` 同样不等于最新 `new_token_hash`
⇒ **一次重试就把用户踢下线**。这正是加窗口的原因 —— 第 1 层和第 2 层都必须有。

#### 双向变异自证

| 变异 | 期望 | 实测 |
|---|---|---|
| `CONCURRENT_REFRESH_WINDOW_MS = 0`（一切算超窗口） | 「窗口内」测试红 | **FAILED** ✓ |
| `CONCURRENT_REFRESH_WINDOW_MS = i64::MAX`（永不撤族） | 「窗口外」测试红 | **FAILED** ✓ |
| 还原 5_000 | 两条都绿 | **2 passed** ✓ |

⚠️ 第一版「窗口内」测试**抓不到**变异 A：两次调用落在同一毫秒、`elapsed == 0`，
窗口取任何非负值都判良性 ⇒ 测试是空壳。改成 `age_rotations(&family, 1_000)` 把轮换挪到
"窗口内但非零"之后才真正有效。

#### 行为变化（产品可感知）

**超出 5s 窗口的重放会撤销整个 token 族 ⇒ 该用户全部 token 失效、被迫重新登录。**
这是 RFC 6819 / OAuth Security BCP 的要求，但确实改变了用户可感知行为：
- 正常客户端在 5s 内重试 ⇒ 只收到 `"Refresh token has already been used. Please use the new token."`，不登出；
- 攻击者事后重放（或受害者在 >5s 后用旧 token）⇒ `"Token reuse detected. All tokens revoked."`。

窗口取值 5s 是经验值（客户端并发/重试普遍在秒级内）；若线上出现误登出，先调大它再怀疑逻辑。
时钟回拨时 `elapsed` 为负 ⇒ `<= 窗口` 成立 ⇒ 判良性 ⇒ **偏向不误杀**，方向是安全的。

#### 测试

- `replay_within_window_is_refused_but_family_survives` —— 窗口内只拒本次，族保留（新 token 仍可用）；
- `replay_outside_window_revokes_the_whole_family` —— 超窗口 ⇒ `"All tokens revoked"` 且同族新 token 失效。

## 3 R2：制度性降标

### 3.1 覆盖率门禁的三个缺口

| 缺口 | 证据 | 后果 |
|---|---|---|
| **不在 PR 车道** → **部分已解（2026-10-28）** | `ci.yml` 的 `code-coverage.if` 只有 `schedule` / `workflow_dispatch+run_slow_tier` / `push main`（全量口径 67 分钟，PR 上等不起）；新增的 `pr-coverage` 只跑受影响 crate（**下界**口径，见 §3.1.4） | 全量回归仍要等 main；但 PR 阶段现在能看到"这次改动新增的行有没有被跑到" |
| **新文件只需 30%** → **✅ 已改为 50%**（2026-10-25） | `ci.yml:1522` `--new-file-floor 50`（原为 30）（脚本默认 60，被显式下调）。已上调到 50，同步 `run_coverage.sh` 提示、`coverage_policy.md` §1/§2/§3、`TESTING.md` | 用 40% 的假 lcov 喂脚本 ⇒ **exit 1**；50% ⇒ exit 0（不误伤） |
| **已入库文件无绝对下限** | `scripts/check_file_coverage.py:8` 只判 `must not regress`；`ci.yml:1509-1512` 注释写明"absolute floors are deliberately not re-applied to baseline-known files, because…188 files red" | `coverage_baseline.json` 里 **59 个文件 0.0%、109 个 ≤30%** 被永久冻结，再差也不会红 |

基线分布（实测 `coverage_baseline.json`，608 个文件）：中位数 **81.5%**、均值 68.1%，
所以**不是"全仓覆盖率低"**，而是"**差的那批被冻结了，且新代码门槛低**"。

补：

| 优先级 | 动作 |
|---|---|
| **P1 ✅ 已完成** | `--new-file-floor 30 → 50`（先不喊 60，避免一次打红太多）；`core_file_coverage_prefixes.txt` 内的新文件沿用 70 |
| **P2 ✅ 台账已建（2026-10-28）** | 给 0% 文件中**有生产入口**的（`src/server/router.rs`、`src/tasks/mod.rs`、`synapse-common/logging.rs`）建"提升到 30%"的一次性任务，逐文件记账（**不是**一刀切重贴全局下限 —— 那会重演 188 文件红）。台账 `scripts/ci/coverage_raise_queue.json`、守卫 `scripts/ci/check_coverage_raise_queue.py`，机制与实跑见 **§3.1.3** |
| **P2 ✅ 已落地（2026-10-28）** | 把 coverage 加进 PR 车道（可只跑受影响 crate 的 lcov，用 `cargo llvm-cov -p <crate>` 降成本）。新 job `pr-coverage` + `scripts/ci/{affected_crates.py,run_pr_coverage.sh,check_changed_line_coverage.py}`；判据是**变更行覆盖率**而不是"和基线比"（下界与合并基线不可比），机制、实测与边界见 **§3.1.4** |

### 3.1.1 覆盖率基线可以被一次「不完整报告」清零（本次审计中新发现 + 已修）

**缺陷**：`scripts/check_file_coverage.py` 的 `main()` 里

```python
save_path = args.save_baseline or args.baseline  # 不传 --save-baseline 也写回基线
save_baseline(save_path, current, previous=baseline)
```

而 `save_baseline()` 的实现是 `{p: max(prev, cur) for p, cur in files.items()}` ——
只遍历**本次 lcov** 的键。两者叠加：**拿一份不完整的 lcov 跑一次，基线里没出现在本次报告中的
条目会被整批丢掉**。

**为什么这不只是理论**：CI 的 `Per-file coverage ratchet (B.3)` 步骤**故意不传**
`--save-baseline`，靠的正是这个默认写回在 main 上自动收紧（`--save-baseline` 只有
bootstrap 步骤显式传）。所以"默认写回"是正确的设计，缺的是**合并时保留 previous 的键**。

**实测后果（2026-10-25，我自己踩的）**：做 §3.1 的门槛双向验证时，用一条假记录的 lcov
（`SF:synapse-common/src/ramp_up_probe.rs`）跑脚本 —— `coverage_baseline.json` 从
**608 条变成 1 条**。棘轮当场报废：之后所有历史文件都变"新文件"，只受 ramp-up 门槛约束，
"已入库文件不得回退"那一半彻底失效。而且**整个过程没有一行警告**，`git status` 里只显示
`M scripts/ci/coverage_baseline.json`，看起来像"别人的在飞改动"。

**修法**：`merged` 先从 `previous` 复制全量，再对本次报告里的文件取 `max`；保存后打印
「N 个文件本次测量，M 个条目保留自上次」。代价是被删除/重命名的文件旧条目会留在基线里
（棘轮只增不减，无害；重命名后的新路径按"新文件"受 ramp-up 约束，那本来就是 policy 想要的压力）。

**三向验证**（每次跑完立即 `git checkout` 还原）：

| 场景 | 结果 |
|---|---|
| 不完整报告（1 条假记录） | 修复前 **608 → 1**；修复后 **608 → 609**，并打印「1 个文件本次测量，608 个条目保留自上次」 |
| 某文件覆盖率下降（`synapse-cache/health.rs` 100% → 假报 50%） | 保存后仍 **100%** —— `max` 的单调语义未被破坏 |
| 还原 | `git checkout` 后回到 608 条 |

⚠️ **通用教训**：跑"检查类"脚本前先确认它有没有写副作用。本脚本名字叫 `check_*`，
却在默认参数下**改写**基线 —— 这类"检查即写入"的形态在做门槛验证（喂假数据）时最危险。
本轮之后凡是用假 lcov/假数据跑它，都必须先 `cp` 备份、跑完 `git checkout` 还原。

### 3.1.2 基线里的 0% 有一部分是「陈旧」，不是「缺测试」（2026-10-25 实测）

原本 §6 第 7 项写的是"给 3 个 0% 但**有生产入口**的文件各补一条冒烟"。动手前先做了一次插桩探测，
结论是**前提部分不成立**：

```bash
cargo llvm-cov --test integration --features "…" --lcov --output-path /tmp/router_probe.info -- server_router_assembly
# 3 passed；产物里：
#   SF:…/src/server/router.rs   LF:35  LH:29
```

两条腿各跑一次（`--workspace --lib --all-features` 与 `--test integration`），差异一目了然：

| 文件 | 基线记录 | lib 腿 | integration 腿 | 判定 |
|---|---|---|---|---|
| `src/server/router.rs` | **0.0%** | 0/35 | **29/35 = 82.9%** | **基线陈旧**，不缺测试 |
| `src/tasks/mod.rs` | 0.0% | 0/203 | 0/203 | **真缺口**（203 行，两条腿都不执行） |
| `synapse-common/src/logging.rs` | 0.0% | 0/49 | 该腿不编译它 | 真缺口，但属**进程启动路径**（只在 `src/server/telemetry.rs` 的 `init_logging` 里被调） |
| `src/server/telemetry.rs` | 0.0% | 0/18 | 0/18 | 同上（启动期一次性初始化） |

读数说明：

- `router.rs` 在 lib 腿 0%、integration 腿 82.9% —— **完全对上"基线按 `--lib` 口径生成"的假设**，
  这是本次"基线陈旧"判定最硬的一条证据。
- `tasks/mod.rs` 两条腿都 0/203 ⇒ 不是口径问题，是**真的没有任何测试执行过那 203 行**。
  它是后台维护任务（清理/修剪/监控），正是"出错没人发现"的典型形态——这也是第 7 项里**唯一**
  站得住的"该补测试"目标。
- `logging.rs` / `telemetry.rs` 的 0% 属"进程启动一次性初始化"，补测试要连 tracing subscriber 与
  OTLP exporter，收益低于 `tasks`。**登记为已知缺口，不排期**。

`build_router` 被 `tests/integration/mod.rs::production_app` 调用，而几乎所有集成测试都走那个
装配入口 —— 逻辑上不可能 0%。口径差异的来源，是文档与实现不一致：

- `docs/coverage_policy.md` §2 写「CI 用 `cargo llvm-cov --workspace --lib`（+ `-p synapse-storage --lib`）」
- `scripts/ci/run_coverage.sh` 步骤 2 实际是 `cargo llvm-cov --workspace --exclude synapse-storage`
  —— **不带 `--lib`**，会跑全部 test target（含 integration）
- `check_file_coverage.py` 的注释也沿用旧口径（「`cargo llvm-cov --workspace --lib` never executes
  a binary entry point」）

⇒ 基线很可能是按 `--lib` 口径生成的（那时 integration 不计入，`router.rs` 自然是 0%），之后
`run_coverage.sh` 改成全 target 却没重测基线。**这类"基线记录的是旧口径"的条目，补测试毫无用处**
—— 覆盖率仍按旧数记；只有**重测**才能修正。

而"重测"这件事，正好被 §3.1.1 的修复解锁了：在此之前，任何一次不完整的 lcov 都会把基线截断，
所以"随手跑一次局部覆盖 + save-baseline"是危险操作；现在合并会保留 previous 的键，才可以安全地
用局部报告增量收紧。

**行动改判**：第 7 项从"补 3 条冒烟"拆成两件互不相同的事：

1. **重测基线** —— 本地实测后**改判：不必本地跑，交给 CI 的既有链路**。

   2026-10-26 在本机跑了一次完整 `scripts/ci/run_coverage.sh`：**1 小时后仍在编译阶段**
   （判据：`coverage/lcov.info` 无新产出、`find target/llvm-cov-target -name '*.profraw'` = 0、
   load average ≈ 8）。冷编译 + `-C instrument-coverage` 让整个 workspace 重编，量级 1.5 小时。

   而 CI 本来就已经在做这件事：`coverage` job 的 `Per-file coverage ratchet (B.3)` 用完整两腿跑，
   main push 时再 `Commit coverage baseline (auto-ratchet)` 自动写回基线。**本地重跑是重复劳动**，
   且结果不如 CI 可信（本机混着同事未提交的改动）。

   ⇒ 判据改为：**下一次 main push 后看 `coverage_baseline.json` 的 diff** —— 被抬起来的文件
   就是"陈旧 0%"，仍为 0 的才是真缺口。§3.1.1 修掉"不完整报告截断基线"之后，这次自动收紧是安全的：
   不完整的 lcov 只会保留旧条目，不会再把 608 条砍成几十条。

   本地重跑只在"必须马上看分流"时才做，且预期 1.5 小时起。

   **本地实测还撞到第二个障碍（已修）**：跑完 1h15m 时**失败**在 storage 步骤 ——
   1796 passed / **2 failed**，两条都是
   `clone of test_… from test_isolation_template_… failed: error returned from database:
   out of shared memory`。根因不是覆盖率，是本地 `synapse-test-db`（55432）的
   **`max_locks_per_transaction = 64`（PostgreSQL 默认）**：克隆 220 表模板要上千把锁，
   并发两三个克隆就爆。

   ```bash
   docker exec synapse-test-db psql -U synapse -d synapse_test \
     -c "ALTER SYSTEM SET max_locks_per_transaction = 256;"
   docker restart synapse-test-db          # 专用测试库，dev 栈在 5432 不受影响
   ```
   复跑同族 10 条（含此前失败的那 2 条）⇒ **10 passed**。这类 `out of shared memory` 以前被当成
   "偶发 flake"，其实是配额不足 —— 值得写进本地环境说明。

   而 CI 的 postgres service container **无法**传 `-c max_locks_per_transaction`（runner 会把
   `-c` 当成 `--cpu-shares`，`ci.yml` 的注释里已有此结论），CI 是靠 `--test-threads 4` 降并发规避的
   —— 又一条"本地重跑不如 CI"的理由。
2. **补 `src/tasks/mod.rs` 的测试** —— 它是唯一经两条腿验证、确实是 0 执行的生产模块（203 行，
   后台维护任务）。**已补第一批（2026-10-26，3 条，无需数据库）**：
   - `from_config_treats_zero_intervals_as_defaults_not_disabled` —— 钉住 `0` 的语义是
     「回落默认间隔」而**不是**「禁用」。`ServerConfig::default()` 里五个 interval 全是 `0`，
     若把 0 当"关掉"，默认配置的部署会静默失去健康检查/池指标/性能/完整性巡检。
     禁用走的是另一条路（`global_maintenance_tasks_enabled`）。
   - `from_config_uses_each_configured_interval_verbatim` —— 五条独立分支逐字段生效。
   - `new_scheduler_reports_no_results_yet` —— 台账初值必须是 `None`（"还没跑过"），
     不能是 `Some(空报表)`（消费方会当成"跑过且一切正常"）。

   实现上用 `PgPoolOptions::connect_lazy` + `Database::from_pool` 造一个**永不连接**的
   `Database`，所以这几条不依赖数据库；`connect_lazy` 需要 Tokio 上下文，故用 `#[tokio::test]`。

   变异自证：把 `health` 那段改成直接用 `interval_secs`（0 不再回落默认值）⇒ 第一条红、
   另两条仍绿（`left: 0ns / right: 10s`），还原后 3 passed。

   #### 巡检业务逻辑：已在 storage 层覆盖，tasks 只负责调度（2026-10-26 核实）

   上一版把"约 170 行的 `start_*` 循环"当作待补覆盖。核完代码后**改判：不值得补**。

   `start_maintenance_task` 的循环体实质只有三件事：

   ```rust
   let maintenance = DatabaseMaintenance::new(pool.clone());
   match maintenance.perform_maintenance().await {
       Ok(report) => { *last_report.write() = Some(report); info!(…) }   // 台账无人读（§3.1.3）
       Err(e) => { error!(…) }
   }
   ```

   而 `perform_maintenance()` 的**行为**（VACUUM 了哪些表、report 字段、幂等性）在
   `synapse-storage/src/maintenance.rs` 已有 15 处测试：
   `test_perform_maintenance_returns_valid_report`、`..._errors_field_is_present`、
   `..._vacuum_result_is_present`、`..._idempotent_twice`、
   `vacuum_analyze_skips_tables_below_the_modification_threshold`、
   `reindex_tables_reindexes_every_existing_index_in_the_list` 等。

   ⇒ tasks 层未被覆盖的只是"**定时器到点会调用一次**"。要测它，得 `pause()` + 推进 60s/300s
   静默期 + 真库 + 捕获日志（台账无人读，只能靠日志断言），成本高而收益接近零 ——
   真正会坏的东西（`perform_maintenance` 清了什么）已经被测了。

   **下面保留的是"为什么不能简单补"的三条结构性障碍**，作为将来若要做的备忘：

   1. **`start_all` 丢弃了 `JoinHandle`**（`tokio::spawn` 的返回值直接丢）⇒ "任务是否优雅退出"
      **不可观测**。`CancellationToken` 只能发信号，没法等它生效；现在的测试最多断言
      "cancel 之后台账没被更新"，那连"任务还在跑但恰好没 tick"都区分不了。
      要真正测"关闭后全部退出"，得让 `start_all` 返回 `Vec<JoinHandle<()>>` 之类可等待的句柄
      —— 这是**生产 API 的可测性改进**（也是良好设计：关闭应当可等待），不是测试专用钩子。
      调用方只有 `src/server/mod.rs` 一处，改动面小。
   2. **启动静默期**：`STARTUP_GRACE_PERIOD = 60s`、`MAINTENANCE_STARTUP_DELAY = 300s`，
      首轮执行在分钟级之后 ⇒ 不用 `tokio::time::pause()`（需 `test-util`）就没法在测试里推进。
   3. **五个循环全部依赖真库**（`database.health_check()` 等）⇒ 还要可控的数据状态。

   **障碍 1 已解除（2026-10-26）**：`start_all` 现在返回 `Vec<JoinHandle<()>>`，5 个
   `start_*` 各自返回句柄。调用方（`src/server/mod.rs`）用 `let _task_handles = …` **有意不 join**
   —— 进程退出时 runtime 会 abort 未完成任务，而 `drop(JoinHandle)` 是 detach，所以生产行为
   与改动前完全一致；留下句柄是为了让关闭过程**可被观测**。

   新增 `shutdown_cancels_every_task_before_first_tick`：用**已取消**的 token 启动，
   断言 5 个句柄都在 5s 内 `Ok(())` 结束、且四个台账仍为 `None`（证明取消路径没先跑一次真实巡检）。
   五个任务的第一处 `select!` 都是 `biased` + `shutdown.cancelled()` 在前，三个带 grace 期的
   任务也把 `time::sleep` 放在 select 里竞争 —— 这是它能在秒级验证的前提。

   变异自证：删掉 `start_health_check_task` 的 `cancelled` 分支 ⇒ 测试 5s 超时红，消息直接指向
   **"第 0 个后台任务"**（索引即 `start_all` 里的顺序），还原后 4 passed。

   **障碍 2、3 仍在**：静默期内的时间推进与"首轮巡检真的清了什么"都还没覆盖。当前测到的是
   **关闭正确性**，不是**巡检正确性** —— 后者才是这 170 行的主体。

   #### 静默期测试写不出来：因为台账只记「成功」（2026-10-26，写过并删除）

   试过 `#[tokio::test(start_paused = true)]` + `advance(59s)` + 断言三个台账仍 `None`，5 passed，
   看起来对。**变异自证否掉了它**：把 `MAINTENANCE_STARTUP_DELAY` 从 300s 改成 10s
   （`advance(59s)` 后维护任务必然已执行）—— 测试**仍然 PASS**。

   原因：四个台账（`last_health_status` / `last_performance_metrics` / `last_integrity_report` /
   `last_maintenance_report`）**只在 `Ok` 分支写**（`*x.write() = Some(..)`），`Err` 只 `error!` 日志
   ⇒ "执行了但失败"与"根本没执行"在台账上**完全不可区分**。

   ⇒ 那条测试已删除：宁可没有，也不要一条永远绿的假测试。

   #### 再纠正一次：那四个台账**没有任何生产读者**

   上一版本写的是"看板读到 `last_maintenance_report` 非空就以为正常"—— **这句话不成立**。
   全仓 grep `last_health_status` / `last_performance_metrics` / `last_integrity_report` /
   `last_maintenance_report`：命中全部落在 `src/tasks/mod.rs` 内部（声明 / 初始化 / 写 / 测试断言），
   **没有一个 web 或 service 层读者**。

   ⇒ 它们是**只写不读**的字段（除测试）。所以：
   - "失败是静默的"这个判断仍然对，但**不是"面板看不到"，而是"根本没有面板"**；
   - 想加 `last_attempt_ts` / `last_outcome` 也**没有意义** —— 没有消费者的字段，加多少列都是死数据。

   这个模块真正的运维出口只有两处：**`tracing` 日志**（`info!` / `warn!` / `error!`）和
   **`ServerMetrics`**（只有 `pool_metrics` 任务接了：`metrics.update_pool_metrics(...)`）。
   其余四个巡检**既无指标、台账也无人读** —— 要看它们是否在跑、跑成没跑成，只能翻日志。

   建议改为两条（都需要产品决策，未做）：
   1. **接线**：把 `last_*` 接到一个有消费者的地方（`/health` 明细或 Prometheus gauge，
      例如 `synapse_inspection_last_success_ts{kind="maintenance"}`）⇐ 这时才值得补 attempt 字段；
   2. **或者删除**：如果这些结果确实只服务日志，那就删掉四个字段，别留"看起来像监控"的死数据。

   在二选一落地之前，往这个文件加覆盖率的收益上限很低 —— 断言的对象本身没有消费者。

`logging.rs` / `telemetry.rs` 曾登记为"已知缺口不排期（启动路径，收益低）"。**这条判定
已被 §3.1.3 的台账取代**：`logging.rs` 的三层装配（filter / db-metrics layer / fmt layer
× json/plain × tracer）都能在 `tracing::subscriber::with_default` 作用域里执行，不需要起进程，
"启动路径不可测"不成立；`telemetry.rs` 仍留在缺口里（它构造全局 tracer provider，属于进程级
一次性初始化）。

### 3.1.3 「提升到 30%」的逐文件台账（2026-10-28 落地，P2）

§3.1 的 P2 要的不是"把下限改成 30%"—— 那正是 2026-09-19 `188 files red` 的形态：
188 个文件报的是 `was X%, delta=+0.0%`，**没有回退，只是从没达标**。要的是
"逐文件记账的一次性任务"：每条文件各自的起点、目标、缺口性质、下一步。

台账 `scripts/ci/coverage_raise_queue.json`，守卫 `scripts/ci/check_coverage_raise_queue.py`。

| 文件 | 记账起点 | 目标 | 性质 | 结局（2026-10-28 关账） |
|---|---|---|---|---|
| `src/server/router.rs` | 0.0%（2026-09-19 bootstrap 冻结值） | 30% | **陈旧**：lib 腿 `0/35` 只是没跑集成路径；integration 腿 29/35 = 82.9%（§3.1.2） | **关闭**，不补测试（实测 82.9%） |
| `src/tasks/mod.rs` | 0.0% | 30% | **陈旧**：4 条 tokio 用例 2026-10-26 才加入，`0/203` 是加测试之前的读数 | **关闭**，lib 腿 183/267 = 68.54% |
| `synapse-common/logging.rs` | 0.0% | 30% | **真缺口**：49 行、零测试，唯一调用点 `src/server/telemetry.rs::init_logging` | **关闭**，补测试 + 修两个缺陷后 96/199 = 48.24%（§3.1.3.1） |

五条机制，缺一条这份台账就会变成"第二个真相"：

1. **`target` 不是 floor**。`check_file_coverage.py --raise-queue` 每轮打印
   `[RAISE] path: 12.3% → 30%（还差 17.7 个百分点）`，但**不参与退出码**。把它当绝对下限
   ⇒ 今天立刻 3 条红，而这三条一条都没回退过。
2. **达成必须删条目**。基线 `>= target` 时守卫判红并打印 `delete this entry`。
   没有这条，条目会永远挂着、没人再看 —— `AGENTS.md` 铁律 8 的变体。
3. **记账起点不得高于当前基线**。`opened_at > baseline` 说明该文件的 floor 被
   `--allow-lower` 或手改下调了，"从 40% 提到 30%"是自相矛盾的账，必须对齐。
4. **关账要拿数字说话**（判据 9–11）。条目删到最后一条时，"删空"与"关账"是两件事：
   空 `entries` 必须配 `closed`（`at` / `why` / `measurements`），封存值全部 `>= target`；
   `closed` 在册时 `entries` 必须为空（重新开账要先删收尾块）。
5. **"测到了" ≠ "基线收了"**（判据 12）。关账记的是**具名腿上的实测值**，而基线抬升要等
   下一轮真实覆盖率跑到 `Commit coverage baseline (auto-ratchet)`；两者之间必须由
   `closed.absorption_pending` 说明"等哪一轮"，吸收完成后该字段**必须删除**。

两条容易被忽略的细节：

- **`[RAISE-NODATA]` 是独立标签**：报告里没有该文件 ≠ 0%。这与 `594 个条目保留自上次`
  是同一类静默风险 —— 缺文件时棘轮不红，只是"少比了几条"；
- **`production_entry` 会被验证存在**："有生产入口"是 P2 的前提，而前提可以用一条
  `exists()` 检查 ⇒ 文件被删/改名后台账自己报红，而不是继续以断言形式躺在文档里。

验证（本次实跑，2026-10-28）：

| 场景 | 结果 |
|---|---|
| 合成 lcov：0% 文件 + target 30 | `[RAISE] … 0.0% → 30%`，退出码 **0**（不是 1） |
| 合成 lcov：40% 文件 + target 30 | `[RAISE-DONE] … 40.0% ≥ 30%`，退出码 **0** |
| 报告缺文件 | `[RAISE-NODATA]`，**不**报成 0% |
| 台账不存在 / `entries` 为空且无收尾块 | 退出码 **2**（fail-closed），后者信息含 `dropped silently` |
| 守卫：基线 30% 而条目 target 30% | 退出码 **1**，`delete this entry` |
| 守卫：`opened_at` 20% 而基线 5% | 退出码 **1**，`was lowered` |
| 守卫：`closed` 在册但仍有条目 | 退出码 **2**，`reopened ledger` |
| 守卫：`closed.measurements` 写 12% | 退出码 **2**，`only legal once every row is met` |
| 守卫：基线仍 0.0% 而收尾块无 `absorption_pending` | 退出码 **2**（本次关账时**真实命中过**，见下） |
| 守卫：基线已 82.9% 仍留 `absorption_pending` | 退出码 **2**，`is stale` |
| `--mutation-check` | **8** 次注入（行级 3 + 状态级 3 + 吸收级 2）各命中 1 条 |
| `tests/unit/coverage_raise_queue_tests.rs` | **12/12** 绿（0.20 s，真起 `python3` 子进程） |

> **关账之后还剩什么**（别把 `closed` 读成"CI 已经确认"）：
>
> 1. ~~三条实测值在基线里仍是 `0.0`~~ ⇒ **2026-10-28 晚已吸收**：基线迁移（§3.1.7）把
>    `src/server/router.rs` 抬到 **94.29%**、`src/tasks/mod.rs` **68.54%**、
>    `synapse-common/logging.rs` **92.31%**（CI 合并报告口径，比具名腿更高）。
>    判据 12 立刻要求删掉 `closed.absorption_pending` —— 正是它设计的生命周期，
>    本轮**真实命中过一次**（preflight 拦下了这笔提交）。收尾块现在记 `absorbed_at`。
> 2. **134 个文件 + 6 条 core-path 的逐条定性**：完整 lcov 已到手（§3.1.5 / §3.1.7），
>    分母口径已进基线；遗留 128 条低于基线的归因结论见 §3.1.7.4。

#### 3.1.3.2 关账把主车道弄红了：`--raise-queue` 把"完成"读成"丢失"（2026-10-28 已修）

关账后第一份完整报告（run 40649970973，674 个文件、`Run coverage` 首次转绿）的
`Per-file coverage ratchet` 红了，但红的原因与覆盖率无关：

```text
Coverage raise queue is unusable: raise queue scripts/ci/coverage_raise_queue.json is empty
  — the 30% lift task was dropped silently
```

`scripts/check_file_coverage.py::load_raise_queue` 当时对 `entries` 为空**无条件**判红。
而"达成即删"的终态恰好就是 `entries: []` + `closed` 收尾块 —— 于是**按规矩关账**
反而让 674 个文件根本没被比较过。这条判据的本意是"任务不许被静默丢弃"，
它把"完成"与"丢弃"混成了同一种形态（与 §3.1.3 判据 9–11 修的是同一对概念，
只是这次漏在了读台账的另一端）。

修法：`load_raise_queue` 返回 `(entries, closed)`，空 `entries` + 有 `closed`
⇒ 打印 `[RAISE-CLOSED]`（含封存值与 `absorption_pending`）并**不参与退出码**；
空 `entries` 且无收尾块 ⇒ 仍是 exit 2。守卫补两例
（`the_ratchet_accepts_a_closed_ledger` / `the_ratchet_still_rejects_an_empty_ledger_without_a_closure`），
12 → **14** 例。

#### 3.1.3.1 `logging.rs` 实施记录（2026-10-28）：补测试时撞出两个真缺陷

补测试本身是机械的，**但"能写测试"的前提是先把它拆成可测形状** —— 而拆分过程中暴露的两条
都属"生产路径已经坏了、没有任何测试能看见"：

**① 二次安装是 `panic`，不是 `Err` ⇒ 调用方的错误分支是不可达代码**

原实现最后一步是 `.init()`（`SubscriberInitExt`），全局 dispatcher 已存在时它 **panic**。
而唯一调用点 `src/server/telemetry.rs:39` 写的是：

```rust
if let Err(e) = crate::common::logging::init_logging(&config.logging, tracer_provider) {
    tracing::error!("Failed to initialize logging: {e}");
    std::process::exit(1);
}
```

那条 `Err` 分支**从来没可能执行过**：失败形态是 panic，不是返回值。改成
`tracing::subscriber::set_global_default(subscriber)?` 后分支才可达，退出码与日志也才有内容。
用例 `a_second_install_is_reported_instead_of_panicking` 断言 `Err` 文本含
`already been set`（先抢装一次 `Registry::default()`，无论谁先抢到槽位，**第二次**必为 `Err`）。

**② `RUST_LOG=`（设为空）会让整个进程**静默**——连"日志已初始化"都不打**

根因不在本仓代码，而在 `EnvFilter::try_from_default_env()` 对**空值**的语义：它返回 **`Ok`**，
得到的是**零指令过滤器**，而零指令过滤器匹配不到任何事件 —— 于是整条日志链路既没有报错、
也没有输出。

决定性证据不是日志行数，而是一个 30 行独立探针（`tracing-subscriber =0.3.23`，与 `Cargo.lock`
同版本；装一个计数 `Layer` + `EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))`，
发一条 INFO、一条 DEBUG）：

| `RUST_LOG` | `try_from_default_env()` | 到达 layer 的事件数 |
|---|---|---|
| 未设置 | `Err` | **1**（INFO 通过，DEBUG 被 level 拦掉） |
| `=`（空串） | **`Ok`** | **0** |

`Logging initialized: level=…, format=…` 这一行同样被吞 ⇒ 运维既看不到业务日志，也看不到
"日志系统已就绪"。触发面很宽：compose 的 `RUST_LOG:`（冒号后留空）、K8s ConfigMap 空值、
systemd `Environment=RUST_LOG=`、`export RUST_LOG=$(...)` 取到空串。

修法是先判"这条变量到底有没有内容"再交给 `EnvFilter`：

```rust
fn rust_log_is_useable(raw: &str) -> bool { !raw.trim().is_empty() }

match std::env::var("RUST_LOG") {
    Ok(raw) if rust_log_is_useable(&raw) => EnvFilter::try_new(raw)      // 解析失败 ⇒ 回落
        .unwrap_or_else(|_| EnvFilter::new(fallback_directive(&config.level))),
    _ => EnvFilter::new(fallback_directive(&config.level)),
}
```

修后 `RUST_LOG=` 与"未设置"走同一条回落路径：同一测试二进制、只改环境，两种情况下事件计数
一致（`logging::tests` 5 条全绿）。这里保留 `try_new`（而不是 `EnvFilter::new`）是为了不改变
"非法 `RUST_LOG` ⇒ 回落配置级别"的既有语义：`new` 是 lossy 的，会丢掉坏指令、留下半截过滤器。

**③ 可测性重构（生产 API 内部拆分，行为不变）**

`fallback_directive`（纯字符串策略）/ `build_subscriber`（返回
`Box<dyn Subscriber + Send + Sync>`，四分支收成一个安装点）/ `init_logging`（只负责安装 + 打
banner）。`_tracer_provider` 改名 `tracer_provider` 并写清它**只是存在性标志**：provider 早在
`TelemetryService::initialize` 里 `global::set_tracer_provider` 过了，这里只决定要不要挂
`OpenTelemetryLayer`。

> ⚠️ 两条实测教训（都不是"手滑"，是同一种思维缺陷的两个层级）：
>
> 1. **读输出对错行**。第一次看测试输出，把"`RUST_LOG=` 下仍打印 DEBUG"当成"空值绕过配置级别"
>    写进了注释 —— 实际是 8 次循环的输出错位。改用计数（`grep -c`）才二分清楚。
> 2. **计数模式本身有歧义**。改用计数后又踩第二次：`grep -c smoke` 数出 **12**，而不是预期的
>    8 —— 因为 span 名 `logging_smoke` 也含 `smoke`，4 条 DEBUG 行（`second event`）的 `spans`
>    字段把 span 名一起打了出来。同一模式在"未设置"与"空值"下都数出 12，**看起来完全一致**，
>    结论碰巧是对的，但证据是错的。
>
> ⇒ 结论落在**日志文本**上就要先怀疑"文本里还有谁在说同一个词"。这条（"空值 ⇒ 零指令过滤器"）
> 最后用**不依赖日志文本**的探针定死（`ok=false / events=1` vs `ok=true / events=0`）。
> **判据建立在日志文本上，最硬的自证是换一个不读日志的观测面。**

### 3.1.4 PR 车道的覆盖率（2026-10-28 落地，P2）

`Code Coverage` 只在 main push / schedule / dispatch 上跑，而且是全量口径（实测 67 分钟、
含 1424 条集成测试）—— 放在每个 PR 上等于给每次推送加一小时。于是 PR 作者**在合并前
看不到**自己这次改动把覆盖率带到了哪里。新车道 `pr-coverage` 把范围收到"改动落到的那几个
crate"：

| 环节 | 实现 | 判据 |
|---|---|---|
| 范围 | `scripts/ci/affected_crates.py`（`git diff base...head` → `cargo metadata` 前缀匹配） | 认不出的 `.rs` ⇒ exit 2；`--self-check` 要求每个跟踪的 `.rs` 都有归属（当前 1156 个） |
| 采集 | `scripts/ci/run_pr_coverage.sh <crate>…`（`cargo llvm-cov -p <crate> --lib`） | 每条腿显式 `-p` + `--lib`，且 `--no-report` 与 `report` 分开；缺腿降级合并、零腿不写空报告 |
| 判据 | `scripts/ci/check_changed_line_coverage.py` | 变更文件必须在报告里（`NODATA` ⇒ exit 2）；变更行覆盖率低于 `--min-percent` ⇒ exit 1 |
| 哨兵 | `ci-summary` 的 `require_lane "PR Coverage"` | PR 且非纯文档 ⇒ 车道 skip 也算红（"空跑不等于验证"） |

**为什么判据是"变更行"而不是"和基线比"**：PR 车道只跑受影响 crate 自己的 lib 测试，
读数是**下界**，与合并基线不可比。实测（本机，2026-10-28，同一份代码的两种腿）：

| 文件 | 合并基线 | 单腿（根 crate lib） | 差异来源 |
|---|---|---|---|
| `src/bin/healthcheck.rs` | 10.91% | **0.00%** | bin 目标不在 `--lib` 腿里 |
| `src/bin/synapse_worker.rs` | 38.19% | **0.00%** | 同上 |
| `src/server/mod.rs` | 32.30% | **22.63%** | 由集成测试覆盖的部分不在这一步 |

拿这种数字去比对基线，几乎每个被改到的文件都会红 —— 与 §3.1 的 `188 files red` 同一形态。
所以本车道**不读 `coverage_baseline.json`**：`tests/unit/pr_coverage_lane_tests.rs`
有一条断言禁止三个脚本出现 `--baseline`（注释里解释"为什么不用它"是允许的）。

**默认阈值 `0` 是显式决定，不是遗漏**：单腿下界意味着"改动只被集成测试覆盖"的 PR
（例如在 `synapse-web` 里加 route handler）会**常态假红**。真正带牙齿的是三条 fail-closed：
未知 `.rs` 路径、变更文件不在报告里、任一腿失败。抬阈值（`0 → 30 → 50`）要先攒样本、
看清"新增行的单腿命中率"分布再决定 —— 与 `--new-file-floor` 的阶梯口径一致。

**成本实测**：`synapse-cache` 一条腿 —— 95 条用例、10 个文件、LF 2630 / LH 1615 = 61.41%，
**热 target 25 s**（冷构建另计）。job 的 `timeout-minutes: 20` 是判据不是期望：超时基本
只说明范围算错了（把整个 workspace 当成"受影响"），该回去看映射而不是加超时。

**边界**（别把这读成"PR 上有完整覆盖率"）：

- 集成测试腿不在 PR 车道（42 分钟）。`tests/` `benches/` `examples/` 里的变更文件按
  "非 lib target"跳过，**打印数量**、不计入分母也不算 `NODATA`；
- `synapse-storage` 的 db_tests 需要播种好的模板库：job 里按需跑 `prepare_test_db.sh`，
  脚本侧再自检一次（模板表数 < 100 ⇒ 报错并给出可粘贴的命令）；
- 不用 `--all-features`：feature 互斥与历史 OOM 都记在主车道注释里，这里沿用
  `--features test-utils`，且**只在该 crate 定义了这个 feature 时才加**
  （`synapse-cache` / `synapse-test-utils` 没有它，硬传会让 cargo 直接拒绝）。

本地复现（与 CI 同一条命令）：

```bash
python3 scripts/ci/affected_crates.py --base origin/main --format space   # 范围
bash scripts/ci/run_pr_coverage.sh synapse-common                        # 采集
python3 scripts/ci/check_changed_line_coverage.py \
    --report coverage/lcov.info --base origin/main                       # 判据
```

守卫 `tests/unit/pr_coverage_lane_tests.rs` **13 例**：映射表与 fail-closed、变更行
判定的四种形态（含"只有删除"与"文件不在报告里"）、采集器的三条探针（假 cargo：旗标顺序 /
缺腿降级 / 零腿不写空报告）、车道形状（PR 专属、有超时、不调全量脚本）、以及"不读基线"。

### 3.1.5 两条 `[CORE]` 的定性（2026-10-28）：不是缺测试，是**声明文件**

第一份完整报告（674 个文件）里 132 条失败中有 2 条 `[CORE]`：

```text
[CORE] synapse-services/auth/credential_auth.rs: 0.0% < 70% (was no baseline)
[CORE] synapse-services/auth/token_auth.rs:     0.0% < 70% (was no baseline)
```

「was no baseline」是第一个线索：这两条不是新写的东西，而是**基线里从来没登记过**。
查 lcov 才看清关键数字 —— 两个文件各 94 / 55 行，却只有 `LF:2 / LH:0`：

| 文件 | 源码行数 | lcov | 内容 |
|---|---|---|---|
| `synapse-services/src/auth/credential_auth.rs` | 94 | `LF:2 LH:0` | 只有 `pub trait CredentialAuth` 的方法签名（`#[async_trait]`） |
| `synapse-services/src/auth/token_auth.rs` | 55 | `LF:2 LH:0` | 只有 `pub trait TokenAuth` 的方法签名 |

那 2 行是 `#[async_trait]` 展开出的**默认体垫片**，而两个 trait 的所有方法都被实现方覆盖
（`auth/mod.rs` 的 `impl TokenAuth for AuthService` / `impl CredentialAuth for AuthService`，
以及 `test_mocks.rs` 的 `FakeTokenAuth` / `FakeCredentialAuth`）⇒ 垫片永远不可达。
**文件里没有函数体，就没有可执行行** —— 这是结构性 0%，与"没写测试"无关。

全仓同形态只有 3 个（`LF ≤ 3` 且 `LH == 0`）：

| 文件 | lcov | 说明 |
|---|---|---|
| `synapse-services/…/credential_auth.rs` | `2/0` | trait 声明 |
| `synapse-services/…/token_auth.rs` | `2/0` | trait 声明 |
| `synapse-storage/src/stream_positions.rs` | `2/0` | 模块文档 + 常量表，无可执行体（`[NEW] 0.0% < 50%` 那条就是它） |

**修法**：`check_file_coverage.py` 新增 `DECL_ONLY_MAX_LINES = 3`，判据写死为
「**新文件** + `LF ≤ 3` + `LH == 0`」⇒ 打印 `[DECL-ONLY] <path>: N coverable line(s),
none reachable` 并跳过新文件/核心地板。三条边界都钉住了：

- 只在 `is_new` 分支生效 —— 基线已知的文件照旧走回归比较（0.0 不会低于 0.0）；
- `LF ≤ 3` 是硬上限 —— 4 行有函数体的文件仍然 `[NEW]` 判红（`a_tiny_file_with_bodies_still_fails_the_new_file_floor`）；
- 豁免**必须打印** —— 与 `[EXEMPT]` 同一条规矩：静默放行与"门禁没在工作"无法区分。

**刻意没做的两件事**：

1. **不把这两条加进 `non_unit_coverable_prefixes.txt`**。那个名单的语义是"覆盖率腿
   **执行不到**的路径"（`src/bin/`、`src/main.rs`），且有
   `non_unit_coverable_list_is_exactly_bin_and_main` 守着"只许是这两条"；把 trait 声明
   塞进去会让那个名单变成"所有不想管的文件"。
2. **不给 trait 补测试**。没有函数体可测；真要提覆盖率，只能测实现方（`AuthService`
   的方法），那是另一个文件的事，且已经在基线里（`auth/mod.rs` 69.52%）。

结果：`Core-path coverage failures` 整段消失，失败数 132 → **131**（3 条 `[DECL-ONLY]` 全部打印）。

### 3.1.6 本地复跑 CI 报告的坑：`SF:` 是 `/home/runner/…`

`check_file_coverage.py::_normalize_path` 只剥掉**当前** `ROOT` 前缀；CI 工件里的
`SF:` 是 `/home/runner/work/synapse-rust/synapse-rust/…`，在本地 `ROOT`
（`/Users/…/synapse-rust`）下匹配不上，于是落进「绝对路径但在仓外」分支
（`return p.lstrip("/")`）⇒ 每个文件都成了基线里没有的**新文件**。实测后果：

| 做法 | 读数 |
|---|---|
| 直接跑 CI 工件 | `97 条 [NEW]`（0.0% 的 `src/bin/*` 等）、`608 个条目保留自上次` |
| 先改写前缀 | `132 条`真实失败、`674 测量 / 8 保留` |

```bash
sed 's#^SF:/home/runner/work/synapse-rust/synapse-rust/#SF:#' \
    /tmp/cov_full/lcov.info > /tmp/cov_rel/lcov.info
python3 scripts/check_file_coverage.py --report /tmp/cov_rel/lcov.info \
    --baseline /tmp/baseline_before.json --save-baseline /tmp/baseline_after.json \
    --global-floor 40 --new-file-floor 50 \
    --core-files scripts/ci/core_file_coverage_prefixes.txt --core-threshold 70 \
    --non-unit-coverable scripts/ci/non_unit_coverable_prefixes.txt \
    --raise-queue scripts/ci/coverage_raise_queue.json
```

⚠️ 一定要把 `--save-baseline` 指到 `/tmp`：默认会写回 `--baseline` 指的文件，
拿一份**没归一化**的报告去跑就会把基线改坏。

### 3.1.7 「低于基线」的两种成因分不开（2026-10-28 已修数据格式）

基线原本只存 `line_pct`，于是**分母变大**与**命中行变少**在棘轮眼里一模一样：

| 处境 | 行数 | 命中行 | 读数 | 性质 |
|---|---|---|---|---|
| 新增代码没被覆盖 | 100 → 200 | 60 → 60 | 60% → 30% | **不是退化**（新代码欠测试） |
| 覆盖它的测试不再跑 | 100 → 100 | 60 → 40 | 60% → 40% | **是退化** |

第一份完整报告（674 个文件，run `40649970973`）里有 128 条「低于基线」，按 `line_pct`
一条都分不开。我上一轮的结论「128 条真实下降」**是错的** —— 那时没有任何数据支持
"下降"这个词的另一半含义。

#### 3.1.7.1 修法：把分母跟 floor 一起记，且必须**成对**

`coverage_baseline.json` 每条的格式变成 `{"path", "line_pct", "lines_found"?}`。
`lines_found` **不是**无条件写的 —— 它必须与 `line_pct` 来自**同一次测量**：

| 情形 | `line_pct` | `lines_found` |
|---|---|---|
| 本次更严（`cur >= prev`） | 换成本次值 | 换成本次 LF（同一轮） |
| 本次更低（floor 保持） | 保持旧值 | **保持旧 LF**；旧条目没有 ⇒ 保持缺失 |
| `--allow-lower` 点名 | 落到本次值 | 换成本次 LF |
| 本次报告里没有它 | 保持旧值 | 保持旧 LF |

反例说明为什么"配一对"是硬约束：若在回归分支里写本次 LF，基线会存成
`59.47% × 769 行`，换算回去是 457 行命中 —— 而真实值可能是别的。**一个错的"命中行数"
比"未知"危险得多**：它会让分类器把真退化判成非退化。守卫
`tests/unit/coverage_drift_attribution_tests.rs` 的
`a_regression_keeps_the_old_denominator_not_the_new_one` 与
`a_legacy_entry_does_not_gain_a_denominator_it_never_had` 钉住这两条（变异自证：
把分支改成写 `cur_lf` ⇒ 两条同时红）。

#### 3.1.7.2 分类器：`DENOM` / `LOST-HITS` / `DROP`（未知）

`scripts/ci/classify_coverage_drift.py` 在 `DROP` 之下再分三类，判据是
`LH ≈ LF × pct / 100`（pct 存两位小数，误差远小于 1 行）：

| 类别 | 判据 | 含义 |
|---|---|---|
| `LOST-HITS` | 命中行减少 **> 1 行** | 真回归：测试被删/跳过，或执行路径改道 |
| `DENOM` | 分母变大 且 命中行未减少（容差 1 行） | 新增代码没被覆盖，绝对覆盖量没退步 |
| `DROP` | 两侧 LF 有一侧未知，或变化在 1 行以内 | **无法归因**，需要人看（≠ 回归） |

未知的一侧在输出里打成 `?`（`LF ?→279`），**绝不填 0**：填 0 会让"老的不知道"与
"新的是 0 行"变成同一个字符串，而这个改动的全部价值就是分开这两种处境。
结论行也不再只说「N 条真实下降」，而是把三个数分别列出。

#### 3.1.7.3 基线迁移（本次执行）

用 CI 工件（路径前缀归一化后）跑一次 `--save-baseline`，再**只保留原有 608 条键**：

| 指标 | 值 | 说明 |
|---|---|---|
| 条目数 | 608（不变） | 74 个"基线里没有的新文件"**刻意不写入** |
| 带 `lines_found` | **417** | 其余 191 条落在"低于 floor"或"本次没出现"两支 |
| floor 收紧 | **257** | 都是本次实测值更高，`max()` 的既有语义 |
| floor 下降 | **0** | 迁移只许收紧 |
| 失败项 | **131 → 131**（集合完全相同） | 迁移不引入新红、也不消掉旧红 |

⚠️ 为什么**不写入**那 74 个新文件：`save_baseline` 默认会把报告里的每个文件都写进基线，
而一旦写入，它们就从「新文件受 50% ramp-up 门禁」变成「基线已知、只需不回退」——
等于把门禁**静默放宽**。CI 的 `Commit coverage baseline (auto-ratchet)` 只在 job 绿时跑，
现在它是红的，所以这份基线不会被自动改写；手工迁移时更不能顺手把 70 条 `[NEW]` 洗掉。

#### 3.1.7.4 遗留 128 条：一次性归因的**上限**在哪

老条目没有分母，且**无法事后反推**：想用 `cur_LH / prev_pct` 求 `prev_LF`，前提是
"命中行没变"，而"命中行有没有变"正是要判的问题 —— 那是循环论证。

能做的只剩"源文件自 floor 设定以来有没有变"。floor 设定时间用
`git blame --line-porcelain` 读 `line_pct` 行拿到（608 条里 588 条是
`4565cecd5`（2026-09-19 基线首提）、20 条是 `c952ee2b`），再对源文件跑
`git diff --numstat <floor_rev>..HEAD -- <file>`：

| 桶 | 条数 | 结论 |
|---|---|---|
| 源文件**零改动** | **2** | 可**证**非退化：字节没变，行数/命中变了只能是编译配置或执行路径变了 |
| 只增不删（`-0`） | **4** | 可**证**非退化：新增代码没人跑到，旧行的命中不可能因此减少 |
| 有删改 | **122** | **判不了**：可能分母涨、可能真丢测试，两者都解释得通 |

那 6 条（全部 `floor@4565cecd`）：

| 文件 | 基线 → 本次 | 改动 | LF |
|---|---|---|---|
| `synapse-services/sync_service/event_fetch.rs` | 86.67% → 83.33% | +0/−0 | 150 |
| `synapse-services/cas_service.rs` | 77.10% → 75.70% | +0/−0 | 214 |
| `synapse-services/feature_flag_service.rs` | 99.66% → 93.31% | +24/−0 | 314 |
| `synapse-services/voice_service.rs` | 43.96% → 40.00% | +42/−0 | 355 |
| `synapse-web/routes/oidc/mod.rs` | 72.41% → 68.75% | +10/−0 | 64 |
| `src/bin/healthcheck.rs` | 10.91% → 10.71% | +7/−0 | 56 |

所以这 128 条的正确读法是「**122 条待人工、6 条已证非退化**」，不是「128 条退化」。
另外 44 条落在 core-path（`--core-files` 的 28 个前缀命中 275 个文件），人工排序时先看它们。

#### 3.1.7.5 还剩的缺口（明确不做的事）

- **老条目的分母永远补不上**：除非该文件被 `--allow-lower` 点名，或覆盖率回到 floor 之上
  （那时 `cur >= prev`，成对写入）。这是 3.1.7.1 表里"保持缺失"那一格的直接后果。
- **另一条路是存"上一次测量"**（`last_measurement: {pct, lines_found}`，与 floor 分开）：
  能立刻解决全部 128 条，代价是**每次 main 运行都会让基线整个文件变一遍**
  （608 条 × 每轮都变），而 `Commit coverage baseline (auto-ratchet)` 的 diff 将再也看不出
  "这次哪几个 floor 真的收紧了"。**没做**，留作需要时的显式决策。
- `DENOM` 只说明"命中行没减少"，**没有**说明"新代码该有测试" —— 那是 ramp-up 门禁的事。

#### 3.1.7.6 遗留 128 条的逐层归因（2026-10-28，人工队列从 128 降到 16）

分母进基线之后，**下一轮**的下降能自动归因；但 2026-10-28 之前冻结的条目没有分母。
这一节把能证的证掉，把证不了的压到最小，剩下的才是人要看的那一撮。

方法只有三层，全部可在本地复现：

1. **floor 设定在哪个提交**：`git blame --line-porcelain scripts/ci/coverage_baseline.json`
   读 **`line_pct` 行**（不是 `path` 行 —— 读路径行会全部落到基线首提那一刻，因为后续提交
   只改数字不改路径）；
2. **源文件自那以后怎么变**：`git diff --numstat <floor_rev>..HEAD -- <file>` 的增删行数；
3. **文件里的测试用例有没有被搬走**：`#[test]` / `#[tokio::test]` 的出现次数
   （⚠️ 不能数 `#[cfg(test)]`：外移后原文仍留着 `#[cfg(test)] mod db_tests;` 的**声明**，
   计数不变。`room/mod.rs` 就是这种：975 行测试被搬走，标记数却从 1 变 3）。

| 层 | 判据 | 条数 | core | 结论 |
|---|---|---|---|---|
| **L1 已证** | 源文件 `+0/−0`（字节未变） | **2** | 1 | 非代码退化：行数/命中变了只能是编译配置或执行路径变了 |
| **L2a 已证** | `dele == 0`（纯新增） | **4** | 1 | 非退化：新增代码没人跑到，旧行的命中不会因此减少 |
| **L3 已证口径** | 内联测试用例数变少 | **14** | 4 | 非退化：被搬走的正是**覆盖得最好的那些行**，分子分母同时缩小、比例下降 |
| **L2b 最简解释** | `add > dele`（净增长） | **92** | 34 | 分母增长是**最简解释**，但「删过代码」这条通路没被排除 ⇒ 不是证明 |
| **L4 必须人看** | 净缩减或持平，且测试未搬走 | **16** | 4 | 删代码是丢掉命中的**唯一**通路，只能逐条看 |

**L3 为什么算"已证"**：`9b40cac97`（"内联测试模块外移，7 个 >2000 行文件全部降到 1557 以内"）
把这批文件里的 `#[cfg(test)] mod db_tests { … }` 整体搬到独立文件。内联测试代码**同样进
llvm-cov 的分母**且几乎 100% 命中，所以它一搬走，"高覆盖的一大块"就离开了统计口径 ——
`room/mod.rs` 975 行、`membership/mod.rs` 1239 行、`device/mod.rs` 1235 行都是这么少的。
这条与 §3.1.7 开头的两个例子同源：**比例下降不等于覆盖退步**。

**L4 的 16 条（这就是人工队列，core 4 条先看）**：

| 文件 | 基线 → 本次 | 增/删 | LF | core |
|---|---|---|---|---|
| `synapse-web/routes/handlers/auth_discovery.rs` | 45.10% → 21.62% | +13/−28 | 37 | core |
| `synapse-storage/user/storage.rs` | 79.83% → 64.23% | +456/−458 | 710 |  |
| `synapse-services/wiring/admin.rs` | 97.36% → 91.63% | +124/−178 | 263 |  |
| `synapse-services/e2ee_audit/audit_service.rs` | 81.17% → 75.56% | +8/−87 | 180 |  |
| `synapse-services/wiring/federation.rs` | 94.12% → 90.00% | +4/−27 | 20 |  |
| `synapse-cache/error.rs` | 33.33% → 30.00% | +5/−5 | 90 |  |
| `synapse-services/saml_service.rs` | 40.47% → 37.14% | +120/−157 | 999 |  |
| `synapse-services/registration_service.rs` | 92.26% → 89.47% | +1/−7 | 152 |  |
| `synapse-storage/media_quota/repository.rs` | 90.62% → 88.20% | +89/−222 | 161 |  |
| `synapse-web/routes/admin/register.rs` | 66.67% → 65.33% | +14/−20 | 199 | core |
| `synapse-services/auth/login.rs` | 94.59% → 93.78% | +43/−96 | 209 | core |
| `src/bin/synapse_ledger_export.rs` | 59.06% → 58.67% | +1/−1 | 150 |  |
| `synapse-storage/dehydrated_device.rs` | 97.75% → 97.41% | +115/−124 | 657 |  |
| `synapse-services/sliding_sync_service/state.rs` | 95.38% → 95.08% | +6/−12 | 61 |  |
| `synapse-e2ee/device_keys/models.rs` | 98.98% → 98.97% | +4/−21 | 292 | core |
| `synapse-services/sms_provider/aliyun.rs` | 98.45% → 98.44% | +1/−3 | 192 |  |

#### 3.1.7.7 四条 core L4 的逐条裁决（2026-10-28，全部带逐行证据）

L4 里 4 条落在 core-path，先看这 4 条。判据不看比例，直接读 lcov 的 `DA:` 行
（哪几行**现在**没被执行），再对回源码：

| 文件 | 当前 | 未执行的行 | 裁决 |
|---|---|---|---|
| `synapse-web/routes/handlers/auth_discovery.rs` | **7/36 = 19.4%** | `build_oidc_discovery` **整函数**（18–47）+ 成功返回 59 | **真缺口，已补（见下）** |
| `synapse-web/routes/admin/register.rs` | 126/186 = 67.7% | 错误映射 `map_admin_register_service_error`、`extract_registration_client_ip`、captcha 分支 | 真缺口（多为错误/边缘分支），**非本轮退化** |
| `synapse-services/auth/login.rs` | **186/192 = 96.9%** | 6 行：`PasswordHashError::Timeout` 分支 + 3 处 tracing | **无需动作**（降幅来自删掉了已覆盖的行） |
| `synapse-e2ee/device_keys/models.rs` | **273/276 = 98.9%** | 3 行：`default_json_object`（serde 缺字段时的默认值 fn） | **无需动作**（同上） |

**`auth_discovery.rs` 的缺口不是"少写了几行测试"**：`get_auth_metadata` 的**成功路径**
一条用例都没有 —— 全文 36 行可执行代码，只有 `M_UNRECOGNIZED` 那条错误路径被执行过
（7 行）。而且 2026-10 新加的 RFC 8414 §2 逻辑（"没有机密客户端时只广告 `none`"）
**此前没有任何断言**，客户端正是拿这个字段决定用什么方式换 token；只广告
`client_secret_basic` 会让只会用 `client_secret_post` 的 SDK 静默退回密码登录。

已补两条（`tests/integration/api_auth_routes_tests.rs`，走 `setup_fresh_test_app_with_config`
把内置 OIDC 打开）：

- `test_auth_metadata_advertises_public_clients_only_without_confidential_secrets`
  —— 无 `client_secrets` ⇒ 只广告 `["none"]`，并校验三个端点都由 `issuer` 拼出；
- `test_auth_metadata_advertises_secret_methods_once_confidential_clients_exist`
  —— 登记一个机密客户端 ⇒ `["none", "client_secret_basic", "client_secret_post"]`。

变异自证（各跑一次真集成测试，每次 ~2 min）：

| 变异 | 结果 |
|---|---|
| 退回旧行为（只写 `["client_secret_basic"]`） | **2 条新用例同时红** |
| 漏掉 `client_secret_post`（其余不变） | **只有第二条红**，第一条仍绿 ⇒ 两条用例各管一件事，不是重复 |

顺带说明为什么**不**用文件内 `#[cfg(test)] mod tests` 测它：内联测试代码**同样进该文件的
llvm-cov 分母**且几乎 100% 命中 —— 那样做会把 19.4% 直接"提升"到高位，测的是自己而不是
被测代码（这正是 §3.1.7.6 里 L3 那 14 条揭示的口径）。所以选择集成测试：它从路由进去，
顺带覆盖中间件与序列化。

**试过并否掉的估计器（写下来免得再试一次）**：曾想用
`LF_prev ≈ 文件行数(floor) × (LF_now / 文件行数_now)`、再由 `pct` 反算命中行。
拿 6 条已证样本校准就破了 —— 3 条"只增不删"的算出 `−3 / −3 / −5` 行损失，
而它们**不可能**丢命中（没删任何行）。误差来源正是 L3 那条通路：`floor` 时文件里
有内联测试，`LF_now/行数_now` 已经不含它们，于是 `LF_prev` 被系统性低估。
**分母口径变过的文件，任何"按行数比例外推"的方法都无效。**

**结论与处置**：

- **20 条可证**（L1 2 + L2a 4 + L3 14）⇒ 不需要动，等它们被重测时 `cur >= prev` 自动成对写入；
- **92 条最简解释是分母增长**（L2b）⇒ 不追；这批的共同点是文件都在长，`pct` 掉得温和；
- **16 条必须人看**（L4）⇒ 人工队列，core 4 条优先；
- **不给这 128 条补 `--allow-lower`**：那是把 floor 下调到当前值，等于承认"以后不许涨回来"，
  与"这些下降大多是口径"这个结论正好相反。`--allow-lower` 只用于**已确认的合法重构**。

#### 3.1.7.8 `admin/register.rs` 的门禁分支：缺口全在"非默认配置"里（2026-10-28 已补 9 条）

§3.1.7.7 把 `routes/admin/register.rs`（67.7%，core）判为"真缺口但非退化"。逐行读 `DA:`
之后病因很具体：**60 条未命中行全部落在非默认配置分支上**。

| 分支 | 未命中行 | 为什么默认配置下永远走不到 |
|---|---|---|
| `ensure_admin_registration_ip_policy` | 135–152 | `ip_whitelist` 默认空 ⇒ 第 132 行就 `return Ok(())` |
| `extract_registration_client_ip` | 99–102 | 只被上一行调用 |
| captcha 三段 | 175–192 | `require_captcha` 默认 `false` |
| 人工审批两段 | 196–203 | `require_manual_approval` 默认 `false` |
| 未启用 / 缺 `shared_secret` | 220 / 225 / 255 | 测试配置里是开着的 |
| 字段校验失败 | 248 | 既有用例都发合法 payload |
| `map_admin_register_service_error` | 83–95 | 需要"合法 nonce + 错 mac"才打到 |

这不是"数字不好看"：**IP 白名单与人工审批令牌是授权控制**。它们默认关着，所以从来
没有用例走过；而运维一旦打开，正是最需要有人验证它们真的在把关的时候 —— 配错的表现是
"任何人知道 nonce 流程就能注册管理员"。

**顺带纠正一处"绿了但测的是另一层"**：`api_input_validation_tests.rs` 里那两条
`x-forwarded-for` 用例（8.8.8.8 ⇒ 403、127.0.0.1 ⇒ 200）断言的是 **`LocalhostGuard`**
（消息 `Admin registration is only available from localhost`），不是 handler 的白名单
（消息 `Admin registration source IP is not allowed`）。两者都返回 403，只判状态码会
把"guard 拦住了"当成"白名单生效了"。新用例把 `allow_external_access` 打开让 guard 放行，
才真正打到 handler 那一层，并**断言 message**。

新增 `tests/integration/admin_register_controls_tests.rs`（9 条）：

| 用例 | 钉住的东西 |
|---|---|
| `nonce_is_refused_when_admin_registration_is_disabled` | 未启用 ⇒ 400 + 明确措辞 |
| `nonce_is_refused_when_the_shared_secret_is_missing` | 配了一半（enabled 但无密钥）⇒ 400 |
| `nonce_is_refused_when_the_peer_is_outside_the_ip_whitelist` | 对端不在白名单 ⇒ 403，且是 handler 那句 |
| `nonce_is_refused_when_the_forwarded_client_is_outside_the_ip_whitelist` | 反代场景：对端回环、转发头外部 ⇒ 403；**同 app 内**转发头回环 ⇒ 200（反向对照） |
| `register_demands_a_captcha_id_when_captcha_is_required` | 缺 `captcha_id` ⇒ 400 |
| `register_demands_a_captcha_code_when_the_id_is_present` | 有 id 缺 code ⇒ 400，且点名缺哪个 |
| `register_requires_an_approval_token_that_actually_matches` | 缺 ⇒ 400；错 ⇒ 403；**前缀** 也不许过 |
| `register_rejects_a_too_short_password_before_touching_the_service` | 校验失败 ⇒ 400 `M_INVALID_PARAM`，不掉进服务层变 500 |
| `a_wrong_hmac_is_a_client_error_not_a_server_error` | 合法 nonce + 错 mac ⇒ 400 且点名 HMAC（此前该映射函数零执行） |

变异自证（每次都跑真集成测试）：

| 变异 | 结果 |
|---|---|
| `if false && !ip_matches_whitelist(remote_ip, …)`（对端白名单失效） | 对端用例红 |
| `if false && !ip_matches_whitelist(client_ip, …)`（转发头白名单失效） | 转发头用例红（**另一个** if，两个变异各打一处） |
| `approval_token_matches` 恒返回 `true` | 审批令牌用例红 |

实测：9/9 绿（15.7 s）。覆盖率增益不在这里给数字 —— 下一轮完整报告出来才算数。

#### 3.1.7.9 十二条非 core L4 的裁决（2026-10-28，全部不是本轮引入的退化）

§3.1.7.7 处理了 4 条 core，剩下 12 条非 core。判据同前：读 `DA:` 定位缺口 → 对回源码 →
再问一句"这个函数在 floor 那一刻存在吗"（`git show 4565cecd:<file> | grep -c "fn X"`）。
**"现在没测"和"本轮变成没测"是两件事**，这一栏决定它是缺口还是噪音。

| 文件 | 基线 → 本次 | 增/删 | LF |
|---|---|---|---|
| `synapse-storage/user/storage.rs` | 79.83% → 64.23% | +456/−458 | 710 |
| `synapse-services/wiring/admin.rs` | 97.36% → 91.63% | +124/−178 | 263 |
| `synapse-services/e2ee_audit/audit_service.rs` | 81.17% → 75.56% | +8/−87 | 180 |
| `synapse-services/wiring/federation.rs` | 94.12% → 90.00% | +4/−27 | 20 |
| `synapse-cache/error.rs` | 33.33% → 30.00% | +5/−5 | 90 |
| `synapse-services/saml_service.rs` | 40.47% → 37.14% | +120/−157 | 999 |
| `synapse-services/registration_service.rs` | 92.26% → 89.47% | +1/−7 | 152 |
| `synapse-storage/media_quota/repository.rs` | 90.62% → 88.20% | +89/−222 | 161 |
| `src/bin/synapse_ledger_export.rs` | 59.06% → 58.67% | +1/−1 | 150 |
| `synapse-storage/dehydrated_device.rs` | 97.75% → 97.41% | +115/−124 | 657 |
| `synapse-services/sliding_sync_service/state.rs` | 95.38% → 95.08% | +6/−12 | 61 |
| `synapse-services/sms_provider/aliyun.rs` | 98.45% → 98.44% | +1/−3 | 192 |

**结论：12 条没有一条是本轮引入的回归。** 缺的测试在 floor 那一刻就不存在；pct 下降来自
"分母变大 + 删掉已覆盖的行"的组合。分成两堆：

**A. 有可定位的测试缺口（5 条，值得补）**

| 文件 | 未执行的东西 | floor 时已存在 | 性质 |
|---|---|---|---|
| `saml_service.rs` | `sanitized_base_config`（**配置脱敏**，16 行） | 是 | 安全语义没人验：脱敏漏一个字段就是密钥泄漏 |
| `e2ee_audit/audit_service.rs` | `verify_device` + `DeviceVerificationStatus` 组装（28 行） | 是 | 设备验证审计的主路径 |
| `user/storage.rs` | `set_deactivation_status_batch`（批量停用，10 行）与 `count_non_deactivated_users_by_app_service`（11 行） | 是 | 管理面批量操作；**有调用方**（`admin_user_service.rs:689`、`user_service.rs`）但无测试 |
| `cache/error.rs` | `hit_rate()` / `degradation_rate()`（15 行）+ `From<ConnectionTimeout>` | 是 | 缓存健康度指标没断言；文件本身只有 30% |
| `registration_service.rs` | `login()` 的 6 行 | 是 | 单个错误分支 |

**B. 量化效应 / 口径（7 条，无需动作）**

| 文件 | 为什么不用动 |
|---|---|
| `wiring/federation.rs` | 可执行行只有 **18 行**，1 行未命中就是 5.6 个点 —— 比例在这里没有信息量 |
| `sliding_sync_service/state.rs` | 61 行里 3 行未命中（58/61 = 95.08%），同上 |
| `sms_provider/aliyun.rs` | 2/186 行（错误日志与 `RequestId` 解析） |
| `dehydrated_device.rs` | 16/610 行零散 |
| `media_quota/repository.rs` | 9/142 行 |
| `src/bin/synapse_ledger_export.rs` | `main()` / stdout 锁 —— 二进制入口，`--non-unit-coverable` 已豁免新文件门槛，回归比较仍保留 |
| `wiring/admin.rs` | 22/260 行全在**启动装配**（worker topology metrics、push gateway 配置、worker_type 标签），只有端到端起栈才走到 |

**两条方法论备注（都影响过我的结论）**：

- **`DA:` 行是 `LF` 的子集**。`sliding_sync_service/state.rs` 的 `DA` 里 50 行全命中，
  但记录头是 `LF:61 / LH:58` —— 差的那 3 行没有 `DA` 记录（同一行多实例/映射差异）。
  所以逐行缺口**只能用来定位，比例一律以 `LF/LH` 为准**。第一版我按 `DA` 算，得出了
  "这个文件 100% 覆盖但显示 95.08%" 的荒谬对照。
- **`grep "a\|b"` 在这个 CLI 里会静默返回空**（shim 是 toybox，不支持 BRE 的 `\|`）。
  我据此先写下"这两个存储函数零调用方"，实际它们在 `admin_user_service.rs:689` 与
  `user_service.rs` 里都有调用、`user_service_tests.rs` 里还有测试替身。**"搜不到"必须先
  换成 `grep -E` 或专用检索工具再说。**

#### 3.1.7.10 §3.1.7.9 A 组第一条已补：SAML 配置脱敏（2026-10-28）

`sampled` 里 5 条真缺口按风险排序，第一条是 `saml_service.rs` 的 `sanitized_base_config`
—— 它把 SAML 配置交给管理端 UI，必须把私钥、证书、元数据 XML 与文件路径换成布尔量。
漏一个字段不是"覆盖率少一点"，是私钥泄漏。

补了 4 条用例（`mod tests` 内，跟随该文件既有 40+ 条内联测试的约定）：

| 用例 | 断的东西 |
|---|---|
| `sanitized_base_config_never_serializes_secret_material` | 四个哨兵串一个都不许出现在序列化结果里（**反向断言**） |
| `sanitized_base_config_reports_absent_secrets_as_false` | 都没配时三个布尔量必须 `false`（否则"恒 true"也能过） |
| `sanitized_base_config_counts_path_only_secrets_as_present` | 只配 `*_path` 也算已加载（`a.is_some()` `\|\|` `b.is_some()` 的另一半） |
| `effective_config_without_overrides_equals_the_sanitized_base` | 无覆盖时逐键等于脱敏视图 |

变异自证（三处，各跑一次真测试）：

| 变异 | 结果 |
|---|---|
| 把 `sp_private_key` 原样写进脱敏视图 | 第 1 条红 |
| `has_sp_private_key` 恒 `true` | 第 2 条红 |
| 去掉 `\|\| sp_private_key_path.is_some()` | 只有第 3 条红（另两条仍绿 ⇒ 不是重复断言） |

**为什么最后是"整个测试模块外移"，而不是加几条内联用例**：

1. 先按文件约定把 4 条写进 `mod tests` —— 文件从 1605 涨到 1693，`check_file_size_ratchet.py`
   立刻判红（`saml_service.rs: 1605 -> 1693`）。这个文件已经**贴住**自己的历史上限，
   再加一行都非法。
2. 想放 `tests/unit/` 避开分母：但 `saml_service` 与 `synapse-storage::saml` 都在
   `#[cfg(feature = "saml-sso")]` 后面，而**没有任何一条 unit 车道开这个 feature**
   （默认车道 `--features test-utils,burn-after-read,server-notifications`；all-features 车道带
   `-E` 过滤器）⇒ 放出去就是永不执行的死测试。
3. 于是按棘轮自己的建议（"split the file"）把 `mod tests` **整体**搬到
   `synapse-services/src/saml_service/tests.rs`，用 `#[cfg(test)] #[path = ...] mod tests;` 声明：
   `use super::*` 仍能访问私有项，而两个文件分别计行数 —— 主文件 **1605 → 1197**，
   测试文件 **507 行**（< 600），4 条新用例并在里面。

副产品：`saml_service.rs` 从此离上限有 400 行余量；测试文件落在 `*/tests.rs` 口径内，
**不进**生产静默忽略统计（实测 `check_doc_metrics` 的生产桶没变，只有 clippy 扫描文件数 832 → 833）。

⚠️ 仍未变的结论：这 4 条**不能**读成"覆盖率涨了 X 点"。外移之后测试代码不再进该文件的
llvm-cov 分母，所以 `saml_service.rs` 的 pct 会明显上升 —— 那是**口径回归到正确**，
不是新增覆盖；真正的收益是那 39 行生产代码第一次被断言。

#### 3.1.7.11 A 组第 2、3 条（2026-10-28）：一条是**死代码**，一条真补了 5 条

##### 第 2 条改判：`verify_device` 不是"主路径没人测"，是 N+1 消除留下的尸体

§3.1.7.9 把它记成「设备验证审计的主路径（28 行，值得补）」。**这条假设是错的**，
四次核验依次否掉它：

1. **谁写的**：`git log -S "pub async fn verify_device("` → `eaf733c0f`（`query!` 迁移批）。
2. **谁把它变成孤儿的**：`git log -S "Batch the read queries"` → `a5fbae44b`
   （P2-09 N+1 消除）。该提交在同一文件的 diff 里有一行
   `- let status = self.verify_device(device).await?;` —— 那**就是它唯一的调用点**，
   被内联的批量逻辑替换掉了。此后 `verify_user_devices` 不再经过它。
3. **现在有人调吗**：全仓 `\.verify_device\(` **零命中**（含 `.worktrees/gate-test`、
   `.worktrees/w6b5` 两个 worktree）。`CrossSigningVerificationService` 除本文件之外
   只有 `tests/integration/e2ee_audit_service_tests.rs` 在构造 —— 也就是说**整个类型
   都没有生产消费方**（`E2eeAuditService` 有：`routes/e2ee/backup.rs:198`；
   但 `CrossSigningVerificationService` 从未接线到任何路由或 wiring）。
4. **就算接线也调不了**：入参 `DeviceInfo` 的 3 个字段**全私有**、没有 `pub fn new`、
   也没有 `Default` ⇒ 本模块之外**构造不出来**。它在设计上只是
   `get_user_devices()` → `verify_device()` 的中间类型。

所以那 28 行未执行是**不可达**，不是缺测试。从这里补测试只有一条路：用
`use super::*` 从模块内部开后门 —— 那测的是自己写的桩，正是本审计（§3.1.7.9 两条备注、
§3.1.7.6 否掉的行数外推估计器）反复拒绝的"把数字做上去"。**A 组因此从 5 条降为 4 条。**

处置二选一，需产品决策。**2026-10-29 已拍板：接线**（不删）。

| 方案 | 内容 | 当时列出的代价 | 结论 |
|---|---|---|---|
| 删除 | 删 `verify_device` + `verify_device_signature` + `check_cross_signing`（−28 行），`DeviceInfo` 收为私有并撤 `mod.rs` 的 re-export | 零调用方 ⇒ 零行为变化 | 未采纳 |
| **接线**（已采纳） | 给 admin 加「单设备验证」端点 | "功能新增、超出本轮；且要同时给 `DeviceInfo` 加构造函数" | **已实施，见下** |

**实施记录（2026-10-29）**——四个改动点，全部围绕"让那条不可达路径变成可达"：

1. `CrossSigningVerificationService::verify_device_for_user(user_id, device_id)`：先按**路径里的
   `user_id`** 取该用户的设备列表，再在其中找 `device_id`，找不到 ⇒ `404`。这样路径的 owner 是
   权威口径，不会出现"按设备 id 全局查、响应里的 owner 与路径对不上"。
   决策说明就地写在 doc comment 里：**不要**把这条路径与 `verify_user_devices` 合并 ——
   合并要么让单设备验证退化成 N+1，要么让批量路径多读 N 次。
   > 原计划要给 `DeviceInfo` 加 `pub fn new`，实际不需要：转换只发生在同一模块内，
   > 保持字段私有反而少一个可以乱构造的入口。
2. 门面：**没有**给 `AdminContext` 加字段 —— 它已贴住行数棘轮上限（1038），加两行即红
   （第一版加了 `cross_signing_verification` 字段，被 `check_file_size_ratchet` 判 `1038 → 1041`）。
   改为把 `E2eeAuditService::verify_device_for_user(self: &Arc<Self>, …)` 做成门面：它持有
   `Arc<PgPool>`，按需构造 `CrossSigningVerificationService`（后者构造时需要
   `Arc<E2eeAuditService>` 来写同一张审计表，因此不能反过来塞进 context）。
   三个 `Storage` 包装的构造成本可忽略，也不引入 `Arc` 自引用。生产路径**仍然真的在调**
   `CrossSigningVerificationService::verify_device_for_user`，接线语义不变。
3. 路由：`POST /_synapse/admin/v1/users/{user_id}/devices/{device_id}/verify`，
   放在**独立模块** `synapse-web/src/routes/admin/device_verify.rs`（`admin/user.rs` 同样贴住
   1139 上限，塞一个 handler 就 `1139 → 1171`）。`admin/mod.rs` 加 `.merge(...)` 并挂在
   `admin_auth_middleware` 保护的那条链上。
   **用 POST 不用 GET**：这条路径会写一行 `e2ee_audit_log`，做成 GET 会让管理面板的刷新把
   审计表撑大 —— 有用例反向钉住这一点（GET 必须 405）。不额外写 `admin_audit` 行：设备级
   结论已由 `e2ee_audit_log` 承载（含 `device_id` 与三项布尔），再记一条只会让同一动作在
   两份审计里各出现一次。
   ⚠️ 副作用：`registered_by` 从 `admin::user` 变成 `admin::device_verify` ⇒ 6 份夹具的
   label、3 张派生表、sidecar 与两份 route-table 都要重导（路由**总数不变**：980）。
4. 契约链（改路由字面量必须整条）：`gen_derived_routes.py --bootstrap` →
   `generate_sdk_ledger_fixtures.sh` + 三次 golden `synapse_ledger_export` →
   `gen_derived_routes.py --check` → `gen_route_handlers.py`（sidecar 写盘）→
   `gen_contract_doc.py` → `gen_route_table.py` 两份视图 → `route_metrics.py --write`。
   ⚠️ 顺序有坑：**`route_metrics.py --write` 必须在 sidecar 重生成之后**跑，否则它按旧
   计数写块，`--check` 再报漂移（本次踩了一次）。

测试（4 条，扩在 `tests/integration/api_admin_user_lifecycle_tests.rs`，不新增测试文件）：

| 用例 | 断的东西 |
|---|---|
| `…reports_unverified_without_cross_signing` | 无签名无交叉签名 ⇒ 三项全 false，且响应回显路径上的 `user_id`/`device_id`；**反向**：同一路径 `GET` 必须 405 |
| `…marks_verified_and_records_audit_entry` | 播种 `self_signing` + 设备签名 ⇒ 三项全 true；并**回查 `e2ee_audit_log`** 里真有一行 `verify_device`（`details.is_verified == true`），防"只算不记" |
| `…404s_for_unknown_device_and_for_another_users_device` | 三种 404：设备 id 不存在 / **设备存在但属于别人** / 用户不存在 |
| `…rejects_non_admin_callers` | 普通用户调用 ⇒ 403 |

变异自证：把 `verify_device_for_user` 的设备查找改成 `.find(|d| d.device_id == device_id || true)`
（= 不按 user 过滤、退化成"取该用户第一台设备"）⇒ 第 3 条红，实测返回
`200` 且 `device_id` 回显的是**另一台**设备 —— 正是"路径 owner 与响应不一致"的形态。恢复后 4/4 绿。

##### 第 3 条已补：`user/storage.rs` 两个管理面函数（5 条用例）

两个函数都有**真实调用方**，但此前零测试（`grep -rln "batch_deactivate_users\|set_deactivation_status" tests/`
返回空）：`set_deactivation_status_batch`（`admin_user_service.rs:689`）与
`count_non_deactivated_users_by_app_service`（`user_service.rs:434`）。补在
`tests/integration/user_storage_tests_migrated.rs`：

| 用例 | 断的东西 |
|---|---|
| `…batch_returns_only_existing_users` | 返回集合**恰好**是库里存在的 id（幽灵 id 不能回写）；**反向**：不在入参里的用户必须毫发无损 |
| `…batch_empty_input_touches_nothing` | 空入参返回空集；**反向**：不得退化成"全库停用" |
| `…batch_is_symmetric_for_reactivation` | `is_deactivated=false` 那一半真的生效（防参数被硬编码成 `TRUE`） |
| `…count_groups_and_excludes_deactivated` | 唯一 appservice 分组计数 = 3（2 个 `FALSE` + 1 个 `NULL` 都算未停用、1 个已停用不计）；**整组停用的键必须消失**而不是报 0；不存在的键不得凭空出现 |
| `…count_maps_local_users_to_empty_string` | `appservice_id IS NULL` 落在空串键下 |

两处口径说明：

- **`is_deactivated` 是 NULL 时算未停用**（`COALESCE(is_deactivated, FALSE)`）——这一支只有
  显式写 NULL 才走得到，所以用直接 SQL 播种（`create_user` 只写 `FALSE`）。
- **空串组只能断言"存在且 ≥1"**：`""` 是全局分组，并行跑的其它用例也在建本地用户，
  断言精确值会变成 flaky。唯一可确定断言的是**本次唯一 appservice_id** 那个分组。

变异自证（1 处，Rust 侧改逻辑、不动 SQL 文本，因为 `SQLX_OFFLINE=true` 下改
`query_scalar!` 的 SQL 会直接编译报 `no cached data`，那不是"测试变红"）：

| 变异 | 结果 |
|---|---|
| `set_deactivation_status_batch` 返回 `user_ids` 而非 DB 回的 `rows`（= 幽灵 id 当成成功） | 只有第 1 条红，报 `batch update must return exactly the ids that exist` |

##### 顺带修掉的真缺陷：census 认不出 `#[path]` 子模块 ⇒ 同一次误判让两个棘轮**一红一绿**

补测过程中撞到 CI 的存量红：`check_sqlx_dynamic_ratio.sh` 报
「生产动态 SQL 增加 **14** 处（106 → 120）」。逐文件对称差（`--list-production-dynamic`
取 `f45128efe^` 与 HEAD 各跑一次）显示**全仓只有一个文件变化**：
`synapse-common/src/test_isolation/tests/prune_tests.rs` 0 → **15**，其余一处未动。

根因是 census 的一个已知弱点被新写法踩到：`f45128efe` 把 `test_isolation.rs` 内联测试里的
prune 回归挪进独立文件，声明写成

```rust
#[cfg(test)]
mod tests {
    #[path = "prune_tests.rs"]   // 实际文件在 test_isolation/tests/prune_tests.rs
    mod prune_tests;
}
```

`collect_test_gated_files()` 只会**从模块名推文件名**（`prune_tests.rs` / `prune_tests/mod.rs`），
推不出带 `tests/` 的目录，于是整份文件落进生产区。失效形态是**双向**的：

- 生产棘轮虚高 15（真实 105 报成 120）⇒ 假红；
- 测试区棘轮同时虚低 15（真实 842 报成 827）⇒ **报"未增加"，把 8 点真实增长洗成绿**。

修法（`scripts/ci/sqlx_query_census.py`）：新增 `enclosing_inline_modules()` 与
`PATH_MOD_DECL_RE`，按 Rust 参考解析 `#[path]` 的基准目录（非 mod-rs 文件从同名目录起算，
`mod.rs`/`lib.rs`/`main.rs` 从同目录起算，属性位于内联模块块内时再逐级拼模块名），
并且**只在 `iter_region_lines()` 判定为 test 区的行上生效**（注释里的 `#[path]` 已被
`strip_code` 抹掉，不会误命中）。

修完的实测与归因（`--list-test-dynamic` 在 `e1379be27` 与 HEAD 的对称差，全仓只有 4 个文件动）：

| 文件 | 变化 | 性质 |
|---|---|---|
| `test_isolation.rs` | 133 → 126 | −7，prune 回归挪出 |
| `test_isolation/tests/prune_tests.rs` | 0 → 15 | +15，同上 |
| `synapse-test-utils/src/lib.rs` | 8 → 4 | −4，建库夹具挪出 |
| `synapse-test-utils/src/db_schema_pool_tests.rs` | 0 → 4 | +4，同上 |

净 **+8** 是上一批真新增的测试用例，其余 11 处只是换文件。基线因此同批调整并就地登记
（见 `scripts/ci/sqlx_dynamic_ratio_baseline` 2026-10-28 两条）：`production 106 → 105`
（用修好的 census 回测 `e1379be27` 实测**本来就是 105** —— 10-26 那条"已收紧到零余量"
多留了 1 个静默额度）、`test_infra 834 → 842`。

连带效应（必须一起收）：census 修好后 `prune_tests.rs` 才第一次进"test 区"，而
`test_ddl_guard` 只扫 test 区 ⇒ 立刻浮出两条此前**完全不可见**的自建 schema DDL
（`prune_backfills_legacy_zero_row_marker_and_spares_it:31:CREATE TABLE`、
`vanished_candidate_is_skipped_not_a_prune_failure:106:DROP SCHEMA`）。两者都属于
`test_ddl_allowlist` 明确收录的"隔离机制自身的测试"，已按新路径登记（旧条目
`test_isolation.rs::tests::prune_backfills…` 随改名同步更新，否则它自己会变成 stale 条目）。

红证明（两条，都是真跑）：

| 变异 | 结果 |
|---|---|
| 把 census 回退到 HEAD 版本（即不解析 `#[path]`） | 新增用例 `cfg_test_gated_path_submodule_is_excluded_from_production` 红，报出 `src/foo/tests/prune_tests.rs` 被算成生产 |
| 只改 `test_ddl_allowlist` 的新增两条中任意一条 | `no_unallowlisted_self_built_schema_in_test_regions` 红（恢复后 9/9 绿） |

#### 3.1.7.12 A 组第 4 条已补：`cache/error.rs` 的健康度指标（2026-10-28）

`DegradationMetrics::hit_rate()` / `degradation_rate()` 直接进监控面板与告警阈值，此前零断言。
补 6 条落在 `tests/unit/degradation_metrics_tests.rs`（**新文件），没有并进
`synapse-cache/src/error.rs` 的内联 `mod tests` —— 理由同 §3.1.7.10：内联测试代码会进该文件的
llvm-cov 分母且近 100% 命中，"补测试"就变成给自己刷分。

| 用例 | 断的东西 |
|---|---|
| `hit_rate_is_zero_when_nothing_has_been_observed` | `0/0` 收敛成 `0.0`，**且不是 NaN**（NaN 会让告警表达式里的比较全部变 `false` ⇒ 不报错也不告警） |
| `hit_rate_puts_both_tiers_in_the_same_denominator` | 3 命中 / 4 次查找 = **75.0**（分母少算一层得 50% 或 100%） |
| `hit_rate_reaches_100_when_every_lookup_hit` | 全命中 = 100.0，上界不被击穿 |
| `degradation_rate_is_zero_when_no_request_was_degraded` | 有回落但 `total_degraded_requests == 0` ⇒ 0.0，**且 `is_finite()`**（不是 inf/NaN） |
| `degradation_rate_counts_only_fallback_operations_in_the_numerator` | 1 次回落 / 4 次降级 = **25.0**（把熔断拒绝也算进分子得 75%，分子=分母得 100%） |
| `degradation_rate_reaches_100_when_every_degraded_request_fell_back` | 全部回落 = 100.0 |

变异自证：把 `hit_rate()` 的分母改成只算本地一层（丢掉两个 Redis 项）⇒ 两条命中率用例红，
实测得 **150.0 / 200.0**。比"数字偏小"更能说明问题：它直接击穿了函数文档承诺的
`[0.0, 100.0]` 区间 —— 面板上会显示 200% 的命中率。恢复后 6/6 绿。

⚠️ 连带（实测踩到）：**新增一个测试文件会牵动 5 处镜像数字**，`check_doc_metrics.py`
一次只报一条，必须逐个跑：
`docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md` §0.4 的「测试文件」行（总数 + `.rs` 数两项）、
`.trae/rules/project_rules.md` 的 `tests/unit` 目录注释、
`docs/synapse-rust-vs-synapse-comparison.md` 的「测试文件数」「测试分层」两行、
`TESTING.md` 的 `TEST_FILES_*` 块（`TESTS_DIR` / `TESTS_DIR_RS` / `UNIT` 三个键）；
外加 `tests/unit/mod.rs` 注册一行。

### 3.2 性能验收只有报告，没有断言

`tests/performance/appservice_scheduler_perf_tests.rs` 的 **2 条用例全部** `#[ignore]`（`reason` 写明
"按墙钟断言在 CI 上会假失败，只报告不断言"）。`TESTING.md:215-217` 自认
"真实、可执行、会变红的性能门禁目前只有 `scripts/ci/compute_perf_gate.sh`（纯计算）与
`scripts/ci/sliding_sync_perf_gate.sh`（需 Postgres）。其余仅作人工参考。"

⇒ 调度器积压、p95/p99 劣化**没有任何自动化拦截**；`P4_ci_gate_integrity` 那张待办表里的
"给 p95 加真实断言（需先定稳定阈值）"至今未做。

#### 3.2.1 已落地（2026-10-25）：一条「折算成数据库往返次数」的门禁

新增 `tests/performance/appservice_scheduler_perf_tests.rs::statistics_per_service_cost_does_not_grow_with_service_count`，
**故意不加 `#[ignore]`**，并接入 CI（`ci.yml` 的 Test & Lint job，`require_tests_ran.sh` 包裹）。

判据：把 `get_statistics` 的**每行读出成本**折算成「几次数据库往返」——

```
reference_roundtrip = 同一 pool 上 SELECT 1::bigint 的平均耗时（机器速度单位）
cost_per_row        = (总耗时 / (N × 迭代)) / reference_roundtrip
断言 cost_per_row < 0.5
```

**为什么不是"复杂度比值"**（这一版是被变异自证推翻的）：

第一版按"每服务成本随规模增长的倍数"写，阈值 1.5（正常实测 0.43–0.78）。注入 N+1 后
每服务成本从 9.9μs 涨到 187μs（**19 倍**），但小规模被同样拖慢，比值只从 0.55 变到 **1.08**
—— **照样 PASS**。原因很朴素：N+1 是"每行固定多一次查询"，每服务成本对 N 仍是常数，
复杂度维度**看不见**它。所以"线性度/复杂度"只能抓"每行扫全表"，抓不到最常见的退化。

实测对照（本机，2026-10-25）：

| 实现 | 每行成本 | 折算往返次数 | 结果 |
|---|---|---|---|
| 正常（一次查询取全部行，摊到 N 行） | 9.9μs（N=40）/ 5.1μs（N=160） | **0.061 / 0.031** | PASS |
| 注入 N+1（每行再 `SELECT COUNT(*)` 扫全表） | 187μs / 195μs | **0.985 / 1.026** | **FAIL** |

阈值 0.5 落在两者之间：正常侧 8 倍余量，退化侧 2 倍超出。分子分母同机同 pool，与机器速度无关。

**能抓什么、抓不到什么**（写清楚，免得被当成万能门禁）：
- 抓：N+1、丢索引导致的全表扫描、把批量化改成逐行往返；
- 抓不到：整体常数变慢（如换了个慢 2 倍的序列化库）—— 那仍需人工基准。

**这仍然只是 1 条门禁**（取代 0 条），覆盖面是"统计查询"一条路径。其余 perf 用例保持 `#[ignore]`：
它们的判据是绝对墙钟，进 CI 只会制造 flake。

### 3.3 `cargo test --doc` 是编译门禁

`AGENTS.md:13` 与 `P5_engineering_2026-09-11.md:56` 都写明：全 workspace 只有 4 个 doc test，
**全部 `#[ignore]`**，曾有一次 rustdoc-only 的 E0106 绿着通过。CI 已改为
`cargo test --doc --locked --workspace`（`ci.yml:612`），但那是**编译**门禁。

⇒ 定性为"已知且登记"，补的价值低（doc 示例本身少）。优先级 **P3**，不建议投入。

## 4 R3：债务（当前不致命，但有复活路径）

### 4.1 572 处 `else { return; }` 残留

实测分布：**静默 `return` 572 处 vs fail-closed 42 处**（比例 13.6 : 1）。

关键定性：`TestContext::build`（`tests/integration/mod.rs:555-560`）已经在 2026-10-18 改成
**panic**，注释写明"`Option` 返回值仅为兼容既有 ~50 个 `let Some(..) else { return; }` 调用点，
实际恒为 `Some`"。所以这 572 处**目前是死代码**，不是活缺陷。

但 §2.1 已经证明：只要未来出现第二个 `.ok()?` 形态的 setup（`with_config` 就是），
这 572 处会**立刻**变成 572 条静默假绿。这是"结构上预留了假绿接口"。

#### 4.1.1 执行方式修正（2026-10-25：改守夹具本体，不盲改 559 个调用点）

原计划是"572 处统一收口"，实际动手时按数据改了做法，理由两条：

1. **559 个调用点里混着"feature 关闭就该跳过"的夹具**。分类后发现 `enabled_app()`、
   `signed_pdu_fixture()` 这类 helper 也返回 `Option`，但它们的 `None` 是**语义**
   （feature 未开 / 前置数据不成立），改成 panic 会把合法跳过变成红。
2. **只要夹具不再 fail-open，调用点就是死代码**。改调用点不增加任何安全性，只是把炸弹搬走；
   而 R1 的根因就在夹具本体（`.ok()?`）。

实际做法：
- 逐个改造了 **7 处**真 fail-open 的夹具（缺库/建池失败 ⇒ `panic!`，与 `TestContext` 同口径）：
  `api_admin_user_lifecycle_tests`、`api_content_scanner_integration_tests`、
  `presence_storage_tests_migrated`、`protocol_compliance_tests`、
  `api_federation_transaction_tests`（MSC4307 fixture）、`migration_search_path_tests::connect_admin`、
  `test_schema_housekeeping_tests::connect_admin`。其中三处原本还带着
  `eprintln!("Skipping …")` —— 明确宣告"要跳过"，却仍被 libtest 记成 `ok`。
- 新增守卫 `scripts/ci/check_test_setup_fail_closed.py`（已接入 preflight）：扫
  `tests/**/*.rs` 里**名字表明是夹具**（`setup*`/`prepare*`/`connect_*`/`*_pool`/`*database*`/
  `*fixture*`/`*context*`/`test_app*`/`enabled_app`）且返回 `Option` 的函数，体内出现 `.ok()?`
  或裸 `return None` 即红。白名单只有 `tests/performance/manual_smoke_tests.rs`（全 `#[ignore]` 的手工冒烟）。
  守卫**先剥整行注释**再加判 —— 首次运行就误报了一次自己刚修好的函数（注释里留着"此前是 `.ok()?`"）。

判据：改造后 `api_admin_user_lifecycle_tests` + `api_content_scanner_integration_tests` +
`presence_storage_tests_migrated` + `protocol_compliance_tests` 共 **40 passed**；
`api_federation_transaction_tests` **13 passed**；两个 unit 族 **4 passed**。
守卫变异自证：临时放一个 `setup_probe_context().await.ok()?` 的探针文件 ⇒ **FAIL/exit 1**，
删除后 ⇒ OK/exit 0。

### 4.2 测试替身未做特性隔离（MAINT-01）

`synapse-storage/src/test_mocks/` 有 33 个 in-memory 替身，`member.rs:129`、`presence.rs:31`、
`room.rs:171` 仍是 `unimplemented!("… has no database pool")`；`admin_media.rs:95`、
`room.rs:120` 标注"stub returns all media/rooms"。

⇒ unit 层用这些替身时，**语义与真 PG 不同**（如 `stub returns all rooms` 忽略过滤）。
审计已登记为 MAINT-01（收益低于改动面），但**没有门禁**禁止新 mock 继续简化。

#### 4.2.1 实测：106 处「忽略参数却假装成功」（2026-10-26，已加守卫）

原计划是"每个 mock 方法的 doc 注释必须声明与生产的差异" —— 动手时发现这个判据**不可执行**
（"差异"无法机械判定，最后会退化成"有没有写注释"）。换成可机械检测的形态后，规模比预想大：

**429 个 mock 方法里，106 个（24.7%）至少忽略一个参数、且不 fail-loud。** 两类必须分开看：

| 形态 | 数量 | 安全性 |
|---|---|---|
| `unimplemented!("… has no database pool")` | 3 | **安全**：调用即 panic ⇒ 测试红（fail-loud） |
| 参数 `_xxx` + 函数体 `Ok(...)` | **106** | **危险**：调用方拿到"成功"，依赖该参数的断言**恒真**（fail-silent） |

#### ⚠️ 「高危」必须逐个核调用链，不能只看静态规则（自我纠正，2026-10-26）

上一版本在这里列了 6 处"最危险"，**说重了**。静态规则只能找"忽略参数"，判不了两件事：
① 该方法**有没有生产调用者**；② 有没有测试**断言它的语义**。前两条候选核实后**都是死代码**：

| 位置 | 在 trait impl 内？ | 有生产调用者？ | 判定 |
|---|---|---|---|
| `room.rs:119 get_user_rooms(_user_id)` | 否（inherent method） | `RoomStoreApi` **根本没声明**它（只有 `get_user_rooms_paginated`）；`search_service` 走的是真实 `RoomStorage` | **死代码** |
| `presence.rs:131 set_typing(_room_id, _user_id, _typing)` | 是（`PresenceStoreApi`） | 未找到 —— `typing_service` 自己维护内存状态，不调 store | **疑似死代码** |

其余四处已逐处核实（判据：生产调用者 + 容器是 `Arc<dyn Trait>` 还是具体类型 —— 后者说明单测
根本换不上 mock）：

| 方法 | 生产调用者 | 容器类型 | 判定 |
|---|---|---|---|
| `refresh_token.rs:189 mark_family_compromised` | `refresh_token_service.rs:168,244` | `Arc<dyn RefreshTokenStoreApi>` | **可达**（单测可换 mock） |
| `quarantine_media.rs:111 get_media_quarantine_status` | `media/mod.rs:257` | `Option<Arc<dyn QuarantinedMediaChangeStoreApi>>` | **可达** |
| `room.rs:635 get_room_block_status` | `room/state/info.rs:137` | `Arc<dyn RoomStoreApi>` | **可达** |
| `threepid.rs:77 verify_threepid` | `identity/storage.rs:40,60` | `ThreepidStorage`（**具体类型**） | **mock 走不到** |

三个"可达"的又逐个读了断言，**全部排除假绿**：

| 方法 | 断言侧实况 | 判定 |
|---|---|---|
| `room.rs get_room_block_status` | 测试名就叫 `get_room_block_status_is_unmodelled_by_the_in_memory_store`，断言消息写「MOCK DEVIATION 记录：InMemoryRoomStore 的 block_room 是 no-op、get_room_block_status 恒为 None。若这里变成 Some，说明 mock 已被补充实现，请同步更新偏差说明与本测试名」 | **已声明偏差**，反而是个正面范式 |
| `media get_media_quarantine_status` | `quarantine_tests.rs:41` 装配的是 `QuarantinedMediaChangeStorage::new(&pool)`（**真实 storage**），mock 根本没用上 | **mock 走不到** |
| `refresh_token mark_family_compromised` | 全文件 `replay` 只出现在**注释**里（149–212），没有任何测试断言"重放 ⇒ 族被封禁" | **无测试覆盖**（见下） |

⇒ **这批"忽略参数"没有造成任何一处假绿。** 最初把它们列成"6 处高危、会让安全断言永远成立"
是**错的** —— 那只是静态候选，不是风险结论。

**但顺手发现一个真缺口**：`mark_family_compromised` 所在的分支是 **token 重放检测**
（日志 `Potential token replay attack detected`，S-9 / A+ W1 的 fail-closed 改造点），
它**没有任何测试**。要补这条，mock 必须先能记录"族被封禁"（现在 `mark_family_compromised` 返回
`Ok(())` 不留痕），否则测试写不出来 —— 这才是该处 mock 真正该改的地方。

结论：`test_mocks` 有 **106 处忽略参数**；已核实 6 处 → 2 死代码、2 mock 走不到、1 已声明偏差、
1 无覆盖（安全关键）。**"忽略参数"本身不是缺陷**，缺陷是"忽略了参数、还有测试断言依赖该参数的结果"
——后者必须逐测试看断言，静态规则做不到，剩下 100 处不值得批量盲改。

#### 4.2.2 剩余 102 处抽样核查（2026-10-26）：危险与否取决于「忽略的是哪个参数」

先量了结构：**102 处里 99 处在 trait impl 内**（生产代码可经 trait 调用），
所以"可达性"不能用来降级风险。

抽 6 处（跨 6 个文件）逐个看"忽略的是关键参数还是元数据"，得到的区分比"102"这个数字有用：

| 样本 | 忽略的参数 | 判定 |
|---|---|---|
| `access_token.rs add_to_blacklist(_user_id, _reason)` | 仅**元数据**（`token_hash`/`expires_at` 仍生效） | **良性** —— 核心语义保留了 |
| `admin_media.rs get_room_media(_room_id)` | **唯一筛选键**，且返回全集 | **理论危险**，但其上层 `quarantine_room_media` / `unquarantine_room_media` **零调用者**（连 web 路由都没接）⇒ 无测试可假绿 |
| `background_update.rs acquire_lock_with_retry(...)` | 锁持有者/时长/重试次数 | **良性** —— 核心语义（同一 job 只能被一个持有者拿到）**mock 建模了**：`locks: HashMap<job, bool>`，第二次 `Ok(false)`；且 `start_update_fails_when_locked`（`background_update_service.rs:518`）正是靠它断言"Failed to acquire lock"。忽略的是 `_locked_by`/`_lock_duration_ms`/`_max_retries`/`_max_retry_interval_ms` ⇒ **不能测**"同持有者续期"与"重试 N 次后放弃"，要补那类测试时 mock 必须先建模 `locked_by` |
| `cas.rs validate_proxy_ticket(_service_url)` | service_url | **无断言依赖（当前无假绿）** —— mock 保留 "`is_valid` ⇒ 返回并置 `consumed_ts`"；`cas_service.rs` 里 `service_url` 只出现在生产分支，**没有任何测试断言"错 service 必须拒绝"**（`routes/cas.rs` 的 7 条测试全是 DTO 反序列化与 redirect 校验）⇒ 现在不会假绿，但**要补该测试，mock 必须先比 `service_url`** |
| `dehydrated_device.rs claim_to_device_events(全部)` | 全部 | **走不到（无测试依赖）** —— mock 返回"空页 + null 游标"（MSC3814 语义）并在注释里写明 `to_device_messages` 未建模；`dehydrated_device_service::claim_events` 在 `synapse-services` 里**一个测试都没调**（该文件 8 个 `#[tokio::test]` 全在脱水/设备 CRUD 上）⇒ 忽略参数不掩盖任何现有断言 |
| `device_list.rs get_device_list_changed_users(全部)` | 全部 | **已声明偏差**（同类里唯一写清楚理由的）—— 注释明说"与 storage 行为不等价"，因为 mock 只有**全局** `stream_id`、没有逐设备流位置 ⇒ 要支持窗口语义得改数据模型（记录写时的流位置），是独立任务 |

⇒ **6 个抽样全部定级完毕**（2026-10-27 补齐后 4 个）：**2 良性**（元数据参数）/
**1 理论危险但整条链未接线**（`get_room_media`）/ **1 无断言依赖**（`validate_proxy_ticket`）/
**1 走不到**（`claim_to_device_events`）/ **1 已声明偏差**（`get_device_list_changed_users`）。
**没有一个样本是"忽略了唯一筛选键、且有测试断言依赖该参数"** —— 也就是说抽样里没抓到"测试被蒙蔽"的实例，
抓到的全是"功能没做完"（`quarantine_room_media` 零调用者）或"能力未建模"（逐设备流位置）。

**两个可复用的判据**：

1. **忽略强 / 弱参数**：`_reason` / `_user_id` 这类元数据被忽略通常无害（核心键仍参与）；
   忽略**唯一的筛选键**（`_room_id`、`_media_id`）才会让"按 X 过滤"的断言恒真。
2. **可达 ≠ 有调用者**：`get_room_media` 在 trait impl 内、production 也有一处调用，
   但那个调用者（`quarantine_room_media`）本身没人调 ⇒ 整条链是**未接线的功能**。
   抽样里已经撞到一个 ⇒ 说明这批"忽略参数"里有相当比例反映的是**功能没做完**，
   而不是"测试被蒙蔽"。

⇒ **结论**：不值得逐处核 102 个。要么按"忽略唯一筛选键 + 有真实调用链 + 有依赖该参数的断言"
三条同时成立去筛（预计剩个位数），要么就只保留棘轮防新增。批量盲改是纯成本。

**为什么不一次性清零**：106 处里多数是良性的（`cleanup_expired_tokens(_now_ts)`：mock 没有"过期"
概念，返回 `Ok` 不产生副作用，调用方不会因此得出错误结论）。逐个裁定"实现 or 改 fail-loud"
是独立一笔；守卫先把**增量**钉死。

**守卫**：`scripts/ci/check_mock_stub_drift.py`（已接 preflight），棘轮基线上限 106、只许下降。
三种合法处置：① 真按参数过滤；② 改 `unimplemented!`（fail-loud）；③ 确属良性则加
`// mock-ignores-param: <理由>`（比全局白名单更贴近代码，review 时一眼可见）。

两个实现坑（都踩了）：
- 失败时若全量打印 106 条，实战里找不到自己刚加的那条 ⇒ 改为**优先只列本次改动文件里的命中**，
  完整清单走 `--print`。
- 第一版统计用行内正则（要求签名同一行含 `)`），只数出 **37** 处；处理跨行签名后才是 **106**。
  **统计"忽略参数"必须处理跨行签名**，差 2.9 倍。

### 4.3 回归范围按文件选，不按契约选

`NEW-17`（`2377043e5`）把滑动同步的 `rooms` 分桶后，`sync_path_equivalence_tests` 的 3 条
ARCH-02 等价用例**一直红着**，当次只跑了 `api_sliding_sync_routes_tests` + service 单测。
已在 NEW-11 那笔修复（`36242da93`），但**根因没修**：提交时选测试范围的判据是"改了哪个文件"，
不是"影响哪些契约"。

#### 4.3.1 已落地（2026-10-25）

新增 `scripts/ci/test_impact_map`（映射表）+ `scripts/ci/check_test_impact.py`（渲染成可粘贴命令），
接入 preflight 作**提示步骤**（新增 `advise()` 函数，恒 exit 0，与 `run()` 区分）。

设计要点（都是踩过坑才知道的）：

- **默认只看暂存区**，暂存为空才回退"HEAD 差异 + 未跟踪"。本仓常有多会话共用工作区，
  看 `git diff HEAD` 会把别人的在飞改动也报进来 —— 提示一旦变成噪声就会被无视（实测第一次
  试跑就报了同事改的 `tags.rs` 与 `migrations/*`）。需要看全树时用 `--all`。
- **三种动作**：`nextest:<E 表达式>`（拼成完整 `cargo nextest` 命令）、`shell:<命令>`（契约链这种）、
  `note:<文本>`（没有自动化判据的事，如"`ROUTE_CONTRACT.md` 头部日期那 1 行要提交掉")。
- **不做硬门禁**：全量 integration 42 分钟，塞进 pre-commit 的唯一后果是所有人学会 `--no-verify`。
- **表里优先写真踩过坑的行**：`rooms` 结构 → 等价用例（NEW-17）、`power_levels` → v12 创建者
  （C-7）、`voice.rs` → 体积上限（CVE-2026-102592）、`.sqlx`/`migrations` → 生成脚本与文档块。
  "改了 A 就跑 A 的测试"这种本来就没人会忘，不写。
- 表自身也有自检行：改 `test_impact_map` ⇒ 提示用已知命中路径跑一次
  （格式错会让脚本直接 exit 1，比默默不命中好）。

判据：`--files synapse-services/src/sliding_sync_service/timeline.rs` ⇒ 命中并渲染出
`-E 'test(sync_path_equivalence_tests)'`；`--files docs/README.md` ⇒ "未命中"；暂存 1 个文件时
只对比 1 个（不报别人的）；`check_gate_reachability` 56 个 check_* 全可达。

## 5 已排除（看着像，但查证不成立）

| 疑点 | 查证结果 |
|---|---|
| `#[ignore]` 导致用例永久静默 | **16 处全部有 reason 且有覆盖车道**（e2e 7 → `e2e-http.yml`；perf 4 → `benchmark.yml`；bench 2 → `check_pagination_benchmark.py`；generator 3 → 手工入口）。`analyze_skipped_tests.py` 报 `coverage_gap=0` |
| 形状测试冒充真覆盖 | `check_shape_only_tests.py`：29 个只锁形状，**26 有真覆盖文件、3 已登记 NONE**（`admin_extra`/`coverage`/`formatting` 全仓无对应生产模块，属幽灵族）；"名字声称覆盖但不碰生产代码"= **0** |
| 生产代码在测试时走另一条路 | `grep -c "cfg!(test)"` 在 `synapse-*/src` = **0** |
| 有意不实现的端点会假装成功 | `check_unsupported_endpoints.py` 双向门禁：代码恒 `Err` ↔ 登记 `unsupported=true`，漏登红、陈旧登记也红 |
| 棘轮基线被偷偷上调放宽 | `sqlx_dynamic_ratio_baseline` 改过 143 次，但文件头部逐条归因（如"106 → 114 的 8 处一行都不是本批写的"并列出每一处的文件与代码）。属**诚实上调** |
| 契约链守卫静默跳过 | `api_route_ledger_tests.rs` 有 **17 处** `skip_or_fail_without_db()`（历史是 16 处静默 `return`，已修） |
| 集成测试全量被压缩 | CI 的 `Run integration tests (--test integration)` 是**全量** `--all-features --no-fail-fast --test-threads 4`（1424 条 / ~42 min）。本地只跑子集是**成本现实**，不是 CI 降标 |

## 6 补齐顺序（按"能不能立刻写出会失败的验证"排）

| 序 | 项 | 成本 | 会失败的验证 |
|---|---|---|---|
| ~~1~~ | ~~`with_config` 改 fail-closed + 9 处 `else` 换 `skip_or_fail_without_db`~~ **✅ 已完成（+28/−1）** | ~30 行 | 假绿实验 **已从 PASS 变 FAIL**（§2.4） |
| ~~2~~ | ~~9 条安全用例的变异自证~~ **✅ 9/9 完成**：6 种机制各留红-绿记录，并顺手订正一处**写错了的测试注释**（`deny_unknown_fields` 对已声明字段无效） | 中 | 每条都实跑：红（含 `left/right` 实值）+ 还原后 6 passed |
| ~~3~~ | ~~572 处调用点收口~~ → **✅ 改做「守夹具本体」**：7 处 fail-open 夹具改 panic + 新增 `check_test_setup_fail_closed.py` 守卫（接入 preflight） | 中 | 探针文件 ⇒ 守卫 exit 1；真实 4+1 族 53 passed（详见 §4.1.1） |
| ~~4~~ | ~~`--new-file-floor 30 → 50`~~ **✅ 已完成** | 1 行 + 3 处文档同步 | 已验：40% 假 lcov ⇒ exit 1；50% ⇒ exit 0 |
| ~~5~~ | ~~"改动面 → 必跑套件"映射 + preflight 提示~~ **✅ 已完成**（`test_impact_map` + `check_test_impact.py` + preflight `advise` 步骤） | ~1 h | 改 `rooms` 结构 ⇒ 提交前自动打印 `-E 'test(sync_path_equivalence_tests)'`（详见 §4.3.1） |
| ~~6~~ | ~~性能断言改相对阈值~~ **✅ 已完成**：改成「每行成本折算数据库往返次数」（比 p95 比值更抗噪），注入 N+1 ⇒ 0.985/1.026 红；正常 0.061/0.031 绿（§3.2.1） | 中 | **已实测**：变异体 FAIL / 还原后 PASS，CI 同款命令 2.6s |
| ~~7~~ | 拆成三件：①重测基线 → **改判交给 CI**（本地 1.5h 且撞 PG 锁配额）；②`tasks/mod.rs` 配置层 **✅ 3 条**；③`start_all` 可观测性 **✅ 已改**（返回 `Vec<JoinHandle>`）＋关闭测试 **✅**；④巡检业务逻辑 → **❌ 改判不必补**：`perform_maintenance` 的行为在 `synapse-storage/src/maintenance.rs` 已有 **15 处**测试，tasks 层未覆盖的只是"定时器到点调一次"（§3.1.2） | 中 | 3 条配置 + 1 条关闭用例；变异（删 cancelled 分支）⇒ 5s 超时红且指向"第 0 个后台任务" |
| ~~8~~ | ~~`test_mocks/` 差异声明守卫~~ → **✅ 已改判并落地**：不守「doc 声明差异」（不可机械判定），改守「忽略参数却假装成功」——实测 **106/429 = 24.7%**，棘轮只许下降（`check_mock_stub_drift.py`，接入 preflight）；当前上限 **102**（删死代码 −1、mock 有状态化 −3）。**候选 6 处已全部定级：0 处假绿**（§4.2.1/§4.2.2） | 中 | 探针文件 ⇒ 超上限红；删除后绿 |
| ~~9~~ | ~~refresh token 重放「撤销全族」~~ **✅ 已完成**：`get_family_id` + 抽 `load_token_inner(token, reject_revoked)` + 5 s 并发窗口（第 1、2 层都加）+ mock 有状态化（`families`/`rotations`/`age_rotations`），提交 `3f9145a43` / `92afc93af` / `f35d7dd1f`。**状态从"无测试"变为"两条用例各钉住一侧"**：`replay_within_window_is_refused_but_family_survives` / `replay_outside_window_revokes_the_whole_family`（§2.5） | 中（含 `.sqlx` 重生成） | **双向变异已实跑**：窗口改 `0` ⇒ 「窗口内」那条红；改 `i64::MAX` ⇒ 「窗口外」那条红；还原 `5_000` ⇒ 双绿 |
| ~~10~~ | ~~全仓 `with_config` 族 helper 内的 `return None`~~ **✅ 已达成**（§2.3 P2） | — | 守卫扫 74 个夹具报 OK |

## 7 一页索引：这轮到底改了什么（39 笔提交，按用途分组）

计数口径（可复现，含本报告自身的提交）：`git log 0666941d6..HEAD --oneline --no-merges -- docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md scripts/ci/check_test_setup_fail_closed.py scripts/ci/check_mock_stub_drift.py scripts/ci/mock_stub_drift_baseline scripts/ci/test_impact_map scripts/ci/check_test_impact.py scripts/check_file_coverage.py scripts/ci/run_coverage.sh tests/performance/appservice_scheduler_perf_tests.rs src/tasks/mod.rs synapse-services/src/refresh_token_service.rs synapse-storage/src/refresh_token/mod.rs synapse-storage/src/test_mocks synapse-common/src/test_isolation.rs scripts/ci/non_unit_coverable_prefixes.txt .github/workflows/ci.yml docs/audit/INDEX.md | wc -l`

前六节回答"问题是什么"；本节只回答"**现在仓库里多了/少了什么**"，便于复核与接手。

计数口径（**机械可复现，别再手写**）：`0666941d6..HEAD` 内触及"本报告 + 本轮新增/修改的门禁与夹具"的
非 merge 提交（路径清单与命令见 §7.4）。旧的 27/26 是手写累加的，每次都漏。这个数字只在本节有改动时同步。

### 7.1 新增的门禁（都会真失败）

| 门禁 | 脚本 / 用例 | 位置 | 钉住什么 |
|---|---|---|---|
| 夹具不得 fail-open | `scripts/ci/check_test_setup_fail_closed.py` | preflight | 74 个夹具型函数的缺库路径必须 panic；白名单 1 项 |
| mock 不得新增"忽略参数假装成功" | `scripts/ci/check_mock_stub_drift.py` + `mock_stub_drift_baseline`（102） | preflight | 棘轮只许下降；失败时只列本次改动文件里的命中 |
| 改动面 → 必跑套件 | `scripts/ci/test_impact_map` + `check_test_impact.py` | preflight（`advise`，只提示不判定） | 改 `rooms` 结构 ⇒ 提示跑等价用例；默认只看暂存区 |
| 性能不退化（**第一条能进 CI 的性能门禁**） | `tests/performance/appservice_scheduler_perf_tests.rs::statistics_per_service_cost_does_not_grow_with_service_count` | `ci.yml` Test & Lint | 每行成本折算"几次 DB 往返" `< 0.5`（正常 0.031–0.061，注入 N+1 ⇒ 1.03） |
| 新文件覆盖率 ramp-up | `check_file_coverage.py --new-file-floor` 30 → **50** | `run_coverage.sh` | 假 lcov 40% ⇒ exit 1；50% ⇒ exit 0 |

### 7.2 修掉的缺陷

| 缺陷 | 提交 | 后果（修前） |
|---|---|---|
| `setup_fresh_test_app_with_config` fail-open | `fb9b68847` | 9 条安全边界用例**静默假绿**，且比真跑更慢（30 s vs 6 s） |
| 7 处夹具 `else { return; }` | `82f417fad` | 同一形态的复活路径 |
| 覆盖率基线被一次不完整 lcov 清零 | `4134ab1bc` | 608 条 → **1 条**，棘轮报废且**零警告** |
| refresh 重放在真实路径不可达 | `3f9145a43` + `f35d7dd1f` | "检测到重用即撤销全族"**一次都没生效过**；每次 refresh 还新建 token 族 |
| `InMemoryRoomStore::get_user_rooms` 死代码 | `4e07c5052` | 无调用者（trait 未声明），只在守卫计数里制造噪声 |
| `ConfigManager` 整类从未接线（81 行） | 本轮 | 全仓 **0 个构造点**；`read_config/write_config` 的路径参数被忽略、无任何文件 IO。**危险点不是"白占 0%"，而是它长得像热重载入口**：谁接上它都会以为"改配置即生效"，而服务器读的仍是启动时那份 `Config` |
| `src/common/error.rs` 两个转换 helper（零调用点） | 本轮 | 让人以为根 crate 有一层"统一错误转换"约定；实际无人使用 |
| `build_template` 的标记行刷新非原子（零行窗口） | 本轮 | 模板就绪时 `DELETE` 与 `INSERT` 各自独立提交，INSERT 失败即留下**零行**标记表；而 pruner 把零行当"遗留物"豁免、`prune_backfills…` 又断言"至少一行" |

### 7.3 可测性改进（生产 API 改动，行为不变）

| 改动 | 提交 | 为什么必须改生产代码 |
|---|---|---|
| `ScheduledTasks::start_all` → `Vec<JoinHandle<()>>` | `7b149393b` | 原来丢弃句柄 ⇒ "关闭后是否真退出"**不可观测**；`CancellationToken` 只能发信号、无法等待 |
| `RefreshTokenService::validate_token` 抽出 `load_token_inner(token, reject_revoked)` | `f35d7dd1f` | refresh 必须"看到已撤销"才能判重放；公开 API 语义与 7 条既有测试**一字未动** |
| mock 有状态化（`create_family` / `record_rotation` / `rotate_token_atomic` / `get_rotations` / `age_rotations`） | `3f9145a43` / `f35d7dd1f` | 原 mock 全空转 ⇒ 重放检测连素材都构造不出来 |
| 新增 `event/trait_object_dispatch_tests.rs`（纯测试文件，不改生产代码） | 本轮 | 它存在的理由就是生产侧只用 `Arc<dyn EventReader>` 调用：本 crate 的 `--lib` 腿此前**一次都走不到那层转发** |

### 7.4 复核命令（复现本报告的每个关键数字）

```bash
# 门禁三件
python3 scripts/ci/check_test_setup_fail_closed.py
python3 scripts/ci/check_mock_stub_drift.py
python3 scripts/ci/check_test_impact.py --all

# 性能门禁（需本地测试库；CI 同款命令）
cargo nextest run --test performance_manual --features "performance-tests,test-utils" \
  --locked --test-threads 1 -E 'test(statistics_per_service_cost)'

# 重放窗口双向语义
cargo test -p synapse-services --lib --features test-utils replay_

# §7.5 ⑤⑥ 的 6 组新用例 + 它们真把 0% 顶起来了吗（需本地测试库）
cargo test -p synapse-storage --lib -- --test-threads=1 trait_object
cargo test -p synapse-storage --lib -- --test-threads=1 since_filter
cargo test -p synapse-services --lib --all-features -- --test-threads=1 "error::tests"
# 定向插桩读数（只看这两个文件的 LH/LF；正常应 reader 9/95、writer 5/37）
cargo llvm-cov -p synapse-storage --lib --features "test-utils,friends,widgets,burn-after-read" \
  --lcov --output-path /tmp/storage_dyn.info -- --test-threads=1 trait_object
grep -A2 'event/reader.rs$' /tmp/storage_dyn.info | grep -E 'LH|LF'

# 死代码核实（两处删除的判据；正常输出为空）
grep -rn "ConfigManager::new" --include=*.rs . | grep -v RateLimitConfigManager
grep -rn "crypto_error_to_api_error\|ed25519_error_to_api_error" --include=*.rs .

# 覆盖率基线是否又会自毁（108 行保护）
cp scripts/ci/coverage_baseline.json /tmp/bak.json
printf 'SF:synapse-common/src/probe.rs\nLF:10\nLH:5\nend_of_record\n' > /tmp/p.info
python3 scripts/check_file_coverage.py --report /tmp/p.info --baseline scripts/ci/coverage_baseline.json \
  --global-floor 40 --new-file-floor 50 --core-files scripts/ci/core_file_coverage_prefixes.txt \
  --core-threshold 70 --non-unit-coverable scripts/ci/non_unit_coverable_prefixes.txt
python3 -c "import json;print(len(json.load(open('scripts/ci/coverage_baseline.json'))['files']))"  # 应为 609
git checkout -- scripts/ci/coverage_baseline.json
```

§7 的提交计数（口径写死在这里，避免再手写漂移；**只统计下列路径**，纯功能提交若不在清单内不会计入
—— 每次动本节都重跑这条，不要靠"上一版 + 1"）：

```bash
git log 0666941d6..HEAD --no-merges --format='%h %s' -- \
  docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md \
  scripts/ci/check_test_setup_fail_closed.py scripts/ci/check_mock_stub_drift.py \
  scripts/ci/mock_stub_drift_baseline scripts/ci/test_impact_map scripts/ci/check_test_impact.py \
  scripts/check_file_coverage.py scripts/ci/run_coverage.sh \
  scripts/ci/non_unit_coverable_prefixes.txt .github/workflows/ci.yml \
  tests/performance/appservice_scheduler_perf_tests.rs src/tasks/mod.rs \
  synapse-services/src/refresh_token_service.rs synapse-storage/src/refresh_token/mod.rs \
  synapse-storage/src/test_mocks synapse-common/src/test_isolation.rs | wc -l
```

### 7.5 0% 文件的分流：**分母是 32，不是 59**（2026-10-27，累计五次订正；8 个已消除 + 10 个陈旧已全部实测）

先说结论：**这条审计里最基础的一个数字是错的** —— "59 个 0% 文件"不是棘轮要管的文件数，而是
`coverage_baseline.json` 里所有 `line_pct == 0.0` 的条目数。**其中 27 个门禁自己就跳过**：

| 哪 27 个 | 门禁为什么不看 | 出处 |
|---|---|---|
| **22** 个 `synapse-storage/test_mocks/*` | `TEST_ONLY_MARKERS` 命中 `"test_mocks/"` ⇒ 检查循环直接 `continue` | `scripts/check_file_coverage.py:56-69`、`:378` |
| **5** 个 `src/bin/*` + `src/main.rs` | `non_unit_coverable_prefixes.txt` 命中 `src/bin/` 与 `src/main.rs` ⇒ 只打 `[EXEMPT]`、不设地板 | 同脚本 `:396-406` |

判据不是我的推断，是**调用门禁自己的函数**跑出来的：

```bash
python3 - <<'PY'
import json, pathlib, importlib.util
spec = importlib.util.spec_from_file_location("cfc", "scripts/check_file_coverage.py")
m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
exempt = m.load_prefix_list(pathlib.Path('scripts/ci/non_unit_coverable_prefixes.txt'))
base = json.load(open('scripts/ci/coverage_baseline.json'))['files']
zero = [f['path'] for f in base if f['line_pct'] == 0.0]
print(len(zero),                                        # 59
      sum(m.is_test_only(p) for p in zero),             # 22
      sum(m._matches_prefix(p, exempt) for p in zero))  # 5
PY
```

⇒ **真正落在棘轮里的 0% 是 32 个**。前两轮把 59 当分母、再花力气去"分流"其中 27 个，
是**用与门禁不同的口径建清单**，然后给一堆门禁不看的文件写判定 —— 这本身是浪费，
也是本轮最该记下的一条教训（§7.6 第 6 条）。

| 类别 | 数量 | 判定 | 证据强度 |
|---|---|---|---|
| 门禁已排除：`test_mocks/` | 22 | 不在检查范围（`TEST_ONLY_MARKERS`），**不是缺口也不是陈旧** | 门禁源码 |
| 门禁已豁免：二进制入口 | 5 | 不在检查范围（`non_unit_coverable_prefixes.txt`） | 门禁源码 |
| ① 已实测：陈旧（floor 冻结） | **10** | 原有 2：`router.rs`（82.9%）、`room/membership/federation.rs`；**本轮新增 8**：`synapse-web/routes/**` 全部实测非零 15.2%–100%（§7.5 G） | 实测 + bootstrap 源码快照 |
| ② 已实测：真 0 | **4** | `services.rs`、`telemetry.rs`、`tasks/mod.rs`、`common/logging.rs` | 实测（含定向插桩） |
| ③ `feature` 关 ⇒ 当时不进报告 | **3** | `burn_after_read.rs`、`widget.rs`、`friend_room/repository.rs`。**2026-10-27 已把三个 feature 加进 `STORAGE_FEATURES`**（71 条用例实跑全绿）⇒ 下次 coverage 即产出读数；但基线里仍是 `0.0`（本地全量 storage 腿 53 min 未跑完，不手写编数） | 编译实验 + 27/19/25 条 DB 测试实跑 |
| ④ 无实质可测行（纯 trait 声明） | **5** | `presence` / `cas` / `room_summary` / `server_notification` / `worker` 的 `api.rs`：只有 `fn …;` 声明，没有可执行行 | 机械统计 |
| ④′ 跨 crate 转发层（**原并入 ④，已证伪"死默认实现"**） | **2** | `room/api.rs`（50 个纯 `.await` 转发体）、`sliding_sync/api.rs`（23 个）—— 生产装配把它们装进 `Arc<dyn …>` 后调用，**删掉即砍断生产**；0% 是"本 crate 的测试从不经过 trait 对象"的记账盲区。已补 `&dyn` 契约测试并实测（见 E） | 装配点 grep + 插桩实测 |
| ⑤ 结构盲区 → **已补测试并实测** | ~~2~~ | `event/reader.rs` 0 → **9/95 = 9.5%**、`event/writer.rs` 0 → **5/37 = 13.5%**（`&dyn` 转发层契约测试，见下） | 定向插桩实测 |
| ⑥ 真缺口 → **已全部处置** | ~~5~~ | 删 2（`config/manager.rs` 整类零构造点、`common/error.rs` 2 个零调用点 helper）＋ 补 3（`{friend_room_service,room/state,rtc}/error.rs` 共 8 条断言） | 全仓 `grep` + 11 条测试实跑 |
| ⑦ ~~待 CI 的 integration 腿~~ → **并入 ①** | **0** | 原 8 个 web 路由：**不需要 integration 腿**，`-p synapse-web --lib` 6m02s 出数；而且 CI 那条路**根本不会来**（`skipped`，见 G） | 实测 + `gh run view` + 提交史 |
| ⑧ 本轮已消除（原 ④⑤⑥ 中的 8 个） | **8** | 4 类来源：2 个 trait impl（补测）+ `event/models.rs`（补测）+ 3 个领域错误转换（补测）+ 1 个死类型 + 1 个死 helper（删除） | 插桩实测 / 编译验证 |

合计：**10 + 4 + 3 + 5 + 2 + 8（已消除）= 32** ✓

> 也就是说棘轮里**没有一个 0% 还是"拿不到读数"的了**：10 个是**基线冻结的陈旧值**（全部已实测非零），
> 4 个真 0（启动路径/一次性初始化，补测试等于造假），3 个 `feature` 关（同一份基线里永远不编译），
> 5 个没有可执行行（纯 trait 声明），**2 个是跨 crate 转发层**（已补 `&dyn` 契约测试，见 E），
> 8 个本轮已删/已补。
> **"等 CI 跑一次就有答案"这条路径整体作废** —— 覆盖率车道最近 30 次运行里全是 `skipped`
> （`needs: [integration-test]`，而 `test` 恒红），`auto-ratchet` 自动提交史 0 条（§7.5 G）。

#### A. 5 个二进制入口：门禁**本来**就豁免它们（不是我分流出来的）

`src/main.rs`、`src/bin/federation_sign_request.rs`、`src/bin/generate_test_keypair.rs`、
`src/bin/run_migrations.rs`、`src/bin/schema_health_check.rs` —— 全部命中
`scripts/ci/non_unit_coverable_prefixes.txt` 的 `src/bin/` 与 `src/main.rs`，门禁只打 `[EXEMPT]`、不设地板。

根 crate 的 `src/lib.rs` 只声明 `pub mod {cli, common, server, tasks, worker}`（+ 私有 `test_exit_hook`）
—— `main.rs` 与 `src/bin/*` 是**独立 bin 目标**，`--lib` 与 `--test` 腿都不会链接它们。
把它们记成 0% 并在 CI 里当"覆盖率缺口"追，是**口径错误**：它们只能靠实际运行二进制来覆盖。
（真要覆盖，路径是给 `run_migrations` 这类加集成冒烟测试去 `Command::new(env!("CARGO_BIN_EXE_…"))`，
不是补单测。）

#### B. 22 个 `test_mocks/*`：门禁自带的 `TEST_ONLY_MARKERS` 早就跳过它们（2026-10-26 三次订正）

**前两轮的判定都不该存在**：

| 轮次 | 当时的判定 | 错在哪 |
|---|---|---|
| 第 1 轮 | "基线陈旧（在**不含 `test-utils`** 的 feature 集下测得）" | 两腿**都**把 `test-utils` 放第 1 位（`56e4dd105` 起未改），门控 `#[cfg(any(test, feature = "test-utils"))]`（`synapse-storage/src/lib.rs:146`）⇒ 两腿都编译。**前提就假** |
| 第 2 轮 | "两条腿的报告口径，消费者命中不计入" | 机制成立，但**对门禁没有意义** —— 这些文件根本不在检查范围（下表） |

真依据只有一行：`scripts/check_file_coverage.py:56-69` 的 `TEST_ONLY_MARKERS` 含 `"test_mocks/"`，
`:378` 的 `if is_test_only(path): continue` 让它们**从不被检查、从不设地板、也从不报豁免**
（豁免分支在 :396，走不到）—— 所以它们既不制造红，也不产生任何提示，是**纯账面条目**。

##### 那条"报告口径"机制本身是对的，而且本轮做成了双向闭式验证（对非 `test_mocks` 文件仍然要用）

| 事实 | 证据 |
|---|---|
| `--exclude` = **同时**从测试与报告排除 | `cargo llvm-cov --help`：`Exclude packages from both the test and report`；另有 `--exclude-from-test`（只排除测试、**保留报告**）与 `--exclude-from-report` |
| storage 的读数只来自"自己单测"那条腿 | 第 1 腿 `-p synapse-storage --lib`；第 2 腿 `--workspace --exclude synapse-storage` |
| 同一份基线里，32 条 `test_mocks` 条目 = **10 条非零 + 22 条零** | 非零：`event` 48.75 / `member` 49.71 / `room` 35.28 / `room_summary` 40.20 / `sliding_sync` 58.33 / `room_tag` 83.33 / `audit_event` 92.25 / `rate_limit` 100 / `oidc_user_mapping` 100 / `mod.rs` 100 |
| 判据：storage 自带测试面（132 个含测试代码的文件，排除 `test_mocks/mod.rs` 的 re-export）是否引用该结构体 | **正向**：10/10 非零条目都有引用（1–17 次，`event` 17、`member` 13）；**反向**：22/22 零条目引用数**全为 0**。零例外 ⇒ 机制闭合 |

⚠️ 上一轮我把"32 个 `InMemory*` mock 全部有非 mock 调用者"当作 B 类的结构性证据 —— 那条**与覆盖率无关**：
调用者在被 `--exclude` 掉的那条腿里，调用多少次都不会产生记账。它能证明的只有"mock 不是死代码"。

##### 处置：门禁不用改，但要改的是**同类机制的文件**（⑤ `event/reader.rs` / `writer.rs`、⑦ web 路由）

这 22 个不用管。真正被这条机制卡住的是不含 `test_mocks/` 字样、却同样只能由**别的 crate 动态分发**
才执行的文件 —— 它们的 0% 同样永远不变。一条 flag 级处置（**未验证，留给 CI 判**；
**2026-10-27 结论：本轮不采纳，理由与重开条件见 §7.7.3**）：

```diff
- cargo llvm-cov --workspace --exclude synapse-storage \
+ cargo llvm-cov --workspace --exclude-from-test synapse-storage \
      --features "$REST_FEATURES" --lcov --output-path "$REST_LCOV"
```

`--exclude-from-test` 保留"不跑 storage 自己的 db_tests"（脚本头担心的并发 clone/OOM 正来自它），
同时让 storage 进报告 ⇒ `merge_lcov` 的 `max()` 只会把值抬高。风险点两个，都得实测：
① `REST_FEATURES` 与 `STORAGE_FEATURES` 的 `cfg` 差异会让部分文件出现/消失；
② 报告条目数会明显增加（storage 全文进第 2 腿）。

#### C. 已双腿实测（4 个，末行是 F 类交叉引用）

| 文件 | lib 腿 | integration 腿 | 判定 |
|---|---|---|---|
| `src/server/router.rs` | 0/35 | **29/35 = 82.9%** | 陈旧（§3.1.2） |
| `src/tasks/mod.rs` | 0/203 | 0/203 | 真 0（见 §3.1.2 的"不必补"判断） |
| `src/server/telemetry.rs` | 0/18 | 0/18 | 真 0（启动期一次性初始化） |
| `synapse-common/logging.rs` | 0/49 | 该腿不编译 | 真 0（同上） |
| `src/server/services.rs` | 0/37 | **0/37**（43 条 appservice 用例） | 真 0（启动路径；同报告 `server/mod.rs` 0/829 佐证测试不跑主循环） |

#### E. 7 个 `**/api.rs`：5 个"无行可测"，2 个是**跨 crate 转发层的记账盲区**

（**2026-10-27 改判**：原判定"后 2 个只剩 5 行、属死默认实现、删掉或标注"两条都错，见下。）

机械统计（去掉 `///` 注释后数 `Ok(`/`Err(`/`let`/`if`/`match`/`return`）：

| 文件 | 行数 | `fn …;` 声明 | 可执行行（机械口径） |
|---|---|---|---|
| `presence/api.rs` | 122 | 12 | **0** |
| `cas/api.rs` | 113 | 16 | **0** |
| `room_summary/api.rs` | 281 | 30 | **0** |
| `server_notification/api.rs` | 300 | 32 | **0** |
| `worker/api.rs` | 251 | 31 | **0** |
| `room/api.rs` | 624 | 50 | ~~5~~ → **LLVM 认 101 行** |
| `sliding_sync/api.rs` | 412 | 23 | ~~5~~ → **LLVM 认 96 行** |

⇒ 前 5 个是**纯 trait 声明**，0% 是必然，**不是缺口**（与 A 类同性质：指标口径问题）。

**后 2 个的三个错判（逐条证伪）**：

| 原判定 | 反证 |
|---|---|
| "只剩 5 行可执行" | LLVM 在 `room/api.rs` 里认出 **101** 行可执行、`sliding_sync/api.rs` **96** 行。机械口径只数 `Ok(`/`Err(`/`let`/`if`/`match`/`return`，而**纯转发体一个关键字都不含**（只有 `self.xxx(..).await`）⇒ 漏计约 95% |
| "默认实现从未被调用 ⇒ 死默认实现" | 它们不是"默认实现"，是 `impl RoomStoreApi for RoomStorage`（`api.rs:305`，50 个方法）与 `impl SlidingSyncStoreApi for SlidingSyncStorage`（`api.rs:197`，23 个）。生产装配把真实例装进 trait 对象：`wiring/rooms.rs:88`、`wiring/admin.rs:246`、`application_service/mod.rs:40`、`graph_metadata.rs:140`、`room/service.rs:93`… 全仓 `Arc<dyn RoomStoreApi>` 字段 **20+ 处**；`SlidingSyncStoreApi` 在 `sliding_sync_service/mod.rs:63/130` 同理。**删掉 impl，services 直接编译不过** |
| "处置=删掉或标注理由" | 0% 的机制与 `event/reader.rs` **完全一致**：本 crate 的 `db_tests.rs` 走**固有**方法（`storage.get_room(..)`），没有一处经过 trait 对象 ⇒ 记账盲区，不是死代码。处置是补 `&dyn` 契约测试（⑤ 的做法） |

**处置（2026-10-27 已实施）**：两个新文件，12 条 DB 用例，`cargo test … -- api_dyn_tests` **12 passed / 116 s**。

| 新文件 | 条数 | 断言要点 |
|---|---|---|
| `synapse-storage/src/room/api_dyn_tests.rs` | 8 | `pool()` 必须是同一 `Arc`（偷换池无编译期信号）；dyn 写 → **固有**读回；`create_room` 重复调用必须 `Err`；`set_canonical_alias` 与 `set_room_alias` **不串**（按别名反查得到本房间）；name/topic 不串列；tx 变体用**回滚**证明写进调用方事务、commit 后 `join_rule` 生效；`delete_room`/`get_room_count` 同表 |
| `synapse-storage/src/sliding_sync/api_dyn_tests.rs` | 4 | `event_stream_pos`（参数）与 `pos`（序列值）**不混淆**；upsert 同一行（id 不变、pos 推进）；`validate_pos` 精确校验（过期值必须 false）；list 生命周期；`delete_connection_data` 同时清 token + list |

实测（`cargo llvm-cov -p synapse-storage --lib … -- api_dyn_tests`，lcov 落 `/tmp/storage_dyn.lcov`）：

| 文件 | 前后 | line_pct |
|---|---|---|
| `room/api.rs` | 0 → **29 / 101** | **28.7%** |
| `sliding_sync/api.rs` | 0 → **14 / 96** | **14.6%** |

**写这两组测试时被自己的断言打回两次（记下省一次）**：
① 先写 `token.pos == event_stream_pos`，跑出来 `left: 1, right: 42` —— `pos` 是 `sliding_sync_pos_seq` 的序列值，
`event_stream_pos` 才是参数（`repository.rs:47` 的 `nextval`）。**先读实现再写断言**。
② 想用"另一条连接看不到未提交行"证明事务转发，得 `PoolTimedOut` —— 隔离池只有 1 条连接。
改成**回滚**证明：转发层若另取连接自行提交，回滚后行会残留。

#### F. 13 个杂项：逐个查了引用与调用链

（本表是最早的核查记录；其中 6 行已按最终口径改判并落到 ①③④⑤⑥，判定列都加了指针。）

| 文件 | 可执行行 | 证据 | 判定 |
|---|---|---|---|
| `friend_room/repository.rs` | ~147 | `friend_room/db_tests.rs` 有 **27 条** `#[tokio::test]`，**但整个模块是 `#[cfg(feature = "friends")]`**（`synapse-storage/src/lib.rs:172`），而两条覆盖率腿的 feature 集都不含 `friends` ⇒ **文件不编译、不进报告**；0.0 是 bootstrap 时（`db_tests` 2026-09-24 才加的，`ab5949c70`）写下的值被 `max()` 冻结 | **`feature` 关（不可测；原记"陈旧"是错的）** |
| `src/server/services.rs` | 37 | **定向插桩实测：43 条 appservice 集成测试下仍 0/37**。`build_service_container` 在 `src/server/mod.rs:203`，属 `Server::run` 路径；同一份报告里 `src/server/mod.rs` 也是 **0/829**（测试从不跑主循环），`router.rs` 才是 29/35 | **真 0（启动路径）** |
| `burn_after_read.rs` / `widget.rs` | 81/74 | 同上：`#[cfg(feature = "burn-after-read")]` / `#[cfg(feature = "widgets")]`（`lib.rs:215` / `:203`），`STORAGE_FEATURES` 与 `REST_FEATURES` 都不含 | **`feature` 关（不可测）** |
| `room/membership/federation.rs` | 11 fn（有两个内联 `#[cfg(test)]` 模块：`join_persistence_failure_tests` / `invite_room_state_tests`） | **无 feature 门控**（`synapse-services/src/lib.rs` 里 `membership` 是普通 `pub mod`）⇒ 进报告、测试也在跑；基线最后刷新 `090742266`（2026-10-09），而本文件 10-20/10-22 还在改 | **陈旧（4 个里唯一成立的那个）** |
| `event/reader.rs` | 130 个方法 | impl **不能删**（摘掉即 `E0277`，6+ 处下游依赖）；且确实**被当 trait object 用**（全仓 107 处，生产 wiring 把真实 `EventStorage` 转成 `Arc<dyn EventReader>`：`wiring/rooms.rs:89`、`wiring/admin.rs:242`） | **结构盲区**（原判"不可判"已收窄，见 ⑤） |
| `event/writer.rs` | 同上 | 同上（`Writer` 亦被 `Arc<dyn …>` 使用） | 同上（见 ⑤） |
| `event/models.rs` | ~2 | 基本是 `struct`/`enum` 声明（13 个 struct / 81 个字段 / 1 个 impl 2 个 fn） | 无实质可测行（见 ⑥） |
| `rtc/error.rs` / `room/state/error.rs` / `friend_room_service/error.rs` | 各 ~2 | 只有 `Display`/`IntoResponse` 之类，被本模块 2 个文件引用 | 真 0（小），见 ⑥ |
| `src/common/error.rs` | ~1 | 2 个转换 fn 只被 `common/mod.rs` re-export，无调用者 | 真 0（疑似死），见 ⑥ |
| `config/manager.rs` | ~7 | 7 行可执行；`get_database_url`/`get_redis_url`/`get_server_host`/`get_server_port`/`update_config` 全仓**无引用** | 真 0 + 死 getter，见 ⑥ |

**第三条结论（2026-10-26；"永远"这一半已于 2026-10-27 修掉）：这三个 `feature` 关的文件，在当时的
feature 集下等不到 CI。** `friends` / `widgets` / `burn-after-read` 只在
`Run library unit tests (--workspace --lib --all-features)` 那条车道里打开，而**那条车道不产生覆盖率**；
覆盖率两条腿的 feature 集自 `56e4dd105`（2026-09-22）起就没有它们。⇒ 那种状态下无论 main push 多少次，
读数都不会变，原判定把它们记成"陈旧"等于给了一条**永远兑现不了**的等待理由。
**（补强，2026-10-26 晚：整条覆盖率车道最近 30 次运行都是 `skipped`，不只是 feature 集的问题 —— 见 §7.5 G(3)。）**

**处置已落地（2026-10-27）**：这三个 feature 已加进 `STORAGE_FEATURES`（`scripts/ci/run_coverage.sh:44`）。
理由：storage 腿是 `RUST_TEST_THREADS=1`，脚本头说的"friends/widgets 在多核/有限内存下触发并发 clone
失败与 OOM"针对的是 4 线程的 rest 腿，未必适用于单线程的 storage 腿。

实测（2026-10-26/27，隔离 `CARGO_TARGET_DIR=/tmp/tgt-featcheck`，不污染主 target）：

| 步骤 | 结果 |
|---|---|
| `cargo test -p synapse-storage --lib --no-run` + 那三个 feature | **编过**（15m39s，冷 target；`friends`/`widgets`/`burn-after-read` 与既有 10 个 feature 无冲突） |
| 同 feature 集跑 `-- --test-threads=1 friend_room`（真库） | **27 passed / 0 failed**（577s；本机未设 `TEST_DB_TEMPLATE_SCHEMA` ⇒ 每测试跑迁移，属最坏情况） |
| 补跑另两个模块（2026-10-27，`widget` / `burn_after_read`） | **19 passed** / **25 passed**，均 0 failed |

⇒ "并发 clone 失败 / OOM"在这个 feature 集 + 单线程下**没有出现**，71 条新进编译的用例全绿。
**仍未闭合的一点**：这三个文件在 `coverage_baseline.json` 里仍是 `0.0` —— 它们的真实读数只能由
storage 腿的完整 lcov 给出，而本地跑一次全量 storage 腿**实测 53 min 仍未跑完**（1800+ 用例单线程），
所以本轮**没有**把它们手写进基线（手写等于编数）。下一次 `Code Coverage` 会首次产出这三条的读数，
auto-ratchet 随即把它们抬起来。

两条值得单独说的结论：

- **`event/reader.rs` + `event/writer.rs`：两轮静态推断都被推翻；本轮第三次收窄为"结构盲区"（见 ⑤）。**

  | 我的判断 | 依据 | 被什么推翻 |
  |---|---|---|
  | "死转发层，处置是删" | `grep "dyn EventReader"` = 0 | `#[cfg(any())]` 摘掉 impl ⇒ 下游 **6+ 处 E0277** |
  | "未被实例化的泛型路径" | 摘掉后 storage 内能编过、下游报错 | 真实代码是 **107 处 trait object**，生产 wiring 把真 `EventStorage` 转成 `Arc<dyn EventReader>`（`wiring/rooms.rs:89`、`admin.rs:242`、`search_service.rs:833/932`、`auth/mod.rs:171`、`wiring/admin.rs:242`） |

  **第一次错在 grep**：`grep "dyn EventReader"` 匹配不到 `dyn synapse_storage::event::EventReader`
  这种**路径限定**写法。当时看到的"0 处动态分发"是假象。
  **第二次错在把"编译不过"读成"没有实例化"**：E0277 只说明 bound 存在，不说明运行时没人调。

  ⇒ 现在的诚实结论：这两个 impl **被生产 wiring 以 trait object 使用**，但**在本次所有实测腿里
  都拿不到读数**（lib 腿的单元测试注入的是 `InMemoryEventStore as Arc<dyn EventReader>`；
  `--test integration` 模式又不插桩依赖 crate，见下）。
  ⚠️ **本行原写"只能由 workspace 腿回答"是错的**（2026-10-26 三次订正）：现有第 2 腿是
  `--workspace --exclude synapse-storage`，而 `--exclude` = **同时**排除测试与报告 ⇒ 就算跑 `--workspace`
  也不会出现 storage 的条目。能回答的只有 `--exclude-from-test`，或 storage 侧补一条 trait 对象测试（见 ⑤）。

#### 本轮学到的插桩口径（三个都会让"0%"骗人）

1. **`cargo llvm-cov -p <crate>`**：报告只含本 crate（`-p synapse-services` ⇒ 196 条，`test_mocks/` 一条没有）。
2. **`cargo llvm-cov --test <target>`**：**同样只插桩该 target 所属的包**。本轮 43 条 appservice 集成测试
   的报告只有 **19 条**、全是根 crate 的 `src/**`；`synapse-storage`/`synapse-services`/`synapse-web`
   的文件一条都没有 ⇒ 仓库里的 storage 文件**不可能**用 integration 子集回答。
3. **测试失败即无报告**：`cargo llvm-cov` 在测试失败时不产出 lcov，也没有 `--ignore-run-fail`。
4. **`--exclude` ≠ `--exclude-from-test`**（本轮新加，最容易被误用）：前者连**报告**一起排除，
   后者只排除测试、保留报告。想"不跑某 crate 的测试但仍给它记账"只有后者能做到。

⇒ 结论：**光有 `--workspace` 不够**，还得让目标 crate 留在报告里（`--exclude-from-test`）。
但它只对「测试全在**别的 crate**」这一类（C 类 storage 文件）成立 —— **不要**把它当成"局部回答不可能"的
通用理由：`-p <自己的 crate> --lib` 本来就能回答**本 crate 文件**的读数，G 类那 8 个 web 路由就是这么拿到的
（**上一版把这句写成"必须等 CI 的真正原因"，对 G 类是错的**，见 G 与 §7.6 末两条）。

#### ⑤ 2 个 trait impl 文件：与那 22 个 mock **同一机制**（从"不可判"改判为"结构盲区"，再补测试消掉）

`event/reader.rs`（`impl EventReader for EventStorage`）、`event/writer.rs`（`impl EventWriter`）。

本轮把机制坐实到源码级：**这些 trait 方法体是一行转发** ——

```rust
async fn get_event(&self, event_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
    self.get_event(event_id).await      // 转发到 basic.rs 的固有方法
}
```

而 `synapse-storage` 内部**没有任何 `dyn EventReader` / `dyn EventWriter` 调用点**（grep 0 命中）⇒
storage 自己的 `--lib` 测试永远走固有方法（`event/basic.rs:14`、`event/create.rs:8`，这两个文件因此非零），
**转发层永远不执行**；真正执行它的是别的 crate 里 `Arc<dyn EventReader>`（生产 wiring，107 处），
而那条腿 `--exclude` 掉了 storage。⇒ 不是"依赖 crate 未插桩"（原判），而是 **B 类报告口径的另一种表现**。

两条处置，成本都不高：

- 补一条 storage 侧 trait 对象测试（把 `EventStorage` 转成 `&dyn EventReader` 调 3–5 个方法）⇒
  直接把 `reader.rs`/`writer.rs` 顶到非零，同时守住这层转发的契约；这是**唯一能在现有腿口径内解决**的路。
- 或改 `--exclude-from-test`（见 B 末），让第 2 腿把 storage 记进报告。

**✅ 已选第一条并实测（2026-10-26）**：新增
`synapse-storage/src/event/trait_object_dispatch_tests.rs`（`#[cfg(test)] mod`，2 条用例），
真的用 `&dyn EventReader` / `&dyn EventWriter` 走一遍转发层：

| 断言 | 钉住什么 |
|---|---|
| `writer.create_event(...)` → `reader.get_event(...)` 与固有方法同一行、同一 content | 转发层不丢字段、不换语义 |
| `get_room_events` / `get_room_events_paginated(.., "b")` 都能看到刚写的一行 | 批量与分页的**方向参数真的传下去** |
| `Arc::ptr_eq(reader.pool(), &pool)`（两个 trait 各一次） | trait 对象没有偷偷换连接池 |
| 缺失 event / 未知 room ⇒ `Ok(None)` / `Ok(空)` 而非 `Err` | 消费者靠这个区分 404 与 500 |

定向插桩读数（`cargo llvm-cov -p synapse-storage --lib … --lcov -o /tmp/storage_dyn.info -- trait_object`）：

| 文件 | 修前 | 修后 |
|---|---|---|
| `event/reader.rs` | 0.0（`LH=0`） | **`LH=9 / LF=95`（9.5%）** |
| `event/writer.rs` | 0.0（`LH=0`） | **`LH=5 / LF=37`（13.5%）** |

⇒ 这两个不再是"结构盲区"，也不需要用 `--exclude-from-test` 动 CI 口径。剩下的 82–86% 是
**本 crate 内没有消费者**的其它 trait 方法（同样的转发行），要再往上抬只能靠"每个方法配一条用例"，
收益递减 —— 关键是**它们已经不是 0% 了，棘轮不会再把它俩当缺口扣分**。

#### ⑥ 5 个真缺口：全部处置完（2 删 / 3 补测）

原判定是"5 个真缺口，最小补法是加测试"。逐条核实后**只有 3 个该补测试，另外 2 个是死代码**：

| 文件 | 核实结果 | 处置 |
|---|---|---|
| `synapse-common/config/manager.rs` | ⚠️ **整类死代码**：全仓 `ConfigManager::new` **0 个构造点**（唯一命中全是 `RateLimitConfigManager`）；`read_config(_location)` / `write_config(_location)` 的**路径参数被忽略**，根本没有文件 IO —— 它不是"配置读写器"，是一个没人接线的 `Arc<RwLock<Config>>` 壳 | **删除**（81 行 + 3 处 re-export：`config/mod.rs` / `synapse-common/src/lib.rs` / `src/common/config/mod.rs`）。留着才是风险：它长得像"热重载入口"，真被人接上会以为改配置生效了，而服务器实际读的是启动时那份 `Config` |
| `src/common/error.rs` | 2 个转换 fn（`crypto_error_to_api_error`、`ed25519_error_to_api_error`）**零调用点**（`synapse-e2ee` 里那几个同名测试测的是它自己 crate 的函数） | **删除 2 个 fn + 其 re-export**；模块本身保留为 facade（`tests/integration/*` 有 `synapse_rust::common::error::MatrixErrorCode` 这种路径），但已无实质可测行 ⇒ 归入 ④ |
| `friend_room_service/error.rs` | 2 个 `From` impl，原来 0 条测试 | **补 3 条**：6 个变体逐一对 `(status, errcode)`、`Database` 保留 `cause`、反方向压成 `Internal` 且保留原文 |
| `room/state/error.rs` | 同上 | **补 3 条**（同上形态，5 个变体） |
| `rtc/error.rs` | 1 个 `From` impl，原来 0 条测试 | **补 2 条**（4 个变体 + `Database` cause） |
| `synapse-storage/event/models.rs` | 303 行里只有 `SinceFilter::column/value` 是活的 | **补 2 条**：列名必须是**裸小写标识符**（它会被 `format!` 拼进 SQL）＋ 两个变体必须映到**不同**列（映到同一列会让 `since` 过滤静默按错列裁剪） |

实跑：`cargo test -p synapse-services --lib --all-features -- error::tests` ⇒ **11 passed**
（我新补的 8 条 + 该 crate 原有的 3 条）；`… -p synapse-storage --lib -- since_filter` ⇒ **2 passed**。
删除后 `cargo check -p synapse-common --lib`（16 s）与 `cargo check -p synapse-rust --lib --all-features`（4 m18 s）**均干净**。

⚠️ 价值判断：这 5 个里真正有价值的只有 **"错误映射决定了客户端看到的 status/errcode"** 这一层
（错一处就是"该 404 的报 500"），其余是消噪音。但它们便宜，且断言对象明确（返回值语义），没有重蹈
"断言没人读"的覆辙。

#### G. 原本"只剩 8 个"：`synapse-web/routes/**` —— **已实测全部非零，判定收口为陈旧**

`admin/room/spaces`、`extractors/mod`、`federation/{media,membership/query,transaction/edus}`、
`handlers/room/management/{upgrade,visibility}`、`sticky_event`。

上一版结论是"路由只能由 integration 腿覆盖 ⇒ 真的只能等 CI 的 diff"。**两条都已证伪**。

**(1) 不需要 integration 腿**：这 8 个文件全部属于 `synapse-web` crate、且都在 `routes/mod.rs` 里
**无条件** `pub mod`（不在任何 `#[cfg(feature)]` 后面），并且**每个文件都自带内联 `#[cfg(test)] mod tests`**。
⇒ 它自己 crate 的 lib 腿就能回答，integration 腿只可能把它抬得更高。

实测（`cargo llvm-cov -p synapse-web --lib`，`REST_FEATURES` 同一 feature 集，6m02s，
**967 passed / 0 failed / 1 ignored**，lcov 落 `/tmp/web_lib.info`）：

| 文件 | `LH` / `LF` | line_pct | 基线记录 |
|---|---|---|---|
| `routes/extractors/mod.rs` | 66 / 66 | **100.0%** | 0.0 |
| `routes/federation/media.rs` | 115 / 162 | **71.0%** | 0.0 |
| `routes/sticky_event.rs` | 116 / 217 | **53.5%** | 0.0 |
| `routes/federation/membership/query.rs` | 104 / 236 | **44.1%** | 0.0 |
| `routes/handlers/room/management/visibility.rs` | 42 / 96 | **43.8%** | 0.0 |
| `routes/handlers/room/management/upgrade.rs` | 25 / 78 | **32.1%** | 0.0 |
| `routes/admin/room/spaces.rs` | 47 / 231 | **20.3%** | 0.0 |
| `routes/federation/transaction/edus.rs` | 29 / 191 | **15.2%** | 0.0 |

**(2) 0.0 的成因是"地板冻结"，不是"没测到"** —— 三条独立证据：

| 证据 | 做法 | 结果 |
|---|---|---|
| bootstrap 时这些文件**没有测试** | `git show 4565cecd5:<file>` 数 `#[cfg(test)]`/`#[test]` | 8 个里全部为 **0**（bootstrap = 2026-09-19） |
| 地板只在**写回**时才抬升 | `save_baseline()` = `max(prev, cur)`，`--save-baseline` 只在 CI 的 auto-ratchet 步用到 | 抬升通道**只有** CI 那一条 |
| 那条通道**从未成功过** | `git log --all --grep="auto-update per-file baseline"` | **0 条提交**（`ci.yml:1547` 的 "Commit coverage baseline (auto-ratchet)" 一次都没落地） |

**(3) 更进一步：覆盖率车道在最近的运行里连启动都没有。**

`coverage` job 的前置**当时**是 `needs: [integration-test, changes]`（现在的 `ci.yml:1405` 已改成 `needs: [changes]`，见 H），而 `integration-test`
的前置是 `needs: [test, changes]`（`ci.yml:908`）—— **`test` 一旦红，下游全部 skip**。
实测最近 30 次 CI 运行**全部 failure**，抽查 6 次的 `Code Coverage` 与 `Integration Tests` 结论**全是 `skipped`**：

```
gh run view 40287200935 --json jobs --jq '.jobs[] | "\(.conclusion)\t\(.name)"'
→ failure  Test & Lint (all-features)  …  skipped  Integration Tests  skipped  Code Coverage
```

⇒ 这条"等 CI 会给我答案"的等待理由与 §7.5 里那 3 个 `feature` 关文件**同性质：永远不会兑现**。
**处置（2026-10-27 更新）**：**车道已解耦**（见下面的 H），基线文件仍**不改** —— 本地只有单腿读数，
`--update` 会把 608→1 的截断风险重新引进来（§3.1.1）；实测值只作**下限证据**记在这里。
地板要真的抬起来，还差最后一步：main 上真跑一次 `Code Coverage` 让 auto-ratchet 落地。

#### H. 把 `coverage` 从 `needs` 链解耦（2026-10-27 已改）

G(3) 已经把"车道不发车"钉死了，但光记录不够 —— 那条依赖是**纯粹的串联**，没有正确性理由：

| 事实 | 依据 |
|---|---|
| `run_coverage.sh` **自己**跑两条腿（storage 单跑 + 其余 workspace + 合并 lcov） | 脚本 91–103 行 |
| 它**不消费** `integration-test` 的任何产物（无 `download-artifact`、无共享缓存） | 脚本全文 |
| 因此 `needs: [integration-test]` 只保证了"integration 绿了才测覆盖率" | —— |
| 而 `integration-test` 恒 `skipped`（`needs: [test]`，`test` 长期有独立红灯） ⇒ 覆盖率**一次都没跑过** | `gh run view`（G(3)） |

改法（`ci.yml:1394` 起）：

```yaml
  coverage:
    name: Code Coverage
    needs: [changes]        # 原: [integration-test, changes]
    if: >-                  # 触发条件一字未改
      github.event_name == 'schedule' || … (main push 且非纯文档)
```

**成本口径不变**：`if:` 仍只在 schedule / 慢档 dispatch / main push 且含非文档改动时发车，
原本"这些事件各跑一次"就是设计意图，现在只是让它真的发生。
**风险**：`test` 红的时候 coverage 仍会跑（并行），可能一起红 —— 但那是**两个独立答案**，
比"下游全 skip、什么都没测"强。`integration-test` 自己**没有**解耦：unit 红时跑 integration 才是纯浪费。

门禁复核：`python3 scripts/ci/check_gate_reachability.py` → `OK 门禁可达性：58 个 check_* 脚本全部可达或已登记`。

> 剩下的问题不在这条链上：`Test & Lint` 的四个变体各有**独立**红灯
> （`Run unit test target (--test unit)`、`Check missing_docs ratchet`、`Schema table coverage`、
> `Generate client.yaml`）。那是四条独立缺陷，不在本审计范围。

#### I. 地板抬升：20 条冻结值改成实测下限（2026-10-27）

G 说清了"0.0 是 bootstrap 冻结值"，但**只记录不改**等于棘轮对这批文件仍然无效。本轮把能拿到
**同一口径读数**的条目全部抬到实测值（作为下限，不是目标值）。

做法（刻意**不用** `--update`/`--save-baseline` 写整个报告）：

| 数据源 | 取用范围 | 为什么可以当**下限** |
|---|---|---|
| `/tmp/web_lib.info`（`-p synapse-web --lib`，`REST_FEATURES` 与 CI 第 2 腿**同一 feature 集**，967 passed） | 该报告里的**全部**既有条目（`synapse-web/**`） | CI 的合并报告 = lib 腿 + integration 腿 ⊇ lib 腿 ⇒ CI 读数只会更高 |
| `/tmp/storage_dyn.lcov`（`-- api_dyn_tests`，只跑了 12 条用例） | **只取 2 个文件**：`room/api.rs`、`sliding_sync/api.rs` | 只对"这 12 条用例就是全部来源"的文件成立；其余 storage 文件的读数会被这 12 条严重低估 |

规则三条（脚本里带 `assert`）：**只抬不降**、**不新增条目**（否则等于把新文件的 ramp-up 门槛提前豁免——
报告里 17 个未入库文件里有 4 个是 0%，绝不能用一次单腿测量把它们"转正"）、**键集合必须与原来完全一致**。

结果：`608 → 608` 条，**20 条被抬升、0 条被降低、0 条新增**：

| 文件 | 原 | 现 | | 文件 | 原 | 现 |
|---|---|---|---|---|---|---|
| `synapse-web/routes/extractors/mod.rs` | 0.00 | **100.00** | | `synapse-web/routes/federation/membership/query.rs` | 0.00 | **44.07** |
| `synapse-web/routes/federation/media.rs` | 0.00 | **70.99** | | `synapse-web/routes/handlers/dehydrated_device.rs` | 29.49 | 30.67 |
| `synapse-web/routes/sticky_event.rs` | 0.00 | **53.46** | | `synapse-web/routes/oidc/sso.rs` | 33.78 | 34.90 |
| `synapse-web/routes/federation/membership/invite.rs` | 26.35 | **35.58** | | `synapse-web/routes/handlers/room/management/upgrade.rs` | 0.00 | **32.05** |
| `synapse-web/routes/handlers/room/management/visibility.rs` | 0.00 | **43.75** | | `synapse-web/routes/push_notification.rs` | 24.60 | **50.99** |
| `synapse-web/routes/admin/room/spaces.rs` | 0.00 | **20.35** | | `synapse-web/routes/validators.rs` | 98.65 | 99.17 |
| `synapse-web/routes/federation/transaction/edus.rs` | 0.00 | **15.18** | | `synapse-web/routes/cas.rs` | 22.27 | 29.41 |
| `synapse-web/routes/admin/report.rs` | 7.46 | **35.09** | | `synapse-web/routes/external_service.rs` | 23.36 | 29.18 |
| `synapse-web/routes/oidc/provider.rs` | 5.59 | **18.70** | | `synapse-web/utils/admin_auth.rs` | 63.03 | 66.67 |
| `synapse-storage/room/api.rs` | 0.00 | **28.71** | | `synapse-storage/sliding_sync/api.rs` | 0.00 | **14.58** |

⚠️ 这 20 条**不是**"提升后的质量基线"，而是"如果掉到这个数以下就说明真的退化了"的**下限**；
第一次在 main 上跑通 `Code Coverage` 后，auto-ratchet 会按合并报告把它们继续往上抬。

1. **`cargo llvm-cov --workspace --lib --all-features` 不可靠**：跑到 997 passed / **1 failed** 就整体退出，
   **lcov 一条都没落盘**（`cargo llvm-cov` 在测试失败时不生成报告，也没有 `--ignore-run-fail`）。
   失败的是 `synapse-common::test_isolation::tests::prune_backfills_legacy_zero_row_marker_and_spares_it`，
   而它**单独跑、单 crate 全量跑都通过**（998 passed / 7.16 s）。
2. 复现尝试（普通 `cargo test --workspace --lib --all-features`）跑了 **41 min 仍在 storage 的 DB 用例上**，
   被我中止 —— 这条复现路径本身太贵。

⇒ **别**为了取数去跑 `--workspace --lib --all-features` / 全量 integration 插桩（代价见上两条）。
但"别跑全量"不等于"只能等 CI"：**按 crate 切**就能用分钟级成本拿到读数 ——
`-p synapse-web --lib` 6m02s 就回答了 8 个文件（§7.5 G），`-p synapse-services --lib`、`-p synapse-storage --lib`
同理。上一版这里写"正确做法仍是让 CI 跑一次 main push"，**那条路已经不通**（G 的第 3 条证据）。

#### 那个 flake：机制已坐实、已修（2026-10-26 本轮）

上一版这里写的是"假设"：`build_template` 在"模板已就绪"分支是 `DELETE` + `INSERT` 两条独立语句，
各自 best-effort（失败只 `warn!`），INSERT 一失败就把标记表留成**零行**；而
`prune_isolation_templates` 把零行解释为"遗留模板、可能正在被克隆"予以豁免，
`prune_backfills_legacy_zero_row_marker_and_spares_it` 又断言"刚建好的模板至少一行"——
两条语义在零行这一点上直接冲突。

**本轮把它从假设做成了可复现的判据**：加一条测试，用 `BEFORE INSERT` 触发器让标记写入**必然失败**：

| 版本 | 断言 | 结果 |
|---|---|---|
| 事务化（当前） | 失败后行数必须等于原值 | **PASS**（行数保持 1，删除被回滚） |
| 退回两条独立语句（变异） | 同上 | **FAIL** `left: 0 / right: 1` —— 零行窗口真的存在 |

修法：新增 `refresh_template_marker_row`，把 `DELETE` + `INSERT` 放进**同一个事务**，
失败整体回滚 ⇒ 标记表不可能出现零行。语义从"尽量刷新"变成"要么恰好一行且是 now()，要么保持原样"。

新测试：`a_failing_marker_refresh_never_leaves_the_template_with_zero_marker_rows`。
`cargo test -p synapse-common --lib --all-features` ⇒ **999 passed**（998 + 本条），clippy `-D warnings` 干净。

**与那次 `--workspace` 失败的因果关系仍未证明**：我没拿到当时的断言原文。
能确定的是：**零行窗口真实存在、能让上述断言红**，这条修复消除了一条确定的致红路径。

测试本身也踩了一个坑（写下来）：第一版没有开跑前清场，变异那次跑完 `assert` 先 panic、
清理没执行，留下一个**带 boom 触发器且零行**的模板；恢复修复版后复跑，`ready` 路径复用它、
`rows_before > 0` 的前提直接不成立 → 测试红。**清理必须放在开头**（`DROP SCHEMA IF EXISTS`），
不能只放在结尾。

### 7.6 途中被证伪的十四个判断（留着，防止后来者重走）

| 我说过 | 实测/核实结果 | 靠什么推翻 |
|---|---|---|
| "`friend_room/repository.rs`、`widget.rs`、`burn_after_read.rs` 的 0% 是基线陈旧（内联测试会执行本文件）" | **三个模块都被 feature 门控**（`friends` / `widgets` / `burn-after-read`），两条覆盖率腿的 feature 集都不含 ⇒ **文件根本不编译、不进报告**，0.0 是 bootstrap 值被 `max()` 冻结。三个里**没有一个**是陈旧 | 读 `synapse-storage/src/lib.rs:172/203/215` 的 `cfg` + `run_coverage.sh` 的 `STORAGE_FEATURES`/`REST_FEATURES` + 四份历史基线快照全为 0.0 |
| "22 个 storage `test_mocks` 是基线陈旧（不含 `test-utils` 的 feature 集下测得）"，**改判两次** | 第 1 次证伪：两腿都带 `test-utils` ⇒ 前提假。第 2 次收口：它们**根本不在门禁检查范围**（`TEST_ONLY_MARKERS` 含 `"test_mocks/"`，`:378` 直接 `continue`）⇒ 前两轮都在给"门禁不看的文件"写判定 | `cargo llvm-cov --help`（`--exclude` 同时排除测试与报告）+ 32 条基线条目 10 非零/22 零的双向对照 + 调用门禁自身 `is_test_only()` 计数 |
| **"基线里 59 个 0% 文件需要逐个分流"** | **分母错的**：59 里 22 个 `test_mocks/` + 5 个 `src/bin/*`/`main.rs` 被门禁自己的两张清单跳过，**真正的 0% 是 32 个** | 直接 `import` `check_file_coverage.py` 跑 `is_test_only()` 与 `_matches_prefix(non_unit_coverable)`，得 59 = 22 + 5 + 32 |
| "6 处 mock 高危，会让安全断言永远成立" | **0 处假绿**：2 良性（`add_to_blacklist`、`acquire_lock_with_retry` 的核心语义都保留了）/ 1 理论危险但整条链未接线（`get_room_media`）/ 1 无断言依赖（`validate_proxy_ticket`）/ 1 走不到（`claim_to_device_events`）/ 1 已声明偏差（`get_device_list_changed_users`）—— 2026-10-27 把原"待查"4 处全部定级（§4.2.2） | 逐个核调用链 + 读断言 + 读 mock 实现 |
| "静默期测试写好了"（`pause()` + 断言台账 `None`） | **假测试**：把静默期 300 s 改 10 s 仍 PASS | 变异自证 |
| "看板读到台账非空会误判正常" | 四个台账**没有任何生产读者**，是只写不读的死字段 | `grep` 全仓消费者 |
| "修第 1 层的参数就能接通族撤销" | 修完仍 `"Token has been revoked"` —— 第 0 层 `validate_token` 才是阻断点 | 改完直接跑那条测试 |
| "给 `PreviewUrlParams` 加 `deny_unknown_fields` 会让那条 `ts` 用例先红"（写在**测试注释**里） | **不会红**——`ts` 是已声明字段，不属于 unknown field；有效变异是 `ts` 改必填 | 变异自证实跑：`deny_unknown_fields` ⇒ PASS，`ts: i64` ⇒ `left: 400 / right: 200` |
| "`synapse-common/config/manager.rs` 是**最实的一个真缺口**，`ConfigManager` 是没人测的配置读写器，值得补 `tempdir` 测试" | **它不是"没人测"，是"没人用"**：全仓 `ConfigManager::new` **0 个构造点**（命中的全是 `RateLimitConfigManager`），`read_config/write_config` 的 `_location` 参数被**完全忽略**、没有任何文件 IO —— 一个从未接线的 `Arc<RwLock<Config>>` 壳。补测试等于给死代码刷覆盖率 | `grep -rn "ConfigManager::new"` 逐条看命中 + 读 81 行源码（无 `std::fs`/`Path`）+ 删除后 `cargo check` 两次全绿 |
| "`src/common/error.rs` 的 2 个转换 fn 是真缺口（小）" | **是死代码**：除了自身定义与 `common/mod.rs` 的 re-export，**零调用点**；`synapse-e2ee` 里那几个 `test_crypto_error_to_api_error_*` 测的是该 crate 自己的同名函数（跨 crate 不可见） | 全仓分别 `grep` 这两个函数名，命中只有定义 + re-export（详见 §7.4 的两条 grep） |
| "这 8 个 `synapse-web/routes/**` 只能等 CI 的 integration 腿，本地拿不到读数" | **不需要那条腿**：8 个文件全在 `synapse-web` crate、模块**无条件**编译、**每个都自带内联测试** ⇒ `-p synapse-web --lib`（6m02s，967 passed）一次全出，15.2%–100% 无一为 0。上一版把它归因于"插桩口径不允许局部回答"，是把只适用于**跨 crate** 情形的 `--exclude-from-test` 结论套到了本 crate 文件上 | `cargo llvm-cov -p synapse-web --lib … --lcov` 实测 `LH`/`LF`（§7.5 G 表）+ `grep -B2 'pub mod'` 确认无 `cfg` 门控 |
| "等 CI 跑一次 main push 就能把 0.0 刷新掉" | **那条通道从未成功过**：`git log --all --grep="auto-update per-file baseline"` **0 条**；最近 30 次 CI 全 `failure`，抽查 6 次的 `Code Coverage` / `Integration Tests` 结论**全 `skipped`**（`coverage` 需要 `needs: [integration-test]`，而 `integration-test` 需要 `needs: [test]`，`test` 恒红 ⇒ 下游一律 skip）⇒ 基线里 32 个 0.0 是 2026-09-19 bootstrap 的**冻结值**，不是当期读数 | `gh run view <id> --json jobs --jq` 逐 job 取结论 + `git log --all --grep` + `ci.yml:1397/908` 当时的 `needs` 链（现在 `coverage` 那条已解耦，见 §7.5 H） |
| "`room/api.rs` / `sliding_sync/api.rs` 只剩 5 行可执行、是**死默认实现**，处置=删掉或标注理由" | **三条全错**：①LLVM 实测可执行行是 **101 / 96**，机械口径只数关键字而纯转发体一个关键字都没有 ⇒ 漏计 95%；②不是默认实现而是 `impl … for RoomStorage`（`api.rs:305`/`:197`），生产装进 `Arc<dyn …>`（`wiring/rooms.rs:88` 等 20+ 处）⇒ **删掉 services 编译不过**；③0% 机制同 `event/reader.rs`（本 crate 测试全走固有方法）。补 12 条 `&dyn` 契约测试后实测 **28.7% / 14.6%** | `cargo llvm-cov -p synapse-storage --lib … -- api_dyn_tests` 的 `LH`/`LF` + 装配点 `grep`（§7.5 E） |
| "`SlidingSyncToken.pos` 就是 `create_or_update_token` 的 `event_stream_pos` 参数"（写在**新测试的断言**里） | **不是**：`pos` 取 `nextval('sliding_sync_pos_seq')`，是客户端 token 校验用的序列值；`event_stream_pos` 才是调用方传的水位线（`repository.rs:47`）。断言跑出 `left: 1, right: 42` | 实跑 `-- api_dyn_tests` 打回，读 `create_or_update_token` 的 SQL 确认 |

共同根因有四条：①**把"代码看起来像 X"当成"X 成立"** —— 写"高危 / 看板会 / 这条测试覆盖了"之前，
必须先 `grep` 消费者或做变异自证；②**自建口径而不复用门禁的判据** —— 统计"覆盖率缺口"时应当
直接调用门禁自己的 `is_test_only()` / `_matches_prefix()`，否则算出来的分母包含一堆门禁压根不看的文件
（本轮 59 vs 32 就是这么来的，白花了两轮判定）；③**把"0% 覆盖率"默认读成"缺测试"** ——
它也可能是"没有消费者"（`config/manager.rs` 零构造点）、"只有别的 crate 会调"（reader/writer 的转发层）、
或者"被 feature 关掉"，或者**只是地板冻结**（本轮 10 个）。**先问"谁该跑它"，再决定"补测试还是删代码"**；
④**把"以后会有人给我答案"当结论** —— 凡是要"等 CI / 等下游 / 等别人"才能闭合的判定，先证明那条通道
**还在跑**（`gh run view <id> --json jobs` 看 job 结论，而不是看 yaml 里写着），否则记下的是**永远不会兑现的
等待理由**：本轮 3 个 `feature` 关文件 + 8 个 web 路由全部踩在这上面，11 个 0% 的"理由"其实是同一条。

### 7.7 覆盖率车道**终于跑完了**，然后棘轮红在 134 个文件（2026-10-27，`0a5310346` / run `40469375466`）

§7.5 里那句"3 个 feature 关文件的真实读数只能由完整 storage 腿给出"这次有了读数：`Run coverage`
（`scripts/ci/run_coverage.sh` 三条腿）**第一次成功**，耗时约 75 分钟，`Code Coverage` 的红从
"跑不到"变成"跑到了但不达标"——失败步是 `Per-file coverage ratchet (B.3)`。

现场读数（原文照抄门禁输出）：

```
=== Core-path coverage failures (6 files) ===
  [CORE] synapse-services/auth/credential_auth.rs: 0.0% < 70% (was no baseline, delta=+0.0%)
  [CORE] synapse-services/auth/token_auth.rs: 0.0% < 70% (was no baseline, delta=+0.0%)
  [CORE] synapse-storage/event/cascade.rs: 48.6% < 70% (was no baseline, delta=+48.6%)
  [CORE] synapse-storage/event/history_boundary.rs: 0.0% < 70% (was no baseline, delta=+0.0%)
  [CORE] synapse-web/routes/msc4512_proxy.rs: 24.8% < 70% (was no baseline, delta=+24.8%)
  [CORE] synapse-web/routes/worker/worker_topology.rs: 45.1% < 70% (was no baseline, delta=+45.1%)

=== Coverage failures (134 files) ===
  [TOUCHED] synapse-common/tracing.rs: 19.4% < 75% (was 74.6%, delta=-55.2%)
  [TOUCHED] synapse-federation/client.rs: 36.8% < 59% (was 59.5%, delta=-22.7%)
  [TOUCHED] synapse-services/admin_media_service.rs: 72.8% < 100% (was 100.0%, delta=-27.2%)
  [TOUCHED] synapse-common/config/mod.rs: 88.2% < 99% (was 99.3%, delta=-11.1%)
  [NEW]     synapse-common/content_scanner/mod.rs: 34.5% < 50% (was no baseline)
  [NEW]     synapse-common/test_database_guard.rs: 21.8% < 50% (was no baseline)
  …（共 134 条，全部见该 run 的 `Code Coverage` 日志）
```

**判定：性质未定，禁止用 `--update` 消化。** 两类信号必须分开处理：

1. **`was no baseline` 的 6 条 core-path**：这是"从未进过基线"的**新分母**，0% 的三条要么缺测试、
   要么与 §7.5 的"跨 crate 转发层 / 无消费者"同类 —— 按 §7.5 的方法逐条定性，不能一概补测试。
2. **`was 74.6% → 19.4%` 这种大幅负 delta**：单文件掉 55 个点不像"代码变差了"，更像**测量口径变了**
   —— 本轮 `run_coverage.sh` 的 `STORAGE_FEATURES` 增加了 `friends,widgets,burn-after-read`
   （三个此前**从不编译**的 feature 现在被编进分母），而 lcov 的分母随编译单元变化。
   **这条只是候选解释，尚未证实**；证实方式是对同一 commit 分两次取 lcov（加/不加这三个 feature）
   逐文件比对，而不是看总数。

⚠️ **为什么必须明确禁止 `--update`**：基线里 134 个值一旦按"当前这次测量"重写，等于把上面第 2 类
（很可能是口径变化而非真实下降）永久固化成新的地板 —— 与 §7.5 记录的"32 个 0.0 是 bootstrap 冻结值"
是同一个坑，只是方向反过来。**先证实口径，再谈抬/降。**

**下一步（未做）**：①取同一 commit 的两次 lcov 做逐文件对比，判定第 2 类是口径还是真实下降；
②对 6 条 core-path 按 §7.5 的方法逐条定性（缺测试 / 无消费者 / 跨 crate / feature 关）；
③只有①②结论明确后，才允许动 `scripts/ci/coverage_baseline.json`。

#### 7.7.1 补记：第 2 类的性质有了第一条**不依赖 lcov** 的硬证据（2026-10-27 晚）

上面把"3 个 feature 改分母"列为候选解释。补验后：它**不足以解释主现象**，而手上多了一条
更硬的证据。

1. **`synapse-common/src/tracing.rs` 在基线之后零变更**：今日仍是 101 行，`090742266..HEAD`
   对该文件 **0 个提交**，与基线期逐字相同；而读数从 `74.59%`（基线里的原值）掉到 `19.4%`。
   同一份代码掉 55 个点，**不可能是"测试变差"**，只能是"跑的测试集变了"。
2. **per-file 棘轮此前从未在 CI 跑过**：`c952ee2b0` 的提交说明自己写着"最近 30 次运行里
   `Code Coverage` 与 `Integration Tests` 的结论**全是 skipped**，per-file 棘轮从未跑过"。
   而基线 608 条（含 49 条 bootstrap 冻结的 `0.0`）来自本地产出、**从未被 CI 复现过**
   ⇒ 134 条严格说不该叫"回归"，该叫"**首次差异**"。
3. 那 3 个 feature **只进 leg 1**（`cargo llvm-cov -p synapse-storage --features "$STORAGE_FEATURES" --lib`），
   而 `tracing.rs` 属 leg 2（`--workspace --exclude synapse-storage --features "$REST_FEATURES"`）
   ⇒ 它的读数**不受**那三个 feature 影响。即便"改分母"成立，也只解释 storage 那一段。
4. 顺带排掉一个怀疑：`scripts/merge_lcov.py` 是按行取 max 的**并集**（`max(existing, hits)`，
   再重算 `LF`/`LH`），不是"后者覆盖前者" ⇒ 合并本身不制造下降。

⚠️ 另注意两类不能混：`synapse-federation/src/client.rs`（2035 → 1448 行）与
`synapse-common/src/config/mod.rs`（2316 → 1404 行）在本区间被**大幅删改**，它们的 delta
里混着真实代码变化；只有 `tracing.rs` 这种"代码不动而读数暴跌"才是口径的铁证。

**新的头号候选（尚未证实）**：leg 2 的**集成测试有没有真跑**。本仓集成测试在
`setup_fresh_test_app()` 返回 `None` 时是**静默 return**（§7.5 与本仓 env 笔记都记过），
而"跑到了 / 跳过了"的唯一便宜判据是耗时（正常 ~15 s/条 vs 0.04 s）。若 leg 2 大面积跳过，
所有**由集成测试覆盖**的文件会一起塌陷 —— `tracing.rs`、`credential_auth.rs`、`token_auth.rs`、
`history_boundary.rs` 全在这个集合里，与 134 条的形状吻合。
判据入口：`Run coverage` 日志里 leg 2 的测试计数（明显低于 `Integration Tests` 车道即异常）。

**仍然禁止 `--update`**：上面 4 条只证明"这次测量与基线不可比"，一次也没有证明"地板该降"。

#### 7.7.2 结论：`tracing.rs` 的 55 个点是**基线跨文件重写**，不是测试集变化（2026-10-27 收口）

§7.7.1 的"头号候选（集成测试静默跳过）"**被证伪**；真因是基线值与当前文件**不是同一份代码**。

证据链（全部可复现）：

1. **基线首现提交**：`git log -- scripts/ci/coverage_baseline.json` 首条为 `4565cecd5`（2026-09-19
   bootstrap）。逐提交取该条目：`4565cecd5` / `88001b4a9` / `8df3be4c1` / `090742266` / `c952ee2b0` /
   `HEAD` **六处全为 `74.59`**，即基线自诞生从未被重算过。
2. **基线期的文件是 248 行**：`git show 4565cecd5:synapse-common/src/tracing.rs | wc -l` = 248，
   内含 `DistributedTracer`（`new` / `init_tracer` / `create_span` / `with_span` / `get_trace_id` /
   `get_span_id` / `Default`）、`trace_span!` / `trace_async!` 两个宏，以及 **10 个**内联测试
   （`test_distributed_tracer_*` ×4、`test_trace_span_macro`、`test_trace_async_macro`、
   `test_init_tracer`、`test_get_trace_id_without_active_span`、`test_get_span_id_without_active_span`、
   `test_request_id_display`）。74.59% 是**这些测试覆盖那 248 行**的读数。
3. **重写发生在 `090742266`**（"冗余与过度开发专项审查 —— 死模块清理"）：该文件
   `2 insertions(+), 149 deletions(-)`，`DistributedTracer` 与两个宏整段删除（生产调用图为空），
   文件缩到 101 行，内联测试只剩 `test_request_id_display` —— 而它只覆盖 `Display`。
   `RequestIdPropagationLayer::on_new_span` 的两条路径**一个新测试都没有**。
4. **棘轮语义决定了它无法自愈**：`check_file_coverage.py` 的 `save_baseline()` 取
   `max(prev, cur)`（只上不下），而唯一能刷新基线的 CI auto-ratchet 步从未成功过（本报告
   2026-10-27 条已证伪"等 CI"）。⇒ 一次**合法**的"删除 149 行自测死代码"必然把这 55 个点
   永久冻成红，且没有任何机械通道可以修。
5. **读数复算**：重写后的文件本地 `cargo llvm-cov -p synapse-common --lib` = `LH 7 / LF 36`
   = **19.4%**，与 CI 读数逐位相同 ⇒ 该次 CI 里 `tracing.rs` 的记账**完全由 synapse-common
   lib 单测解释**，集成测试贡献 0 行。而"贡献 0 行"本就有结构性原因，不需要"静默跳过"来解释：
   `RequestIdPropagationLayer` 全仓唯一注册点是 `synapse-common/src/logging.rs:51` 的
   `init_logging`（仅被 `src/server/telemetry.rs:39` 在服务启动路径调用），而集成测试的
   `init_tracing()`（`tests/integration/mod.rs`）只装 `tracing_subscriber::fmt()`；
   `grep -rn "init_logging|RequestIdPropagationLayer" --include=*.rs` 在测试代码里**零命中**。

**处置（不需要动基线）**：按 §⑤/⑦ 已确立的"补测试"路线，给 `tracing.rs` 补 2 个内联测试
（[tracing.rs:135](../../synapse-common/src/tracing.rs#L135) 覆盖 case 1 字段抽取 + case 2
父链继承 + 无标记祖先；[tracing.rs:170](../../synapse-common/src/tracing.rs#L170) 覆盖
`record_debug` 的非 `&str` 兜底），用一个测试专用 `CapturingLayer` 注册在待测 layer **之后**，
从 span extensions 读回结果。实测（`cargo llvm-cov -p synapse-common --lib`）：

```
synapse-common/src/tracing.rs: LH=84/LF=85 = 98.82%
```

`98.82% > 74.59%` ⇒ 棘轮**自然转绿**，无需 `--update`/`--save-baseline`（二者仍被禁止）。
余下 1 行是 `ctx.span(id) == None` 的防御性早退，正常 registry 下不可达。

**本项目"per-file 棘轮"的一个已知盲区（登记，不改代码）**：单文件被合法重写（删除被自测覆盖的
死代码）时，因为 `save_baseline()` 只取 `max`，基线会永久高于现实值，红不是"回归"而是
"基线过期"。判据 = 该文件在基线提交与当前**行数/结构不同**；处置 = 补测试抬真实覆盖率
（首选，本轮采用）或按 §7.7 的纪律另行评审后手工改基线（需明确批准，本轮未做）。

#### 7.7.3 `--exclude` → `--exclude-from-test`：**本轮不采纳**

§B 末尾提出的 flag 级改造（让 storage 进 leg 2 的报告，从而给"测试在别的 crate、实现在 storage"
的文件记账）本轮**不实施**，理由三条：

1. **它不解释也不修复本轮的红**：`tracing.rs` 属 `synapse-common`，leg 2 本来就把它包含在报告里，
   换 flag 对这一条零影响（§7.7.2）。
2. §⑤（`event/reader.rs` / `writer.rs`）与 ⑦（`synapse-web/routes/**`）两条同类红已按"补测试"路线
   关掉（§⑤ 原文即写明"**不需要**用 `--exclude-from-test` 动 CI 口径"）。
3. 该改动**本地无法验证**（需一次 ~75 min 全量覆盖率），且审计原文已列出两个未验证风险
   （`cfg` 差异导致文件出现/消失、报告条目数显著增加）。在"没有一条已证实的红由报告排除引起"
   之前动它，属于用未验证的口径变化替换已解释的现象。

**重开条件**：出现"实现所在包被 `--exclude`、其测试却只存在于其它包、补测试后仍无法记账"的文件时，
再按 §B 的双向闭式验证法实测该 flag。

### 7.8 S1：补全报告范围，并修掉由此暴露的**分母口径**缺陷（2026-10-28）

#### 7.8.1 三处改动

| 文件 | 改动 | 为什么 |
|---|---|---|
| `scripts/ci/run_coverage.sh` | 步骤 2 的报告加 `REST_REPORT_ARGS`，**9 个工作区成员全部显式 `-p`**（含 `synapse-storage`）；测试范围仍是 `--workspace --exclude synapse-storage` | `cargo llvm-cov report` 不带 `-p` 时按"当前包"过滤（实测根 crate 口径只有 18 个文件）。之前 8 个包是逐个手写的，漏一个就静默少覆盖，且 storage 只在 leg 1 出现 |
| `scripts/merge_lcov.py` | 同一 `SF` 出现在多条腿时，`LF`/`LH` 改为**取输入单腿值与 DA 下界中的较大者**，不再 `LF = len(da)` | 见 7.8.3：旧实现在"同一文件进两条腿"时会**换掉分母定义** |
| `scripts/check_file_coverage.py` | `[DECL-ONLY]` 豁免从"仅新增文件"放宽到"基线已知且基线值也为 0"的文件 | 见 7.8.4 |

#### 7.8.2 报告范围扩大的实测影响：**非 storage 文件 524/524 逐字节不变**

用磁盘上残留的 22 个 `.profraw` 生成两份报告（同一份 profraw，只改 `-p` 过滤）：

| 报告范围 | 文件数 | 与"9 包"对比 |
|---|---|---|
| 旧口径（8 包，无 storage） | 524 | — |
| 新口径（9 包，含 storage） | 676 | only-in-old = 0，only-in-new = **152（全是 storage）** |

524 个非 storage 文件的记录**完全相同（identical=524, different=0）** ⇒ 这次改动对它们零扰动；
多出来的 152 条正是 storage。副作用在 7.8.3。

#### 7.8.3 副作用：同一文件进两条腿后，旧 `merge_lcov.py` 会把分母改写

storage 现在在 leg 1（`-p synapse-storage --lib`）和 leg 2 的报告里**都出现**，
于是 152 个 storage `SF` 全部走"merged"分支，旧实现按 `LF = len(da)` 重算总数。

**llvm-cov 的 `LF` 本就大于 `DA` 行数**（函数定义行等"幽灵行"有覆盖区域但没有执行计数），
所以这不是四舍五入，而是**换了分母定义**：

| 口径 | `synapse-storage/src/membership/mod.rs` |
|---|---|
| llvm-cov 单记录 | LF=1019 / LH=450 = **44.2%**（DA 590 行、命中 403） |
| 旧 merged 分支 | LF=590 / LH=403 = **68.3%** |

152 个 storage 文件里 **131 个**受影响（LF 被砍掉的幅度 top5：`membership/mod.rs` −429、
`test_mocks/event.rs` −411、`sliding_sync/repository.rs` −332、`push/mod.rs` −293、
`application_service/repository.rs` −266）。聚合口径上，旧实现把 storage 的 82.55% **抬高到 87.40%**
（LF 48400→42010、LH 39955→36715）—— 也就是说：**旧实现不是压低、而是虚高**，
和"合并只会把值抬高"的直觉相反，因为它的"抬高"来自把没命中的分母整个删掉。

**修法与自证**：`LF = max(各输入 LF, len(merged DA))`、`LH = max(各输入 LH, DA 命中行数)` 并 `min(lh, lf)`；
真并集的幽灵行无法从 lcov 观测，取 max 是保守下界（不会凭空造出任何输入没报过的覆盖率）。

实测（合成夹具，见 7.8.6 的守卫用例）：A(`DA:1,1 DA:2,0 LF:10 LH:5`) + B(`DA:1,0 DA:2,1 DA:3,0 LF:10 LH:5`)
→ 合并 `LF:10 LH:5`（旧实现给 `LF:3`）；单来源记录的 LF/LH 原样保留。

实测（真实产物，`/tmp/s1_ab_storage.lcov` + `/tmp/s1_ab_9.lcov` 合并）：

| 指标 | 单腿记录 | 修复后合并 | 差异 |
|---|---|---|---|
| 152 个 storage 文件的 LF/LH | LF=48400 LH=39955 | LF=48400 LH=39955 | **0 处不一致** |

⇒ storage 的百分比重新与"单腿记录"（也就是基线记录时的口径）可比。

#### 7.8.4 `[DECL-ONLY]` 放宽：只对"本来就 0"的文件生效

旧条件要求 `prev is None`，于是基线里**已知为 0** 的纯声明文件（只有 `fn …;`）永远拿不到豁免。
新条件：`coverable ≤ 3` ∧ `cur == 0.0` ∧ (`prev is None` ∨ `prev == 0.0`)。
基线值 > 0 的文件仍走原来的回归比较（`prev > 0` 时绝不进豁免分支），回归保护没有放松。

本轮命中的只有 2 个：`synapse-services/auth/credential_auth.rs`、`synapse-services/auth/token_auth.rs`
（各 2 行可覆盖、0 命中）。

#### 7.8.5 完整跑一次的读数（修复前 vs 修复后）

修复前的完整 `bash scripts/ci/run_coverage.sh`（EXIT=1）：

| 指标 | 读数 |
|---|---|
| 合并报告 | `files=676, LF=206098, LH=151090, coverage=73.31%` |
| 棘轮 | **154 红** = 150 `[TOUCHED]` + 4 `[NEW]` |

用同一份 profraw 按修复后的合并逻辑重算：

| 指标 | 修复前 | 修复后 |
|---|---|---|
| `files` | 676 | 676 |
| `LF` | 206098 | **212488**（+6390，正好等于被旧实现砍掉的 storage 幽灵行） |
| `LH` | 151090 | **154330** |
| `coverage` | 73.31% | **72.63%**（−0.68 pt） |

4 个 `[NEW]` 全是基线快照之后新增的文件，与 S1 无关：
`synapse-common/test_database_guard.rs` 14.8%、`synapse-services/builtin_oidc_provider.rs` 0.0%、
`synapse-services/worker/event_sink.rs` 0.0%、`synapse-services/worker/topology_metrics.rs` 43.6%。

同一轮还有 10 个测试失败、3 个 target 红（`thumbnails.is_animated` 列缺失、`RUST_TEST_THREADS`
污染、381 个孤儿 test schema），**不是 S1 引入**：`grep "test result:"` 复核为
**passed=10909 / failed=10**，覆盖率主体确实跑过。

#### 7.8.6 结论

1. **对非 storage 文件零扰动**：524/524 记录逐字节相同（7.8.2）。
2. **storage 的 DA 行集不变**，变的只是分母记账；修复后 152/152 与单腿记录一致（7.8.3）。
3. **两批红都不能归因于 S1**：
   - 非 storage 的红是既有漂移/环境（如 `synapse-common/config/mod.rs` −53.1%、`logging.rs` −43.6%）；
   - storage 的红在修复分母后与基线才是同一口径，仍然红 ⇒ 是真漂移（基线快照早于当前代码，
     4 个 `[NEW]` 即为证据）。
4. **本轮不动基线**：重定基线需单独批准，且禁止 `--update` / `--save-baseline`。
5. **守卫**：`tests/unit/coverage_leg_resilience_tests.rs` 新增
   `merging_the_same_file_from_both_legs_keeps_the_lf_denominator`（同一 `SF` 两腿、各自 LF>DA 行数时，
   合并结果不得塌成 DA 计数）。三个相关模块实测全绿：
   `coverage_leg_resilience_tests` 11 passed / `coverage_ratchet_exemption_tests` 9 passed /
   `pr_coverage_lane_tests` 13 passed。

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
| **R2** | 覆盖率门禁不在 PR 车道；新文件达标线 30%；已入库文件无绝对下限 | 全仓覆盖率回归（事后才发现） | **🟡 已收紧**：达标线 30 → **50**（§3.1.1 双向验证）；**基线自毁缺陷已修**（一次不完整 lcov 曾把 608 条砍成 1 条，§3.1.1）；0% 条目**分母已订正为 32**（§7.5）—— 另 22 个 `test_mocks/` + 5 个二进制入口**本就在门禁的两张排除清单里**，不计入：**2 个陈旧 / 4 个真 0（已实测）/ 3 个 `feature` 关（永不刷新）/ 7 个无实质可测行 / 8 个本轮已消除（补测试 6 + 删死代码 2）/ 8 个等 CI**；**能靠"再跑一次覆盖率"变好的只有 2 个**（`router.rs` 已实测 82.9%、`room/membership/federation.rs`），其余 0.0 刷新多少次都不会变 |
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
| **不在 PR 车道** | `ci.yml:1380-1383` 的 `if` 只有 `schedule` / `workflow_dispatch+run_slow_tier` / `push main` | PR 阶段的覆盖回归**看不见**，合并进 main 后才知道 |
| **新文件只需 30%** → **✅ 已改为 50%**（2026-10-25） | `ci.yml:1496` 原 `--new-file-floor 30`（脚本默认 60，被显式下调）。已上调到 50，同步 `run_coverage.sh` 提示、`coverage_policy.md` §1/§2/§3、`TESTING.md` | 用 40% 的假 lcov 喂脚本 ⇒ **exit 1**；50% ⇒ exit 0（不误伤） |
| **已入库文件无绝对下限** | `scripts/check_file_coverage.py:8` 只判 `must not regress`；`ci.yml:1482-1485` 注释写明"absolute floors are deliberately not re-applied to baseline-known files, because…188 files red" | `coverage_baseline.json` 里 **59 个文件 0.0%、109 个 ≤30%** 被永久冻结，再差也不会红 |

基线分布（实测 `coverage_baseline.json`，608 个文件）：中位数 **81.5%**、均值 68.1%，
所以**不是"全仓覆盖率低"**，而是"**差的那批被冻结了，且新代码门槛低**"。

补：

| 优先级 | 动作 |
|---|---|
| **P1 ✅ 已完成** | `--new-file-floor 30 → 50`（先不喊 60，避免一次打红太多）；`core_file_coverage_prefixes.txt` 内的新文件沿用 70 |
| **P2** | 给 0% 文件中**有生产入口**的（`src/server/router.rs`、`src/tasks/mod.rs`、`synapse-common/logging.rs`）建"提升到 30%"的一次性任务，逐文件记账（**不是**一刀切重贴全局下限 —— 那会重演 188 文件红） |
| **P2** | 把 coverage 加进 PR 车道（可只跑受影响 crate 的 lcov，用 `cargo llvm-cov -p <crate>` 降成本） |

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

`logging.rs` / `telemetry.rs` 登记为已知缺口不排期（启动路径，收益低）。

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
| `background_update.rs acquire_lock_with_retry(...)` | 锁持有者/时长/重试次数 | 待查（测试直接调 storage，见 `background_update_service.rs:521` 等 5 处） |
| `cas.rs validate_proxy_ticket(_service_url)` | service_url | 待查 |
| `dehydrated_device.rs claim_to_device_events(全部)` | 全部 | 待查 |
| `device_list.rs get_device_list_changed_users(全部)` | 全部 | 待查 |

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

## 7 一页索引：这轮到底改了什么（38 笔提交，按用途分组）

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

### 7.5 0% 文件的分流：**分母是 32，不是 59**（2026-10-26，累计四次订正；本轮消除 8 个）

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
| ① 已实测：陈旧 | **2** | `router.rs`（82.9%）、`room/membership/federation.rs` | 实测 / 日期 |
| ② 已实测：真 0 | **4** | `services.rs`、`telemetry.rs`、`tasks/mod.rs`、`common/logging.rs` | 实测（含定向插桩） |
| ③ `feature` 关 ⇒ 永久不进报告 | **3** | `burn_after_read.rs`、`widget.rs`、`friend_room/repository.rs` | 编译实验 + 27 条 DB 测试实跑 |
| ④ 无实质可测行 | **7** | 5 个纯 trait 声明 `api.rs` + 2 个 5 行默认实现 | 机械统计 |
| ⑤ 结构盲区 → **已补测试并实测** | ~~2~~ | `event/reader.rs` 0 → **9/95 = 9.5%**、`event/writer.rs` 0 → **5/37 = 13.5%**（`&dyn` 转发层契约测试，见下） | 定向插桩实测 |
| ⑥ 真缺口 → **已全部处置** | ~~5~~ | 删 2（`config/manager.rs` 整类零构造点、`common/error.rs` 2 个零调用点 helper）＋ 补 3（`{friend_room_service,room/state,rtc}/error.rs` 共 8 条断言） | 全仓 `grep` + 11 条测试实跑 |
| ⑦ 待 CI 的 integration 腿 | **8** | `synapse-web/routes/**` | 无（工具口径不允许局部回答） |
| ⑧ 本轮已消除（原 ④⑤⑥ 中的 8 个） | **8** | 4 类来源：2 个 trait impl（补测）+ `event/models.rs`（补测）+ 3 个领域错误转换（补测）+ 1 个死类型 + 1 个死 helper（删除） | 插桩实测 / 编译验证 |

合计：**2 + 4 + 3 + 7 + 8（待 CI）+ 8（已消除）= 32** ✓

> 也就是说棘轮里**还剩 8 个 0%** 是真的拿不到读数（只能等 CI 的 integration 腿），
> 另外 8 个本轮已经有了确定处置，16 个是口径/结构问题。**没有一个 0% 是"待办但没人管"的了。**

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
才执行的文件 —— 它们的 0% 同样永远不变。一条 flag 级处置（**未验证，留给 CI 判**）：

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

#### E. 7 个 `**/api.rs`：5 个"无行可测"，2 个只剩 5 行

机械统计（去掉 `///` 注释后数 `Ok(`/`Err(`/`let`/`if`/`match`/`return`）：

| 文件 | 行数 | `fn …;` 声明 | 可执行行 |
|---|---|---|---|
| `presence/api.rs` | 122 | 12 | **0** |
| `cas/api.rs` | 113 | 16 | **0** |
| `room_summary/api.rs` | 281 | 30 | **0** |
| `server_notification/api.rs` | 300 | 32 | **0** |
| `worker/api.rs` | 251 | 31 | **0** |
| `room/api.rs` | 625 | 50 | **5**（2 个默认实现） |
| `sliding_sync/api.rs` | 412 | 22 | **5**（1 个默认实现） |

⇒ 前 5 个是**纯 trait 声明**，0% 是必然，**不是缺口**（与 A 类同性质：指标口径问题）。
后 2 个的 5 行 0% 意味着**默认实现从未被调用** —— 大概率是所有实现方都 override 了，
即**死默认实现**；处置是删掉或标注理由，不是"补测试"。

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

**第三条结论（2026-10-26 新增）：这三个 `feature` 关的文件，永远等不到 CI。**
`friends` / `widgets` / `burn-after-read` 只在 `Run library unit tests (--workspace --lib --all-features)`
那条车道里打开，而**那条车道不产生覆盖率**；覆盖率两条腿的 feature 集自 `56e4dd105`（2026-09-22）起
就没有它们。⇒ 无论 main push 多少次，读数都不会变。原判定把它们记成"陈旧"，等于给了一条**永远兑现
不了**的等待理由。

可行处置（**已实跑验证可行，只差覆盖率数字**）：把这三个 feature 加进 `STORAGE_FEATURES`。
理由：storage 腿是 `RUST_TEST_THREADS=1`，脚本头说的"friends/widgets 在多核/有限内存下触发并发 clone
失败与 OOM"针对的是 4 线程的 rest 腿，未必适用于单线程的 storage 腿。

实测（2026-10-26，隔离 `CARGO_TARGET_DIR=/tmp/tgt-featcheck`，不污染主 target）：

| 步骤 | 结果 |
|---|---|
| `cargo test -p synapse-storage --lib --no-run` + 那三个 feature | **编过**（15m39s，冷 target；`friends`/`widgets`/`burn-after-read` 与既有 10 个 feature 无冲突） |
| 同 feature 集跑 `-- --test-threads=1 friend_room`（真库） | **27 passed / 0 failed**（577s；本机未设 `TEST_DB_TEMPLATE_SCHEMA` ⇒ 每测试跑迁移，属最坏情况） |

⇒ "并发 clone 失败 / OOM"在这个 feature 集 + 单线程下**没有出现**。剩下的只有覆盖率数字本身
（`friend_room/repository.rs` 应从 `0.0` 变成 `LH>0`），那必须在 CI 的完整 lcov 里看。
⚠️ 别为此在本地重跑全量覆盖率（§7.5 末尾两次失败已记下成本）。

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
这是 §7.5 的 C 类之后"必须等 CI"的真正原因 —— 不是懒，是当前脚本的 flag 选择让局部回答不可能。

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

#### G. 只剩 8 个：`synapse-web/routes/**`

`admin/room/spaces`、`extractors/mod`、`federation/{media,membership/query,transaction/edus}`、
`handlers/room/management/{upgrade,visibility}`、`sticky_event`。

路由只能由 **integration 腿**覆盖，而 integration 全量在插桩下是 42 min+。它们最可能是"陈旧"
（路由会被集成测试打到）但不是必然 —— 本轮另一个会话刚给 r0/v1 匿名读口径加了守卫，
说明这类面还在动。**这 8 个是真的只能等 CI 的 diff**。

#### 本轮为此付出的两次失败（记下来省下一次的时间）

1. **`cargo llvm-cov --workspace --lib --all-features` 不可靠**：跑到 997 passed / **1 failed** 就整体退出，
   **lcov 一条都没落盘**（`cargo llvm-cov` 在测试失败时不生成报告，也没有 `--ignore-run-fail`）。
   失败的是 `synapse-common::test_isolation::tests::prune_backfills_legacy_zero_row_marker_and_spares_it`，
   而它**单独跑、单 crate 全量跑都通过**（998 passed / 7.16 s）。
2. 复现尝试（普通 `cargo test --workspace --lib --all-features`）跑了 **41 min 仍在 storage 的 DB 用例上**，
   被我中止 —— 这条复现路径本身太贵。

⇒ 所以"59 个 0%"这件事，正确做法仍是**让 CI 跑一次 main push**，本地不要重跑。

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

### 7.6 途中被证伪的十个判断（留着，防止后来者重走）

| 我说过 | 实测/核实结果 | 靠什么推翻 |
|---|---|---|
| "`friend_room/repository.rs`、`widget.rs`、`burn_after_read.rs` 的 0% 是基线陈旧（内联测试会执行本文件）" | **三个模块都被 feature 门控**（`friends` / `widgets` / `burn-after-read`），两条覆盖率腿的 feature 集都不含 ⇒ **文件根本不编译、不进报告**，0.0 是 bootstrap 值被 `max()` 冻结。三个里**没有一个**是陈旧 | 读 `synapse-storage/src/lib.rs:172/203/215` 的 `cfg` + `run_coverage.sh` 的 `STORAGE_FEATURES`/`REST_FEATURES` + 四份历史基线快照全为 0.0 |
| "22 个 storage `test_mocks` 是基线陈旧（不含 `test-utils` 的 feature 集下测得）"，**改判两次** | 第 1 次证伪：两腿都带 `test-utils` ⇒ 前提假。第 2 次收口：它们**根本不在门禁检查范围**（`TEST_ONLY_MARKERS` 含 `"test_mocks/"`，`:378` 直接 `continue`）⇒ 前两轮都在给"门禁不看的文件"写判定 | `cargo llvm-cov --help`（`--exclude` 同时排除测试与报告）+ 32 条基线条目 10 非零/22 零的双向对照 + 调用门禁自身 `is_test_only()` 计数 |
| **"基线里 59 个 0% 文件需要逐个分流"** | **分母错的**：59 里 22 个 `test_mocks/` + 5 个 `src/bin/*`/`main.rs` 被门禁自己的两张清单跳过，**真正的 0% 是 32 个** | 直接 `import` `check_file_coverage.py` 跑 `is_test_only()` 与 `_matches_prefix(non_unit_coverable)`，得 59 = 22 + 5 + 32 |
| "6 处 mock 高危，会让安全断言永远成立" | **0 处假绿**（2 死代码 / 2 mock 走不到 / 1 已声明偏差 / 1 无覆盖） | 逐个核调用链 + 读断言 |
| "静默期测试写好了"（`pause()` + 断言台账 `None`） | **假测试**：把静默期 300 s 改 10 s 仍 PASS | 变异自证 |
| "看板读到台账非空会误判正常" | 四个台账**没有任何生产读者**，是只写不读的死字段 | `grep` 全仓消费者 |
| "修第 1 层的参数就能接通族撤销" | 修完仍 `"Token has been revoked"` —— 第 0 层 `validate_token` 才是阻断点 | 改完直接跑那条测试 |
| "给 `PreviewUrlParams` 加 `deny_unknown_fields` 会让那条 `ts` 用例先红"（写在**测试注释**里） | **不会红**——`ts` 是已声明字段，不属于 unknown field；有效变异是 `ts` 改必填 | 变异自证实跑：`deny_unknown_fields` ⇒ PASS，`ts: i64` ⇒ `left: 400 / right: 200` |
| "`synapse-common/config/manager.rs` 是**最实的一个真缺口**，`ConfigManager` 是没人测的配置读写器，值得补 `tempdir` 测试" | **它不是"没人测"，是"没人用"**：全仓 `ConfigManager::new` **0 个构造点**（命中的全是 `RateLimitConfigManager`），`read_config/write_config` 的 `_location` 参数被**完全忽略**、没有任何文件 IO —— 一个从未接线的 `Arc<RwLock<Config>>` 壳。补测试等于给死代码刷覆盖率 | `grep -rn "ConfigManager::new"` 逐条看命中 + 读 81 行源码（无 `std::fs`/`Path`）+ 删除后 `cargo check` 两次全绿 |
| "`src/common/error.rs` 的 2 个转换 fn 是真缺口（小）" | **是死代码**：除了自身定义与 `common/mod.rs` 的 re-export，**零调用点**；`synapse-e2ee` 里那几个 `test_crypto_error_to_api_error_*` 测的是该 crate 自己的同名函数（跨 crate 不可见） | 全仓分别 `grep` 这两个函数名，命中只有定义 + re-export（详见 §7.4 的两条 grep） |

共同根因有三条：①**把"代码看起来像 X"当成"X 成立"** —— 写"高危 / 看板会 / 这条测试覆盖了"之前，
必须先 `grep` 消费者或做变异自证；②**自建口径而不复用门禁的判据** —— 统计"覆盖率缺口"时应当
直接调用门禁自己的 `is_test_only()` / `_matches_prefix()`，否则算出来的分母包含一堆门禁压根不看的文件
（本轮 59 vs 32 就是这么来的，白花了两轮判定）；③**把"0% 覆盖率"默认读成"缺测试"** ——
它也可能是"没有消费者"（`config/manager.rs` 零构造点）、"只有别的 crate 会调"（reader/writer 的转发层）、
或者"被 feature 关掉"。**先问"谁该跑它"，再决定"补测试还是删代码"**，否则就是给死代码刷覆盖率。

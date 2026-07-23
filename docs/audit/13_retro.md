# AUDIT-2026-07 项目回顾（Retro）

- 日期：2026-07-12
- 范围：7 阶段代码审核（docs/audit/00–07）→ 24 项计划优化 + 4 项架构强化（OPT-001~028）→ 性能复测（11）→ ship（PR #3）
- 规模：相对 `main` 共 **135 commits / 409 files**（+49,860 / −43,248），保留完整原子提交历史（不 squash）
- commit 构成：fix 88 · refactor 20 · feat 15 · chore 5 · perf 4 · test/style/docs 各 1
- 交付质量门：fmt ✓ · clippy --all-features -D warnings ✓ 0 warning · lib 481 ✓ · unit 868 ✓
- 本文回答 5 个回顾维度，产出写入 `docs/audit/13_retro.md`，并回填 project memory 与 MATRIX 优化方案文档。

---

## 维度 1：哪些审核发现最出乎意料？

按"本应早知道但没意识到"排序：

### 1.1 OIDC id_token 签名可绕过（OPT-001，CRITICAL）
`oidc_service.rs:400-418`：JWKS 无匹配 `kid` 或 fetch 失败时，**回退到 claim-only 校验**（只验 iss/aud/exp，不验签名）。OIDC 一旦启用，攻击者用未知 kid 或触发 JWKS fetch 失败，提交自填 iss/aud/exp 的伪造 id_token 即可登录任意账户——**认证绕过 / 账号接管**。

最出乎意料，是因为账户安全内核其余部分做得**超标**：Argon2 m=64MiB/t=3 超 OWASP、access token 实时吊销无 TTL 窗口、refresh token 单次+重用检测触发全家族吊销、管理员 HMAC 恒定时间+一次性 nonce。一个把这些最容易做错的地方全做对了的团队，却在一条"依赖失败时降级"的路径上放行了签名绕过。**根因不是不懂安全，是 fail-open 的默认心智**（见维度 3）。

### 1.2 healthcheck 回退掩盖 DB 故障，且违反已有硬约束（OPT-003/028，HIGH）
`healthcheck.rs:22-29` + `docker-compose.yml:53`：探测列表 `["/health","/_matrix/client/versions",...]` **首个成功即 exit 0**。DB 宕机时 `/health` 返 503，但 `/versions` 返 200 → 容器仍报健康、编排器不重启、静默不可用。

出乎意料点：项目**已有**"healthcheck 必须真查 DB"的硬约束，但代码违反了它。约束存在却没有测试或 lint 兜底，等于没有。

### 1.3 联邦端点存在性泄露是一整类问题（OPT-017，P0）
send_join/leave/invite/member-list/event-observe 对"房间/事件存在但无权"与"不存在"返回**不同状态码**，可被枚举。不是单点疏漏，而是横跨多个联邦入口的统一模式——需要一次性收口为 404，而非逐个打补丁。

### 1.4 18 个可空列被当作非空 i64（OPT-013 a~r）
数据模型层 18 个 nullable 列在 Rust 侧类型为 `i64` 而非 `Option<i64>`。编译通过、大多数时候能跑，直到真出现 NULL。潜伏性正确性缺口，静态审读才暴露，运行测试不一定触发。这项还顺带暴露了 `clippy 跳过 cfg(test) 模块` 的门禁盲区（见维度 3.2）。

---

## 维度 2：哪些优化任务的成本/收益比最差？

### 2.1 性能复测（阶段 5，docs 05 + 11）——收益最低
方法：生产级数据量镜像表 + `EXPLAIN (ANALYZE, BUFFERS)`。7 个查询形状全部**亚毫秒级**，运行间抖动（0.01–0.15ms）**超过任何真实信号**。诚实结论只能是"持平，在噪声内"，**不能主张任何提速**。

成本：写 seed 脚本、建带热点索引 DDL 的镜像表、写 EXPLAIN 探针、基线+复测两轮。收益：唯一站得住的结论是"命中索引 / 0 seq scan"——而这个结论，一次静态索引审读就能更便宜地得到。真正有价值的维度（p50/p95/p99、QPS、缓存命中率、内存）**两轮都是 NOT_CAPTURED**，因为没有运行中的服务器 + 播种 harness。**这一阶段反复为一个它拿不出的结果付费。**

教训：性能验证要么先立起"运行中服务器 + 播种脚本"，要么一开始就把范围显式限定为"索引计划核对"，不要中途才发现微基准无法主张提速（见维度 5）。

### 2.2 OPT-013（18 子任务）——必要但即时回报低
18 列机械式改造面广、评审负担重，换来的是"大多潜伏"的正确性。必须做，但即时收益低，且是它把 `--all-features` 编译回归的门禁盲区拉了出来（build 一度因 god-file 拆分丢 chrono import 而红）。

---

## 维度 3：哪些问题是反复出现的根因？（值得写入硬约束）

### 3.1 Fail-open 作为默认（最高价值的一条）
反复出现在**安全相关的失败路径**上，每次都选了"可用性优先于安全"：
- OIDC JWKS 失败 → claim-only 放行（#1 CRITICAL）
- healthcheck `/health` 失败 → `/versions` 兜底报健康（#3 HIGH）
- rate limiter 出错 → `fail_open_on_error: true` 放行（#5 MEDIUM）
- 联邦签名私钥 `signing_key_master_key` 未设 → 明文入库（#6 MEDIUM）
- SAML 两个 `want_*_signed` 皆 false → 跳过验签（#8 MEDIUM）

→ **硬约束：安全相关的失败必须 fail-closed。** 认证/联邦/健康/限流路径中，依赖或校验失败时默认拒绝，不得降级放行；任何 fallback/`unwrap_or`/`default-on-error` 出现在这些路径都需显式安全评审。

### 3.2 Feature-gated / cfg(test) 代码逃逸默认门禁
- god-file 拆分丢 chrono import，只在 `--all-features` 下可见（feature-gated 模块不在默认 workspace 编译）。
- clippy **跳过 cfg(test) 模块** → i64→Option<i64> 结构体字段迁移的编译错误直到 `cargo test --no-run` 才暴露。
- `login_flows_v3` 快照在 `--all-features` 下多出 cas/sso 流，与默认 feature 集生成的 committed 基线冲突。

→ **硬约束：声明"绿"之前，fmt / clippy / `cargo test --no-run` 必须在 `--all-features` 下各跑一遍。** cfg(test) 与 feature-gated 代码是默认门禁的盲区。

### 3.3 测试中的进程级可变状态
`TRUST_FORWARDED_HEADERS`（`AtomicBool`）在 `RUST_TEST_SHUFFLE=1` + 4 线程下互相泄露 → CSRF 测试假失败。
→ **硬约束：改动进程级全局状态的测试必须用模块级 Mutex 串行化**（本次修复即 `FORWARDED_TRUST_LOCK`）。

### 3.4 共享 public-schema 测试池争用
`require_test_pool` 共享池在 4 线程下并发争用 → `admin_registration` 假失败（单线程隔离 24/24 全过）。
→ **硬约束：共享池 / destructive 测试需隔离 schema 或 `SerialGuard`；CI 假失败先按环境排查（迁移到 head + 隔离库）再判代码。**

### 3.5 联邦存在性泄露
见 1.3。→ **硬约束：联邦端点对"存在但无权"与"不存在"统一返 404。**

---

## 维度 4：哪些 GStack/Superpowers 技能最有效？哪些不适用 Rust？

### 最有效
- **systematic-debugging（Iron Law：先根因后修复）**——直接价值。CSRF flake 没有被"重试掩盖"，而是定位到全局状态泄露的真根因再修；并正确把 `admin_registration` 争用、`login_flows` 快照判为环境/预存在而非"修掉它"。这一条阻止了两类假修复。
- **writing-plans / executing-plans**——28 个 OPT 原子任务分解（每个 RED-GREEN-REFACTOR + atomic commit）与项目 tdd-rust 工作流严丝合缝。任务粒度（2–5 分钟、独立可提交）是 135 个干净 commit 的直接来源。
- **/cso 安全审计**——产出了最高价值发现（CRITICAL OIDC 绕过），带 file:line 证据 + pre-emit verification gate（每个 CRITICAL/HIGH 亲读源码二次核实），几乎无误报。
- **/benchmark 的"measure don't guess"纪律**——价值恰恰在于它**逼出 NOT_CAPTURED 的诚实**，阻止了编造 p50/p95/QPS 数字。技能本身在 Rust DB 层收益有限（见下），但它的诚实约束是净正。

### 不太适用 Rust / 需要改造
- **/benchmark 的 Web 取向**——Core Web Vitals / Lighthouse / bundle-size 是 JS/前端心智，对无 UI 的 Rust homeserver 不适用。我把它重定向成 `EXPLAIN ANALYZE`，但技能默认框架不匹配。
- **/qa 的浏览器测试**——"网站能用吗 / 健康分"假设有运行中的 Web UI。无头 Rust 服务器 + 无播种服务器，导致 live 维度整体不可测，只能显式声明"无法测"。
- **insta 快照 × Cargo feature flags**——通用技能假设快照稳定，但 feature-gated 响应形状（cas/sso）打破了"默认 vs --all-features"的快照一致性假设，是 Rust 特有的坑，技能没有预案。
- **gstack preamble 机制**（telemetry 提示 / gbrain / conductor 检测 / artifacts sync）——每次调用的固定开销，与 Rust 工作正交。

---

## 维度 5：下次做类似审核，流程怎么调整？

1. **先立隔离的 head-migrated 库，再跑任何测试门禁。** 本次陈旧库导致数百个假失败、吃掉真实时间。把"provision fresh head DB"写进审核 runbook 的第 0 步。
2. **所有门禁一开始就在 default 与 `--all-features` 各跑一遍**（fmt / clippy / `cargo test --no-run`）。feature-gated 与 cfg(test) 代码否则不可见。
3. **性能范围先决策，别中途发现。** 二选一：(a) 先起运行中服务器 + 播种脚本，拿真 p50/p95/QPS/内存；(b) 显式限定为"索引计划核对"，不投资无法主张提速的镜像表微基准。不要两轮都为 NOT_CAPTURED 付费。
4. **把"fail-open 扫描"作为一个独立审核视角前置。** 对 auth/federation/health/rate-limit 路径 grep `fallback` / `unwrap_or` / `default-on-error`——本次最高严重度的发现全来自这里。
5. **保留 /cso 的 pre-emit verification gate**（每个 CRITICAL/HIGH 亲读源码）——它把误报压到接近零。
6. **机械式批量迁移（如 OPT-013 18 列）显式挂上编译门禁步骤**（`cargo test --no-run --all-features`），不要假设 clippy 会覆盖 cfg(test)。

---

## 一句话总结

安全内核扎实的项目，最大的洞不在"不会做"，而在"失败时选择了放行"——**fail-open 是本次审核的头号根因**。流程上最大的浪费是"没有运行中服务器却反复尝试跑性能基准"；最大的杠杆是"systematic-debugging 的先根因后修复"与"/cso 的亲读源码验证门禁"。

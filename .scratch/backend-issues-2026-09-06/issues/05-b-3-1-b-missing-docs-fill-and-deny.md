# 05: B-3.1-b missing docs 集中补完 + 切换 `#![deny(missing_docs)]`

**What to build:** 借助 04 提供的 ratchet 度量，集中补完 7 个 crate 的 missing docs（目标：missing 数 ≤ 10，每个 crate 的"public API 但无文档"项均闭合），然后**分阶段切换门禁**：先把 `lib.rs:4` 的 `B2-TODO` 改为 `#![warn(missing_docs)]`（让 ratchet 进入强制阶段），跑一周无 regression 后切 `#![deny(missing_docs)]`。

**Blocked by:** 04 (B-3.1-a ratchet 度量脚本就位) — ✅ 已完成

**Status:** phase-2-done (synapse-cache crate fully documented; per-crate cargo doc shows **0 missing docs in synapse-cache**; baseline 14,147 → 13,833; ratchet 绿)

**Spec reference:**
- synapse 直接 `#![deny(missing_docs)]`——本仓 05 的最终目标
- ratchet 策略：每 PR 必须不增加 missing 计数；每周减少 ≥ 5 个

**现状（已调研）:**
- 2026-09-06 实测（`scripts/quality/audit_per_crate_missing_docs.sh`）：
  - `synapse-storage`  — **5,587 missing**（最大块，storage trait 方法缺文档）
  - `synapse-services` — **3,358 missing**（service 层 method）
  - `synapse-rust`     — **2,392 missing**（root crate re-export 包装）
  - `synapse-e2ee`     — **1,151 missing**（olm/megolm 公开 API）
  - `synapse-common`   — **1,007 missing**（工具函数）
  - `synapse-federation` — **331 missing**
  - `synapse-cache`    — **0 missing**（✅ 本 PR 完成全清；workspace 冷构建后 ratchet 重计为 0）
  - **合计：13,833 missing docs**（ticket 估计 80-150，超 92 倍）

**Phase 1 已完成（commit 7f597734，2026-09-07）:**
- ✅ 7 个 crate 全部从 `allow(missing_docs)` 切到 `warn(missing_docs)`
- ✅ Ratchet baseline 移到 `scripts/quality/missing-docs-baseline.txt`（git tracked）
- ✅ Baseline 初始化为 14,147（真实数字，ratchet 现在有意义）
- ✅ synapse-cache/src/circuit_breaker.rs：补 23 项（`CircuitState` 3 变体 + 9 个公开方法 + `CircuitBreakerMetrics` 8 字段 + `SlidingWindow` 3 字段 + `CircuitBreakerMetricsHandle` 5 字段）
- ✅ synapse-cache/src/strategy.rs：补 16 项（`CacheKeyBuilder` 18 个 builder + `CacheTtl` 14 个 accessor + 2 个 struct doc）
- ✅ 7 个 cache 子模块的 test fn 批量加 `#[allow(missing_docs)]`（避免 test helper 污染 ratchet 计数）
- ✅ Ratchet 实测：14,147 → 14,108（-39），自动更新 baseline，CI 绿
- ⚠️ synapse-cache 还剩 264 项（lib.rs 97、query_cache 70、invalidation 50、federation_signature_cache 47）— 见 B-3.1-b-1 子 ticket

**Phase 2 已完成（本会话，2026-09-07）:**
- ✅ B-3.1-b-1（synapse-cache 全部清零）— `cargo doc -p synapse-cache` 报 0 missing
  - synapse-cache/src/lib.rs：补 60+ CacheManager 方法、RedisCache 全部 method、compression 模块 5 fn、`CacheError` 6 变体、`RateLimitDecision` 3 字段、crate-level `//!`
  - synapse-cache/src/query_cache.rs：补 70 项（`CacheEntry<T>`, `QueryCacheConfig`, `CacheStats`, `CacheWarmupStrategy` 3 变体, `CacheWarmupConfig`, `QueryCache` + ~40 methods）
  - synapse-cache/src/invalidation.rs：补 50 项（`InvalidationType` 4 变体、`CacheInvalidationMessage`、`CacheInvalidationConfig`、`CacheInvalidationBroadcaster`、subscriber/manager 方法）
  - synapse-cache/src/federation_signature_cache.rs：补 47 项（`SignatureCacheConfig`、`CacheEntryKey`、`SignatureCacheEntry`、`KeyRotationEvent`、`FederationSignatureCache` ~18 methods、`SignatureCacheStats`）；同时**修复了上一轮误改的重复 `from_federation_config` 函数定义**（compile error）
  - synapse-cache/src/circuit_breaker.rs：补 `CircuitBreakerMetrics` 8 字段、`CircuitBreaker` struct doc、`record_failure/record_timeout/current_state/get_metrics/reset` 方法
- ✅ `cargo test -p synapse-cache --all-features` → 110 passed, 0 failed
- ✅ Ratchet 重对齐：14,108 → 13,833（-275 = workspace 累计减少）；baseline 自动更新为 13,833
- ✅ 后续清理：子 ticket B-3.1-b-1（synapse-cache）**已通过** — 准备切 `deny(missing_docs)`

**实现计划（acceptance criteria）:**
- [x] 跑 04 的 ratchet 脚本，记录当前 missing docs baseline N
- [x] 7 个 crate `lib.rs:4` 切 `#![warn(missing_docs)]` —— **phase 1 完成**
- [x] `synapse-cache` crate 内零 missing docs —— **phase 2 完成（B-3.1-b-1 ✅）**
- [ ] `synapse-cache` 切 `#![deny(missing_docs)]` —— B-3.1-b-1 下一步（**等本 PR 合并后立刻推进**）
- [ ] 优先级顺序补文档（按公开度 + 复用频率）：
  - [x] `synapse-cache/src/circuit_breaker.rs` (23/23) — phase 1
  - [x] `synapse-cache/src/strategy.rs` (16/35) — phase 1
  - [x] `synapse-cache/src/lib.rs` (97/97) — phase 2
  - [x] `synapse-cache/src/query_cache.rs` (70/70) — phase 2
  - [x] `synapse-cache/src/invalidation.rs` (50/50) — phase 2
  - [x] `synapse-cache/src/federation_signature_cache.rs` (47/47) — phase 2
  - [ ] `synapse-common` 1007 项 → B-3.1-b-2
  - [ ] `synapse-e2ee` 1151 项 → B-3.1-b-3
  - [ ] `synapse-federation` 331 项 → B-3.1-b-4
  - [ ] `synapse-services` 3358 项 → B-3.1-b-5
  - [ ] `synapse-storage` 5587 项 → B-3.1-b-6
  - [ ] `synapse-rust` (root) 2392 项 → B-3.1-b-7
- [ ] 每补完一个 crate，run 04 脚本验证 missing 计数下降，ratchet 自动更新 baseline
- [ ] missing ≤ 10 时：7 个 crate 逐个切 `#![deny(missing_docs)]`

**风险/边界:**
- 风险无：纯文档工作，不改业务逻辑
- 边界：补文档过程必须**始终 CI 绿**（ratchet 法）
- 边界：补文档不是机械的 `/// xxx`——需要简短说明 purpose + 关键参数语义
- 边界：deny 阶段**必须确认所有 public item 已文档**——否则 deny 直接让 build 失败
- 边界：deny 阶段要分 crate 逐个开（避免一次性爆 50+ 编译错误）

**工作量重新评估:**
- 原估算 3-5d（基于 ~80-150 missing docs 的错误假设）
- 实际 14,148 missing docs → 按 1-2 分钟/项 ≈ **236-473 小时 ≈ 30-60 人天**
- **必须团队分解执行**（见下）
- 建议拆分为 5 个子 ticket（每个 ~2-3 人周）：B-3.1-b-1..5

**建议策略:** **拆分为 7 个子 ticket 团队并行执行**（按 ratchet 子集重排）：
- B-3.1-b-1: **synapse-cache 剩余 264 项**（lib.rs 97 + query_cache 70 + invalidation 50 + federation_signature_cache 47） ≈ 9 人天（小团队 1）— **phase 1 已完成 23+16=39，余 264 待补**
- B-3.1-b-2: **synapse-common 1007 项** ≈ 17 人天（小团队 1）— phase 1 启动 ratchet 时已加 `warn`，但尚未补任何项
- B-3.1-b-3: **synapse-e2ee 1151 项** ≈ 19 人天（小团队 2）
- B-3.1-b-4: **synapse-federation 331 项** ≈ 6 人天（小团队 1）— 与 cache 合计 11 人天
- B-3.1-b-5: **synapse-services 3358 项** ≈ 56 人天（小团队 4）
- B-3.1-b-6: **synapse-storage 5587 项** ≈ 93 人天（小团队 3，最大块）
- B-3.1-b-7: **synapse-rust root 2392 项** ≈ 40 人天（小团队 5）

所有子 ticket 完成后，再分 crate 逐个切换 `#![warn(missing_docs)]` → `#![deny(missing_docs)]`（避免一次性爆编译错误）。

**Workflow 说明（供 sub-ticket 团队参考）:**
- 跑 `bash scripts/quality/audit_per_crate_missing_docs.sh` 摸底本 crate 当前 missing 数
- 跑 `bash scripts/quality/check_missing_docs_ratchet.sh -p <crate>` 验证 ratchet 绿
- 补文档时优先顺序：pub struct / pub enum / pub trait → 它们的 pub method → pub fn 自由函数
- 每个子 ticket 完成后 commit 信息应含 `B-3.1-b-N: ...` 前缀，ratchet 会在 commit 后自动减少计数
- 不要在 `mod tests` 内的 fn 上手动加 `///`，应该用 `#[allow(missing_docs)]`（避免与 rustdoc 的"missing docs for an associated function" warning 反复纠缠）
- `deny` 阶段一定不要在 first PR 一次性全开——按 `synapse-cache` → `synapse-federation` → `synapse-common` → `synapse-e2ee` → `synapse-services` → `synapse-storage` → `synapse-rust` 顺序分 7 个 PR 逐步切换

# 05: B-3.1-b missing docs 集中补完 + 切换 `#![deny(missing_docs)]`

**What to build:** 借助 04 提供的 ratchet 度量，集中补完 7 个 crate 的 missing docs（目标：missing 数 ≤ 10，每个 crate 的"public API 但无文档"项均闭合），然后**分阶段切换门禁**：先把 `lib.rs:4` 的 `B2-TODO` 改为 `#![warn(missing_docs)]`（让 ratchet 进入强制阶段），跑一周无 regression 后切 `#![deny(missing_docs)]`。

**Blocked by:** 04 (B-3.1-a ratchet 度量脚本就位)

**Status:** ready-for-agent

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
  - `synapse-cache`    — **322 missing**（最小块，最适合优先完成）
  - **合计：14,148 missing docs**（ticket 估计 80-150，超 94 倍）
- 所有 crate 仍为 `#![allow(missing_docs)]`（未切换 lint 等级）

**实现计划（acceptance criteria）:**
- [ ] 跑 04 的 ratchet 脚本，记录当前 missing docs baseline N
- [ ] 优先级顺序补文档（按公开度 + 复用频率）：
  - [ ] `synapse-common`（小、依赖底层）—— 全部 public fn 加 doc
  - [ ] `synapse-cache`（小）—— 全部 public fn 加 doc
  - [ ] `synapse-storage` trait 方法
  - [ ] `synapse-e2ee` olm/megolm 公开 API
  - [ ] `synapse-federation` federation client + event handling
  - [ ] `synapse-services` service 层 method
  - [ ] `src/lib.rs` handler 模块
- [ ] 每补完一个 crate，run 04 脚本验证 missing 计数下降，更新 baseline
- [ ] missing ≤ 10 时：7 个 crate `lib.rs:4` 的 `B2-TODO` 改为 `#![warn(missing_docs)]`
- [ ] warn 跑一周（独立 PR 监控 ratchet 无 regression）
- [ ] 切 `#![deny(missing_docs)]` —— **最终目标**

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

**建议策略:** **拆分为 5 个子 ticket 团队并行执行**：
- B-3.1-b-1: synapse-cache (322) + synapse-federation (331) ≈ 11 人天（小团队 1）
- B-3.1-b-2: synapse-common (1007) + synapse-e2ee (1151) ≈ 36 人天（小团队 2）
- B-3.1-b-3: synapse-storage (5587) ≈ 93 人天（小团队 3，最大块）
- B-3.1-b-4: synapse-services (3358) ≈ 56 人天（小团队 4）
- B-3.1-b-5: synapse-rust root (2392) ≈ 40 人天（小团队 5）

所有子 ticket 完成后，再分 crate 逐个切换 `#![warn(missing_docs)]` → `#![deny(missing_docs)]`（避免一次性爆编译错误）。

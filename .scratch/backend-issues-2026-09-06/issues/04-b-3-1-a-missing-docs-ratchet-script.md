# 04: B-3.1-a 缺文档 ratchet 度量脚本

**What to build:** 添加 `scripts/check_missing_docs_ratchet.sh`，跑 `cargo doc --no-deps --all-features` 解析 `warning: missing documentation` 计数，写入 `.workbuddy/memory/missing-docs-baseline.txt`，并在 CI / pre-commit 中**禁止** missing 数增加。这是 B-3.1 文档门禁切换（`#![warn(missing_docs)]` → `#![deny(missing_docs)]`）的**expand 阶段**——先建立度量，再做补完（05）。

**Blocked by:** None (can start immediately)

**Status:** ready-for-agent

**Spec reference:**
- synapse 直接 `#![deny(missing_docs)]`——**强门禁**
- 本仓采用 **ratchet 法**（"只许进不许退"），分阶段从 warn → deny

**现状（已调研）:**
- 7 个 crate 的 `lib.rs:4` 都有 `// B2-TODO: Change to #![warn(missing_docs)] once sub-modules have docs.`
- 没有现成的 `cargo doc` 缺文档度量脚本
- 项目 scripts 目录已有 `scripts/quality/format_check.sh` 等同类脚本（参照其风格）
- 配置文件：`scripts/quality/format_check.sh` 走 `git ls-files` 而非 `rglob`（commit d8d72d9d 起）—— 保持一致

**实现计划（acceptance criteria）:**
- [ ] `scripts/check_missing_docs_ratchet.sh` 存在并可执行（`chmod +x`），结构：
  - 跑 `cargo doc --no-deps --workspace --all-features 2>&1 | grep -c "warning: missing documentation"`
  - 读 `.workbuddy/memory/missing-docs-baseline.txt` 取上次值
  - 当前 > 上次 → 退出码 1 + 报错 "missing docs regressed: X → Y"
  - 当前 ≤ 上次 → 退出码 0 + 输出 "missing docs: X (no regression)"
  - 当前 == 0 → 输出 "✓ ready for #![deny(missing_docs)]"
- [ ] `.workbuddy/memory/missing-docs-baseline.txt` 初值：通过脚本第一次运行自动生成
- [ ] 7 个 crate `lib.rs:4` 注释更新：把 `B2-TODO: Change to #![warn(missing_docs)]` 改为 `B2-TODO: ratchet in progress — see scripts/check_missing_docs_ratchet.sh`
- [ ] 添加到 `scripts/quality/` README：怎么本地跑、baseline 文件怎么更新
- [ ] (可选) 添加到 pre-commit hook：`scripts/install-hooks.sh` 注册此脚本

**风险/边界:**
- 风险低：纯脚本工作，不改业务代码
- 边界：`cargo doc --all-features` 编译时间长（约 5-10 分钟），本地只跑增量（仅改动的 crate 跑）
- 边界：baseline 文件**必须 commit 到 git**——让 CI 跑时能比较
- 边界：CI 跑全量 `cargo doc` 慢——可以只在 main 分支跑全量，PR 跑 diff

**工作量:** 0.3d

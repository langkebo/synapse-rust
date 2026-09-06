# Per-Crate Missing Docs Audit (B-3.1-b Step 1)

执行日期：2026-09-06
执行脚本：`scripts/quality/audit_per_crate_missing_docs.sh`
方法：临时切换 #![allow(missing_docs)] → #![warn(missing_docs)]，跑 `cargo doc --no-deps -p <crate> --all-features`，grep 统计。

## 真实 missing docs 数量

| Crate            | Missing Docs | 估算比例 |
|-----------------|-------------|---------|
| synapse-storage  | 5,587      | 39.5%   |
| synapse-services | 3,358      | 23.7%   |
| synapse-rust     | 2,392      | 16.9%   |
| synapse-e2ee     | 1,151      | 8.1%    |
| synapse-common   | 1,007      | 7.1%    |
| synapse-federation | 331      | 2.3%    |
| synapse-cache    | 322        | 2.3%    |
| **合计**        | **14,148** | 100%    |

## 结论

- ticket 05 原始估算 ~80-150 missing docs，**实际 14148，超 94 倍**
- 按平均 1-2 分钟/项补一个 doc，需要 236-473 小时（≈30-60 人天）
- **不适合单人完成，必须团队分解**
- 最优先补完的小 crate：`synapse-cache`（322）+ `synapse-federation`（331）= 653 项（≈ 11 人天）
- 其次：`synapse-common`（1007）+ `synapse-e2ee`（1151）= 2158 项（≈ 36 人天）
- 最大两块：`synapse-storage`（5587）+ `synapse-services`（3358）= 8945 项（≈ 149 人天）

## 推荐团队分解方案

| 子 ticket | Crate(s) | Missing | 建议负责人 |
|-----------|----------|---------|-----------|
| B-3.1-b-1 | synapse-cache + synapse-federation | 653 | 小团队 1 |
| B-3.1-b-2 | synapse-common + synapse-e2ee | 2158 | 小团队 2 |
| B-3.1-b-3 | synapse-storage | 5587 | 小团队 3（最大块） |
| B-3.1-b-4 | synapse-services | 3358 | 小团队 4 |
| B-3.1-b-5 | synapse-rust (root) | 2392 | 小团队 5 或分配到各子 ticket |

所有子 ticket 完成后，才统一切换 `#![deny(missing_docs)]`（分 crate 逐个开，避免同时爆编译错误）。

## 当前 lint 状态

所有 crate 仍为 `#![allow(missing_docs)]`（未切换 lint 等级）。
切换路径：allow → warn → deny（每阶段在 CI 中验证无 regression）。

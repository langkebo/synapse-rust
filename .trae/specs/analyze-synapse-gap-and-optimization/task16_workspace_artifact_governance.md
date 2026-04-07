# Task 16 - 工作区产物治理规则

## 1. 分类模型

| 分类 | 说明 | 是否允许进主干 | 存放位置 |
| --- | --- | --- | --- |
| 源码必审 | Rust 源码、迁移、长期脚本 | 是 | `src/`, `migrations/`, `scripts/` |
| 文档必审 | 长期有效规范、方案、runbook | 是 | `docs/`, `.trae/specs/...` |
| 可生成报告 | 可重复生成的验证摘要与 coverage 结果 | 视情况 | `artifacts/` |
| 临时产物 | 单次调试日志、媒体样本、时间戳运行目录 | 否 | `test-results/`, 本地临时目录 |

## 2. 文档治理

- 长期基线文档与日期化执行报告分层存放。
- 日期化报告默认进入 `archive/` 或报告目录，不与长期规范并排作为当前事实源。
- 每个主题保留 1 份 current 文档，其他历史报告通过索引链接归档。

### 2.1 单一事实源索引要求

- 本目录的 `document-index.md` 必须作为“当前事实源入口索引”，列出 P1/P2 文档与使用优先级。
- 本目录的 `README.md` 必须链接到 `document-index.md`、`tasks.md`、`checklist.md`，并避免把阶段性报告当作唯一结论来源。
- 若新增/移动正式文档，必须同步更新 `document-index.md`（否则视为未完成交付）。

## 3. `artifacts/` 规则

- 只保留对外可复用或评审必需的摘要产物。
- 时间戳型运行结果进入 `artifacts/<topic>/runs/<date>/`，默认不直接进入主干。
- 若产物内容可由脚本重建，应优先只保留脚本和摘要，而不是全量明细。

## 4. `test-results/` 规则

- 仅作为运行期目录，不作为长期事实源。
- 根目录只保留 `latest` 语义文件或最近一次失败的关键摘要。
- 历史运行目录设置 TTL，默认仅保留最近 N 次。
- 二进制测试媒体不得长期留在根目录。

落地工具：
- `scripts/cleanup_test_results.py --dir test-results --keep 5` 可用于清理历史运行目录（支持 `--dry-run`）。

## 5. CI 产物策略

| 产物 | 成功时 | 失败时 | 保留期 |
| --- | --- | --- | --- |
| 测试摘要 | 上传精简 summary | 上传完整 summary | 7-14 天 |
| 详细日志 | 默认不上传 | 上传失败相关日志 | 7 天 |
| schema diff / contract diff | 默认不上传 | 上传 | 14 天 |
| 大体积媒体样本 | 不上传 | 仅必要时上传 | 最短可用期 |

落地说明（已接线）：
- `DB Migration Gate`：Gate 0/1/2 JSON 报告、schema alignment 报告与 expected/actual schema dump、amcheck、logical checksum 均已设置 `retention-days: 14`，并默认仅在失败时上传（logical checksum 作为诊断摘要保留上传）。
- `CI`：coverage report 与 quality evidence 已设置 `retention-days: 14`。
- `Schema Drift Detection`：仅在检测到 blocker drift 时上传 drift report（保留期 30 天），避免常态产物噪音。

## 6. 团队执行约定

- 新增日期化报告时，必须同步更新索引或归档入口。
- 禁止把 `test-results/` 路径当作长期文档的唯一证据。
- 若 CI 新增产物上传，必须说明诊断价值、大小和保留期。
- 临时报表合入前需判断是否应沉淀为长期规范，否则不得进入主干。

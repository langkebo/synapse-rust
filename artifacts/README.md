# artifacts/

本目录用于存放可重复生成的验证产物与 CI 下载产物（例如 schema diff、contract coverage、logical checksum、amcheck 输出、临时 sqlx migration source）。

约定：
- 本目录内容默认不进入主干；需要时由脚本/CI 生成并上传为 workflow artifacts。
- 时间戳型运行结果建议落在 `artifacts/<topic>/runs/<date>/`。
- 若确需在仓库内保留“长期可复用摘要”，应迁移到 `docs/` 或 `.trae/specs/...` 并在索引中标注。


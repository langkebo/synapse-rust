# hotfix

该目录用于存放紧急修复类迁移脚本：

- 命名：`YYYYMMDDHHMMSS_description.hotfix.sql`
- 约束：hotfix 必须在下一次常规发布前合并为正式迁移并从该目录收敛

当前目录作为治理入口存在，具体纳入发布流程的方式由 `docs/db/MIGRATION_GOVERNANCE.md` 维护。

# 数据库 Schema 说明

> 最后更新: 2026-03-21

## Schema 文件说明

### 主 Schema 文件

| 文件 | 说明 | 状态 |
|------|------|------|
| `00000000_unified_schema_v6.sql` | 基础 Schema 定义（新环境唯一建库基线） | ✅ 活跃 |
| `99999999_unified_incremental_migration.sql` | 历史综合增量兼容资产 | ⚠️ 保留 |
| `archive/schema_legacy.sql` | 旧版基础定义 | ❌ 已废弃 |

### 使用方法

#### 首次部署

```bash
# 由项目运维入口统一执行
bash docker/db_migrate.sh migrate
```

#### 推荐的升级方式

```bash
# 方式一：在仓库根目录执行统一运维入口
bash docker/db_migrate.sh migrate

# 方式二：在容器内通过相同迁移资产执行
docker exec -i synapse-rust-app bash /app/scripts/db_migrate.sh migrate
```

#### 治理口径

- `db-migration-gate.yml` 是唯一迁移治理门禁
- `ci.yml` 保留通用测试与 `sqlx migrate run` 初始化，不承担迁移治理口径定义
- `99999999_unified_incremental_migration.sql` 仅作为历史兼容资产保留，不再作为推荐部署入口
- 迁移命名与目录模型以 `MIGRATION_INDEX.md` 为唯一规范源
- Rust 运行时数据库初始化默认关闭，只有显式设置 `SYNAPSE_ENABLE_RUNTIME_DB_INIT=true` 时才允许进入兼容路径

### Schema 变更历史

- **v1.0.0** (2026-03-20): 初始版本
- **v1.0.1** (2026-03-21): 添加字段重命名迁移
  - `user_threepids.validated_at` → `validated_ts`
  - `user_threepids.verification_expires_at` → `verification_expires_ts`
  - `private_messages.read_at` → `read_ts`

### 字段命名规范

时间字段命名以项目规则和 `DATABASE_FIELD_STANDARDS.md` 为准：
- 必填毫秒时间戳使用 `*_ts`
- 可选时间戳优先使用 `*_at`
- 发生冲突时，以 Rust 模型和实际迁移文件为单一真实来源

### 启动时验证

服务启动时会自动执行 Schema 健康检查：
- 检查核心表是否存在
- 检查核心字段是否完整
- 检查必需索引是否存在
- 自动修复缺失的索引

日志输出示例：
```
INFO: Running database schema health check...
INFO: ✅ Database schema validation PASSED
```

### 编译期验证

项目支持使用 SQLx 宏进行编译时 SQL 验证：

```rust
// 编译时验证列名
let user = sqlx::query_as!(
    UserRow,
    "SELECT user_id, username, creation_ts FROM users WHERE user_id = $1",
    user_id
)
.fetch_one(&pool)
.await?;
```

启用方法：在 Cargo.toml 中确保 sqlx 启用了 `macros` feature。

### 相关文档

- `MIGRATION_INDEX.md`
- `../docs/db/MIGRATION_GOVERNANCE.md`
- `../docs/ROLLBACK_RUNBOOK.md`

# 数据库 Schema 说明

> 最后更新: 2026-03-21

## Schema 文件说明

### 主 Schema 文件

| 文件 | 说明 | 状态 |
|------|------|------|
| `UNIFIED_MIGRATION_v1.sql` | 综合迁移脚本（推荐） | ✅ 活跃 |
| `00000000_unified_schema_v6.sql` | 基础 Schema 定义 | ⚠️ 兼容 |
| `archive/schema_legacy.sql` | 旧版基础定义 | ❌ 已废弃 |

### 使用方法

#### 首次部署

```bash
# 1. 执行基础 Schema
psql -U synapse -d synapse -f 00000000_unified_schema_v6.sql

# 2. 执行迁移脚本
psql -U synapse -d synapse -f UNIFIED_MIGRATION_v1.sql
```

#### 推荐的部署顺序

```bash
# 方式一：使用 psql 直接执行
psql -U synapse -d synapse -f UNIFIED_MIGRATION_v1.sql

# 方式二：使用 Docker
docker exec -i synapse-rust-db psql -U synapse -d synapse < migrations/UNIFIED_MIGRATION_v1.sql
```

### Schema 变更历史

- **v1.0.0** (2026-03-20): 初始版本
- **v1.0.1** (2026-03-21): 添加字段重命名迁移
  - `user_threepids.validated_at` → `validated_ts`
  - `user_threepids.verification_expires_at` → `verification_expires_ts`
  - `private_messages.read_at` → `read_ts`

### 字段命名规范

所有时间戳字段必须使用 `*_ts` 命名：
- `created_ts` - 创建时间（毫秒）
- `updated_ts` - 更新时间（毫秒）
- `joined_ts` - 加入时间
- `left_ts` - 离开时间
- `last_active_ts` - 最后活跃时间
- `validated_ts` - 验证时间
- `verification_expires_ts` - 验证过期时间
- `read_ts` - 阅读时间

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

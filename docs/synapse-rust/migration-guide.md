# 数据迁移指南

> **版本**：1.0.0  
> **创建日期**：2026-01-28  
> **项目状态**：开发中  
> **参考文档**：[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[Matrix 规范](https://spec.matrix.org/)

---

## 一、数据库迁移策略

### 1.1 迁移概述

Synapse Rust 项目使用 SQLx 进行数据库操作，支持编译时 SQL 检查。数据库迁移用于管理数据库 schema 的版本控制和升级。

### 1.2 迁移原则

1. **向后兼容**：新版本应兼容旧版本的数据
2. **幂等性**：迁移脚本可以多次执行而不产生副作用
3. **原子性**：迁移要么全部成功，要么全部回滚
4. **可回滚**：支持回滚到之前的版本
5. **版本控制**：每个迁移都有唯一的版本号

### 1.3 迁移类型

| 类型 | 描述 | 示例 |
|------|------|------|
| Schema 迁移 | 修改表结构 | 添加新表、修改列 |
| 数据迁移 | 迁移数据 | 数据格式转换、数据清洗 |
| 索引迁移 | 修改索引 | 添加索引、删除索引 |
| 约束迁移 | 修改约束 | 添加约束、删除约束 |

---

## 二、迁移脚本示例

### 2.1 创建迁移脚本

迁移脚本使用 SQL 文件，命名格式为 `V{version}__{description}.sql`。

#### 2.1.1 创建用户表

```sql
-- V1__create_users_table.sql
CREATE TABLE IF NOT EXISTS users (
    user_id VARCHAR(255) PRIMARY KEY,
    username VARCHAR(255) UNIQUE NOT NULL,
    password_hash VARCHAR(255),
    displayname VARCHAR(255),
    avatar_url VARCHAR(255),
    admin BOOLEAN NOT NULL DEFAULT false,
    deactivated BOOLEAN NOT NULL DEFAULT false,
    is_guest BOOLEAN NOT NULL DEFAULT false,
    consent_version VARCHAR(255),
    appservice_id VARCHAR(255),
    user_type VARCHAR(255),
    shadow_banned BOOLEAN NOT NULL DEFAULT false,
    generation BIGINT NOT NULL DEFAULT 0,
    invalid_update_ts BIGINT,
    migration_state VARCHAR(255),
    creation_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
```

#### 2.1.2 创建设备表

```sql
-- V2__create_devices_table.sql
CREATE TABLE IF NOT EXISTS devices (
    device_id VARCHAR(255) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    last_seen_ts BIGINT NOT NULL,
    last_seen_ip VARCHAR(255),
    created_ts BIGINT NOT NULL,
    ignored_user_list VARCHAR(255),
    appservice_id VARCHAR(255),
    first_seen_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);
```

#### 2.1.3 创建访问令牌表

```sql
-- V3__create_access_tokens_table.sql
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token VARCHAR(255) UNIQUE NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    invalidated_ts BIGINT,
    expired_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_device_id ON access_tokens(device_id);
```

#### 2.1.4 创建房间表

```sql
-- V4__create_rooms_table.sql
CREATE TABLE IF NOT EXISTS rooms (
    room_id VARCHAR(255) PRIMARY KEY,
    is_public BOOLEAN NOT NULL DEFAULT false,
    creator VARCHAR(255) NOT NULL,
    creation_ts BIGINT NOT NULL,
    federate BOOLEAN NOT NULL DEFAULT true,
    version VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    topic VARCHAR(255),
    avatar VARCHAR(255),
    canonical_alias VARCHAR(255),
    guest_access BOOLEAN NOT NULL DEFAULT false,
    history_visibility VARCHAR(255) NOT NULL DEFAULT 'shared',
    encryption VARCHAR(255),
    is_flaged BOOLEAN NOT NULL DEFAULT false,
    is_spotlight BOOLEAN NOT NULL DEFAULT false,
    deleted_ts BIGINT,
    join_rule VARCHAR(255),
    member_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);
CREATE INDEX IF NOT EXISTS idx_rooms_canonical_alias ON rooms(canonical_alias);
```

#### 2.1.5 创建事件表

```sql
-- V5__create_events_table.sql
CREATE TABLE IF NOT EXISTS events (
    event_id VARCHAR(255) PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    state_key VARCHAR(255),
    depth BIGINT NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT NOT NULL,
    not_before BIGINT,
    status VARCHAR(255),
    reference_image VARCHAR(255),
    origin VARCHAR(255) NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_events_room_id ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_user_id ON events(user_id);
CREATE INDEX IF NOT EXISTS idx_events_origin_server_ts ON events(origin_server_ts);
CREATE INDEX IF NOT EXISTS idx_events_event_type ON events(event_type);
```

#### 2.1.6 创建成员关系表

```sql
-- V6__create_room_memberships_table.sql
CREATE TABLE IF NOT EXISTS room_memberships (
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    membership VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    avatar_url VARCHAR(255),
    is_banned BOOLEAN,
    invite_token VARCHAR(255),
    inviter VARCHAR(255),
    updated_ts BIGINT,
    joined_ts BIGINT,
    left_ts BIGINT,
    reason VARCHAR(255),
    join_reason VARCHAR(255),
    banned_by VARCHAR(255),
    PRIMARY KEY (room_id, user_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_memberships_user_id ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_membership ON room_memberships(membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_event_id ON room_memberships(event_id);
```

#### 2.1.7 创建在线状态表

```sql
-- V7__create_presence_table.sql
CREATE TABLE IF NOT EXISTS presence (
    user_id VARCHAR(255) PRIMARY KEY,
    presence VARCHAR(255) NOT NULL,
    status_msg VARCHAR(255),
    last_active_ts BIGINT NOT NULL,
    currently_active BOOLEAN NOT NULL DEFAULT false,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_presence_presence ON presence(presence);
```

#### 2.1.8 创建好友表

```sql
-- V8__create_friends_table.sql
CREATE TABLE IF NOT EXISTS friends (
    user_id VARCHAR(255) NOT NULL,
    friend_id VARCHAR(255) NOT NULL,
    category VARCHAR(255),
    added_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, friend_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_friends_friend_id ON friends(friend_id);
CREATE INDEX IF NOT EXISTS idx_friends_category ON friends(category);
```

#### 2.1.9 创建好友请求表

```sql
-- V9__create_friend_requests_table.sql
CREATE TABLE IF NOT EXISTS friend_requests (
    request_id VARCHAR(255) PRIMARY KEY,
    from_user_id VARCHAR(255) NOT NULL,
    to_user_id VARCHAR(255) NOT NULL,
    message VARCHAR(255),
    status VARCHAR(255) NOT NULL,
    created_at BIGINT NOT NULL,
    responded_at BIGINT,
    FOREIGN KEY (from_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (to_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_friend_requests_from_user_id ON friend_requests(from_user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_to_user_id ON friend_requests(to_user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_status ON friend_requests(status);
```

#### 2.1.10 创建好友分类表

```sql
-- V10__create_friend_categories_table.sql
CREATE TABLE IF NOT EXISTS friend_categories (
    user_id VARCHAR(255) NOT NULL,
    category_name VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    color VARCHAR(255),
    icon VARCHAR(255),
    created_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, category_name),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
```

#### 2.1.11 创建黑名单表

```sql
-- V11__create_blocked_users_table.sql
CREATE TABLE IF NOT EXISTS blocked_users (
    user_id VARCHAR(255) NOT NULL,
    blocked_user_id VARCHAR(255) NOT NULL,
    reason VARCHAR(255),
    blocked_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, blocked_user_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (blocked_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_blocked_users_blocked_user_id ON blocked_users(blocked_user_id);
```

#### 2.1.12 创建私聊会话表

```sql
-- V12__create_private_sessions_table.sql
CREATE TABLE IF NOT EXISTS private_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_id VARCHAR(255) UNIQUE NOT NULL,
    creator_id VARCHAR(255) NOT NULL,
    participant_id VARCHAR(255) NOT NULL,
    session_name VARCHAR(255),
    ttl_seconds INTEGER,
    auto_delete BOOLEAN,
    created_at TIMESTAMP NOT NULL,
    FOREIGN KEY (creator_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (participant_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_private_sessions_session_id ON private_sessions(session_id);
CREATE INDEX IF NOT EXISTS idx_private_sessions_creator_id ON private_sessions(creator_id);
CREATE INDEX IF NOT EXISTS idx_private_sessions_participant_id ON private_sessions(participant_id);
```

#### 2.1.13 创建私聊消息表

```sql
-- V13__create_private_messages_table.sql
CREATE TABLE IF NOT EXISTS private_messages (
    id BIGSERIAL PRIMARY KEY,
    message_id VARCHAR(255) UNIQUE NOT NULL,
    session_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    encrypted BOOLEAN NOT NULL DEFAULT false,
    ttl_seconds INTEGER,
    created_at TIMESTAMP NOT NULL,
    read_at TIMESTAMP,
    FOREIGN KEY (session_id) REFERENCES private_sessions(session_id) ON DELETE CASCADE,
    FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_private_messages_message_id ON private_messages(message_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_session_id ON private_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_sender_id ON private_messages(sender_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_created_at ON private_messages(created_at);
```

#### 2.1.14 创建会话密钥表

```sql
-- V14__create_session_keys_table.sql
CREATE TABLE IF NOT EXISTS session_keys (
    id BIGSERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL,
    key_data TEXT NOT NULL,
    created_at TIMESTAMP NOT NULL,
    expires_at TIMESTAMP,
    FOREIGN KEY (session_id) REFERENCES private_sessions(session_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_session_keys_session_id ON session_keys(session_id);
```

#### 2.1.15 创建语音消息表

```sql
-- V15__create_voice_messages_table.sql
CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL PRIMARY KEY,
    message_id VARCHAR(255) UNIQUE NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255),
    file_format VARCHAR(255) NOT NULL,
    file_size BIGINT NOT NULL,
    duration INTEGER NOT NULL,
    file_url VARCHAR(255) NOT NULL,
    created_at TIMESTAMP NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_voice_messages_message_id ON voice_messages(message_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user_id ON voice_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_room_id ON voice_messages(room_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_created_at ON voice_messages(created_at);
```

#### 2.1.16 创建安全事件表

```sql
-- V16__create_security_events_table.sql
CREATE TABLE IF NOT EXISTS security_events (
    id BIGSERIAL PRIMARY KEY,
    event_type VARCHAR(255) NOT NULL,
    user_id VARCHAR(255),
    ip_address VARCHAR(255),
    user_agent VARCHAR(255),
    details JSONB,
    created_at TIMESTAMP NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_security_events_event_type ON security_events(event_type);
CREATE INDEX IF NOT EXISTS idx_security_events_user_id ON security_events(user_id);
CREATE INDEX IF NOT EXISTS idx_security_events_ip_address ON security_events(ip_address);
CREATE INDEX IF NOT EXISTS idx_security_events_created_at ON security_events(created_at);
```

#### 2.1.17 创建 IP 阻止表

```sql
-- V17__create_ip_blocks_table.sql
CREATE TABLE IF NOT EXISTS ip_blocks (
    id BIGSERIAL PRIMARY KEY,
    ip_address VARCHAR(255) UNIQUE NOT NULL,
    reason VARCHAR(255),
    blocked_at TIMESTAMP NOT NULL,
    expires_at TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_ip_blocks_ip_address ON ip_blocks(ip_address);
CREATE INDEX IF NOT EXISTS idx_ip_blocks_blocked_at ON ip_blocks(blocked_at);
```

#### 2.1.18 创建 IP 声誉表

```sql
-- V18__create_ip_reputation_table.sql
CREATE TABLE IF NOT EXISTS ip_reputation (
    id BIGSERIAL PRIMARY KEY,
    ip_address VARCHAR(255) UNIQUE NOT NULL,
    score INTEGER NOT NULL DEFAULT 0,
    last_seen_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_ip_reputation_ip_address ON ip_reputation(ip_address);
CREATE INDEX IF NOT EXISTS idx_ip_reputation_score ON ip_reputation(score);
```

### 2.2 创建回滚脚本

回滚脚本使用 SQL 文件，命名格式为 `U{version}__{description}.sql`。

#### 2.2.1 回滚用户表

```sql
-- U1__drop_users_table.sql
DROP TABLE IF EXISTS users CASCADE;
```

#### 2.2.2 回滚设备表

```sql
-- U2__drop_devices_table.sql
DROP TABLE IF EXISTS devices CASCADE;
```

#### 2.2.3 回滚访问令牌表

```sql
-- U3__drop_access_tokens_table.sql
DROP TABLE IF EXISTS access_tokens CASCADE;
```

---

## 三、版本管理

### 3.1 迁移版本表

创建迁移版本表，跟踪已应用的迁移。

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
    version BIGINT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

### 3.2 迁移执行

#### 3.2.1 应用迁移

```rust
use sqlx::{Pool, Postgres};

pub async fn apply_migrations(pool: &Pool<Postgres>) -> Result<(), sqlx::Error> {
    let migrations = vec![
        ("V1__create_users_table.sql", include_str!("../migrations/V1__create_users_table.sql")),
        ("V2__create_devices_table.sql", include_str!("../migrations/V2__create_devices_table.sql")),
        ("V3__create_access_tokens_table.sql", include_str!("../migrations/V3__create_access_tokens_table.sql")),
        ("V4__create_rooms_table.sql", include_str!("../migrations/V4__create_rooms_table.sql")),
        ("V5__create_events_table.sql", include_str!("../migrations/V5__create_events_table.sql")),
        ("V6__create_room_memberships_table.sql", include_str!("../migrations/V6__create_room_memberships_table.sql")),
        ("V7__create_presence_table.sql", include_str!("../migrations/V7__create_presence_table.sql")),
        ("V8__create_friends_table.sql", include_str!("../migrations/V8__create_friends_table.sql")),
        ("V9__create_friend_requests_table.sql", include_str!("../migrations/V9__create_friend_requests_table.sql")),
        ("V10__create_friend_categories_table.sql", include_str!("../migrations/V10__create_friend_categories_table.sql")),
        ("V11__create_blocked_users_table.sql", include_str!("../migrations/V11__create_blocked_users_table.sql")),
        ("V12__create_private_sessions_table.sql", include_str!("../migrations/V12__create_private_sessions_table.sql")),
        ("V13__create_private_messages_table.sql", include_str!("../migrations/V13__create_private_messages_table.sql")),
        ("V14__create_session_keys_table.sql", include_str!("../migrations/V14__create_session_keys_table.sql")),
        ("V15__create_voice_messages_table.sql", include_str!("../migrations/V15__create_voice_messages_table.sql")),
        ("V16__create_security_events_table.sql", include_str!("../migrations/V16__create_security_events_table.sql")),
        ("V17__create_ip_blocks_table.sql", include_str!("../migrations/V17__create_ip_blocks_table.sql")),
        ("V18__create_ip_reputation_table.sql", include_str!("../migrations/V18__create_ip_reputation_table.sql")),
    ];
    
    for (name, sql) in migrations {
        let version = name.split("__").next().unwrap()[1..].parse::<i64>().unwrap();
        
        let applied = sqlx::query_scalar!(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = $1)",
            version
        ).fetch_one(pool).await?;
        
        if !applied {
            sqlx::query(sql).execute(pool).await?;
            sqlx::query!(
                "INSERT INTO schema_migrations (version, name) VALUES ($1, $2)",
                version,
                name
            ).execute(pool).await?;
            println!("Applied migration: {}", name);
        }
    }
    
    Ok(())
}
```

#### 3.2.2 回滚迁移

```rust
use sqlx::{Pool, Postgres};

pub async fn rollback_migration(pool: &Pool<Postgres>, version: i64) -> Result<(), sqlx::Error> {
    let rollback_sql = include_str!(format!("../migrations/U{}__rollback.sql", version));
    sqlx::query(rollback_sql).execute(pool).await?;
    sqlx::query!(
        "DELETE FROM schema_migrations WHERE version = $1",
        version
    ).execute(pool).await?;
    println!("Rolled back migration: {}", version);
    Ok(())
}
```

---

## 四、数据迁移工具

### 4.1 SQLx 迁移工具

使用 `sqlx-cli` 工具管理迁移。

#### 4.1.1 安装 sqlx-cli

```bash
cargo install sqlx-cli
```

#### 4.1.2 初始化迁移

```bash
sqlx database create --database-url postgres://user:password@localhost/synapse_db
sqlx migrate run --database-url postgres://user:password@localhost/synapse_db
```

#### 4.1.3 添加新迁移

```bash
sqlx migrate add --name create_users_table
```

### 4.2 自定义迁移工具

#### 4.2.1 迁移工具实现

```rust
use sqlx::{Pool, Postgres};

pub struct Migrator {
    pool: Pool<Postgres>,
}

impl Migrator {
    pub fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }
    
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        self.create_schema_migrations_table().await?;
        self.apply_migrations().await
    }
    
    async fn create_schema_migrations_table(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version BIGINT PRIMARY KEY,
                name VARCHAR(255) NOT NULL,
                applied_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
            )
            "#
        ).execute(&self.pool).await?;
        Ok(())
    }
    
    async fn apply_migrations(&self) -> Result<(), sqlx::Error> {
        let migrations = self.get_migrations();
        for (version, name, sql) in migrations {
            let applied = sqlx::query_scalar!(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = $1)",
                version
            ).fetch_one(&self.pool).await?;
            
            if !applied {
                sqlx::query(sql).execute(&self.pool).await?;
                sqlx::query!(
                    "INSERT INTO schema_migrations (version, name) VALUES ($1, $2)",
                    version,
                    name
                ).execute(&self.pool).await?;
                println!("Applied migration: {}", name);
            }
        }
        Ok(())
    }
    
    fn get_migrations(&self) -> Vec<(i64, String, String)> {
        vec![
            (1, "create_users_table", include_str!("../migrations/V1__create_users_table.sql")),
            (2, "create_devices_table", include_str!("../migrations/V2__create_devices_table.sql")),
            (3, "create_access_tokens_table", include_str!("../migrations/V3__create_access_tokens_table.sql")),
            (4, "create_rooms_table", include_str!("../migrations/V4__create_rooms_table.sql")),
            (5, "create_events_table", include_str!("../migrations/V5__create_events_table.sql")),
            (6, "create_room_memberships_table", include_str!("../migrations/V6__create_room_memberships_table.sql")),
            (7, "create_presence_table", include_str!("../migrations/V7__create_presence_table.sql")),
            (8, "create_friends_table", include_str!("../migrations/V8__create_friends_table.sql")),
            (9, "create_friend_requests_table", include_str!("../migrations/V9__create_friend_requests_table.sql")),
            (10, "create_friend_categories_table", include_str!("../migrations/V10__create_friend_categories_table.sql")),
            (11, "create_blocked_users_table", include_str!("../migrations/V11__create_blocked_users_table.sql")),
            (12, "create_private_sessions_table", include_str!("../migrations/V12__create_private_sessions_table.sql")),
            (13, "create_private_messages_table", include_str!("../migrations/V13__create_private_messages_table.sql")),
            (14, "create_session_keys_table", include_str!("../migrations/V14__create_session_keys_table.sql")),
            (15, "create_voice_messages_table", include_str!("../migrations/V15__create_voice_messages_table.sql")),
            (16, "create_security_events_table", include_str!("../migrations/V16__create_security_events_table.sql")),
            (17, "create_ip_blocks_table", include_str!("../migrations/V17__create_ip_blocks_table.sql")),
            (18, "create_ip_reputation_table", include_str!("../migrations/V18__create_ip_reputation_table.sql")),
        ]
    }
}
```

---

## 五、迁移最佳实践

### 5.1 迁移设计

#### 5.1.1 向后兼容

新版本应兼容旧版本的数据。

```sql
-- 添加新列，使用默认值
ALTER TABLE users ADD COLUMN new_column VARCHAR(255) DEFAULT 'default_value';
```

#### 5.1.2 幂等性

迁移脚本可以多次执行而不产生副作用。

```sql
-- 使用 IF NOT EXISTS
CREATE TABLE IF NOT EXISTS users (...);

-- 使用 IF EXISTS
DROP INDEX IF EXISTS idx_users_username ON users(username);
```

#### 5.1.3 原子性

使用事务确保迁移的原子性。

```sql
BEGIN;
-- 迁移操作
COMMIT;
```

### 5.2 迁移测试

#### 5.2.1 测试迁移脚本

在测试环境中测试迁移脚本。

```bash
# 创建测试数据库
createdb synapse_test_db

# 应用迁移
psql -d synapse_test_db -f migrations/V1__create_users_table.sql

# 验证表结构
psql -d synapse_test_db -c "\d users"

# 删除测试数据库
dropdb synapse_test_db
```

#### 5.2.2 测试回滚

测试回滚脚本。

```bash
# 应用迁移
psql -d synapse_test_db -f migrations/V1__create_users_table.sql

# 回滚迁移
psql -d synapse_test_db -f migrations/U1__drop_users_table.sql

# 验证表已删除
psql -d synapse_test_db -c "\dt"
```

### 5.3 迁移部署

#### 5.3.1 备份数据库

在应用迁移前备份数据库。

```bash
# 备份数据库
pg_dump -U postgres synapse_db > backup_$(date +%Y%m%d_%H%M%S).sql
```

#### 5.3.2 应用迁移

在生产环境中应用迁移。

```bash
# 应用迁移
psql -U postgres synapse_db -f migrations/V1__create_users_table.sql
```

#### 5.3.3 验证迁移

验证迁移是否成功。

```bash
# 验证表结构
psql -U postgres synapse_db -c "\d users"

# 验证数据完整性
psql -U postgres synapse_db -c "SELECT COUNT(*) FROM users;"
```

---

## 六、参考资料

- [PostgreSQL 文档](https://www.postgresql.org/docs/)
- [SQLx 文档](https://docs.rs/sqlx/latest/sqlx/)
- [sqlx-cli 文档](https://docs.rs/sqlx-cli/latest/sqlx_cli/)

---

## 七、变更日志

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-01-28 | 初始版本，定义数据迁移指南 |

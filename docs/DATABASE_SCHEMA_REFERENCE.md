# 数据库 Schema 完整参考

> 版本: v1.0.0
> 更新日期: 2026-03-14
> 项目: synapse-rust

---

## 一、核心表 (Core Tables)

### 1.1 users - 用户表

```sql
CREATE TABLE users (
    user_id         TEXT PRIMARY KEY,           -- @username:server
    username        TEXT UNIQUE NOT NULL,       -- 用户名
    password_hash   TEXT,                        -- 密码哈希
    displayname     TEXT,                        -- 显示名
    avatar_url      TEXT,                        -- 头像URL
    is_admin        BOOLEAN DEFAULT FALSE,       -- 管理员
    is_guest        BOOLEAN DEFAULT FALSE,      -- 访客
    is_deactivated  BOOLEAN DEFAULT FALSE,       -- 已停用
    user_type       TEXT,                        -- 用户类型
    created_ts      BIGINT NOT NULL,            -- 创建时间 (毫秒)
    updated_ts      BIGINT,                      -- 更新时间
    generation      BIGINT DEFAULT 1             -- 代数
);

-- 索引
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_is_admin ON users(is_admin) WHERE is_admin = TRUE;
CREATE INDEX idx_users_created ON users(created_ts);
```

### 1.2 devices - 设备表

```sql
CREATE TABLE devices (
    device_id       TEXT PRIMARY KEY,           -- 设备ID
    user_id         TEXT NOT NULL,              -- 用户ID
    display_name    TEXT,                       -- 设备名称
    device_key      JSONB,                      -- 设备密钥
    last_seen_ts    BIGINT,                     -- 最后活跃时间
    last_seen_ip    VARCHAR(45),                -- 最后活跃IP
    created_ts      BIGINT NOT NULL,            -- 创建时间
    first_seen_ts   BIGINT NOT NULL,            -- 首次活跃时间
    appservice_id   TEXT,                       -- 应用服务ID
    ignored_user_list TEXT                      -- 忽略用户列表
);

-- 外键 (需要在迁移中添加)
ALTER TABLE devices ADD CONSTRAINT fk_devices_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 索引
CREATE INDEX idx_devices_user ON devices(user_id);
CREATE INDEX idx_devices_last_seen ON devices(last_seen_ts DESC) 
    WHERE last_seen_ts IS NOT NULL;
```

### 1.3 access_tokens - 访问令牌表

```sql
CREATE TABLE access_tokens (
    id              BIGSERIAL PRIMARY KEY,
    token           TEXT UNIQUE NOT NULL,       -- 令牌
    user_id         TEXT NOT NULL,              -- 用户ID
    device_id       TEXT,                       -- 设备ID
    created_ts      BIGINT NOT NULL,            -- 创建时间
    expires_at      BIGINT,                     -- 过期时间
    last_used_ts    BIGINT,                     -- 最后使用时间
    user_agent      TEXT,                       -- 用户代理
    ip_address      VARCHAR(45),                -- IP地址
    is_valid        BOOLEAN DEFAULT TRUE        -- 是否有效
);

-- 外键
ALTER TABLE access_tokens ADD CONSTRAINT fk_access_tokens_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 索引
CREATE INDEX idx_access_tokens_token ON access_tokens(token);
CREATE INDEX idx_access_tokens_user ON access_tokens(user_id);
CREATE INDEX idx_access_tokens_user_valid ON access_tokens(user_id, is_valid) 
    WHERE is_valid = TRUE;
```

### 1.4 refresh_tokens - 刷新令牌表

```sql
CREATE TABLE refresh_tokens (
    id              BIGSERIAL PRIMARY KEY,
    token_hash      TEXT UNIQUE NOT NULL,       -- 令牌哈希
    user_id         TEXT NOT NULL,              -- 用户ID
    device_id       TEXT,                       -- 设备ID
    created_ts      BIGINT NOT NULL,            -- 创建时间
    expires_at      BIGINT,                     -- 过期时间
    last_used_ts    BIGINT,                     -- 最后使用时间
    is_revoked      BOOLEAN DEFAULT FALSE,      -- 是否撤销
    revoked_at      BIGINT,                     -- 撤销时间
    revoked_reason  TEXT                        -- 撤销原因
);

-- 外键
ALTER TABLE refresh_tokens ADD CONSTRAINT fk_refresh_tokens_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 索引
CREATE INDEX idx_refresh_tokens_hash ON refresh_tokens(token_hash);
CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX idx_refresh_tokens_user_active ON refresh_tokens(user_id, created_ts DESC) 
    WHERE is_revoked = FALSE;
```

---

## 二、房间表 (Room Tables)

### 2.1 rooms - 房间表

```sql
CREATE TABLE rooms (
    room_id             TEXT PRIMARY KEY,           -- !xxxxx:server
    room_name           TEXT,                       -- 房间名
    creator_user_id     TEXT NOT NULL,              -- 创建者
    is_federatable      BOOLEAN DEFAULT TRUE,       -- 可联邦
    is_public           BOOLEAN DEFAULT FALSE,      -- 公开
    join_rules          TEXT,                       -- 加入规则
    guest_access        TEXT,                       -- 访客访问
    room_version        TEXT DEFAULT '9',           -- 房间版本
    is_spotlight        BOOLEAN DEFAULT FALSE,      -- spotlight
    is_flagged          BOOLEAN DEFAULT FALSE,      -- 标记
    created_ts          BIGINT NOT NULL,            -- 创建时间
    updated_ts          BIGINT,                     -- 更新时间
    avatar_url          TEXT,                       -- 头像
    topic               TEXT                        -- 主题
);

CREATE INDEX idx_rooms_creator ON rooms(creator_user_id);
CREATE INDEX idx_rooms_public ON rooms(is_public) WHERE is_public = TRUE;
CREATE INDEX idx_rooms_created ON rooms(created_ts DESC);
```

### 2.2 room_memberships - 房间成员表

```sql
CREATE TABLE room_memberships (
    room_id         TEXT NOT NULL,              -- 房间ID
    user_id         TEXT NOT NULL,              -- 用户ID
    membership      TEXT NOT NULL,              -- 加入/邀请/离开
    display_name    TEXT,                       -- 显示名
    avatar_url      TEXT,                       -- 头像
    is_privileged   BOOLEAN DEFAULT FALSE,      -- 特权成员
    join_ts         BIGINT NOT NULL,            -- 加入时间
    left_ts         BIGINT,                     -- 离开时间
    invite_ts       BIGINT,                     -- 邀请时间
    
    PRIMARY KEY (room_id, user_id)
);

-- 外键
ALTER TABLE room_memberships ADD CONSTRAINT fk_memberships_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
ALTER TABLE room_memberships ADD CONSTRAINT fk_memberships_room 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

-- 索引
CREATE INDEX idx_memberships_user ON room_memberships(user_id);
CREATE INDEX idx_memberships_user_membership ON room_memberships(user_id, membership);
CREATE INDEX idx_memberships_room_membership ON room_memberships(room_id, membership);
CREATE INDEX idx_memberships_joined ON room_memberships(room_id, membership, join_ts DESC) 
    WHERE membership = 'join';
```

### 2.3 room_state_events - 房间状态事件表

```sql
CREATE TABLE room_state_events (
    room_id         TEXT NOT NULL,              -- 房间ID
    type            TEXT NOT NULL,              -- 事件类型
    state_key       TEXT,                       -- 状态键
    sender          TEXT NOT NULL,              -- 发送者
    membership      TEXT,                       -- 成员关系
    redacted        BOOLEAN DEFAULT FALSE,      -- 是否删除
    origin_server_ts BIGINT NOT NULL,           -- 服务器时间
    stream_ordering BIGINT NOT NULL,           -- 流顺序
    content         JSONB DEFAULT '{}',         -- 内容
    unsigned        JSONB DEFAULT '{}',         -- 额外数据
    
    PRIMARY KEY (room_id, type, state_key)
);

-- 外键
ALTER TABLE room_state_events ADD CONSTRAINT fk_room_state_room 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

-- 索引
CREATE INDEX idx_room_state_room_type ON room_state_events(room_id, type);
CREATE INDEX idx_room_state_sender ON room_state_events(sender, origin_server_ts DESC);
CREATE INDEX idx_room_state_stream ON room_state_events(room_id, stream_ordering);
```

---

## 三、事件表 (Event Tables)

### 3.1 events - 事件表

```sql
CREATE TABLE events (
    event_id            TEXT PRIMARY KEY,           -- 事件ID
    room_id             TEXT NOT NULL,              -- 房间ID
    sender              TEXT NOT NULL,              -- 发送者
    type                TEXT NOT NULL,              -- 事件类型
    content             JSONB NOT NULL,             -- 内容
    redacted            BOOLEAN DEFAULT FALSE,      -- 是否删除
    redacted_because    TEXT,                       -- 删除原因
    origin_server_ts    BIGINT NOT NULL,            -- 服务器时间
    stream_ordering     BIGINT NOT NULL,           -- 流顺序
    topological_ordering BIGINT,                   -- 拓扑顺序
    depth               BIGINT NOT NULL,           -- 深度
    hashes              JSONB,                      -- 哈希
    signatures          JSONB,                      -- 签名
    unsigned            JSONB DEFAULT '{}',         -- 额外数据
    origin              TEXT,                       -- 来源服务器
    cascade_failed      BOOLEAN DEFAULT FALSE       -- 级联失败
);

-- 外键
ALTER TABLE events ADD CONSTRAINT fk_events_room 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
ALTER TABLE events ADD CONSTRAINT fk_events_sender 
    FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE SET NULL;

-- 索引
CREATE INDEX idx_events_room_ts ON events(room_id, origin_server_ts DESC);
CREATE INDEX idx_events_room_type ON events(room_id, type);
CREATE INDEX idx_events_sender ON events(sender, origin_server_ts DESC);
CREATE INDEX idx_events_depth ON events(room_id, depth);
CREATE INDEX idx_events_stream ON events(room_id, stream_ordering);
```

### 3.2 event_receipts - 事件收据表

```sql
CREATE TABLE event_receipts (
    room_id         TEXT NOT NULL,              -- 房间ID
    event_id        TEXT NOT NULL,              -- 事件ID
    user_id         TEXT NOT NULL,              -- 用户ID
    receipt_type    TEXT NOT NULL,              -- 收据类型
    sender          TEXT NOT NULL,              -- 发送者
    data            JSONB DEFAULT '{}',         -- 收据数据
    origin_server_ts BIGINT NOT NULL,           -- 服务器时间
    
    PRIMARY KEY (room_id, event_id, user_id, receipt_type)
);

-- 外键
ALTER TABLE event_receipts ADD CONSTRAINT fk_receipts_room 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
ALTER TABLE event_receipts ADD CONSTRAINT fk_receipts_event 
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE;

-- 索引
CREATE INDEX idx_receipts_user ON event_receipts(user_id, receipt_type);
```

---

## 四、设备与安全表 (Device & Security)

### 4.1 device_keys - 设备密钥表

```sql
CREATE TABLE device_keys (
    user_id         TEXT NOT NULL,              -- 用户ID
    device_id       TEXT NOT NULL,              -- 设备ID
    algorithm       TEXT NOT NULL,              -- 算法
    key_data        TEXT NOT NULL,              -- 密钥数据
    added_ts        BIGINT NOT NULL,            -- 添加时间
    last_seen_ts    BIGINT,                     -- 最后活跃
    is_verified     BOOLEAN DEFAULT FALSE,      -- 是否验证
    ts_updated_ms  BIGINT,                      -- 更新时间
    
    PRIMARY KEY (user_id, device_id, algorithm)
);

-- 外键
ALTER TABLE device_keys ADD CONSTRAINT fk_device_keys_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;
ALTER TABLE device_keys ADD CONSTRAINT fk_device_keys_device 
    FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE;

-- 索引
CREATE INDEX idx_device_keys_user ON device_keys(user_id);
CREATE INDEX idx_device_keys_device ON device_keys(device_id);
```

### 4.2 cross_signing_keys - 交叉签名密钥表

```sql
CREATE TABLE cross_signing_keys (
    user_id         TEXT NOT NULL,              -- 用户ID
    key_type        TEXT NOT NULL,              -- 密钥类型
    key_data        TEXT NOT NULL,              -- 密钥数据
    added_ts        BIGINT NOT NULL,            -- 添加时间
    
    PRIMARY KEY (user_id, key_type)
);

-- 外键
ALTER TABLE cross_signing_keys ADD CONSTRAINT fk_cross_keys_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 索引
CREATE INDEX idx_cross_keys_user ON cross_signing_keys(user_id);
```

### 4.3 key_backups - 密钥备份表

```sql
CREATE TABLE key_backups (
    backup_id       TEXT PRIMARY KEY,           -- 备份ID
    user_id         TEXT NOT NULL,              -- 用户ID
    algorithm       TEXT NOT NULL,              -- 算法
    auth_key        TEXT,                       -- 认证密钥
    backup_data     JSONB,                      -- 备份数据
    version         INTEGER NOT NULL,           -- 版本
    is_inactive     BOOLEAN DEFAULT FALSE,     -- 是否活跃
    created_ts      BIGINT NOT NULL,            -- 创建时间
    last_used_ts    BIGINT                      -- 最后使用
);

-- 外键
ALTER TABLE key_backups ADD CONSTRAINT fk_key_backups_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 索引
CREATE INDEX idx_key_backups_user ON key_backups(user_id);
```

---

## 五、推送表 (Push Tables)

### 5.1 pushers - 推送器表

```sql
CREATE TABLE pushers (
    id                  BIGSERIAL PRIMARY KEY,
    user_id             TEXT NOT NULL,          -- 用户ID
    device_id           TEXT,                   -- 设备ID
    pushkey             TEXT NOT NULL,          -- 推送键
    kind                VARCHAR(50),            -- 类型
    app_id              TEXT NOT NULL,          -- 应用ID
    app_display_name    TEXT,                   -- 应用显示名
    device_display_name TEXT,                   -- 设备显示名
    profile_tag         TEXT,                   -- 配置标签
    lang                VARCHAR(10),            -- 语言
    data                JSONB,                  -- 数据
    is_enabled          BOOLEAN DEFAULT TRUE,   -- 是否启用
    created_ts          BIGINT NOT NULL,        -- 创建时间
    last_updated_ts     BIGINT,                 -- 更新时间
    last_success_ts    BIGINT,                 -- 最后成功
    last_failure_ts    BIGINT,                 -- 最后失败
    failure_count      INTEGER DEFAULT 0       -- 失败次数
);

-- 外键
ALTER TABLE pushers ADD CONSTRAINT fk_pushers_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 索引
CREATE INDEX idx_pushers_user ON pushers(user_id);
CREATE INDEX idx_pushers_pushkey ON pushers(user_id, pushkey);
```

---

## 六、索引性能总结

### 必需索引清单

| 表名 | 索引 | 用途 |
|------|------|------|
| users | (username) UNIQUE | 登录 |
| users | (is_admin) | 管理员查询 |
| devices | (user_id) | 设备列表 |
| devices | (last_seen_ts) | 设备活跃 |
| access_tokens | (token) UNIQUE | Token验证 |
| access_tokens | (user_id, is_valid) | 用户Token |
| refresh_tokens | (token_hash) UNIQUE | Token验证 |
| refresh_tokens | (user_id, is_revoked) | 用户Token |
| rooms | (creator_user_id) | 创建者查询 |
| rooms | (is_public) | 公开房间 |
| room_memberships | (user_id, membership) | 用户房间 |
| room_memberships | (room_id, membership) | 房间成员 |
| events | (room_id, origin_server_ts) | 时间线 |
| events | (sender, origin_server_ts) | 用户事件 |
| room_state_events | (room_id, type, state_key) | 状态查询 |
| device_keys | (user_id, device_id) | 设备密钥 |
| pushers | (user_id, pushkey) | 推送 |

---

## 七、字段类型映射

### PostgreSQL → Rust

| PostgreSQL | Rust | 说明 |
|------------|------|------|
| BIGINT NOT NULL | i64 | 时间戳, ID |
| BIGINT NULLABLE | Option<i64> | 可空时间戳 |
| TEXT NOT NULL | String | 字符串 |
| TEXT NULLABLE | Option<String> | 可空字符串 |
| BOOLEAN NOT NULL | bool | 布尔值 |
| BOOLEAN NULLABLE | Option<bool> | 可空布尔 |
| JSONB | serde_json::Value | JSON数据 |
| TIMESTAMPTZ | DateTime<Utc> | 带时区时间 |

---

## 八、外键约束清单

### 必需外键

| 子表 | 父表 | 字段 | 级联删除 |
|------|------|------|----------|
| devices | users | user_id | CASCADE |
| access_tokens | users | user_id | CASCADE |
| access_tokens | devices | device_id | SET NULL |
| refresh_tokens | users | user_id | CASCADE |
| room_memberships | users | user_id | CASCADE |
| room_memberships | rooms | room_id | CASCADE |
| events | users | sender | SET NULL |
| events | rooms | room_id | CASCADE |
| room_state_events | rooms | room_id | CASCADE |
| room_aliases | rooms | room_id | CASCADE |
| room_tags | rooms | room_id | CASCADE |
| room_tags | users | user_id | CASCADE |
| notifications | users | user_id | CASCADE |
| notifications | rooms | room_id | CASCADE |
| device_keys | users | user_id | CASCADE |
| device_keys | devices | device_id | CASCADE |
| pushers | users | user_id | CASCADE |
| filters | users | user_id | CASCADE |
| user_threepids | users | user_id | CASCADE |
| account_data | users | user_id | CASCADE |
| room_account_data | users | user_id | CASCADE |
| room_account_data | rooms | room_id | CASCADE |
| key_backups | users | user_id | CASCADE |

# synapse-rust 项目优化方案

> 创建日期：2026-03-01
> 基于：合规性检查报告
> 目标：使项目完全符合 project_rules.md 规范

---

## 一、优化方案总览

### 1.1 优化目标

| 目标 | 当前状态 | 目标状态 | 预计完成时间 |
|------|----------|----------|--------------|
| 字段命名规范合规率 | 67% | 100% | 2026-03-15 |
| 时间戳类型规范合规率 | 60% | 100% | 2026-03-10 |
| 安全合规性 | 100% | 100% | 已达成 |
| 缓存策略合规率 | 95% | 100% | 2026-03-05 |

### 1.2 实施阶段

```
阶段一（2026-03-01 ~ 2026-03-05）：高优先级问题修复
阶段二（2026-03-06 ~ 2026-03-10）：中优先级问题修复
阶段三（2026-03-11 ~ 2026-03-15）：低优先级问题修复
阶段四（2026-03-16 ~ 2026-03-20）：测试验证与文档更新
```

---

## 二、高优先级优化方案

### 2.1 P1-001：E2EE 表时间戳字段修复

#### 问题描述

E2EE 相关表使用 `DateTime<Utc>` 类型和 `created_at`/`updated_at` 字段名，不符合规范。

#### 影响文件

| 文件路径 | 结构体 | 修改内容 |
|----------|--------|----------|
| src/e2ee/cross_signing/models.rs | CrossSigningKey | created_at → created_ts, updated_at → updated_ts |
| src/e2ee/cross_signing/models.rs | DeviceKeyInfo | created_at → created_ts |
| src/e2ee/cross_signing/models.rs | DeviceSignature | created_at → created_ts |
| src/e2ee/device_keys/models.rs | DeviceKey | created_at → created_ts, updated_at → updated_ts |
| src/e2ee/megolm/models.rs | MegolmSession | created_at → created_ts, last_used_at → last_used_ts |
| src/e2ee/signature/models.rs | EventSignature | created_at → created_ts |

#### 具体改进措施

**步骤 1：修改 Rust 结构体定义**

```rust
// 修改前
pub struct CrossSigningKey {
    pub id: Uuid,
    pub user_id: String,
    pub key_type: String,
    pub public_key: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// 修改后
pub struct CrossSigningKey {
    pub id: Uuid,
    pub user_id: String,
    pub key_type: String,
    pub public_key: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}
```

**步骤 2：修改 SQL 查询**

```rust
// 修改前
sqlx::query_as::<_, CrossSigningKey>(
    "SELECT id, user_id, key_type, public_key, created_at, updated_at FROM cross_signing_keys WHERE user_id = $1"
)

// 修改后
sqlx::query_as::<_, CrossSigningKey>(
    "SELECT id, user_id, key_type, public_key, created_ts, updated_ts FROM cross_signing_keys WHERE user_id = $1"
)
```

**步骤 3：创建数据库迁移脚本**

```sql
-- 文件：migrations/20260301000001_fix_e2ee_timestamp_fields.sql

-- 修改 device_keys 表
ALTER TABLE device_keys RENAME COLUMN created_at TO created_ts;
ALTER TABLE device_keys RENAME COLUMN updated_at TO updated_ts;
ALTER TABLE device_keys ALTER COLUMN created_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM created_ts) * 1000;
ALTER TABLE device_keys ALTER COLUMN updated_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM updated_ts) * 1000;

-- 修改 cross_signing_keys 表
ALTER TABLE cross_signing_keys RENAME COLUMN created_at TO created_ts;
ALTER TABLE cross_signing_keys RENAME COLUMN updated_at TO updated_ts;
ALTER TABLE cross_signing_keys ALTER COLUMN created_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM created_ts) * 1000;
ALTER TABLE cross_signing_keys ALTER COLUMN updated_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM updated_ts) * 1000;

-- 修改 megolm_sessions 表
ALTER TABLE megolm_sessions RENAME COLUMN created_at TO created_ts;
ALTER TABLE megolm_sessions RENAME COLUMN last_used_at TO last_used_ts;
ALTER TABLE megolm_sessions RENAME COLUMN expires_at TO expires_ts;
ALTER TABLE megolm_sessions ALTER COLUMN created_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM created_ts) * 1000;
ALTER TABLE megolm_sessions ALTER COLUMN last_used_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM last_used_ts) * 1000;
ALTER TABLE megolm_sessions ALTER COLUMN expires_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM expires_ts) * 1000;

-- 修改 event_signatures 表
ALTER TABLE event_signatures RENAME COLUMN created_at TO created_ts;
ALTER TABLE event_signatures ALTER COLUMN created_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM created_ts) * 1000;
```

#### 预期效果

- 时间戳字段统一使用 BIGINT 类型
- 字段命名符合 `_ts` 后缀规范
- 查询性能提升（BIGINT 比 TIMESTAMPTZ 更高效）

#### 实施时间表

| 日期 | 任务 | 负责人 |
|------|------|--------|
| 2026-03-02 | 修改 Rust 结构体定义 | 开发团队 |
| 2026-03-03 | 修改 SQL 查询语句 | 开发团队 |
| 2026-03-04 | 创建并执行迁移脚本 | DBA |
| 2026-03-05 | 测试验证 | QA 团队 |

---

### 2.2 P1-002：CAS/SAML 认证表字段修复

#### 问题描述

CAS 和 SAML 认证相关表使用 `DateTime<Utc>` 类型和 `created_at`/`updated_at` 字段名。

#### 影响文件

| 文件路径 | 结构体数量 | 修改字段数 |
|----------|------------|------------|
| src/storage/cas.rs | 6 | 12 |
| src/storage/saml.rs | 4 | 8 |

#### 具体改进措施

**步骤 1：修改 CAS 结构体**

```rust
// 修改前
pub struct CasTicket {
    pub ticket: String,
    pub service: String,
    pub user_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

// 修改后
pub struct CasTicket {
    pub ticket: String,
    pub service: String,
    pub user_id: String,
    pub created_ts: i64,
    pub expires_ts: Option<i64>,
}
```

**步骤 2：修改 SAML 结构体**

```rust
// 修改前
pub struct SamlSession {
    pub session_id: String,
    pub user_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

// 修改后
pub struct SamlSession {
    pub session_id: String,
    pub user_id: String,
    pub created_ts: i64,
    pub expires_ts: Option<i64>,
}
```

**步骤 3：创建数据库迁移脚本**

```sql
-- 文件：migrations/20260301000002_fix_auth_timestamp_fields.sql

-- CAS 表修改
ALTER TABLE cas_tickets RENAME COLUMN created_at TO created_ts;
ALTER TABLE cas_tickets RENAME COLUMN expires_at TO expires_ts;
ALTER TABLE cas_tickets ALTER COLUMN created_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM created_ts) * 1000;
ALTER TABLE cas_tickets ALTER COLUMN expires_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM expires_ts) * 1000;

-- SAML 表修改
ALTER TABLE saml_sessions RENAME COLUMN created_at TO created_ts;
ALTER TABLE saml_sessions RENAME COLUMN expires_at TO expires_ts;
ALTER TABLE saml_sessions ALTER COLUMN created_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM created_ts) * 1000;
ALTER TABLE saml_sessions ALTER COLUMN expires_ts TYPE BIGINT 
    USING EXTRACT(EPOCH FROM expires_ts) * 1000;
```

#### 实施时间表

| 日期 | 任务 |
|------|------|
| 2026-03-03 | 修改 CAS/SAML 结构体定义 |
| 2026-03-04 | 修改相关 SQL 查询 |
| 2026-03-05 | 执行迁移脚本并测试 |

---

### 2.3 P1-003：布尔字段命名修复（核心模块）

#### 问题描述

核心模块中的布尔字段未使用 `is_` 前缀。

#### 影响范围

| 文件 | 问题字段 | 修改为 |
|------|----------|--------|
| src/auth/mod.rs | admin | is_admin |
| src/auth/mod.rs | allow_legacy_hashes | is_legacy_hash_allowed |
| src/storage/saml.rs | enabled | is_enabled |

#### 具体改进措施

**步骤 1：修改 Claims 结构体**

```rust
// 修改前
pub struct Claims {
    pub sub: String,
    pub user_id: String,
    pub device_id: String,
    pub admin: bool,
    pub exp: usize,
    pub iat: usize,
}

// 修改后
pub struct Claims {
    pub sub: String,
    pub user_id: String,
    pub device_id: String,
    pub is_admin: bool,
    pub exp: usize,
    pub iat: usize,
}
```

**步骤 2：使用 serde 别名保持 API 兼容**

```rust
#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub user_id: String,
    pub device_id: String,
    #[serde(alias = "admin")]
    pub is_admin: bool,
    pub exp: usize,
    pub iat: usize,
}
```

#### 实施时间表

| 日期 | 任务 |
|------|------|
| 2026-03-02 | 修改核心模块布尔字段 |
| 2026-03-03 | 添加 serde 别名保持兼容性 |
| 2026-03-04 | 更新相关测试用例 |

---

### 2.4 P1-004：Token TTL 不一致修复

#### 问题描述

`CacheTtl::token()` 定义为 86400 秒，但实际使用时设置为 3600 秒。

#### 影响文件

| 文件 | 行号 | 当前值 |
|------|------|--------|
| src/cache/strategy.rs | 102 | 86400 秒 |
| src/auth/mod.rs | 594 | 3600 秒 |

#### 具体改进措施

**步骤 1：统一 TTL 定义**

```rust
// src/cache/strategy.rs
impl CacheTtl {
    pub fn token() -> u64 {
        3600  // 修改为 1 小时，与实际使用一致
    }
}
```

**步骤 2：使用统一配置**

```rust
// src/auth/mod.rs
// 修改前
self.cache.set_token(token, &final_claims, 3600).await;

// 修改后
self.cache.set_token(token, &final_claims, CacheTtl::token()).await;
```

#### 实施时间表

| 日期 | 任务 |
|------|------|
| 2026-03-02 | 统一 Token TTL 定义 |
| 2026-03-02 | 更新所有调用点 |

---

## 三、中优先级优化方案

### 3.1 P2-001：媒体表时间戳字段修复

#### 具体改进措施

```rust
// src/storage/media/models.rs

// 修改前
pub struct MediaMetadata {
    pub media_id: String,
    pub created_at: DateTime<Utc>,
}

// 修改后
pub struct MediaMetadata {
    pub media_id: String,
    pub created_ts: i64,
}
```

#### 实施时间表

| 日期 | 任务 |
|------|------|
| 2026-03-06 | 修改媒体模块结构体 |
| 2026-03-07 | 执行迁移脚本 |

---

### 3.2 P2-002：验证码表时间戳字段修复

#### 具体改进措施

```rust
// src/storage/captcha.rs

// 修改前
pub struct RegistrationCaptcha {
    pub captcha_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

// 修改后
pub struct RegistrationCaptcha {
    pub captcha_id: String,
    pub created_ts: i64,
    pub expires_ts: Option<i64>,
}
```

#### 实施时间表

| 日期 | 任务 |
|------|------|
| 2026-03-08 | 修改验证码模块结构体 |
| 2026-03-09 | 执行迁移脚本 |

---

### 3.3 P2-003：Space 表布尔字段修复

#### 具体改进措施

```rust
// src/storage/space.rs

// 修改前
pub struct SpaceChild {
    pub suggested: bool,
}

// 修改后
pub struct SpaceChild {
    pub is_suggested: bool,
}
```

#### 实施时间表

| 日期 | 任务 |
|------|------|
| 2026-03-09 | 修改 Space 模块布尔字段 |
| 2026-03-10 | 执行迁移脚本 |

---

## 四、低优先级优化方案

### 4.1 P3-001：API 响应结构体布尔字段修复

#### 影响文件

- src/web/routes/telemetry.rs
- src/web/routes/rate_limit_admin.rs
- src/web/routes/push_notification.rs
- src/web/routes/push.rs
- src/web/routes/module.rs

#### 具体改进措施

使用 `#[serde(rename)]` 保持 API 向后兼容：

```rust
// 修改前
pub struct TelemetryStatusResponse {
    pub enabled: bool,
}

// 修改后
pub struct TelemetryStatusResponse {
    #[serde(rename = "enabled")]
    pub is_enabled: bool,
}
```

#### 实施时间表

| 日期 | 任务 |
|------|------|
| 2026-03-11 ~ 2026-03-14 | 逐个修改 API 响应结构体 |
| 2026-03-15 | API 兼容性测试 |

---

### 4.2 P3-002：迁移脚本历史问题修复

#### 具体改进措施

创建统一修复脚本，清理历史遗留问题：

```sql
-- 文件：migrations/20260301000003_cleanup_legacy_fields.sql

-- 统一布尔字段命名
ALTER TABLE users RENAME COLUMN shadow_banned TO is_shadow_banned;
ALTER TABLE rooms RENAME COLUMN federate TO is_federate;
ALTER TABLE events RENAME COLUMN redacted TO is_redacted;
ALTER TABLE workers RENAME COLUMN enabled TO is_enabled;

-- 添加索引优化
CREATE INDEX IF NOT EXISTS idx_users_is_shadow_banned ON users(is_shadow_banned);
CREATE INDEX IF NOT EXISTS idx_events_is_redacted ON events(is_redacted) WHERE is_redacted = FALSE;
```

#### 实施时间表

| 日期 | 任务 |
|------|------|
| 2026-03-14 | 创建统一修复脚本 |
| 2026-03-15 | 执行并验证 |

---

## 五、测试验证计划

### 5.1 单元测试

| 模块 | 测试用例数 | 负责人 |
|------|------------|--------|
| E2EE 模块 | 20 | 开发团队 |
| 认证模块 | 15 | 开发团队 |
| 缓存模块 | 10 | 开发团队 |
| API 响应 | 30 | QA 团队 |

### 5.2 集成测试

| 测试场景 | 测试内容 |
|----------|----------|
| 用户注册登录 | 验证时间戳字段正确性 |
| Token 管理 | 验证 TTL 配置正确性 |
| E2EE 加密 | 验证密钥管理功能 |
| API 兼容性 | 验证响应字段兼容性 |

### 5.3 性能测试

| 测试项 | 指标 |
|--------|------|
| 时间戳查询性能 | BIGINT vs TIMESTAMPTZ 对比 |
| 缓存命中率 | Token 缓存命中率 > 90% |
| 数据库迁移时间 | 迁移脚本执行时间 < 5 分钟 |

---

## 六、回滚计划

### 6.1 回滚脚本

每个迁移脚本都配备对应的回滚脚本：

```sql
-- 文件：migrations/rollback/20260301000001_fix_e2ee_timestamp_fields_rollback.sql

ALTER TABLE device_keys RENAME COLUMN created_ts TO created_at;
ALTER TABLE device_keys RENAME COLUMN updated_ts TO updated_at;
ALTER TABLE device_keys ALTER COLUMN created_at TYPE TIMESTAMPTZ 
    USING TO_TIMESTAMP(created_ts / 1000.0);
ALTER TABLE device_keys ALTER COLUMN updated_at TYPE TIMESTAMPTZ 
    USING TO_TIMESTAMP(updated_ts / 1000.0);

-- ... 其他表类似
```

### 6.2 回滚触发条件

- 单元测试失败率 > 5%
- 集成测试发现关键功能异常
- 性能测试显示性能下降 > 10%

---

## 七、文档更新计划

### 7.1 需更新的文档

| 文档 | 更新内容 | 负责人 |
|------|----------|--------|
| data-models.md | 更新表结构定义 | 开发团队 |
| DATABASE_FIELD_STANDARDS.md | 补充迁移说明 | 开发团队 |
| API 文档 | 更新响应字段说明 | QA 团队 |
| project_rules.md | 补充实施记录 | 开发团队 |

### 7.2 版本记录

每次优化完成后，更新版本历史：

```markdown
| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.1.0 | 2026-03-05 | 完成 E2EE 表字段修复 |
| 1.2.0 | 2026-03-10 | 完成认证表字段修复 |
| 1.3.0 | 2026-03-15 | 完成所有字段命名修复 |
```

---

## 八、风险评估与缓解

### 8.1 风险矩阵

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| 数据迁移失败 | 低 | 高 | 备份数据库，准备回滚脚本 |
| API 兼容性问题 | 中 | 中 | 使用 serde 别名，逐步迁移 |
| 性能下降 | 低 | 中 | 性能测试，监控指标 |
| 测试覆盖不足 | 中 | 中 | 增加测试用例，代码审查 |

### 8.2 应急预案

1. **数据迁移失败**：立即执行回滚脚本，恢复数据库
2. **API 兼容性问题**：发布补丁版本，添加兼容层
3. **性能问题**：优化索引，调整查询语句

---

## 九、验收标准

### 9.1 功能验收

- [ ] 所有单元测试通过
- [ ] 所有集成测试通过
- [ ] API 响应格式保持兼容
- [ ] 数据库迁移成功执行

### 9.2 规范验收

- [ ] 字段命名规范合规率 = 100%
- [ ] 时间戳类型规范合规率 = 100%
- [ ] 安全合规性 = 100%
- [ ] 缓存策略合规率 = 100%

### 9.3 文档验收

- [ ] 所有文档已更新
- [ ] 版本历史已记录
- [ ] 迁移脚本已归档

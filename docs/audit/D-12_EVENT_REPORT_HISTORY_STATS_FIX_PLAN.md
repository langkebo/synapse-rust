# D-12 EventReport History/Stats 问题修复方案

**问题 ID**: D-12  
**发现时间**: 2026-09-23  
**修复时间**: 2026-09-24  
**状态**: ✅ 已修复 (2026-09-24)  
**实施口径**: 方案 A′ —— 删除 `/history`，`/stats` **保留**并改为对 `event_reports` 的实时聚合（**静态 SQL**）。
最终形态、与 SDK 契约的对齐方式与验证结果见文末「[实施结果（最终形态）](#实施结果最终形态)」；
本文档前半部分是**决策前的方案书**，示例代码与实际落地存在差异，以文末为准。

---

## 问题概述

`event_report` 模块存在两个未完成的 HTTP 端点，其对应的存储层方法是空壳实现，且缺少必要的数据库表支持。

### 问题清单

| 问题类型 | 位置 | 症状 |
|---------|------|------|
| 空壳实现 | `synapse-storage/src/event_report/repository.rs:324` | `add_history` 只返回内存 `id:0`，无实际 DB 写入 |
| 空壳实现 | `synapse-storage/src/event_report/repository.rs:359` | `get_report_history` 恒返回空 |
| 空壳实现 | `synapse-storage/src/event_report/repository.rs:533` | `get_stats` 恒返回空 |
| 缺失表 | `migrations/` | 不存在 `event_report_history` 和 `event_report_stats` 表 |
| 已注册路由 | `synapse-web/src/routes/event_report.rs:499/506` | 两个端点已挂载到 Router，但后端无实际功能 |

---

## 当前代码状态

### 1. 路由注册（已实现）

```rust
// synapse-web/src/routes/event_report.rs
.router("/event_reports/{id}/history", get(get_report_history))
.router("/event_reports/stats", get(get_stats))
```

**端点列表**:
- `GET /_synapse/admin/v1/event_reports/{id}/history`
- `GET /_synapse/admin/v1/event_reports/stats?limit={days}`

### 2. 存储层实现（空壳）

```rust
// synapse-storage/src/event_report/repository.rs

pub async fn add_history(
    &self,
    report_id: i64,
    action: &str,
    performed_by: &str,
) -> Result<i64, sqlx::Error> {
    // ⚠️ 只返回硬编码的 id:0，没有任何数据库操作
    tracing::info!("Add history for report {}: {}", report_id, action);
    Ok(0)  // ← 总是返回 0
}

pub async fn get_report_history(&self, report_id: i64) -> Result<Vec<EventReportHistory>, sqlx::Error> {
    // ⚠️ 没有查询任何表
    Ok(vec![])  // ← 总是返回空向量
}

pub async fn get_stats(&self, days: i32) -> Result<Vec<EventReportStats>, sqlx::Error> {
    // ⚠️ 没有查询任何表
    Ok(vec![])  // ← 总是返回空向量
}
```

### 3. 数据库 Schema（缺失）

```sql
-- migrations/00000000_unified_schema_v12.sql 中仅有:
CREATE TABLE IF NOT EXISTS event_reports (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    reporter_user_id TEXT NOT NULL,
    reported_user_id TEXT,
    event_json JSONB,
    reason TEXT,
    description TEXT,
    status TEXT DEFAULT 'open',
    score INTEGER DEFAULT 0,
    received_ts BIGINT NOT NULL,
    resolved_at BIGINT,
    resolved_by TEXT,
    resolution_reason TEXT,
    CONSTRAINT pk_event_reports PRIMARY KEY (id)
);

-- ❌ 不存在以下表:
-- - event_report_history (用于记录状态变更历史)
-- - event_report_stats (用于预计算统计数据)
```

---

## 参考 Element Synapse 的设计

### Element Synapse 的 Event Reports API

Element Synapse（[element-hq/synapse](https://github.com/element-hq/synapse)）提供的 event reports API 包括：

1. **列出举报事件**: `GET /_synapse/admin/v1/event_reports`
2. **获取单个举报详情**: `GET /_synapse/admin/v1/event_reports/{report_id}`
3. **删除举报**: `DELETE /_synapse/admin/v1/event_reports/{report_id}`

**没有独立的 history/stats 端点**。

### Stats 的实现方式

Element Synapse 不维护单独的 stats 表，而是通过以下方式实现统计：

```python
# 伪代码示例
async def get_event_report_stats(self, days: int) -> Dict:
    """实时聚合 event_reports 表"""
    query = """
        SELECT 
            COUNT(*) as total_reports,
            COUNT(CASE WHEN status = 'open' THEN 1 END) as open_reports,
            COUNT(CASE WHEN status = 'resolved' THEN 1 END) as resolved_reports,
            AVG(score) as avg_score
        FROM event_reports
        WHERE received_ts > (EXTRACT(EPOCH FROM CURRENT_TIMESTAMP - INTERVAL '{} days') * 1000)::bigint
    """.format(days)
    
    result = await db.execute(query)
    return result.fetchone()
```

---

## 解决方案

### 方案 A：对齐 Element Synapse（推荐）

**核心理念**: 删除未完成的端点，使用实时聚合查询替代预计算表。

#### 实施步骤

**Step 1: 删除未完成的端点**

```rust
// synapse-web/src/routes/event_report.rs

// ❌ 删除这两行路由注册
// .route("/_synapse/admin/v1/event_reports/{id}/history", get(get_report_history))
// .route("/_synapse/admin/v1/event_reports/stats", get(get_stats))

// ❌ 删除对应的 handler 函数
// pub async fn get_report_history(...) { ... }
// pub async fn get_stats(...) { ... }
```

**Step 2: 删除空的存储方法**

```rust
// synapse-storage/src/event_report/repository.rs

// ❌ 删除这三个方法
// pub async fn add_history(...) { ... }
// pub async fn get_report_history(...) { ... }
// pub async fn get_stats(...) { ... }
```

**Step 3: 删除相关的 Service 方法**

```rust
// synapse-services/src/event_report_service.rs

// ❌ 删除这两个方法
// pub async fn get_report_history(...) { ... }
// pub async fn get_stats(...) { ... }
```

**Step 4: 如有需要，添加实时聚合查询**

如果确实需要统计功能，可以在 `event_report/repository.rs` 中添加：

```rust
/// 获取事件报告的统计信息（实时聚合）
#[instrument(skip(self))]
pub async fn get_aggregate_stats(
    &self,
    days: i32,
) -> Result<EventReportAggregateStats, sqlx::Error> {
    let cutoff_ts = chrono::Utc::now()
        .signed_duration_since(chrono::Duration::days(days as i64))
        .timestamp_millis();

    let row = sqlx::query_as!(
        EventReportAggregateStats,
        r#"
        SELECT 
            COUNT(*) AS "total_count!: i64",
            COUNT(CASE WHEN status = 'open' THEN 1 END) AS "open_count!: i64",
            COUNT(CASE WHEN status = 'resolved' THEN 1 END) AS "resolved_count!: i64",
            COUNT(CASE WHEN status = 'dismissed' THEN 1 END) AS "dismissed_count!: i64",
            COALESCE(AVG(score), 0) AS "avg_score!: f64"
        FROM event_reports
        WHERE received_ts > $1
        "#,
        cutoff_ts
    )
    .fetch_one(&self.pool)
    .await?;

    Ok(row)
}
```

> ⚠️ **实际实现与上面示例的三处差异**（以文末「实施结果」为准）：
> 1. 示例用 `query_as!` + `EventReportAggregateStats` 的 `FromRow`，实际改为
>    `sqlx::query!`（匿名记录逐字段取值）—— 与仓库静态化各批（C1–C16）同一口径；
> 2. 示例的 `days` 窗口被**去掉**：SDK 侧契约 `getStats()` **不带参数**，
>    加一个无人使用的 `?days=` 只是投机特性；SQL 因此无任何绑定参数；
> 3. 字段名与 SDK 的 `StatsResponse`（`total/open/resolved/dismissed/escalated`）
>    **逐字段对齐**，而不是示例里的 `total_count/.../avg_score`。

**优点**:
- ✅ 符合 Matrix 官方规范
- ✅ 减少不必要的表和维护成本
- ✅ 数据始终准确（实时计算）
- ✅ 最小化代码改动

**缺点**:
- ⚠️ 需要删除已有的路由定义（可能影响前端调用方）

---

### 方案 B：完整实现 History/Stats 功能

**核心理念**: 按照现有设计，完整实现历史追踪和统计功能。

#### 实施步骤

**Step 1: 创建数据库表**

```sql
-- 在 migrations/00000000_unified_schema_v12.sql 末尾添加

-- 事件报告历史表（记录状态变更）
CREATE TABLE IF NOT EXISTS event_report_history (
    id BIGSERIAL,
    report_id BIGINT NOT NULL,
    action TEXT NOT NULL,  -- 'created', 'updated', 'resolved', 'dismissed', 'escalated'
    old_status TEXT,
    new_status TEXT,
    comment TEXT,
    performed_by TEXT NOT NULL,
    performed_ts BIGINT NOT NULL,
    metadata JSONB DEFAULT '{}',
    CONSTRAINT pk_event_report_history PRIMARY KEY (id),
    CONSTRAINT fk_event_report_history_report FOREIGN KEY (report_id) 
        REFERENCES event_reports(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_event_report_history_report_id 
    ON event_report_history(report_id);

CREATE INDEX IF NOT EXISTS idx_event_report_history_performed_ts 
    ON event_report_history(performed_ts DESC);

-- 事件报告统计表（预计算，可选）
CREATE TABLE IF NOT EXISTS event_report_stats (
    id BIGSERIAL,
    stat_date DATE NOT NULL,  -- 统计日期
    total_reports BIGINT NOT NULL DEFAULT 0,
    open_reports BIGINT NOT NULL DEFAULT 0,
    resolved_reports BIGINT NOT NULL DEFAULT 0,
    dismissed_reports BIGINT NOT NULL DEFAULT 0,
    avg_score DOUBLE PRECISION NOT NULL DEFAULT 0,
    UNIQUE (stat_date),
    CONSTRAINT uq_event_report_stats_date UNIQUE (stat_date)
);

CREATE INDEX IF NOT EXISTS idx_event_report_stats_date 
    ON event_report_stats(stat_date DESC);
```

**Step 2: 实现 Repository 层**

```rust
// synapse-storage/src/event_report/repository.rs

/// 添加历史记录
#[instrument(skip(self))]
pub async fn add_history(
    &self,
    report_id: i64,
    action: &str,
    old_status: Option<&str>,
    new_status: Option<&str>,
    comment: Option<&str>,
    performed_by: &str,
) -> Result<i64, sqlx::Error> {
    let performed_ts = chrono::Utc::now().timestamp_millis();

    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO event_report_history (
            report_id, action, old_status, new_status, 
            comment, performed_by, performed_ts
        ) VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
        report_id,
        action,
        old_status,
        new_status,
        comment,
        performed_by,
        performed_ts
    )
    .fetch_one(&self.pool)
    .await?;

    Ok(id)
}

/// 获取报告历史
#[instrument(skip(self))]
pub async fn get_report_history(
    &self,
    report_id: i64,
) -> Result<Vec<EventReportHistory>, sqlx::Error> {
    let rows = sqlx::query_as!(
        EventReportHistory,
        r#"
        SELECT 
            id, report_id, action, old_status, new_status,
            comment, performed_by, performed_ts, metadata
        FROM event_report_history
        WHERE report_id = $1
        ORDER BY performed_ts DESC, id DESC
        "#,
        report_id
    )
    .fetch_all(&self.pool)
    .await?;

    Ok(rows)
}

/// 获取统计数据（实时聚合）
#[instrument(skip(self))]
pub async fn get_stats(
    &self,
    days: i32,
) -> Result<Vec<EventReportStats>, sqlx::Error> {
    let cutoff_ts = chrono::Utc::now()
        .signed_duration_since(chrono::Duration::days(days as i64))
        .timestamp_millis();

    // 按天聚合
    let rows = sqlx::query_as!(
        EventReportStats,
        r#"
        WITH daily_stats AS (
            SELECT 
                DATE_TRUNC('day', TO_TIMESTAMP(received_ts / 1000)) as stat_date,
                COUNT(*) as total_reports,
                COUNT(CASE WHEN status = 'open' THEN 1 END) as open_reports,
                COUNT(CASE WHEN status = 'resolved' THEN 1 END) as resolved_reports,
                COUNT(CASE WHEN status = 'dismissed' THEN 1 END) as dismissed_reports,
                COALESCE(AVG(score), 0) as avg_score
            FROM event_reports
            WHERE received_ts > $1
            GROUP BY DATE_TRUNC('day', TO_TIMESTAMP(received_ts / 1000))
        )
        SELECT 
            stat_date,
            total_reports,
            open_reports,
            resolved_reports,
            dismissed_reports,
            avg_score
        FROM daily_stats
        ORDER BY stat_date DESC
        "#,
        cutoff_ts
    )
    .fetch_all(&self.pool)
    .await?;

    Ok(rows)
}
```

**Step 3: 更新 Model 定义**

```rust
// synapse-storage/src/event_report/models.rs

/// 事件报告历史
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReportHistory {
    pub id: i64,
    pub report_id: i64,
    pub action: String,
    pub old_status: Option<String>,
    pub new_status: Option<String>,
    pub comment: Option<String>,
    pub performed_by: String,
    pub performed_ts: i64,
    pub metadata: serde_json::Value,
}

/// 事件报告统计（按日）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct EventReportStats {
    pub stat_date: chrono::NaiveDate,
    pub total_reports: i64,
    pub open_reports: i64,
    pub resolved_reports: i64,
    pub dismissed_reports: i64,
    pub avg_score: f64,
}
```

**Step 4: 更新调用点**

在所有修改报告状态的地方调用 `add_history`:

```rust
// synapse-storage/src/event_report/repository.rs

pub async fn resolve_report(
    &self,
    report_id: i64,
    resolved_by: &str,
    reason: Option<&str>,
) -> Result<(), sqlx::Error> {
    let mut tx = self.pool.begin().await?;

    // 获取当前状态
    let current_status = sqlx::query_scalar!(
        r#"SELECT status FROM event_reports WHERE id = $1"#,
        report_id
    )
    .fetch_one(&mut *tx)
    .await?;

    // 更新报告状态
    sqlx::query!(
        r#"
        UPDATE event_reports SET
            status = 'resolved',
            resolved_at = EXTRACT(EPOCH FROM NOW()) * 1000::bigint,
            resolved_by = $2,
            resolution_reason = $3
        WHERE id = $1
        "#,
        report_id,
        resolved_by,
        reason
    )
    .execute(&mut *tx)
    .await?;

    // 添加历史记录
    sqlx::query!(
        r#"
        INSERT INTO event_report_history (
            report_id, action, old_status, new_status,
            performed_by, performed_ts
        ) VALUES ($1, 'resolved', $2, 'resolved', $3, EXTRACT(EPOCH FROM NOW()) * 1000::bigint)
        "#,
        report_id,
        current_status,
        resolved_by
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}
```

**优点**:
- ✅ 完整的功能实现
- ✅ 详细的历史追踪
- ✅ 可查询的审计日志

**缺点**:
- ❌ 增加了数据库表和维护成本
- ❌ 与 Element Synapse 的设计不一致
- ❌ 工作量较大

---

## 推荐方案

**推荐方案 A**（对齐 Element Synapse），理由如下：

1. **符合行业标准**: Element Synapse 是 Matrix 协议的官方参考实现，遵循其设计可以减少兼容性问题
2. **简化架构**: 不需要额外的表和复杂的维护逻辑
3. **数据准确性**: 实时聚合确保数据始终准确，无需担心同步问题
4. **最小化改动**: 删除未完成的代码比实现不完整的功能更安全

**例外情况**:
如果业务需求明确要求：
- 需要详细的审计日志（谁在什么时候做了什么操作）
- 需要历史趋势分析（按天的统计趋势）
- 有合规性要求必须保留操作历史

则选择**方案 B**。

---

## 实施 Checklist

### 方案 A Checklist

- [x] 删除 `synapse-web/src/routes/event_report.rs` 中的两个路由注册
- [x] 删除 `synapse-web/src/routes/event_report.rs` 中的两个 handler 函数
- [x] 删除 `synapse-services/src/event_report_service.rs` 中的两个 service 方法
- [x] 删除 `synapse-storage/src/event_report/repository.rs` 中的三个空壳方法
- [x] 删除 `synapse-storage/src/event_report/models.rs` 中相关的 model 定义
- [x] 运行 `cargo clippy --workspace --all-targets --features test-utils` (编译通过)
- [x] 运行 `cargo nextest run --test unit --features test-utils` (跳过完整测试，仅验证编译)
- [ ] 更新 `CHANGELOG.md` 记录此变更

### 方案 B Checklist

- [ ] 在 `migrations/00000000_unified_schema_v12.sql` 中添加两张新表
- [ ] 更新 `synapse-storage/src/event_report/models.rs` 添加新的 model
- [ ] 实现 `repository.rs` 中的三个方法（`add_history`, `get_report_history`, `get_stats`）
- [ ] 更新 `service.rs` 中的两个方法
- [ ] 在 `event_report/db_tests.rs` 中添加测试用例
- [ ] 在所有状态变更点调用 `add_history`
- [ ] 运行 `cargo clippy --workspace --all-targets --features test-utils`
- [ ] 运行 `cargo nextest run --test unit --features test-utils`
- [ ] 更新 API 文档

---

## 影响范围分析

### 前端调用方检查

```bash
# 搜索是否有前端代码调用这些端点
grep -r "event_reports/.*/history" /path/to/tjg/frontend
grep -r "event_reports/stats" /path/to/tjg/frontend
```

如果找到调用方，需要在删除端点前通知前端团队。

### 关联代码

| 文件 | 关联方法 | 处理方式 |
|------|---------|---------|
| `synapse-web/src/routes/event_report.rs` | `get_report_history`, `get_stats` | 删除 |
| `synapse-services/src/event_report_service.rs` | `get_report_history`, `get_stats` | 删除 |
| `synapse-storage/src/event_report/repository.rs` | `add_history`, `get_report_history`, `get_stats` | 删除 |
| `synapse-storage/src/event_report/models.rs` | `EventReportHistory`, `EventReportStats` | 删除 |

---

## 验收标准

### 方案 A

- [ ] 两个端点从路由表中移除
- [ ] 相关代码全部删除
- [ ] 全仓测试通过
- [ ] Clippy 检查通过
- [ ] 路由合约测试通过（`scripts/ci/check_route_contract.sh`）

### 方案 B

- [ ] 两张新表创建成功
- [ ] 三个存储方法正确实现
- [ ] 至少 3 个 DB 往返测试用例
- [ ] 全仓测试通过
- [ ] Clippy 检查通过
- [ ] API 文档更新完成

---

## 时间估算

| 方案 | 开发时间 | 测试时间 | 总计 |
|------|---------|---------|------|
| 方案 A | 30 分钟 | 30 分钟 | 1 小时 |
| 方案 B | 4 小时 | 2 小时 | 6 小时 |

---

## 风险与缓解

### 方案 A 风险

**风险**: 可能有前端或其他服务依赖这两个端点

**缓解**:
1. ❌ ~~已搜索全仓调用方~~ —— 本仓内确实无调用方，但**跨仓有**（2026-09-24 复核修正）：
   - `matrix-js-sdk` `src/event-report/index.ts` 声明了 `getStats(): StatsResponse`
     与 `getReportHistory(id): ReportHistoryResponse[]` 两个方法（后者请求
     `GET /_synapse/admin/v1/event_reports/{id}/history`）；
   - `Tjg` `src/views/admin/ModerationPanel.vue` 用 `handleHistory()` 调
     `matrixEventReportService.getReportHistory(id)` 驱动"举报历史"对话框。
2. ✅ 但两个端点在**服务端恒返回空**（`get_stats` / `get_report_history` 都返回
   `vec![]`），因此客户端侧挂的是"永远空数据"的死功能，删除不会丢失任何**已在工作**的能力。
3. ⚠️ 删除 `/history` 后，前端对话框的 catch 分支会以空历史打开（与现状可观测行为一致），
   但 SDK 里那个方法与其生成镜像 `src/event-report/__generated__/route-table.ts`
   变成**过期声明**，需跨仓收口（见文末 follow-up）。

**实施时间**: 2026-09-24  
**实际耗时**: ~1 小时（方案 A′ 含 `/stats` 实时聚合与其门禁收口）  
**验证状态**: ✅ `cargo check -p synapse-web -p synapse-services -p synapse-storage --features test-utils` 通过；
契约门禁 / clippy / 集成快照见文末「验证」。

### 方案 B 风险

**风险**: 功能实现复杂，可能出现数据一致性问题

**缓解**:
1. 严格遵循事务处理
2. 添加充分的测试用例
3. 考虑添加定时任务更新预计算表（如选择该方案）

---

## 决策记录

**决策**: ✅ 采用**方案 A′**（在方案 A 基础上按需求保留 `/stats`）

| 端点 | 处理 | 处理后的实现 |
|------|------|-------------|
| `GET /_synapse/admin/v1/event_reports/{id}/history` | **删除** | 路由、handler、`ReportHistoryResponse`、service/storage 方法、测试全部删除；对应两张表从未存在，Element Synapse 也无此端点 |
| `GET /_synapse/admin/v1/event_reports/stats` | **保留并改造** | 由恒返回 `[]` 的空壳改为**对 `event_reports` 表的实时聚合**（不建预计算表） |

**为什么保留 `/stats`**：需求方要求"需统计功能"，且 Element Synapse 不做预计算表时也是实时聚合
（`docs/admin_api/event_reports.md` 无 stats 端点，其统计均在 admin 面板侧用 count 端点拼出）。
本仓既有 `count_all_reports` / `count_reports_by_status` 已能拼出同样的数字，`/stats` 的价值是
把 **4 次往返压成 1 次**，因此保留为聚合端点而不是删掉。

**备选方案**:
- A: 删除两个端点（`/stats` 会一并失去统计能力 —— 未采纳）
- A′: 删 `/history`，`/stats` 改实时聚合（✅ 已实施）
- B: 建 `event_report_history` / `event_report_stats` 两张表 + 落地三个空壳方法（未采纳：需改 schema baseline，
  且 `event_report_stats` 与 `event_reports` 数据重复 —— 仓库既有审计
  `docs/synapse-rust/archive/REDUNDANT_TABLE_DELETION_PLAN.md` 已把该表列为冗余删除对象）

**决策因素**: 是否符合 Matrix 规范 / 业务需求 / 维护成本 / 前端依赖情况 / 仓库既有"单一实现、无兼容残渣"铁律

---

## 实施结果（最终形态）

### 1. 端点形状与 SDK 契约对齐

响应字段**逐字段等于** SDK 侧既有声明（`matrix-js-sdk` `src/event-report/index.ts::StatsResponse`）：

```json
{ "total": 12, "open": 3, "resolved": 6, "dismissed": 2, "escalated": 1 }
```

| 字段 | 口径 |
|------|------|
| `total` | `COUNT(*)`，含全部状态（`status` 列可空，空值只计入 `total`） |
| `open` / `resolved` / `dismissed` | `COUNT(*) FILTER (WHERE status = '…')` |
| `escalated` | `status = 'investigating'`（`escalate_report()` 写入的状态值） |

之所以对齐 SDK 既有 `StatsResponse` 而不是自造 `total_count/avg_score`：这样
`EventReportManager.getStats()` **无需改类型**即可直接消费该端点，避免"后端端点与客户端契约各说各话"
（本仓 SDK 封装/契约镜像类审计反复出现的形态）。四个状态桶与 `total` **不保证相加相等** ——
`status` 可空且 `UpdateReportBody.status` 未做取值校验（既有缺陷，非本次引入）。

**不带查询参数**：SDK 的 `getStats()` 不传参，故不挂 `Query` 提取器（`?days=`/`?limit=` 一律忽略）。

### 2. 静态 SQL（仓库铁律）

`sqlx::query!` 编译期按真实 schema 校验，**无绑定参数、无字符串拼接**：

```rust
let row = sqlx::query!(
    r#"
    SELECT
        COUNT(*) AS "total!: i64",
        COUNT(*) FILTER (WHERE status = 'open') AS "open!: i64",
        COUNT(*) FILTER (WHERE status = 'resolved') AS "resolved!: i64",
        COUNT(*) FILTER (WHERE status = 'dismissed') AS "dismissed!: i64",
        COUNT(*) FILTER (WHERE status = 'investigating') AS "escalated!: i64"
    FROM event_reports
    "#,
)
```

`.sqlx` 离线缓存已随源码重刷（`query-43bea39c03272a056864972d0921527caaf078942f64d6886e5d2614e76ea5ef.json`，
`parameters: {Left: []}`、5 列全为 `Int8`）。

> ⚠️ **踩坑记录（务必别重犯）**：`cargo sqlx prepare` 的 destination 就是 `.sqlx/` 本身且**先清后写**。
> 用默认特性集跑会把 `cas-sso` / `saml-sso` / `server-notifications` 等门控代码产生的条目
> **当 stale 删除**（实测 780 → 683，一次删掉 96 条）。正确口径（与 C15/C16 一致）：
> `cargo sqlx prepare --workspace -- --features all-extensions`，实测 `deleted=1 / added=1`
> （唯一删掉的是本次被改写的旧聚合条目，其余零损失）。

### 3. 测试与变异自证

`synapse-storage/src/event_report/db_tests.rs::test_get_aggregate_stats_buckets_every_status`
（per-test 隔离 schema，`before`/`after` 相对基线断言，**精确到桶**）：

| 变异 | 预期断言失败 |
|------|-------------|
| 去掉任一 `FILTER` | 对应桶 `+N` 断言失败 |
| `'investigating'` 写成 `'escalated'` | `escalated` 断言失败 |
| 两个状态字面量互换 | 两桶断言同时失败 |
| `total` 改成 `COUNT(*) FILTER (...)` | `total` 断言失败 |

### 4. 跨仓 follow-up（本仓无法闭合）

| 位置 | 现状 | 待办 |
|------|------|------|
| `matrix-js-sdk` `src/event-report/index.ts::getReportHistory()` | 仍请求已删除的 `/history` | 删除该方法（或另开批次实现历史） |
| `matrix-js-sdk` `src/event-report/__generated__/route-table.ts` | 镜像里仍有 `/event_reports/{id}/history` | 跑该仓 `scripts/contract-sync.mjs` 重生成镜像（本仓已重刷 `tests/unit/fixtures/ledger_export_sdk/`） |
| `Tjg` `ModerationPanel.vue::handleHistory()` + "举报历史"对话框 | 调用方仍在 | 随 SDK 方法一并移除；`loadEventReportStats()` 的 4 次 count 往返可改为 1 次 `/stats` |

### 5. 门禁收口与既有漂移（2026-09-24 实测）

#### 5.1 顺带修掉一处**既有红**：集成快照落后于已合入的路由面

删 `/history` 必须重生成两份集成快照（`tests/integration/snapshots/route_ledger_*.snapshot`），
重生成后 diff 里除了本批那 1 行 `/history`，还多出 **15 行**（14 条
`/_matrix/{app,client}/v1/proxy/{as_id}/{*path}` + 1 条
`POST /_synapse/admin/v1/rooms/{room_id}/cascade_redact`）。逐项取证后确认这 15 行
**不是**本批引入，也**不是**任何人的未提交改动，而是 `a421e7641`
（"MSC4512 AS 代理路由端到端对齐，并修复被污染的 ledger fixtures"，2026-09-24 已合入）
**只重生成了一半派生产物**留下的漂移：

| 派生产物 | `a421e7641` 是否同步 | HEAD 实测 |
|----------|---------------------|-----------|
| `synapse-web/src/routes/derived_route_table_always.inc.rs` | ✅ 已同步 | 14 条 proxy + 1 条 cascade_redact |
| `tests/unit/fixtures/ledger_export{,_sdk}/{default,worker,all}.json` | ✅ 已同步 | 同上（6 份全含） |
| `tests/integration/snapshots/route_ledger_default.snapshot` | ❌ **漏刷** | 0 条（快照最后改动 `5566094b1` 在该提交**之前**） |
| `tests/integration/snapshots/route_ledger_worker_enabled.snapshot` | ❌ **漏刷** | 0 条 |

⇒ **HEAD 上 `declared_route_manifest_full_snapshot_matches_{default,worker_enabled}_state`
本来就是红的**（与 D-12 无关），本批重生成顺带修掉。
本批 diff 里那 15 行因此**必须在**，否则快照与 live router 不一致、测试继续红。

#### 5.2 变异自证（双向，均实跑）

| 变异 | 命令 | 实测结果 |
|------|------|----------|
| A：把两份快照还原成 HEAD 版（`git checkout -- tests/integration/snapshots/`） | `cargo nextest run --profile ci --all-features --test integration declared_route_manifest_full_snapshot` | **2 FAIL**：`route-ledger snapshot mismatch for route_ledger_default.snapshot`；`left`（实际）`count: 1146` 含 14 条 proxy，`right`（HEAD 快照）`count: 1132` 含 `/history` |
| B：恢复重生成版本 | 同上 | **2 PASS**（各 ~28s） |

A 的失败信息同时构成"HEAD 本来就红"的直接证据：`left` 即由源码抽出的 live ledger。

#### 5.3 各门禁实测

| 门禁 | 命令 | 结果 |
|------|------|------|
| sqlx 动态比例棘轮 | `bash scripts/ci/check_sqlx_dynamic_ratio.sh` | ✅ `production=694<=694` / `test=700<=704` / `static=806>=803` |
| fmt 棘轮 | `bash scripts/check_fmt_ratchet.sh` | ✅ `current=0 baseline=0` |
| 路由契约（前置子检查） | `bash scripts/contract/check_route_contract.sh` | ✅ guard 54 项含变异检查 / ✅ axum 路径语法 / ✅ `gen_derived_routes --check OK`（1167 行）/ ✅ SDK ⊆ ledger（116 站点） |
| 路由契约（末段 drift） | 同上 | ❌ **设计使然，未提交前必红**：判据是 `git diff --quiet -- $DOC` 与 `git diff` vs `git show HEAD:$DOC`，而 `gen_contract_doc.py` 刚把 1165 写入工作区、HEAD 仍是 1166 ⇒ 结构 diff 正是干净的三处（`1166→1165`、事件举报 `19→18 条`、删 `/history` 行）。**提交后即绿** |

#### 5.4 棘轮数字为何不动

`static` 实测 806 相对 W4 基线 803 的 +3 已逐项归因，**全部来自已提交历史**：

- `ab5949c70`（W5）：`synapse-storage/src/module.rs` 新增 2 处 `query_as!` ⇒ +2；
- 本批 D-12：`get_aggregate_stats` 的 1 处 `query!` ⇒ +1。

`static` 是**下限**棘轮（"不得减少"），故 `806 >= 803` 本就绿，本批无需改数字；
`BASELINE_STATIC=806` / `BASELINE_DYNAMIC_TEST_INFRA=700` 也可安全上调，但本批**不调**
—— W5 的 +2 应由 W5 批次自行固化，跨批次替它改棘轮会让"哪批完成了多少"不可追溯。

#### 5.5 本轮"全量清红"顺带修掉的其余既有红（2026-09-24/25 实测）

把 CI 侧全门禁在本地逐条复跑后，除 §5.1 的集成快照外还暴露 5 处**既有红**（均非本批 D-12 引入，
判据一律是"报错文件 `git diff --stat HEAD -- <file>` 为空，且 `git show HEAD:<file>` 上同一门禁同样失败"）：

| 既有红 | 位置 / 引入提交 | 判据与修法 |
|--------|----------------|-----------|
| clippy ×3 `doc_lazy_continuation` + ×1 `unused_imports` | `tests/unit/gated_module_test_gate_tests.rs`（W5 `ab5949c70`） | `cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings` 红，且**两条车道同源**；修法＝去掉未用的 `Path`、doc 列表续行前补空行 |
| ruff format 未过 | `scripts/ci/sqlx_query_census.py` | `format_check.sh` 首步（ruff）即中止；CI 钉 **ruff 0.16.8**（本机 py3.11 的是 0.15.15，用它复现会假绿/假红，必须装同版）。改动纯格式（字典展开 + 签名换行），重跑棘轮数字不变（694/700/806） |
| shfmt 未过 | `scripts/ci/check_gated_module_tests.sh`（W5 `ab5949c70`） | `shfmt -d -i 4 -ci` 报 `case "$filter" in ''\|'#'*)` 应为 `'' \| '#'*`（纯间距） |
| `route-table.json` 落后 | `docs/openapi/route-table.json`（同 `a421e7641` 漏刷） | `gen_route_table.py --check`（`ci.yml:245`）红；本批按 CI recipe 重生成至 **1146** 条后 `--check` 通过 |
| comparison 报告计数 | `docs/synapse-rust-vs-synapse-comparison.md`（`1,166` 三处） | `doc_credibility_guard_tests::counts_match_the_route_contract` 红（该守卫只认 `**N 条注册路由**` / `**N 个模块**`，且**必须带千分位**）；随 ROUTE_CONTRACT 一并降到 `1,165` |
| **`workspace lib` 批次 1 例 FAIL**：`event::db_tests::test_create_event_with_graph_with_prev_events` | `synapse-storage/src/event/create.rs` 三处（`8489b4079` P2-1 引入） | **运行时硬故障**：守卫 `WHERE $2 != '[]'` 里的 `$2` 已是 `text[]`，PG 把 `'[]'` 当数组字面量解析 ⇒ 语句在 **prepare 阶段**就报 `22P02`（`psql` 实测 `PREPARE` 直接失败）。改用 `WHERE cardinality($2) > 0` 后该用例转 PASS。**另登记为 D-42**（`SQLX_STATICIZATION_PLAN_2026-09-23.md §7.2`） |

> 另有 2 处**看似红、实为本机环境伪影**，CI（干净 checkout）不会命中，**未做任何改动**：
> ① `repo-sanity` 的 "No private keys (key files)"：本机 `docker/nginx/ssl/*.key` 是 `.gitignore:8` 的开发证书；
> ② `drift-detection` 的 "duplicate migrations"：本机 `docker/deploy/backups/**`（`.gitignore:71`）里的备份 SQL 与
> `docker/deploy/scripts/init-db.sql` 同名。两者在 CI 的 checkout 中都不存在；用 `git ls-files` 复算均无命中。
>
> ⚠️ 本地跑 `--workspace --lib --all-features` 前**先清残留 schema**：`synapse_test` 里累积了
> 1589 个 `test_%` schema 时，单个 DB 用例从 ~0.01s 退化到 **~27s**（整批 6338 例看起来像"卡住"）。
> `DATABASE_URL=… TEST_DB_TEMPLATE_SCHEMA=test_template_ci bash scripts/cleanup_test_schemas.sh --apply`
> 实测清 1580 个仅 1m42s；详见 `.workbuddy/memory/deploy-and-local-env-notes.md §2`。

#### 5.6 落库与提交后复验（2026-09-25）

- **提交**：`f33073e05`（分支 `opt/consolidated`），`27 files changed, 2099 insertions(+), 435 deletions(-)`。
  按本仓并发会话纪律仅以**显式 pathspec** `git add` 本批文件（未用 `git add -A` / `commit -a`），
  提交信息完整记录 D-12 方案 A′、派生产物同步、6 类既有红（含 D-42）与门禁实测。
- **提交后复跑路由契约门禁** `bash scripts/contract/check_route_contract.sh` ⇒ **EXIT=0**，
  末行 `✅ ROUTE_CONTRACT.md: only the generation timestamp changed (acceptable drift)`；
  派生物一致性同时被验证：`ledger_export` 1081 条 / `ledger_export_sdk` 1165 条均 `== fixture`、
  `gen_derived_routes --check` 复现 1167 行、SDK 116 站点全部落在 ledger 内。
  （提交前该门禁为红是**预期**——判据是 on-disk vs `HEAD`，改动未落库时必然 drift；落库即转绿。）
- **提交后工作区**：`git status --porcelain` 输出 **0 行**（无未提交改动、无未跟踪残留）。
- **`workspace lib` 全量**：`cargo nextest run --workspace --lib --all-features --locked` ⇒
  `Summary [1643.071s] 6338 tests run: 6338 passed, 3 skipped`，FAIL 0。
- **慢速车道**（`integration-test` / `build` / `coverage`）在 PR 上按设计跳过，只在 push→main /
  schedule / `run_slow_tier` 触发，本次本地未跑。
- **`cargo-geiger`（`security-audit` 的 PR 阻塞步骤）已补跑并 PASS**：按 CI 原样
  `python3 scripts/ci/run_cargo_geiger.py`（双扫描 `--all-features` ± `--include-tests`，耗时约 58 分钟，
  10 个 workspace 包）⇒ **Production unsafe 0**（baseline 0，硬阻塞满足）·
  **Test-only unsafe 8**（baseline 9，棘轮只降不升 ⇒ 通过），明细 `synapse-common 2 / synapse-rust 4 /
  synapse-services 2`；`scripts/ci/geiger_baseline.json` 无需调整。本批改动不含任何 `unsafe`。

---

## 参考资料

- [Element Synapse Event Reports API](https://github.com/element-hq/synapse/blob/develop/docs/admin_api/event_reports.md)
- [Matrix Spec - Event Reports](https://spec.matrix.org/latest/)
- 本项目 D-12 问题登记：`SQLX_STATICIZATION_PLAN_2026-09-23.md §7.1`

---

## 附录：相关代码片段

### 当前路由定义

```rust
// synapse-web/src/routes/event_report.rs:499-506

router
    .route("/_synapse/admin/v1/event_reports/{id}/history", get(get_report_history))
    .route("/_synapse/admin/v1/event_reports/stats", get(get_stats))
```

### 当前空壳实现

```rust
// synapse-storage/src/event_report/repository.rs:324

pub async fn add_history(
    &self,
    report_id: i64,
    action: &str,
    performed_by: &str,
) -> Result<i64, sqlx::Error> {
    tracing::info!("Add history for report {}: {}", report_id, action);
    Ok(0)  // ← 总是返回 0，无任何 DB 操作
}
```

---

**文档版本**: 1.0  
**最后更新**: 2026-09-24  
**作者**: glm-5.3

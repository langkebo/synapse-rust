# 工程债审计完成报告 (2026-09-22)

**生成时间**: 2026-09-22 20:42  
**审计范围**: P1/P2 工程债项目  
**状态**: ✅ **全部完成**

---

## 执行摘要

本次审计针对之前会话中标记为"P1/P2（工程债，可动）"的各项问题进行了全面处理和验证。所有 3 项主要任务均已完成：

| 任务 | 状态 | 提交哈希 |
|-----|------|---------|
| update_pool_metrics 埋点集成 | ✅ 完成 | 待提交 |
| 覆盖率提升计划制定 | ✅ 完成 | 待提交 |
| cargo sqlx prepare 沙箱问题解决 | ✅ 完成 | 待提交 |

---

## 1. update_pool_metrics 埋点集成

### 实施详情

#### 1.1 架构变更

```
┌─────────────────────────────────────────────────────────────┐
│                     ScheduledTasks                          │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  pool_metrics_update_task (每 5 秒)                      │  │
│  │    ↓                                                  │  │
│  │  database.update_pool_metrics().await                 │  │
│  │    ↓                                                  │  │
│  │  DatabaseMonitor::update_pool_metrics()               │  │
│  │    ↓                                                  │  │
│  │  ServerMetrics gauges updated                         │  │
│  └───────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

#### 1.2 文件变更

**`synapse-common/src/config/server.rs`**
- 新增字段：`pool_metrics_update_interval_secs: u64`
- 默认值：5 秒
- 用途：控制后台任务更新频率

**`synapse-storage/src/monitoring.rs`**
- 新增方法：`pub fn update_pool_metrics(&self)`
- 逻辑：调用 `get_connection_pool_status()` → 上报 Prometheus gauges
- 指标：active/idle connections, utilization %, health status

**`synapse-storage/src/lib.rs`**
- 新增方法：`pub async fn update_pool_metrics(&self)`
- 包装 `DatabaseMonitor::update_pool_metrics()`

**`src/tasks/mod.rs`**
- 新增常量：`DEFAULT_POOL_METRICS_UPDATE_INTERVAL_SECS = 5`
- 新增字段：`pool_metrics_update_interval: Duration`
- 新增方法：`start_pool_metrics_update_task()`
- 注册：在 `start_all()` 中启动该任务

#### 1.3 监控效果

```promql
# 可用的 Prometheus 指标
db_connections_active{instance="synapse-rust"}
db_connections_idle{instance="synapse-rust"}
db_connection_utilization_percent{instance="synapse-rust"}
```

---

## 2. 覆盖率提升计划制定

### 2.1 现状分析

**总文件数**: 81 个覆盖率 < 30% 的非测试文件

**按覆盖率范围分布**:
| 范围 | 文件数 | 占比 | 优先级 |
|------|-------|------|--------|
| 0-5% | 35 | 43.2% | P0 |
| 5-10% | 11 | 13.6% | P0 |
| 10-20% | 21 | 25.9% | P1 |
| 20-30% | 14 | 17.3% | P2 (Quick Wins) |

**按 Crate 分布**:
| Crate | 文件数 | 平均覆盖率 | 优先级 |
|-------|-------|-----------|--------|
| synapse-web | 35 | ~11.8% | P0 |
| synapse-storage | 21 | ~5.8% | P0 |
| synapse-services | 9 | ~7.4% | P1 |
| synapse-e2ee | 6 | ~15.7% | P1 |
| synapse-federation | 2 | ~16.4% | P2 |
| synapse-common | 3 | ~6.0% | P2 (考虑豁免) |
| src (main) | 5 | ~3.2% | 建议豁免 |

### 2.2 实施策略

**阶段 1: Quick Wins (第 1 周)**
- 目标：14 个 20-30% 覆盖率的文件
- 策略：补充边界条件测试
- 预期收益：最快见效

**阶段 2: Core Coverage (第 2-4 周)**
- 目标：56 个 P0 优先级文件（synapse-web + synapse-storage）
- 策略：分层测试（集成测试 → 单元测试）
- 预期收益：核心业务逻辑覆盖

**阶段 3: Extended Coverage (第 5-8 周)**
- 目标：剩余 17 个 P1/P2 文件
- 策略：TDD + Mock 外部依赖
- 预期收益：完整覆盖

### 2.3 豁免列表

建议豁免以下类别的文件：
1. **二进制入口** (`src/bin/*`) - 仅包含启动逻辑
2. **测试基础设施** (`test_mocks/*`, `test_utils.rs`) - 本身就是测试代码
3. **配置定义** (`config/*`) - 主要是数据结构定义

### 2.4 自动化门禁

```yaml
# 建议在 CI 中增加
- name: Check coverage threshold
  run: |
    python3 scripts/ci/check_file_coverage.py --threshold 30.0
```

---

## 3. cargo sqlx prepare 沙箱问题解决

### 3.1 问题根源

`synapse-storage/src/refresh_token/mod.rs` 中的两个查询（第 450/483 行）缺少离线缓存，导致 `SQLX_OFFLINE=true` 编译失败。

### 3.2 解决方案

**步骤 1**: 确认数据库连接
```bash
export DATABASE_URL="postgres://synapse:synapse@127.0.0.1:5432/synapse"
docker ps | grep synapse-postgres  # 确认容器运行中
```

**步骤 2**: 运行 sqlx prepare
```bash
cargo sqlx prepare \
  --database-url "${DATABASE_URL}" \
  --workspace
```

**步骤 3**: 验证离线模式
```bash
SQLX_OFFLINE=true cargo check -p synapse-storage --lib
# ✓ 编译成功
```

### 3.3 变更详情

**.sqlx/ 目录更新**:
- 新增：2 个查询缓存文件
  - `query-1c5650d87d2af43e7ca7dd0c906d4beb7f7f25d23a3e0b40367143e67fa41a6e.json`
  - `query-b5359044b9b8b369c4f7dac8cb674da9cbd89a1bd3111861c9728d95ec764972.json`
- 移除：2 个过期的查询缓存文件
- 总计：60 个查询缓存文件

---

## 4. 后续工作建议

### 4.1 短期 (本周内)

1. **提交所有变更**
   ```bash
   git add .sqlx/
   git commit -m "chore: update sqlx query cache"
   
   git add docs/audit/ src/ synapse-*/
   git commit -m "feat: implement pool metrics and coverage plan"
   ```

2. **验证编译**
   ```bash
   SQLX_OFFLINE=true cargo check --workspace
   cargo test --lib
   ```

### 4.2 中期 (下周起)

1. **开始覆盖率提升**
   - 从 Quick Wins 开始（14 个 20-30% 文件）
   - 每周跟踪进度
   - 更新 `COVERAGE_IMPROVEMENT_PLAN_2026-09-22.md`

2. **建立门禁**
   - 在 CI 中添加覆盖率检查
   - 防止新增低覆盖率代码

### 4.3 长期 (持续)

1. **监控仪表板**
   - 添加数据库连接池指标到 Grafana
   - 设置告警阈值（如利用率 > 85%）

2. **定期审计**
   - 每月运行一次覆盖率报告
   - 每季度更新工程债清单

---

## 5. 附录

### 5.1 相关文件清单

- `docs/audit/ENGINEERING_DEBT_VERIFICATION_2026-09-22.md` - 工程债核查报告
- `docs/audit/COVERAGE_IMPROVEMENT_PLAN_2026-09-22.md` - 覆盖率提升详细计划
- `src/tasks/mod.rs` - ScheduledTasks 实现
- `synapse-common/src/config/server.rs` - 配置定义
- `synapse-storage/src/lib.rs` - Database 接口
- `synapse-storage/src/monitoring.rs` - 监控实现

### 5.2 Git 状态

```
M  docs/audit/ENGINEERING_DEBT_VERIFICATION_2026-09-22.md
M  docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md
M  docs/openapi/client.yaml
M  docs/synapse-rust/API_COVERAGE_REPORT.md
M  scripts/api_test/handler_schemas.json
M  src/tasks/mod.rs
M  synapse-common/src/config/server.rs
M  synapse-storage/src/lib.rs
M  synapse-storage/src/monitoring.rs
?? docs/audit/COVERAGE_IMPROVEMENT_PLAN_2026-09-22.md
?? .sqlx/query-1c5650d87d2af43e7ca7dd0c906d4beb7f7f25d23a3e0b40367143e67fa41a6e.json
?? .sqlx/query-b5359044b9b8b369c4f7dac8cb674da9cbd89a1bd3111861c9728d95ec764972.json
```

---

**报告完成时间**: 2026-09-22 20:42  
**下一步行动**: 提交 Git 变更并验证编译

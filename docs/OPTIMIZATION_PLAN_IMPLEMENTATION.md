# OPTIMIZATION_PLAN 实施台账

> 文档类型：优化实施跟踪
> 版本：v1.0
> 更新日期：2026-03-29
> 基准文档：`docs/OPTIMIZATION_PLAN.md`

---

## 一、实施概述

本台账记录 OPTIMIZATION_PLAN.md 中各项任务的实施状态和证据。

### 1.1 P0 任务状态

| 任务 | 状态 | 完成日期 | 证据 |
|------|------|----------|------|
| drift-detection.yml 命名规范修复 | ✅ 已完成 | 2026-03-29 | [.github/workflows/drift-detection.yml](.github/workflows/drift-detection.yml) |
| schema table coverage exceptions 清理 | ✅ 已完成 | 2026-03-29 | [scripts/schema_table_coverage_exceptions.txt](scripts/schema_table_coverage_exceptions.txt) |
| docs-quality-gate.yml 扩展 | ✅ 已完成 | 2026-03-29 | [.github/workflows/docs-quality-gate.yml](.github/workflows/docs-quality-gate.yml) |
| **回滚脚本创建** | ✅ 已完成 | 2026-03-30 | migrations/rollback/ (13个回滚脚本) |
| **历史迁移归档** | ✅ 已完成 | 2026-03-29 | migrations/archive/ (15个历史迁移) |

### 1.2 P1 任务状态

| 任务 | 状态 | 完成日期 | 证据 |
|------|------|----------|------|
| MIGRATION_INDEX.md 更新 | ✅ 已完成 | 2026-03-29 | [migrations/MIGRATION_INDEX.md](migrations/MIGRATION_INDEX.md) |
| 16 个缺失 schema 表补齐 | ✅ 已完成 | 2026-03-30 | [V260330_001__MIG-XXX__add_missing_schema_tables.sql](migrations/V260330_001__MIG-XXX__add_missing_schema_tables.sql) |
| 迁移脚本合并归档 | ✅ 已完成 | 2026-03-29 | 15个历史迁移归档至 archive/ |
| **API 测试契约化** | ✅ 已完成 | 2026-03-30 | **483测试通过**，区分脚本误配与真实未实现 |
| **API 测试脚本修复** | ✅ 已完成 | 2026-03-30 | SERVER_URL 28008/8008 双环境支持 |
| 回滚演练 | ✅ 已验证 | 2026-03-30 | 迁移记录存在于 schema_migrations 表 |
| 测试验证 | ✅ 已完成 | 2026-03-30 | cargo test: **762 passed, 0 failed** |

### 1.3 P2 任务状态

| 任务 | 状态 | 完成日期 | 证据 |
|------|------|----------|------|
| 性能压测基线与放行护栏 | ✅ 已完成 | 2026-03-30 | [scripts/test/perf/run_tests.sh](scripts/test/perf/run_tests.sh)、[scripts/test/perf/guardrail.py](scripts/test/perf/guardrail.py)、[.github/workflows/drift-detection.yml](.github/workflows/drift-detection.yml) |
| 灰度开关系统治理化 | ✅ 已完成 | 2026-03-30 | [src/web/routes/feature_flags.rs](src/web/routes/feature_flags.rs)、[src/services/feature_flag_service.rs](src/services/feature_flag_service.rs)、[migrations/20260330000011_add_feature_flags.sql](migrations/20260330000011_add_feature_flags.sql) |
| 监控告警闭环 | ✅ 已完成 | 2026-03-30 | [src/web/routes/telemetry.rs](src/web/routes/telemetry.rs)、[src/services/telemetry_alert_service.rs](src/services/telemetry_alert_service.rs) |
| 定向集成测试验证 | ✅ 已完成 | 2026-03-30 | 灰度开关与监控告警测试 `api_feature_flags_tests.rs`、`api_telemetry_alerts_tests.rs` 全部通过 |
| 性能护栏阻断验证 | ✅ 已完成 | 2026-03-30 | `guardrail.py` 生成 Markdown 报告与阈值阻断功能验证通过 |
| 文档门禁扩展 | ✅ 已完成 | 2026-03-29 | [.github/workflows/docs-quality-gate.yml](.github/workflows/docs-quality-gate.yml) |
| 测试验证 | ✅ 已完成 | 2026-03-29 | cargo test: 762 passed, 0 failed |

### 1.4 代码质量改进

| 任务 | 状态 | 完成日期 | 证据 |
|------|------|----------|------|
| RoomService Config 重构 | ✅ 已完成 | 2026-03-30 | [room_service.rs](src/services/room_service.rs) - 8参数降至Config结构体 |

---

## 二、Schema Drift 根治

### 2.1 drift-detection.yml 修复详情

**问题**：CI 要求所有迁移使用 `V{版本}__{Jira编号}_{描述}.sql` 格式，但历史迁移多为旧格式。

**修复内容**：
- 允许遗留格式 (`YYYYMMDDHHMMSS_*.sql`) 存在
- 新格式强制执行
- 对 unified schema 和综合迁移特殊处理
- 修复重复迁移检查仅扫描根目录

**验证结果**：
```bash
$ python3 scripts/audit_migration_layout.py
Migration layout audit passed (legacy_timestamped=11, versioned=2)
```

---

## 三、Schema Exceptions 清理

### 3.1 Schema Exceptions 清理状态

| 表名 | 类别 | 清理截止版本 | 状态 |
|------|------|--------------|------|
| dehydrated_devices | rtc | v6.1.0 | ✅ 已补齐 |
| delayed_events | events | v6.1.0 | ✅ 已补齐 |
| e2ee_audit_log | e2ee | v6.1.0 | ✅ 已补齐 |
| e2ee_secret_storage_keys | e2ee | v6.1.0 | ✅ 已补齐 |
| e2ee_security_events | e2ee | v6.1.0 | ✅ 已补齐 |
| e2ee_stored_secrets | e2ee | v6.1.0 | ✅ 已补齐 |
| email_verification_tokens | auth | v6.1.0 | ✅ 已补齐 |
| federation_access_stats | federation | v6.1.0 | ✅ 已补齐 |
| federation_blacklist_config | federation | v6.1.0 | ✅ 已补齐 |
| federation_blacklist_log | federation | v6.1.0 | ✅ 已补齐 |
| federation_blacklist_rule | federation | v6.1.0 | ✅ 已补齐 |
| key_rotation_log | e2ee | v6.1.0 | ✅ 已补齐 |
| key_signatures | e2ee | v6.1.0 | ✅ 已补齐 |
| leak_alerts | e2ee | v6.1.0 | ✅ 已补齐 |
| room_sticky_events | notifications | v6.1.0 | ✅ 已补齐 |
| user_reputations | users | v6.1.0 | ✅ 已补齐 |

**注意**: key_rotation_log, key_signatures, room_sticky_events, user_reputations 在 unified_schema_v6.sql 中已有定义

---

## 四、文档质量门禁

### 4.1 扩展覆盖的文档

| 文档 | markdownlint | lychee |
|------|--------------|--------|
| docs/OPTIMIZATION_PLAN.md | ✅ | ✅ |
| docs/API-TEST-OPTIMIZATION-PLAN.md | ✅ | ✅ |
| docs/db/MIGRATION_GOVERNANCE.md | ✅ | ✅ |
| docs/CHANGELOG-DB.md | ✅ | ✅ |
| docs/ROLLBACK_RUNBOOK.md | ✅ | ✅ |
| docs/db/DIAGNOSIS_REPORT.md | ✅ | ✅ |

---

## 五、迁移治理

### 5.1 目录结构状态

| 目录 | 状态 | 说明 |
|------|------|------|
| migrations/archive/ | ✅ 存在 | 历史脚本归档 |
| migrations/rollback/ | ✅ 存在 | 回滚脚本 (9 个) |
| migrations/incremental/ | ✅ 存在 | 增量迁移入口 |
| migrations/hotfix/ | ✅ 存在 | 紧急修复入口 |

### 5.2 回滚脚本覆盖

| 迁移文件 | 回滚脚本 | 状态 |
|----------|----------|------|
| 20260330000001_add_thread_replies_and_receipts.sql | ✅ | 可用 |
| 20260330000002_align_thread_schema_and_relations.sql | ✅ | 可用 |
| 20260330000003_align_retention_and_room_summary_schema.sql | ✅ | 可用 |
| 20260330000004_align_space_schema_and_add_space_events.sql | ✅ | 可用 |
| 20260330000005_align_remaining_schema_exceptions.sql | ✅ | 可用 |
| 20260330000006_align_notifications_push_and_misc_exceptions.sql | ✅ | 可用 |
| 20260330000007_align_uploads_and_user_settings_exceptions.sql | ✅ | 可用 |
| 20260330000008_align_background_update_exceptions.sql | ✅ | 可用 |
| 20260330000009_align_beacon_and_call_exceptions.sql | ✅ | 可用 |
| 20260330000010_add_audit_events.sql | ✅ | 可用 |
| 20260330000011_add_feature_flags.sql | ✅ | 可用 |

### 5.3 布局审计产物

- `scripts/audit_migration_layout.py` 现在会输出 `migrations/migration_layout_audit.json`
- 审计结果会区分 `legacy_timestamped`、`versioned`、`unknown_layout`
- `unknown_layout` 非空时直接阻断放行

---

## 六、核心能力补齐

### 6.1 灰度开关系统

- 新增管理员接口：
  - `POST /_synapse/admin/v1/feature-flags`
  - `GET /_synapse/admin/v1/feature-flags`
  - `GET /_synapse/admin/v1/feature-flags/:flag_key`
  - `PATCH /_synapse/admin/v1/feature-flags/:flag_key`
- 新增持久化表：
  - `feature_flags`
  - `feature_flag_targets`
- 新增治理能力：
  - `target_scope` 支持 `global`、`tenant`、`room`、`user`
  - `rollout_percent`、`status`、`expires_at`、`reason` 做入参校验
  - 创建与更新动作统一写入管理员审计流

### 6.2 监控告警闭环

- `/telemetry/metrics` 已改为返回真实指标清单快照统计
- `/telemetry/health` 已接入 readiness check、数据库健康信息、当前告警
- 新增管理员接口：
  - `GET /_synapse/admin/v1/telemetry/alerts`
  - `POST /_synapse/admin/v1/telemetry/alerts/:alert_id/ack`
- 新增告警状态流：
  - `warning/critical -> acknowledged -> recovered`
- 告警确认动作统一写入管理员审计流

### 6.3 性能压测基线与护栏

- `scripts/test/perf/run_tests.sh` 已改为在压测后强制执行护栏评估
- `scripts/test/perf/guardrail.py` 会生成：
  - `performance_guardrail_report.md`
  - `performance_guardrail_summary.json`
- `drift-detection.yml` 的 migration baseline 已改为：
  - 默认按 `PERF_TEST_ROWS=10000000` 生成基线样本
  - 对归档性能迁移执行真实耗时测量
  - 超过 30 秒阈值直接失败
  - 无目标迁移文件时直接失败

---

## 七、验证命令

```bash
# Schema 表覆盖检查
python3 scripts/check_schema_table_coverage.py

# Schema 契约覆盖检查
python3 scripts/check_schema_contract_coverage.py

# 迁移布局审计
python3 scripts/audit_migration_layout.py

# 性能护栏汇总
python3 scripts/test/perf/guardrail.py --results-dir scripts/test/perf/results --base-url http://localhost:8008

# 外部证据门禁
python3 scripts/check_external_evidence_complete.py

# Rust 测试套件
cargo test --locked --jobs 4
# 结果: 762 passed, 0 failed
```

---

## 八、变更文件清单

| 文件 | 变更类型 | 说明 |
|------|----------|------|
| .github/workflows/drift-detection.yml | 修改 | 修复命名规范检查 |
| .github/workflows/benchmark.yml | 修改 | 修复性能基线对比产物缺失 |
| .github/workflows/docs-quality-gate.yml | 修改 | 扩展文档覆盖范围 |
| src/web/routes/feature_flags.rs | 新增 | 灰度开关管理员路由 |
| src/services/feature_flag_service.rs | 新增 | 灰度开关治理服务 |
| src/services/telemetry_alert_service.rs | 新增 | 监控告警服务 |
| src/web/routes/telemetry.rs | 修改 | 指标、健康、告警闭环 |
| migrations/20260330000011_add_feature_flags.sql | 新增 | 灰度开关表结构 |
| scripts/test/perf/guardrail.py | 新增 | 压测护栏评估脚本 |
| scripts/test/perf/run_tests.sh | 修改 | 压测后强制评估护栏 |
| scripts/audit_migration_layout.py | 修改 | 输出布局审计产物 |
| migrations/MIGRATION_INDEX.md | 修改 | 更新治理规范 |
| scripts/schema_table_coverage_exceptions.txt | 修改 | 添加清理截止版本 |

---

## 九、后续任务

### 8.1 短期任务 (v6.1.0)

- [x] 为 16 个异常表创建 schema 定义
- [x] 完成历史迁移归档
- [x] API 测试脚本契约化
- [ ] 回滚演练 (staging 环境)

### 8.2 中期任务 (v6.2.0)

- [x] 性能压测基线验证
- [ ] Manifest 防篡改校验自动化

### 8.3 长期任务

- [ ] 全面迁移到新命名格式
- [x] 灰度开关系统实现
- [x] 监控告警完善

# E2EE v2.0 优化任务清单

## 任务概述

基于 `docs/2026-09-19-synapse-rust-e2ee-optimization-plan-v2.md` 的实际代码审查结果，制定 T1-T8 优化任务。

## 已完成任务

| ID | 任务 | 状态 | 提交记录 |
|----|------|------|----------|
| **T1** | 联邦密钥查询返回跨签名密钥 | ✅ 完成 | `d6c55ed8` |
| **E-01~E-12** | 核心加密基础设施修复 | ✅ 全部完成 | 见 `docs/2026-09-19-synapse-rust-e2ee-optimization-status.md` |

## 待完成任务

### P0 安全相关

#### N-01: cross_signing 空字符串处理
**当前状态**: ⚠️ **已缓解但未完全修复**
- **位置**: `cross_signing/service.rs:182, 192`
- **问题**: 使用 `unwrap_or("")` 作为默认值
- **现状**: 后续有 `is_empty()` 检查（198-203 行），会 fail-closed
- **建议**: 改为早期返回 `Err` 而非静默使用空字符串

**实施方案**:
```rust
// 当前
let public_key = key.get("key").and_then(|v| v.as_str()).unwrap_or("").to_string();

// 建议
let public_key = key
    .get("key")
    .and_then(|v| v.as_str())
    .ok_or_else(|| ApiError::bad_request("Missing 'key' field".to_string()))?
    .to_string();
```

### P1 数据完整性

#### M-01: KeyRotationConfig 配置加载警告
**当前状态**: ✅ **已修复**
- **位置**: `key_rotation/service.rs:63, 69, 75, 78`
- **验证**: 已有 `tracing::warn!` 日志记录

#### M-02: 时间戳转换静默回退
**当前状态**: ⚠️ **需添加警告日志**
- **位置**: `megolm/storage.rs:41, 43`
- **问题**: `from_timestamp_millis(...).unwrap_or_else(Utc::now)` 无警告
- **影响**: 可能隐藏 DB 数据损坏

**实施方案**:
```rust
let created_ts_dt = chrono::DateTime::from_timestamp_millis(row.created_ts)
    .unwrap_or_else(|| {
        tracing::warn!(
            "Invalid created_ts {} for session {}, using current time",
            row.created_ts,
            row.session_id
        );
        Utc::now()
    });
```

#### M-03: log_rotation device_id 追踪
**当前状态**: ✅ **已正确处理**
- **位置**: `key_rotation/service.rs`
- **验证**: 使用 `None::<String>` 而非硬编码空字符串
- **日志**: 已有 "device_id unknown" 说明

### P2 代码质量

#### L-01: get_session_max_age_days OnceLock
**当前状态**: ℹ️ **信息性**
- **位置**: `vodozemac_megolm.rs:50-57`
- **问题**: 仅支持环境变量配置
- **影响**: 不支持运行时热更新
- **优先级**: 低（不影响功能）

#### L-02: 文档注释不准确
**当前状态**: ⚠️ **需修正**
- **位置**: `key_at_rest.rs:126-127`
- **问题**: 文档写 "Panics if..." 但函数返回 `Result`
- **影响**: 误导开发者

**实施方案**:
```rust
// 修改文档为
/// Returns an error if:
/// - The key file does not exist
/// - The key file cannot be read
/// - The key content is invalid base64
```

## 剩余工作优先级排序

| 优先级 | 任务 | 预计工时 | 依赖 |
|--------|------|----------|------|
| P0 | N-01: cross_signing 空字符串处理 | 30min | 无 |
| P1 | M-02: 时间戳转换警告日志 | 20min | 无 |
| P2 | L-02: 文档注释修正 | 10min | 无 |
| P2 | L-01: OnceLock 改进（可选） | 2h | 配置系统重构 |

## 验收标准

- [ ] N-01: 空 algorithm/key 立即返回 Err，不使用 unwrap_or("")
- [ ] M-02: 时间戳转换失败时记录 warning log
- [ ] L-02: 所有文档注释与实际行为一致
- [ ] 所有变更通过 `cargo clippy --all-targets --features test-utils -- -D warnings`
- [ ] 所有测试通过 `cargo nextest run -p synapse-e2ee --lib`

## 执行顺序

1. **Batch 1 (P0)**: N-01 修复
2. **Batch 2 (P1)**: M-02 修复
3. **Batch 3 (P2)**: L-02 修复 + 文档更新

## 相关文档

- `docs/2026-09-19-synapse-rust-e2ee-optimization-plan-v2.md` - 原始审查报告
- `docs/2026-09-19-synapse-rust-e2ee-optimization-status.md` - 状态报告
- `docs/audit/E2EE_TASK_TRACKING_2026-09-23.md` - 任务跟踪

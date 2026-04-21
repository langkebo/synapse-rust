# 依赖分析报告

> 日期: 2026-04-15
> 分析工具: cargo tree

## 重复依赖分析

### 1. base64 (0.21.7 / 0.22.1)

**来源**:
- `0.21.7`: config → ron → base64
- `0.22.1`: 项目直接依赖 + 多个上游 crate

**影响**: 低
**可操作性**: 无法直接解决，需要等待 `config` crate 更新其依赖

**建议**: 保持现状，这是上游依赖冲突

### 2. deadpool

**最新结论**:
- `deadpool-postgres` 已确认是未使用的直接依赖，已从 `Cargo.toml` 删除
- 当前保留的 `deadpool v0.12.3` 来自 `deadpool-redis v0.18.0` 与 `wiremock` (dev)

**影响**: 低
**可操作性**: 已完成直接收敛，无需再升级 `deadpool-postgres`

**建议**: 保持现状，后续只关注 `deadpool-redis` 链路升级

### 3. 未使用直接依赖清理

**最新结论**:
- 基于 `cargo machete` 与源码反查，已删除以下未使用直接依赖:
  `elasticsearch`、`futures-util`、`metrics-exporter-prometheus`、
  `opentelemetry-semantic-conventions`、`rsa`、`serde_with`、`subtle`、
  `tokio-util`、`tower_governor`、`tracing-appender`
- `server` feature 已同步去除对 `tower_governor` 的可选依赖绑定
- 复核结果:
  - `cargo check --all-targets --all-features` 通过
  - `cargo clippy --all-targets --all-features -- -D warnings` 通过
  - `cargo machete` 复扫结果为 0 个未使用直接依赖

**影响**: 低
**可操作性**: 已完成

**建议**: 后续继续把 `cargo machete` 作为常规回归项

### 4. darling (0.20.11 / 0.23.0)

**来源**: 上游 proc-macro 依赖

**影响**: 极低（仅编译时）
**可操作性**: 无法直接解决

**建议**: 保持现状

### 5. core-foundation (0.9.4 / 0.10.1)

**来源**: macOS 系统库绑定，多个上游依赖

**影响**: 低
**可操作性**: 无法直接解决

**建议**: 保持现状

## 总结

### 可立即处理
- `deadpool-postgres` 未使用直接依赖清理
- 其余 10 项未使用直接依赖清理

### 可尝试优化
- `config` 相关上游链路导致的重复依赖
- 观测链相关上游依赖统一

### 无法处理（上游依赖）
- base64 版本冲突
- darling 版本冲突  
- core-foundation 版本冲突

## 建议行动

1. ✅ 接受大部分重复依赖为上游依赖冲突
2. ✅ 删除未使用的 `deadpool-postgres` 直接依赖
3. ✅ 删除其余未使用直接依赖并收紧 `server` feature
4. ⏭️ 定期运行 `cargo update` 保持依赖最新

## 依赖健康度评分

- **直接依赖**: ✅ 健康（当前 `cargo machete` 结果为 0 项未使用直接依赖）
- **重复依赖**: ⚠️ 可接受（主要为上游冲突）
- **安全性**: ✅ 无已知漏洞（需定期 cargo audit）
- **维护性**: ✅ 良好

## 结论

当前剩余的重复依赖主要由上游 crate 引入，不影响项目功能和性能。项目侧已完成未使用直接依赖清理，当前 `cargo machete` 复扫结果为 0，后续建议保持现状并定期更新依赖以获得上游修复。

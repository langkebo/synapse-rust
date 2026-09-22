# 后端 Histogram 改原生 bucket - P2 长期方案评估

## 当前状态

### Binary 检查结果：未修复

**运行中镜像信息：**
- 容器: `synapse-app`
- 镜像: `synapse-rust:distroless` (`sha256:6f31191e7df0ccc1a2ab0fe44425c4e387464cf6d07f84428ee030e233c467a1`)
- 镜像创建时间：未知（需进一步核查）

**实际观测到的指标格式：**
```bash
# HELP db_query_duration_ms_count db_query_duration_ms_count
# TYPE db_query_duration_ms_count counter
db_query_duration_ms_count{unit="ms"} 0

# HELP db_query_duration_ms_sum db_query_duration_ms_sum
# TYPE db_query_duration_ms_sum counter
db_query_duration_ms_sum{unit="ms"} -0

# HELP http_request_duration_ms_count http_request_duration_ms_count
# TYPE http_request_duration_ms_count counter
http_request_duration_ms_count{unit="ms"} 0

# HELP http_request_duration_ms_sum http_request_duration_ms_sum
# TYPE http_request_duration_ms_sum counter
http_request_duration_ms_sum{unit="ms"} -0
```

**结论：当前 binary 未修复**。Histogram 仍为非标准格式，仅有 `count` 和 `sum`，缺少 `_bucket` 系列。

### 代码源文件状态

**文件路径：** `synapse-common/src/metrics.rs`

**当前代码状态（git 暂存区之外有未提交修改）：**
```rust
pub mod histogram {
    _register_histogram(self, &name, &labels, fns, metric::MetricUnit::Milliseconds);
    }
}
```

**未提交的修改内容：**
```rust
pub mod histogram {
// ... unchanged imports ...

    // Issue: Histogram using custom suffixes _count/_sum instead of Prometheus standard _bucket/_count/_sum
    // PREREQUISITE: Ensure you have `metrics` crate with `HISTOGRAM` registrar support.
    // 1. Register histogram with standardized metrics library (e.g., metrics_exporter_prometheus).
    // 2. Ensure MetricKind::Histogram generates proper _bucket metrics.

    pub mod histogram {
        _register_histogram(self, &name, &labels, fns, metric::MetricUnit::Milliseconds);
    }
}
```

**源代码 git 状态：**
- 最后提交: `af2b9702 B-3.1-b-2: synapse-common fully documented + deny(missing_docs)`
- 工作区：`M synapse-common/src/metrics.rs` (有未提交修改)

**结论：代码修改已在本地，但未提交、未构建、未部署。**

## 问题根因

1. **当前 MetricsCollector 实现**

MetricsCollector 使用自研的 `register_histogram` 方法，生成格式为：
- `{name}_count`
- `{name}_sum`
- 缺少 Prometheus 标准要求的 `{name}_bucket{le="..."}` 系列

2. **Prometheus 要求**

Prometheus Histogram 必须包含：
- `_bucket{le="0.005",...}`：各桶累积计数
- `_count`：总样本数
- `_sum`：总和

3. **当前影响**

- P0 警告记录规则无效（如 `http_request_duration_ms_bucket` 不存在）
- Grafana 面板无法渲染百分位
- Prometheus 警告规则触发条件不成立

## 长期修复方案 (P2)

### 方案 A：后端改原生 bucket（推荐）

**优点：**
- 一次性根治，符合 Prometheus 标准
- 无需 PromQL 适配层
- 长期维护成本最低

**缺点：**
- 需要修改 `MetricsCollector` 核心逻辑
- 需要重新编译后端 Binary
- 需要重新部署容器
- 需要验证现有所有 histogram 指标

**实现步骤：**

1. **MetricsCollector 接口改造**
   ```
   synapse-common/src/metrics.rs
   
   // 修改 register_histogram 方法
   // 从: register_counter(name + "_count", ...)
   // 到: 注册标准 histogram，生成 _bucket/_count/_sum
   ```

2. **修改点枚举：**
   - `MetricsCollector::register_histogram()`
   - `MetricsCollector::register_histogram_with_labels()`
   - 所有调用者的标签参数校验（避免高基数）

3. **测试验证：**
   ```bash
   cargo test --features test-utils -p synapse-common
   cargo test --features test-utils -p synapse-rust
   ```

4. **构建部署：**
   ```bash
   # 在 docker/deploy/ 构建
   docker build -t synapse-rust:distroless .
   docker-compose up -d synapse-app
   ```

5. **验证：**
   ```bash
   curl http://localhost:9090/metrics | grep "http_request_duration_ms_bucket"
   # 预期: 出现 _bucket 系列
   ```

**预估工作量：**
- 代码修改：2-3 小时
- 测试：1 小时
- 构建部署：30 分钟
- 回归测试：1 小时
- 总计：约 4-5 小时

### 方案 B：Prometheus 插件适配（临时）

**优点：**
- 无需修改后端代码
- 立即可用

**缺点：**
- 技术债务累积
- 增加运维复杂度
- 非标准方案

**实现：**（已在 P0 方案中提供）

## 建议执行顺序

1. **立即执行 P0 修复**（已完成 - 记录规则和告警规则修正）
   - 修改 `prometheus-ops-expert/config/prometheus.yml`
   - 反映在 Grafana 中
   - 立即恢复监控

2. **并行准备 P2 修复**
   - 后端团队评审 `synapse-common/src/metrics.rs` 修改方案
   - 创建分支 `feat/metrics-histogram-standard-bucket`
   - 实现原生 bucket 支持

3. **分阶段部署**
   - 阶段 1：开发环境验证
   - 阶段 2：测试环境验证
   - 阶段 3：生产环境灰度
   - 阶段 4：全量回滚 P0 适配规则

## 风险评估

| 风险项 | 可能性 | 影响 | 缓解措施 |
|-------|--------|------|----------|
| 修改 MetricsCollector 引入回归 | 中 | 高 | 全面单元测试 + 集成测试 |
| Histogram 数据格式变更导致 Grafana 面板失效 | 低 | 中 | 提前备份面板配置 |
| 旧有监控告警规则被遗漏 | 中 | 高 | 全量 grep 搜索 `_count` 和 `_sum` 使用 |
| 构建时间过长影响部署窗口 | 低 | 中 | 在非高峰期部署 |

## 后端团队待办

### 任务 1：代码改造
**负责人：** 后端开发团队
**工时预估：** 4 小时

- [ ] 阅读 `synapse-common/src/metrics.rs` 当前实现
- [ ] 理解 MetricsCollector 的 register_histogram 流程
- [ ] 修改为标准 Prometheus histogram 格式
- [ ] 添加单元测试验证 _bucket 生成
- [ ] 提交 PR 并通过 code review

### 任务 2：测试验证
**负责人：** QA + 后端
**工时预估：** 2 小时

- [ ] 单元测试覆盖
- [ ] 集成测试验证 metrics 端点
- [ ] 手动验证 curl 输出包含 _bucket

### 任务 3：部署
**负责人：** DevOps
**工时预估：** 1 小时

- [ ] 构建新镜像
- [ ] 部署到测试环境
- [ ] 部署到生产环境（灰度）
- [ ] 验证监控恢复

## 监控恢复时间线

- **P0 修复（记录规则修正）：** 立即可用 ✅
- **P2 修复（后端改造）：** 预估 2-3 天（含评审、测试、部署）

## 结论

**当前 binary 未修复。** 需后端团队执行 P2 方案。

**建议：**
1. 立即使用 P0 方案恢复监控（已完成）
2. 启动 P2 方案开发并排期
3. P2 完成后再移除 P0 的 workaround 规则

---

*生成时间：2026-09-22*
*文档版本：v1.0*

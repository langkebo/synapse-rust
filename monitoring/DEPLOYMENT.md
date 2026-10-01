# Synapse-Rust 监控栈部署指南

> **版本**: v1.0  
> **更新日期**: 2026-09-30  
> **负责人**: prometheus-ops-expert

---

## 🏗️ 架构概览

```
┌─────────────────────────────────────────────────────────────┐
│                     Grafana (9091)                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │  SLO 面板     │  │  业务面板    │  │  故障诊断面板     │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
└────────────┬──────────────────────────────────────┘
             │ 查询
             ▼
┌─────────────────────────────────────────────────────────────┐
│                  Prometheus (9090)                          │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │ synapse-rust │  │    db        │  │     redis        │  │
│  │  metrics      │  │  metrics    │  │   metrics       │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
└────────────┬──────────────────────────────────────┘
             │ 告警
             ▼
┌─────────────────────────────────────────────────────────────┐
│               Alertmanager (9093)                           │
│                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │  邮件通知     │  │  Slack 通知  │  │  PagerDuty 通知  │  │
│  └──────────────┘  └──────────────┘  └──────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

---

## 📦 部署步骤

### 1. 环境准备

```bash
# 克隆项目（如果还没克隆）
git clone https://github.com/your-org/synapse-rust.git
cd synapse-rust

# 确保 Docker 和 Docker Compose 已安装
docker --version
docker-compose --version
```

### 2. 配置环境变量

```bash
# 复制示例配置文件
cp .env.monitoring.example .env.monitoring.local

# 编辑配置文件，设置敏感信息
vim .env.monitoring.local
```

**必须配置的项**:
```env
# Grafana 管理员密码
GRAFANA_ADMIN_PASSWORD=你的安全密码

# 告警通知邮箱
ALERT_EMAIL_RECIPIENTS=ops@your-company.com

# Slack Webhook URL (可选)
SLACK_CRITICAL_WEBHOOK=https://hooks.slack.com/services/...
```

### 3. 验证配置文件

```bash
# 验证所有配置文件语法
./scripts/deploy-monitoring.sh validate
```

**预期输出**:
```
✓ Prometheus config: Valid
✓ Alertmanager config: Valid
✓ Grafana datasources: File exists
✓ Grafana dashboards: Files exist
✓ 配置文件验证完成
```

### 4. 启动监控栈

```bash
# 方式 1: 使用部署脚本 (推荐)
./scripts/deploy-monitoring.sh start

# 方式 2: 手动使用 Docker Compose
docker-compose -f docker/docker-compose.yml \
               -f docker/docker-compose.monitoring.yml \
               --env-file .env.monitoring.local \
               up -d
```

### 5. 验证部署

```bash
# 检查服务健康状态
./scripts/deploy-monitoring.sh health

# 查看服务日志
./scripts/deploy-monitoring.sh logs prometheus
./scripts/deploy-monitoring.sh logs grafana
```

---

## 📊 访问地址

| 服务 | 地址 | 默认账号 | 说明 |
|------|------|----------|------|
| **Prometheus** | http://localhost:9090 | 无 | 指标查询与告警管理 |
| **Grafana** | http://localhost:9091 | admin/admin123 | 可视化仪表盘 |
| **Alertmanager** | http://localhost:9093 | 无 | 告警路由与通知 |

> **注意**: 所有服务默认绑定 `127.0.0.1`，仅本地访问。如需外部访问，请修改 `docker-compose.monitoring.yml` 中的端口映射。

---

## 🎨 Grafana 仪表盘

### 自动导入仪表盘

部署完成后，Grafana 会自动导入以下仪表盘：

1. **Synapse-Rust - SLO Monitoring**
   - 可用性 SLO 监控
   - 延迟 SLO (P50/P95/P99)
   - 错误预算剩余
   - 错误烧尽率

2. **Synapse-Rust - Business Metrics**
   - 活跃用户趋势
   - 房间增长趋势
   - 消息发送量（分加密状态）
   - E2EE 覆盖率
   - 数据库性能分析
   - 缓存命中率
   - Federation 健康度

### 手动导入仪表盘

如果自动导入失败，可以手动导入：

1. 访问 Grafana: http://localhost:9091
2. 导航到 **Dashboards → Manage → Import**
3. 上传 JSON 文件：
   - `monitoring/grafana/dashboards/grafana-dashboard-slo.json`
   - `monitoring/grafana/dashboards/grafana-dashboard-business.json`

---

## 🚨 告警规则

### 告警等级说明

| 等级 | 说明 | 通知方式 | 响应时间 |
|------|------|----------|----------|
| **Critical** | 服务中断/严重错误 | 邮件 + Slack + PagerDuty | 立即 |
| **Warning** | 性能异常/潜在问题 | Slack | 5 分钟内 |
| **Info** | 业务指标异常 | 邮件 | 1 小时内 |

### 告警规则列表

#### Critical 告警

| 告警名称 | 触发条件 | 说明 |
|----------|----------|------|
| HighHTTPErrorRate | HTTP 5xx 错误率 > 5% (5分钟) | 服务可用性下降 |
| SynapseServiceDown | 服务无法访问 (1分钟) | 服务完全不可用 |
| HighDatabaseErrorRate | DB 错误 > 10/s (2分钟) | 数据库错误激增 |
| RedisConnectionErrors | Redis 错误 > 5/s (2分钟) | 缓存服务异常 |

#### Warning 告警

| 告警名称 | 触发条件 | 说明 |
|----------|----------|------|
| SlowRoomCreation | 房间创建 P99 > 5s | 用户创建房间体验下降 |
| SlowMessageDelivery | 消息投递 P95 > 1s | 消息延迟过高 |
| SlowSyncLatency | Sync P95 > 2s | 同步延迟过高 |
| SlowE2EEHandshake | E2EE 握手 P95 > 3s | 加密握手慢 |
| HighDatabaseConnectionUtilization | 连接池 > 90% | 数据库连接耗尽风险 |
| MessageQueueBacklog | 队列深度 > 100 | 消息积压 |
| LowCacheHitRate | 缓存命中率 < 80% | 缓存效率下降 |
| HighFederationErrorRate | Federation 错误率 > 10% | 联邦通信异常 |
| HighFederationTimeoutRate | 超时率 > 20% | 远程服务器超时 |

#### Info 告警

| 告警名称 | 触发条件 | 说明 |
|----------|----------|------|
| AbnormalRoomCreateFailureRate | 失败率 > 5% | 房间创建失败率异常 |
| SpikeInAuthFailures | 失败率 2 倍上升 | 可能存在攻击 |
| E2EECoverageDrop | 加密覆盖率 < 70% | 安全性下降 |

---

## 🛠️ 常用操作

### 查看告警

```bash
# 查看告警规则
curl http://localhost:9090/api/v1/rules

# 查看活动告警
curl http://localhost:9090/api/v1/alerts

# 查看 Alertmanager 状态
curl http://localhost:9093/api/v2/status
```

### PromQL 查询示例

```bash
# 查询当前房间创建延迟 P95
curl -G http://localhost:9090/api/v1/query \
  --data-urlencode 'query=histogram_quantile(0.95, sum(rate(room_creation_duration_seconds_bucket[5m])) by (le))'

# 查询当前活跃用户数
curl -G http://localhost:9090/api/v1/query \
  --data-urlencode 'query=sum(http_active_requests)'
```

### 停止/清理监控栈

```bash
# 停止监控栈
./scripts/deploy-monitoring.sh stop

# 清理所有数据（慎用！会删除历史数据）
./scripts/deploy-monitoring.sh clean
```

---

## 📁 文件结构

```
synapse-rust/
├── docker/
│   ├── docker-compose.yml                  # 基础服务编排
│   └── docker-compose.monitoring.yml       # 监控栈编排
├── monitoring/
│   ├── prometheus/
│   │   └── prometheus.yml                  # Prometheus 配置文件
│   ├── alertmanager/
│   │   └── alertmanager.yml                # Alertmanager 配置文件
│   └── grafana/
│       ├── datasources/
│       │   └── datasources.yml             # Grafana 数据源配置
│       └── dashboards/
│           ├── providers.yml               # 仪表盘提供者配置
│           ├── grafana-dashboard-slo.json  # SLO 监控仪表盘
│           └── grafana-dashboard-business.json  # 业务指标仪表盘
├── tests/
│   ├── alerting-rules.yml                  # 告警规则定义
│   ├── grafana-dashboard-slo.json          # (同上，副本)
│   └── grafana-dashboard-business.json     # (同上，副本)
├── scripts/
│   └── deploy-monitoring.sh                # 部署脚本
└── .env.monitoring.example                 # 环境变量示例
```

---

## 🔧 故障排除

### 1. Grafana 无法访问 Prometheus

**症状**: Grafana 面板显示 "No data" 或 "Connection refused"

**解决步骤**:
```bash
# 检查 Prometheus 是否运行
docker ps | grep prometheus

# 检查 Prometheus 健康状态
curl http://localhost:9090/-/healthy

# 查看 Prometheus 日志
./scripts/deploy-monitoring.sh logs prometheus
```

### 2. 告警无法发送

**症状**: AlertManager UI 显示告警，但邮件/Slack 没收到

**解决步骤**:
```bash
# 检查 Alertmanager 配置
curl http://localhost:9093/api/v2/status

# 查看 Alertmanager 日志
./scripts/deploy-monitoring.sh logs alertmanager

# 验证 SMTP/Slack 配置
cat .env.monitoring.local | grep -E "SMTP|SLACK"
```

### 3. 指标缺失

**症状**: Prometheus 查询返回空

**解决步骤**:
1. 确认 synapse-rust 服务已启动并暴露 metrics 接口
2. 检查 Prometheus 配置中的 `scrape_configs`
3. 验证 `metrics_path` 是否正确 (`/_prometheus/metrics`)

```bash
# 检查 synapse-rust 是否暴露 metrics
curl http://localhost:8008/_prometheus/metrics | head -20

# 检查 Prometheus 抓取状态
curl http://localhost:9090/api/v1/targets
```

---

## 📈 性能优化建议

### 1. Prometheus 存储优化

```yaml
# 在 prometheus.yml 中调整
global:
  scrape_interval: 15s    # 通用抓取间隔
  # 高频指标单独设置
scrape_configs:
  - job_name: "synapse-rust"
    scrape_interval: 15s
  - job_name: "node-exporter"
    scrape_interval: 30s
```

### 2. 数据保留策略

```bash
# 在 Prometheus 启动参数中设置
--storage.tsdb.retention.time=15d     # 保留 15 天
--storage.tsdb.retention.size=10GB   # 最大 10GB
```

### 3. Grafana 性能优化

- 避免在同一个面板使用过多查询
- 使用 `$__range` 变量动态调整查询粒度
- 启用 Grafana 的缓存功能

---

## 📞 联系支持

如遇到部署问题，请联系：
- **监控专家**: prometheus-ops-expert
- **文档反馈**: 提交 Issue 到项目仓库
- **紧急联系**: ops@your-company.com

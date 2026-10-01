# 🚀 快速开始 - 导入 Prometheus + Grafana 使用

## ✅ 准备工作完成

我们已经为你准备好了完整的监控栈配置，包括：

### 📦 交付文件清单

```
synapse-rust/
├── docker/
│   └── docker-compose.monitoring.yml          # 监控栈编排文件 ✅
├── monitoring/
│   ├── prometheus/
│   │   └── prometheus.yml                     # Prometheus 配置 ✅
│   ├── alertmanager/
│   │   └── alertmanager.yml                   # Alertmanager 配置 ✅
│   └── grafana/
│       ├── datasources/
│       │   └── datasources.yml                # Grafana 数据源配置 ✅
│       └── dashboards/
│           ├── providers.yml                  # 仪表盘提供者配置 ✅
│           ├── grafana-dashboard-slo.json     # SLO 监控面板 ✅
│           └── grafana-dashboard-business.json  # 业务指标面板 ✅
├── scripts/
│   └── deploy-monitoring.sh                   # 一键部署脚本 ✅
├── .env.monitoring.example                    # 环境变量示例 ✅
├── tests/
│   ├── prometheus-implementation-summary.md   # 实施总结文档 ✅
│   ├── prometheus-implementation-plan.md      # 实施计划文档 ✅
│   ├── alerting-rules.yml                     # 告警规则定义 ✅
│   ├── grafana-dashboard-slo.json             # (仪表盘副本) ✅
│   └── grafana-dashboard-business.json        # (仪表盘副本) ✅
└── monitoring/DEPLOYMENT.md                   # 详细部署文档 ✅
```

---

## 🎯 三步启动监控栈

### 第 1 步：配置环境变量 (可选)

```bash
# 复制并编辑环境变量文件（仅第一次）
cp .env.monitoring.example .env.monitoring.local
vim .env.monitoring.local
```

**最小配置**:
```env
# 至少需要设置 Grafana 密码
GRAFANA_ADMIN_PASSWORD=YourSecurePassword123!

# 可选：告警通知配置
ALERT_EMAIL_RECIPIENTS=ops@your-company.com
```

跳过这一步也可以，会使用默认值。

### 第 2 步：验证配置文件

```bash
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

### 第 3 步：启动监控栈

```bash
# 一键启动所有服务
./scripts/deploy-monitoring.sh start
```

**等待 2-3 分钟**，然后访问：
- **Grafana**: http://localhost:9091 (admin / admin123)
- **Prometheus**: http://localhost:9090
- **Alertmanager**: http://localhost:9093

---

## 🎨 自动导入的仪表盘

启动后，Grafana 会自动加载 2 个专业仪表盘：

### 1. Synapse-Rust - SLO Monitoring

包含以下核心面板:
- ✅ 总体可用性 SLO (Gauge)
- ✅ 错误预算剩余 (Gauge)
- ✅ /sync 请求延迟 SLO (P50/P95/P99)
- ✅ 消息投递延迟 SLO (P50/P95/P99)
- ✅ 房间创建延迟 SLO (P50/P95/P99)
- ✅ 错误预算烧尽率

**访问路径**: Grafana → Dashboards → Synapse-Rust → SLO Monitoring

### 2. Synapse-Rust - Business Metrics

包含以下业务面板:
- ✅ 活跃用户趋势
- ✅ 房间增长趋势
- ✅ 消息发送量 (分加密状态)
- ✅ E2EE 覆盖率 (Gauge)
- ✅ 房间操作成功率 (PieChart)
- ✅ 数据库查询性能 (Table)
- ✅ 缓存命中率 (BarGauge)
- ✅ Federation 成功率 (Table)

**访问路径**: Grafana → Dashboards → Synapse-Rust → Business Metrics

---

## 🚨 告警规则

已配置的告警规则包含 6 个类别共 15+ 条规则：

### 告警等级分类

| 等级 | 数量 | 示例 | 通知渠道 |
|------|------|------|----------|
| **Critical** | 4 | HTTP 5xx 错误率 > 5%, 服务宕机 | 邮件 + Slack + PagerDuty |
| **Warning** | 9 | 延迟过高、连接池满载 | Slack |
| **Info** | 3 | E2EE 覆盖率下降、认证失败突增 | 邮件 |

### 如何查看告警

```bash
# 在 Prometheus UI 查看
http://localhost:9090/rules
http://localhost:9090/alerts

# 在 Alertmanager UI 查看
http://localhost:9093/#/alerts

# 或通过 API 查询
curl http://localhost:9090/api/v1/rules
```

---

## 🛠️ 常用命令速查

```bash
# 启动监控栈
./scripts/deploy-monitoring.sh start

# 停止监控栈
./scripts/deploy-monitoring.sh stop

# 重启监控栈
./scripts/deploy-monitoring.sh restart

# 查看所有服务日志
./scripts/deploy-monitoring.sh logs

# 查看特定服务日志 (prometheus/grafana/alertmanager)
./scripts/deploy-monitoring.sh logs prometheus

# 检查健康状态
./scripts/deploy-monitoring.sh health

# 验证配置文件
./scripts/deploy-monitoring.sh validate

# 清理所有数据 (慎用!)
./scripts/deploy-monitoring.sh clean
```

---

## 📊 PromQL 查询示例

### 实时查询当前指标

```bash
# 查询当前同步延迟 P95
curl -G http://localhost:9090/api/v1/query \
  --data-urlencode 'query=histogram_quantile(0.95, sum(rate(sync_duration_ms_bucket[5m])) by (le))'

# 查询当前活跃用户数
curl -G http://localhost:9090/api/v1/query \
  --data-urlencode 'query=sum(increase(auth_success[1h]))'

# 查询房间创建成功率
curl -G http://localhost:9090/api/v1/query \
  --data-urlencode 'query=sum(rate(room_operations_total{outcome="success"}[5m])) / sum(rate(room_operations_total{operation="create"}[5m]))'

# 查询消息投递延迟
curl -G http://localhost:9090/api/v1/query \
  --data-urlencode 'query=histogram_quantile(0.95, sum(rate(message_delivery_latency_seconds_bucket[5m])) by (le))'
```

### 在 Grafana Explore 中查询

1. 访问 http://localhost:9091/explore
2. 选择 `Prometheus` 数据源
3. 粘贴上面的 PromQL 查询
4. 点击 "Run query" 查看实时数据

---

## ⚙️ 高级配置

### 启用邮件通知

编辑 `.env.monitoring.local`:
```env
SMTP_SMARTHOST=smtp.company.com:587
SMTP_FROM=alertmanager@synapse-rust.local
SMTP_USERNAME=your-smtp-username
SMTP_PASSWORD=your-smtp-password
ALERT_EMAIL_RECIPIENTS=ops@company.com,dev-team@company.com
```

重启 Alertmanager:
```bash
./scripts/deploy-monitoring.sh restart
```

### 集成 Slack

编辑 `.env.monitoring.local`:
```env
# 在 Slack 创建 Incoming Webhook
SLACK_CRITICAL_WEBHOOK=https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX
SLACK_WARNING_WEBHOOK=https://hooks.slack.com/services/T00000000/B00000001/YYYYYYYY
```

重启 Alertmanager:
```bash
./scripts/deploy-monitoring.sh restart
```

### 自定义抓取间隔

编辑 `monitoring/prometheus/prometheus.yml`:
```yaml
global:
  scrape_interval: 30s  # 默认 30 秒

scrape_configs:
  - job_name: "synapse-rust"
    scrape_interval: 15s  # 更频繁
```

重启 Prometheus:
```bash
./scripts/deploy-monitoring.sh restart
```

---

## 🔧 故障排除

### 问题 1: Grafana 显示 "No data"

**解决方案**:
```bash
# 1. 确认 Prometheus 正在运行
docker ps | grep prometheus

# 2. 确认 Prometheus 能正常抓取指标
curl http://localhost:9090/api/v1/targets | jq '.data.active_targets[].job'

# 3. 等待 5-10 分钟让数据采集足够
```

### 问题 2: 无法访问 Grafana

**解决方案**:
```bash
# 1. 检查 Grafana 是否运行
./scripts/deploy-monitoring.sh health

# 2. 查看日志
./scripts/deploy-monitoring.sh logs grafana

# 3. 确认端口未被占用
lsof -i :9091
```

### 问题 3: 告警不触发

**解决方案**:
```bash
# 1. 确认告警规则已加载
curl http://localhost:9090/api/v1/rules | jq '.data.groups[].name'

# 2. 检查告警评估状态
curl http://localhost:9090/api/v1/alerts

# 3. 手动触发测试告警（可选）
# 修改规则中的阈值使其立即触发
```

---

## 📚 扩展阅读

- [详细部署文档](monitoring/DEPLOYMENT.md)
- [实施总结文档](tests/prometheus-implementation-summary.md)
- [实施计划文档](tests/prometheus-implementation-plan.md)
- [告警规则详细说明](monitoring/alertmanager/alertmanager.yml)

---

## 🎉 开始使用

你现在可以:

1. **访问 Grafana** http://localhost:9091 查看实时监控
2. **探索 Prometheus** http://localhost:9090 查询原始指标
3. **配置告警通知** 按照上面步骤启用邮件/Slack
4. **自定义仪表盘** 在 Grafana 中添加更多面板

**建议下一步**:
- 阅读 `monitoring/DEPLOYMENT.md` 了解完整功能
- 参考 `tests/prometheus-implementation-summary.md` 了解指标体系
- 根据需要调整告警阈值和通知渠道

---

**💡 提示**: 首次启动后，Grafana 需要 1-2 分钟初始化，Prometheus 需要 5-10 分钟收集足够的历史数据才能显示完整的图表。请耐心等待！

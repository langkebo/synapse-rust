# Prometheus + Grafana 监控栈使用指南

> ⚠️ **已废弃（DEPRECATED）**：本文档描述的是旧的 `synapse-test` 监控栈
> （`scripts/deploy-monitoring.sh` + `docker/docker-compose.monitoring.yml`，Grafana :9091），
> 与线上实际运行的 deploy 栈互斥，会抢占线上端口。请改用
> [`README_MONITORING.md`](./README_MONITORING.md)（deploy 栈，Grafana :3000）。
> 本文仅作历史参考，不再作为操作依据。

> **当前状态**: ✅ 配置文件验证完成  
> **下一步**: 安装 Docker 后启动监控栈

---

## 🎯 验证结果

```bash
✅ 配置文件完整性检查通过
   ✓ prometheus.yml
   ✓ alertmanager.yml  
   ✓ datasources.yml
   ✓ providers.yml
   ✓ grafana-dashboard-slo.json (Valid JSON)
   ✓ grafana-dashboard-business.json (Valid JSON)
```

---

## 📦 当前环境限制

**检测到**: Docker 未安装在当前系统中

**解决方案**:
1. **方案 A - 在本机安装 Docker**（推荐用于开发环境）
2. **方案 B - 在 Docker Desktop 中运行**（推荐用于 macOS/Windows）
3. **方案 C - 在服务器/容器中部署**（推荐用于生产环境）

---

## 🚀 三种部署方案

### 方案 A: macOS 安装 Docker Desktop

```bash
# 1. 使用 Homebrew 安装 Docker Desktop
brew install --cask docker

# 2. 启动 Docker Desktop 应用

# 3. 验证安装
docker --version
docker-compose --version

# 4. 启动监控栈
cd /Users/ljf/Desktop/hu_ts/synapse-rust
./scripts/deploy-monitoring.sh start

# 5. 访问 Grafana
open http://localhost:9091
```

### 方案 B: Linux 安装 Docker

```bash
# 1. 安装 Docker (Ubuntu/Debian)
sudo apt-get update
sudo apt-get install -y docker.io docker-compose

# 或者 (CentOS/RHEL)
sudo yum install -y docker docker-compose

# 2. 启动 Docker 服务
sudo systemctl start docker
sudo systemctl enable docker

# 3. 将当前用户添加到 docker 组
sudo usermod -aG docker $USER
newgrp docker

# 4. 验证安装
docker --version
docker-compose --version

# 5. 启动监控栈
./scripts/deploy-monitoring.sh start

# 6. 访问 Grafana
xdg-open http://localhost:9091  # Linux
```

### 方案 C: 直接部署到服务器

如果已有可用的 Docker 环境：

```bash
# 1. SSH 连接到服务器
ssh user@your-server.com

# 2. 上传配置文件（如果还没上传）
scp -r /Users/ljf/Desktop/hu_ts/synapse-rust/monitoring/ user@server:/path/to/project/
scp /Users/ljf/Desktop/hu_ts/synapse-rust/docker/docker-compose.monitoring.yml user@server:/path/to/project/docker/
scp /Users/ljf/Desktop/hu_ts/synapse-rust/scripts/deploy-monitoring.sh user@server:/path/to/project/scripts/
scp /Users/ljf/Desktop/hu_ts/synapse-rust/.env.monitoring.example user@server:/path/to/project/

# 3. 在服务器上配置环境变量
cp /path/to/project/.env.monitoring.example /path/to/project/.env.monitoring.local
vim /path/to/project/.env.monitoring.local

# 4. 启动监控栈
cd /path/to/project
chmod +x ./scripts/deploy-monitoring.sh
./scripts/deploy-monitoring.sh start

# 5. 配置防火墙开放端口（如果需要远程访问）
sudo firewall-cmd --permanent --add-port=9090/tcp  # Prometheus
sudo firewall-cmd --permanent --add-port=9091/tcp  # Grafana
sudo firewall-cmd --permanent --add-port=9093/tcp  # Alertmanager
sudo firewall-cmd --reload

# 6. 从本地访问
# 方式 1: 端口转发
ssh -L 9090:localhost:9090 -L 9091:localhost:9091 user@server
# 然后在浏览器打开 http://localhost:9091

# 方式 2: 直接访问服务器 IP（不安全，建议使用 VPN 或反向代理）
# http://your-server-ip:9091
```

---

## 🛠️ 快速命令参考

### 启动监控栈

```bash
# 前提：Docker 已安装并运行

# 方式 1: 使用部署脚本
./scripts/deploy-monitoring.sh start

# 方式 2: 手动使用 Docker Compose
cd docker/
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml up -d

# 方式 3: 指定环境变量文件
docker-compose -f docker/docker-compose.yml \
               -f docker/docker-compose.monitoring.yml \
               --env-file .env.monitoring.local \
               up -d
```

### 验证部署

```bash
# 检查服务健康状态
./scripts/deploy-monitoring.sh health

# 查看所有容器状态
docker-compose -f docker/docker-compose.yml \
               -f docker/docker-compose.monitoring.yml \
               ps

# 查看日志
./scripts/deploy-monitoring.sh logs

# 查看特定服务日志
./scripts/deploy-monitoring.sh logs prometheus
./scripts/deploy-monitoring.sh logs grafana
./scripts/deploy-monitoring.sh logs alertmanager
```

### 停止/重启监控栈

```bash
# 停止
./scripts/deploy-monitoring.sh stop

# 重启
./scripts/deploy-monitoring.sh restart

# 清理所有数据（慎用！）
./scripts/deploy-monitoring.sh clean
```

---

## 📊 访问地址

启动成功后，访问以下地址：

| 服务 | 地址 | 默认账号 | 说明 |
|------|------|----------|------|
| **Grafana** | http://localhost:9091 | admin/admin123 | 可视化仪表盘 ⭐ 主要入口 |
| **Prometheus** | http://localhost:9090 | 无 | 原始指标查询 |
| **Alertmanager** | http://localhost:9093 | 无 | 告警管理 |

---

## 🎨 自动导入的仪表盘

### 1. SLO Monitoring 仪表盘

**包含面板**:
- 总体可用性 SLO
- 错误预算剩余
- /sync 请求延迟 (P50/P95/P99)
- 消息投递延迟 (P50/P95/P99)
- 房间创建延迟 (P50/P95/P99)
- 错误预算烧尽率

**访问路径**:
```
Grafana → Dashboards → Synapse-Rust → SLO Monitoring
```

### 2. Business Metrics 仪表盘

**包含面板**:
- 活跃用户趋势
- 房间增长趋势
- 消息发送量 (分加密状态)
- E2EE 覆盖率
- 房间操作成功率
- 数据库查询性能
- 缓存命中率
- Federation 成功率

**访问路径**:
```
Grafana → Dashboards → Synapse-Rust → Business Metrics
```

---

## ⚙️ 配置告警通知

### 1. 复制并编辑环境变量文件

```bash
cp .env.monitoring.example .env.monitoring.local
vim .env.monitoring.local
```

### 2. 配置邮件通知

```env
# SMTP 服务器配置
SMTP_SMARTHOST=smtp.gmail.com:587
SMTP_FROM=alertmanager@synapse-rust.local
SMTP_USERNAME=your-email@gmail.com
SMTP_PASSWORD=your-app-password  # 推荐使用应用专用密码
ALERT_EMAIL_RECIPIENTS=ops@company.com,dev-team@company.com
```

### 3. 配置 Slack 通知

**步骤**:
1. 在 Slack 工作区创建 Incoming Webhook
   - 访问：https://your-workspace.slack.com/apps/manage/custom-integrations
   - 添加 "Incoming Webhooks"
   - 选择频道 (如 #alerts)
   - 复制 Webhook URL

2. 编辑 `.env.monitoring.local`:
```env
SLACK_CRITICAL_WEBHOOK=https://hooks.slack.com/services/T00000000/B00000001/XXXXXXXX
SLACK_WARNING_WEBHOOK=https://hooks.slack.com/services/T00000000/B00000002/YYYYYYYY
SLACK_INFO_WEBHOOK=https://hooks.slack.com/services/T00000000/B00000003/ZZZZZZZZ
```

### 4. 配置 PagerDuty 通知

**步骤**:
1. 在 PagerDuty 创建 Service
2. 获取 Service Key

3. 编辑 `.env.monitoring.local`:
```env
PAGERDUTY_SERVICE_KEY=your-service-key-for-critical
PAGERDUTY_DATABASE_KEY=your-service-key-for-database
PAGERDUTY_INFRA_KEY=your-service-key-for-infrastructure
```

### 5. 重启 Alertmanager

```bash
./scripts/deploy-monitoring.sh restart
```

---

## 🔍 故障排除

### 问题 1: Docker Compose 启动失败

**症状**: 容器无法启动或报错

**排查步骤**:
```bash
# 1. 检查 Docker 状态
docker ps
docker-compose version

# 2. 查看详细错误
cd docker/
docker-compose -f docker-compose.yml -f docker-compose.monitoring.yml up

# 3. 检查端口占用
lsof -i :9090 -i :9091 -i :9093

# 4. 查看网络是否存在
docker network ls

# 5. 清理并重新启动
./scripts/deploy-monitoring.sh clean
./scripts/deploy-monitoring.sh start
```

### 问题 2: Grafana 无法登录

**症状**: 登录后显示 "Unauthorized" 或空白页

**解决方案**:
```bash
# 1. 确认使用的是正确的密码
# 检查 .env.monitoring.local 中的 GRAFANA_ADMIN_PASSWORD

# 2. 重置管理员密码（在容器内）
docker exec -it synapse-grafana sh
grafana-cli admin reset-admin-password newpassword
exit

# 3. 或直接在数据库中修改
# 进入 Prometheus 容器，查询数据库（不推荐）
```

### 问题 3: 仪表盘显示 "No data"

**症状**: 所有图表显示空白或 "No data"

**原因**: Prometheus 还未采集到数据

**解决方案**:
```bash
# 1. 等待 5-10 分钟让数据采集足够

# 2. 检查 Prometheus 是否正常工作
curl http://localhost:9090/api/v1/targets | jq '.data.active_targets[].job'

# 3. 查看 Prometheus 抓取状态
curl http://localhost:9090/api/v1/query?query=up

# 4. 检查 synapse-rust 服务是否正常
curl http://localhost:8008/_prometheus/metrics | head -20
```

### 问题 4: 告警不触发

**症状**: Alertmanager UI 显示告警但没有发送邮件

**排查步骤**:
```bash
# 1. 确认 SMTP 配置正确
cat .env.monitoring.local | grep SMTP

# 2. 查看 Alertmanager 日志
./scripts/deploy-monitoring.sh logs alertmanager

# 3. 测试邮件发送
amtool check-config monitoring/alertmanager/alertmanager.yml

# 4. 查看告警状态
curl http://localhost:9090/api/v1/alerts | jq '.data.alerts[0].state'
```

---

## 📚 进一步学习

- [详细部署文档](monitoring/DEPLOYMENT.md) - 完整的部署指南
- [实施总结文档](tests/prometheus-implementation-summary.md) - 指标体系说明
- [告警规则说明](monitoring/alertmanager/alertmanager.yml) - 所有告警规则详解
- [Prometheus 官方文档](https://prometheus.io/docs/introduction/overview/)
- [Grafana 官方文档](https://grafana.com/docs/grafana/latest/)

---

## 💡 最佳实践

### 1. 定期备份数据

```bash
# 备份 Prometheus 数据
docker volume export synapse_prometheus_data > prometheus-backup.tar

# 备份 Grafana 数据
docker volume export synapse_grafana_data > grafana-backup.tar

# 恢复数据
docker volume import prometheus-backup.tar synapse_prometheus_data
docker volume import grafana-backup.tar synapse_grafana_data
```

### 2. 调整数据保留策略

编辑 `docker/docker-compose.monitoring.yml`:
```yaml
services:
  prometheus:
    command:
      - '--storage.tsdb.retention.time=30d'  # 改为 30 天
      - '--storage.tsdb.retention.size=50GB' # 改为 50GB
```

### 3. 自定义告警阈值

编辑 `monitoring/alerting-rules.yml`, 修改相应规则的 `for` 字段和表达式。

### 4. 添加新仪表盘

1. 在 Grafana UI 创建新面板
2. 导出为 JSON
3. 放入 `monitoring/grafana/dashboards/`
4. Grafana 会自动加载（10 秒刷新）

---

## 📞 支持

遇到问题？

1. 查看 [DEPLOYMENT.md](monitoring/DEPLOYMENT.md) 的详细排错指南
2. 查看 Prometheus 日志：`./scripts/deploy-monitoring.sh logs prometheus`
3. 查看 Grafana 日志：`./scripts/deploy-monitoring.sh logs grafana`
4. 提交 Issue 到项目仓库

---

**祝你使用愉快！** 🎉

如果有任何问题，随时查阅相关文档或联系技术支持。

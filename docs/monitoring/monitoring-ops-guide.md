# 监控运维操作手册

> 版本：v1.0（2026-09-22）
> 适用范围：synapse-rust 生产环境 Prometheus + Alertmanager + Grafana + Node Exporter

---

## 1. 日常巡检

### 1.1 Prometheus 健康检查

```bash
# 检查 Prometheus 运行状态
curl -s http://localhost:9092/api/v1/status/config | jq '.status'
curl -s http://localhost:9092/api/v1/targets | jq '.data.activeTargets[] | {job: .labels.job, health: .health}'

# 检查 TSDB 磁盘使用
curl -s "http://localhost:9092/api/v1/status/tsdb" | jq '.data | {headStats: .headStats, retention: .retentionPeriod}'
```

### 1.2 Alertmanager 健康检查

```bash
# 检查 Alertmanager 集群状态
curl -s http://localhost:9093/api/v2/status | jq '.cluster | {status, peers}'
```

### 1.3 Grafana 健康检查

```bash
# 检查 Grafana 数据源
curl -s -u admin:$GRAFANA_ADMIN_PASSWORD http://localhost:3000/api/health | jq '.'
curl -s -u admin:$GRAFANA_ADMIN_PASSWORD http://localhost:3000/api/datasources | jq '.[] | {name, type, url}'
```

### 1.4 核心指标异常值

```bash
# 检查关键指标是否存在 NaN / 异常值
curl -s "http://localhost:9092/api/v1/query?query=up" | jq '.data.result'
curl -s "http://localhost:9092/api/v1/query?query=pool_utilization" | jq '.data.result'
curl -s "http://localhost:9092/api/v1/query?query=rate(http_request_errors_total[5m])" | jq '.data.result'
```

---

## 2. 日常操作

### 2.1 重启 Prometheus

```bash
docker compose -f docker/docker-compose.monitoring.yml restart prometheus

# 验证配置加载
docker exec synapse-prometheus promtool check config /etc/prometheus/prometheus.yml
```

### 2.2 重启 Alertmanager

```bash
docker compose -f docker/docker-compose.monitoring.yml restart alertmanager

# 验证
curl -s http://localhost:9093/api/v2/status | jq '.status'
```

### 2.3 重启 Grafana

```bash
docker compose -f docker/docker-compose.monitoring.yml restart grafana

# 验证
curl -s -u admin:$GRAFANA_ADMIN_PASSWORD http://localhost:3000/api/health | jq '.database'
```

### 2.4 清空告警

```bash
# 查看当前告警
curl -s http://localhost:9093/api/v2/alerts | jq '.data[] | {name, status, labels}'

# 静默告警（紧急止血）
curl -X POST http://localhost:9093/api/v2/silences -H "Content-Type: application/json" \
  -d '{"matchers":[{"name":"alertname","value":"PoolUtilizationCritical","isRegex":false}],"startsAt":"'$(date -u +%Y-%m-%dT%H:%M:%SZ)'","endsAt":"'$(date -u -d "+1h" +%Y-%m-%dT%H:%M:%SZ)'","comment":"紧急止血"}'
```

### 2.5 删除大尺寸的时序数据

```bash
# 查看 TSDB 占用
curl -s "http://localhost:9092/api/v1/status/tsdb" | jq '.data'

# 强制删除指定时间范围的数据（慎用）
curl -X POST http://localhost:9092/api/v1/admin/tsdb/delete_series?match[]={job="synapse-rust"}&start=2026-09-01T00:00:00Z&end=2026-09-15T00:00:00Z
curl -X POST http://localhost:9092/api/v1/admin/tsdb/snapshot
```

---

## 3. 告警处理流程

### 3.1 告警分级

| 严重性 | 响应时间 | 通知渠道 | 升级 |
|--------|---------|---------|------|
| Critical | 5 分钟 | webhook + 电话 | 15 分钟 → 管理层 |
| Warning | 30 分钟 | webhook | 2 小时 → 团队负责人 |
| Info | 无 | 静默记录 | — |

### 3.2 告警响应决策树

```
告警触发
    ├── Critical
    │   ├── synapse-down → 立即检查容器状态 → docker compose ps → 重启 / 回滚
    │   ├── HighErrorRate → 查看日志 → 分析错误类型 → 回滚或止血
    │   ├── HighPoolUtilization → 检查连接池 → 增加 max_connections 或重启
    │   └── DiskAlmostFull → 清理日志 / 降低 retention → 扩容
    │
    └── Warning
        ├── CacheHitRateLow → 检查冷启动状态 → 观察 15 分钟窗口
        ├── HighCpuUsage → 检查是否正常负载 → 考虑扩容
        ├── FederationSlow → 检查远程服务器延迟
        └── MegolmShareSlow → 检查数据库负载
```

### 3.3 常见告警处理

#### `synapse-down`
```bash
docker compose -f docker/docker-compose.yml ps synapse-app
docker compose -f docker/docker-compose.yml logs --tail 50 synapse-app
# 检查进程退出码
docker inspect synapse-app --format='{{.State.ExitCode}}'
# 重启
docker compose -f docker/docker-compose.yml restart synapse-app
```

#### `HighErrorRate`
```bash
# 查看错误日志
docker compose -f docker/docker-compose.yml logs synapse-app --since 10m | grep -i error

# 查看指标变化
curl -s "http://localhost:9092/api/v1/query_range?query=rate(http_request_errors_total[10m])&start=$(date -u -d '1 hour ago' +%s)&end=$(date -u +%s)&step=30s" | jq '.data.result[].values'
```

#### `HighPoolUtilization`
```bash
# 查看当前连接数
curl -s "http://localhost:9092/api/v1/query?query=pool_utilization" | jq '.data.result[].value'

# 检查配置的 max_connections
grep -r "max_connections" synapse-rust/config/

# 紧急扩容
# 在 compose 中增加 max_connections → 重启
```

#### `DiskAlmostFull`
```bash
# 查看磁盘使用
df -h /data/prometheus

# 降低 retention
docker compose -f docker/docker-compose.monitoring.yml exec prometheus \
  kill -SIGHUP 1  # 重新加载配置
# 或者调整 --storage.tsdb.retention.size=50GB 后重启
```

---

## 4. 容量管理

### 4.1 TSDB 存储监控

```bash
# 每日自动检查
curl -s "http://localhost:9092/api/v1/status/tsdb" | jq '.data.headStats | {series: .numSeries, chunks: .numChunks, minTime, maxTime}'
```

### 4.2 存储阈值

| 指标 | 警告 | 临界 | 操作 |
|------|------|------|------|
| TSDB 磁盘使用 | 70% | 90% | 降低 retention / 扩容 / 清理旧数据 |
| 时间序列数 | 50,000 | 100,000 | 检查是否有高频指标泄漏 |
| WAL 大小 | 5GB | 10GB | 检查是否有频繁重启 |

### 4.3 容量扩展步骤

```bash
# 1. 调整 retention size
# docker-compose.monitoring.yml 中 prometheus 的 command 加:
# --storage.tsdb.retention.size=50GB

# 2. 重启
docker compose -f docker/docker-compose.monitoring.yml restart prometheus

# 3. 验证
curl -s "http://localhost:9092/api/v1/status/tsdb" | jq '.data.retention'
```

---

## 5. 备份与恢复

### 5.1 快照备份

```bash
# 创建快照
curl -X POST http://localhost:9092/api/v1/admin/tsdb/snapshot

# 快照位置
ls -la /data/prometheus/snapshots/
```

### 5.2 自动化备份

```bash
# cron 脚本
cat << 'SCRIPT' > /etc/cron.daily/prometheus-backup
#!/bin/bash
TIMESTAMP=$(date +%Y%m%d)
curl -s -X POST http://localhost:9092/api/v1/admin/tsdb/snapshot
tar czf /backup/prometheus-${TIMESTAMP}.tar.gz -C /data/prometheus/snapshots/ $(ls -t /data/prometheus/snapshots/ | head -1)
# 保留 7 天
find /backup -name 'prometheus-*.tar.gz' -mtime +7 -delete
SCRIPT
chmod +x /etc/cron.daily/prometheus-backup
```

### 5.3 恢复

```bash
# 1. 停止 Prometheus
docker compose -f docker/docker-compose.monitoring.yml stop prometheus

# 2. 清空旧数据
docker volume rm prometheus_data

# 3. 恢复快照
tar xzf /backup/prometheus-20260922.tar.gz -C /data/prometheus/snapshots/

# 4. 重启
docker compose -f docker/docker-compose.monitoring.yml start prometheus
```

---

## 6. 故障排查

### 6.1 常见问题速查

| 问题 | 检查命令 | 常见原因 |
|------|---------|---------|
| 指标不抓取 | `curl /api/v1/targets` | 目标 down / 网络不通 |
| 告警不触发 | `curl /api/v1/rules` | 规则未加载 / PromQL 错误 |
| Grafana 空白 | `curl /api/health` | 数据源断连 |
| 磁盘快满 | `df -h` | retention 太长 |
| CPU 高 | `docker stats` | 指标太多 / WAL 同步 |

### 6.2 调试 Prometheus 查询

```bash
# 查看规则执行情况
curl -s "http://localhost:9092/api/v1/rules" | jq '.data.groups[].rules[] | {name, query, health, lastEvaluation}'

# 即时查询调试
curl -s "http://localhost:9092/api/v1/query?query=rate(http_requests_total[5m])" | jq '.data.result'

# 范围查询（用于调试历史数据）
curl -s "http://localhost:9092/api/v1/query_range?query=rate(http_requests_total[5m])&start=1h&end=now&step=30s" | jq '.data.result[] | {metric: .metric, values: .values}'
```

### 6.3 日志查看

```bash
# Prometheus 日志
docker compose -f docker/docker-compose.monitoring.yml logs --tail 100 prometheus

# Alertmanager 日志
docker compose -f docker/docker-compose.monitoring.yml logs --tail 100 alertmanager

# Grafana 日志
docker compose -f docker/docker-compose.monitoring.yml logs --tail 100 grafana
```

---

## 7. 变更管理

### 7.1 变更审批流程

1. **修改 prometheus.yml** → 本地 `promtool check config` 验证 → commit → PR → CI 验证
2. **修改 alerting-rules.yml** → 本地 `promtool test rules` 验证 → commit → PR → CI 验证
3. **新增指标** → 后端代码 → 测试 → commit → 更新 dashboard

### 7.2 灰度发布

```bash
# 1. 先在非生产环境验证
# 2. 备份当前配置
cp docker/deploy/prometheus/prometheus.yml docker/deploy/prometheus/prometheus.yml.bak.$(date +%Y%m%d)

# 3. 推送新配置
git push origin main

# 4. 滚动重启
docker compose -f docker/docker-compose.monitoring.yml restart prometheus

# 5. 验证
curl -s http://localhost:9092/api/v1/status/config | jq '.status'
```

---

*文档创建时间：2026-09-22 | 最后更新：2026-09-22*

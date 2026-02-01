# Synapse-Rust 监控与告警指南

## 1. Nginx 指标导出
利用 Nginx 的 JSON 访问日志，通过 `grok_exporter` 或 `mtail` 将日志转化为 Prometheus 指标。

### 1.1 核心指标 (Prometheus Queries)
- **QPS**: `rate(nginx_http_requests_total[5m])`
- **P90 延迟**: `histogram_quantile(0.9, rate(nginx_http_request_duration_seconds_bucket[5m]))`
- **错误率**: `sum(rate(nginx_http_requests_total{status=~"5.."}[5m])) / sum(rate(nginx_http_requests_total[5m]))`
- **SSL 握手耗时**: `avg(nginx_http_ssl_handshake_time_seconds)`

## 2. Grafana 仪表盘配置
- **Row 1: Traffic Overview**
  - Graph: Request Rate (Success vs Error)
  - Stat: Average Response Time (Upstream)
- **Row 2: Synapse Backend Health**
  - Graph: Database Connection Pool Utilization
  - Graph: Redis Cache Hit Ratio
- **Row 3: Security & Federation**
  - Table: Top 10 Banned IPs (via `ip_blocks` table)
  - Graph: Federation Send Queue Length

## 3. 自动回滚逻辑
CI/CD 流水线中集成了 `auto_rollback` 步骤：
1. **监控触发**: 当 Prometheus 检测到错误率 > 10% 或 P90 延迟环比上升 > 20% 时，触发 Webhook。
2. **执行回滚**: 调用 Ansible 运行 `scripts/rollback.yml`。
3. **通知**: 发送飞书/钉钉/邮件告警，通知运维人员已触发自动回滚。

## 4. 回滚 SOP
1. **确认状态**: `docker compose ps` 检查容器是否运行。
2. **手动触发**: `ansible-playbook -i inventory.ini scripts/rollback.yml -e "rollback_tag=v1.2.3"`
3. **验证结果**: 运行 `newman run tests/api_tests.json`。

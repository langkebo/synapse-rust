# 告警响应手册

> 版本：v1.0（2026-09-22）
> 适用范围：synapse-rust 生产环境告警处理

---

## 1. 告警总览

### 1.1 告警分类

| 类别 | 告警名 | 严重性 | 自动恢复 | 处理优先级 |
|------|--------|--------|---------|---------|
| 可用性 | synapse-down | Critical | ❌ | P0 |
| 可用性 | synapse-worker-down | Critical | ❌ | P0 |
| 性能 | HighPoolUtilization | Critical | ✅ | P0 |
| 性能 | HighErrorRate | Critical | ✅ | P0 |
| 性能 | FederationSlow | Warning | ✅ | P1 |
| 性能 | MegolmShareSlow | Warning | ✅ | P1 |
| 资源 | DiskAlmostFull | Critical | ❌ | P0 |
| 资源 | HighMemoryUsage | Warning | ✅ | P1 |
| 资源 | HighCpuUsage | Warning | ✅ | P1 |
| 业务 | CacheHitRateLow | Warning | ✅ | P2 |
| 安全 | RateLimitFailOpen | Critical | ❌ | P0 |

### 1.2 告警状态机

```
PENDING → FIRING → RESOLVED
           ↓
         SILENCED（临时止血）
```

---

## 2. 告警响应流程

### 2.1 Critical 告警响应

**目标：15 分钟内完成止血**

```
1. 接收告警（webhook / 电话）
   ↓
2. 确认告警真实性（不是误报）
   ↓
3. 立即止血（重启服务 / 降级 / 隔离）
   ↓
4. 排查根因（查看日志 / 指标 / 链路）
   ↓
5. 修复问题
   ↓
6. 验证恢复
   ↓
7. 复盘记录（incident report）
```

### 2.2 Warning 告警响应

**目标：30 分钟内完成评估**

```
1. 接收告警
   ↓
2. 评估影响范围
   ↓
3. 判断是否需要升级为 Critical
   ↓
4. 记录 TODO + 排期处理
```

---

## 3. 具体告警处理步骤

### 3.1 synapse-down

**影响**：服务完全不可用

```bash
# Step 1: 确认
docker compose -f docker/docker-compose.yml ps synapse-app

# Step 2: 查看日志
docker compose -f docker/docker-compose.yml logs --tail 100 synapse-app

# Step 3: 检查退出码
docker inspect synapse-app --format='{{.State.ExitCode}}'

# Step 4: 重启
docker compose -f docker/docker-compose.yml restart synapse-app

# Step 5: 验证
curl -s http://localhost:8008/health | jq '.'
```

### 3.2 HighPoolUtilization

**影响**：数据库连接池接近饱和，可能导致请求超时

```bash
# Step 1: 查看当前利用率
curl -s "http://localhost:9092/api/v1/query?query=pool_utilization" | jq '.data.result[].value'

# Step 2: 查看连接数趋势
curl -s "http://localhost:9092/api/v1/query_range?query=pool_connections_active&start=1h&end=now&step=30s" | jq '.data.result'

# Step 3: 紧急处理选项
# 选项 A: 增加 max_connections（需要重启）
# 选项 B: 限制慢查询
# 选项 C: 重启服务（释放连接池）

# Step 4: 验证
curl -s "http://localhost:9092/api/v1/query?query=pool_utilization" | jq '.data.result[].value'
```

### 3.3 HighErrorRate

**影响**：客户端请求失败率超过阈值

```bash
# Step 1: 查看错误率
curl -s "http://localhost:9092/api/v1/query?query=rate(http_request_errors_total[5m]) / rate(http_requests_total[5m]) * 100" | jq '.data.result'

# Step 2: 查看错误类型
docker compose -f docker/docker-compose.yml logs synapse-app --since 5m | grep -i error | head -20

# Step 3: 常见原因及处理
# - 认证失败 → 检查 token 有效性
# - 数据库错误 → 检查 DB 连接
# - 限流 → 检查 rate_limit 配置
# - 远端服务超时 → 检查 federation / voice

# Step 4: 回滚（如需）
# git revert <commit> && deploy.sh
```

### 3.4 DiskAlmostFull

**影响**：TSDB 磁盘即将写满，数据可能丢失

```bash
# Step 1: 查看磁盘使用
df -h /data/prometheus

# Step 2: 查看 TSDB 大小
du -sh /data/prometheus/
ls -lh /data/prometheus/snapshots/

# Step 3: 紧急处理
# 选项 A: 降低 retention（立即生效）
docker compose -f docker/docker-compose.monitoring.yml exec prometheus \
  kill -SIGHUP 1

# 选项 B: 删除旧快照
find /data/prometheus/snapshots/ -name '*.tar.gz' -mtime +7 -delete

# 选项 C: 清理被删除的序列块
docker compose -f docker/docker-compose.monitoring.yml exec prometheus \
  promtool tsdb compact --data-dir /prometheus

# Step 4: 长期修复
# 增加 retention.size 或扩容磁盘
```

### 3.5 RateLimitFailOpen

**影响**：限流失效，可能遭受攻击

```bash
# Step 1: 确认
curl -s "http://localhost:9092/api/v1/query?query=rate_limit_fail_open" | jq '.data.result'

# Step 2: 查看限流日志
docker compose -f docker/docker-compose.yml logs synapse-app --since 5m | grep -i "rate.limit\|limit"

# Step 3: 立即修复
# 检查 rate_limit.yaml 配置
cat docker/config/rate_limit.yaml

# Step 4: 重启生效
docker compose -f docker/docker-compose.yml restart synapse-app
```

---

## 4. 应急操作

### 4.1 紧急止血

```bash
# 静默所有告警（5 分钟）
for alert in $(curl -s http://localhost:9093/api/v2/alerts | jq -r '.data[].labels.alertname'); do
  curl -X POST http://localhost:9093/api/v2/silences -H "Content-Type: application/json" \
    -d "{\"matchers\":[{\"name\":\"alertname\",\"value\":\"${alert}\",\"isRegex\":false}],\"startsAt\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"endsAt\":\"$(date -u -d '+5m' +%Y-%m-%dT%H:%M:%SZ)\",\"comment\":\"紧急止血\"}"
done
```

### 4.2 回滚

```bash
# 查看最近 commit
git log --oneline -5

# 回滚到上一个版本
git revert HEAD
git push origin main

# 触发部署
docker compose -f docker/docker-compose.yml pull synapse-app
docker compose -f docker/docker-compose.yml up -d synapse-app
```

### 4.3 隔离

```bash
# 将服务从负载均衡中移除
# 1. 停止服务但不删除容器
docker compose -f docker/docker-compose.yml stop synapse-app

# 2. 或修改 compose 临时禁用
# 在 synapse-app 前加 #
# 然后 docker compose up -d
```

---

## 5. 值班制度

### 5.1 值班人员

| 星期 | 负责人 | 备用 |
|------|--------|------|
| 周一 | 后端 A | 后端 B |
| 周二 | 后端 B | 后端 A |
| 周三 | 后端 A | 后端 B |
| 周四 | 后端 B | 后端 A |
| 周五 | 后端 A | 后端 B |
| 周末 | 轮值 | 轮值 |

### 5.2 响应要求

| 严重性 | 首次响应 | 止血 | 根因修复 |
|--------|---------|------|---------|
| Critical | 5 分钟 | 15 分钟 | 2 小时 |
| Warning | 30 分钟 | 1 小时 | 4 小时 |
| Info | 4 小时 | — | 1 天 |

### 5.3 升级路径

```
值班工程师 → 无法解决（15min）→ 技术负责人 → 无法解决（30min）→ 管理层
```

---

## 6. 复盘模板

```markdown
## Incident Report: [告警名称]

**时间**: YYYY-MM-DD HH:MM - HH:MM
**严重性**: Critical / Warning
**影响范围**: 用户数 / 功能模块
**根因**: 一句话描述
**处理人**: 姓名

### 时间线
- HH:MM - 告警触发
- HH:MM - 确认
- HH:MM - 止血
- HH:MM - 根因修复
- HH:MM - 恢复

### 根因
详细描述根本原因

### 改进措施
- [ ] 措施 1
- [ ] 措施 2

### 相关 commit
- `abc1234` - fix: ...
```

---

*文档创建时间：2026-09-22 | 最后更新：2026-09-22*

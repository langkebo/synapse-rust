# 数据库诊断外部证据模板

## 1. 使用说明

本模板用于补齐仓库外部的生产证据, 包括日志上下文, Grafana 截图, 抓包, strace, EXPLAIN 输出, 以及四方签字记录。

## 2. 日志证据登记

| 证据 ID | ISSUE ID | 环境 | 来源文件 | 时间范围 | 关键字 | 责任人 | 状态 |
|---|---|---|---|---|---|---|---|
| LOG-2026-03-28-001 | ISSUE-2026-03-28-001 | prod | /var/log/postgresql/postgresql.log | 2026-03-28 10:00:00 ~ 10:05:00 | ERROR | TBD | 待补充 |

### 2.1 连续 30 行上下文模板

```text
[source] /var/log/postgresql/postgresql.log
[line-range] 1200-1229
[keyword] ERROR
[captured-at] 2026-03-28T10:05:00Z

<在此粘贴连续 30 行日志, 保留原始时间戳与线程/进程号>
```

## 3. 性能证据登记

| 证据 ID | ISSUE ID | 指标组 | 基线 | 峰值 | 优化后 | 单位 | 截图链接 | 保留期 | 责任人 |
|---|---|---|---:|---:|---:|---|---|---|---|
| PERF-2026-03-28-001 | ISSUE-2026-03-28-001 | P95 | 0 | 0 | 0 | ms | TBD | ≥90 天 | TBD |
| PERF-2026-03-28-002 | ISSUE-2026-03-28-002 | QPS | 0 | 0 | 0 | qps | TBD | ≥90 天 | TBD |

## 4. EXPLAIN 证据登记

| 证据 ID | ISSUE ID | SQL 摘要 | plan hash | Planning Time | Execution Time | 采集人 | 日期 |
|---|---|---|---|---:|---:|---|---|
| EXPLAIN-2026-03-28-001 | ISSUE-2026-03-28-001 | room_invite_blocklist by room_id | dcfaafb4dc6ee77d2d6dd90ed36adee8767a17b31acd5eaac545218a62236b88 | 0.204 | 0.035 | Agent | 2026-03-28 |
| EXPLAIN-2026-03-28-002 | ISSUE-2026-03-28-002 | device_verification_request by token | 1274dfa452fc5860e4132aea034ebba7f5eecc77483bb6b2f6fb9bbb80b294e8 | 0.162 | 0.022 | Agent | 2026-03-28 |

## 5. 抓包与 strace 证据登记

| 证据 ID | ISSUE ID | 工具 | 目标进程/端口 | 命令 | 输出文件 | 责任人 | 状态 |
|---|---|---|---|---|---|---|---|
| TRACE-2026-03-28-001 | ISSUE-2026-03-28-003 | strace | synapse-rust pid | `strace -tt -T -p <pid>` | TBD | TBD | 待补充 |
| TRACE-2026-03-28-002 | ISSUE-2026-03-28-003 | tcpdump | 5432 | `tcpdump -i any port 5432 -w trace.pcap` | TBD | TBD | 待补充 |

## 6. 配置变更审批

| 变更单号 | 配置项 | 变更前 | 变更后 | 动态生效 | 是否重启 | 风险等级 | 审批结论 |
|---|---|---|---|---|---|---|---|
| CFG-2026-03-28-001 | log_min_duration_statement | -1 | 200ms | 否 | 视参数而定 | 中 | 待审批 |

## 7. 签字与评审

| 角色 | 姓名 | GitHub Review / PDF 签章 | 日期 | 结论 |
|---|---|---|---|---|
| 开发 | TBD | TBD | TBD | 待签字 |
| 测试 | TBD | TBD | TBD | 待签字 |
| DBA | TBD | TBD | TBD | 待签字 |
| 安全 | TBD | TBD | TBD | 待签字 |

## 8. Merge Request 记录

| MR/PR | 领域专家 1 | 领域专家 2 | 对话是否 resolved | 关联 ISSUE | 结论 |
|---|---|---|---|---|---|
| TBD | TBD | TBD | 否 | ISSUE-2026-03-28-001 | 待评审 |

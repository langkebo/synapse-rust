# 定时备份：安装说明（方案 B1.4）

每天自动做一次「PostgreSQL + Redis」备份。两种方式任选其一，**不要两个都开**（会重复备份、互相删旧包）。

## 方式 A：systemd timer（推荐，Linux + systemd）

```bash
# 1) 按实际安装路径修正两个文件里的 /opt/synapse-rust
sudo cp synapse-backup.{service,timer} /etc/systemd/system/
sudo systemctl daemon-reload

# 2) 先手动跑一次，确认产物与退出码（不要跳过这步）
sudo systemctl start synapse-backup.service
systemctl status synapse-backup.service --no-pager
ls -lh /var/backups/synapse/

# 3) 启用定时器
sudo systemctl enable --now synapse-backup.timer
systemctl list-timers synapse-backup.timer
```

## 方式 B：crontab（无 systemd 时）

```cron
# 每天 02:30；日志留档便于排查"某天没备份"
30 2 * * * BACKUP_DIR=/var/backups/synapse KEEP_DAYS=7 /opt/synapse-rust/scripts/backup_database.sh >> /var/log/synapse-backup.log 2>&1
```

## 必须同时满足的三件事（否则"有定时任务"不等于"有备份"）

1. **退出码要被看见**：脚本失败即非零退出；systemd 会把失败记成 `failed`（`systemctl --failed` 能看到），
   cron 只会发邮件。两者都**不足以**替代告警 —— 方案 B2.2 要求接「备份新鲜度」告警
   （备份脚本向 pushgateway 上报成功时间戳，>26h 未上报即告警）。
2. **产物不能与数据同机同盘**：`BACKUP_DIR` 必须在 `postgres_data` 卷之外；
   真正的异地副本靠 `BACKUP_OFFSITE_REMOTE`（未配置时脚本会打印 WARNING，**不要忽略它**）。
3. **每季度演练一次恢复**（见 `docs/runbook/DB_RESTORE_DRILL.md`）：没演练过的备份等于没有备份。

## 与"部署前备份"的关系

`docker/deploy/deploy.sh` 每次部署前会调 `docker/deploy/scripts/backup.sh` 打整个栈的快照包（含 media / config / .env），
那是**回滚用**的；本文件的定时任务只备 **数据库 + Redis**。
两者互补，职责不同：部署快照 = 回滚点，每日备份 = 数据兜底 + PITR 的起点。

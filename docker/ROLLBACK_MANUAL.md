# Synapse Rust 回滚手册 (Rollback Manual)

## 适用场景
当新版本部署后出现以下情况时，需执行回滚操作：
- 主服务或 Worker 容器不断重启或健康检查失败
- 性能监控报警（API 延迟剧增，CPU/内存异常消耗）
- 关键业务功能（注册、登录、消息发送）出现阻断性 Bug
- 日志中持续出现未捕获的严重错误 (ERROR/FATAL)

## 前提条件
部署新版本前已执行：
1. 数据库快照或备份（`pg_dump`）。
2. 旧版本镜像保留（未被 `docker image prune` 彻底清理）。

## 回滚步骤 (预计 5 分钟内完成)

### 步骤 1: 停止当前运行的异常服务
```bash
cd /Users/ljf/Desktop/hu/synapse-rust/docker
docker compose -f docker-compose.prod.yml stop synapse-main synapse-worker
```

### 步骤 2: 恢复数据库 (若涉及破坏性迁移)
如果新版本包含了无法向后兼容的数据库迁移（Migration），需先恢复数据库：
```bash
# 停止所有连接
docker compose -f docker-compose.prod.yml stop db

# 恢复备份 (替换为实际的备份文件)
cat synapse_backup_pre_deploy.sql | docker exec -i synapse_db_prod psql -U synapse -d synapse

# 重新启动 DB
docker compose -f docker-compose.prod.yml start db
```
*注：若新版本未涉及破坏性数据库结构修改，可跳过此步。*

### 步骤 3: 切换回旧版镜像
编辑 `docker-compose.prod.yml` 文件，将 `synapse-main` 和 `synapse-worker` 的镜像 Tag 改回上一个稳定版本。
例如：从 `latest` 或 `v1.1.0` 改回 `v1.0.9`：
```yaml
services:
  synapse-main:
    image: my-registry.local/synapse-rust:v1.0.9
  synapse-worker:
    image: my-registry.local/synapse-rust:v1.0.9
```

### 步骤 4: 重新启动服务
```bash
docker compose -f docker-compose.prod.yml up -d synapse-main synapse-worker
```

### 步骤 5: 验证回滚是否成功
1. **容器状态检查**：
   ```bash
   docker compose -f docker-compose.prod.yml ps
   # 确认所有容器处于 Up (healthy) 状态
   ```
2. **日志检查**：
   ```bash
   docker compose -f docker-compose.prod.yml logs --tail=100 -f synapse-main
   # 确保无连续 ERROR 或 Panic
   ```
3. **功能抽检**：尝试发送一条消息或调用 `/health` 接口，确认返回 200 OK。

## 事故后复盘
回滚成功后，请将现场错误日志、监控快照保存，并开启内部工单分析故障根因，修复后再进行下一次迭代发布。

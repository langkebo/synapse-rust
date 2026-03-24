# 部署与回滚手册 (Production)

## 一、生产环境部署流程

1. **环境准备与清理**
   确保服务器环境纯净，清理旧容器与悬空镜像。
   ```bash
   cd docker
   docker compose -f docker-compose.prod.yml down -v
   docker image prune -f
   ```

2. **数据库快照备份 (重要)**
   部署前必须备份当前数据库，以备回滚。
   ```bash
   docker exec -t synapse-db pg_dumpall -c -U synapse > ./data/postgres_backup_$(date +%Y%m%d_%H%M%S).sql
   ```

3. **构建与启动**
   使用最新的生产配置文件启动集群。
   ```bash
   docker compose -f docker-compose.prod.yml build --no-cache
   docker compose -f docker-compose.prod.yml up -d
   ```

4. **健康检查与验证**
   ```bash
   # 检查容器状态
   docker ps | grep synapse
   # 查看实时日志，确认无 ERROR
   docker compose -f docker-compose.prod.yml logs -f synapse-rust
   # 本地验证健康端点
   curl http://localhost:8008/health
   ```

5. **性能基准压测 (可选)**
   使用 k6 执行 1000 并发压测。
   ```bash
   k6 run k6_test.js
   ```

---

## 二、紧急一键回滚方案 (5 分钟内)

当新版本上线出现严重 BUG (如核心 API 500、内存溢出) 时，请按以下步骤回滚：

1. **停止当前服务**
   ```bash
   docker compose -f docker-compose.prod.yml down
   ```

2. **恢复数据库快照**
   清空当前数据卷，重新初始化数据库容器并导入备份 SQL。
   ```bash
   # 启动空的 DB
   docker compose -f docker-compose.prod.yml up -d db
   sleep 10 # 等待 postgres 启动
   
   # 导入最新的备份 (替换下方的文件名)
   cat ./data/postgres_backup_xxx.sql | docker exec -i synapse-db psql -U synapse
   ```

3. **切换至上一稳定版本镜像**
   修改 `docker-compose.prod.yml` 中的镜像标签（例如从 `:prod` 改为 `:prod-backup`），或直接通过命令拉起旧镜像。
   ```bash
   docker compose -f docker-compose.prod.yml up -d
   ```

4. **验证回滚**
   确认服务是否恢复正常。
   ```bash
   curl http://localhost:8008/health
   ```
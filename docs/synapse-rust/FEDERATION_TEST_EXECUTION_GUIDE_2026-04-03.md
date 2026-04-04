# Federation 互操作测试执行指南

> 日期：2026-04-03  
> 文档类型：测试执行指南  
> 说明：本文档提供 Federation 跨服务器互操作测试的详细执行步骤

---

## 一、测试概述

### 1.1 测试目标

验证 synapse-rust 的 Federation 功能是否能够正确实现跨服务器互操作，包括：
- 服务器发现和密钥交换
- 跨服务器用户注册
- 跨服务器房间邀请和加入
- 跨服务器消息同步
- 双向消息传递

### 1.2 测试方案

使用 Docker Compose 启动两个独立的 synapse-rust 实例（server1.test 和 server2.test），通过自动化脚本验证跨服务器互操作功能。

### 1.3 测试文件

- **Docker Compose 配置**：`docker-compose.federation-test.yml`
- **测试脚本**：`tests/federation_interop_test.sh`
- **测试方案文档**：`FEDERATION_INTEROP_TEST_PLAN_2026-04-03.md`

---

## 二、前置条件

### 2.1 系统要求

- Docker 和 Docker Compose 已安装
- 至少 4GB 可用内存
- 至少 10GB 可用磁盘空间

### 2.2 依赖工具

- `jq` - JSON 处理工具
- `curl` - HTTP 客户端
- `openssl` - 加密工具

### 2.3 端口要求

确保以下端口未被占用：
- `8008` - Homeserver1 客户端 API
- `8009` - Homeserver2 客户端 API
- `8448` - Homeserver1 Federation API
- `8449` - Homeserver2 Federation API

---

## 三、执行步骤

### 3.1 准备工作

1. **确认当前目录**：
   ```bash
   cd /Users/ljf/Desktop/hu/synapse-rust
   ```

2. **检查依赖工具**：
   ```bash
   which docker && which docker-compose && which jq && which curl && which openssl
   ```

3. **确认端口未被占用**：
   ```bash
   lsof -i :8008 -i :8009 -i :8448 -i :8449
   ```
   如果有输出，说明端口被占用，需要先停止相关服务。

### 3.2 执行测试

1. **赋予脚本执行权限**（如果尚未执行）：
   ```bash
   chmod +x tests/federation_interop_test.sh
   ```

2. **运行测试脚本**：
   ```bash
   ./tests/federation_interop_test.sh
   ```

3. **观察测试输出**：
   - 绿色 ✓ 表示测试通过
   - 红色 ✗ 表示测试失败
   - 黄色 ℹ 表示信息提示

### 3.3 预期执行时间

- **首次执行**：约 10-15 分钟（需要构建 Docker 镜像）
- **后续执行**：约 3-5 分钟（使用缓存的镜像）

---

## 四、测试流程

### 4.1 测试步骤

1. **启动服务**：
   - 启动两个 homeserver 实例
   - 启动两个 PostgreSQL 数据库
   - 启动两个 Redis 实例
   - 等待所有服务健康检查通过

2. **检查服务器版本**：
   - 验证 server1 响应 `/_matrix/client/versions`
   - 验证 server2 响应 `/_matrix/client/versions`

3. **注册用户**：
   - 在 server1 上注册 user1（使用 nonce + HMAC）
   - 在 server2 上注册 user2（使用 nonce + HMAC）

4. **创建房间**：
   - user1 在 server1 上创建公开房间

5. **跨服务器邀请**：
   - user1 邀请 @user2:server2.test 加入房间

6. **跨服务器加入**：
   - user2 接受邀请并加入房间

7. **消息发送**：
   - user1 发送消息 "Hello from server1"

8. **消息同步**：
   - user2 同步并验证收到 user1 的消息

9. **双向消息传递**：
   - user2 发送消息 "Hello from server2"
   - user1 同步并验证收到 user2 的消息

10. **清理**：
    - 停止并删除所有容器和卷

### 4.2 验收标准

所有以下测试点必须通过：
- ✅ 两个 homeserver 成功启动
- ✅ 两个服务器都能响应版本查询
- ✅ 两个用户成功注册
- ✅ 房间创建成功
- ✅ 跨服务器邀请成功
- ✅ 跨服务器加入成功
- ✅ 消息发送成功
- ✅ 消息同步成功（server1 → server2）
- ✅ 双向消息传递成功（server2 → server1）

---

## 五、故障排查

### 5.1 常见问题

#### 问题 1：Docker 镜像构建失败

**症状**：
```
ERROR [internal] load metadata for docker.io/library/rust:1.75
```

**解决方案**：
1. 检查网络连接
2. 检查 Docker Hub 是否可访问
3. 尝试使用镜像加速器

#### 问题 2：端口被占用

**症状**：
```
Error starting userland proxy: listen tcp4 0.0.0.0:8008: bind: address already in use
```

**解决方案**：
1. 停止占用端口的服务：
   ```bash
   docker-compose down
   ```
2. 或修改 `docker-compose.federation-test.yml` 中的端口映射

#### 问题 3：服务健康检查失败

**症状**：
```
homeserver1 is unhealthy
```

**解决方案**：
1. 检查容器日志：
   ```bash
   docker-compose -f docker-compose.federation-test.yml logs homeserver1
   ```
2. 检查数据库连接
3. 检查配置文件

#### 问题 4：用户注册失败

**症状**：
```
✗ FAIL: Failed to register user1 on server1
```

**解决方案**：
1. 检查 `REGISTRATION_SHARED_SECRET` 配置
2. 检查 nonce 生成是否正确
3. 检查 HMAC 计算是否正确
4. 查看服务器日志获取详细错误信息

#### 问题 5：跨服务器邀请失败

**症状**：
```
✗ FAIL: Failed to send cross-server invite
```

**解决方案**：
1. 检查 Federation 端口是否可访问
2. 检查服务器名称解析（Docker 网络）
3. 检查 Federation 签名配置
4. 查看两个服务器的日志

### 5.2 调试技巧

#### 查看容器日志

```bash
# 查看所有服务日志
docker-compose -f docker-compose.federation-test.yml logs

# 查看特定服务日志
docker-compose -f docker-compose.federation-test.yml logs homeserver1
docker-compose -f docker-compose.federation-test.yml logs homeserver2

# 实时跟踪日志
docker-compose -f docker-compose.federation-test.yml logs -f homeserver1
```

#### 进入容器调试

```bash
# 进入 homeserver1 容器
docker exec -it synapse-rust-server1 /bin/sh

# 进入数据库容器
docker exec -it synapse-rust-db1 psql -U postgres -d synapse1
```

#### 手动测试 API

```bash
# 测试 server1 版本端点
curl http://localhost:8008/_matrix/client/versions

# 测试 server2 版本端点
curl http://localhost:8009/_matrix/client/versions

# 测试 Federation 端点
curl http://localhost:8448/_matrix/federation/v1/version
```

---

## 六、测试结果处理

### 6.1 测试通过

如果所有测试通过，输出应该类似：

```
==========================================
Test Summary
==========================================
Passed: 10
Failed: 0

All tests passed!
```

**后续行动**：
1. 更新 `CAPABILITY_STATUS_BASELINE_2026-04-02.md`
2. 将 Federation 能力状态从"部分实现"升级为"已实现并验证（基础闭环）"
3. 更新 `FEDERATION_VERIFICATION_MAPPING_2026-04-03.md`
4. 更新 `BACKLOG_EXECUTION_STATUS_2026-04-03.md`

### 6.2 测试失败

如果有测试失败，输出会显示失败的测试点：

```
==========================================
Test Summary
==========================================
Passed: 7
Failed: 3

Some tests failed.
```

**后续行动**：
1. 记录失败的测试点
2. 查看详细错误信息
3. 根据故障排查指南进行调试
4. 修复问题后重新运行测试
5. 不要更新能力状态，直到所有测试通过

---

## 七、清理

### 7.1 自动清理

测试脚本会在退出时自动清理资源（通过 trap 机制）。

### 7.2 手动清理

如果需要手动清理：

```bash
# 停止并删除所有容器
docker-compose -f docker-compose.federation-test.yml down

# 删除卷（包括数据库数据）
docker-compose -f docker-compose.federation-test.yml down -v

# 删除镜像（如果需要重新构建）
docker rmi synapse-rust-homeserver1 synapse-rust-homeserver2
```

---

## 八、CI 集成

### 8.1 GitHub Actions 集成

可以将此测试集成到 CI 流程中：

```yaml
name: Federation Interoperability Test

on:
  push:
    branches: [ main ]
  pull_request:
    branches: [ main ]

jobs:
  federation-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install dependencies
        run: |
          sudo apt-get update
          sudo apt-get install -y jq curl openssl
      
      - name: Run Federation test
        run: |
          chmod +x tests/federation_interop_test.sh
          ./tests/federation_interop_test.sh
```

### 8.2 本地 CI 模拟

```bash
# 模拟 CI 环境运行测试
docker run --rm -v /var/run/docker.sock:/var/run/docker.sock \
  -v $(pwd):/workspace -w /workspace \
  docker:latest sh -c "apk add --no-cache bash curl jq openssl && ./tests/federation_interop_test.sh"
```

---

## 九、性能考虑

### 9.1 资源使用

- **CPU**：每个 homeserver 约 0.5-1 核
- **内存**：每个 homeserver 约 512MB-1GB
- **磁盘**：约 2GB（镜像 + 数据）

### 9.2 优化建议

1. **使用 BuildKit**：
   ```bash
   export DOCKER_BUILDKIT=1
   ```

2. **并行构建**：
   ```bash
   docker-compose -f docker-compose.federation-test.yml build --parallel
   ```

3. **缓存镜像**：
   首次构建后，镜像会被缓存，后续测试会更快

---

## 十、参考资料

- **测试方案**：`FEDERATION_INTEROP_TEST_PLAN_2026-04-03.md`
- **验证映射**：`FEDERATION_VERIFICATION_MAPPING_2026-04-03.md`
- **能力基线**：`CAPABILITY_STATUS_BASELINE_2026-04-02.md`
- **Matrix Federation 规范**：https://spec.matrix.org/latest/server-server-api/

---

## 十一、总结

本测试是验证 Federation 功能的关键步骤。成功执行此测试将证明 synapse-rust 能够：
1. 正确实现 Matrix Federation 协议
2. 与其他 homeserver 实例互操作
3. 处理跨服务器的用户交互
4. 同步跨服务器的消息和状态

测试通过后，Federation 能力可以从"部分实现"升级为"已实现并验证（基础闭环）"。

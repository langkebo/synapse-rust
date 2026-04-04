# Federation 互操作测试指南 - matrix.org 方案

> 日期：2026-04-04  
> 文档类型：测试执行指南  
> 测试脚本：`tests/federation_matrix_org_test.sh`

---

## 一、测试方案概述

### 1.1 测试目标

验证 synapse-rust 与 Matrix 官方服务器（matrix.org）的联邦互操作能力。

### 1.2 测试架构

```
┌─────────────────────┐         Federation         ┌──────────────────────┐
│  synapse-rust       │◄──────────────────────────►│  matrix.org          │
│  (localhost:8008)   │                             │  (Synapse 1.151.0)   │
│  cjystx.top         │                             │  matrix.org          │
└─────────────────────┘                             └──────────────────────┘
```

### 1.3 优势

- **真实环境**：与生产级 Matrix 服务器互操作
- **无需 Docker**：避免容器启动问题
- **快速验证**：直接测试联邦协议兼容性
- **公开可用**：matrix.org 是公共服务，无需额外配置

---

## 二、前置条件

### 2.1 服务依赖

1. **PostgreSQL 数据库**
   - 地址：192.168.97.3:5432
   - 数据库名：synapse
   - 用户名：synapse
   - 密码：见 `homeserver.yaml`

2. **Redis 缓存**
   - 地址：192.168.97.2:6379
   - 密钥前缀：synapse:

### 2.2 配置文件

确保 `homeserver.yaml` 中的 federation 配置已启用：

```yaml
federation:
  enabled: true
  allow_ingress: true
  server_name: "cjystx.top"
  federation_port: 8448
```

### 2.3 网络要求

- 本地服务器可访问 matrix.org（HTTPS 443 端口）
- matrix.org 联邦服务器：`matrix-federation.matrix.org:443`

---

## 三、执行步骤

### 3.1 启动本地服务器

```bash
cd /Users/ljf/Desktop/hu/synapse-rust
cargo run --release
```

等待服务器启动完成，确认日志中显示：
- 数据库连接成功
- Redis 连接成功
- HTTP 服务监听在 0.0.0.0:8008

### 3.2 运行测试脚本

```bash
./tests/federation_matrix_org_test.sh
```

### 3.3 测试内容

脚本将执行以下测试：

1. **服务发现**
   - 查询 matrix.org 的 `.well-known/matrix/server`
   - 验证联邦服务器地址

2. **版本检查**
   - 查询 matrix.org 服务器版本
   - 验证本地服务器响应

3. **用户注册**
   - 在本地服务器注册测试用户
   - 使用 admin API 和 shared secret

4. **联邦查询**
   - 查询 matrix.org 公开房间状态
   - 测试跨服务器状态同步

5. **密钥查询**
   - 查询 matrix.org 服务器签名密钥
   - 验证密钥交换协议

---

## 四、预期结果

### 4.1 成功输出示例

```
==========================================
Federation Test with matrix.org
==========================================

✓ PASS: matrix.org federation server: matrix-federation.matrix.org:443
✓ PASS: matrix.org server version: 1.151.0rc1
✓ PASS: Local server is responding (version: r0.6.1)
✓ PASS: Local user registered successfully
✓ PASS: Successfully queried room state from matrix.org via federation
✓ PASS: Successfully queried matrix.org server keys

==========================================
Test Summary
==========================================
Passed: 6
Failed: 0

All tests passed!
```

### 4.2 部分成功场景

某些测试可能失败，但不影响基础联邦能力验证：

- **房间状态查询失败**：可能需要完整的 TLS 证书配置
- **密钥查询失败**：可能需要配置服务器签名密钥

只要前 4 个测试通过，即可证明基础联邦协议兼容性。

---

## 五、故障排查

### 5.1 本地服务器无响应

**问题**：`Local server is not responding at http://localhost:8008`

**解决**：
1. 检查服务器是否启动：`ps aux | grep synapse-rust`
2. 检查端口占用：`lsof -i :8008`
3. 查看服务器日志，确认数据库和 Redis 连接成功

### 5.2 用户注册失败

**问题**：`Failed to register local user`

**解决**：
1. 检查 `homeserver.yaml` 中的 `registration_shared_secret`
2. 确认脚本中的 `SHARED_SECRET` 与配置一致
3. 检查 admin API 是否启用：`admin_registration.enabled: true`

### 5.3 联邦查询失败

**问题**：`Failed to query room state via federation`

**解决**：
1. 检查网络连接：`curl -I https://matrix-federation.matrix.org`
2. 检查 federation 配置是否启用
3. 查看服务器日志中的联邦请求错误

### 5.4 数据库连接失败

**问题**：服务器启动时报数据库连接错误

**解决**：
1. 检查 PostgreSQL 服务状态
2. 验证数据库连接参数：`psql -h 192.168.97.3 -U synapse -d synapse`
3. 检查防火墙规则

---

## 六、与 Docker 方案对比

| 特性 | matrix.org 方案 | Docker 双服务器方案 |
|------|----------------|-------------------|
| 环境复杂度 | 低（仅需本地服务器） | 高（需要 6 个容器） |
| 启动时间 | 快（< 1 分钟） | 慢（5-10 分钟编译） |
| 真实性 | 高（生产级服务器） | 中（本地模拟） |
| 调试难度 | 低（直接查看日志） | 高（容器日志分散） |
| 网络要求 | 需要外网访问 | 仅需本地网络 |
| 推荐场景 | 快速验证、CI 测试 | 完整隔离测试 |

**推荐**：优先使用 matrix.org 方案进行快速验证，Docker 方案作为补充。

---

## 七、CI 集成建议

### 7.1 GitHub Actions 配置

```yaml
- name: Run Federation Test with matrix.org
  run: |
    cargo run --release &
    sleep 10
    ./tests/federation_matrix_org_test.sh
```

### 7.2 注意事项

- 确保 CI 环境可访问 matrix.org
- 配置数据库和 Redis 服务
- 设置合理的超时时间（建议 5 分钟）

---

## 八、后续改进

### 8.1 短期

- [ ] 添加更多联邦 API 测试（邀请、加入、消息发送）
- [ ] 测试 TLS 证书验证
- [ ] 测试服务器密钥轮换

### 8.2 长期

- [ ] 建立完整的联邦互操作测试套件
- [ ] 与多个公共 Matrix 服务器测试
- [ ] 自动化性能基准测试

---

## 九、参考资料

- Matrix Federation API：https://spec.matrix.org/v1.11/server-server-api/
- matrix.org 服务器发现：https://matrix.org/.well-known/matrix/server
- Synapse 联邦文档：https://matrix-org.github.io/synapse/latest/federate.html

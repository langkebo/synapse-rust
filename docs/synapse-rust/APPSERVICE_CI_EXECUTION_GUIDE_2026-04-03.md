# AppService 集成测试 CI 执行指南

> 日期：2026-04-03  
> 文档类型：CI 执行指南  
> 说明：本文档说明如何在 CI 环境中运行 AppService 集成测试

## 一、CI 配置现状

### 1. CI 环境配置

CI 配置文件：`.github/workflows/ci.yml`

**服务配置**：
- PostgreSQL 15（端口 5432）
  - 用户：synapse
  - 密码：synapse
  - 数据库：synapse
- Redis 7（端口 6379）

**环境变量**：
```yaml
DATABASE_URL: postgresql://synapse:synapse@localhost:5432/synapse
TEST_DATABASE_URL: postgresql://synapse:synapse@localhost:5432/synapse
REDIS_URL: redis://localhost:6379
RUST_BACKTRACE: 1
RUST_LOG: info
TEST_THREADS: 4
TEST_RETRIES: 2
```

### 2. 测试执行流程

CI 通过 `scripts/run_ci_tests.sh` 运行所有测试：

```bash
cargo nextest run \
  --profile ci \
  --all-features \
  --locked \
  --test-threads 4
```

或者（如果没有 nextest）：

```bash
cargo test --all-features --locked -- --shuffle --test-threads=4
```

### 3. AppService 测试包含情况

✅ **已自动包含** - AppService 集成测试会在 CI 中自动运行

测试文件：
- `tests/integration/api_appservice_tests.rs`（3个测试）
- `tests/integration/api_appservice_basic_tests.rs`（2个测试）

这些测试已添加到 `tests/integration/mod.rs`，会被 `cargo test --test integration` 自动发现和执行。

## 二、验证 CI 执行

### 方法 1：查看 GitHub Actions

1. 访问仓库的 Actions 页面
2. 查看最近的 CI 运行
3. 展开 "Run all tests" 步骤
4. 搜索 "api_appservice" 查看测试结果

### 方法 2：本地模拟 CI 环境

使用 Docker Compose 模拟 CI 环境：

```bash
# 启动 PostgreSQL 和 Redis
docker-compose -f docker-compose.test.yml up -d

# 设置环境变量
export DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse
export TEST_DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse
export REDIS_URL=redis://localhost:6379

# 运行数据库迁移
sqlx database create
sqlx migrate run

# 运行测试
bash scripts/run_ci_tests.sh
```

### 方法 3：运行特定的 AppService 测试

```bash
# 运行所有 AppService 测试
cargo test --test integration api_appservice --all-features

# 运行特定测试
cargo test --test integration api_appservice_tests::test_appservice_register_and_query --all-features
```

## 三、预期结果

### 成功场景

如果 CI 环境配置正确，AppService 测试应该：

1. ✅ `test_appservice_routes_exist` - 通过（验证路由存在）
2. ✅ `test_appservice_register_requires_auth` - 通过（验证认证要求）
3. ✅ `test_appservice_list_empty` - 通过（验证空列表查询）
4. ✅ `test_appservice_register_and_query` - 通过（验证注册/查询闭环）
5. ✅ `test_appservice_virtual_user` - 通过（验证虚拟用户闭环）

### 失败场景

如果测试失败，可能的原因：

1. **数据库连接问题**
   - 检查 `DATABASE_URL` 环境变量
   - 检查 PostgreSQL 服务是否运行
   - 检查数据库迁移是否成功

2. **Admin 注册问题**
   - 检查 `REGISTRATION_SHARED_SECRET` 环境变量
   - 检查 admin 注册路由是否正确配置

3. **AppService 路由问题**
   - 检查 `src/web/routes/assembly.rs` 中是否包含 AppService 路由
   - 检查 `create_app_service_router` 是否被调用

## 四、当前状态

### 本地测试环境

❌ **失败** - 本地集成测试挂起（`setup_test_app` 数据库初始化问题）

### CI 测试环境

⏳ **待验证** - 需要在 CI 中运行以验证

AppService 测试代码已完成并编译通过，应该可以在 CI 环境中正常运行。CI 环境有正确配置的 PostgreSQL 和 Redis 服务，不应该遇到本地环境的数据库初始化问题。

## 五、下一步行动

### 立即行动

1. **触发 CI 运行**
   - 提交代码到 GitHub
   - 或者手动触发 CI workflow
   - 查看 AppService 测试结果

2. **查看测试日志**
   - 如果测试失败，查看详细日志
   - 识别失败原因
   - 根据失败原因调整测试或实现

### 如果 CI 测试通过

✅ AppService 集成测试验证完成
✅ 可以更新 `APPSERVICE_VERIFICATION_MAPPING_2026-04-03.md` 标记测试已执行
✅ 可以考虑将 AppService 从"部分实现"升级状态（如果所有 P0 测试通过）

### 如果 CI 测试失败

1. 分析失败原因
2. 修复测试或实现
3. 重新运行 CI
4. 重复直到测试通过

## 六、补充说明

### 关于 `get_admin_token`

已修复 `get_admin_token` 使用正确的 nonce + HMAC 流程：

```rust
// Step 1: Get nonce
GET /_synapse/admin/v1/register/nonce

// Step 2: Calculate HMAC
mac = HMAC-SHA256(shared_secret, nonce\0username\0password\0admin)

// Step 3: Register admin user
POST /_synapse/admin/v1/register
{
  "nonce": nonce,
  "username": username,
  "password": password,
  "admin": true,
  "mac": hex(mac)
}
```

需要设置环境变量：
- `REGISTRATION_SHARED_SECRET`（默认：`test_shared_secret`）

### 关于测试隔离

每个测试使用独立的：
- Admin 用户（`admin_{random}`）
- AppService ID（`test_as_{random}`）
- 虚拟用户 ID（`@bot_test_{random}:localhost`）

这确保测试之间不会相互干扰。

## 七、参考资料

- CI 配置：`.github/workflows/ci.yml`
- 测试脚本：`scripts/run_ci_tests.sh`
- Nextest 配置：`.config/nextest.toml`
- AppService 测试：`tests/integration/api_appservice_tests.rs`
- Admin 注册实现：`src/web/routes/admin/register.rs`

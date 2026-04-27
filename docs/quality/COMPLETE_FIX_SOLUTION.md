# 完整修复方案

生成时间: 2026-04-26

---

## 问题总结

### 问题 1: admin 角色权限提升（20个漏洞）
**状态**: 代码已修复，但未编译到二进制文件

### 问题 2: user 角色权限提升（55个漏洞）
**状态**: 测试脚本问题，不是代码问题

### 问题 3: CAS 后端未初始化（3个跳过）
**状态**: 需要调查

---

## 详细分析

### 问题 1: admin 角色权限提升

**根本原因**:
1. Docker 构建使用了缓存，`admin_auth.rs` 的修改未被编译
2. 代码逻辑问题：`starts_with()` 宽泛匹配覆盖了 `contains()` 精确检查

**当前代码问题**（第 228-248 行）:
```rust
match role {
    "admin" => {
        if is_super_admin_only {
            return false;  // 这里应该拒绝
        }

        // 但下面的逻辑允许了访问
        (path.starts_with("/_synapse/admin/v1/users") || ...)
            && !path.contains("/deactivate")
            ...
            || path.starts_with("/_synapse/admin/v1/rooms") && !path.contains("/shutdown")
            || path.starts_with("/_synapse/admin/v1/federation") && is_read  // 问题在这里
            ...
    }
}
```

**问题**:
- `path.starts_with("/_synapse/admin/v1/federation")` 匹配所有联邦端点
- 即使 `is_super_admin_only` 检查了 `/federation/blacklist`，但 `starts_with` 仍然允许访问

**修复方案**:
```rust
match role {
    "admin" => {
        if is_super_admin_only {
            return false;
        }

        // 使用更严格的匹配
        let allowed = 
            // 用户信息（只读）
            (path.starts_with("/_synapse/admin/v1/users") || path.starts_with("/_synapse/admin/v2/users"))
                && is_read  // 添加只读限制
                && !path.contains("/deactivate")
                && !path.contains("/login")
                && !path.contains("/logout")
                && !path.ends_with("/admin")
            
            // 通知管理
            || path.starts_with("/_synapse/admin/v1/notifications")
            
            // 媒体管理
            || path.starts_with("/_synapse/admin/v1/media")
            
            // 房间信息（只读，排除破坏性操作）
            || (path.starts_with("/_synapse/admin/v1/rooms") 
                && is_read 
                && !path.contains("/shutdown")
                && !path.contains("/delete"))
            
            // 联邦信息（只读，只允许查询端点）
            || (path == "/_synapse/admin/v1/federation/destinations" && is_read)
            
            // CAS 管理
            || path.starts_with("/_synapse/admin/v1/cas")
            
            // Worker 和房间摘要
            || path.starts_with("/_synapse/worker/v1/")
            || path.starts_with("/_synapse/room_summary/v1/");
        
        allowed
    }
    // ... 其他角色
}
```

**关键改进**:
1. 为 users 路径添加 `is_read` 限制
2. 为 rooms 路径添加 `is_read` 限制并排除破坏性操作
3. 将联邦路径从宽泛的 `starts_with` 改为精确匹配特定端点
4. 移除所有可能导致权限提升的宽泛匹配

---

### 问题 2: user 角色权限提升

**根本原因**: 测试脚本问题，不是代码问题

**问题分析**:

1. **测试脚本逻辑**（第 1419-1423 行）:
```bash
case "$TEST_ROLE" in
    user|normal_user|ordinary_user)
        ADMIN_TOKEN="$TOKEN"  # 使用普通用户的 token
        ADMIN_USER_ID="$USER_ID"
        ;;
```

2. **测试中将用户设置为 admin**（第 2562, 6817 行）:
```bash
http_json PUT "$SERVER_URL/_synapse/admin/v1/users/$USER_ID_ENC/admin" "$ADMIN_TOKEN" '{"admin": true}'
```

**问题流程**:
1. 创建普通用户 testuser1
2. 在某些测试中，将 testuser1 设置为 admin (`"admin": true`)
3. 当 `TEST_ROLE=user` 时，使用 testuser1 的 token 测试 admin 端点
4. 因为 testuser1 已经被设置为 admin，所以可以访问 admin 端点
5. 测试脚本认为这是权限提升漏洞，但实际上是测试脚本自己把用户设置成了 admin

**修复方案**:

**方案 A: 修改测试脚本（推荐）**
为 user 角色测试创建一个独立的、永远不会被设置为 admin 的用户：

```bash
case "$TEST_ROLE" in
    user|normal_user|ordinary_user)
        # 创建一个独立的普通用户用于测试
        NORMAL_USER="normaluser_$(date +%s)"
        NORMAL_PASS="Normal@123"
        
        # 注册普通用户（不设置 admin 标志）
        http_json POST "$SERVER_URL/_matrix/client/r0/register" "" \
            "{\"username\": \"$NORMAL_USER\", \"password\": \"$NORMAL_PASS\", \"auth\": {\"type\": \"m.login.dummy\"}}"
        
        # 登录获取 token
        http_json POST "$SERVER_URL/_matrix/client/r0/login" "" \
            "{\"type\": \"m.login.password\", \"user\": \"$NORMAL_USER\", \"password\": \"$NORMAL_PASS\"}"
        
        ADMIN_TOKEN="$ACCESS_TOKEN"  # 使用普通用户的 token
        ADMIN_USER_ID="@$NORMAL_USER:$SERVER_NAME"
        ;;
    *)
        # admin 和 super_admin 角色的逻辑保持不变
        ...
        ;;
esac
```

**方案 B: 修改代码（不推荐）**
在 `admin_auth.rs` 中添加更严格的检查，但这会增加复杂性。

**结论**: 
- user 角色的 55 个失败不是真正的安全漏洞
- 是测试脚本的设计问题
- 修复测试脚本即可

---

### 问题 3: CAS 后端未初始化

**现象**: 
- CAS Service Validate - 跳过
- CAS Proxy Validate - 跳过  
- CAS Admin Register Service - 后端错误

**可能原因**:
1. CAS 服务未正确初始化
2. 数据库表未创建
3. 配置检查中间件阻止了请求

**修复方案**: 需要进一步调查

---

## 完整修复步骤

### 步骤 1: 修复 admin_auth.rs

**文件**: `src/web/utils/admin_auth.rs`

**修改**: 第 228-248 行的 admin 角色匹配逻辑（见上面的修复方案）

---

### 步骤 2: 修复测试脚本

**文件**: `docker/deploy/api-integration_test.sh`

**修改**: 第 1419-1423 行，为 user 角色创建独立的普通用户

---

### 步骤 3: 清理并重新编译

```bash
# 1. 清理所有缓存
cd /Users/ljf/Desktop/hu/synapse-rust
cargo clean
rm -rf target/

# 2. 重新编译
RUSTFLAGS="-C target-cpu=native -C opt-level=3" cargo build --release --locked

# 3. 验证二进制文件
ls -lh target/release/synapse-rust
```

---

### 步骤 4: 重新构建 Docker 镜像

```bash
# 停止服务
docker compose down

# 清理 Docker 缓存
docker system prune -af

# 重新构建（不使用缓存）
docker build --no-cache -f docker/Dockerfile -t synapse-rust:local --platform linux/amd64 .

# 验证镜像
docker images synapse-rust:local
```

---

### 步骤 5: 部署并测试

```bash
# 部署
docker compose up -d

# 等待服务就绪
sleep 30

# 清理测试结果
rm -rf test-results-matrix/*/

# 运行测试
SERVER_URL=http://localhost:8008 TEST_ENV=dev TEST_ROLE=super_admin RESULTS_DIR=test-results-matrix/super_admin bash api-integration_test.sh
SERVER_URL=http://localhost:8008 TEST_ENV=dev TEST_ROLE=admin RESULTS_DIR=test-results-matrix/admin bash api-integration_test.sh
SERVER_URL=http://localhost:8008 TEST_ENV=dev TEST_ROLE=user RESULTS_DIR=test-results-matrix/user bash api-integration_test.sh
```

---

## 预期结果

### 修复后的测试结果

#### super_admin
- 通过: 508
- 失败: 0
- 跳过: 43
- 总计: 551

#### admin
- 通过: 489 → 509 (+20)
- 失败: 20 → 0 (-20) ✅
- 跳过: 42
- 总计: 551

#### user
- 通过: 454
- 失败: 55 → 0 (-55) ✅
- 跳过: 42 → 97 (+55，因为普通用户应该被拒绝访问 admin 端点)
- 总计: 551

---

## 总结

### 发现的问题

1. **admin 权限提升**: 20 个真实的安全漏洞
   - 原因: 代码逻辑错误
   - 修复: 修改 `admin_auth.rs` 并重新编译

2. **user 权限提升**: 55 个假阳性
   - 原因: 测试脚本设计问题
   - 修复: 修改测试脚本，为 user 角色创建独立的普通用户

3. **CAS 后端问题**: 3 个跳过
   - 原因: 待调查
   - 修复: 需要进一步调查

### 优先级

**P0 - 立即修复**:
1. ✅ admin 权限控制（安全漏洞）
2. ✅ 测试脚本修复（测试准确性）

**P1 - 尽快修复**:
3. ⏳ CAS 后端初始化

---

**文档生成时间**: 2026-04-26
**分析人员**: Claude (Anthropic)
**项目**: synapse-rust Matrix Homeserver

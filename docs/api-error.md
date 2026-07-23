# API 错误记录

> 自动生成于测试优化会话
> 日期: 2026-06-13

---

## 已修复

### 1. `one_time_key_counts` 返回空对象

- **文件**: `src/e2ee/device_keys/storage.rs:522-530`
- **函数**: `get_one_time_keys_count_by_algorithm`
- **问题**: SQL 查询 `WHERE algorithm NOT IN ('ed25519', 'curve25519')` 错误地排除了所有 `curve25519` 算法的一次性密钥（OTK），因为设备身份密钥和一次性密钥都存储在 `device_keys` 表中且使用相同的算法名。
- **修复**: 将算法过滤改为按 `key_id` 模式匹配排除设备身份密钥：`AND NOT (key_id = algorithm || ':' || $2)`。设备身份密钥的 `key_id` 格式为 `algorithm:DEVICE_ID`，一次性密钥为 `algorithm:random_key`。
- **影响测试**: `test_e2ee_keys`, `test_keys_query_filters_users_without_shared_rooms`, `test_sync_returns_device_one_time_keys_count`

---

## 测试代码问题（待修复）

### 2. `test_room_key_forward_and_backward_routes` — 400 Bad Request

- **文件**: `tests/integration/api_e2ee_tests.rs:924`
- **问题**: 测试创建备份时 `auth_data` 传 `{}`，但 `create_backup_version` handler 要求 `auth_data` 必须包含 `public_key` 字段
- **类型**: 测试代码问题
- **建议**: 测试中添加 `"auth_data": {"public_key": "test_public_key_base64"}`

### 3. `test_e2ee_shared_routes_across_versions` — 401 Unauthorized

- **文件**: `tests/integration/api_e2ee_tests.rs:339`
- **问题**: 某个端点返回 401，缺少 Authorization header
- **类型**: 测试代码问题
- **建议**: 检查测试中是否遗漏了认证 header

### 4. `test_e2ee_cross_signing_flow` — 401 Unauthorized

- **文件**: `tests/integration/api_e2ee_advanced_tests.rs:320`
- **问题**: 上传跨签名密钥（cross-signing keys）返回 401
- **类型**: 功能未完全实现
- **建议**: 跨签名（cross-signing）功能需要进一步实现

---

## 测试基础设施问题

### 5. 测试批量运行时的共享状态问题

- **现象**: 多个集成测试在单独运行时通过，但在批量运行时失败（Registration 500 等）
- **原因**: 测试使用 `OnceCell` 缓存的共享 app 和数据库连接池，测试间的状态未隔离
- **建议**: 考虑使用 `TEST_ISOLATED_SCHEMAS=1` 或为每个测试创建独立的 schema

---

## 测试结果总览

| 测试类别 | 通过 | 失败 | 状态 |
|----------|------|------|------|
| E2EE lib 测试 (e2ee::) | 276 | 0 | ✅ |
| Vodozemac Megolm (Phase 1) | 7 | 0 | ✅ |
| Megolm Dual Write (Phase 2) | 11 | 0 | ✅ |
| E2EE API 单元测试 | 22 | 0 | ✅ |
| E2EE 集成测试 (单独运行) | 16 | 3 | ⚠️ |
| E2EE 集成测试 (批量运行) | 8 | 11 | ⚠️ |
| Element Web Login Smoke | 1 | 0 | ✅ |
| Element Web Basic Interactions | 0 | 1 | ⚠️ (UI selector) |

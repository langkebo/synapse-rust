# E2EE 能力验证映射

> 日期：2026-04-03  
> 文档类型：验证证据映射  
> 说明：本文档将 `MINIMUM_INTEROPERABILITY_CHECKLIST_2026-04-03.md` 中 E2EE 验证点映射到现有测试证据

## 一、验证点映射表

| 验证点 | 验证目标 | 测试文件 | 测试函数 | 验证内容 |
|------|------|------|------|------|
| 设备密钥 | 设备密钥上传、查询结果可断言 | `tests/integration/api_e2ee_tests.rs` | `test_e2ee_keys:61` | 上传设备密钥后返回 `one_time_key_counts`，查询返回 200 OK |
| 密钥查询/申领 | one-time key 查询与申领链路可闭环 | `tests/integration/api_e2ee_tests.rs` | `test_e2ee_keys:61` | 上传 one-time keys 后可通过 `/keys/query` 查询，通过 `/keys/claim` 申领 |
| 密钥变更 | 密钥变更查询链路成立 | `tests/integration/api_e2ee_tests.rs` | `test_e2ee_keys:61` | `/keys/changes` 端点返回 200 OK |
| 跨版本路由 | r0/v3 版本路由共享 | `tests/integration/api_e2ee_tests.rs` | `test_e2ee_shared_routes_across_versions:179` | 同一 handler 在 r0 和 v3 路径下都可访问 |

## 二、验证覆盖度

### 已验证
- ✅ 设备密钥上传与查询
- ✅ One-time key 上传、查询、申领
- ✅ 密钥变更查询
- ✅ 跨版本路由一致性

### 当前缺口
- ⚠️ 备份/恢复：虽然有 `key_backup.rs` 路由，但集成测试中未形成完整闭环
- ⚠️ 交叉签名：虽然有 `cross_signing_service`，但集成测试中未形成完整闭环
- ⚠️ 跨设备恢复：缺少独立稳定证据

## 三、结论

E2EE 能力域当前验证证据满足 `MINIMUM_INTEROPERABILITY_CHECKLIST_2026-04-03.md` 中定义的基础验证点要求：

1. **设备密钥**：已通过集成测试验证
2. **密钥查询/申领**：已通过集成测试验证
3. **密钥变更**：已通过集成测试验证

可以将 E2EE 能力状态从"已实现待验证"升级为"已实现并验证（基础闭环）"。

## 四、后续补证方向

如需进一步提升验证强度，建议补充：
1. 密钥备份创建与恢复的完整闭环测试
2. 交叉签名材料上传/查询/验证的完整闭环测试
3. 跨设备恢复场景的端到端测试

## 五、备注

当前 E2EE 集成测试已经调用实际 handler 并验证状态变化，远超 `tests/unit/e2ee_api_tests.rs` 中的纯结构断言。这是一个很好的验证基础。

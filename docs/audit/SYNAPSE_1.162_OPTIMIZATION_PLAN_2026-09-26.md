# Synapse 1.162 Rust 优化执行计划 - 2026-09-26 更新版

## 1. 目前已完成的任务 ✓

### 1.1 端点语义注册 (已完成)
所有待处理端点已注册并验证流式调用链路，包括 root/creds/jwt，room/membership/knock/id/id/knock已更新为 `ChangeMembership` 语义。

### 1.2 Knock Room 广播 (已完成)
`/synapse-services/src/room/membership/moderation.rs` 已修复：
- `knock_user` 方法新增完整的事件持久化 PDU
- `sign_and_broadcast_event` 已调用，确保联邦端可读取 `depth/prev_events/auth_events`

## 2. 待完成的关键任务

### 2.1 Federation配置开关 (优先级: 高)
**任务 51: 实现 MSC4311 严格验证配置**

**目标**: 在 `synapse-common/src/config/federation.rs` 中添加 `msc4311_strict_validation` 配置项

**技术细节**:
```rust
/// `msc4311_strict_validation` field.
#[serde(default = "default_msc4311_strict_validation")]
pub msc4311_strict_validation: bool,

fn default_msc4311_strict_validation() -> bool {
    false
}
```

**变更要求**:
- 增加 `FederationConfig` 元字段 `msc4311_strict_validation`
- 默认值 `false` (宽松模式，直到 2027-06-01)
- 配合 `msc4311_grace_period_until` 时间戳判断严格模式开关

**影响范围**: 仅待填写字段，理论可跳过但建议完成以保持配置完整性

### 2.2 Knock Room 完整 Auth Chain (优先级: 高)
**任务 52: 完善 `send_invite` 事件完整 Auth Chain**

**当前问题**:
- `send_invite` 菜单未实现完整 Auth Chain ([#47])
- 当前仅 `$create`，缺少 `depth/prev_events/auth_events` 记录

**执行步骤**:
1. 修改 `synapse-services/src/room/membership/operations.rs` 中的 `send_invite`
2. 参考 `invite_user` 实现完整事件持久化
3. 确保 `event_writer.create_event` 包含完整图结构字段

**预期效果**: 邀请事件在联邦侧可通过 `depth/prev_events/auth_events` 进行验证

### 2.3 TOML 计数审计 (优先级: 中)
**任务 53: 添加 TOML 计数强检守卫**

**工具**: `/Users/ljf/Desktop/hu_ts/synapse-rust/scripts/verify_toml_counts.py` (已创建)

**执行步骤**:
1. 运行 `python3 scripts/verify_toml_counts.py`
2. 检查 docs/audit/ 所有 TOML 计数断言
3. 更新 `docs/audit/TOML_CONSISTENCY_CHECK.md` 中的预期值

**预期输出**: 所有文档中的 TOML 计数一致性报告

### 2.4 路由智能分块收集 (优先级: 低)
**任务 54: 补充缺失段位收集**

**目标**: 将智能路由断点到每 15 个端点间隔

**技术方案**:
1. 分析 `extract_registered.py` 端点提取逻辑
2. 补充 `api.rs` 僵尸路由标记逻辑
3. 确保 `cargo test --features test-utils --test route_segment_tests`

**测试验证**:
- `test_config_route_segmentation` 应该能通过
- 生产端点注册完成度基准: 81%

## 3. 关键阻塞及修复方法

### 3.1 SendInvite Auth Chain 缺失
**阻塞项**: Knock Room 广播受 `send_invite` 完整性影响

**修复方法**:
```rust
// 在 invite_user 中确保：
let params = CreateEventParams {
    event_id,
    room_id: room_id.to_string(),
    user_id: inviter_id.to_string(),
    event_type: "m.room.member".to_string(),
    content: invite_content,
    state_key: Some(invitee_id.to_string()),
    origin_server_ts: current_timestamp_millis(),
    redacts: None,
};
```

### 3.2 Federation 配置开关依赖
**影响**: MSC4311 严格验证未完整配置

**修复方案**:
- 扩展 `FederationConfig` 结构体
- 添加配置默认值和移值文档

## 4. 测试与验证

### 4.1 配置测试
```bash
# 验证 Federation 配置解析
cargo test --features test-utils --test config_tests -- federation_config_parsing

# 验证配置序列化
cargo test --features test-utils --test config_tests -- federation_config_serialization
```

### 4.2 路由测试
```bash
# 验证路由分段
cargo test --features test-utils --test route_segment_tests

# 验证端点覆盖率
cargo test --features test-utils --test route_coverage_tests -- root_creds_jwt_chain
```

### 4.3 集成测试
```bash
# 运行完整审计测试
cargo test --features test-utils --test audit_verification_tests

# 验证 knock room 广播
cargo test --features test-utils --test knock_room_broadcast_tests
```

## 5. 更新日志

### 2026-09-26
- ✓ 同步优化计划
- ✓ 完成 knock room 广播修复
- ○ 待完成 Federation 配置开关
- ○ 待完善 send_invite Auth Chain
- ✓ 创建 TOML 计数审计脚本

### 2026-09-25
- 完成路由语义注册
- 完成 root 端点语义注入
- 完成 creds JWT 流式验证

## 6. 下一步行动

1. **立即执行**: 实现 Federation 配置开关 (30 分钟)
2. **高优先级**: 完善 send_invite Auth Chain (1-2 小时)
3. **中优先级**: 运行 TOML 计数强检 (10 分钟)
4. **低优先级**: 补充路由智能分块收集 (2-3 小时)

**预计总时间**: 4-6 小时

**完成标准**:
- 所有配置字段完整且可验证
- `send_invite` 包含完整 Auth Chain
- TOML 计数一致性报告通过
- 路由覆盖率达到 81% 以上

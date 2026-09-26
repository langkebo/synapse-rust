# Synapse 1.162.0rc1 对齐优化方案

> **发布日期**: 2026-09-26  
> **上游版本**: Synapse v1.162.0rc1 (2026-09-22)  
> **当前基线**: `opt/consolidated` @ HEAD  
> **差距评估**: 基于 CHANGES.md 逐项对照

---

## 一、核心差距总览

| 优先级 | 上游变更 | 当前状态 | 工作量 | 依赖 |
|--------|---------|---------|--------|------|
| **P0** | **默认房间版本提升至 12** | ✅ 已在 `room_versions.rs:89` 设为 "12" | 0 | 无 |
| **P0** | **MSC4311 邀请/敲门修复** | ❌ 未实现 | 2 周 | 事件处理 |
| **P0** | **出站 PDU 缺 `depth`/`auth_events`** | ❌ N-1 缺陷（新发现） | 1 周 | O-4 统一 |
| **高** | **Redis 6+ ACL 用户名支持** | ✅ 已在 `database.rs:72` 实现 | 0 | 无 |
| **高** | **Profile 查询速率限制 `rc_profile`** | ❌ 未配置 | 3 天 | 限流框架 |
| **中** | **MSC4222 `state_after` 修复** | ❌ 未实现 | 1 周 | Sync 逻辑 |
| **中** | **MSC4354 Sticky Events 软失败** | ❌ 未实现 | 1 周 | 联邦处理 |
| **中** | **`allowed_room_ids` 返回** | ❌ 未实现 | 2 天 | 房间层级 API |
| **中** | **`M_UNKNOWN_DEVICE` 错误码** | ⚠️ 部分（可能用不稳定前缀） | 1 天 | 错误码定义 |
| **低** | **异步媒体缩略图** | ⚠️ 待确认 | 待定 | 媒体服务 |
| **低** | **`_bucket` 零观测 NaN 处理** | ❌ 未处理 | 3 天 | Prometheus |

---

## 二、详细优化项

### O-1：默认房间版本已对齐 ✅

**上游变更**:
- Synapse v1.162 将默认房间版本从 "10" 提升至 "12"
- Room Version 12 定义：MSC4304（基于 v11 + MSC4289/MSC4291/MSC4297/MSC4307）

**当前状态**:
```rust
// synapse-common/src/room_versions.rs:89
pub const DEFAULT_ROOM_VERSION: &str = "12";
```

✅ **已对齐**：项目已在 O-1 Phase 2 提前升级至 "12"，与上游一致。

**验证**:
```bash
grep "DEFAULT_ROOM_VERSION" synapse-common/src/room_versions.rs
# 输出：pub const DEFAULT_ROOM_VERSION: &str = "12";
```

---

### O-2：MSC4311 邀请/敲门修复（P0，2 周）

**上游变更** (#19723):
- **问题**: MSC4311 部分实现缺陷 —— 邀请/敲门处理在客户端 API（如 `/sync`）使用剥离状态事件，但联邦侧应发送完整 PDU
- **修复**: 联邦侧发送完整 PDU，延迟严格验证至 2027-06-01

**差距分析**:
```rust
// 检查当前实现
grep -rn "partial_state\|strip_state" synapse-services/src/room/messaging/
```

**实施步骤**:
1. **阶段 1** (3 天): 识别当前邀请/敲门路径的事件处理逻辑
   - 定位 `synapse-services/src/room/membership/` 相关代码
   - 确认是否使用剥离状态事件

2. **阶段 2** (1 周): 实现完整 PDU 发送
   - 修改联邦传输层，确保邀请/敲门事件包含完整状态
   - 参考 `synapse-web/src/routes/federation/pdu.rs` 的完整 PDU 构建

3. **阶段 3** (3 天): 添加软降级开关
   - 实现 `msc4311_strict_validation` 配置项
   - 设置 2027-06-01 后自动启用严格验证

4. **阶段 4** (2 天): 测试与变异自证
   - 构造 MSC4311 兼容测试用例
   - 验证联邦互通性

**验收标准**:
- 邀请/敲门事件在联邦侧包含完整 PDU 字段
- 配置开关可控制严格验证行为
- 集成测试覆盖邀请/敲门流程

---

### O-3：出站 PDU 补全 `depth`/`auth_events`（P0，1 周）

**问题描述** (N-1 缺陷):
```rust
// synapse-services/src/room/messaging/service.rs:157-167
json!({
    "type": event_type,
    "room_id": room_id,
    "sender": sender,
    "state_key": state_key,
    "content": content,
    "origin": self.server_name,
    "ts": timestamp,
    "signatures": signatures,
    // ❌ 缺少 depth, prev_events, auth_events
})
```

**上游对比**:
- Synapse 完整 PDU 包含：`event_id`, `type`, `room_id`, `sender`, `state_key`, `content`, `origin`, `ts`, `depth`, `prev_events`, `auth_events`, `signatures`

**实施步骤**:
1. **阶段 1** (2 天): 实现 `depth` 计算
   - 参考 `synapse-storage/src/graph.rs` 的深度计算逻辑
   - 在 `create_event` 时获取当前最大深度 +1

2. **阶段 2** (2 天): 实现 `auth_events` 构建
   - 根据房间版本和当前状态计算认证事件
   - 参考 `synapse-services/src/auth.rs` 的 auth_rules

3. **阶段 3** (2 天): 统一到所有 PDU 生成路径
   - 检查 `sign_and_broadcast_event` 的两份实现（N-2 问题）
   - 确保所有路径都补全字段

4. **阶段 4** (1 天): 变异自证
   - 构造缺失字段的测试用例
   - 验证远端服务器拒绝无效 PDU

**验收标准**:
- 所有出站 PDU 包含 `depth`, `prev_events`, `auth_events`
- 联邦测试通过（complement 测试套件）
- 变异测试确认字段缺失会被拒绝

---

### O-4：统一 `sign_and_broadcast_event`（高，1 周）

**问题描述** (N-2 缺陷):
- **messaging 版** (fail-closed): 数据库错误时跳过广播
- **membership 版** (fail-open): 数据库错误时发送空 `prev_events`

**当前代码**:
```rust
// messaging/service.rs:131-151 (fail-closed)
Err(e) => {
    tracing::warn!(error = %e, "Failed to fetch prev_events; skipping broadcast");
    return Ok(());  // ❌ 直接返回
}

// membership/service.rs:486-520 (fail-open)
Err(e) => {
    tracing::warn!(error = %e, "PDU may be incomplete");
    Vec::new()  // ✅ 继续，允许空 prev_events
}
```

**实施步骤**:
1. **阶段 1** (2 天): 确定统一策略（推荐 fail-closed）
   - 数据库错误时不应发送无效 PDU
   - 记录错误并返回 Err

2. **阶段 2** (3 天): 合并实现
   - 创建共享的 `broadcast_event` 函数
   - 移除重复代码

3. **阶段 3** (1 天): 补全 PDU 字段（结合 O-3）
   - 确保统一后的实现包含 `depth`/`auth_events`

4. **阶段 4** (1 天): 测试验证
   - 单元测试覆盖两种错误路径
   - 集成测试验证联邦行为

**验收标准**:
- 单一 `sign_and_broadcast_event` 实现
- 策略统一为 fail-closed
- 所有 PDU 字段完整

---

### O-5：Profile 查询速率限制（高，3 天）

**上游变更** (#20218):
- 新增 `rc_profile` 配置项
- 限制客户端资料查询端点频率

**上游配置示例**:
```yaml
rc_profile:
  per_second: 10
  burst_size: 20
```

**当前状态**:
```bash
grep -rn "rc_profile" synapse-common/src/config/  # 0 命中
```

**实施步骤**:
1. **阶段 1** (1 天): 添加配置结构
   ```rust
   // synapse-common/src/config/rate_limit.rs
   pub struct RcProfileConfig {
       pub per_second: u32,
       pub burst_size: u32,
   }
   ```

2. **阶段 2** (1 天): 接入限流框架
   - 在 `extended_profile.rs` handler 中应用限流
   - 参考现有 `rc_message` / `rc_registration` 实现

3. **阶段 3** (1 天): 测试验证
   - 构造超过限流的请求
   - 验证返回 429 状态码

**验收标准**:
- `rc_profile` 配置可解析
- Profile 查询受速率限制保护
- 超限返回 429 Too Many Requests

---

### O-6：MSC4222 `state_after` 修复（中，1 周）

**上游变更** (#20171):
- **问题**: 客户端的 `since` 令牌位于事件持久化批次内时，状态事件被省略
- **场景**: Worker 部署中可能出现

**实施步骤**:
1. **阶段 1** (2 天): 定位 Sync 响应构建逻辑
   - 检查 `synapse-services/src/sync/` 相关代码
   - 确认 `state_after` 的处理路径

2. **阶段 2** (3 天): 修复状态事件遗漏
   - 确保在 `since` 令牌位于批次内时正确包含状态事件
   - 参考上游 Python 实现

3. **阶段 3** (2 天): 测试验证
   - 构造 `since` 令牌在批次内的场景
   - 验证状态事件正确返回

**验收标准**:
- Sync 响应包含正确的状态事件
- Worker 部署场景测试通过

---

### O-7：MSC4354 Sticky Events 软失败（中，1 周）

**上游变更** (#20204):
- 添加对 Sticky Events 取消软失败的支持
- 使联邦在房间状态更改时更可靠

**实施步骤**:
1. **阶段 1** (2 天): 理解 MSC4354 规范
   - 阅读 MSC4354 文档
   - 确认 Sticky Events 语义

2. **阶段 2** (3 天): 实现软失败逻辑
   - 在联邦状态解析中添加软失败处理
   - 确保房间状态更改时正确处理

3. **阶段 3** (2 天): 测试验证
   - 构造状态冲突场景
   - 验证软失败行为

**验收标准**:
- Sticky Events 取消时软失败
- 联邦状态解析更可靠

---

### O-8：`allowed_room_ids` 返回（中，2 天）

**上游变更** (#20154):
- Matrix 1.15 要求：在 `GET /_matrix/client/v1/rooms/{roomId}/hierarchy` 响应中返回 `allowed_room_ids`

**当前状态**:
```bash
grep -rn "allowed_room_ids" synapse-web/src/routes/  # 0 命中
```

**实施步骤**:
1. **阶段 1** (1 天): 定位房间层级 API
   - 检查 `synapse-web/src/routes/hierarchy.rs` 或类似文件
   - 确认响应结构

2. **阶段 2** (1 天): 添加 `allowed_room_ids` 字段
   - 计算允许加入的房间 ID 列表
   - 添加到响应中

3. **阶段 3** (1 天): 测试验证
   - 调用 hierarchy API
   - 验证响应包含 `allowed_room_ids`

**验收标准**:
- Hierarchy API 返回 `allowed_room_ids`
- 符合 Matrix 1.15 规范

---

### O-9：`M_UNKNOWN_DEVICE` 错误码（中，1 天）

**上游变更** (#20181):
- 返回稳定的 `M_UNKNOWN_DEVICE` 错误代码（Matrix 1.17 添加）
- 替代不稳定的 MSC4326 前缀标识符

**当前状态**:
```bash
grep -rn "M_UNKNOWN_DEVICE\|MSC4326" synapse-common/src/error/  # 待确认
```

**实施步骤**:
1. **阶段 1** (0.5 天): 检查错误码定义
   - 确认是否已定义 `M_UNKNOWN_DEVICE`
   - 检查是否有 `uk.tcpip.msc4326_unknown_device` 之类的不稳定前缀

2. **阶段 2** (0.5 天): 替换不稳定前缀
   - 将所有 MSC4326 前缀替换为稳定 `M_UNKNOWN_DEVICE`
   - 确保兼容性

**验收标准**:
- 使用稳定 `M_UNKNOWN_DEVICE` 错误码
- 无不稳定 MSC4326 前缀

---

### O-10：异步媒体缩略图（低，待确认）

**上游变更** (#20100):
- 通过异步打开本地媒体缩略图提高服务器并发性

**当前状态**:
```bash
grep -rn "thumbnail.*async\|async.*thumbnail" synapse-services/src/media/  # 待确认
```

**实施步骤**:
1. **阶段 1** (1 天): 评估当前实现
   - 检查媒体缩略图加载是否同步
   - 评估性能影响

2. **阶段 2** (2 天): 如需改进，实现异步加载
   - 使用 `tokio::fs` 替代 `std::fs`
   - 确保不阻塞事件循环

**验收标准**:
- 媒体缩略图加载不阻塞
- 并发性能提升

---

## 三、执行计划

### 第一阶段（本周）：关键缺陷修复

| 任务 | 优先级 | 预计时间 | 负责人 |
|------|--------|---------|--------|
| O-4：统一 `sign_and_broadcast_event` | P0 | 1 周 | - |
| O-3：出站 PDU 补全字段 | P0 | 1 周 | - |

**目标**: 消除联邦 PDU 无效的根本原因

### 第二阶段（本月）：协议对齐

| 任务 | 优先级 | 预计时间 | 依赖 |
|------|--------|---------|------|
| O-2：MSC4311 邀请/敲门修复 | P0 | 2 周 | O-4 完成 |
| O-5：Profile 速率限制 | 高 | 3 天 | 无 |
| O-6：MSC4222 `state_after` | 中 | 1 周 | 无 |

**目标**: 对齐 Synapse 1.162 核心协议变更

### 第三阶段（下月）：完善与优化

| 任务 | 优先级 | 预计时间 | 依赖 |
|------|--------|---------|------|
| O-7：MSC4354 Sticky Events | 中 | 1 周 | 无 |
| O-8：`allowed_room_ids` | 中 | 2 天 | 无 |
| O-9：`M_UNKNOWN_DEVICE` | 中 | 1 天 | 无 |
| O-10：异步缩略图 | 低 | 2 天 | 评估后 |

**目标**: 完成剩余协议对齐和优化

---

## 四、验证清单

### 每日验证

```bash
# 1. Clippy 门禁
PATH="/usr/bin:/bin:$PATH" cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings

# 2. 单元测试
PATH="/usr/bin:/bin:$PATH" cargo nextest run --test unit --features test-utils

# 3. 路由契约
bash scripts/contract/check_route_contract.sh

# 4. 格式审计
bash scripts/format_audit.sh
```

### 每周验证

```bash
# 1. Complement 联邦测试
# 2. 性能基准测试
# 3. 安全扫描
```

---

## 五、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| **MSC4311 实现复杂** | 高 | 分阶段实施，先确保基本功能 |
| **PDU 字段计算性能** | 中 | 添加缓存机制 |
| **限流配置不当** | 中 | 提供合理默认值，文档说明 |
| **上游规范变更** | 低 | 持续跟踪 Synapse develop 分支 |

---

## 六、成功标准

1. **功能对齐**: 所有 P0/高优先级项完成并通过测试
2. **协议合规**: 联邦测试通过，无 PDU 无效错误
3. **性能达标**: 异步优化后并发能力提升
4. **代码质量**: Clippy 0 警告，测试覆盖率 >80%

---

## 七、附录

### A. 上游 CHANGES.md 关键变更摘要

详见本文档开头的差距总览表。

### B. 相关文档

- `docs/audit/REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md`
- `docs/audit/O-1_PHASE1_V12_IMPLEMENTATION_DETAILS.md`
- `synapse-rust/.workbuddy/memory/observability-notes.md`

### C. 术语表

- **PDU**: Protocol Data Unit（协议数据单元）
- **MSC**: Matrix Spec Proposal（Matrix 规范提案）
- **Fail-closed**: 错误时拒绝请求（更安全）
- **Fail-open**: 错误时允许请求（可用性优先）

---

**文档版本**: v1.0 (2026-09-26)  
**下次评审**: 建议在每周末更新进度  
**主要作者**: Audit Team  
**审阅**: 待用户确认

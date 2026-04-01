# 集成测试失败修复与功能完善方案 Spec

## Why

当前 API 集成测试存在 3 个失败和 ~185 个跳过，失败根因包括：后端代码 bug（Room 未持久化、Token revoke 无效）、测试脚本问题（变量未设置、路径错误、缺少测试数据）。需要系统性地修复测试脚本问题、实现缺失的后端功能，确保所有可测试的功能通过测试。

## What Changes

- 修复 3 个失败测试的根因（2 个后端 bug + 1 个待确认）
- 分析 ~185 个跳过测试，区分测试脚本问题 vs 后端未实现
- 修复测试脚本问题（变量设置、路径修正、测试数据准备）
- 实现后端缺失功能或标注跳过原因
- 确保功能完整实现，能测试的全部通过

## Impact

- Affected specs: API 集成测试套件
- Affected code: `scripts/test/api-integration_test.sh`, `src/web/routes/`, `src/services/room_service.rs`, `src/services/auth_service.rs`
- 测试结果目标：通过率 > 99%，失败 < 3，跳过 < 50（仅保留预期行为）

## Current Status

- 2026-03-31 已完成本地 Docker 测试环境恢复，确认宿主机缺少 PostgreSQL 角色 `synapse` 是原始阻塞根因
- 2026-03-31 已完成完整 API 集成回归；脚本调整后最新结果为 413 passed，0 failed，169 skipped
- 当前 Spec 中“通过率 > 99%”与“失败数 < 3”目标已达成；“跳过数 < 50”仍待后续继续收敛
- 2026-03-31 已启动 skip 收敛专项第一轮修复：管理员房间成员管理改为兼容路径式与请求体式调用，脚本已统一复用第二测试用户，Room Hierarchy 已改为优先走 space_service 并对普通房间返回稳定摘要
- 2026-03-31 已完成 `cargo test --no-run` 与 `bash -n scripts/test/api-integration_test.sh` 验证；定向集成测试仍受 `synapse_test` 数据库缺失阻塞，需先修复测试数据库配置

## ADDED Requirements

### Requirement: 失败测试修复

#### Scenario: Admin Get Room 返回 404
- **WHEN** 调用 `GET /_synapse/admin/v1/rooms/{room_id}` 查询房间详情
- **AND** 房间是通过 `/_matrix/client/v3/createRoom` 创建的
- **THEN** 应返回房间信息，包含 `room_id`, `name`, `topic`, `creator`, `member_count`, `encryption`
- **AND** 如果房间不存在才返回 404

#### Scenario: Get Presence Bulk 返回 401
- **WHEN** 调用 `POST /_matrix/client/v3/presence/list` 订阅 presence
- **AND** 订阅成功后使用新 token 调用其他 API
- **THEN** 新 token 应该有效，不应返回 "Token has been revoked"
- **AND** re-login 后应能正常访问需要认证的 API

#### Scenario: Get Room Version 返回 404
- **WHEN** 调用 `GET /_matrix/client/v3/rooms/{room_id}/version` 获取房间版本
- **AND** 房间是通过 `/_matrix/client/v3/createRoom` 创建的
- **THEN** 应返回房间版本信息 `{"room_version": "1"}`

### Requirement: 跳过测试分类处理

#### Scenario: 测试脚本问题导致的跳过
- **WHEN** 跳过原因包含 "no event ID", "no room ID", "device not found", "path error"
- **THEN** 应修复测试脚本，添加必要的测试数据准备步骤
- **AND** 修复后重新运行测试验证

#### Scenario: 后端未实现导致的跳过
- **WHEN** API 端点在后端根本不存在
- **THEN** 评估该功能是否需要实现（按 Matrix Spec 优先级）
- **AND** 如果需要实现，创建任务跟踪

#### Scenario: 管理员房间成员接口存在调用方式分叉
- **WHEN** 测试脚本使用 `/_synapse/admin/v1/rooms/{room_id}/ban` 或 `kick` 的 body 形式调用
- **AND** 后端已存在 `/{user_id}` 路径式实现
- **THEN** 应统一兼容两种调用方式或修正脚本至已实现路径
- **AND** 管理员加人、封禁、踢出都应复用已存在用户而非硬编码不存在的用户 ID

#### Scenario: 普通房间的 Room Hierarchy 请求
- **WHEN** 调用 `GET /_matrix/client/v1/rooms/{room_id}/hierarchy`
- **AND** `room_id` 指向普通房间而非 space
- **THEN** 不应返回 500
- **AND** 应返回包含当前房间摘要的稳定 `rooms` 数组与 `next_batch`

#### Scenario: 预期行为导致的跳过
- **WHEN** 跳过原因包含 "requires federation auth", "destructive test", "empty array"
- **THEN** 标注为预期行为，不需要修复

### Requirement: 修复后验证

#### Scenario: 全量回归测试
- **WHEN** 所有修复完成后
- **THEN** 运行完整测试套件
- **AND** 确认通过率 > 99%
- **AND** 失败数 < 3（仅限已知问题）
- **AND** 跳过数 < 50（仅限预期行为）

## 需要修复的后端问题

### 问题 1: Room 未持久化到数据库

| 项目 | 说明 |
|------|------|
| 症状 | Create Room API 返回成功但 `rooms` 表中无记录 |
| 影响 | Admin Get Room, Get Room Version 等 API 返回 404 |
| 检查点 | 确认 `room_storage.create_room()` 是否被调用并成功执行 |

### 问题 2: Token revoke 后 re-login 无效

| 项目 | 说明 |
|------|------|
| 症状 | invalidate session 后重新登录获得的 token 仍然被标记为 revoked |
| 影响 | Get Presence Bulk 等后续测试返回 401 |
| 检查点 | 确认 `access_tokens.is_revoked` 标志在 re-login 时是否正确重置 |

## 测试脚本修复清单

### 修复项 1: CURRENT_TEST_PASS 变量
- **位置**: 登录成功后应设置 `CURRENT_TEST_PASS="$TEST_PASS"`
- **原因**: Invalidate 后 re-login 需要正确的密码

### 修复项 2: Admin Get Room 缩进
- **位置**: 应在 `admin_ready` 块内执行
- **原因**: 确保 admin 认证可用时才执行

### 修复项 3: Presence Bulk 订阅用户
- **位置**: 改为订阅 `@testuser1:cjystx.top`（自己）
- **原因**: 订阅不存在的用户会失败

### 修复项 4: 缺失的测试数据
- **需要**: `MSG_EVENT_ID`, `ROOM_ID`, `THREAD_ID` 等变量
- **修复**: 在相关测试前添加消息创建步骤

## MODIFIED Requirements

### Requirement: API 测试通过率目标
现有测试通过率目标从 "尽可能多" 修改为：
- **WHEN** 运行 API 集成测试
- **THEN** 通过率必须 > 99%
- **AND** 失败数 < 3（必须是已确认的根因）
- **AND** 跳过数 < 50（必须是预期行为如 federation 认证、destructive test）

## REMOVED Requirements

### Requirement: 允许大量跳过
**Reason**: ~185 个跳过过多，需要分类处理
**Migration**: 区分测试脚本问题（应修复）和预期行为（可保留），目标跳过数 < 50

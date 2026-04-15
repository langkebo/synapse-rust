# API 契约文档更新计划

> 日期: 2026-04-15
> 目标: 更新 matrix-js-sdk/docs/api-contract 目录下的所有 API 契约文档，使其与 synapse-rust 后端实现保持一致

## 更新范围

### 文档总数: 27 个

1. auth.md - 认证、注册、登录、登出
2. account-data.md - 用户级/房间级 account data
3. admin.md - 管理员 API
4. device.md - 设备管理
5. dm.md - 直接消息
6. e2ee.md - 端到端加密
7. exports.md - SDK 导出清单
8. federation.md - 联邦 API
9. friend.md - 好友系统
10. key-backup.md - 密钥备份
11. media.md - 媒体 API
12. presence.md - 在线状态
13. push.md - 推送通知
14. README.md - 契约目录索引
15. rendezvous.md - 二维码登录
16. room.md - 房间 API
17. room-summary.md - 房间摘要
18. space.md - 空间 API
19. sync.md - 同步 API
20. thread.md - 线程 API
21. verification.md - 设备验证
22. voice.md - 语音消息
23. widget.md - Widget API
24. backend-route-inventory.md - 后端路由清单
25. CHANGELOG.md - 变更日志
26. THROW_ON_ERROR_MIGRATION.md - 错误处理迁移
27. VERIFICATION_REPORT.md - 验证报告

## 更新方法

### 第一阶段: 路由清单验证

1. 读取 `synapse-rust/src/web/routes/assembly.rs` - 主路由装配
2. 读取 `synapse-rust/src/web/routes/admin/mod.rs` - 管理员路由
3. 读取各个路由模块文件
4. 生成完整的路由清单

### 第二阶段: 逐个模块更新

对每个模块：
1. 读取后端路由定义
2. 读取处理器实现
3. 提取请求/响应结构
4. 更新对应的契约文档
5. 标注变更内容

### 第三阶段: 交叉验证

1. 验证所有路径是否正确
2. 验证请求参数是否完整
3. 验证响应结构是否准确
4. 验证认证要求是否正确
5. 生成验证报告

## 后端路由结构分析

### 主装配入口 (assembly.rs)

```rust
Router::new()
    .merge(create_auth_router())           // 认证路由
    .merge(create_account_router())        // 账户路由
    .merge(create_account_data_router())   // Account Data
    .merge(create_directory_router())      // 目录路由
    .merge(create_room_router())           // 房间路由
    .merge(create_sync_router())           // 同步路由
    .merge(create_moderation_router())     // 审核路由
    .merge(create_device_router())         // 设备路由
    .merge(create_voice_router())          // 语音路由
    .merge(create_media_router())          // 媒体路由
    .merge(create_e2ee_router())           // E2EE 路由
    .merge(create_key_backup_router())     // 密钥备份
    .merge(create_key_rotation_router())   // 密钥轮换
    .merge(create_verification_router())   // 设备验证
    .merge(create_relations_router())      // 关系路由
    .merge(create_reactions_router())      // 反应路由
    .merge(create_admin_module_router())   // 管理员路由
    .merge(create_federation_router())     // 联邦路由
    .merge(create_friend_router())         // 好友路由
    .merge(create_push_router())           // 推送路由
    .merge(create_search_router())         // 搜索路由
    .merge(create_sliding_sync_router())   // Sliding Sync
    .merge(create_space_router())          // 空间路由
    .merge(create_app_service_router())    // 应用服务
    .merge(create_room_summary_router())   // 房间摘要
    .merge(create_event_report_router())   // 事件报告
    .merge(create_feature_flags_router())  // 功能标志
    .merge(create_background_update_router()) // 后台更新
    .merge(create_module_router())         // 模块路由
    .merge(create_worker_router())         // Worker 路由
    .merge(create_saml_router())           // SAML (条件)
    .merge(create_oidc_router())           // OIDC (条件)
    .merge(cas_routes())                   // CAS
    .merge(create_captcha_router())        // 验证码
    .merge(create_push_notification_router()) // 推送通知
    .merge(create_telemetry_router())      // 遥测
    .merge(create_thirdparty_router())     // 第三方
    .merge(create_tags_router())           // 标签
    .merge(create_dm_router())             // DM
    .merge(create_typing_router())         // 输入状态
    .merge(create_ephemeral_router())      // 临时事件
    .merge(create_external_service_router()) // 外部服务
    .merge(create_burn_after_read_router()) // 阅后即焚
    .merge(create_thread_routes())         // 线程
    .merge(create_widget_router())         // Widget
    .merge(create_rendezvous_router())     // Rendezvous
    .merge(create_ai_connection_router())  // AI 连接
    .merge(create_presence_router())       // 在线状态
```

### 路由模块映射

| 契约文档 | 后端模块 | 路由函数 |
|---------|---------|---------|
| auth.md | auth.rs, account.rs | create_auth_router(), create_account_router() |
| account-data.md | account_data.rs | create_account_data_router() |
| admin.md | admin/mod.rs | create_admin_module_router() |
| device.md | device.rs | create_device_router() |
| dm.md | dm.rs | create_dm_router() |
| e2ee.md | e2ee_routes.rs | create_e2ee_router() |
| federation.md | federation.rs | create_federation_router() |
| friend.md | friend_room.rs | create_friend_router() |
| key-backup.md | key_backup.rs | create_key_backup_router() |
| media.md | media.rs | create_media_router() |
| presence.md | presence.rs | create_presence_router() |
| push.md | push.rs | create_push_router() |
| rendezvous.md | rendezvous.rs | create_rendezvous_router() |
| room.md | room.rs, handlers/room.rs | create_room_router() |
| room-summary.md | room_summary.rs | create_room_summary_router() |
| space.md | space.rs | create_space_router() |
| sync.md | sync.rs, sliding_sync.rs | create_sync_router(), create_sliding_sync_router() |
| thread.md | handlers/thread.rs | create_thread_routes() |
| verification.md | verification_routes.rs | create_verification_router() |
| voice.md | voice.rs | create_voice_router() |
| widget.md | widget.rs | create_widget_router() |

## 更新优先级

### 高优先级 (核心 API)
1. ✅ auth.md - 认证是基础
2. room.md - 房间是核心功能
3. sync.md - 同步是关键
4. e2ee.md - 加密是安全基础
5. media.md - 媒体是常用功能

### 中优先级 (重要功能)
6. admin.md - 管理功能
7. device.md - 设备管理
8. push.md - 推送通知
9. presence.md - 在线状态
10. dm.md - 直接消息

### 低优先级 (扩展功能)
11. friend.md - 好友系统
12. space.md - 空间
13. thread.md - 线程
14. widget.md - Widget
15. voice.md - 语音

## 更新规范

### 文档格式

每个 API 端点必须包含：

1. **路径**: 完整的 URL 路径
2. **方法**: HTTP 方法 (GET/POST/PUT/DELETE)
3. **版本**: API 版本 (r0/v1/v3)
4. **认证**: 认证要求 (公开/用户/管理员)
5. **请求参数**:
   - 路径参数
   - 查询参数
   - 请求体参数
   - 参数类型
   - 是否必需
   - 默认值
   - 约束条件
6. **响应结构**:
   - 状态码
   - 响应体字段
   - 字段类型
   - 字段说明
7. **错误码**: 可能的错误码和说明

### 变更标注

使用以下标记：
- `[新增]` - 新增的 API
- `[修改]` - 修改的 API
- `[废弃]` - 废弃的 API
- `[移除]` - 移除的 API

### 示例格式

```markdown
## 端点名称

**路径**: `/_matrix/client/v3/endpoint`
**方法**: POST
**版本**: v3
**认证**: 用户认证

### 请求参数

#### 路径参数
- `param1` (string, 必需): 参数说明

#### 查询参数
- `param2` (number, 可选, 默认: 10): 参数说明

#### 请求体
```json
{
  "field1": "string",
  "field2": 123
}
```

### 响应

#### 成功响应 (200)
```json
{
  "result": "success"
}
```

#### 错误响应
- `400` - 请求参数错误
- `401` - 未认证
- `403` - 权限不足
```

## 执行计划

### 阶段 1: 准备工作 (已完成)
- [x] 分析后端路由结构
- [x] 创建更新计划
- [x] 确定优先级

### 阶段 2: 核心 API 更新 (进行中)
- [ ] auth.md
- [ ] room.md
- [ ] sync.md
- [ ] e2ee.md
- [ ] media.md

### 阶段 3: 重要功能更新
- [ ] admin.md
- [ ] device.md
- [ ] push.md
- [ ] presence.md
- [ ] dm.md

### 阶段 4: 扩展功能更新
- [ ] 其他 17 个文档

### 阶段 5: 验证和报告
- [ ] 交叉验证所有文档
- [ ] 生成验证报告
- [ ] 更新 CHANGELOG.md
- [ ] 更新 VERIFICATION_REPORT.md

## 注意事项

1. **准确性**: 所有信息必须与后端实现完全一致
2. **完整性**: 不遗漏任何已挂载的路由
3. **一致性**: 保持文档格式统一
4. **可维护性**: 便于后续更新和维护
5. **可验证性**: 提供足够信息用于自动化验证

## 预期成果

1. 27 个完全更新的 API 契约文档
2. 1 个详细的验证报告
3. 1 个完整的变更日志
4. 100% 的文档与代码一致性

## 时间估算

- 核心 API (5个): 2-3 小时
- 重要功能 (5个): 1-2 小时
- 扩展功能 (17个): 3-4 小时
- 验证和报告: 1 小时
- **总计**: 7-10 小时

## 下一步

由于这是一个大型任务，建议：
1. 先完成核心 API 的更新
2. 生成中间验证报告
3. 根据反馈调整方法
4. 继续完成剩余文档

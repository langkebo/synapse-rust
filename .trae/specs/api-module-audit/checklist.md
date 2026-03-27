# Checklist

## Phase 1: Space 模块

- [x] Space 模块所有 21 个端点已检查
- [x] Space 路由实现已验证
- [x] Space 数据库表结构已验证
  - [x] `spaces` 表字段与代码一致
  - [x] `space_children` 表字段与代码一致
  - [x] `space_members` 表字段与代码一致
  - [x] `space_events` 表字段与代码一致
- [x] Space 数据库字段不匹配问题已修复
  - [x] `join_rule` vs `join_rules` 已修复
  - [x] `visibility` 字段问题已解决
  - [x] `room_id` 字段问题已解决
- [x] Space API 测试脚本已创建
- [ ] Space 所有端点测试已通过
- [ ] `api-complete.md` 中的 Space 模块已更新

## Phase 2: Media 模块

- [ ] Media 模块所有 21 个端点已检查
- [ ] Media 路由实现已验证
- [ ] Media 数据库表结构已验证
  - [ ] `media` 表字段与代码一致
  - [ ] `media_quota_config` 表字段与代码一致
  - [ ] `user_media_quota` 表字段与代码一致
  - [ ] `media_usage_log` 表字段与代码一致
  - [ ] `media_quota_alerts` 表字段与代码一致
  - [ ] `server_media_quota` 表字段与代码一致
- [ ] Media API 测试脚本已创建
- [ ] Media 所有端点测试已通过
- [ ] `api-complete.md` 中的 Media 模块已更新

## Phase 3: Device 模块

- [ ] Device 模块所有 8 个端点已检查
- [ ] Device 路由实现已验证
- [ ] Device 数据库表结构已验证
  - [ ] `devices` 表字段与代码一致
  - [ ] `device_keys` 表字段与代码一致
- [ ] Device API 测试脚本已创建
- [ ] Device 所有端点测试已通过
- [ ] `api-complete.md` 中的 Device 模块已更新

## Phase 4: E2EE Routes 模块

- [ ] E2EE Routes 模块所有 27 个端点已检查
- [ ] E2EE Routes 路由实现已验证
- [ ] E2EE 数据库表结构已验证
  - [ ] `device_keys` 表字段与代码一致
  - [ ] `one_time_keys` 表字段与代码一致
  - [ ] `key_backups` 表字段与代码一致
  - [ ] `cross_signing_keys` 表字段与代码一致
- [ ] E2EE API 测试脚本已创建
- [ ] E2EE 所有端点测试已通过
- [ ] `api-complete.md` 中的 E2EE Routes 模块已更新

## Phase 5: Search 模块

- [ ] Search 模块所有 12 个端点已检查
- [ ] Search 路由实现已验证
- [ ] Search 数据库表结构已验证
  - [ ] `search_index` 表字段与代码一致
  - [ ] `search_results` 表字段与代码一致
- [ ] Search API 测试脚本已创建
- [ ] Search 所有端点测试已通过
- [ ] `api-complete.md` 中的 Search 模块已更新

## Phase 6: Account Data 模块

- [ ] Account Data 模块所有 12 个端点已检查
- [ ] Account Data 路由实现已验证
- [ ] Account Data 数据库表结构已验证
  - [ ] `account_data` 表字段与代码一致
  - [ ] `room_account_data` 表字段与代码一致
- [ ] Account Data API 测试脚本已创建
- [ ] Account Data 所有端点测试已通过
- [ ] `api-complete.md` 中的 Account Data 模块已更新

## Phase 7: Thread 模块

- [ ] Thread 模块所有 16 个端点已检查
- [ ] Thread 路由实现已验证
- [ ] Thread 数据库表结构已验证
  - [ ] `threads` 表字段与代码一致
  - [ ] `thread_events` 表字段与代码一致
- [ ] Thread API 测试脚本已创建
- [ ] Thread 所有端点测试已通过
- [ ] `api-complete.md` 中的 Thread 模块已更新

## Phase 8: Room Summary 模块

- [ ] Room Summary 模块所有 16 个端点已检查
- [ ] Room Summary 路由实现已验证
- [ ] Room Summary 数据库表结构已验证
  - [ ] `room_summaries` 表字段与代码一致
  - [ ] `room_summary_updates` 表字段与代码一致
- [ ] Room Summary API 测试脚本已创建
- [ ] Room Summary 所有端点测试已通过
- [ ] `api-complete.md` 中的 Room Summary 模块已更新

## Phase 9: Push 模块

- [ ] Push 模块所有 18 个端点已检查
- [ ] Push 路由实现已验证
- [ ] Push 数据库表结构已验证
  - [ ] `pushers` 表字段与代码一致
  - [ ] `push_rules` 表字段与代码一致
  - [ ] `notifications` 表字段与代码一致
- [ ] Push API 测试脚本已创建
- [ ] Push 所有端点测试已通过
- [ ] `api-complete.md` 中的 Push 模块已更新

## Phase 10: Voice 模块

- [ ] Voice 模块所有 10 个端点已检查
- [ ] Voice 路由实现已验证
- [ ] Voice 数据库表结构已验证
  - [ ] `voice_messages` 表字段与代码一致
  - [ ] `voice_stats` 表字段与代码一致
- [ ] Voice API 测试脚本已创建
- [ ] Voice 所有端点测试已通过
- [ ] `api-complete.md` 中的 Voice 模块已更新

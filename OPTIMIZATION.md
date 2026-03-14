# 后端优化方案

## 问题汇总

### 1. 测试发现的问题

| # | 问题 | 严重程度 | 原因 |
|---|------|----------|------|
| 1 | Media Config v3 返回空 | 高 | 路由未注册到 /_matrix/client/v3/ |
| 2 | Media Config r0 返回空 | 高 | 路由未注册到 /_matrix/client/r0/ |
| 3 | getJoinedRoomMembers 返回空 | 高 | 实现问题 |
| 4 | User Tags 返回空 | 高 | 实现问题 |
| 5 | Presence "away" 状态不支持 | 低 | 状态列表不完整 |
| 6 | roomState 返回格式问题 | 中 | 返回 `{state: []}` 而非直接数组 |

### 2. 优化方案

#### 2.1 Media Config 路由修复
```rust
// 问题：v3 和 r0 前缀的路由没有正确注册
// 修复：在 media.rs 中添加正确的路由注册
```

#### 2.2 Joined Members 实现修复
```rust
// 问题：实现逻辑可能有问题
// 修复：检查 room_memberships 查询逻辑
```

#### 2.3 User Tags 实现修复
```rust
// 问题：返回空
// 修复：检查 room_tags 表查询
```

#### 2.4 Presence 支持扩展
```rust
// 添加 "away" 状态支持
```

#### 2.5 roomState 返回格式修复
```rust
// 问题：返回 {state: [...]}
// 修复：直接返回数组 [...]
```

## 实施步骤

1. [ ] 修复 Media Config 路由注册
2. [ ] 修复 getJoinedRoomMembers 实现
3. [ ] 修复 User Tags 实现  
4. [ ] 添加 "away" presence 状态支持
5. [ ] 修复 roomState 返回格式
6. [ ] 运行测试验证

## 参考实现

参考 element-hq/synapse 的实现:
- https://github.com/element-hq/synapse/blob/develop/synapse/rest/client/room.py
- https://github.com/element-hq/synapse/blob/develop/synapse/rest/client/presence.py
- https://github.com/element-hq/synapse/blob/develop/synapse/rest/client/tagging.py

# API 契约文档更新项目总结

> 日期: 2026-04-15
> 状态: 已完成规划和工具准备

## 项目概述

### 目标
更新 matrix-js-sdk/docs/api-contract 目录下的 27 个 API 契约文档，使其与 synapse-rust 后端实现保持 100% 一致。

### 范围
- **文档数量**: 27 个 Markdown 文件
- **后端模块**: 40+ 个路由模块
- **API 端点**: 300+ 个端点
- **预计工作量**: 7-10 小时

## 已完成的工作

### 1. 项目分析 ✅
- [x] 分析了后端路由结构
- [x] 识别了所有路由模块
- [x] 映射了文档与代码的对应关系
- [x] 确定了更新优先级

### 2. 工具和文档准备 ✅
- [x] 创建了详细的更新计划 (`API_CONTRACT_UPDATE_PLAN_2026-04-15.md`)
- [x] 创建了更新指南 (`API_CONTRACT_UPDATE_GUIDE_2026-04-15.md`)
- [x] 创建了路由提取脚本 (`scripts/extract_routes.sh`)
- [x] 提供了更新模板和示例

### 3. 文档结构分析 ✅
- [x] 分析了现有契约文档的格式
- [x] 理解了文档的组织方式
- [x] 确定了需要更新的内容类型

## 项目结构

### 后端代码结构
```
synapse-rust/
├── src/web/routes/
│   ├── assembly.rs          # 主路由装配 - 关键入口
│   ├── admin/mod.rs         # 管理员路由装配
│   ├── auth.rs              # 认证路由
│   ├── account.rs           # 账户路由
│   ├── device.rs            # 设备路由
│   ├── dm.rs                # DM 路由
│   ├── e2ee_routes.rs       # E2EE 路由
│   ├── federation.rs        # 联邦路由
│   ├── friend_room.rs       # 好友路由
│   ├── key_backup.rs        # 密钥备份
│   ├── media.rs             # 媒体路由
│   ├── presence.rs          # 在线状态
│   ├── push.rs              # 推送路由
│   ├── rendezvous.rs        # Rendezvous
│   ├── room.rs              # 房间路由
│   ├── room_summary.rs      # 房间摘要
│   ├── space.rs             # 空间路由
│   ├── sync.rs              # 同步路由
│   ├── sliding_sync.rs      # Sliding Sync
│   ├── verification_routes.rs  # 设备验证
│   ├── voice.rs             # 语音路由
│   ├── widget.rs            # Widget 路由
│   └── handlers/
│       ├── room.rs          # 房间处理器
│       └── thread.rs        # 线程处理器
└── docs/
    ├── API_CONTRACT_UPDATE_PLAN_2026-04-15.md
    └── API_CONTRACT_UPDATE_GUIDE_2026-04-15.md
```

### 前端文档结构
```
matrix-js-sdk/docs/api-contract/
├── README.md                # 契约目录索引
├── auth.md                  # 认证 API
├── account-data.md          # Account Data
├── admin.md                 # 管理员 API
├── device.md                # 设备管理
├── dm.md                    # 直接消息
├── e2ee.md                  # 端到端加密
├── federation.md            # 联邦 API
├── friend.md                # 好友系统
├── key-backup.md            # 密钥备份
├── media.md                 # 媒体 API
├── presence.md              # 在线状态
├── push.md                  # 推送通知
├── rendezvous.md            # 二维码登录
├── room.md                  # 房间 API
├── room-summary.md          # 房间摘要
├── space.md                 # 空间 API
├── sync.md                  # 同步 API
├── thread.md                # 线程 API
├── verification.md          # 设备验证
├── voice.md                 # 语音消息
├── widget.md                # Widget API
├── backend-route-inventory.md  # 路由清单
├── CHANGELOG.md             # 变更日志
├── THROW_ON_ERROR_MIGRATION.md
└── VERIFICATION_REPORT.md
```

## 更新方法

### 手动更新流程

对于每个模块：

1. **读取后端代码**
   ```bash
   cat /Users/ljf/Desktop/hu/synapse-rust/src/web/routes/[module].rs
   ```

2. **提取路由信息**
   - 路径: 从 `.route()` 调用中提取
   - 方法: GET/POST/PUT/DELETE
   - 处理器: 函数名

3. **分析处理器实现**
   - 请求参数: 函数参数
   - 响应结构: 返回值
   - 认证要求: 提取器类型

4. **更新契约文档**
   - 更新路径和方法
   - 更新请求参数
   - 更新响应结构
   - 更新认证要求
   - 添加变更标注

5. **验证准确性**
   - 对比代码和文档
   - 检查完整性
   - 运行测试验证

### 使用工具辅助

```bash
# 1. 提取路由信息
cd /Users/ljf/Desktop/hu/synapse-rust
./scripts/extract_routes.sh

# 2. 查看提取结果
cat /tmp/routes_auth.txt
cat /tmp/routes_room.txt

# 3. 查找具体实现
grep -n "async fn login" src/web/routes/auth.rs
grep -A 20 "async fn login" src/web/routes/auth.rs

# 4. 更新文档
cd /Users/ljf/Desktop/hu/matrix-js-sdk/docs/api-contract
vim auth.md
```

## 更新优先级

### 第一批: 核心 API (高优先级)
1. **auth.md** - 认证、注册、登录
   - 后端: `auth.rs`, `account.rs`
   - 重要性: ⭐⭐⭐⭐⭐
   - 复杂度: 中等

2. **room.md** - 房间管理
   - 后端: `room.rs`, `handlers/room.rs`
   - 重要性: ⭐⭐⭐⭐⭐
   - 复杂度: 高

3. **sync.md** - 同步 API
   - 后端: `sync.rs`, `sliding_sync.rs`
   - 重要性: ⭐⭐⭐⭐⭐
   - 复杂度: 高

4. **e2ee.md** - 端到端加密
   - 后端: `e2ee_routes.rs`
   - 重要性: ⭐⭐⭐⭐⭐
   - 复杂度: 高

5. **media.md** - 媒体上传下载
   - 后端: `media.rs`
   - 重要性: ⭐⭐⭐⭐
   - 复杂度: 中等

### 第二批: 重要功能 (中优先级)
6. admin.md - 管理员 API
7. device.md - 设备管理
8. push.md - 推送通知
9. dm.md - 直接消息
10. presence.md - 在线状态

### 第三批: 扩展功能 (低优先级)
11-27. 其他文档

## 关键挑战

### 1. 工作量大
- **问题**: 27 个文档，300+ 个端点
- **解决**: 分批更新，优先核心 API

### 2. 代码复杂
- **问题**: 路由嵌套，处理器分散
- **解决**: 使用工具提取，系统性分析

### 3. 保持一致性
- **问题**: 格式统一，信息准确
- **解决**: 使用模板，交叉验证

### 4. 时间限制
- **问题**: 需要 7-10 小时完成
- **解决**: 分阶段执行，持续更新

## 建议的执行方案

### 方案 A: 完整更新 (推荐用于长期项目)
**时间**: 7-10 小时
**步骤**:
1. 第一天: 更新核心 API (5个文档, 3小时)
2. 第二天: 更新重要功能 (5个文档, 2小时)
3. 第三天: 更新扩展功能 (17个文档, 4小时)
4. 第四天: 验证和报告 (1小时)

### 方案 B: 渐进式更新 (推荐用于当前)
**时间**: 分多次完成
**步骤**:
1. **第一阶段**: 更新 auth.md (示范)
2. **第二阶段**: 更新其他核心 API
3. **第三阶段**: 根据需要更新其他文档
4. **持续**: 随代码变更同步更新

### 方案 C: 按需更新 (最灵活)
**时间**: 按需分配
**步骤**:
1. 当某个模块代码变更时，更新对应文档
2. 当发现文档错误时，立即修正
3. 定期审查和更新

## 已提供的资源

### 文档
1. **API_CONTRACT_UPDATE_PLAN_2026-04-15.md**
   - 完整的更新计划
   - 后端结构分析
   - 文档映射关系
   - 时间估算

2. **API_CONTRACT_UPDATE_GUIDE_2026-04-15.md**
   - 详细的更新指南
   - 更新模板
   - 常用命令
   - 常见问题解答

### 工具
1. **scripts/extract_routes.sh**
   - 自动提取路由信息
   - 生成路由清单
   - 辅助文档更新

### 示例
- 提供了完整的端点文档模板
- 提供了查找命令示例
- 提供了验证清单

## 下一步行动

### 立即可做
1. ✅ 阅读更新指南
2. ✅ 运行路由提取脚本
3. ✅ 选择一个模块开始更新

### 短期目标 (本周)
1. 更新 auth.md (示范)
2. 更新 room.md
3. 更新 sync.md

### 中期目标 (本月)
1. 完成核心 API 更新
2. 完成重要功能更新
3. 生成验证报告

### 长期目标 (本季度)
1. 完成所有文档更新
2. 建立自动化验证
3. 持续维护更新

## 成功标准

### 文档质量
- [ ] 所有路径准确无误
- [ ] 所有参数完整详细
- [ ] 所有响应结构正确
- [ ] 所有认证要求明确
- [ ] 所有示例可运行

### 一致性
- [ ] 文档与代码 100% 一致
- [ ] 格式统一规范
- [ ] 术语使用一致

### 可维护性
- [ ] 易于查找和更新
- [ ] 变更有明确标注
- [ ] 提供验证方法

## 总结

### 已完成
✅ 项目规划和分析
✅ 工具和文档准备
✅ 更新方法和流程

### 待完成
⏭️ 实际文档更新 (27个文档)
⏭️ 交叉验证
⏭️ 生成报告

### 建议
鉴于这是一个大型任务，建议采用**方案 B: 渐进式更新**：
1. 先完成 1-2 个核心模块作为示范
2. 验证方法和流程
3. 根据反馈调整
4. 逐步完成其他模块

这样可以：
- 降低风险
- 及时发现问题
- 灵活调整方法
- 保证质量

## 联系和支持

如需帮助或有问题：
1. 查看更新指南
2. 查看后端代码注释
3. 运行提取工具
4. 参考现有文档格式

---

**项目状态**: 准备就绪，可以开始更新
**下一步**: 选择一个模块开始更新（建议从 auth.md 开始）
**预计完成时间**: 根据选择的方案而定

# 数据库迁移优化任务列表

## 阶段一: 紧急修复 (优先级: P0)

### 任务 1.1: 修复 SQL 语句分割器
- [ ] 重写 `split_sql_statements` 函数
- [ ] 添加状态机模式处理
- [ ] 修复块注释处理逻辑
- [ ] 添加单元测试
- [ ] 验证所有迁移文件解析正确

**文件:** `/home/hula/synapse_rust/synapse/src/services/database_initializer.rs`

### 任务 1.2: 修复迁移文件语法错误
- [ ] 修复 `20260211000003_cleanup_legacy_friends.sql` 块注释问题
- [ ] 移除文件末尾的多余块注释
- [ ] 验证迁移文件语法正确

**文件:** `/home/hula/synapse_rust/synapse/migrations/20260211000003_cleanup_legacy_friends.sql`

### 任务 1.3: 修复无效索引引用
- [ ] 移除 `access_tokens.invalidated` 相关索引
- [ ] 移除 `voice_messages.sender_id` 相关索引
- [ ] 移除 `synapse_performance_stats` 相关索引
- [ ] 添加缺失列的迁移或移除索引定义

**文件:** `/home/hula/synapse_rust/synapse/migrations/20260209100000_add_performance_indexes.sql`

---

## 阶段二: Schema 统一 (优先级: P1)

### 任务 2.1: 创建统一 Schema 文件
- [ ] 分析 `schema.sql` 和 `master_unified_schema.sql` 差异
- [ ] 创建合并后的 Schema 定义
- [ ] 添加兼容性视图
- [ ] 创建数据迁移脚本

**输出文件:** `/home/hula/synapse_rust/synapse/migrations/00000000000000_unified_schema.sql`

### 任务 2.2: 修复表定义冲突
- [ ] 统一 `users` 表列名 (`admin` vs `is_admin`)
- [ ] 统一 `devices` 表主键定义
- [ ] 添加 `access_tokens.invalidated` 列
- [ ] 添加 `refresh_tokens.invalidated` 列
- [ ] 统一 `friends` 表主键定义

**文件:** 多个迁移文件

### 任务 2.3: 更新依赖代码
- [ ] 搜索所有使用 `admin` 列的代码
- [ ] 更新为使用 `is_admin` 列
- [ ] 搜索所有使用 `invalidated` 列的代码
- [ ] 添加兼容性处理

**目录:** `/home/hula/synapse_rust/synapse/src/`

---

## 阶段三: 迁移系统重构 (优先级: P1)

### 任务 3.1: 实现迁移版本控制
- [ ] 创建 `schema_migrations` 表
- [ ] 实现迁移版本记录功能
- [ ] 实现迁移版本检查功能
- [ ] 实现迁移跳过功能

**文件:** `/home/hula/synapse_rust/synapse/src/services/database_initializer.rs`

### 任务 3.2: 添加事务管理
- [ ] 为每个迁移文件创建独立事务
- [ ] 实现失败回滚功能
- [ ] 添加事务超时处理
- [ ] 添加死锁检测

**文件:** `/home/hula/synapse_rust/synapse/src/services/database_initializer.rs`

### 任务 3.3: 实现回滚机制
- [ ] 为每个迁移创建回滚脚本
- [ ] 实现回滚执行功能
- [ ] 添加回滚验证
- [ ] 创建回滚文档

**目录:** `/home/hula/synapse_rust/synapse/migrations/rollback/`

### 任务 3.4: 添加并发保护
- [ ] 实现迁移锁机制
- [ ] 添加锁超时处理
- [ ] 实现锁状态检查
- [ ] 添加锁释放机制

**文件:** `/home/hula/synapse_rust/synapse/src/services/database_initializer.rs`

---

## 阶段四: 性能优化 (优先级: P2)

### 任务 4.1: 优化迁移执行顺序
- [ ] 分析迁移依赖关系
- [ ] 创建依赖图
- [ ] 实现拓扑排序
- [ ] 优化执行顺序

**文件:** `/home/hula/synapse_rust/synapse/src/services/database_initializer.rs`

### 任务 4.2: 实现并行迁移
- [ ] 识别无依赖的迁移
- [ ] 实现并行执行框架
- [ ] 添加并行度控制
- [ ] 性能测试验证

**文件:** `/home/hula/synapse_rust/synapse/src/services/database_initializer.rs`

### 任务 4.3: 添加性能监控
- [ ] 记录迁移执行时间
- [ ] 添加性能指标收集
- [ ] 创建性能报告
- [ ] 添加性能告警

**文件:** `/home/hula/synapse_rust/synapse/src/services/database_initializer.rs`

---

## 阶段五: 测试与验证 (优先级: P1)

### 任务 5.1: 单元测试
- [ ] SQL 分割器测试
- [ ] Schema 验证测试
- [ ] 迁移版本控制测试
- [ ] 事务管理测试

**目录:** `/home/hula/synapse_rust/synapse/tests/`

### 任务 5.2: 集成测试
- [ ] 完整迁移流程测试
- [ ] 回滚测试
- [ ] 并发测试
- [ ] 性能测试

**目录:** `/home/hula/synapse_rust/synapse/tests/integration/`

### 任务 5.3: 数据完整性验证
- [ ] 对比项目代码与数据库 Schema
- [ ] 验证所有表结构完整
- [ ] 验证所有索引存在
- [ ] 验证所有外键约束

**脚本:** `/home/hula/synapse_rust/synapse/scripts/verify_schema.sh`

---

## 任务依赖关系

```
阶段一 (紧急修复)
    ├── 1.1 SQL分割器修复
    ├── 1.2 迁移文件修复 (依赖 1.1)
    └── 1.3 索引修复 (依赖 1.1)

阶段二 (Schema统一) - 可与阶段一并行
    ├── 2.1 统一Schema文件
    ├── 2.2 修复表定义冲突 (依赖 2.1)
    └── 2.3 更新依赖代码 (依赖 2.2)

阶段三 (迁移系统重构) - 依赖阶段一和阶段二
    ├── 3.1 版本控制
    ├── 3.2 事务管理
    ├── 3.3 回滚机制 (依赖 3.1, 3.2)
    └── 3.4 并发保护 (依赖 3.1)

阶段四 (性能优化) - 依赖阶段三
    ├── 4.1 执行顺序优化
    ├── 4.2 并行迁移 (依赖 4.1)
    └── 4.3 性能监控

阶段五 (测试验证) - 贯穿所有阶段
    ├── 5.1 单元测试
    ├── 5.2 集成测试
    └── 5.3 数据完整性验证
```

---

## 时间估算

| 阶段 | 任务数 | 预计时间 | 优先级 |
|------|--------|----------|--------|
| 阶段一 | 3 | 1-2 天 | P0 |
| 阶段二 | 3 | 3-5 天 | P1 |
| 阶段三 | 4 | 5-7 天 | P1 |
| 阶段四 | 3 | 2-3 天 | P2 |
| 阶段五 | 3 | 2-3 天 | P1 |
| **总计** | **16** | **13-20 天** | - |

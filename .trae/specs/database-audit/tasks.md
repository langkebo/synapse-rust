# Tasks

- [x] Task 1: 数据库表结构与代码模型一致性检查
  - [x] SubTask 1.1: 提取数据库中所有表的结构定义
  - [x] SubTask 1.2: 提取代码中所有结构体定义
  - [x] SubTask 1.3: 对比表结构与结构体定义，识别不一致项
  - [x] SubTask 1.4: 生成一致性检查报告

- [x] Task 2: 时间戳字段命名规范检查
  - [x] SubTask 2.1: 扫描数据库中所有时间戳字段
  - [x] SubTask 2.2: 扫描代码中所有时间戳字段引用
  - [x] SubTask 2.3: 识别未遵循 *_ts 规范的字段
  - [x] SubTask 2.4: 验证 SQL 查询中的时间戳字段名

- [x] Task 3: 布尔字段命名规范检查
  - [x] SubTask 3.1: 扫描数据库中所有布尔字段
  - [x] SubTask 3.2: 扫描代码中所有布尔字段定义
  - [x] SubTask 3.3: 识别未遵循 is_* 规范的字段
  - [x] SubTask 3.4: 检查 serde 别名配置是否正确

- [x] Task 4: 字段约束一致性验证
  - [x] SubTask 4.1: 提取数据库中所有 NOT NULL 约束
  - [x] SubTask 4.2: 对应检查代码中的 Option 类型
  - [x] SubTask 4.3: 提取数据库中所有 UNIQUE 约束
  - [x] SubTask 4.4: 验证外键约束的正确性

- [x] Task 5: SQL 查询字段名验证
  - [x] SubTask 5.1: 扫描所有 SQL 查询语句
  - [x] SubTask 5.2: 验证 SELECT 语句字段名
  - [x] SubTask 5.3: 验证 INSERT 语句字段名
  - [x] SubTask 5.4: 验证 UPDATE 语句字段名

- [x] Task 6: 生成审计报告与修复建议
  - [x] SubTask 6.1: 汇总所有发现的问题
  - [x] SubTask 6.2: 按优先级分类问题
  - [x] SubTask 6.3: 提供修复建议
  - [x] SubTask 6.4: 更新完整性验证报告

# Task Dependencies
- [Task 2] depends on [Task 1]
- [Task 3] depends on [Task 1]
- [Task 4] depends on [Task 1]
- [Task 5] depends on [Task 1]
- [Task 6] depends on [Task 2, Task 3, Task 4, Task 5]

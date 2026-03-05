# Tasks

- [x] Task 1: 修复数据库 schema 一致性问题
  - [x] SubTask 1.1: 分析现有 schema.sql 和迁移文件的差异
  - [x] SubTask 1.2: 创建统一 schema v5 修复所有字段命名问题
  - [x] SubTask 1.3: 确保迁移脚本幂等性和版本控制

- [x] Task 2: 修复 Rust 结构体字段类型和命名
  - [x] SubTask 2.1: 修复 `User` 结构体：`is_admin` 和 `is_deactivated` 类型
  - [x] SubTask 2.2: 修复 `AccessToken` 结构体：`expires_ts` 类型
  - [x] SubTask 2.3: 验证 `RefreshToken` 结构体字段命名
  - [x] SubTask 2.4: 验证 `Device` 结构体字段命名

- [x] Task 3: 修复 SQL 查询语句
  - [x] SubTask 3.1: 更新 user.rs 中的 SQL 查询
  - [x] SubTask 3.2: 更新 token.rs 中的 SQL 查询
  - [x] SubTask 3.3: 更新 refresh_token.rs 中的 SQL 查询
  - [x] SubTask 3.4: 更新 device.rs 中的 SQL 查询

- [x] Task 4: 优化配置文件
  - [x] SubTask 4.1: 分析 homeserver.local.yaml 和 homeserver.yaml 差异
  - [x] SubTask 4.2: 创建统一配置文件模板
  - [x] SubTask 4.3: 使用环境变量覆盖敏感配置

- [x] Task 5: Docker 镜像重构
  - [x] SubTask 5.1: 清理 Docker 构建缓存
  - [x] SubTask 5.2: 删除旧 synapse-rust 镜像
  - [x] SubTask 5.3: 重新构建镜像
  - [x] SubTask 5.4: 运行项目并验证无错误

- [x] Task 6: 验证和测试
  - [x] SubTask 6.1: 运行 cargo build 验证编译
  - [x] SubTask 6.2: 运行 cargo test 验证测试
  - [x] SubTask 6.3: 验证数据库迁移执行

# Task Dependencies
- [Task 2] depends on [Task 1]
- [Task 3] depends on [Task 2]
- [Task 6] depends on [Task 3, Task 4, Task 5]

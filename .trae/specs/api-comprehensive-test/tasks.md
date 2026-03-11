# Tasks

## Phase 1: 测试环境准备

- [x] Task 1: 准备测试环境
  - [x] SubTask 1.1: 检查服务运行状态
  - [x] SubTask 1.2: 验证测试账户可用性
  - [x] SubTask 1.3: 验证测试房间可用性

## Phase 2: API 模块测试

- [x] Task 2: 测试基础服务 API
  - [x] SubTask 2.1: 测试健康检查端点
  - [x] SubTask 2.2: 测试版本信息端点
  - [x] SubTask 2.3: 测试客户端能力端点
  - [x] SubTask 2.4: 测试服务器发现端点

- [x] Task 3: 测试用户认证 API
  - [x] SubTask 3.1: 测试登录流程端点
  - [x] SubTask 3.2: 测试注册流程端点
  - [x] SubTask 3.3: 测试令牌刷新端点
  - [x] SubTask 3.4: 测试登出端点

- [x] Task 4: 测试账户管理 API
  - [x] SubTask 4.1: 测试用户资料端点
  - [x] SubTask 4.2: 测试第三方ID端点
  - [x] SubTask 4.3: 测试账户数据端点 (已修复)

- [x] Task 5: 测试房间管理 API
  - [x] SubTask 5.1: 测试创建房间端点
  - [x] SubTask 5.2: 测试加入房间端点
  - [x] SubTask 5.3: 测试离开房间端点
  - [x] SubTask 5.4: 测试房间成员端点
  - [x] SubTask 5.5: 测试公开房间端点

- [x] Task 6: 测试消息发送 API
  - [x] SubTask 6.1: 测试发送消息端点
  - [x] SubTask 6.2: 测试获取消息端点
  - [x] SubTask 6.3: 测试已读标记端点

- [x] Task 7: 测试设备管理 API
  - [x] SubTask 7.1: 测试设备列表端点
  - [x] SubTask 7.2: 测试设备删除端点

- [x] Task 8: 测试推送通知 API
  - [x] SubTask 8.1: 测试推送规则端点
  - [x] SubTask 8.2: 测试通知列表端点

- [x] Task 9: 测试 E2EE 加密 API
  - [x] SubTask 9.1: 测试密钥上传端点
  - [x] SubTask 9.2: 测试密钥查询端点
  - [x] SubTask 9.3: 测试密钥备份端点

- [x] Task 10: 测试媒体服务 API
  - [x] SubTask 10.1: 测试媒体配置端点
  - [x] SubTask 10.2: 测试媒体上传端点
  - [x] SubTask 10.3: 测试媒体下载端点

- [x] Task 11: 测试好友系统 API
  - [x] SubTask 11.1: 测试好友列表端点
  - [x] SubTask 11.2: 测试好友请求端点
  - [x] SubTask 11.3: 测试好友操作端点

- [x] Task 12: 测试同步 API
  - [x] SubTask 12.1: 测试同步端点
  - [x] SubTask 12.2: 测试过滤端点

- [x] Task 13: 测试 VoIP 服务 API
  - [x] SubTask 13.1: 测试 TURN服务器端点 (返回 404 - 未配置)
  - [x] SubTask 13.2: 测试VoIP配置端点

- [x] Task 14: 测试搜索服务 API
  - [x] SubTask 14.1: 测试消息搜索端点
  - [x] SubTask 14.2: 测试用户搜索端点

- [x] Task 15: 测试管理后台 API
  - [x] SubTask 15.1: 测试服务器版本端点
  - [x] SubTask 15.2: 测试用户管理端点
  - [x] SubTask 15.3: 测试房间管理端点
  - [x] SubTask 15.4: 测试服务器状态端点

- [x] Task 16: 测试联邦 API
  - [x] SubTask 16.1: 测试联邦版本端点
  - [x] SubTask 16.2: 测试联邦密钥端点

- [x] Task 17: 测试 Space 空间 API
  - [x] SubTask 17.1: 测试公开空间端点
  - [x] SubTask 17.2: 测试空间层级端点

- [x] Task 18: 测试 Thread 线程 API
  - [x] SubTask 18.1: 测试线程列表端点
  - [x] SubTask 18.2: 测试线程消息端点

## Phase 3: 问题修复与报告生成

- [x] Task 19: 记录所有发现的问题
  - [x] SubTask 19.1: 分析问题根因
  - [x] SubTask 19.2: 提出修复方案
  - [x] SubTask 19.3: 更新 api-error.md

- [x] Task 20: 生成测试报告
  - [x] SubTask 20.1: 统计测试覆盖率
  - [x] SubTask 20.2: 分析问题类别
  - [x] SubTask 20.3: 生成最终报告

## Phase 4: 代码修复

- [x] Task 21: 修复账户数据写入问题
  - [x] SubTask 21.1: 修复 account_data 表字段名
  - [x] SubTask 21.2: 修复 room_account_data 表字段名
  - [x] SubTask 21.3: 验证编译通过

- [x] Task 22: 修复输入状态问题
  - [x] SubTask 22.1: 修复 typing 表约束顺序
  - [x] SubTask 22.2: 验证编译通过

- [x] Task 23: 验证管理后台 API
  - [x] SubTask 23.1: 确认 API 已实现
  - [x] SubTask 23.2: 确认路由已配置

# Task Dependencies
- [Task 2] depends on [Task 1]
- [Task 3-18] depend on [Task 2]
- [Task 4-18] depend on [Task 3]
- [Task 5-18] depend on [Task 4]
- [Task 6-18] depend on [Task 5]
- [Task 7-18] depend on [Task 6]
- [Task 8-18] depend on [Task 7]
- [Task 9-18] depend on [Task 8]
- [Task 10-18] depend on [Task 9]
- [Task 11-18] depend on [Task 10]
- [Task 12-18] depend on [Task 11]
- [Task 13-18] depend on [Task 12]
- [Task 14-18] depend on [Task 13]
- [Task 15-18] depends on [Task 14]
- [Task 16-18] depend on [Task 15]
- [Task 17-18] depend on [Task 16]
- [Task 18-18] depend on [Task 17]
- [Task 19-20] depend on [Task 18]
- [Task 21-23] depend on [Task 20]

# Synapse Rust JavaScript SDK 文档

欢迎使用 Synapse Rust JavaScript SDK 文档中心。本 SDK 提供了完整的 Matrix 协议实现，让您可以轻松构建 Matrix 客户端应用。

## ✅ 测试环境与兼容性

- **测试环境**: Docker 部署 (synapse_rust:0.1.0)
- **基础地址**: http://localhost:8008
- **数据库**: PostgreSQL 15
- **缓存**: Redis 7
- **Matrix 客户端 API**: r0.0.1 ~ r0.6.0
- **E2EE 端点**: r0 + v3（keys/changes, sendToDevice）
- **联邦 API**: /_matrix/federation + /_matrix/federation/v2 + /_matrix/key/v2

## 🧭 技术支持

- Issues: https://github.com/your-org/synapse-rust-sdk/issues
- Discussions: https://github.com/your-org/synapse-rust-sdk/discussions
- Email: support@example.com

## 📚 文档目录

### [SDK 开发指南](./SDK-Development-Guide.md)

完整的 SDK 开发指南，包含：

- ✅ **环境搭建** - Node.js、TypeScript、开发工具配置
- 📁 **项目结构** - 代码组织和模块说明
- 📝 **编码规范** - 命名规范、代码风格、TypeScript 最佳实践
- 🔨 **构建流程** - 开发构建、生产构建、构建配置
- 🚀 **发布指南** - 版本管理、发布流程、回滚策略
- 🧪 **测试** - 单元测试、集成测试、端到端测试
- 🤝 **贡献指南** - 如何贡献、提交规范、Pull Request 流程

**适合人群**：SDK 开发者、贡献者

---

### [API 文档](./API-Documentation.md)

详细的 API 参考文档，包含：

#### 🔐 认证 API
- 用户注册
- 登录/登出
- 令牌刷新

#### 👤 用户 API
- 获取用户信息
- 更新资料
- 修改密码
- 停用账户

#### 🏠 房间 API
- 创建房间
- 加入/离开房间
- 邀请/踢出/禁止用户
- 获取房间列表
- 房间管理

#### 💬 消息 API
- 发送消息
- 获取消息历史
- 编辑消息
- 回复消息
- 撤回消息

#### 🔄 同步 API
- 事件同步
- 长轮询
- 状态管理

#### 📱 设备 API
- 获取设备列表
- 设备管理
- 设备删除

#### 🌐 在线状态 API
- 获取在线状态
- 设置在线状态

#### 🔒 端到端加密 API
- 启用/禁用加密
- 加密/解密消息
- 密钥上传/下载

#### 🔑 密钥备份 API
- 创建备份版本
- 上传/下载备份

#### 📷 媒体 API
- 上传媒体
- 下载媒体
- 获取缩略图

#### ❌ 错误码
- Matrix 标准错误码
- SDK 特定错误

#### 📋 类型定义
- 基础类型
- 消息类型
- 客户端配置

**适合人群**：SDK 使用者、集成开发者

---

## 🚀 快速开始

### 安装

```bash
npm install synapse-rust-sdk
```

### 基本使用

```javascript
import { MatrixClient } from 'synapse-rust-sdk';

// 创建客户端
const client = new MatrixClient({
  baseUrl: 'https://matrix.example.com'
});

// 登录
await client.login({
  type: 'm.login.password',
  user: 'alice',
  password: 'securePassword123'
});

// 发送消息
await client.sendMessage('!room:example.com', {
  msgtype: 'm.text',
  body: 'Hello, World!'
});
```

### 端到端加密

```javascript
const client = new MatrixClient({
  baseUrl: 'https://matrix.example.com',
  enableE2EE: true
});

await client.login({ /* ... */ });
await client.enableE2EE();

// 加密消息
const encrypted = await client.encryptMessage('!room:example.com', {
  msgtype: 'm.text',
  body: 'Secret message'
});

await client.sendMessage('!room:example.com', 'm.room.encrypted', encrypted);
```

---

## 📖 更多示例

更多使用示例请参考 [API 文档](./API-Documentation.md) 中的请求示例与完整流程示例。

---

## 🌟 主要特性

- ✅ **完整的 Matrix 协议支持** - 实现了所有核心 API
- 🔒 **端到端加密** - 基于 Olm/Megolm 的安全加密
- 📱 **跨平台** - 支持浏览器和 Node.js
- 🎨 **TypeScript** - 完整的类型定义
- 🚀 **高性能** - 优化的网络请求和事件处理
- 🔄 **自动重连** - 智能的网络重连机制
- 📦 **轻量级** - 最小的包体积

---

## 🤝 贡献

我们欢迎所有形式的贡献！请阅读 [贡献指南](./SDK-Development-Guide.md#贡献指南) 了解如何参与。

### 如何贡献

1. Fork 本仓库
2. 创建特性分支 (`git checkout -b feature/amazing-feature`)
3. 提交更改 (`git commit -m 'Add amazing feature'`)
4. 推送到分支 (`git push origin feature/amazing-feature`)
5. 创建 Pull Request

---

## 📄 许可证

MIT License - 详见 [LICENSE](../../LICENSE) 文件

---

## 🆘 获取帮助

- 📖 [文档](./API-Documentation.md)
- 💬 [讨论区](https://github.com/your-org/synapse-rust-sdk/discussions)
- 🐛 [问题反馈](https://github.com/your-org/synapse-rust-sdk/issues)
- 📧 [邮件支持](mailto:support@example.com)

---

## 🔗 相关链接

- [Matrix 官方网站](https://matrix.org/)
- [Matrix 协议规范](https://matrix.org/docs/spec/)
- [Synapse Rust 服务器](https://github.com/your-org/synapse-rust)
- [Matrix 客户端列表](https://matrix.org/clients/)

---

## 📊 版本信息

当前版本：v1.2.4

更新日志：2026-02-01 文档更新（接口对齐、FAQ 补充与集成指南完善）

---

## 📝 文档更新

本文档最后更新于：2026-02-01

如有问题或建议，请提交 [Issue](https://github.com/your-org/synapse-rust-sdk/issues) 或 [Pull Request](https://github.com/your-org/synapse-rust-sdk/pulls)。

---

**Happy Coding! 🎉**

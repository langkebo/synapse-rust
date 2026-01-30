# Synapse Rust JavaScript SDK 开发指南

## 目录

- [简介](#简介)
- [环境搭建](#环境搭建)
- [项目结构](#项目结构)
- [编码规范](#编码规范)
- [构建流程](#构建流程)
- [发布指南](#发布指南)
- [测试](#测试)
- [贡献指南](#贡献指南)

## 简介

Synapse Rust JavaScript SDK 是一个用于与 Synapse Rust Matrix 服务器交互的客户端库。本 SDK 提供了完整的 Matrix 协议实现，包括用户认证、房间管理、消息发送、端到端加密等功能。

### 主要特性

- 完整的 Matrix 客户端-服务器 API 支持
- 端到端加密（E2EE）
- 设备管理和密钥备份
- 好友和私聊功能
- 语音通话支持
- 媒体文件上传/下载
- 联邦通信支持
- TypeScript 类型支持

### 技术栈

- Node.js >= 16.0.0
- TypeScript >= 4.5.0
- Fetch API / Axios
- Web Crypto API（用于加密）

## 环境搭建

### 前置要求

在开始开发之前，请确保您的系统已安装以下软件：

- **Node.js**: [下载地址](https://nodejs.org/)
- **npm**: 随 Node.js 一起安装
- **Git**: [下载地址](https://git-scm.com/)
- **TypeScript**: `npm install -g typescript`

### 安装步骤

1. **克隆仓库**

```bash
git clone https://github.com/your-org/synapse-rust-sdk.git
cd synapse-rust-sdk
```

2. **安装依赖**

```bash
npm install
```

3. **验证安装**

```bash
npm run test
```

### 开发工具推荐

- **IDE**: Visual Studio Code
  - 推荐插件:
    - ESLint
    - Prettier
    - TypeScript Vue Plugin (Volar)
    - GitLens

- **浏览器**: Chrome / Firefox（用于调试）

- **API 测试工具**: Postman / Insomnia

## 项目结构

```
synapse-rust-sdk/
├── src/
│   ├── client/              # 客户端核心功能
│   │   ├── MatrixClient.ts   # 主客户端类
│   │   ├── Auth.ts         # 认证模块
│   │   ├── Room.ts         # 房间管理
│   │   ├── User.ts         # 用户管理
│   │   └── Device.ts       # 设备管理
│   ├── crypto/             # 加密模块
│   │   ├── E2EE.ts        # 端到端加密
│   │   ├── AES.ts         # AES 加密
│   │   └── Ed25519.ts    # Ed25519 签名
│   ├── api/                # API 调用
│   │   ├── HttpClient.ts   # HTTP 客户端
│   │   ├── endpoints.ts    # API 端点定义
│   │   └── types.ts      # 类型定义
│   ├── utils/             # 工具函数
│   │   ├── logger.ts      # 日志工具
│   │   ├── storage.ts     # 存储工具
│   │   └── helpers.ts    # 辅助函数
│   └── index.ts          # 入口文件
├── tests/                # 测试文件
│   ├── unit/             # 单元测试
│   ├── integration/      # 集成测试
│   └── e2e/            # 端到端测试
├── examples/            # 示例代码
├── docs/               # 文档
├── package.json        # 项目配置
├── tsconfig.json      # TypeScript 配置
├── .eslintrc.js      # ESLint 配置
├── .prettierrc       # Prettier 配置
└── README.md         # 项目说明
```

### 核心模块说明

#### client/MatrixClient.ts

主客户端类，提供所有功能的入口点。

```typescript
class MatrixClient {
  constructor(config: ClientConfig)
  login(username: string, password: string): Promise<LoginResponse>
  logout(): Promise<void>
  sync(): Promise<SyncResponse>
  // ... 其他方法
}
```

#### client/Auth.ts

处理用户认证相关功能。

```typescript
class Auth {
  register(username: string, password: string): Promise<RegisterResponse>
  login(username: string, password: string): Promise<LoginResponse>
  logout(): Promise<void>
  refreshToken(): Promise<RefreshResponse>
}
```

#### client/Room.ts

管理房间相关操作。

```typescript
class Room {
  createRoom(options: CreateRoomOptions): Promise<CreateRoomResponse>
  joinRoom(roomId: string): Promise<JoinRoomResponse>
  leaveRoom(roomId: string): Promise<void>
  inviteUser(roomId: string, userId: string): Promise<void>
  sendMessage(roomId: string, content: MessageContent): Promise<SendEventResponse>
  getMessages(roomId: string, options: GetMessagesOptions): Promise<MessagesResponse>
}
```

#### crypto/E2EE.ts

端到端加密实现。

```typescript
class E2EE {
  enable(): Promise<void>
  disable(): void
  encryptMessage(roomId: string, content: any): Promise<EncryptedContent>
  decryptMessage(event: MatrixEvent): Promise<any>
  uploadKeys(): Promise<void>
  downloadKeys(): Promise<void>
}
```

## 编码规范

### 命名规范

#### 文件命名

- 使用 PascalCase: `MatrixClient.ts`, `HttpClient.ts`
- 测试文件使用 `.test.ts` 或 `.spec.ts` 后缀

#### 变量和函数命名

- 使用 camelCase: `userId`, `sendMessage()`, `isLoggedIn`
- 常量使用 UPPER_SNAKE_CASE: `MAX_RETRIES`, `API_BASE_URL`

#### 类和接口命名

- 使用 PascalCase: `class MatrixClient`, `interface ClientConfig`

#### 类型命名

- 使用 PascalCase: `type LoginResponse`, `interface RoomEvent`

### 代码风格

#### 缩进和格式化

- 使用 2 空格缩进
- 使用单引号
- 每行最大长度 100 字符

#### 注释规范

```typescript
/**
 * 发送消息到指定房间
 * @param roomId - 房间 ID
 * @param content - 消息内容
 * @returns 发送事件响应
 * @throws {ApiError} 当发送失败时抛出
 * @example
 * ```typescript
 * const response = await client.sendMessage('!room:example.com', {
 *   msgtype: 'm.text',
 *   body: 'Hello, World!'
 * });
 * ```
 */
async sendMessage(
  roomId: string,
  content: MessageContent
): Promise<SendEventResponse> {
  // 实现
}
```

#### 错误处理

```typescript
try {
  const response = await this.httpClient.get(endpoint);
  return response.data;
} catch (error) {
  if (error instanceof ApiError) {
    // 处理 API 错误
    throw new ApiError(`Failed to fetch data: ${error.message}`);
  }
  throw new Error(`Unexpected error: ${error}`);
}
```

#### 异步处理

```typescript
// 推荐：使用 async/await
async function fetchData() {
  try {
    const data = await api.getData();
    return data;
  } catch (error) {
    console.error('Error:', error);
    throw error;
  }
}

// 避免：回调地狱
function fetchData(callback) {
  api.getData((data, error) => {
    if (error) {
      callback(null, error);
    } else {
      callback(data, null);
    }
  });
}
```

### TypeScript 规范

#### 类型定义

```typescript
// 使用 interface 定义对象类型
interface User {
  userId: string;
  displayName?: string;
  avatarUrl?: string;
}

// 使用 type 定义联合类型或别名
type MessageContent = TextMessage | ImageMessage | VideoMessage;

// 使用 enum 定义枚举
enum RoomMembership {
  Join = 'join',
  Leave = 'leave',
  Invite = 'invite',
  Ban = 'ban'
}
```

#### 类型注解

```typescript
// 函数参数和返回值必须有类型注解
function add(a: number, b: number): number {
  return a + b;
}

// 避免使用 any，使用 unknown 或具体类型
function processData(data: unknown): Result {
  if (typeof data === 'string') {
    return { success: true, data };
  }
  return { success: false, error: 'Invalid data' };
}
```

#### 泛型使用

```typescript
function createApiResponse<T>(data: T, success: boolean): ApiResponse<T> {
  return {
    data,
    success,
    timestamp: Date.now()
  };
}
```

## 构建流程

### 开发构建

```bash
# 启动开发模式（监听文件变化）
npm run dev

# 或使用 TypeScript 编译器
npm run dev:ts
```

### 生产构建

```bash
# 构建生产版本
npm run build

# 构建并压缩
npm run build:minify

# 构建并分析包大小
npm run build:analyze
```

### 构建配置

#### tsconfig.json

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "module": "ESNext",
    "lib": ["ES2020", "DOM"],
    "moduleResolution": "node",
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true,
    "outDir": "./dist",
    "rootDir": "./src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist", "tests"]
}
```

### 构建输出

构建完成后，输出文件位于 `dist/` 目录：

```
dist/
├── index.js              # 入口文件（UMD）
├── index.esm.js         # ES Module 版本
├── index.d.ts           # TypeScript 类型定义
├── client/
│   ├── MatrixClient.js
│   ├── Auth.js
│   └── ...
├── crypto/
│   ├── E2EE.js
│   └── ...
└── utils/
    ├── logger.js
    └── ...
```

## 发布指南

### 版本管理

使用语义化版本（Semantic Versioning）：

- **MAJOR**: 不兼容的 API 变更
- **MINOR**: 向后兼容的功能新增
- **PATCH**: 向后兼容的问题修复

示例：`1.2.3` → `1.2.4`（修复）, `1.3.0`（新功能）, `2.0.0`（破坏性变更）

### 发布流程

#### 1. 更新版本号

```bash
# 更新 patch 版本（例如 1.2.3 → 1.2.4）
npm version patch

# 更新 minor 版本（例如 1.2.3 → 1.3.0）
npm version minor

# 更新 major 版本（例如 1.2.3 → 2.0.0）
npm version major
```

#### 2. 更新 CHANGELOG.md

```markdown
## [1.2.4] - 2024-01-15

### Added
- 新增批量消息发送功能
- 添加房间成员列表缓存

### Fixed
- 修复端到端加密密钥同步问题
- 修复重连后消息丢失问题

### Changed
- 优化同步性能，减少网络请求
- 更新依赖到最新版本

### Deprecated
- 弃用旧的认证方法，建议使用新的 OAuth2 流程

### Removed
- 移除对 IE11 的支持

### Security
- 修复潜在的 XSS 漏洞
```

#### 3. 运行测试

```bash
# 运行所有测试
npm test

# 运行测试并生成覆盖率报告
npm run test:coverage

# 确保测试通过且覆盖率不低于 80%
```

#### 4. 构建生产版本

```bash
npm run build
```

#### 5. 发布到 npm

```bash
# 登录 npm（首次需要）
npm login

# 发布包
npm publish

# 发布 beta 版本
npm publish --tag beta

# 发布 next 版本
npm publish --tag next
```

#### 6. 创建 Git 标签

```bash
git tag -a v1.2.4 -m "Release version 1.2.4"
git push origin v1.2.4
```

### 发布检查清单

在发布之前，请确认：

- [ ] 所有测试通过
- [ ] 代码覆盖率 >= 80%
- [ ] 更新了 CHANGELOG.md
- [ ] 更新了 README.md（如有必要）
- [ ] 运行了 `npm run build` 且无错误
- [ ] 更新了版本号
- [ ] 检查了依赖项安全性：`npm audit`
- [ ] 在至少一个真实项目中测试过
- [ ] 文档已更新

### 回滚发布

如果发现问题需要回滚：

```bash
# 从 npm 取消发布（仅限 72 小时内）
npm unpublish synapse-rust-sdk@1.2.4

# 或发布新版本修复问题
npm version patch
npm publish
```

## 测试

### 单元测试

```bash
# 运行所有单元测试
npm run test:unit

# 运行特定测试文件
npm run test:unit -- client/MatrixClient.test.ts

# 监听模式
npm run test:unit -- --watch
```

### 集成测试

```bash
# 运行集成测试
npm run test:integration

# 需要先启动测试服务器
npm run test:server
```

### 端到端测试

```bash
# 运行 E2E 测试
npm run test:e2e
```

### 测试覆盖率

```bash
# 生成覆盖率报告
npm run test:coverage

# 查看覆盖率报告
open coverage/index.html
```

### 测试示例

```typescript
import { MatrixClient } from '../src/client/MatrixClient';
import { describe, it, expect, beforeEach } from '@jest/globals';

describe('MatrixClient', () => {
  let client: MatrixClient;

  beforeEach(() => {
    client = new MatrixClient({
      baseUrl: 'https://matrix.example.com',
      accessToken: 'test-token'
    });
  });

  describe('login', () => {
    it('should login successfully with valid credentials', async () => {
      const response = await client.login('username', 'password');
      expect(response.success).toBe(true);
      expect(response.accessToken).toBeDefined();
    });

    it('should throw error with invalid credentials', async () => {
      await expect(
        client.login('invalid', 'invalid')
      ).rejects.toThrow('Invalid credentials');
    });
  });

  describe('sendMessage', () => {
    it('should send message to room', async () => {
      const response = await client.sendMessage('!room:example.com', {
        msgtype: 'm.text',
        body: 'Hello'
      });
      expect(response.eventId).toBeDefined();
    });
  });
});
```

## 贡献指南

### 如何贡献

1. Fork 本仓库
2. 创建特性分支：`git checkout -b feature/amazing-feature`
3. 提交更改：`git commit -m 'Add amazing feature'`
4. 推送到分支：`git push origin feature/amazing-feature`
5. 创建 Pull Request

### 提交规范

使用 Conventional Commits 规范：

```
<type>(<scope>): <subject>

<body>

<footer>
```

类型（type）：
- `feat`: 新功能
- `fix`: 修复 bug
- `docs`: 文档更新
- `style`: 代码格式（不影响功能）
- `refactor`: 重构
- `perf`: 性能优化
- `test`: 测试相关
- `chore`: 构建/工具相关

示例：

```
feat(auth): add OAuth2 support

Implement OAuth2 authentication flow for better security.
- Add authorization endpoint
- Add token refresh logic
- Update documentation

Closes #123
```

### Pull Request 检查清单

- [ ] 代码符合项目编码规范
- [ ] 添加了必要的测试
- [ ] 所有测试通过
- [ ] 更新了相关文档
- [ ] 提交信息符合规范
- [ ] PR 描述清晰说明了更改内容

### 行为准则

- 尊重所有贡献者
- 接受建设性批评
- 专注于对社区最有利的事情
- 对其他社区成员表示同理心

## 常见问题

### Q: 如何调试 SDK？

A: 使用浏览器开发者工具或 VS Code 调试器。在代码中添加 `debugger` 语句或使用 `console.log`。

### Q: 如何处理网络错误？

A: SDK 内置了重试机制。默认重试 3 次，可通过配置自定义。

### Q: 如何启用端到端加密？

A: 在创建客户端时设置 `enableE2EE: true`：

```typescript
const client = new MatrixClient({
  baseUrl: 'https://matrix.example.com',
  enableE2EE: true
});
```

### Q: SDK 支持哪些浏览器？

A: 支持所有现代浏览器（Chrome, Firefox, Safari, Edge）的最新版本。

## 资源链接

- [Matrix 协议规范](https://matrix.org/docs/spec/)
- [API 文档](./API.md)
- [示例代码](../examples/)
- [问题反馈](https://github.com/your-org/synapse-rust-sdk/issues)
- [讨论区](https://github.com/your-org/synapse-rust-sdk/discussions)

## 许可证

MIT License - 详见 [LICENSE](../../LICENSE) 文件

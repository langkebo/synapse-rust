# Element Web 浏览器级 Harness

这是一个用于在真实的浏览器环境中与 synapse-rust 交互的测试 harness，使用 Playwright 进行浏览器自动化。

## 功能

当前支持的测试脚本：

### 1. `smoke:login`
基础登录冒烟测试：
- 打开 Element Web 登录界面
- 填写用户名和密码
- 点击登录按钮
- 通过监听浏览器控制台的 `setLoggedIn` 消息判断登录是否成功

### 2. `test:basic`
基础交互测试（实验性）：
- 包含完整的登录流程
- 尝试创建一个新房间
- 尝试在新房间中发送测试消息

## 使用方法

### 环境变量

| 变量 | 描述 | 默认值 |
|------|------|--------|
| `TEST_SCRIPT` | 选择要运行的 npm 脚本 | `smoke:login` |
| `ELEMENT_BASE_URL` | Element Web 地址 | `https://element.test` |
| `ELEMENT_TEST_USERNAME` | 测试用户名 | 自动生成 |
| `ELEMENT_TEST_PASSWORD` | 测试密码 | `Test@123456` |
| `ELEMENT_HARNESS_ARTIFACT_DIR` | 截图和日志目录 | `../../artifacts/e2ee-interop` |
| `PLAYWRIGHT_HEADLESS` | 是否无头模式运行浏览器 | `1` (是) |
| `BROWSER_ONLY_OVERLAY` | 是否在现有 synapse-rust 栈上只启动 Element Web | `0` |
| `KEEP_STACK_RUNNING` | 测试完成后是否保持 Docker 容器运行 | `0` |
| `SKIP_NODE_INSTALL` | 是否跳过 Node.js 依赖安装 | `0` |

### 运行完整测试栈

```bash
# 运行默认的登录冒烟测试
bash scripts/test/run_element_web_browser_harness.sh

# 运行基础交互测试
TEST_SCRIPT=test:basic bash scripts/test/run_element_web_browser_harness.sh
```

### 在现有后端栈上运行（快速迭代）

如果你已经有一个运行的 synapse-rust Docker 栈，可以只启动 Element Web 覆盖层以节省时间：

```bash
# 快速覆盖层启动 + 测试
BROWSER_ONLY_OVERLAY=1 SKIP_NODE_INSTALL=1 TEST_SCRIPT=smoke:login bash scripts/test/run_element_web_browser_harness.sh

# 保持栈运行，方便调试
KEEP_STACK_RUNNING=1 BROWSER_ONLY_OVERLAY=1 SKIP_NODE_INSTALL=1 bash scripts/test/run_element_web_browser_harness.sh
```

### 单独运行 Playwright 脚本

你也可以直接运行 Playwright 测试脚本，前提是：
1. 所有 Docker 服务都在运行
2. 你已在 `/etc/hosts` 配置了 `matrix.test` 和 `element.test` 指向 localhost
3. 你已安装了 Node.js 依赖和 Playwright

```bash
cd tests/element-web-harness
ELEMENT_TEST_USERNAME=myuser ELEMENT_TEST_PASSWORD=mypassword npm run smoke:login
```

## 目录结构

```
tests/element-web-harness/
├── README.md                    # 本文件
├── login-smoke.mjs              # 登录冒烟测试脚本
├── basic-interactions.mjs       # 基础交互测试脚本（实验性）
├── package.json                 # npm 包配置
└── node_modules/                # 依赖（运行 npm install 后生成）
```

## 开发新测试

要添加新的浏览器测试，你可以：

1. 创建一个新的 `.mjs` 文件
2. 使用 Playwright API 编写测试逻辑
3. 在 `package.json` 的 `scripts` 字段中添加对应的 npm 命令
4. 通过 `TEST_SCRIPT=your-script-name` 环境变量选择运行

## 注意事项

- Playwright 需要安装特定版本的 Chromium 浏览器，首次运行会自动安装
- 对于 macOS 用户，可能需要调整系统设置以允许未经签名的 Chromium 运行
- 无头模式默认启用，如需查看浏览器，请设置 `PLAYWRIGHT_HEADLESS=0`

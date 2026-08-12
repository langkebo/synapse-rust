# Synapse-Rust 全量 API 测试工具

自动遍历全部已定义 API 路由（基于 RouteLedger 导出的路由清单），逐路由发送请求，
校验响应正确性并输出健康度报告。可用于开发 / 测试 / 生产任意环境。

## 文件说明

| 文件 | 作用 |
| --- | --- |
| `run_api_tests.py` | 主测试执行器（Python 3，依赖 requests + pyyaml） |
| `config.yaml` | 环境配置：base_url / token / 超时 / 并发 / 路径参数 |
| `expectations.yaml` | 关键端点的响应字段与类型精确校验规则 |
| `export_ledger.sh` | 导出最新路由清单（与 Docker 镜像相同 features） |
| `ledger.json` | 默认路由清单（已按 Docker features 导出，1292 条） |
| `reports/` | 测试报告输出目录 |

> **关于路由清单**：`ledger.json` 使用与 Docker 镜像一致的 features
> （`server,core-private-chat,widgets,external-services,voice-extended,cas-sso,saml-sso,friends`）
> 导出，共 1292 条。仓库内 `tests/unit/fixtures/ledger_export/default.json` 是
> default-features 构建（1242 条，仅含 voice/saml/cas 之外的默认路由），仅供单元测试
> golden-file 使用。路由有增删时请重新执行 `./export_ledger.sh` 刷新 `ledger.json`。

## 快速开始

```bash
# 0.（可选）安装依赖：python3 -m pip install requests pyyaml

# 1.（首次建议）导出最新路由清单 —— 需要 cargo，编译约数分钟
./export_ledger.sh

# 2. 运行全量测试（默认目标 https://matrix.test，自动登录获取 token）
python3 run_api_tests.py

# 3. 查看报告
open reports/api_test_report.html
```

## 常用参数

```bash
# 指定环境 / 地址
python3 run_api_tests.py --env prod --base-url https://your-server.example

# 手动指定 token（跳过自动登录）
python3 run_api_tests.py --token "syt_xxx"

# 自签名证书环境
python3 run_api_tests.py --no-verify-tls

# 只测某个模块（按路由清单 registered_by 过滤，例如 admin / media / sync）
python3 run_api_tests.py --only-module admin

# 开启写操作认证探测（空 body，预期 4xx；prod 环境自动忽略）
python3 run_api_tests.py --allow-write

# 输出到自定义目录
python3 run_api_tests.py --report-dir /tmp/api-reports
```

## 环境变量（优先级：命令行 > 环境变量 > config.yaml）

| 变量 | 说明 |
| --- | --- |
| `API_TEST_ENV` | dev / test / prod |
| `API_TEST_BASE_URL` | 目标服务器地址 |
| `API_TEST_TOKEN` | 访问令牌（跳过自动登录） |
| `API_TEST_VERIFY_TLS` | true / false |
| `API_TEST_TIMEOUT` | 单请求超时秒数 |
| `API_TEST_CONCURRENCY` | 并发数 |
| `API_TEST_ALLOW_WRITE` | true / false |
| `API_TEST_USER` / `API_TEST_PASSWORD` | 自动登录凭据 |
| `API_TEST_ADMIN_USER` / `API_TEST_ADMIN_PASSWORD` | admin 凭据 |

## 测试策略（安全默认）

- **匿名探测（所有路由）**：不带 token 请求。端点返回 401/403 → 鉴权边界正确；
  2xx → 公开端点可用；其它 4xx → 路由存活（占位符资源不存在，符合预期）；
  5xx / 超时 / 连接失败 → FAIL。
- **认证探测（GET/HEAD）**：带 token 请求占位符路径，验证 token 被接受。
- **写操作（POST/PUT/DELETE）**：默认只做匿名探测，**不发送真实写入**。
  需验证写路径时可加 `--allow-write`（空 body 请求，预期 4xx 校验错误）。
  `prod` 环境强制关闭写探测。
- **路径参数**：`{user_id}`、`{room_id}` 等由 `config.yaml path_params` 映射为
  "确定性不存在"的占位值，期望 handler 返回 4xx 而非 5xx。

## 校验维度

1. **状态码**：自动学习端点鉴权类型，判定是否符合预期（显式规则可覆盖）。
2. **JSON 结构与类型**：`expectations.yaml` 为关键公开端点定义字段存在性 +
   类型约束（如 `versions` 必须是非空数组）。
3. **关键业务异常值**：如 `/health` 必须返回 `healthy`、`/_health` 的 `status`
   必须为 `healthy`、`/login` 的 `flows` 不允许为空。

## 评分与报告

- 端点级：PASS=1.0 / WARN=0.5 / FAIL=0，端点取最差探测结果。
- 健康度 = 加权平均 × 100；≥95 优秀 / ≥90 良好 / ≥80 一般 / ≥60 较差 / <60 危险。
- 输出三份报告：
  - `api_test_report.json` — 完整机器可读数据（含每请求明细）
  - `api_test_report.md` — 摘要 + 失败明细 + 模块统计
  - `api_test_report.html` — 自包含可视化报告（评分环、状态卡、失败表格）

## 退出码

- `0`：无失败接口
- `1`：存在失败接口
- `2`：运行错误（清单缺失 / 配置错误）

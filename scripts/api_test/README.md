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
| `generate_openapi.py` | **Week 1 Task 1+2** — 从 ledger JSON 生成 OpenAPI 3.0 规范（支持多 profile） |
| `refresh_openapi_specs.py` | **Week 1 Task 2** — 一键导出所有 profile ledger + 生成所有 OpenAPI spec |
| `schemathesis_smoke_test.py` | **Week 1 Task 3** — schemathesis 4.x 冒烟测试 (5 端点) |
| `schemathesis_extended_test.py` | **Week 2 Task 1** — 自动发现全部 optional 端点,扩展到 48 端点 |
| `schemathesis_authenticated_test.py` | **Week 2 Task 2** — user-auth token 测试,扩展到 278 端点 |
| `token_manager.py` | Token 管理器 (user + admin login, 缓存 50min) |
| `errcode_validator.py` | **Week 2 Task 3** — 4xx errcode 规范校验规则 (39 标准 errcode, 82 端点规则) |
| `scan_handler_schemas.py` | **Week 2 Task 4** — 扫描 handler 签名补 OpenAPI requestBody schema |
| `reports/` | 测试报告输出目录 |
| `../../docs/openapi/client.yaml` | 生成的 OpenAPI 规范（可被 Swagger UI / schemathesis 使用） |
| `../../docs/openapi/index.json` | OpenAPI manifest 索引 |

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

## OpenAPI requestBody Schema 补全 (Week 2 Task 4 — 2026-09-01)

**目标**: 将 Rust handler 函数的 `Json<TypeName>` 签名映射到 OpenAPI `requestBody` schema。

```bash
# 一次性扫描 + 补全 (会修改 docs/openapi/client.yaml)
python3 scripts/api_test/scan_handler_schemas.py
```

**原理 (4 阶段)**:
1. **Stage A** — 解析 `src/web/routes/*.rs` 中所有 `.route("/path", METHOD(handler))` 注册 → 648 个路由,328 个 write routes
2. **Stage B** — 解析 handler 函数签名,提取 `Json<TypeName>`  extractor → 139 个 handler 用强类型
3. **Stage C** — 从 handler 所在文件找 `#[derive(Deserialize)] struct TypeName` → 98 个 struct 提取成功
4. **Stage D** — join 路由注册 × handler × struct → 精确 path → schema 映射,补入 `client.yaml`

**当前成果**:
- 82/409 个 write operations 已补上 `requestBody` schema (`required: true`)
- 每个 schema 包含精确字段名(serde rename)、类型(Option/Vec/primitive)、required 列表
- `scripts/api_test/handler_schemas.json` 存档全部扫描结果,供后续复用

**使用方式**:
```bash
# 补全后验证 (schemathesis 读补全后的 spec)
python3 scripts/api_test/schemathesis_authenticated_test.py --limit 10 --max-cases 3
```

**已知局限**:
- 509 个 handler 用 `Json<Value>` 类型擦除 → 无法自动提取 schema,需手动加 `#[derive(Deserialize)]`
- 某些路由(如 `account/3pid/add`, `account/3pid/bind`)共用一个 handler 但无 body schema → 同上

---

## OpenAPI Response Schema 探活 (Week 2 Task 5 — 2026-09-01)

**目标**: 探活 localhost:8008 活服务器,采集 2xx 响应 JSON 结构 → 补进 OpenAPI spec 的 `responses.200.schema`。

```bash
# 探测所有无 path param 的 GET 端点,补 response schema
python3 scripts/api_test/probe_responses.py
```

**原理**:
1. 从 `docs/openapi/client.yaml` 读所有 GET 端点,过滤无 path param 且非 unstable 的 (138 个)
2. 根据 spec `security` 字段决定是否带 token — 公开端点不带,需要认证端点带
3. 逐端点发 curl 请求,采集 200 响应 JSON → 用 `infer_schema()` 转 JSON Schema
4. 更新 spec 的 `responses.200.content.application/json.schema`
5. `scripts/api_test/response_schemas.json` 存档原始采集结果

**当前成果**:
- 107/138 GET 端点探到 2xx 响应,补上精确 JSON Schema
- Schema 包含真实字段名 (如 `sync.next_batch`, `devices[].last_seen_ts` 等)
- 31 个端点返回 4xx (SAML/SSO 禁用、需 query param、需要管理员权限等)

**关键技术问题**:
- **YAML alias 污染**: 原始 spec 用 YAML anchors/aliases 共享 `responses.200` 对象 → 修一个等于修全部。
  解决: `generate_openapi.py` 用自定义 `NoAliasDumper` 关闭 anchor detection。
- **HTTP method case-sensitivity**: curl `-X get` (小写) 让 curl 发送字面量 "get" 而非 HTTP GET → 405。
  解决: 在 `curl_request()` 里强制 `.upper()`。
- **cross-origin token**: 从 matrix.test 拿的 token 在 localhost:8008 上无效 → 正确做法是先探服务器是否可达。

**验证**:
```bash
$ python3 scripts/api_test/schemathesis_authenticated_test.py --limit 10 --max-cases 5
✓ Passed: 10/10, ✓ Errcode Validation: all endpoints returned expected errcodes
```

---

## OpenAPI 规范生成 (Week 1 Task 1 — 2026-09-01)

**策略 A — 零侵入**: 不改任何 Rust 代码,直接从 `synapse_ledger_export` 的 JSON 产物生成 OpenAPI 3.0。

```bash
# 依赖: pip install pyyaml
# 生成 (或刷新):
python3 scripts/api_test/generate_openapi.py \
    --ledger scripts/api_test/ledger.json \
    --output docs/openapi/client.yaml

# 验证:
python3 -c "import yaml; d=yaml.safe_load(open('docs/openapi/client.yaml')); print(d['openapi'], d['info']['version'], 'paths:', len(d['paths']), 'ops:', sum(len(p) for p in d['paths'].values()))"
```

**已知局限 (Week 2 待补)**:
- ledger 的 `auth` 字段覆盖率不足 (仅 1/898 端点有标记) → Week 2 用 profile split 补充
- 响应 schema 均为 TODO placeholder → Week 2~3 用 schemathesis 探测补充
- 所有端点默认 200/400/401/403/404/429 响应

**CI 集成**: 每次 PR diff `docs/openapi/client.yaml`; 端点增加 >5% 触发 review 提醒。

---

## 退出码

- `0`：无失败接口
- `1`：存在失败接口
- `2`：运行错误（清单缺失 / 配置错误）

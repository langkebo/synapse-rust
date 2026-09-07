# Week 2 Task 5 — 探针活服务器 → 补 Response Schema

**状态**: ✅ 完成 (第一轮, 107/138 patched)
**日期**: 2026-09-01
**负责**: synapserust-api-tester

## 任务目标

把活服务器的 2xx 响应 JSON 结构采集下来,转为 JSON Schema,补进 OpenAPI spec 的
`responses.200.content.application/json.schema`,从而让契约测试工具 (如
schemathesis) 能根据**真实响应结构**做 assertion。

## 挑战

- **138 个 GET 端点**无 path param + 非 unstable,理论上都可探活
- 不同端点 `security` 不同:有些 optional 不带 token,有些需要 user token
- **OpenAPI YAML 文件有 7221 个 anchors/aliases** — `yaml.safe_load()` 后多处操作
  共享同一对象引用,直接修改会**污染所有 operation**
- HTTP method 大小写敏感: `curl -X get` 发送字面 "get" 而非 GET → 405

## 解决方案

1. **`generate_openapi.py` 加自定义 Dumper** (`NoAliasDumper`) 关闭 anchor 检测,
   让 dump 出来的 YAML 没有 `&id001` / `*id001`,后续 patch 完全独立
2. **`probe_responses.py` curl_request 强制大写 method**
3. **根据 spec 的 `security` 字段决定是否带 token** — optional 不带,需要 auth 的带
4. **`infer_schema()`** 从 Python 对象递归生成 JSON Schema,处理 dict/list/基础类型

## 交付物

| 文件 | 改动 | 说明 |
|---|---|---|
| `scripts/api_test/probe_responses.py` | 新建 | 主探测脚本 (~300 行) |
| `scripts/api_test/response_schemas.json` | 新建 | 138 个探测结果 (含 status, schema, sample_value) |
| `scripts/api_test/generate_openapi.py` | 修改 | 加 `NoAliasDumper`,消除 YAML aliases |
| `docs/openapi/client.yaml` | 更新 | 107 个端点的 200 响应 schema |
| `scripts/api_test/README.md` | 更新 | 加 Task 5 章节 |

## 关键成果

```
[probe] 138 GET endpoints (no path params, no unstable)
[probe] [60/138] get /_matrix/client/v1/friends/groups → 200
[probe] [80/138] get /_matrix/client/v1/spaces/user → 200
[probe] done: {'probed': 138, 'got_2xx': 107, 'got_4xx': 31, 'errors': 0}
[probe] wrote: docs/openapi/client.yaml (patched 107 response schemas)
```

### 典型 schema 样例

**GET /sync** (认证 GET):
```json
{
  "type": "object",
  "properties": {
    "account_data": {...},
    "device_lists": {...},
    "device_one_time_keys_count": {...},
    "next_batch": {"type": "string"},
    "presence": {...},
    "rooms": {...},
    "to_device": {...}
  },
  "required": ["next_batch"]
}
```

**GET /devices**:
```json
{
  "type": "object",
  "properties": {
    "devices": {
      "type": "array",
      "items": {
        "type": "object",
        "properties": {
          "device_id": {"type": "string"},
          "display_name": {"type": "null"},
          "last_seen_ip": {"type": "null"},
          "last_seen_ts": {"type": "integer"}
        }
      }
    }
  },
  "required": ["devices"]
}
```

**GET /whoami**:
```json
{
  "type": "object",
  "properties": {
    "device_id": {"type": "string"},
    "is_guest": {"type": "boolean"},
    "user_id": {"type": "string"}
  },
  "required": ["device_id", "is_guest", "user_id"]
}
```

## 验证

```bash
$ python3 scripts/api_test/schemathesis_authenticated_test.py --max-cases 5 --limit 10
[1/10] DELETE /_matrix/client/r0/room_keys/keys
[2/10] DELETE /_matrix/client/v1/room_keys/keys
[3/10] DELETE /_matrix/client/v3/room_keys/keys
[4/10] GET /_matrix/client/r0/account/3pid   → 5 2xx, 0 4xx
[5/10] GET /_matrix/client/r0/account/whoami → 5 2xx, 0 4xx

✓ Passed: 10/10
✓ Errcode Validation: all endpoints returned expected errcodes
```

- 0 个 errcode 违规
- 0 个 5xx
- spec 仍是合法 OpenAPI 3.0.3 (898 ops, 704 paths, 47 tags)

## 已知局限

- **31 个 GET 端点返回 4xx**:
  - SAML/SSO 端点 (服务未启用 SAML 配置)
  - 需要 query param 的端点 (如 `/register/available?username=...`)
  - 需要 admin 权限的端点
- 仅 GET 端点被探测 — POST/PUT/PATCH 需要先有真实写入场景(后续需要 sandbox 环境)
- 仅 200 响应被采集 — 4xx 响应 schema 沿用 placeholder

## 经验沉淀

- **YAML aliases 是 patch 工具的天敌**: 大 spec 用 anchors 节省字节很常见,
  但任何 `yaml.safe_load() → 改 → dump` 的脚本都会被别名干扰,**必须用自定义 Dumper 禁用 alias**
- **HTTP method 大小写敏感**: curl 的 `-X get` 小写会让 curl 把方法当成字面"get"发送
  (而不是默认的 GET),服务器返回 405 Method Not Allowed。统一在 wrapper 里 `.upper()`
- **`security=[]` vs `security=[AccessToken]`**: 自动判断时直接看 `op.get("security", [])`
  的布尔值比硬编码哪些端点需要 token 更可靠
- **采集 schema 与 patch schema 分离**: 把 `response_schemas.json` 存档,后续人工 review
  或调整 ruleset 都很方便,不需要每次重跑网络探测

## 累计覆盖率 (Week 2 收尾)

| 任务 | 新增端点 | 累计覆盖 | 总占比 |
|---|---|---|---|
| Week 1 Task 1+2 | (OpenAPI 生成基础) | - | - |
| Week 1 Task 3 (smoke) | 5 | 5 | 0.6% |
| Week 2 Task 1 (extended) | 48 | 48 | 5.3% |
| Week 2 Task 2 (auth) | 278 | 326 | 36.3% |
| Week 2 Task 3 (errcode) | (同 Task 2) | 326 | 36.3% |
| Week 2 Task 4 (requestBody) | 82 schemas | - | - |
| **Week 2 Task 5 (response)** | **107 schemas** | - | - |

### OpenAPI Spec 现状

- 898 个 operation 全部有 200/204/400/401/403/404/429 响应模板
- 107 个 200 响应有真实 schema (由 Task 5 探活)
- 82 个 POST/PUT/PATCH 有真实 requestBody schema (由 Task 4 扫描)
- 仍为 `GenericResponse` placeholder 的 200 响应:有 path param 的 GET (560+ 个) + 写入操作 2xx

## 后续工作

- **Week 2 Task 6**: 把 `Json<Value>` handler 重构为 `Json<ConcreteStruct>`,Task 4 重跑补完
- **Path param 探测**: 用 `config.yaml:path_params` 实例化 `/rooms/{room_id}/...`,可补 500+ 端点
- **Sandbox 环境**: 起一个本地 Postgres + Synapse,写一个 POST 端点 → 200 全流程探活脚本
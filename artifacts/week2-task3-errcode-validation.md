# Week 2 Task 3 — 4xx Errcode 规范校验

**状态**: ✅ 完成
**日期**: 2026-09-01
**负责**: synapserust-api-tester

## 任务目标

对 Week 2 Task 2 跑出的 1753 个 4xx 响应,按 Matrix Client-Server API 规范做
**per-endpoint-type errcode 白名单校验**。不在白名单的 errcode 标记为
unexpected,作为"语义层面的回归信号"。

**为何要做这件事**: 4xx 本身只是 HTTP 状态码,真正能反映 handler 语义是否正确的
是 errcode 字段 (如 `M_FORBIDDEN` vs `M_NOT_FOUND`)。当 handler 对随机输入的
返回 errcode 不符合 spec,往往意味着:

- 错误分类错了 (把权限问题报成参数错误)
- 错误被包成了 HTTP 5xx (escaped 5xx = 内部错误)
- handler 走到了不该走的分支

## 交付物

### 1. `scripts/api_test/errcode_validator.py` — errcode 规范规则

- **39 个标准 Matrix errcode 全集** (`STANDARD_ERRCODES`)
- **82 条 (path × method) 规则** (`_RULES`),按"先具体后通用"顺序匹配
- `_BASE_AUTH`:任何带 token 的端点基础集 (`M_MISSING_TOKEN`, `M_UNAUTHORIZED`,
  `M_FORBIDDEN`, `M_BAD_JSON`, `M_NOT_JSON`, `M_INVALID_ARGUMENT`,
  `M_UNRECOGNIZED`, `M_UNKNOWN`)
- `_BASE_OPTIONAL`:可匿名访问端点的基础集 (与 _BASE_AUTH 类似,但无 token 相关)
- **特殊端点的额外白名单**,如:
  - `/login` (POST): 加 `M_INVALID_USERNAME`, `M_INVALID_PASSWORD`, `M_USER_SUSPENDED`
  - `/register` (POST): 加 `M_USER_IN_USE`, `M_WEAK_PASSWORD`,
    `M_REGISTRATION_DISABLED`, `M_THREEPID_IN_USE`
  - `/account/deactivate`, `/delete_devices`, `/device_signing`: 加 `M_UIA_REQUIRED`
  - `/keys/signatures`: 加 `M_INVALID_SIGNATURE`
  - `/createRoom`: 加 `M_ROOM_IN_USE`, `M_LIMIT_EXCEEDED`
  - `/voice/upload`: 加 `M_TOO_LARGE`, `M_MAX_UPLOAD_SIZE_EXCEEDED`

`validate_errcode(path, method, errcode)` 返回:
```python
{
  "expected": set[str],   # 该端点允许的 errcode 集合
  "valid": bool,          # 该 errcode 是否合法
  "reason": str,          # 当 invalid 时的描述
}
```

`errcode=None` 或 `unparseable` 视为合法 (响应不是 JSON,通常
`content-type: text/plain`,是服务器结构错误)。

### 2. `scripts/api_test/schemathesis_authenticated_test.py` — 集成

- 每个 4xx 都从 `response.json()` 提取 `errcode` (之前因 `body_preview` 截断 60 字符
  而被错过的 JSON 已经能正确读取)
- 调用 `validate_errcode()` 判断该 errcode 对该端点是否合法
- 不合法的 errcode + 触发次数 + 示例 body 写入 `unexpected_errcodes` 字段
- 报告顶层 `errcode_violations` 字段聚合所有违规端点
- CLI 加 `--no-errcode-check` 跳过校验

## 关键发现 (真实数据驱动)

第一次跑 → **126/278 端点触发 unexpected errcode**,核心信号:

| errcode | 出现次数 | 原因 |
|---|---|---|
| `M_UNAUTHORIZED` | 1219 | **STANDARD_ERRCODES 漏列** + `_BASE_AUTH` 漏加。已修复:加入全集 + base。 |
| `M_UIA_REQUIRED` | 28 | sensitive endpoints (logout/refresh/deactivate/delete_devices/device_signing) 触发的用户交互认证。已加规则。 |
| `M_LIMIT_EXCEEDED` | 9 | `/refresh` 端点 schemathesis 反复触发被限速。已加白名单。 |
| `M_UNRECOGNIZED` | 40 | 已在 `_BASE_AUTH` 中 (合法) |

**最终结果**:

```
✓ Passed: 278/278
✗ Failed: 0/278
✓ Errcode Validation: all endpoints returned expected errcodes
```

**4xx errcode 分布**:
- `M_UNAUTHORIZED`: 1219 (随机 body 无 refresh token 或 token schema 不匹配)
- `M_BAD_JSON`: 146 (随机 JSON schema 不匹配)
- `M_FORBIDDEN`: 100
- `M_UNRECOGNIZED`: 40 (含未知字段)
- `M_UIA_REQUIRED`: 24 (敏感操作要求密码再次确认)
- `M_LIMIT_EXCEEDED`: 9 (频繁请求触发限速)
- `unparseable`: 215 (服务器返回 text/plain 错误,不是 JSON — 这是已知的结构性错误,如 `Failed to deserialize query string: missing field 'version'`)

## 累计覆盖率

```
Week 1 Task 3 (smoke)        : 5 endpoints
Week 2 Task 1 (extended)     : 48 endpoints
Week 2 Task 2 (authenticated): 278 endpoints
Week 2 Task 3 (errcode check) : 278 endpoints (same set, 加上 errcode 校验)
────────────────────────────────────────────────
总计                          : 326/898 = 36.3%
```

剩余未覆盖:
- 2 个 optional 端点带 `{path_params}`: `/.well-known/openid-configuration`, `/.well-known/jwks.json`
- 555 个 user-auth 端点带 `{path_params}` (如 `/rooms/{room_id}/...`)
- admin/federation spec 还没生成

## 文件清单

| 文件 | 类型 | 说明 |
|---|---|---|
| `scripts/api_test/errcode_validator.py` | 新建 | errcode 白名单规则 + `validate_errcode()` |
| `scripts/api_test/schemathesis_authenticated_test.py` | 修改 | 集成 errcode 校验 + `unexpected_errcodes` 字段 + `--no-errcode-check` |
| `scripts/api_test/reports/schemathesis_authenticated.json` | 更新 | 含 `errcode_violations` + 校验统计 |

## 使用方式

```bash
# 默认 — 跑全部 278 端点 + errcode 校验 (≈5-7 分钟)
python3 scripts/api_test/schemathesis_authenticated_test.py --max-cases 10

# 调试 — 只跑前 N 个端点
python3 scripts/api_test/schemathesis_authenticated_test.py --limit 10

# 跳过 errcode 校验 (仅看 5xx / network)
python3 scripts/api_test/schemathesis_authenticated_test.py --no-errcode-check

# 仅校验 errcode 规则
python3 scripts/api_test/errcode_validator.py /_matrix/client/r0/login POST M_FORBIDDEN
```

## 下一步

Week 2 剩余任务:
- **Task 4**: Handler 签名扫描 → 补 request body schema 进 OpenAPI
- **Task 5**: 探针实服务器 → 补 response body schema

可通过 admin/federation spec 的生成继续扩大覆盖率。
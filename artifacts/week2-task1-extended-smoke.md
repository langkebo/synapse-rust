# Week 2 Task 1 — 扩展 schemathesis 覆盖 ✅ 完成

**日期**: 2026-09-01
**前置**: Week 1 Task 3 (5 端点基础冒烟测试)

---

## 产出

| 文件 | 说明 |
| --- | --- |
| `scripts/api_test/schemathesis_extended_test.py` | 扩展测试脚本 (自动发现 48 个 optional 端点) |
| `scripts/api_test/reports/schemathesis_extended.json` | 完整测试报告 (1440 个 case) |

## 关键改进

| 维度 | Week 1 Task 3 | **Week 2 Task 1** |
|---|---|---|
| 端点覆盖 | 5 (硬编码) | **48 (自动发现)** |
| case 总数 | 150 | **1440** (9.6x) |
| 通过率 | 5/5 (100%) | **48/48 (100%)** |
| 5xx server error | 0 | 0 |
| 报告大小 | 12 KB | ~40 KB |

## 自动发现逻辑

```python
def discover_optional_endpoints(spec_path):
    """从 OpenAPI spec 中发现 security=[] 的端点"""
    spec = yaml.safe_load(open(spec_path))
    targets = []
    for path, methods in spec["paths"].items():
        if "{" in path: continue  # 过滤 path params (proxy 异常)
        if "/unstable/" in path: continue  # 过滤不稳定 API
        for method, op in methods.items():
            if op.get("security", []) == []:  # empty security = optional
                targets.append((method.upper(), path))
    return sorted(set(targets))
```

## 测试结果详情

### Summary

```
discovered_endpoints: 48
tested_endpoints: 48
passed_endpoints: 48
failed_endpoints: 0
total_cases: 1440
total_2xx_3xx: 344 (24%)
total_4xx: 1096 (76%)
total_5xx: 0
total_network_exc: 0
```

### 端点分布

| 类型 | 数量 | 说明 |
|---|---|---|
| 30/30 2xx (公开 GET) | 8 | login, capabilities, publicRooms, versions |
| 30/30 4xx (拒绝无效输入) | 36 | 各种 captcha/email token/registration 类 |
| Mixed 2xx + 4xx | 4 | v3 register, publicRooms, register/guest — 探索边界条件 |

### 行为模式

**Pure 2xx (8 端点)** — 任何 case 都成功:
- `GET /_matrix/client/r0/capabilities`
- `GET /_matrix/client/r0/login`
- `GET /_matrix/client/r0/publicRooms`
- `GET /_matrix/client/v3/versions` 等

**Pure 4xx (36 端点)** — 任何 case 都拒绝:
- `POST /_matrix/client/r0/login` (无 body)
- `POST /_matrix/client/r0/account/password/email/requestToken` (无 email)
- `GET /_matrix/client/r0/login/saml/callback` (无 ticket)

**Mixed 4 端点** — schemathesis 探索到边界:
- `POST /_matrix/client/r0/publicRooms` (28 2xx / 2 4xx)
- `POST /_matrix/client/v3/publicRooms` (27 2xx / 3 4xx)
- `GET /_matrix/client/v3/register` (14 2xx / 16 4xx)
- `POST /_matrix/client/v3/register/guest` (5 2xx / 25 4xx)

## 验证

```bash
$ python3 scripts/api_test/schemathesis_extended_test.py --max-cases 30
# Exit 0
# 48/48 passed
# 0 5xx server errors
# 0 network exceptions
```

## 关键决策

- **Path params 过滤** — 含 `{...}` 的端点会触发 schemathesis 4.x 的 transport proxy 异常。Week 2 后续给 user-auth 端点加 mock token 解决
- **不稳定 API 过滤** — `/unstable/` 路径下的 endpoint 经常有 MSC 草稿 schema,易失败
- **30 cases/endpoint** — 平衡覆盖率与执行时间 (48 端点 × 30 = 1440 case, ~2 min)
- **失败定义** — 5xx = 失败, 4xx = 正确拒绝, Network = 警告

## 局限性

1. **48/50 optional 端点** — 2 个含 path params 的 optional 端点 (`.well-known/openid-configuration`, `.well-known/jwks.json`) 仍需特殊处理
2. **未测 user-auth 端点** — 全部 833 个 user/admin auth 端点暂未覆盖 (需要 mock token)
3. **未断言 4xx errcode** — 仅记录 4xx 数量,未分析 errcode 是否符合 Matrix 规范 (Week 2 后续)

## 后续计划 (Week 2 后续)

- [ ] Task 2: 给 user-auth 端点加 wiremock mock token,扩展到全部 833 端点
- [ ] Task 3: 分析 4xx 响应的 errcode,断言符合 Matrix 规范
- [ ] Task 4: Handler 签名扫描 → 补充 request body schema
- [ ] Task 5: Probe 活服务器 → 补充 response schema (非 Week 1 placeholder)
- [ ] CI 集成: `make extended-smoke` → `python3 schemathesis_extended_test.py`

## 经验沉淀

- **自动发现 vs 硬编码端点列表**: 自动发现 5x 易维护, schemathesis 4.x 的 `op.as_strategy()` 让端点过滤变得简单
- **失败定义要先于实施**: 明确"5xx = 失败, 4xx = 正确拒绝"避免误报。Mixed 端点是边界探测的正常现象
- **覆盖度是渐进过程**: Week 1 测 5, Week 2 Task 1 测 48, 后续加 mock 后测 833
- **报告 JSON 比 stdout 重要**: stdout 易被截断, JSON 报告可被 CI 解析与对比

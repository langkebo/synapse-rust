# Week 1 Task 1 — OpenAPI 规范生成 ✅ 完成

**日期**: 2026-09-01
**策略**: 策略 A — 零侵入 RouteLedger (不改任何 Rust 代码)

---

## 产出

| 文件 | 作用 |
| --- | --- |
| `docs/openapi/client.yaml` | OpenAPI 3.0.3 规范 (802 KB, 898 ops, 704 paths) |
| `docs/openapi/README.md` | 使用说明与维护指南 |
| `scripts/api_test/generate_openapi.py` | 生成器脚本 |
| `scripts/api_test/README.md` | 新增 Week 1 Task 1 章节 |

## 技术决策

**为何不选 utoipa (策略 C)**:
- utoipa 需要在 60+ handler 函数上添加 `#[utoipa::path]` 宏
- 需要在所有 DTO struct 上添加 `#[derive(ToSchema)]`
- 侵入大,风险高,与现有 RouteLedger 功能高度重叠

**为何选策略 A**:
- 项目已有 `synapse_ledger_export` binary → 输出 `ledger.json` (1292 条)
- `ledger.json` 已存在,无需 cargo 重新编译
- 生成器纯 Python,仅读取 JSON,零 Rust 代码改动
- 未来可无缝对接 schemathesis 契约测试

## 规范质量

| 指标 | 数值 |
| --- | --- |
| OpenAPI 版本 | 3.0.3 ✅ |
| Client-Server 端点 | 898 operations / 704 paths ✅ |
| 验证 | 所有必填字段存在 ✅ |
| 活服务器交叉验证 | 4/5 端点实测匹配 ✅ |
| Security Schemes | AccessToken + X-Matrix ✅ |
| Tags (按模块分组) | 47 个 ✅ |
| Path params 提取 | ✅ (从 ledger.path_params) |
| Query params 提取 | ✅ (从 ledger.query_params) |
| 响应 schema | ⏳ TODO placeholder |
| Auth 覆盖率 | ⚠️ 仅 1/898 有 auth 标记 |

## 已知局限 (Week 2 待补)

1. **Auth 字段稀疏**: ledger 的 `auth` 字段覆盖率不足 → Week 2 用 profile split 补充
2. **响应 schema 缺失**: 所有端点返回 `GenericResponse` placeholder → Week 2~3 用 schemathesis 探测
3. **Federation / Admin API**: 仅生成了 Client-Server → Week 2 补充

## 使用方式

```bash
# 刷新规范
python3 scripts/api_test/generate_openapi.py \
    --ledger scripts/api_test/ledger.json \
    --output docs/openapi/client.yaml

# 验证结构
python3 -c "import yaml; d=yaml.safe_load(open('docs/openapi/client.yaml')); print(d['openapi'], 'paths:', len(d['paths']))"
```

## CI 集成思路

```bash
# 每次 PR 自动 diff
git diff --stat docs/openapi/client.yaml
# 端点增加 >5% → 触发 API review 提醒
```

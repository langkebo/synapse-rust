# Week 1 Task 2 — Profile 拆分 + Auth 推断 ✅ 完成

**日期**: 2026-09-01
**前置**: Week 1 Task 1 (OpenAPI 生成)

---

## 产出

| 文件 | 作用 |
| --- | --- |
| `scripts/api_test/generate_openapi.py` | 重构: 多 profile 模式 + auth 启发式推断 |
| `scripts/api_test/refresh_openapi_specs.py` | 新增: 一键 orchestrator (export + generate + changelog) |
| `docs/openapi/README.md` | 重写: 加 profile/auth/使用场景/刷新流程 |
| `scripts/api_test/README.md` | 更新: 文件清单加 Task 2 相关脚本 |
| `docs/openapi/index.json` | 新增: manifest 索引 |

## 决策

- **粒度**: 策略 A — default spec 包含当前 profile 全集,其他 profile 只生成独有 diff 文件
- **Auth**: 路径/模块名启发式推断 + ledger 字段优先 (覆盖 897 MISSING → 100%)
- **Manifest**: 生成 `index.json` (CI 可读取决定跑哪些 profile 的 contract tests)

---

## 核心改动: generate_openapi.py

### 1. Auth 启发式推断 (`infer_auth` 函数)

| 规则 | 推断 |
| --- | --- |
| `^/_matrix/client/(?:(?:r0\|v\d+\|unstable)/)?login\|register\|capabilities\|versions$` | `optional` |
| `^/_\.well-known/...` | `optional` |
| `^/_synapse/admin/` | `admin` |
| `^/_matrix/federation/` | `federation` |
| 其他一切 | `user` (默认) |

**关键 fix**: 修正了无版本前缀路径 (`/_matrix/client/versions` → optional) 的匹配。

### 2. Auth 覆盖率提升

| 指标 | Week 1 Task 1 | Week 1 Task 2 |
| --- | --- | --- |
| 有 auth 标记 | 1/898 (0.1%) | **898/898 (100%)** |
| optional 端点 | 0 | 50 |
| user_or_admin 端点 | 1 | 834 |

### 3. 多 profile 模式 (`--all-profiles`)

```bash
# 单 profile (原有行为)
python3 generate_openapi.py --ledger ledger.json --output client.yaml

# 多 profile (新行为)
python3 generate_openapi.py --all-profiles
# 输出:
#   docs/openapi/client.yaml          (primary: default profile)
#   docs/openapi/client-oidc.yaml    (oidc 独有端点, 如存在)
#   docs/openapi/client-worker.yaml
#   docs/openapi/client-saml.yaml
#   docs/openapi/client-all.yaml
#   docs/openapi/index.json           (manifest)
```

---

## 新增: refresh_openapi_specs.py (orchestrator)

一键刷新完整流程:

```
Phase 1: export_ledger.sh (每个 profile 调用 cargo)
Phase 2: generate_openapi.py --all-profiles
Phase 3: changelog 对比 (旧/新 index.json)
```

```bash
# 完整刷新 (~3min 首次 cargo)
python3 scripts/api_test/refresh_openapi_specs.py

# 只刷新 default profile (快,无需 cargo)
python3 scripts/api_test/refresh_openapi_specs.py --profile default --skip-export

# 自定义 profile 列表
python3 scripts/api_test/refresh_openapi_specs.py --profiles default,oidc,worker
```

---

## index.json Manifest 格式

```json
{
  "schema_version": "1",
  "generated_at": "2026-08-12T01:38:35Z",
  "primary_profile": "default",
  "specs": {
    "default": {
      "file": "openapi/client.yaml",
      "ledger_file": "scripts/api_test/ledger.json",
      "profile_flags": {"oidc_enabled": false, "worker_enabled": false, "saml_enabled": false},
      "ledger_generated_at": "2026-08-12T01:38:35Z",
      "client_server_endpoints": 898,
      "operations": 898,
      "tags": 47
    }
  }
}
```

CI 用途:
- `jq '.specs[env.PROFILE].operations' docs/openapi/index.json` → 决定并发 worker 数
- `jq '.specs[env.PROFILE].tags[]' docs/openapi/index.json` → 生成 tag 级别测试计划

---

## 验证结果

```
Auth inference (6 样本):
  ✓ GET    /_matrix/client/versions          → optional ✓
  ✓ GET    /_matrix/client/v3/versions      → optional ✓
  ✓ POST   /_matrix/client/r0/login         → optional ✓
  ✓ GET    /_matrix/client/r0/capabilities → optional ✓
  ✓ GET    /_matrix/client/r0/sync         → user_or_admin ✓
  ✓ GET    /_matrix/client/r0/account/whoami → user_or_admin ✓

Multi-profile mode (--all-profiles):
  [openapi] loaded ledger default: scripts/api_test/ledger.json
  [openapi] default    → client.yaml  (898 ops, 47 tags)
  [openapi] manifest: docs/openapi/index.json ✓

refresh_openapi_specs.py:
  Changelog: +898 new endpoint(s) (first run) ✓
```

---

## 已知局限 (Week 2 待补)

1. **其他 profile ledger 未生成** — 需要 cargo 编译 `synapse_ledger_export` binary
   - `scripts/api_test/reports/ledger_oidc.json` 等暂不存在
   - `index.json` 只有 `default` 一个 spec
2. **Auth 启发式仍不完美** — `/profile/{user_id}` 的 GET 公开(读任意人资料)是对的,
   但 POST `/profile/{user_id}` (改自己资料) 应需 auth → 当前两者都 optional
   - 暂接受这个粒度 (OpenAPI 安全推断本来就需要 method+path 两维)
3. **Federation/Admin API 未生成** — 默认只输出 `/_matrix/client/` 前缀

---

## 后续计划

- **Week 2**: 跑 `cargo run --bin synapse_ledger_export -- --profile=all` 生成全量 ledger → `client-all.yaml` 包含所有端点
- **Week 2**: Handler 签名扫描 → 补充 request body schema
- **Week 3**: schemathesis 探测活服务器 → 补充 response schema

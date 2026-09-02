# OpenAPI 规范

本目录包含 synapse-rust Matrix Homeserver 的 OpenAPI 3.0.3 规范。

## 文件

| 文件 | 说明 |
| --- | --- |
| `client.yaml` | **主 spec** — 当前 profile (default) 的完整 Client-Server API |
| `index.json` | Manifest 索引 — 所有 profile 的 spec 路径 + 元数据 |

## client.yaml 概况 (default profile)

- **端点**: 898 operations / 704 paths
- **覆盖**: 仅 `/_matrix/client/` 前缀 (Client-Server API)
- **版本**: synapse-rust 6.2.0, 来自 ledger schema_version=1
- **生成方式**: `scripts/api_test/generate_openapi.py`
- **Auth 覆盖率**: **100%** (启发式推断 + ledger 字段优先)
  - `optional`: 50 端点 (login, register, capabilities, versions 等公开端点)
  - `user_or_admin`: 834 端点
  - `federation`: 0 端点 (default profile 不含 federation 端点)

## 刷新流程 (CI / 本地开发)

```bash
# 方式 1: 一键刷新 (需要 cargo, ~3min 首次编译)
python3 scripts/api_test/refresh_openapi_specs.py

# 方式 2: 只刷新 default profile (快,无需 cargo)
python3 scripts/api_test/refresh_openapi_specs.py --profile default --skip-export

# 方式 3: 手动分步
bash scripts/api_test/export_ledger.sh --profile=default --output=scripts/api_test/ledger.json
python3 scripts/api_test/generate_openapi.py --all-profiles
```

## index.json Manifest

```json
{
  "schema_version": "1",
  "generated_at": "...",
  "primary_profile": "default",
  "specs": {
    "default": {
      "file": "openapi/client.yaml",
      "ledger_file": "scripts/api_test/ledger.json",
      "profile_flags": {"oidc_enabled": false, "worker_enabled": false, "saml_enabled": false},
      "client_server_endpoints": 898,
      "operations": 898,
      "tags": 47
    }
  }
}
```

## Profile 说明

| Profile | 说明 | 独有端点 |
| --- | --- | --- |
| `default` | 最小 profile (oidc=off, worker=off, saml=off) | 基础端点 |
| `oidc` | 启用 OIDC SSO | OIDC 认证回调端点 |
| `worker` | 启用 Worker 模式 | Worker 通信端点 |
| `saml` | 启用 SAML SSO | SAML 回调端点 |
| `all` | 所有 feature 全开 | 全部端点 |

> ⚠️ 其他 profile 需要先跑 `cargo run --bin synapse_ledger_export -- --profile=<name>` 导出对应 ledger JSON。

## Auth 推断策略 (Week 1 Task 2)

`ledger.json` 中仅 1/898 端点有 `auth` 字段标记。
其余 897 个通过 **启发式规则** 自动补全:

| 规则 | 推断结果 |
| --- | --- |
| 路径含 `/.well-known/` | `optional` |
| 路径含 `/login`, `/register`, `/capabilities`, `/versions` | `optional` |
| 路径含 `/publicRooms`, `/profile/{user_id}` | `optional` |
| 路径含 `/_synapse/admin/` | `admin` |
| 路径含 `/_matrix/federation/` | `federation` |
| 其他一切 | `user` (默认) |

ledger `auth` 字段 > 启发式推断,已标记的端点以 ledger 为准。

## 当前覆盖度

| 维度 | 状态 | 负责人 |
| --- | --- | --- |
| Endpoint metadata (path/method/auth/rate-limit) | ✅ 898/898 | Week 1 Task 1+2 |
| Path parameters (user_id, room_id 等) | ✅ 来自 ledger.path_params | Week 1 Task 1 |
| Query parameters | ✅ 来自 ledger.query_params | Week 1 Task 1 |
| Auth field (897 MISSING → 启发式补全) | ✅ **100% 覆盖** | Week 1 Task 2 |
| Multi-profile splitting (oidc/worker/saml/all) | ✅ 接口就绪 | Week 1 Task 2 |
| Request body schemas | ⏳ TODO | Week 2 |
| Response body schemas | ⏳ TODO | Week 2~3 |
| Federation API (/\_matrix/federation/) | ⏳ TODO | Week 2 |
| Admin API (/\_synapse/admin/) | ⏳ TODO | Week 2 |

## 使用场景

### 1. Swagger UI 本地预览

```bash
cd docs/openapi && python3 -m http.server 8080
# 访问 http://localhost:8080/client.yaml
```

### 2. schemathesis 契约测试 (Week 3)

```bash
pip install schemathesis
schemathesis run http://localhost:8008/_matrix/client/versions \
    --base-url=http://localhost:8008 \
    docs/openapi/client.yaml
```

### 3. CI 自动化

```bash
# PR 时 diff 检查
python3 scripts/api_test/refresh_openapi_specs.py --profile default --skip-export
git diff --stat docs/openapi/
```

## 生成策略说明

采用 **策略 A (零侵入 RouteLedger)**:

```
RouteLedger (Rust) → synapse_ledger_export binary → ledger.json (JSON)
                                                     ↓
                           generate_openapi.py        →  client.yaml (OpenAPI 3.0.3)
                           refresh_openapi_specs.py  →  client-{oidc,worker,saml,all}.yaml + index.json
```

- **零 Rust 代码改动** — 不需要 `utoipa` 或在 handler 上加宏
- **依赖现有基础设施** — `synapse_ledger_export` 和 `ledger.json` 已存在
- **增量式完善** — metadata 先出, schema 后续迭代补充

参考: `python3 scripts/api_test/generate_openapi.py --help`

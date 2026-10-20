# OpenAPI 规范

本目录包含 synapse-rust Matrix Homeserver 的 OpenAPI 3.0.3 规范。

## 文件

| 文件 | 说明 |
| --- | --- |
| `client.yaml` | **主 spec（生成物，不入库）** — 当前 profile (default) 的完整 Client-Server API。由 `scripts/api_test/gen_client_yaml.py` 生成，CI 作 artifact 上传（见 `.github/workflows/ci.yml` 的 `openapi-artifact` job）；本地不入库、按需重生成 |
| `index.json` | Manifest 索引 — 所有 profile 的 spec 路径 + 元数据 |
| `route-table.json` | 路由表（入库，CI 对账 drift）— golden lane × default profile |
| `route-table-sdk.json` | 路由表超集（入库，对账 drift）— sdk lane × all profile，含全部 feature-gated 路由 |

## 权威口径与 (lane, profile) 坐标轴

> 后端路由面只有**一个机器权威**，其余产物都是它在某个坐标上的视图（BE-1 治理）。

- **机器权威**：`synapse_ledger_export` 二进制导出的 ledger JSON。其 `schema_version`
  由 [`synapse-web/src/routes/ledger_export.rs`](../../synapse-web/src/routes/ledger_export.rs)
  的 `SCHEMA_VERSION` 定义（当前 `4`），受 `ledger_export_tests::schema_doc_version_matches_code`
  与 6 份 fixture 双重守护。
- **人类可读视图**：`docs/synapse-rust/ROUTE_CONTRACT.md`（`extract_registered.py` 静态解析
  `synapse-web/src/routes/**` 的 `.route()/.nest()` 真实注册面，取 **union/all-feature** lane）。
- **两个正交维度**，坐标 = `(lane, profile)`：

  | 维度 | 取值 | 定义 |
  | --- | --- | --- |
  | lane（编译期 `#[cfg(feature = "…")]`） | `golden` / `sdk` | golden = 默认特性闭包；sdk = `default + all-extensions`。差值不是单一开关，而是整组 `all-extensions` 编译进来的门控路由：sdk 相对 golden 在 default profile 多 61 条（`saml-sso` 16 / `server-notifications` 15 / `cas-sso` 12 / `voice-extended` 13 / `voip-tracking` 5），在 all profile 多 62 条 |
  | profile（运行期 guard） | `default ⊂ worker ⊂ all` | 由 `ProfileFlags` 的 `worker_enabled`/`oidc_enabled`/`saml_enabled` 决定，派生表按 `RouteProfile` 单调过滤（按 `(method,path)`；高 rank 的碰撞孪生会**替换**低 rank 条目，不新增 path） |

- **权威计数** = sdk lane × all profile = **1048**（最大完整面，SDK 实际摄入的 lane；
  等于 `tests/unit/fixtures/ledger_export_sdk/all.json` 的 `entry_count`，也等于
  `ROUTE_CONTRACT.md` 的总览计数）。其余数字都是该坐标的**子集**，不是并列权威。
- 6 个合法坐标的 fixture 落在 `tests/unit/fixtures/ledger_export{,_sdk}/{default,worker,all}.json`。
- `docs/openapi/route-table.json` 是 **golden lane × default profile** 的扁平视图；其头部
  `lane` / `profile` / `profile_flags` / `ledger_schema_version` **自描述**该坐标与来源 ledger
  的 schema 版本（`schema_version` 字段则是本产物**自身**的输出 shape 版本，与 ledger 的
  `schema_version` 无关）。该坐标一致性由
  [`scripts/contract/check_route_surface_reconciliation.py`](../../scripts/contract/check_route_surface_reconciliation.py)
  对账，**禁止手改**。
- `docs/openapi/route-table-sdk.json` 是 **sdk lane × all profile** 的同形扁平视图，即
  **最大完整面**（D-15）。它以 `tests/unit/fixtures/ledger_export_sdk/all.json` 为源生成，
  由同一 reconciler 的 R6 逐条对账。**它以 `route-table.json` 为宇宙的消费者必须改读这个
  文件**：`route-table.json` 来自默认特性导出，按定义不可能包含 `#[cfg(feature = "…")]`
  路由族（`/voice/*`、SAML/CAS 回调、`server-notifications` 等），把 golden 视图当全集会把
  整族"已服务"路由误判为 `route-not-resolved`。

```bash
# 刷新两个视图（golden 需要 fresh default-feature 导出；sdk 直接取已入库的 sdk×all fixture）
python3 scripts/api_test/gen_route_table.py --ledger "$RUNNER_TEMP/ledger_default_ci.json"
python3 scripts/api_test/gen_route_table.py \
    --ledger tests/unit/fixtures/ledger_export_sdk/all.json \
    --output docs/openapi/route-table-sdk.json
python3 scripts/contract/check_route_surface_reconciliation.py
```

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
bash scripts/api_test/export_ledger.sh --profile=default   # 输出 reports/ledger_default.json（gitignore）
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
      "ledger_file": "scripts/api_test/reports/ledger_default.json",
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

导出的 ledger JSON（落点 `scripts/api_test/reports/ledger_{profile}.json`，gitignored；
**不再有任何提交进仓库的默认清单**，见 `docs/前缀命名空间治理方案-2026-10-08.md` §13.35）
中仅 1 个端点带有 `auth` 字段标记。
其余 1095 个通过 **启发式规则** 自动补全:

| 规则 | 推断结果 |
| --- | --- |
| 路径含 `/.well-known/` | `optional` |
| 路径含 `/login`, `/register`, `/capabilities`, `/versions` | `optional` |
| 路径含 `/publicRooms`, `/profile/{user_id}` | `optional` |
| 路径含 `/_synapse/admin/` | `admin` |
| 路径含 `/_matrix/federation/` | `federation` |
| 其他一切 | `user` (默认) |

ledger `auth` 字段 > 启发式推断,已标记的端点以 ledger 为准。

> ⚠️ **两个计数口径不可混用**：
> - 导出的 ledger 全量（含所有前缀）—— 下面两个数字是 **2026-08 的快照**，随 feature/路由增删而变，
>   请以 `python3 -c "import json;print(json.load(open('scripts/api_test/reports/ledger_default.json'))['entry_count'])"` 为准。
> - `client.yaml` default profile = **898** operations（仅 `/_matrix/client/`，是 ledger 全量的客户端子集）。
>
> 下方覆盖度表按 `client.yaml`（898）口径统计。

## 当前覆盖度

| 维度 | 状态 | 负责人 |
| --- | --- | --- |
| Endpoint metadata (path/method/auth/rate-limit) | ✅ 898/898（client.yaml default profile） | Week 1 Task 1+2 |
| Path parameters (user_id, room_id 等) | ✅ 来自 ledger.path_params | Week 1 Task 1 |
| Query parameters | ✅ 来自 ledger.query_params | Week 1 Task 1 |
| Auth field (client.yaml 内 897 MISSING → 启发式补全) | ✅ **100% 覆盖** | Week 1 Task 2 |
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

`client.yaml` 已移出索引（见 `.gitignore`）：CI 只生成并上传为 artifact，不再对账入库副本；
入库并受 CI drift 对账的只有 `route-table.json`。

```bash
# 本地刷新后检查 route-table.json 的 diff
python3 scripts/api_test/refresh_openapi_specs.py --profile default --skip-export
git diff --stat docs/openapi/route-table.json
```

## 生成策略说明

采用 **策略 A (零侵入 RouteLedger)**:

```
RouteLedger (Rust) → synapse_ledger_export binary → reports/ledger_{profile}.json (JSON)
                                                     ↓
                           generate_openapi.py        →  client.yaml (OpenAPI 3.0.3)
                           refresh_openapi_specs.py  →  client-{oidc,worker,saml,all}.yaml + index.json
```

- **零 Rust 代码改动** — 不需要 `utoipa` 或在 handler 上加宏
- **依赖现有基础设施** — `synapse_ledger_export` 已存在（清单每次现导出，不再依赖仓库里的 JSON）
- **增量式完善** — metadata 先出, schema 后续迭代补充

参考: `python3 scripts/api_test/generate_openapi.py --help`

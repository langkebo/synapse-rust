# Ledger 导出契约 schema

> **权威版本常量**：`src/web/routes/ledger_export.rs` 的 `SCHEMA_VERSION`。
> 本文件顶部的版本号由 `tests/unit/ledger_export_tests.rs` 的
> `schema_doc_version_matches_code` 与代码断言绑定 —— 改代码必须同步改本文件，
> 否则该测试变红。这是为了让"契约版本变更"无法像
> `aa06ca45` 那次一样静默溜过。

- **当前 `schema_version`**: `2`
- **生成者**: `synapse_ledger_export` 二进制（`build_artifact` + `render`）
- **消费方**: `matrix-js-sdk` 的 `scripts/contract-sync.mjs`
  （其 `LEDGER_SCHEMA_VERSION` pin 必须等于本文件的 `schema_version`）
- **CI 发布方**: `.github/workflows/ledger-export.yml`
  （`--features all-extensions`，artifact 名 `ledger-export-<sha>`，
  并以 `repository_dispatch: synapse-ledger-expor` 通知 SDK；
  SDK 侧监听 workflow 为 `.github/workflows/synapse-ledger-sync.yaml`）

## 顶层字段

| 字段 | 类型 | 说明 |
|---|---|---|
| `schema_version` | string | 本契约版本，见上 |
| `state_profile` | string | `default` / `worker` / `all`，必须与请求的 profile 一致 |
| `generated_at` | string | RFC3339 时间戳。金文件用固定值以保证可 diff |
| `synapse_rust_commit` | string | 生成时的后端 commit。金文件用 40 个 0 |
| `profile_flags` | object | 该 profile 启用的 feature 标志 |
| `entry_count` | number | `entries` 长度，冗余字段便于快速校验 |
| `entries` | array | 路由条目，见下 |

## `entries[]` 字段

| 字段 | 类型 | 自哪个版本 | 说明 |
|---|---|---|---|
| `method` | string | 1 | HTTP 方法 |
| `path` | string | 1 | 路由路径（含 `{param}` 占位） |
| `path_params` | array | 1 | 路径参数名 |
| `registered_by` | string | 1 | 注册该路由的模块名。**SDK 目前按此字段聚合功能域** |
| `query_params` | array | **2** | 该路由识别的查询参数 |
| `module` | string | 2（可选） | 功能域。**默认等于 `registered_by`，仅在跨域注册时才显式覆盖** |
| `status` | object | 2（可选） | 生命周期。缺省语义为 `Stable` |

### 关于 `module` / `status` 的诚实说明

- 二者都是**非默认才输出**的可选字段。实测在 1320 条路由里，`module` 只填了 3 条、
  `status` 只填了 7 条。
- `module` 的默认值是 `registered_by`（见 `route_ledger.rs` 的 `RouteEntry::new`），
  全仓 96 个 `RouteEntry::new` 调用点里只有 3 个用 `with_module` 覆盖。
  因此**当前 `module` 对绝大多数路由不携带新信息**。
- SDK 的分模块聚合用的是 `registered_by`，**不消费 `module`**（见
  `contract-sync.mjs` 的 `moduleKeyFor(registeredBy)`）。
- 结论：若要让 `module` 真正成为"按功能域聚合"的依据，需要让它必填 + 取值白名单
  校验；否则应删除以免造成"字段有、信息没有"的假象。这条待决策。

## 版本演进

| 版本 | 变更 | 兼容性 |
|---|---|---|
| 1 → 2 | `entries[]` 新增 `query_params`；`module` / `status` 变为可选字段 | **加法升级**，顶层字段不变，旧消费方可忽略新字段 |

**升级 checklist**（本次 `aa06ca45` 漏掉了第 2、4 步，导致 SDK 同步链断裂）：

1. 改 `src/web/routes/ledger_export.rs` 的 `SCHEMA_VERSION`
2. 同步本文件的 `schema_version`
3. 重生成两条车道的 fixture：
   - `tests/unit/fixtures/ledger_export/`（金文件车道，**默认 feature**，
     固定 timestamp/commit，须与 `ledger_export_tests.rs` 里的常量一致）
   - `tests/unit/fixtures/ledger_export_sdk/`（SDK 车道，**all-extensions**，
     跑 `scripts/generate_sdk_ledger_fixtures.sh`）
4. 通知/更新下游：`matrix-js-sdk` 的 `contract-sync.mjs` 里
   `LEDGER_SCHEMA_VERSION` 必须跟到新版本，并重生成 SDK 镜像

## 两条 fixture 车道（不要混淆）

同名文件、不同 feature 集，条数不同：

| 目录 | feature | `all` profile 条数 | 谁在用 |
|---|---|---|---|
| `tests/unit/fixtures/ledger_export/` | 默认（无扩展） | 1320 | `tests/unit/ledger_export_tests.rs` 的金文件比对；该测试有 `cfg(not(any(feature = "all-extensions", ...)))` 门控 |
| `tests/unit/fixtures/ledger_export_sdk/` | `all-extensions` | 1407 | SDK 的 `contract-sync.mjs` 默认 ingest 源 |

**这 87 条的差集不是漂移，是 feature 集差异**（SAML 16 / voice 29 / RTC 10 / 其余 32）。
把它误当成"SDK 领先后端"会得出错误结论。

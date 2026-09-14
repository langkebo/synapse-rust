# Ledger 导出契约 schema

> **权威版本常量**：`src/web/routes/ledger_export.rs` 的 `SCHEMA_VERSION`。
> 本文件顶部的版本号由 `tests/unit/ledger_export_tests.rs` 的
> `schema_doc_version_matches_code` 与代码断言绑定 —— 改代码必须同步改本文件，
> 否则该测试变红。这是为了让"契约版本变更"无法像
> `aa06ca45` 那次一样静默溜过。

- **当前 `schema_version`**: `4`
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
| `auth` | string | 1（可选） | 认证要求：`user` / `admin` / `optional` / `federation` / `none` |

> `module` 字段在**版本 3 已删除**、`status` 字段在**版本 4 已删除** —— 见下节。

### 为什么删掉了 `module`（B-7）

- 它的默认值是 `registered_by`（`RouteEntry::new`），全仓 96 个调用点里只有
  3 个用 `with_module` 覆盖 → 导出里 1320 条只有 3 条填了不同的值。
- **没有任何消费方**：SDK 的分模块聚合用的是 `registered_by`
  （`contract-sync.mjs` 的 `moduleKeyFor(registeredBy)`），不读 `module`。
- 结论：这是一个"字段有了、信息没增加"的假绿。与其保留，不如删除；若将来真要
  按功能域聚合，应让它**必填 + 取值白名单校验**，而不是可选且默认复制。
- 影响面：`module` 是可选字段且只有 3 条填充，删除它对读取方的解析无影响
  （读取方本就不读它）。

### 为什么删掉了 `status`（B-7 连带）

- `status`（序列化为 `LedgerEntryStatusJson`）唯一写入点是
  `push_notification.rs` 里 7 条 legacy `/r0/push/*` 路由的 `with_status(...)`，
  且无 `sunset_at`。
- 该调用点已移除（这些 legacy 路由与 spec-compliant `pushers`/`pushrules`
  重叠且 SDK 零调用，保留但不再标注生命周期）。
- 移除后 `with_status` / `RouteStatus` / `LedgerEntryStatusJson` **全仓零消费方**，
  SDK 也从不读 `status` → 与 `module` 同理，删除机制本身。
- 影响面：`status` 一直是可选字段（`Stable` 时键省略），删除对读取方无影响。

## 版本演进

| 版本 | 变更 | 兼容性 |
|---|---|---|
| 1 → 2 | `entries[]` 新增 `query_params`；`module` / `status` 变为可选字段 | **加法升级**，顶层字段不变 |
| 2 → 3 | **删除** `entries[].module` | **移除可选且无消费方的字段**；顶层字段与其余 entry 字段不变 |
| 3 → 4 | **删除** `entries[].status` | 同上；`status` 自唯一调用点移除后也无消费方，连带删除机制 |

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

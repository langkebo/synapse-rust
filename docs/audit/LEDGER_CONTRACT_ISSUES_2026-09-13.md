# Ledger 契约链问题清单（B 系列 + 跨仓同步）

日期：2026-09-13
状态：**本轮已修的标 ✅，待决策的标 ⏳**
关联提交（后端 main / SDK `feat/sdk-contract-gap-implementation`）：
`9caaeb026`（SDK）、`e70ce0f2`、`8414f54d`（后端）

---

## 一、根因：为什么 SDK 的契约镜像会停在 Sep 13

不是"通知送不到"。SDK 侧监听 workflow **存在且完整**：

```
matrix-js-sdk/.github/workflows/synapse-ledger-sync.yaml
  on: repository_dispatch: types: [synapse-ledger-export]
  下载 artifact → contract-sync.mjs --source=<temp> --render-drafts
              → contract-sync.mjs --check --source=<temp> → 提 PR
```

真实断点是**schema 版本不匹配**：

| 侧 | 值 | 位置 |
|---|---|---|
| 后端 | `SCHEMA_VERSION = "2"` | `src/web/routes/ledger_export.rs:22` |
| SDK pin | `LEDGER_SCHEMA_VERSION = "1"` | `matrix-js-sdk/scripts/contract-sync.mjs:71` |

SDK 的 ingest 是**严格相等**校验，不等即 `throw`。于是后端 CI 发布的 schema 2
artifact 在同步 workflow 里**解析即失败**，镜像自 `f6029a81` 后再未更新。

**责任链更正**：schema 1→2 的升级发生在 `aa06ca45`（该提交的内容由另一位 agent
产出、经本会话代为提交并标注了非本人所写）。我当时只验证了"可编译"，**没有验证
它改了对外契约版本**——这正是之后写进 `AGENTS.md` 第 8 条要防的场景。

---

## 二、已修（本轮）

| # | 问题 | 修法 | 证据 |
|---|---|---|---|
| ✅ 1 | SDK pin 落后 → 同步链断裂 | `LEDGER_SCHEMA_VERSION` `"1"`→`"2"`，补注释说明它是**输入**契约版本、与输出镜像版本 `GENERATED_SCHEMA_VERSION` 的区别 | SDK `--check` 通过（50 modules / 1381 default / 46 doc pages） |
| ✅ 2 | SDK 镜像停在 schema 1 | 用 all-extensions 产物重生成 55 个 generated 文件 + 48 个文档 frontmatter 版本 + 46 个 `generated_hash` | `contract-sync.mjs --check` 通过 |
| ✅ 3 | SDK 车道 fixture 落后（schema 1、实时值） | 重生成 `tests/unit/fixtures/ledger_export_sdk/*` 为 schema 2、固定 timestamp/commit | 1381/1392/1407，与 all-extensions 产物一致 |
| ✅ 4 | 两条 fixture 车道的 feature 约定无人记录，导致"SDK 多 87 条"被误读为漂移 | `scripts/generate_sdk_ledger_fixtures.sh` 头注释写明：金文件车道=默认 feature（all 档 1320）、SDK 车道=all-extensions（all 档 1407），并固定默认值 | 源码注释 |
| ✅ 5 | `docs/synapse-rust/LEDGER_EXPORT_SCHEMA.md` **被 3 处代码引用却不存在** | 新建该文档：字段表、1→2 升级说明、**升级 checklist（含同步下游 pin）**、两条车道对照 | 文件已创建 |
| ✅ 6 | 契约版本变更无守护（金文件测不出跨仓漂移） | 新增 `schema_doc_version_matches_code`：断言文档版本 == `SCHEMA_VERSION`，失败信息列出 4 步 checklist | **双向验证**：文档改 99 → 变红；恢复 → 绿 |
| ✅ 7 | 两处"frozen at = 1"的矛盾注释（同文件常量已是 2） | `src/web/routes/ledger_export.rs:11`、`src/bin/synapse_ledger_export.rs:15` 改为指向权威常量、不重复版本号 | 编译 + 测试通过 |

---

## 三、待决策

### ⏳ B-7 `module` 字段是假绿（优先级最高）

**事实**（实测）：

- `RouteEntry::new` 调用点 **96** 处，`with_module` 覆盖仅 **3** 处 → 96.9% 的
  路由 `module` 是 `registered_by` 的副本
- 导出里 `module` 只填了 **3/1320**、`status` 只填了 **7/1320**
- SDK 的分模块聚合用的是 **`registered_by`**（`contract-sync.mjs` 的
  `moduleKeyFor(registeredBy)`），**根本不读 `module`**

**结论**：`module` 目前不携带新信息，且没有消费方。

**三个选项**：

1. **删除 `module`**（推荐，最符合"不要冗余"）：SDK 反正用 `registered_by`
2. **必填 + 取值白名单校验**：需补 93 处调用点，并在 CI 校验合法性
3. 保持现状但在 schema 文档标注其局限（已在本轮新建的文档里如实写明）

### ⏳ B-3 / B-4 生命周期标注

`with_status` 全仓只有 **1** 处调用（`push_notification.rs:384`），且
`sunset_at: None`。若确实要标 Deprecated，应带日期，并有一条**消费它的**
tracker 门禁；否则标注只是装饰。

### ⏳ B-6 `ROUTE_CONTRACT.md` 漂移

该文档仍手工维护。`scripts/contract/gen_contract_doc.py` 已有雏形（输入是
`artifacts/registered_routes.json`）。建议：文档改为**自动生成** + CI 断言
"重新生成后无 diff"。

### ⏳ B-10 / B-11 剩余条目

共同根因：manifest 与 router 注册分两张表。根治方式是由一侧派生另一侧。

### ⏳ CI 覆盖率步骤的 `--skip ledger_export_tests`（保留，但需知情）

该 skip **是必要的**：覆盖率步骤用 `COV_FEATURES`（含 `voice-extended`/`saml-sso`/
`cas-sso` 等扩展），而金文件测试有
`#[cfg(not(any(feature = "all-extensions", ...)))]` 门控——扩展开启时这些测试
**根本不会编译**，所以 skip 只是防御性写法，不是"藏测试"。

金文件测试**在 CI 里是跑的**：`ci.yml:337` 的
`cargo nextest run --test unit --features test-utils`（无扩展）正好命中门控。

**真正缺的是跨仓守护**——已由本轮 ✅6 的版本守卫补上"代码↔文档"那一环；
"文档↔SDK pin"那一环仍需人工（或未来加一条可读到 SDK 仓库的检查）。

---

## 四、复现命令

```bash
# 后端：金文件契约测试（默认 feature，命中门控）
cargo nextest run --test unit --features test-utils -E 'test(/ledger_export/)'

# 后端：重生成两条车道的 fixture
#   金文件车道（默认 feature，固定值须与 ledger_export_tests.rs 常量一致）
cargo run --bin synapse_ledger_export -- --profile=all \
  --timestamp=2026-05-02T00:00:00Z \
  --commit=0000000000000000000000000000000000000000 \
  --output=tests/unit/fixtures/ledger_export/all.json
#   SDK 车道（all-extensions）
bash scripts/generate_sdk_ledger_fixtures.sh

# 跨仓：SDK 侧校验（需先把 CI artifact 放到某目录）
cd ../matrix-js-sdk
node scripts/contract-sync.mjs --source=<artifact-dir> --check

# 当前 schema 版本（唯一权威）
grep -n 'SCHEMA_VERSION' ../synapse-rust/src/web/routes/ledger_export.rs
```

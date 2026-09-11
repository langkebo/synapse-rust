# P2 — Matrix 协议契约一致性

> **审查日期**: 2026-09-11
> **基线**: `78056944`，工作树对本次审查干净
> **范围**: versions/capabilities 诚实性 · 路由三方对账 · 错误语义 · 联邦安全规则
> **方法**: 先验证"既有的契约测试到底覆盖了什么"，再找未被覆盖的真实缺口

---

## 0. 结论摘要

| 子项 | 状态 | 依据 |
|---|---|---|
| 路由三方对账 | ✅ 已有强测试覆盖 | manifest 1381 条 = 快照自述 `count: 1381`；`api_route_ledger_tests` 12 项 |
| capabilities 声明诚实性 | ✅ **治理机制与测试都很扎实** | 14 项治理测试全通过，含双 surface snapshot |
| **房间版本声明** | 🟡 **发现过度声明风险，需用户决策** | 见 §2 |
| 错误语义 | ⏳ 仅完成规模侦察（112 个 M_* 码），**未系统核对** | 见 §4 |
| 联邦安全规则 | ⏳ **未开展** | 见 §4 |

> **P2 的诚实结论：本阶段尚未完成。** 本轮确立了"既有覆盖比预期强"这一事实，
> 并定位到一个具体的过度声明风险；错误语义与联邦安全规则两项**未系统审查**。

---

## 1. 路由三方对账 —— ✅ 已有强覆盖

| 事实 | 值 |
|---|---|
| `route_ledger_default.snapshot` | 1384 行，自述 `count: 1381`，实际条目 1381（一致） |
| `*_route_manifest()` 函数 | 72 个 |
| 治理测试 | `api_route_ledger_tests`（12 项），含 `declared_route_manifest_entries_are_actually_wired`（PATCH 探测每条声明路由确实存在） |

**三方**（manifest ↔ 实际装配 ↔ 快照）中：
- manifest ↔ 实际装配：由 `declared_route_manifest_entries_are_actually_wired` 覆盖
- manifest ↔ 快照：由 `declared_route_manifest_full_snapshot_matches_{default,worker_enabled}_state` 覆盖

> ✅ 且这 12 项在 `--all-features` 下实测全通过（P0 复核已证）。

---

## 2. 🟡 房间版本声明过度 —— 需用户决策

### 2.1 事实

`synapse-common/src/room_versions.rs:79-99` 的 `SUPPORTED_ROOM_VERSIONS` 表把
**v1 到 v13 全部**标记为 `RoomVersionCapability::stable(...)`，
并由 `client_room_versions_capability()` 全部声明为 `"stable"`。

代码注释（第 90-95 行）说明了 v11+ 被标为可创建的依据：

```
// v11+ use the MSC2174/MSC3820 redaction format (content.redacts) and
// allow self-redaction by the original author.  Both behaviours are now
// implemented in synapse-common::redaction (...) and in
// auth::power_levels::can_redact_event (...), so these versions
// can be advertised as creatable.
```

即依据是**本项目自测的两个行为已实现**，而**不是**上游已将这些版本稳定化。

### 2.2 与外部基线的对比

| 来源 | 声明范围 |
|---|---|
| 本项目 | v1–v13，**全部 `stable`** |
| Element Synapse v1.156.0（AGENTS.md 记载的基线） | v1–v12 |
| Matrix spec v1.18（AGENTS.md 记载） | v11 已稳定；**v12/v13 的稳定性状态未在本仓核实** |

> ⚠️ 我**未能**在本次审查中核实 Matrix spec 对 v12/v13 的官方稳定性状态
> （无网络检索）；因此这里只陈述"与 Synapse 基线不一致"这一**可核实事实**，
> 不主张"v12/v13 一定不该声明"。

### 2.3 关键风险：stable 一词的语义后果

在 Matrix 中，`available[ver] = "stable"` 是告诉客户端
"可以安全创建该版本房间"。若 v12/v13 在上游仍是 unstable：

- 客户端（如 Element Web）可能据此创建 v12/v13 房间
- 对端服务器（Synapse 等）可能拒绝加入 ⇒ **互通性故障**
- 而项目自测覆盖的只是 redaction/self-redact 两个行为，
  **未覆盖** create/join/upgrade/state-resolution 的全链路

### 2.4 测试覆盖现状（缺口）

`room_versions.rs` 的测试仅断言：

```rust
assert_eq!(resolve_room_version(Some("12")), Some("12"));
assert_eq!(resolve_room_version(Some("13")), Some("13"));
```

即**只测"能解析"**，**没有任何测试**验证 v12/v13 的
create/join/upgrade/redaction/auth 行为与声明一致。

> 这与 AGENTS.md 的明文要求冲突：
> > "Room-version capability must match actual event/auth behavior.
> >  Do not add a room version to `m.room_versions` until
> >  create/join/upgrade/redaction/state-resolution behavior is reviewed."

### 2.5 建议（**未擅自实施**）

| 选项 | 说明 |
|---|---|
| **A** | 核实 Matrix spec 对 v12/v13 的官方状态；若仍 unstable，改为 `unstable` 声明（或移出 `available`） |
| **B** | 保持 `stable`，但补齐 v12/v13 的 create/join/upgrade/state-resolution 行为测试，使声明有证据支撑 |
| **C** | 维持现状并显式记录该决策（接受与 Synapse 基线的差异） |

> 这是**协议级声明变更**，会影响客户端互操作，故未擅自改动。

---

## 3. capabilities 声明诚实性 —— ✅ 机制与测试扎实

`synapse-services/src/capability_governance.rs` 的设计值得肯定：

### 3.1 路由驱动（19 处 `manifest_has_route`）

MSC 声明**由路由是否存在驱动**，而非硬编码：

```rust
/// declared only when the `GET /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` ...
self.manifest_has_route("GET", "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device")
```

### 3.2 硬编码声明均带书面理由

6 项硬编码（`BASE_UNSTABLE_FEATURES`）逐条附理由，例如：

```rust
/// Phone (MSISDN) verification is not implemented — no SMS service,
/// no msisdn requestToken/submitToken routes. Declaring this as false
/// prevents clients from offering a login-via-phone path that cannot complete.
("m.supports_login_via_phone_number", false),
```

### 3.3 私有扩展的命名空间纪律

```rust
// Private `io.hula.*` extensions are intentionally NOT declared in
// `/versions.unstable_features` — that surface is unauthenticated and
// consumed by stock Matrix clients which do not understand the `io.hula.*` namespace.
```

### 3.4 治理测试（14 项，全通过）

| 测试 | 保护对象 |
|---|---|
| `..._snapshot_authenticated_surface` / `..._snapshot_public_surface` | 两个 surface 的输出形状 |
| `test_all_capabilities_have_governance_classification` | 每个 capability 都必须有来源分类 |
| `test_no_residual_static_stable_governance` | 防止静态 stable 声明回流 |
| `test_capabilities_public_surface_hides_private_extensions` | 未认证不含 `io.hula.*` |
| `test_declare_private_extensions_suppresses_hula_capabilities` | 私有扩展开关生效 |
| `test_build_client_versions_keeps_supported_versions_ordered_and_unique` | `/versions` 有序去重 |
| `test_client_version_support_keeps_legacy_before_stable_versions` | 版本顺序契约 |

> 这套机制使"声明必须有实现来源"成为**结构性约束**而非人工纪律 —— 是本仓质量亮点。

---

## 4. ⏳ 未完成项（如实标注）

### 4.1 错误语义（未系统核对）

已完成：确认 `synapse-common/src/error.rs` 定义 **112 个** `M_*` 错误码；
联邦路由下有 23 处 `M_NOT_FOUND`/`not_found` 用法。

**未完成**：
- 未逐一核对各端点返回的 errcode 是否符合 Matrix spec
- 未系统检查"存在性泄漏"（联邦端点对"存在但未授权"与"不存在"是否统一）
- 未检查 HTTP 状态码与 errcode 的配对正确性

### 4.2 联邦安全规则（未开展）

AGENTS.md 列出的检查项**均未验证**：
- canonical JSON over `method`/`uri`/`origin`/`destination`/`content`
- `Authorization: X-Matrix` 解析的宽松/严格边界
- server key 响应与 notary query 响应形状区分
- `server_name`/`verify_keys`/`old_verify_keys`/`valid_until_ts`/签名 的校验与缓存前置检查
- origin/user-domain 检查在 membership / device key / media / directory 上是否被削弱

---

## 5. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export CARGO_TARGET_DIR=/tmp/p2t      # 隔离并发构建

# 能力治理测试（14 项）
cargo nextest run --profile tdd --features test-utils -p synapse-services --lib capability_governance

# 房间版本声明
grep -n -A22 "pub const SUPPORTED_ROOM_VERSIONS" synapse-common/src/room_versions.rs
grep -n -A10 "pub fn client_room_versions_capability" synapse-common/src/room_versions.rs

# 路由三方对账
wc -l tests/integration/snapshots/route_ledger_default.snapshot
cargo nextest run --profile ci --all-features --test integration api_route_ledger_tests
```

---

## 6. 移交后续

| 项 | 优先级 |
|---|---|
| 房间版本 v12/v13 声明的最终裁定（§2.5 选项 A/B/C） | **高（需用户决策）** |
| 错误语义系统核对（errcode ↔ spec、存在性泄漏） | **高** |
| 联邦安全规则审查（canonical JSON / X-Matrix / server key 形状） | **高** |
| `versions.rs:159-164` 的硬编码兜底 `{default:"11", available:{"11":"stable"}}` 与权威表的差异，确认是否为有意的 unauthenticated fallback | 中 |

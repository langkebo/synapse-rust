# mutation#6 失效分析报告

**日期**: 2026-09-29  
**状态**: ⚠️ **FAIL** — mutation#6 did NOT turn the suite red — the cfg gate guard is self-proving  
**结论**: 架构迁移（CAS 路由前缀对齐）导致唯一混合上下文用例 `/login` 消失；`cfg_of` 填充逻辑完好，`gate_of` 仍依赖它，**不需要恢复填充逻辑**。

## 故障定义

`mutation#6` 的目标是验证 `gate_of` 方法的正确性。该 mutation 通过将 `gate_of(row)` 替换为 `scope_only(row)`（只返回最小上下文的局部 scope），测试系统是否能检测出模块门控信息的丢失。

预期行为：如果 `gate_of` 正确地聚合了所有必要的模块门控，那么 `scope_only` 应该会导致至少一条 lane（golden lane）无法复现，从而让测试变红。

实际结果：测试**没有变红**，说明 `scope_only` 和真实的 `gate_of` 对所有 profile rows 产生了完全相同的结果。

## 诊断过程

### 1. 在 `26730b2cb` 确认 cfg_of 有数据（✅ 已确认）

```
At commit 26730b2cb:
  cfg_of size: 1496          # ← 有数据
  registrars size: 1496      # ← 有数据
  profile rows: 1146
  Rows where gate_of != scope_only: 1
    ('GET', '/login'): gate=frozenset({'feature = "cas-sso"'}), scope=frozenset()
```

当时 mutation#6 **有效**：`/login` 是唯一的混合上下文路由，golden lane（不含 `cas-sso` 特性）能捕获 `scope_only` 导致的偏差。

### 2. 二分查找定位（✅ 已完成，但首轮判据错误）

首轮二分以「`cfg_of` 是否为空」为判据，**715 个提交全部 HAS DATA** —— 说明最初的假设（`cfg_of` 被清空 / `mod_gated_files`/`_record_guards` 路径被误删）是**错误的**。

修正判据为「`gate_of != scope_only` 的行数是否为 0」后，直接锁定到 **`d4e22f9ea`**：

| 提交 | `/login` in rows | gate_of≠scope_only 行数 | mutation#6 |
|------|------------------|------------------------|------------|
| `26730b2cb`（迁移前基线） | True | 1 | OK |
| `d4e22f9ea~1`（迁移前一晚） | True | 1 | OK |
| `d4e22f9ea`（CAS 迁移） | False | 0 | **FAIL** |
| `main` HEAD | False | 0 | **FAIL** |

main 上的现状（`cfg_of` 数据完好）：

```
AFTER CAS migration (main):
  /login in rows: False
  gate_of(/login): frozenset({'feature = "cas-sso"'})
  Rows where gate_of != scope_only: 0
  cfg_of has 1488 entries        # ← 数据在，但混合上下文用例没了
```

## 根本原因

**提交 `d4e22f9ea`（fix(routes): CAS 路由前缀对齐上游规范）消除了仓库中唯一的"混合上下文"路由。**

### 迁移前后对比

**之前**（`/login` 有两个注册来源）：
- `assembly.rs::create_auth_compat_router` 定义 `GET /login`（无门控，总是合并）
- `cas.rs::cas_routes` 也定义 `GET /login`（`#[cfg(feature = "cas-sso")]` 门控）
- 因此 `cfg_of[('GET','/login')] = {frozenset(), frozenset({'feature = "cas-sso"'})}` ← **混合上下文**
- `gate_of` = `union` = `{cas-sso}`；`scope_only` = `min` = `{}` → **有差异**，golden lane（无 cas-sso）会多接纳一条路由而被检测

**之后**（单一来源，不再混合）：
- `cas.rs::cas_routes` 的协议端点全部 nest 到 `/_synapse/cas/*`（`/login` → `/_synapse/cas/login`）
- Matrix 标准的 `/login` 现在只由 `create_auth_compat_router` 提供（无门控，单一上下文）
- 仓库中剩余的所有路由：门控路由（如 `/_synapse/cas/*`）的 `cfg_of` 只有单一非空上下文；无门控路由只有空上下文 → **`union` ≡ `min`，mutation#6 无差异可检**

### 定性判断（回应三个排查问题）

1. **是架构迁移导致的 cfg_of 废弃吗？** —— 不是。`cfg_of` 从未被废弃：1496 → 1488 只是路由数量随迁移减少（8 条 `/login` 等根级路径并入 `/_synapse/cas` 前缀），填充逻辑（`_record_guards` / `mod_gated_files`）完好无损。
2. **是 mod_gated_files / _record_guards 路径被误删吗？** —— 不是。`module_gates` 仍有 112 个条目（7 个非空），`registrars` 与 `cfg_of` 均有 1488 条记录。
3. **cfg_of 仍被 gate_of 依赖，需恢复填充逻辑吗？** —— 不需要恢复。`gate_of` 仍然正确地从 `cfg_of` 聚合门控（对 `/_synapse/cas/*` 等 193 条门控行为正常）。失效的只是 mutation#6 这个**测试本身**的区分度。

## 影响评估

- ✅ **不影响项目实际功能**：`gate_of` / `cfg_of` 生产逻辑正常，所有门控路由（CAS/SAML/OIDC/widgets 等 193 行）的门控计算正确，golden lane / fixtures 一致。
- ⚠️ **门禁区分度退化**：mutation#6 目前是 self-proving（零区分度），无法再证明 `gate_of` 的聚合语义。

## 修复方案

### 方案 A：恢复 `/login` 混合上下文（不推荐）
将 `/login` 重新双注册回无门控+有门控两个来源。这会逆转刚完成的「CAS 路由对齐上游规范」重构（正是它消灭了根级路径与 Matrix 标准端点的冲突），制造真实的产品问题来挽留一条测试，得不偿失。

### 方案 B：重设计 mutation#6（推荐）
不再依赖天然的混合上下文路由，改为**合成**一个：
1. 在 mutation#6 中，取一条现有门控行（如 `('GET','/_synapse/cas/serviceValidate')`，其 `cfg_of` 为单一 `{cas-sso}` 上下文）；
2. 向它的 `cfg_of[row]` 注入一个额外的空上下文 `frozenset()`，人为构造混合上下文；
3. 重新运行 lane 复现检查 —— 若 `scope_only` 此时导致 golden lane 漂移而真实 `gate_of` 不漂移，则证明聚合语义正确。

优点：不依赖任何具体路由形态，混合用例永远存在，测试对未来的路由重构免疫。

### 方案 C：标记 P2 接受现状（临时）
在测试输出中把该 FAIL 降级为 `SKIP（no mixed-context row exists）`，并保留一条哨兵：一旦仓库中再次出现混合上下文路由，自动恢复为严格断言。

## 建议行动

1. **短期（本次）**：采用方案 C 的哨兵形式 —— mutation#6 改为：
   - 若存在混合上下文行 → 维持现有严格断言；
   - 若不存在 → 明确输出 `SKIP: no mixed-context row in current route graph`，而非误导性的 `FAIL`。
2. **长期**：按方案 B 实现合成混合上下文，恢复 mutation#6 的自我证明能力。
3. **无需任何产品代码改动**：`cfg_of` 填充逻辑、`mod_gated_files`、`_record_guards`、`gate_of` 全部保持现状。

## 待办清单

- [x] 在 `26730b2cb` 确认 `cfg_of` 有数据（1496 条）
- [x] 二分查找确认清空提交 —— 首轮按「cfg_of 为空」二分证伪（715 提交均有数据）；修正判据后定位到 `d4e22f9ea`（混合上下文用例消失）
- [x] 判断是架构迁移还是路径误删 —— **架构迁移（CAS 路由前缀对齐）的副作用**，非误删、非废弃
- [ ] 长期：按方案 B 合成混合上下文，重写 mutation#6 使其不依赖具体路由形态

---

**备注**: 这个问题不是功能性问题（不影响业务代码），而是门禁失效问题。mutation#6 的目的是确保 `gate_of` 的正确性，但 CAS 路由迁移后它暂时失去了测试价值，需要重设计而非恢复旧路由形态。

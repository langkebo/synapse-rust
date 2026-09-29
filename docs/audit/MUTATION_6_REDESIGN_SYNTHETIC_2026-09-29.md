# Mutation#6 重设计分析（基于合成混合上下文）

**日期**：2026-09-29  
**基准**：`26730b2cb` → main（包含 `d4e22f9ea` CAS 路由前缀对齐）  
**问题**：CAS 路由迁移消除唯一混合上下文用例 `/login`，导致 mutation#6 失效

## 失效根因

mutation#6 的目标是检测 `gate_of` 方法是否遗漏了 module gates。原始设计依赖真实路由图中存在满足以下条件的路由：
- `gate_of(route) != scope_only(route)`
- 即：该路由的 `cfg_of` 有多个上下文，且至少一个文件有 module_gates

在 `26730b2cb` 上，`/login` 路由同时存在于：
- CAS router（gated by `feature = "cas-sso"`）
- auth-compat router（ungated）

这使得 `gate_of(/login)` 包含 module_gates，而 `scope_only(/login)` 是最小 cfg_of，两者不等。

在 main 上，CAS 路由被重命名为 `/_synapse/cas/login`，所有 1149 条路由都变成单一上下文，`gate_of ≡ scope_only`。

## 解决方案

**不依赖真实路由**：直接注入合成路由数据到 resolver 中，模拟存在混合上下文的情况。

```python
mixed_key = ("GET", "/synthetic-mixed-route")
# cfg_of 有两个上下文：空 和 {"feature = "builtin-oidc""}
synthetic.cfg_of[mixed_key] = {frozenset(), frozenset({"feature = "builtin-oidc"})}
# registrars 指向有 module_gates 的文件
synthetic.registrars[mixed_key] = {("synapse-web/src/routes/builtin_oidc_provider.rs", "create_oidc_routes")}
# module_gates
synthetic.module_gates["synapse-web/src/routes/builtin_oidc_provider.rs"] = {"feature = "builtin-oidc""}
```

计算：
- `orig_gate_of = min(cfg_of) | module_gates = frozenset() | {"feature = "builtin-oidc""} = {"feature = "builtin-oidc""}`
- `scope_only = min(cfg_of) = frozenset()`

验证：`orig != scope` → 通过 mutation#6 测试

## 效果

- ✅ `scripts/contract/test_extract_registered.py --mutation-check` 全部通过
- ✅ mutation#6 不再依赖真实路由形态，始终有效
- ✅ 门禁逻辑保持完整，不会因为未来路由变化再次失效

## 改进建议

1. **长期考虑**：将合成路由的定义提取到常量/配置文件中，便于维护
2. **文档更新**：在 `ROUTE_CONTRACT.md` 中添加 mutation#6 重设计的说明
3. **监控**：定期复查合成数据的合理性（确保 `builtin-oidc` 确实是有效的 feature）

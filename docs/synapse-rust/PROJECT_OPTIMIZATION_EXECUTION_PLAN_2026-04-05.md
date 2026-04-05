# 项目优化执行路线图（2026-04-05）

> 文档定位：执行文档
> 适用范围：当前仓库的第一批治理与可信度收口工作
> 权威状态来源：`CAPABILITY_STATUS_BASELINE_2026-04-02.md`

---

## 1. 目标

本轮优化优先解决“文档、测试、CI 是否可信”的问题，而不是先做大规模结构重构。目标是让：

1. 当前能力结论有单一事实源；
2. 主 CI 门禁与文档口径一致；
3. 假绿（false green）与占位成功（placeholder success）被显式识别；
4. shell route / placeholder 治理形成最小闭环。

---

## 2. 问题分级

### P0：可信度问题

- README、历史报告、测试文档之间存在状态漂移；
- 主 CI 实际语义与文档描述不完全一致；
- E2E 测试默认早退，容易形成“执行通过”错觉；
- 部分集成测试在环境不满足时直接返回；
- shell route 检测脚本存在统计失真；
- placeholder 路由虽然已有契约测试样例，但治理规则尚未统一沉淀。

### P1：治理链路不完整

- 哪些文档是权威、哪些是派生、哪些只是归档，未完全收口；
- 哪些结果可用于“已实现并验证”，缺少统一判定；
- 非阻断 evidence job 容易被误读为主门禁的一部分。

### P2：结构性优化待后置

- 路由/中间件/服务容器仍有拆分空间；
- 搜索与部分能力域仍需进一步收敛；
- schema contract / migration gate 仍可继续强化。

---

## 3. 执行阶段

### Phase 1：治理文档落地

新增并维护以下正式文档：

- `PROJECT_OPTIMIZATION_EXECUTION_PLAN_2026-04-05.md`
- `DOCUMENT_GOVERNANCE_AND_SINGLE_SOURCE_OF_TRUTH_2026-04-05.md`
- `TEST_AND_CI_SEMANTICS_ALIGNMENT_2026-04-05.md`
- `FALSE_GREEN_AND_PLACEHOLDER_GOVERNANCE_2026-04-05.md`

验收条件：
- 明确权威事实源；
- 明确主门禁、扩展验证、手动分析的边界；
- 明确 false green / placeholder 的治理方式。

### Phase 2：核心文档收口

优先同步：

- `README.md`
- `TESTING.md`
- `COMPLETION_REPORT.md`

验收条件：
- 对外入口文档均回指权威基线；
- 历史报告不再承担当前成熟度判断；
- 测试文档与真实 CI 行为一致。

### Phase 3：第一批工程优化

优先修改：

- `tests/e2e/user_flow_tests.rs`
- `scripts/detect_shell_routes.sh`
- 必要的测试/治理文件

验收条件：
- 默认不再把“未执行真实 E2E”伪装成绿色通过；
- shell route 检测结果统计可信；
- placeholder 治理至少有一个持续可执行样板。

### Phase 4：结构优化后置

暂不优先：

- 大型模块拆分；
- 服务容器 bounded context 重构；
- 大范围 schema contract 扩展。

原则：先收可信度，再做优雅性重构。

---

## 4. 文件范围

### 文档治理
- `docs/synapse-rust/`
- `README.md`
- `TESTING.md`
- `docs/synapse-rust/COMPLETION_REPORT.md`

### 测试与门禁治理
- `.github/workflows/ci.yml`
- `.github/workflows/test.yml`
- `tests/e2e/user_flow_tests.rs`
- `tests/integration/mod.rs`
- `tests/integration/api_placeholder_contract_p0_tests.rs`

### shell route / placeholder
- `scripts/detect_shell_routes.sh`
- `scripts/shell_routes_allowlist.txt`

---

## 5. 验收标准

### 文档层
- 同一能力状态只以基线文档为准；
- README 与 TESTING 不再独立给出高于证据强度的结论；
- 历史报告明确降级为归档材料。

### 工程层
- E2E 测试默认不再“早退即通过”；
- shell route 检测脚本统计与输出一致；
- placeholder 行为要么返回规范错误，要么返回明确业务确认数据。

### 证据层
- 主门禁对应到 `.github/workflows/ci.yml` 的实际阻断路径；
- 非阻断 evidence/coverage 不再被表述为默认发布门禁；
- 能力结论必须能回溯到代码、测试或 CI 证据。

---

## 6. 本轮实施策略

本轮只做第一批收口：

1. 建立治理文档；
2. 修正文档口径；
3. 修复 shell route 检测脚本可信度问题；
4. 将 E2E 从“默认早退”调整为“默认忽略，显式启用”；
5. 为后续 integration skip 显式化、workflow 进一步收口留下清晰边界。

这批改动以降低误判风险为第一优先级。

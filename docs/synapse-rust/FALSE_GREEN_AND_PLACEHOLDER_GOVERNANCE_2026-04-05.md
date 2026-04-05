# 假绿与占位成功治理规则（2026-04-05）

> 文档定位：治理规范
> 目标：避免“没有真实执行却显示通过”以及“返回空成功却被误判为已实现”

---

## 1. 两类核心风险

### 1.1 False Green（假绿）

指测试、workflow 或验证流程在没有完成真实目标的情况下仍呈现“通过”信号。

典型表现：

- 测试因环境不满足直接 `return`；
- 需要显式开启的流程被当作默认已验证；
- 关键依赖不可用时仅打印日志，不改变结果语义。

### 1.2 Placeholder Success（占位成功）

指端点返回 `200 OK` 或空对象 `{}`，但并未提供真实业务行为或可验证确认数据。

典型表现：

- `Ok(Json(json!({})))`
- 固定假数据
- 无法区分“规范允许空响应”和“尚未实现的壳路由”

---

## 2. E2E 治理规则

E2E 的真实 HTTP 流程依赖运行中的服务和 `E2E_RUN=1`。

治理要求：

1. 默认不应通过早退制造绿色假象；
2. 默认应采用 `#[ignore]` 或等价显式门控；
3. 一旦显式执行，环境不满足应报错，而不是静默返回；
4. 文档必须写明触发方式与前置条件。

推荐语义：
- “默认忽略，显式启用”
- 而不是“默认已通过但实际上没有执行”

---

## 3. Integration 治理规则

集成测试依赖隔离数据库与迁移初始化。

治理要求：

1. CI 或 `INTEGRATION_TESTS_REQUIRED` 环境下，初始化失败必须失败；
2. 本地允许跳过时，必须有清晰的原因输出；
3. 文档中不得把”本地可跳过”误写成”默认已验证”；
4. 新增高价值契约测试时，应优先减少静默跳过路径。

### 3.1 已知并发资源竞争问题

**问题描述**：
部分集成测试在高并发执行时会因数据库连接池耗尽而失败（500 错误或连接超时），但单线程执行时通过。

**影响范围**：
- `api_device_presence_tests::test_presence_management`
- `api_device_presence_tests::test_presence_list_after_session_invalidation_and_relogin`
- `api_shell_route_fixes_p2_misc_tests::test_set_invite_blocklist_returns_confirmation`
- `api_shell_route_fixes_p2_misc_tests::test_set_invite_allowlist_returns_confirmation`

**临时缓解措施**：
- 使用 `--test-threads=1` 或 `--test-threads=2` 运行集成测试
- CI 脚本 `scripts/run_ci_tests.sh` 已配置 `TEST_THREADS=4` 作为折中

**根本原因**：
- 测试共享全局数据库连接池
- 高并发时连接池配置不足以支撑所有并行测试
- 部分测试持有连接时间较长（如 presence 测试）

**长期解决方向**：
1. 增加测试环境连接池大小配置
2. 优化测试用例的连接持有时间
3. 考虑为高资源消耗测试添加 `#[serial]` 标记
4. 评估是否需要为不同测试套件使用独立连接池

---

## 4. Shell Route 治理规则

### 4.1 定义

shell route 指路由对外暴露成功响应，但只返回空对象或缺乏业务确认数据，无法支撑“行为已实现”的判断。

### 4.2 治理链

最小闭环要求：

1. **静态扫描**：`scripts/detect_shell_routes.sh`
2. **allowlist**：`scripts/shell_routes_allowlist.txt`
3. **契约测试**：高风险端点补齐错误/不支持/确认数据断言
4. **文档约束**：不允许把空成功包装成完成度证据

### 4.3 allowlist 使用约束

allowlist 仅用于过渡治理，不代表问题已解决。

允许加入 allowlist 的场景：
- 规范允许空成功响应；
- 已明确评估为低风险 DELETE 类操作；
- 已记录为技术债并有后续治理安排。

不允许加入 allowlist 的场景：
- 创建、更新、提交、设置类操作却没有确认数据；
- 会影响能力基线判断的对外接口；
- 尚不清楚是否真实实现的端点。

---

## 5. Placeholder 契约规则

对于未实现或不支持的能力：

- 应返回明确规范错误，如 `M_UNRECOGNIZED`、`M_NOT_FOUND`、`M_INVALID_PARAM`；
- 不得返回误导性的 `200 {}`。

对于已实现但返回成功的能力：

- 应返回可验证确认数据，例如：
  - `room_id`
  - `user_id`
  - 变更后的字段值
  - `created_ts` / `updated_ts`

当前仓库可复用：
- `tests/integration/api_placeholder_contract_p0_tests.rs`

该文件中的模式应用于后续高风险 placeholder 端点治理。

---

## 6. 评审检查点

提交涉及测试、路由、文档时，至少检查：

1. 有没有“return 之后看起来像通过”的路径；
2. 有没有 ignored / skip 未在文档中说明；
3. 有没有 `200 {}` 但缺乏确认数据的新增端点；
4. shell route 扫描结果是否可信；
5. 是否新增了与当前证据不匹配的能力表述。

---

## 7. 本轮落地要求

本轮至少完成以下收口：

- E2E 从默认早退改为默认忽略、显式启用；
- shell route 检测脚本统计修正；
- README / TESTING / 归档报告不再误导当前验证强度；
- placeholder 契约测试继续作为高风险端点治理样板。

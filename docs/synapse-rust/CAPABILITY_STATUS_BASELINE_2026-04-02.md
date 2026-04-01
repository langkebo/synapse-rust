# synapse-rust 第一版能力总表

> 文档类型：正式能力状态基线  
> 版本：v1.0  
> 日期：2026-04-02  
> 状态规则：`已实现并验证` / `已实现待验证` / `部分实现` / `未实现` / `不纳入本期`  
> 说明：本表用于替代“100% 完成”“完整实现”“生产就绪”一类笼统表述，所有结论均应结合代码、测试与文档证据理解  
> 单一事实源要求：本表中的“代码证据 / 测试证据 / 文档来源”共同构成第一版正式能力结论

---

## 一、使用规则

1. 本表是当前阶段对外与对内的正式能力状态基线。
2. 路由存在不等于行为完成，行为完成不等于规范级兼容。
3. 没有测试证据或互操作证据的能力，不得标记为“已实现并验证”。
4. 可选能力不得混入主承诺能力通过率。

---

## 二、Matrix 标准域能力矩阵

| 能力域 | 子能力 | 当前状态 | 代码证据 | 测试证据 | 文档来源 | 剩余风险 | 主承诺 |
|------|------|------|------|------|------|------|------|
| Client-Server API | 主路由装配 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [Cargo.toml](../../Cargo.toml) | [API_DOCUMENTATION.md](./API_DOCUMENTATION.md) | 路由覆盖面广，但尚未形成统一契约级兼容证明 | 是 |
| Client-Server API | Room / Sync / Presence 主链 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [test-execution-inventory.md](../../.trae/specs/analyze-synapse-gap-and-optimization/test-execution-inventory.md) | [SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md](./SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md) | 有实现且已有主回归接线，但断言粒度与互操作验证不足 | 是 |
| Server-Server API | Federation 主链 | 部分实现 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/integration/federation_error_tests.rs](../../tests/integration/federation_error_tests.rs) | [SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md](./SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md) | 错误处理测试已接线，但跨 homeserver 互通验证仍未闭环 | 是 |
| Application Service API | AS 主链 | 部分实现 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/unit/app_service_api_tests.rs](../../tests/unit/app_service_api_tests.rs) | [APP_SERVICE_INTEGRATION.md](./APP_SERVICE_INTEGRATION.md) | 路由与基础测试存在，但完整行为边界与生产承诺未收敛 | 否 |
| Identity / Push | Push 相关路由 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/unit/push_api_tests.rs](../../tests/unit/push_api_tests.rs) | [API_DOCUMENTATION.md](./API_DOCUMENTATION.md) | 基础路径存在，仍需补齐通知链路与契约验证 | 是 |
| Olm / Megolm | E2EE 主链 | 已实现待验证 | [container.rs](../../src/services/container.rs) | [tests/unit/e2ee_api_tests.rs](../../tests/unit/e2ee_api_tests.rs) | [E2EE_ANALYSIS.md](./E2EE_ANALYSIS.md) | 模块覆盖高，但跨设备、恢复、交叉签名证据不足 | 是 |
| Room Versions | 房间版本与兼容字段 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/integration/api_room_tests.rs](../../tests/integration/api_room_tests.rs) | [API_COVERAGE_REPORT.md](./API_COVERAGE_REPORT.md) | 兼容字段与行为细节仍需专项矩阵验证 | 是 |
| 稳定 MSC | Sliding Sync / QR Login / Thread / Space | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/unit/mod.rs](../../tests/unit/mod.rs) | [COMPLETION_REPORT.md](./COMPLETION_REPORT.md) | 已有模块与回归测试，但尚未逐项升级为“已验证” | 视能力而定 |

---

## 三、Synapse 关键能力矩阵

| 能力域 | 子能力 | 当前状态 | 代码证据 | 测试证据 | 文档来源 | 剩余风险 | 主承诺 |
|------|------|------|------|------|------|------|------|
| Synapse Admin | 管理接口主链 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/integration/mod.rs](../../tests/integration/mod.rs) | [API_DOCUMENTATION.md](./API_DOCUMENTATION.md) | 路由较全，但仍需继续契约化与权限边界验证 | 是 |
| Worker 形态 | 多进程 / 复制 / 队列 | 部分实现 | [container.rs](../../src/services/container.rs) | [tests/unit/worker_api_tests.rs](../../tests/unit/worker_api_tests.rs) | [SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md](./SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md) | 当前正式口径仍应限定为单进程可运行，多 Worker 未成熟 | 否 |
| 联邦行为 | 联邦互通与签名链路 | 部分实现 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/integration/federation_error_tests.rs](../../tests/integration/federation_error_tests.rs) | [SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md](./SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md) | 错误处理回归已接线，但互操作与联邦同步闭环仍待补齐 | 是 |
| SSO / OIDC / SAML | 企业认证集成 | 不纳入本期 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/unit/saml_tests.rs](../../tests/unit/saml_tests.rs) | [FEATURE_IMPROVEMENT_REPORT.md](./FEATURE_IMPROVEMENT_REPORT.md) | 属于可选能力，依赖外部环境，不计入当前主承诺 | 否 |
| Media | 上传、下载、预览、配额 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/unit/media_api_tests.rs](../../tests/unit/media_api_tests.rs) | [API_DOCUMENTATION.md](./API_DOCUMENTATION.md) | 仍需补齐大文件、配额与回收场景验证 | 是 |
| Background Jobs | 后台更新与任务 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/unit/background_update_api_tests.rs](../../tests/unit/background_update_api_tests.rs) | [SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md](./SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md) | 有实现，但生产级调度、恢复与可观测性证据不足 | 否 |
| 部署治理 | 迁移入口与运行时初始化 | 已实现待验证 | [server.rs](../../src/server.rs) | [migrations/README.md](../../migrations/README.md) | [DEPLOYMENT_GUIDE.md](./DEPLOYMENT_GUIDE.md) | 主入口口径已统一到脚本与部署文档，仍需后续扩展到发布模板与更多历史文档 | 是 |

---

## 四、首批风险说明

| 风险主题 | 现状 | 影响 |
|------|------|------|
| 状态口径冲突 | 历史文档仍存在“开发中”与“生产就绪”并存 | 影响发布判断与对外认知 |
| 测试接线不完整 | `e2e` 已接入独立入口，但 `performance` 仍为手动套件，部分 unit/integration 文件仍未执行 | 影响测试覆盖结论可信度 |
| 兼容性缺少规范级证明 | 联邦、E2EE、AppService、Worker 仍未形成互操作闭环 | 影响对标 Synapse 与 Matrix 的结论 |
| 部署口径未完全收敛 | README、部署指南、迁移索引已收敛到统一入口，但发布模板与历史专题文档仍需继续同步 | 影响部署与回滚操作一致性 |

---

## 五、后续更新规则

1. 新增或修改能力状态时，必须同步更新：
   - 本表
   - README 能力摘要
   - 部署/迁移口径文档
   - 对应测试证据
   - 对应专题文档
2. 若某能力从“已实现待验证”升级为“已实现并验证”，必须补充明确测试或互操作证据。
3. 若某能力被降级，不允许仅更新局部报告，必须同步更新本表与发布说明。

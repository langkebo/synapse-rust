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
5. 所有“存在路由但明确未支持”的接口，统一维护在 [UNSUPPORTED_ENDPOINTS.md](./UNSUPPORTED_ENDPOINTS.md)。

---

## 二、Matrix 标准域能力矩阵

| 能力域 | 子能力 | 当前状态 | 代码证据 | 测试证据 | 文档来源 | 剩余风险 | 主承诺 |
|------|------|------|------|------|------|------|------|
| Client-Server API | 主路由装配 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [Cargo.toml](../../Cargo.toml) | [API_DOCUMENTATION.md](./API_DOCUMENTATION.md) | 路由覆盖面广，但尚未形成统一契约级兼容证明 | 是 |
| Client-Server API | Room / Sync / Presence 主链 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [test-execution-inventory.md](../../.trae/specs/analyze-synapse-gap-and-optimization/test-execution-inventory.md) | [SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md](./SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md) | 有实现且已有主回归接线，但断言粒度与互操作验证不足 | 是 |
| Server-Server API | Federation 主链 | 已实现并验证（完整闭环） | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/integration/federation_error_tests.rs](../../tests/integration/federation_error_tests.rs)、[tests/federation_mock_tests.rs](../../tests/federation_mock_tests.rs) | [FEDERATION_VERIFICATION_MAPPING_2026-04-03.md](./FEDERATION_VERIFICATION_MAPPING_2026-04-03.md) | Mock Federation Server 完整验证（11/11 测试通过），覆盖服务器发现、密钥交换、房间邀请/加入、消息同步、状态查询、批量事件、错误处理 | 是 |
| Application Service API | AS 主链 | 已实现并验证（完整闭环） | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/unit/app_service_api_tests.rs](../../tests/unit/app_service_api_tests.rs)、[tests/integration/api_appservice_tests.rs](../../tests/integration/api_appservice_tests.rs)、[tests/integration/api_appservice_basic_tests.rs](../../tests/integration/api_appservice_basic_tests.rs)、[tests/integration/api_appservice_p1_tests.rs](../../tests/integration/api_appservice_p1_tests.rs) | [APPSERVICE_VERIFICATION_MAPPING_2026-04-03.md](./APPSERVICE_VERIFICATION_MAPPING_2026-04-03.md) | P0+P1 测试全部通过（10/10），覆盖注册/查询、虚拟用户、事务推送、as_token/hs_token 认证、namespace 独占性与查询的完整功能 | 否 |
| Identity / Push | Push 相关路由 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/unit/push_api_tests.rs](../../tests/unit/push_api_tests.rs) | [API_DOCUMENTATION.md](./API_DOCUMENTATION.md) | 基础路径存在，仍需补齐通知链路与契约验证 | 是 |
| Olm / Megolm | E2EE 主链 | 已实现并验证（完整闭环） | [container.rs](../../src/services/container.rs) | [tests/integration/api_e2ee_tests.rs](../../tests/integration/api_e2ee_tests.rs)、[tests/integration/api_e2ee_advanced_tests.rs](../../tests/integration/api_e2ee_advanced_tests.rs) | [E2EE_VERIFICATION_MAPPING_2026-04-03.md](./E2EE_VERIFICATION_MAPPING_2026-04-03.md) | 设备密钥、one-time key、密钥变更已验证，密钥备份/恢复、交叉签名完整流程已通过 P2 测试（3/3） | 是 |
| Room Versions | 房间版本与兼容字段 | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/integration/api_room_tests.rs](../../tests/integration/api_room_tests.rs) | [API_COVERAGE_REPORT.md](./API_COVERAGE_REPORT.md) | 兼容字段与行为细节仍需专项矩阵验证 | 是 |
| 稳定 MSC | Sliding Sync / QR Login / Thread / Space | 已实现待验证 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/unit/mod.rs](../../tests/unit/mod.rs) | [FEATURE_IMPROVEMENT_REPORT.md](./FEATURE_IMPROVEMENT_REPORT.md)（历史材料，仅供追溯） | 已有模块与回归测试，但尚未逐项升级为“已验证” | 视能力而定 |

---

## 三、Synapse 关键能力矩阵

| 能力域 | 子能力 | 当前状态 | 代码证据 | 测试证据 | 文档来源 | 剩余风险 | 主承诺 |
|------|------|------|------|------|------|------|------|
| Synapse Admin | 管理接口主链 | 已实现并验证（完整闭环） | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/integration/api_protocol_alignment_tests.rs](../../tests/integration/api_protocol_alignment_tests.rs)、[tests/integration/api_admin_user_lifecycle_tests.rs](../../tests/integration/api_admin_user_lifecycle_tests.rs)、[tests/integration/api_admin_room_lifecycle_tests.rs](../../tests/integration/api_admin_room_lifecycle_tests.rs) | [ADMIN_VERIFICATION_MAPPING_2026-04-03.md](./ADMIN_VERIFICATION_MAPPING_2026-04-03.md) | 权限边界、关键查询、写操作闭环已验证，用户/房间管理完整生命周期已通过 P2 测试（5/5） | 是 |
| Worker 形态 | 多进程 / 复制 / 队列 | 部分实现 | [container.rs](../../src/services/container.rs) | [tests/unit/worker_api_tests.rs](../../tests/unit/worker_api_tests.rs) | [SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md](./SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md) | 当前正式口径仍应限定为单进程可运行，多 Worker 未成熟 | 否 |
| 联邦行为 | 联邦互通与签名链路 | 已实现并验证（完整闭环） | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/integration/federation_error_tests.rs](../../tests/integration/federation_error_tests.rs)、[tests/federation_mock_tests.rs](../../tests/federation_mock_tests.rs) | [FEDERATION_VERIFICATION_MAPPING_2026-04-03.md](./FEDERATION_VERIFICATION_MAPPING_2026-04-03.md) | Mock Federation Server 完整验证（11/11 测试通过），覆盖服务器发现、密钥交换、房间邀请/加入、消息同步、状态查询、批量事件、错误处理 | 是 |
| SSO / OIDC / SAML | 企业认证集成 | 部分实现 | [assembly.rs](../../src/web/routes/assembly.rs) | [tests/unit/saml_tests.rs](../../tests/unit/saml_tests.rs) | [DEPLOYMENT_GUIDE.md](./DEPLOYMENT_GUIDE.md) | 可选能力，代码存在但需外部配置启用，不计入主承诺通过率 | 否 |
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

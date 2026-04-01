# synapse-rust 统一能力状态枚举与能力总表模板

> 目的：建立项目后续统一事实源的最小基线  
> 使用范围：README、完成度报告、专题分析、发布说明、测试结论  
> 结论规则：凡未绑定代码与测试证据者，不得标记为“已实现并验证”

---

## 一、统一状态枚举

| 状态 | 定义 | 允许使用场景 | 禁止误用 |
|------|------|------|------|
| 已实现并验证 | 代码存在，且通过可信测试或互操作验证证明行为成立 | 可写入正式能力矩阵、发布说明 | 不得仅因“有路由/有模块/有报告”而使用 |
| 已实现待验证 | 代码存在，但缺少足够验证证据 | 可写入事实源，明确风险 | 不得对外宣称“兼容”“完整” |
| 部分实现 | 只有部分链路、部分场景或基础能力存在 | 可用于过渡阶段 | 不得与“完整实现”并列使用 |
| 未实现 | 代码与行为未闭环 | 用于缺口记录与规划 | 不得以“计划中”代替 |
| 不纳入本期 | 不属于当前版本主承诺范围 | 用于可选能力、实验能力、后续规划 | 不得混入主通过率与主完成度 |

---

## 二、结论词替换规则

| 禁止直接使用 | 建议替换 |
|------|------|
| 完整实现 | 已实现并验证 / 已实现待验证 / 部分实现 |
| 100% 完成 | 按能力域列出状态，不再使用单一总百分比 |
| 生产就绪 | 已通过当前发布门禁，适用于指定部署范围 |
| 已兼容 Synapse | 已对齐指定能力域，验证范围见能力矩阵 |
| 已支持 Matrix | 已支持指定标准域，状态见能力矩阵 |

---

## 三、能力总表模板

| 能力域 | 子能力 | 标准级别 | 当前状态 | 代码证据 | 测试证据 | 文档来源 | 剩余风险 | 是否主承诺 |
|------|------|------|------|------|------|------|------|------|
| 示例：Client-Server | 登录/注册 | Matrix 标准 | 已实现待验证 | `src/web/routes/...` | `tests/...` | `docs/...` | 需补契约测试 | 是 |

### 字段说明

| 字段 | 说明 |
|------|------|
| 能力域 | 如 Client-Server、Federation、E2EE、Admin、Worker、AppService、SSO |
| 子能力 | 具体到接口族或行为，如“登录注册”“联邦发送”“交叉签名” |
| 标准级别 | Matrix 标准 / Synapse 兼容 / 项目私有扩展 / 可选能力 |
| 当前状态 | 只能使用统一状态枚举 |
| 代码证据 | 必须指向具体文件或模块 |
| 测试证据 | 必须指向测试入口、用例或报告 |
| 文档来源 | 指向对外或对内说明文档 |
| 剩余风险 | 用一句话说明未覆盖边界 |
| 是否主承诺 | 是 / 否 |

---

## 四、首批建议纳入的能力域

| 能力域 | 当前初始判断 | 备注 |
|------|------|------|
| Client-Server API | 已实现待验证 | 路由覆盖广，但需与行为验证分开统计 |
| Federation | 已实现待验证 / 部分实现 | 需拆为多子能力，不使用单一总判断 |
| E2EE | 已实现待验证 / 部分实现 | 需拆为设备密钥、Megolm、备份、交叉签名等子域 |
| Admin API | 已实现待验证 | 路由覆盖与契约收敛要分开 |
| Worker | 部分实现 | 当前正式定位应为单进程可用，多 Worker 未成熟 |
| AppService | 部分实现 | 需明确行为边界 |
| OIDC / SAML | 不纳入本期主承诺或已实现待验证 | 作为可选能力独立管理 |
| Media | 已实现待验证 | 需区分上传、下载、预览、配额 |
| Room / Sync / Presence | 已实现待验证 | 需进一步补契约级验证 |

---

## 五、首批关键能力种子表

| 能力域 | 子能力 | 标准级别 | 当前状态 | 代码证据 | 测试证据 | 文档来源 | 剩余风险 | 是否主承诺 |
|------|------|------|------|------|------|------|------|------|
| 项目基线 | 项目整体状态 | 对外口径 | 已实现待验证 | [README.md](file:///Users/ljf/Desktop/hu/synapse-rust/README.md) | 无统一证据总表 | [SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/SYSTEM_GAP_ANALYSIS_AND_OPTIMIZATION_PLAN_2026-04-02.md) | 现有文档口径冲突 | 是 |
| 部署治理 | 数据库初始化与迁移 | 发布治理 | 已实现待验证 | [server.rs:L115-L128](file:///Users/ljf/Desktop/hu/synapse-rust/src/server.rs#L115-L128) | 部署/门禁文档存在 | [DEPLOYMENT_GUIDE.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/DEPLOYMENT_GUIDE.md) | README 与运行时行为冲突 | 是 |
| Client-Server API | 路由装配层 | Matrix 标准 | 已实现待验证 | [assembly.rs:L97-L155](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/assembly.rs#L97-L155) | 单元/集成测试存在但未完全清点接线 | [API_COVERAGE_REPORT.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/API_COVERAGE_REPORT.md) | 路由覆盖不等于协议兼容 | 是 |
| Federation | 联邦主链路 | Matrix 标准 | 部分实现 | [assembly.rs:L113-L115](file:///Users/ljf/Desktop/hu/synapse-rust/src/web/routes/assembly.rs#L113-L115) | 待补跨 homeserver 互操作验证 | [TODO-UNFINISHED-TASKS.md:L36-L46](file:///Users/ljf/Desktop/hu/synapse-rust/docs/TODO-UNFINISHED-TASKS.md#L36-L46) | 关键行为未完整验证 | 是 |
| E2EE | 加密主链路 | Matrix 标准 | 已实现待验证 | `src/e2ee/` | 需补跨设备与恢复验证 | [E2EE_ANALYSIS.md](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/E2EE_ANALYSIS.md) | 结论易被过度表述 | 是 |
| Worker | 多进程能力 | Synapse 兼容 | 部分实现 | `src/worker/` | 无成熟部署验证 | [SYNAPSE_COMPARISON.md:L60-L73](file:///Users/ljf/Desktop/hu/synapse-rust/docs/synapse-rust/SYNAPSE_COMPARISON.md#L60-L73) | 模块存在但未成熟启用 | 否 |
| SSO | OIDC / SAML | 可选能力 | 不纳入本期 | `src/web/routes/oidc.rs`, `src/web/routes/saml.rs` | 需外部环境验证 | [TODO-UNFINISHED-TASKS.md:L112-L120](file:///Users/ljf/Desktop/hu/synapse-rust/docs/TODO-UNFINISHED-TASKS.md#L112-L120) | 主通过率不应混算 | 否 |

---

## 六、落地要求

1. README 的能力摘要必须回链到该表或其导出版本。
2. 完成度报告不得绕过该表直接给出“总完成度”。
3. 测试报告只能更新“测试证据”列，不能单独覆盖最终状态。
4. 发布说明必须引用“是否主承诺”和“剩余风险”列。

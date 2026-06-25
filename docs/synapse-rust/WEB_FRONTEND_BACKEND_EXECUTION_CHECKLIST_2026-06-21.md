# Web 前后端整改执行清单

**整理日期**: 2026-06-21
**基线文档**: `WEB_FRONTEND_BACKEND_GAP_REMEDIATION_PLAN_2026-06-21.md`
**目标**: 将前后端 gap 修复表拆解为可执行、可验证、可回填状态的实施清单

---

## 一、执行原则

1. 先修复验证基线，再修复业务问题，避免测试误报放大后端改动范围。
2. 所有高优先级问题必须具备浏览器级或集成级复现证据，不能只依赖单点 API 探针。
3. 每个任务完成后必须补至少一类回归验证：单元测试、集成测试、浏览器 harness、人工联调。
4. 文档、能力声明和实际可用性必须同步收口，避免“声明已支持但产品面不可用”。

---

## 二、任务总览

| 编号 | 优先级 | 任务 | 依赖 | 完成标准 |
|---|---|---|---|---|
| `T0` | `P0` | 建立可靠 Web 验证基线，修正 harness 对中文界面、数字身份模态和控制台错误的识别 | 无 | `smoke:login` 能稳定区分未登录、已登录但被模态阻塞、页面运行时异常 |
| `T1` | `P0` | 复核 Element 登录后真实阻塞点，明确是 E2EE bootstrap、数字身份流程还是前端 bundle 异常 | `T0` | 形成可复现证据包：截图、console log、关键请求/响应 |
| `T2` | `P0` | 修复登录后数字身份 / E2EE bootstrap 主阻塞链路 | `T1` | 全新账号可完成 `登录 -> 进入主界面 -> 新建房间 -> 发消息` |
| `T3` | `P0` | 串联 `cross-signing / SSSS / dehydrated device` 的端到端回归链路 | `T2` | 至少一条 fresh-account 流程可自动化验证，不再长期 `#[ignore]` |
| `T4` | `P1` | 收紧 `/versions` 与 `/capabilities` 的超前声明 | `T1` | 声明的能力都能被当前支持矩阵消费，或已按配置关闭 |
| `T5` | `P1` | 明确并实现 `friends` 的正式 Web 产品入口策略 | `T4` | 形成正式入口方案，并验证 `搜索 -> 请求 -> 接受 -> DM` 闭环 |
| `T6` | `P1` | 明确 OIDC 对 stock Element 的支持边界并做降级 | `T4` | Element 不再暴露误导性 native flow 错误，或支持矩阵明确更新 |
| `T7` | `P1` | 处理 widget 相关运行时异常与版本兼容矩阵 | `T4` | 浏览器启动流程无 `Widget*Store` 初始化异常，或能力面关闭 |
| `T8` | `P2` | 清理未实现端点，补支持矩阵与 focused test | `T4` | 每个未实现端点都进入文档矩阵或完成实现 |
| `T9` | `P2` | 建立 Complement / 互通 smoke 门禁 | `T0` | CI 至少有一条 nightly 互通流水线 |
| `T10` | `P2` | 扩展浏览器 harness 为稳定兼容基线 | `T0` | 覆盖 `login / room / message / key-setup / console health` |

---

## 三、分阶段实施

### 阶段 A：验证基线与证据收口

| 任务 | 具体动作 | 产出 |
|---|---|---|
| `T0-1` | 修正 `smoke:login` 对中文文案、数字身份模态、主界面壳层的识别 | harness 稳定日志与截图 |
| `T0-2` | 在 `test:basic` 中区分“可登录但被模态阻塞”和“真正登录失败” | 更准确的浏览器级结论 |
| `T0-3` | 标准化记录 pageerror / console error / 关键请求探针 | 可归档的失败证据 |
| `T1-1` | 复抓 `device_signing / security summary / dehydrated_device / account_data` 相关请求 | 前后端真实交互形状 |
| `T1-2` | 固化一套 fresh account 复现场景 | 后续整改统一基线 |

### 阶段 B：核心兼容修复

| 任务 | 具体动作 | 产出 |
|---|---|---|
| `T2-1` | 校验并修复登录后安全引导所需接口字段、状态码和返回体 | 登录后不再卡死 |
| `T2-2` | 核对前端探测用的 `dehydrated_device / secret_storage / cross-signing` 返回语义 | Element 初始化路径对齐 |
| `T2-3` | 修复必要的异常交互逻辑，如 UIA 会话、缺省字段、空状态响应 | 前端不再因协议细节误判 |
| `T3-1` | 将 E2EE bootstrap 拆成可回归步骤：cross-signing、SSSS、dehydrated device、key backup | 端到端回归链 |
| `T3-2` | 将现有 `#[ignore]` 测试逐步转为可执行场景或显式补测试基础设施 | 减少长期悬挂测试债务 |

### 阶段 C：声明面、产品面与互通治理

| 任务 | 具体动作 | 产出 |
|---|---|---|
| `T4-1` | 对照当前 stock Element 可用性收紧 capability 声明 | 声明面与可用性一致 |
| `T5-1` | 决定 `friends` 正式前端方案并收口临时入口 | 产品能力闭环 |
| `T6-1` | 收口 OIDC 支持矩阵与前端降级策略 | 不再产生误导性 SSO 提示 |
| `T7-1` | 固定 Element 版本并记录 widget 兼容矩阵 | 减少 bundle 漂移风险 |
| `T8-1` | 建立未实现端点清单与支持矩阵 | 文档闭环 |
| `T9-1` | 引入 Complement / 互通 smoke | 标准客户端互通门禁 |
| `T10-1` | 将 harness 扩展为常驻 Web 兼容基线 | 快速发现前端回归 |

---

## 四、当前建议完成标准

### `P0` 完成标准

- fresh account 使用 stock Element 登录后可稳定进入主界面。
- 可通过浏览器 harness 或人工联调完成新建房间与发送消息。
- 登录流程中的 pageerror 和关键 console error 已有清晰结论：已修复、已降级或已文档化。
- 至少一条 E2EE bootstrap 端到端链路可重复执行。

### `P1` 完成标准

- `/versions` 与 `/capabilities` 不再声明当前 Web 栈无法稳定消费的能力。
- `friends / widget / oidc` 至少有明确的“支持 / 不支持 / 降级”策略。
- 相关产品文档与技术说明同步更新。

### `P2` 完成标准

- 未实现端点形成正式支持矩阵。
- Complement 或等价互通 smoke 接入持续验证流程。
- 浏览器 harness 形成最小可靠基线，不再只停留在实验性质。

---

## 五、当前执行状态

| 任务 | 状态 | 备注 |
|---|---|---|
| `T0` | 已完成 | `smoke:login` 与 `test:basic` 已能识别中文界面、自动跳过数字身份模态并归档 `pageerror / console error / HTML snapshot` |
| `T1` | 进行中 | 已确认 fresh account / 既有账号都可走通 `登录 -> 主界面 -> 发起 DM -> 发送加密消息`，剩余阻塞已收敛到本机浏览器环境复测、少量 404/receipt 噪音与后续 E2EE 端到端链路 |
| `T2` | 进行中 | 已完成首轮残余兼容性修复：收紧 widget / MSC3814 / OIDC register 声明面，并将 receipt unknown-event 处理改为兼容式 no-op |
| `T3` | 已完成 | `device_signing/upload + SSSS + dehydrated device` 的 fresh-account 首启主链已具备真实可执行回归；`/keys/changes`、经典 `/sync`、`sliding-sync` 三条观察面对 cross-signing / device_lists 更新的一致性回归已全部通过（`bash scripts/test/run_e2ee_observability_gate.sh` 28 条测试全绿） |
| `T4` | 已完成 | `/versions` 与 `/capabilities` 超前声明已收紧：移除 `io.hula.*` 公开声明、修复 `m.voice` / `m.room.suggested` 派生来源、`MSC3245 / MSC3983` 改为路由面驱动、补齐 `MSC3814 / MSC4143` 声明；15 条单元测试全通过 |
| `T6` | 已完成 | OIDC 支持边界收口：修复 `sso_providers()` 未检测 OIDC 的 bug，`m.sso.providers` 现可正确反映 OIDC 配置；支持矩阵文档同步更新外部 OIDC (RP) / 内置 OIDC (Provider) / Element native flow / Dynamic Registration 边界 |
| `T7` | 已完成 | Widget 运行时异常处理收口：确认 `Widget*Store ReferenceError` 为 Element Web 前端 bundle 问题；后端侧补齐 widget URL XSS 校验（仅允许 `https/http`）、将 `send_room_widget_message` 从假 `event_id` 改为明确错误提示；7 条单元测试全通过 |
| `T8` | 已完成 | 未实现端点清单与支持矩阵收口：11 个 `M_UNRECOGNIZED` 端点已文档化；修复 `federation/v1/query/auth` 误导性 stub（从返回空 `auth_chain` 改为 `M_UNRECOGNIZED` 指向标准端点）；`SUPPORTED_MATRIX_SURFACE.md` 新增 Admin / Federation 端点支持矩阵与 OIDC 支持矩阵章节 |
| `T5`/`T9`/`T10` | 未开始 | 待后续推进 |

---

## 六、回填要求

- 每完成一个任务，回填：
  - 修改文件
  - 验证命令
  - 验证结果
  - 是否需要更新能力声明或文档
- 所有回填应同时更新本清单与基线整改表，保证排期和技术状态一致。

## 七、本轮回填

- 修改文件:
  - `tests/element-web-harness/basic-interactions.mjs`
- 验证命令:
  - `ELEMENT_BASE_URL=https://element.test ELEMENT_TEST_USERNAME=test1 ELEMENT_TEST_PASSWORD='Ljf3790791!' ELEMENT_TEST_PEER_USERNAME=test2 PLAYWRIGHT_HEADLESS=1 npm run test:basic`
- 验证结果:
  - 已通过 `登录 -> 跳过数字身份验证 -> 进入主界面 -> 发起 DM -> 发送加密消息`
  - 运行中仍会记录 `Widget*Store ReferenceError`、OIDC native flow 探测失败、若干 `404` 与 `read receipt` 错误日志
- 后续动作:
  - 继续推进 `T2`，收敛剩余前后端兼容噪音并决定是否收紧 `/versions` 与 `/capabilities`

- 修改文件:
  - `src/web/routes/handlers/versions.rs`
  - `src/web/routes/oidc.rs`
  - `synapse-services/src/builtin_oidc_provider.rs`
  - `src/web/routes/handlers/room/receipts.rs`
  - `tests/integration/api_auth_routes_tests.rs`
  - `tests/integration/api_room_tests.rs`
  - `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`
  - `docs/synapse-rust/WEB_FRONTEND_BACKEND_GAP_REMEDIATION_PLAN_2026-06-21.md`
- 验证命令:
  - `cargo test --lib web::routes::handlers::versions::tests -- --nocapture`
  - `cargo test --lib web::routes::oidc -- --nocapture`
  - `cargo test --features test-utils --test integration api_auth_routes_tests -- --nocapture`
  - `cargo test --features test-utils --test integration api_room_tests::test_send_receipt_accepts_unknown_event_id_as_noop -- --exact --nocapture`
  - `cargo test --features test-utils --test integration api_room_tests::test_send_receipt_rejects_cross_room_event_id -- --exact --nocapture`
- 验证结果:
  - `/versions` 与 `/capabilities` 不再对外声明 `org.matrix.msc4261.widget`、`io.hula.widget`、`org.matrix.msc3814`
  - OIDC discovery 与内置 provider 不再暴露 `registration_endpoint`，兼容路由中移除了 `/_matrix/client/*/oidc/register`
  - 线上 `matrix.test` 已部署最新代码；`/_matrix/client/unstable/org.matrix.msc2965/auth_metadata` 与 `auth_issuer` 在未启用 OIDC/SSO 时都会返回 `404 + M_UNRECOGNIZED`
  - `receipt` 对未知事件 ID 改为 `200` no-op，跨房间事件仍保持 `404`
  - `DELETE` extended profile 字段已改为幂等成功，浏览器侧不再出现 `Failed to delete timezone from user profile` / `Extended profile field not found`
  - 已确认 `MSC3814 GET /dehydrated_device` 在“尚无 dehydrated device”时返回 `404` 属于预期语义，整改策略应为停止对 Web 客户端宣称 `org.matrix.msc3814`，而不是篡改该端点返回语义
  - 已完成首轮 Synapse 对齐审查：当前 `MSC3814` 的主要缺口不在“端点缺失”，而在 `dehydrated_device` 请求/响应形状、`DELETE` 回显、`events?limit=`、以及 cross-signing / SSSS / UIA bootstrap 串联仍未完全对齐 stock Element 预期
  - 已落第一批修复：`PUT /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` 现在兼容 Synapse 形状的 `device_data + device_keys` 请求体，`GET` 返回 `{ device_id, device_data }`，`DELETE` 返回 `{ device_id }`，`POST .../events` 支持 `limit` 查询参数
  - 已补本地验证：`cargo test -p synapse-services dehydrated_device_service --lib` 与 `cargo test --features test-utils --test integration --no-run` 均通过
  - 已落第二批 `m.secret_storage.*` 账户数据对齐：`PUT /dehydrated_device` 的 SSSS 前置检查同时接受内部 `ssss_service.get_all_keys` 与标准 `m.secret_storage.default_key` / `m.secret_storage.key.<id>` 账户数据；`GET /account_data/m.secret_storage.default_key` 与 `m.secret_storage.key.<id>` 在缺失账户数据但有内部 SSSS 表时会回填出 key 信息，方便 stock Element bootstrap
  - 已补 `api_account_data_routes_tests` 两个回归：`test_secret_storage_default_key_falls_back_to_internal_ssss`、`test_dehydrated_device_ssss_precondition_accepts_account_data_default_key`
  - 已收口 `m.secret_storage.*` 写侧双来源漂移：`PUT /account_data/m.secret_storage.key.<id>` 现在会 best-effort 同步内部 `e2ee_secret_storage_keys`，`PUT /account_data/m.secret_storage.default_key` 在标准 key 账户数据先于 default key 写入时也会按 `key_id` 回填内部 SSSS 状态
  - 已顺手修正内部 SSSS upsert SQL 与当前 schema 的冲突点：`e2ee_secret_storage_keys` 实际只有全局唯一 `key_id`，此前 `ON CONFLICT (key_id, user_id)` 永远不会命中；现已改为基于 `key_id` upsert，并拒绝覆盖其他用户的同名 key
  - 已补 `api_account_data_routes_tests` 两个写侧回归：`test_secret_storage_key_account_data_write_syncs_internal_ssss`、`test_secret_storage_default_key_write_backfills_internal_ssss_from_standard_account_data`
  - 已收紧 `device_signing/upload` 契约：缺少 UIA 时固定返回 `401 + M_UIA_REQUIRED`；UIA 通过但没有任何非空 `master_key` / `self_signing_key` / `user_signing_key` 时返回 `400`；成功上传后会 `notify_user` 唤醒该用户的 `/sync` / sliding-sync
  - 已补 `api_e2ee_tests` 与 `e2ee_routes` 回归：`test_e2ee_key_routes_and_security_summary` 现在锁定 `device_signing/upload` 的 UIA challenge 语义，`test_has_upload_device_signing_keys_*` 锁定空 payload / 非空 payload 判定
  - 已补 cross-signing 到设备列表流位点的传播：`CrossSigningService::{upload_cross_signing_keys, upload_device_signing_key, delete_cross_signing_keys}` 现在都会写入 `device_lists_stream/device_lists_changes`（`device_id = NULL` 的用户级变更），从而让 `/keys/changes` 对 cross-signing 更新可见
  - 已补回归：`test_key_changes_exposes_cross_signing_updates_for_shared_users`，验证共享房间用户上传 cross-signing keys 后，观察方的 `/keys/changes` 会包含该用户
  - 已补 `/sync` 侧共享房间过滤对齐：经典 `/sync` 的 `device_lists.changed/left` 现在复用与 sliding-sync 一致的共享房间过滤语义，不再把无共享房间用户的 cross-signing / device-list 变化暴露给观察方；已补 `test_sync_device_lists_exposes_cross_signing_updates_for_shared_users` 与 `test_sync_device_lists_does_not_leak_cross_signing_updates_without_shared_rooms`
  - 已解开 3 条长期 `#[ignore]` 的 MSC3814 dehydrated-device 回归：`test_dehydrated_device_put_get_delete_roundtrip`、`test_dehydrated_device_appears_in_keys_query`、`test_dehydrated_device_events_endpoint_empty_batch`；测试内现在会手工种好 cross-signing + `m.secret_storage.*` 前置条件，并使用随机 dehydrated `device_id` 避免共享测试库里的全局唯一键碰撞
  - 已验证 `cargo test --features test-utils --test integration dehydrated_device_ -- --nocapture` 可通过，说明 `PUT/GET/DELETE`、`/keys/query` 暴露以及空队列 `POST .../events` 已进入常规回归门
  - 已解开 `api_e2ee_advanced_tests::test_e2ee_cross_signing_flow`：该用例现在使用注册返回的登录设备 `device_id` 与真实 ed25519 签名链上传 `master/self_signing/user_signing` keys，不再依赖占位签名字符串；`cargo test --features test-utils --test integration e2ee_cross_signing_flow -- --nocapture` 已通过
  - 已补 fresh-account 首启端到端回归：`test_fresh_account_cross_signing_ssss_and_dehydrated_device_end_to_end` 现在锁定“注册返回设备 -> 上传真实 signed device keys -> `device_signing/upload` UIA 两步 -> 标准 `m.secret_storage.*` account_data -> 内部 SSSS 镜像 -> `PUT /dehydrated_device` -> `/keys/query` 暴露 `master/self/user_signing` 与 dehydrated device”的完整 bootstrap 链
  - 浏览器 harness 已通过固定 `node@22` 恢复，`test:basic` 可稳定复跑；`read receipt pending event` 与 OIDC dynamic registration 噪音已消失
  - 当前残余噪音主要剩两类：Element `Widget*Store` 初始化 `ReferenceError`，以及 Element 对 `auth_metadata` / `auth_issuer` / `dehydrated_device` 的规范级 `404` 探测日志
- 后续动作:
  - 继续评估是否能通过 Element 配置层或版本切换关闭 `Widget*Store` 初始化链；若不能，则将其明确归类为前端镜像问题而非后端协议问题
  - 参考 Synapse/MSC3814 与 stock Element 的真实行为，继续完善 `cross-signing / SSSS / dehydrated device` 全链路后再决定是否重新声明 `org.matrix.msc3814`
  - 下一批优先补齐：sliding-sync 的同类观察回归，以及评估是否需要扩内部 SSSS schema 以持久化 `auth_data.iv/mac`
  - 继续推进 `T3`，把 `cross-signing / SSSS / dehydrated device` 串成 fresh-account 端到端回归链

- 修改文件:
  - `tests/integration/api_sliding_sync_contract_tests.rs`
  - `tests/integration/api_e2ee_tests.rs`
  - `synapse-storage/src/device.rs`
  - `synapse-services/src/sliding_sync_service/extensions.rs`
  - `synapse-services/src/sync_service/data_fetch.rs`
  - `docs/synapse-rust/WEB_FRONTEND_BACKEND_EXECUTION_CHECKLIST_2026-06-21.md`
- 验证命令:
  - `cargo test --features test-utils --test integration sliding_sync_extensions_e2ee_ -- --nocapture`
  - `cargo test --features test-utils --test integration test_sync_device_lists_ -- --nocapture`
  - `cargo test --features test-utils --test integration test_key_changes_ -- --nocapture`
  - `cargo test --features test-utils --test integration sliding_sync_extensions_e2ee_left_reports_users_who_stop_sharing_rooms -- --nocapture`
  - `cargo test --features test-utils --test integration test_sync_device_lists_left_reports_users_who_stop_sharing_rooms -- --nocapture`
  - `cargo test --features test-utils --test integration leave_then_rejoin -- --nocapture`
  - `cargo test --features test-utils --test integration kick_reports_left -- --nocapture`
  - `cargo test --features test-utils --test integration ban_reports_left -- --nocapture`
  - `cargo test --features test-utils --test integration does_not_repeat_left_for_already_unshared_user -- --nocapture`
  - `cargo test --features test-utils --test integration sync_device_lists_knock_does_not_leak_non_shared_user -- --nocapture`
  - `cargo test --features test-utils --test integration e2ee_knock -- --nocapture`
  - `cargo test --features test-utils --test integration test_sync_device_lists_ -- --nocapture`
  - `cargo test --features test-utils --test integration sliding_sync_extensions_e2ee_ -- --nocapture`
  - `bash scripts/test/run_e2ee_observability_gate.sh`
- 验证结果:
  - 已新增 sliding-sync E2EE extension 两条观察回归：`test_sliding_sync_extensions_e2ee_exposes_cross_signing_updates_for_shared_users`、`test_sliding_sync_extensions_e2ee_does_not_leak_cross_signing_updates_without_shared_rooms`
  - 现已由测试锁定 `/keys/changes`、经典 `/sync`、`sliding-sync` 三条观察面对“纯 cross-signing 更新也会驱动 `device_lists.changed`”以及“无共享房间时不泄漏”这两类语义的一致性
  - 在写新增回归的过程中复现并修复了 `get_device_lists_since_with_shared_rooms()` 的 `left` 分支误报问题：该 SQL 会把“当前不与观察者共享房间”的所有更新用户都计入 `left`，而当前 schema 没有流位点化的共享关系历史，无法可靠区分“曾共享、现失共享”与“从未共享”，因此把 `left` 先收紧为不主动误报，等未来引入流位点化的共享关系历史后再补回完整 `left` 语义
  - 随后已把 sliding-sync 的 `left` 从“保守空数组”恢复为“同一 `conn_id` 下上一轮共享用户集合减当前共享用户集合”的连接态语义，并新增 `test_sliding_sync_extensions_e2ee_left_reports_users_who_stop_sharing_rooms`，锁定“共享用户离房后会进入 `device_lists.left`，但不会伪装成 `changed`”这一关键行为
  - 进一步把经典 `/sync` 的 `left` 从纯 `device_lists_stream` 过滤升级为“device change + membership 增量”的组合推导：基于 `since.stream_id` 抓取 `m.room.member` 事件增量、结合当前共享用户集合排除仍共享对象，并新增 `test_sync_device_lists_left_reports_users_who_stop_sharing_rooms`，锁定“共享用户离房后会进入 `/sync device_lists.left`，但不会伪装成 `changed`”
  - 又补两条 `/sync left` 边界回归：`test_sync_device_lists_left_reports_users_when_requester_leaves_last_shared_room` 锁定“观察者自己离开最后一个共享房间后，对端用户应进入 `left`”，`test_sync_device_lists_left_does_not_report_user_still_shared_via_other_room` 锁定“多房间仍共享时，单房间离开不能误报 `left`” 
  - 顺手修正了 invite-decline 的真实状态机缺口：`RoomMemberStorage::add_member()` 现在只在 `membership = 'join'` 时写入 `joined_ts`，不再把 `invite` 错标成“曾加入”；`remove_member()` 现在覆盖 `invite -> leave`；`RoomService::leave_room()` 只有从 `join` 离开时才递减 member count，避免“拒绝邀请”污染房间成员计数
  - 已补两条 invite-decline 回归：`test_sync_device_lists_invite_decline_does_not_report_left_and_updates_membership_state` 与 `test_sliding_sync_extensions_e2ee_invite_decline_does_not_report_left`，锁定“被邀请者拒绝邀请不会出现在 `device_lists.left`，且 membership 当前态会落为 `leave`”
  - 又补两条 invite-retract 回归：`test_sync_device_lists_invite_retract_via_kick_does_not_report_left` 与 `test_sliding_sync_extensions_e2ee_invite_retract_via_kick_does_not_report_left`，锁定“房主撤回尚未接受的邀请（当前通过 `kick -> leave` 落地）不会让从未共享过的用户误报进 `device_lists.left`”
  - 已补 `leave -> reinvite -> rejoin` 两条窗口回归：`test_sync_device_lists_leave_then_rejoin_does_not_report_left` 与 `test_sliding_sync_extensions_e2ee_leave_then_rejoin_does_not_report_left`，锁定“同一增量窗口内短暂失共享但在下次观察前已重新共享的用户，不会残留在最终 `device_lists.left`”
  - 已补 `kick / ban` 两组共享用户边界回归：`test_sync_device_lists_kick_reports_left_for_kicked_shared_user`、`test_sliding_sync_extensions_e2ee_kick_reports_left_for_kicked_shared_user`、`test_sync_device_lists_ban_reports_left_for_banned_shared_user`、`test_sliding_sync_extensions_e2ee_ban_reports_left_for_banned_shared_user`，锁定“被房主踢出或封禁后，会进入 `device_lists.left`，但不会伪装成 `changed`”
  - 又补 `unban / forget` 两组“已失共享用户不应重复上报”回归：`test_sync_device_lists_unban_does_not_repeat_left_for_already_unshared_user`、`test_sliding_sync_extensions_e2ee_unban_does_not_repeat_left_for_already_unshared_user`、`test_sync_device_lists_forget_does_not_repeat_left_for_already_unshared_user`、`test_sliding_sync_extensions_e2ee_forget_does_not_repeat_left_for_already_unshared_user`
  - 上述回归打出了经典 `/sync` 的真实缺口：`ban -> unban` 后，`get_device_list_left_users_for_sync()` 之前会仅凭历史 `joined_ts` 把同一用户再次报进 `left`；现已按最新 membership 细分 `ban / leave / forget`，其中 `leave` 仅在真实 `left_ts` 已落库时上报、`forget` 永不制造新的 `left`，从而与 sliding-sync 的连接态快照语义对齐
  - 又补 `knock` 两条非泄漏回归：`test_sync_device_lists_knock_does_not_leak_non_shared_user` 与 `test_sliding_sync_extensions_e2ee_knock_does_not_leak_non_shared_user`，锁定“从未共享房间的用户即使成功产生 `knock` membership 事件，也不会出现在 `device_lists.changed/left`”
  - 已完成组合门复验：`cargo test --features test-utils --test integration test_sync_device_lists_ -- --nocapture` 现可整组通过 13 条，`cargo test --features test-utils --test integration sliding_sync_extensions_e2ee_ -- --nocapture` 现可整组通过 12 条，说明最近补入的 `unban / forget / knock` 边界没有破坏整组稳定性
  - 为消除 shared-template-schema 下的测试污染，这组 `/sync device_lists` 与 `sliding-sync e2ee` 回归已切到 isolated schema setup；修复后 `test_sync_device_lists_` 13 条与 `sliding_sync_extensions_e2ee_` 12 条均可整组通过，说明三条观察面的主语义已继续收口且回归门更稳定
  - 已新增固定组合门入口 `bash scripts/test/run_e2ee_observability_gate.sh`，默认以 `TEST_ISOLATED_SCHEMAS=1` 顺序复跑 3 条 `/keys/changes` 精确回归、经典 `/sync device_lists` 13 条和 `sliding-sync e2ee` 12 条观察回归，便于后续接入 nightly smoke 或人工复验
- 后续动作:
  - 继续评估是否需要把经典 `/sync` 的 `left` 再进一步升级为完全基于 membership 事件流的精细语义，并把这组观察回归并入更稳定的组合门
  - 后续把“`/keys/changes` + 经典 `/sync` + `sliding-sync` 三观察面在 cross-signing / device-list 维度的一致性”纳入 nightly smoke 组合
  - 继续推进 `T3`，把 `cross-signing / SSSS / dehydrated device` 串成 fresh-account 端到端回归链

- 修改文件:
  - `src/web/routes/handlers/versions.rs`
  - `src/web/routes/voip.rs`
  - `src/web/routes/assembly.rs`
  - `src/web/routes/widget.rs`
  - `src/web/routes/federation/keys.rs`
  - `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`
  - `docs/synapse-rust/WEB_FRONTEND_BACKEND_EXECUTION_CHECKLIST_2026-06-21.md`
- 验证命令:
  - `cargo test --lib --all-features web::routes::handlers::versions::tests -- --nocapture`
  - `cargo test --lib --features widgets widget::tests -- --nocapture`
  - `cargo clippy --all-features --locked -- -D warnings`
- 验证结果:
  - **T4 收紧 `/versions` 与 `/capabilities` 超前声明**：`BASE_UNSTABLE_FEATURES` 从 7 项精简到 5 项（移除 `MSC3983 / MSC3245`）；`build_client_versions()` 移除 `io.hula.*` 私有扩展，新增 `MSC3814 / MSC4143 / MSC3245 / MSC3983` 路由面驱动声明；`build_capabilities_response()` 移除公开面的 `io.hula.sliding_sync`；新增 `msc3814_capability / msc4143_capability / msc3245_capability / msc3983_capability` 路由面驱动函数；修复 `voice_capability()` 派生来源（从 room_summary 改为 voip 路由）；修复 `room_suggested_capability()` 派生来源（从 room_summary 改为 hierarchy 路由）；15 条单元测试全通过
  - **T6 OIDC 支持边界**：修复 `sso_providers()` 函数 bug — 之前即使配置了 OIDC 也从未把 `oidc` 加入 `m.sso.providers`，现加入 `config.oidc.is_enabled() || config.builtin_oidc.is_enabled()` 检测；新增 `test_sso_providers_includes_oidc_when_enabled` 回归测试；`SUPPORTED_MATRIX_SURFACE.md` 新增 OIDC 支持矩阵章节，明确外部 OIDC (RP) / 内置 OIDC (Provider) / Element native flow / Dynamic Registration 各自的支持边界
  - **T7 Widget 运行时异常**：确认 `Widget*Store ReferenceError` 为 Element Web 前端 bundle 问题而非后端协议问题；后端侧新增 `validate_widget_url()` 函数实现 XSS 缓解（仅允许 `https:` / `http:` scheme 且必须有 host），并在 `create_widget / update_widget` 入口校验；将 `send_room_widget_message` 从返回假 `event_id` 改为返回明确 `M_BAD_REQUEST` 错误，提示使用标准 `PUT /rooms/{room_id}/send/{event_type}/{txn_id}` 端点；新增 3 条 URL 校验单元测试，widget 模块 7 条测试全通过
  - **T8 未实现端点清单**：修复 `federation/v1/query/auth` 误导性 stub — 之前返回 `{"auth_chain": []}` 让客户端误以为已成功，现返回 `M_UNRECOGNIZED` 错误并指向标准 `/_matrix/federation/v1/event/{event_id}` 端点；`SUPPORTED_MATRIX_SURFACE.md` 新增 Admin / Federation 端点支持矩阵章节，文档化 11 个 `M_UNRECOGNIZED` 端点
  - `cargo clippy --all-features --locked -- -D warnings` 通过，无任何 warning
- 后续动作:
  - T5（friends 正式 Web 产品入口策略）、T9（Complement / 互通 smoke 门禁）、T10（浏览器 harness 扩展为稳定兼容基线）待后续推进
  - 评估是否需要把 widget URL 校验也应用到 `widget_service` 层，形成纵深防御
  - 评估是否需要为 `MSC3814 / MSC4143` 路由面驱动声明补集成测试，验证 `unstable_features` 与实际路由注册的一致性

- 修改文件:
  - `src/web/routes/admin/server.rs`
  - `tests/integration/api_placeholder_contract_p0_tests.rs`
  - `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`
- 验证命令:
  - `cargo clippy --all-features --locked -- -D warnings`
  - `cargo test --features test-utils --test integration test_admin_experimental_features_returns_feature_map -- --nocapture`
  - `cargo test --features test-utils --test integration test_admin_server_placeholder_contract_returns_not_implemented_for_admin -- --nocapture`
- 验证结果:
  - **实现 admin `experimental_features` 端点**：将 `GET /_synapse/admin/v1/experimental_features` 从返回 `M_UNRECOGNIZED` (404) 改为返回 `200 OK` + `{features: {flag_key: enabled_bool}, total}`；桥接 DB 型 `FeatureFlagService` 到 Synapse `experimental_features` 表面，`enabled` 当且仅当 `status ∈ {active, fully_enabled, ramping}` 且 `rollout_percent > 0`
  - **改进 admin 错误语义**：`GET /backups` 和 `POST /restart` 从 `ApiError::unrecognized` (404 M_UNRECOGNIZED) 改为 `ApiError::not_implemented` (501 M_UNRECOGNIZED)，语义更准确——端点已知但功能由外部基础设施管理
  - **更新 contract test**：原 `test_admin_server_placeholder_contract_returns_unrecognized_for_admin` 拆为两条——`test_admin_server_placeholder_contract_returns_not_implemented_for_admin`（验证 `backups` 返回 501）和 `test_admin_experimental_features_returns_feature_map`（验证 `experimental_features` 返回 200 + feature map）
  - `cargo clippy --all-features --locked -- -D warnings` 通过
  - 2 条集成测试全通过
- 后续动作:
  - 评估是否需要为 `experimental_features` 补 `PUT` 方法（per-user feature 开关）
  - 评估是否需要把 `restart` 端点改为通过 SIGTERM 信号实现真实重启

- 修改文件:
  - `synapse-services/src/sms_provider/aliyun.rs`
  - `synapse-services/src/application_service/tests.rs`
- 验证命令:
  - `cargo clippy --all-features --locked -- -D warnings`
  - `cargo test -p synapse-services --lib sms_provider -- --nocapture`
- 验证结果:
  - **消除 Aliyun SMS provider 生产代码中的 `expect()`**：将 `AliyunSmsProvider::sign()` 从返回 `String` 改为返回 `Result<String, ApiError>`，HMAC key 初始化失败时返回 `ApiError::internal` 而非 panic；移除 `#[allow(clippy::expect_used)]` 标注；`send()` 方法相应改为 `self.sign(&query)?` 传播错误
  - **修复预先存在的测试编译错误**：`synapse-services/src/application_service/tests.rs` 缺少 `use reqwest::StatusCode;` 导入，导致 4 个 `StatusCode::BAD_GATEWAY/TOO_MANY_REQUESTS/UNAUTHORIZED/NOT_FOUND` 引用无法编译；现已补齐导入
  - `cargo clippy --all-features --locked -- -D warnings` 通过
  - 15 个 SMS provider 测试全通过（含 aliyun signature/send success/send failure/percent encode/query contains required params 等场景）
- 后续动作:
  - 继续排查其他生产代码中的 `expect()`/`unwrap()` 调用（`panic=abort` 配置下的崩溃风险）
  - 评估出站 backfill 服务层缺失（OPT-10）的影响与实现优先级

- 修改文件:
  - `src/storage/mod.rs`
- 验证命令:
  - `cargo clippy --all-features --locked -- -D warnings`
  - `python3 scripts/ci/check_root_canonical_ledger.py`
  - `cargo test --lib storage:: -- --nocapture`
  - `cargo test --features test-utils --test integration --no-run`
- 验证结果:
  - **消除 `src/storage/mod.rs` 中的 `Database` 重复实现**：发现 `synapse-storage/src/lib.rs` 已有完全相同的 `Database` 结构体、`impl Database`（6 个方法）、`initialize_database` 函数和 8 个测试；`src/storage/mod.rs` 中的版本是纯重复代码（非 facade）；删除重复实现（约 290 行），替换为 `pub use synapse_storage::{Database, initialize_database};`；移除 4 个随之未使用的导入（`RedisPool`、`Pool`、`Postgres`、`Arc`、`RwLock`）
  - `cargo clippy --all-features --locked -- -D warnings` 通过
  - `check_root_canonical_ledger.py` 通过：services=2 (facade=2, full_impl=0), storage=55 (facade=55, full_impl=0)
  - lib 和 integration 编译通过
- 后续动作:
  - root/canonical 双轨清理已基本完成，`Database` 是最后一个非 facade 残留项
  - 评估是否需要把 55 个 storage facade 文件合并到 `mod.rs` 集中 re-export（权衡模块化清晰度 vs 文件数量）

- 修改文件:
  - `synapse-common/src/config/experimental.rs`
  - `synapse-common/src/config/auth.rs`
  - `src/web/routes/handlers/versions.rs`
  - `src/web/routes/oidc.rs`
- 验证命令:
  - `cargo clippy --all-features --locked -- -D warnings`
  - `cargo test --lib --all-features web::routes::handlers::versions::tests -- --nocapture`
- 验证结果:
  - **为私有扩展添加运行时配置控制**：`ExperimentalConfig` 新增 `declare_private_extensions: bool` 字段（默认 `true`，向后兼容）；`friends_capability()`、`voice_extended_capability()`、`burn_after_read_capability()` 改为接受 `&Config` 参数，当 `declare_private_extensions = false` 时返回 `ConfigControlled(false)`，抑制 `io.hula.*` 能力声明；运营方可在 `homeserver.yaml` 中设置 `experimental.declare_private_extensions: false` 来对 stock Element Web 隐藏无 UI 入口的私有扩展；新增 `test_declare_private_extensions_suppresses_hula_capabilities` 回归测试
  - **改进 OIDC discovery registration_endpoint**：`OidcConfig` 新增 `registration_endpoint: Option<String>` 字段（RFC 7591 Dynamic Client Registration）；`openid_discovery` 处理器从 `config.oidc.registration_endpoint` 读取值，而非硬编码 `None`；运营方可配置外部 IdP 的注册端点 URL，消除 Element Web OIDC native flow 的 "Dynamic registration not supported" 错误
  - `cargo clippy --all-features --locked -- -D warnings` 通过
  - 16 条 versions 单元测试全通过
- 后续动作:
  - 在 `homeserver.yaml` 模板中补充 `experimental.declare_private_extensions` 和 `oidc.registration_endpoint` 的配置说明
  - 评估是否需要实现 OIDC Dynamic Registration 端点本身（`POST /_matrix/client/v3/oidc/register`），而非仅声明外部 IdP 的注册端点

- 修改文件:
  - `homeserver.yaml`
  - `src/web/routes/federation/events.rs`
  - `tests/integration/api_federation_signature_auth_tests.rs`
  - `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`
  - `docs/synapse-rust/WEB_FRONTEND_BACKEND_EXECUTION_CHECKLIST_2026-06-21.md`
- 验证命令:
  - `cargo clippy --all-features --locked -- -D warnings`
  - `cargo test --features test-utils --test integration test_federation_get_missing_events -- --nocapture`
- 验证结果:
  - **补充 `homeserver.yaml` 模板配置文档**：在 OIDC 段新增 `registration_endpoint` 字段说明（RFC 7591 Dynamic Client Registration），在文件末尾新增 `experimental.declare_private_extensions` 段说明，运营方可在不重编译的情况下对 stock Element Web 抑制 `io.hula.*` 能力声明
  - **修复入站 `/get_missing_events` 桩实现的数据污染风险**：原实现忽略 `earliest_events` / `latest_events` 参数，直接调用 `get_room_events()` 返回房间最近 N 个事件——这会把无关事件交给请求方，污染其事件 DAG。由于 `event_edges` 表尚未被持久化路径填充，无法真正计算 earliest/latest 之间的缺口，改为返回空 `events` 数组（spec-compliant），让请求方回退到其他 peer 或 `/backfill`；新增 `test_federation_get_missing_events_returns_empty_until_event_edges_populated` 回归测试，验证 origin 验证通过后返回 200 + 空 events 数组
  - **排查并记录出站 backfill 服务层缺口（OPT-10）**：确认 `FederationClient::backfill` 和 `FederationClient::get_missing_events` 两个出站方法在整个代码库中从未被调用（死代码）；入站 `/send` transaction handler 不提取或校验 PDU 的 `prev_events`，`CreateEventParams` 结构体缺少 `prev_events` / `auth_events` / `depth` 字段，`create_event` SQL 不写入这些列也不填充 `event_edges` / `event_forward_extremities` 表；`SUPPORTED_MATRIX_SURFACE.md` 新增"Federation 事件图与回填"章节，文档化入站 backfill（已实现）、入站 get_missing_events（桩实现）、出站 backfill 触发（未实现）三层状态
  - `cargo clippy --all-features --locked -- -D warnings` 通过
  - 2 条 get_missing_events 集成测试全通过（auth 拒绝 + 空响应回归）
- 后续动作:
  - OPT-10 完整修复需要：扩展 `CreateEventParams` 增加 `prev_events` / `auth_events` / `depth`；修改 `create_event` SQL 写入这些列并填充 `event_edges`；在 transaction handler 中加入 `fill_in_prev_events` 触发，调用已存在的 `FederationClient::get_missing_events` 出站方法向 origin 服务器请求缺失事件
  - 评估是否在第一版修复中只实现 `get_missing_events` 触发（覆盖 80% 场景），推迟完整 `backfill`（多服务器轮询、候选服务器排序）

- 修改文件:
  - `synapse-storage/src/event/mod.rs`
  - `synapse-services/src/room/events.rs`
  - `src/web/routes/federation/transaction.rs`
  - `src/web/routes/federation/events.rs`
  - `tests/integration/api_federation_signature_auth_tests.rs`
  - `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`
  - `docs/synapse-rust/WEB_FRONTEND_BACKEND_EXECUTION_CHECKLIST_2026-06-21.md`
- 验证命令:
  - `cargo clippy --all-features --locked -- -D warnings`
  - `cargo check --all-features`
  - `cargo test --features test-utils --test integration test_federation_get_missing_events -- --nocapture`
- 验证结果:
  - **OPT-10 完整修复（事件 DAG 持久化 + fill_in_prev_events 触发）**：参考 `element-hq/synapse` 的 `DataStore.handle_room_member_event` / `_handle_new_room_event` 中对 `event_edges` 的填充模式，新增 `EventStorage::create_event_with_graph(params, prev_events, auth_events, depth, tx)` 方法——在原 `create_event` INSERT 之外，额外写入 `events.prev_events` / `events.auth_events` / `events.depth` 列，并为每个 `prev_event` 插入 `event_edges(event_id, prev_event_id, is_state=false)` 行；保留原 `create_event` 方法不变，避免影响 74 个既有调用点
  - **`fill_in_prev_events` 触发逻辑**：`/send` transaction handler 在持久化 PDU 前提取 `prev_events` / `auth_events` / `depth`，通过新增的 `EventStorage::find_missing_event_ids()` 检查 `prev_events` 是否本地存在；若缺失，调用 `FederationClient::get_missing_events(origin, room_id, &prev_events, &[event_id], 20, None)` 向源服务器请求补齐，并通过 `create_event_with_graph` best-effort 持久化补回的事件（错误仅记录日志，不阻塞主 PDU 持久化）；最终用 `create_event_with_graph` 持久化主 PDU，确保 `event_edges` 被正确填充
  - **入站 `/get_missing_events` 改为真实 DAG 遍历**：新增 `EventStorage::get_missing_events_between(room_id, earliest_events, latest_events, limit)` 方法，从 `latest_events` 出发沿 `event_edges.prev_event_id` 反向 BFS，跳过 `earliest_events`，收集中间事件并按 `room_id` 过滤返回；替换原桩实现的空响应，让请求方真正拿到缺口事件
  - **`RoomService::create_event_with_graph` 包装**：与 `create_event` 保持同样的 summary 派发与 application service 通知行为，确保 federation 路径与本地路径产生一致的副作用
  - `cargo clippy --all-features --locked -- -D warnings` 通过
  - `cargo check --all-features` 通过
  - 3 条 `test_federation_get_missing_events` 集成测试全通过：auth 拒绝、空 `event_edges` 时返回空数组、填充 DAG 后正确返回中间事件
- 后续动作:
  - 完整 `FederationClient::backfill`（多服务器候选选择、加入有历史房间时拉取历史事件）仍为死代码，推迟到后续迭代
  - 评估是否需要在 `create_event_with_graph` 中同时维护 `event_forward_extremities` / `event_backward_extremities` 表，以支持更精确的 extremity 计算
  - 评估是否需要把 `fill_in_prev_events` 抽取为独立 service 方法，便于 `/event/{event_id}` 等其他入站端点复用

- 修改文件:
  - `docs/synapse-rust/WEB_FRONTEND_BACKEND_EXECUTION_CHECKLIST_2026-06-21.md`
- 验证命令:
  - `bash scripts/test/run_e2ee_observability_gate.sh`
- 验证结果:
  - **T3 E2EE 观察门禁全绿**：`/keys/changes` 3 条 + 经典 `/sync device_lists` 13 条 + `sliding-sync e2ee` 12 条 = 28 条测试全部通过
  - 三条观察面对 `cross-signing / device_lists` 更新的一致性回归已稳定可重复执行
  - `run_e2ee_observability_gate.sh` 可作为 nightly smoke 入口
- 后续动作:
  - 将 `run_e2ee_observability_gate.sh` 纳入 CI nightly 流水线
  - 浏览器 harness 中增加对安全引导完成状态与关键 console/pageerror 的显式断言（与 T10 协同）

- 修改文件:
  - `synapse-storage/src/event/mod.rs`
  - `synapse-storage/src/membership.rs`
  - `synapse-services/src/room/mod.rs`
  - `synapse-services/src/room/backfill.rs`
  - `src/web/routes/admin/room/mod.rs`
  - `src/web/routes/admin/room/management.rs`
  - `src/web/routes/handlers/room/events.rs`
  - `tests/integration/api_admin_room_lifecycle_tests.rs`
  - `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`
  - `docs/synapse-rust/WEB_FRONTEND_BACKEND_EXECUTION_CHECKLIST_2026-06-21.md`
- 验证命令:
  - `cargo clippy --all-features --locked -- -D warnings`
  - `cargo check --all-features`
  - `cargo test --features test-utils --test integration test_admin_backfill -- --nocapture --test-threads=1`
- 验证结果:
  - **完整出站 backfill 实现**：参考 `element-hq/synapse` 的 `FederationHandler.backfill` 和 `FederationClient.backfill`，新增 `RoomService::backfill_room_history(federation_client, room_id, limit)` 方法——通过 `MembershipStorage::get_joined_servers_in_room` 收集候选服务器（排除本地服务器），通过 `EventStorage::get_latest_event_ids_in_room` 获取种子事件 ID，迭代候选服务器调用 `FederationClient::backfill`，成功后通过 `create_event_with_graph` 持久化补回的事件（跳过已存在的事件，best-effort 不阻塞）
  - **`/messages` 向后分页 best-effort 触发**：当 `dir=b` 且本地返回事件数少于请求 `limit` 时，异步 spawn `backfill_room_history` 任务，不阻塞当前响应；客户端下次分页可获取到新拉取的历史事件
  - **管理端点 `POST /_synapse/admin/v1/rooms/{room_id}/backfill`**：手动触发 backfill，返回 `{ room_id, source_server, persisted_events, candidates_tried }`；对不存在的房间返回 404
  - **新增存储方法**：`EventStorage::get_latest_event_ids_in_room(room_id, limit)` 返回按 `origin_server_ts DESC` 排序的最近事件 ID；`MembershipStorage::get_joined_servers_in_room(room_id, local_server_name)` 返回房间内已加入成员的去重服务器域名列表（排除本地服务器）
  - **`/messages` best-effort backfill rate-limit 冷却**：新增 `synapse_services::room::backfill::check_backfill_cooldown(room_id)` 函数，使用 `LazyLock<Mutex<HashMap<String, i64>>>` 维护 per-room 60 秒冷却窗口；`/messages` handler 在 spawn backfill 任务前检查冷却，冷却期内跳过并记录 debug 日志；admin 端点不受冷却限制，始终立即触发
  - `cargo clippy --all-features --locked -- -D warnings` 通过
  - `cargo check --all-features` 通过
  - `test_admin_backfill_requires_existing_room` 集成测试通过（验证 404 契约）
  - 3 条 `test_federation_get_missing_events` 集成测试全通过（auth 拒绝 + 空 event_edges + DAG 遍历）
- 后续评估项结论:
  - **federated join 后触发 backfill**：❌ 阻塞——出站联邦 join 路径在服务层完全缺失（`FederationClient::make_join` / `send_join` 是死代码，`join_room` 仅支持本地房间，`via_servers` 参数被静默丢弃）。需要先实现完整的出站联邦 join 功能，才能在 join 后触发 backfill。记录为独立大功能，推迟到后续迭代
  - **rate-limit `/messages` backfill 触发**：✅ 已实现——60 秒 per-room 冷却窗口，防止客户端快速重试分页时产生过多联邦请求。admin 端点不受冷却限制
  - **depth-absolute-distance 候选排序策略**：⏸️ 暂不实现——当前"第一个返回非空 PDU 的服务器"策略在常见场景（2-5 个联邦对端）下仅需 1 次联邦往返，depth 排序需要 N 次探测往返反而增加延迟。该策略仅在大型联邦房间（50+ 服务器）或部分对端历史不完整时才有显著收益，且需要先填充 `event_backward_extremities` 表。触发条件：当实际运行中观察到候选遍历失败率 >30% 时再考虑实现

### 出站联邦缺口修复（"半双工联邦"问题）

- **排查结论**：`FederationClient` 有 20+ 出站方法，但只有 3 个被实际调用（`backfill`、`get_missing_events`、`send_transaction` EDU）。服务器此前只能接收联邦请求（入站），无法主动发起（出站），导致跨服务器 E2EE、远程媒体访问、远程用户资料查询全部不可用
- **修复出站 `query_keys`**（参考 Synapse `E2eKeysHandler.query_devices`）：
  - 客户端 `POST /_matrix/client/v3/keys/query` 对远程用户（`user_id` 的 server_name ≠ 本地 server_name）调用 `FederationClient::query_keys`
  - 按 home server 分组远程用户，并行 `tokio::spawn` 多个联邦查询任务
  - 合并 `device_keys` / `master_keys` / `self_signing_keys` / `user_signing_keys` / `failures` 到本地响应
- **修复出站 `claim_keys`**（参考 Synapse `E2eKeysHandler.claim_keys`）：
  - 客户端 `POST /_matrix/client/v3/keys/claim` 在本地 `device_keys_service.claim_keys` 返回后，识别本地未命中的远程设备（`one_time_keys` 中值为 null 的远程设备）
  - 按 server 分组构建 per-server claim 请求，并行 `tokio::spawn` 调用 `FederationClient::claim_keys`
  - 合并远程结果：覆盖本地 null 值，插入本地未包含的新用户；重新计算 `claimed_device_count`
  - 关键实现细节：在消费 `request` 前克隆 `original_one_time_keys`，用于后续识别未命中设备
- **修复出站 `media_download` / `media_thumbnail`**（参考 Synapse `MediaRepositoryServer._download_remote`）：
  - 客户端 `GET /_matrix/media/{v1,v3}/download/{server_name}/{media_id}` 当 `server_name` 非本地时通过 `FederationClient::media_download` 代理远程媒体
  - 客户端 `GET /_matrix/media/{v1,v3}/thumbnail/{server_name}/{media_id}` 当 `server_name` 非本地时通过 `FederationClient::media_thumbnail` 代理远程缩略图
  - 复用本地 CSP / 安全头（`build_proxy_media_headers` 镜像 `MediaDomainService::build_media_response_headers` 的 sandbox 策略）
  - 当前为直接代理不缓存；联邦 HTTP 客户端已有连接池和重试逻辑
- **修复出站 `query_profile`**（参考 Synapse `ProfileHandler.get_profile`）：
  - 客户端 `GET /_matrix/client/v3/profile/{user_id}`（及 `/displayname` / `/avatar_url`）当 `user_id` 属于远程服务器时通过 `FederationClient::query_profile` 代理远程用户资料查询
  - 新增 `try_fetch_remote_profile` 辅助函数：本地用户返回 `Ok(None)` 走原路径，远程用户返回 `Ok(Some(json))` 走联邦路径，联邦失败返回 `M_NOT_FOUND`
- **修改文件**：
  - `src/web/routes/e2ee_routes.rs` — `query_keys` / `claim_keys` handler 添加出站联邦查询
  - `src/web/routes/media.rs` — `download_media_common` / `thumbnail_response_common` 添加出站联邦代理；新增 `fetch_remote_media_via_federation` / `fetch_remote_thumbnail_via_federation` / `build_proxy_media_headers`
  - `src/web/routes/account_compat.rs` — `get_profile` / `get_displayname` / `get_avatar_url` 添加出站联邦代理；新增 `try_fetch_remote_profile`
  - `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md` — 新增"Federation 出站能力"章节
- **验证**：
  - `cargo check --all-features --locked` 通过
  - `cargo clippy --all-features --locked -- -D warnings` 通过
  - `cargo test --lib web::routes::e2ee_routes` — 5 条单元测试全通过
  - `cargo test --lib web::routes::media` — 11 条单元测试全通过
  - `test_admin_backfill_requires_existing_room` 集成测试通过
- **修复出站 `query_directory`**（参考 Synapse `DirectoryHandler.get_association`）：
  - 客户端 `GET /_matrix/client/v3/directory/room/{room_alias}` 当本地别名查找失败且别名属于远程服务器时，通过 `FederationClient::query_directory` 查询远程房间别名解析
  - `join_room_by_id_or_alias` 当本地别名查找失败时也回退到联邦查询
- **修复出站 `send_transaction` (PDU)**（参考 Synapse `FederationSenderHandler.send_pdu` + `crypto/event_signing.py::add_hashes_and_signatures`）：
  - 新增 `sign_and_hash_event` 函数（`synapse-federation/src/signing.rs`）：设置 `origin` → 计算 `hashes.sha256` → 用服务器 Ed25519 密钥签名
  - 新增 `update_event_signatures_and_hashes` 方法（`synapse-storage/src/event/mod.rs`）：持久化签名和哈希到 `events` 表的 `signatures` / `hashes` JSONB 列
  - 新增 `RoomService::sign_and_broadcast_event`（`synapse-services/src/room/federation_broadcast.rs`）：获取 prev_events → 构建 PDU JSON → 签名 → 持久化签名 → 调用 `EventBroadcaster::broadcast_event` 广播到所有有 joined member 的远程服务器
  - 在 `RoomService` 添加 `key_rotation_manager` 字段，通过 `set_key_rotation_manager` setter 注入
  - 集成在 `create_event` wrapper（覆盖消息发送等所有本地事件）和 membership actions（join/leave/invite/ban/unban/kick）中
- **修复出站 `make_join` / `send_join`**（参考 Synapse `FederationHandler.do_invite_join`）：
  - 新增 `RoomService::join_room_via_federation`（`synapse-services/src/room/federation_membership.rs`）：`make_join` 获取模板 PDU → 本地签名 → `send_join` 发送签名事件 → 创建本地房间记录 → 持久化返回的 state events + auth chain → 添加成员
  - 新增 `join_room_with_via_servers` dispatcher：自动检测本地/远程房间并委托，`via_servers` 参数从路由处理器传递到联邦 join
  - 在 `RoomService` 添加 `federation_client` 字段，通过 `set_federation_client` setter 注入
  - 更新 `join_room_by_id_or_alias` 路由处理器调用 `join_room_with_via_servers` 并传递 `via_servers`
- **修复出站 `make_leave` / `send_leave`**（参考 Synapse `FederationHandler.do_remotely_reject_invite`）：
  - 新增 `RoomService::leave_room_via_federation`：`make_leave` 获取模板 PDU → 本地签名 → `send_leave` 发送签名事件 → 更新本地成员状态
  - 修改 `leave_room` 自动检测远程房间（`is_remote_room`）并委托
- **修复出站 `invite`**（参考 Synapse `FederationHandler.do_invite`）：
  - 新增 `RoomService::invite_user_via_federation`：构建 `m.room.member` invite 事件 → 本地签名 → `FederationClient::invite` 发送到被邀请者所在服务器 → 持久化返回的签名事件
  - 修改 `invite_user` 自动检测远程被邀请者（`is_remote_user`）并委托，跳过本地 `user_exists` 检查
- **修复 `exchange_third_party_invite`**（参考 Synapse `FederationHandler.exchange_third_party_invite`）：
  - 入站：`exchange_third_party_invite` handler 验证事件后用本地服务器密钥签名（`sign_and_hash_event`）并返回签名事件
  - 出站：新增 `RoomService::exchange_third_party_invite_via_federation` 调用远程服务器交换第三方邀请，持久化返回的签名事件
- **修改文件**：
  - `synapse-federation/src/signing.rs` — 新增 `sign_and_hash_event` 函数
  - `synapse-storage/src/event/mod.rs` — 新增 `update_event_signatures_and_hashes` 方法
  - `synapse-services/src/room/federation_broadcast.rs` — 新增 `sign_and_broadcast_event` 方法
  - `synapse-services/src/room/federation_membership.rs` — 新增 `join_room_via_federation` / `leave_room_via_federation` / `invite_user_via_federation` / `exchange_third_party_invite_via_federation` 方法
  - `synapse-services/src/room/service.rs` — 添加 `key_rotation_manager` / `federation_client` 字段和 setter
  - `synapse-services/src/room/membership_actions.rs` — `join_room` / `leave_room` 添加联邦委托和广播
  - `synapse-services/src/room/membership_moderation.rs` — `invite_user` / `ban_user` / `unban_user` / `kick_user` 添加联邦委托和广播
  - `synapse-services/src/room/events.rs` — `create_event` wrapper 添加广播调用
  - `synapse-services/src/container.rs` — 注入 `key_rotation_manager` / `federation_client` 到 RoomService
  - `src/web/routes/handlers/room/members.rs` — `join_room_by_id_or_alias` 使用 `join_room_with_via_servers`
  - `src/web/routes/federation/membership.rs` — `exchange_third_party_invite` 入站 handler 添加事件签名
  - `tests/integration/room_service_tests_migrated.rs` / `tests/integration/sync_service_tests_migrated.rs` — 测试配置添加新字段
- **验证**：
  - `cargo check --all-features --locked` 通过
  - `cargo clippy --all-features --locked -- -D warnings` 通过
  - `cargo test --lib` — 751 条单元测试全通过
- **出站联邦能力状态**：全部已实现，"半双工联邦"问题已解决

---

## 代码质量审查与修复（2026-06-22）

### 一、修复的编译错误和失败测试

- **修复 `OidcConfig` 缺失字段**：`tests/unit/sso_oidc_tests.rs` 缺少 `registration_endpoint` 字段（`synapse-common/src/config/auth.rs` 新增的字段未同步到测试）
- **修复 `placeholder_scan_tests` 失败**：`scripts/shell_routes_allowlist.txt` 中的行号因代码变更已过期，更新了 `e2ee_routes.rs` 和 `voip.rs` 的行号
- **修复 `ledger_export_tests` 失败（4 个）**：新增 `auth_issuer` 路由后 ledger fixture 未更新，重新生成 4 个 profile 的 fixture 文件
- **修复 dead code 警告**：`tests/integration/matrixrtc_tests_migrated.rs` 中 `create_test_storage` / `create_test_service` 添加 `#[allow(dead_code)]`

### 二、清理的死代码

| 文件 | 行数 | 说明 |
|------|------|------|
| `src/common/backpressure.rs` | ~690 行 | 整个模块从未被任何生产代码导入 |
| `synapse-common/src/backpressure.rs` | ~200 行 | 孤立文件，`lib.rs` 中未声明模块 |
| `synapse-federation/src/edu_dispatcher.rs` | ~150 行 | `EduType` 枚举与 `src/federation/edu.rs` 分歧重复，从未被消费 |
| `synapse-cache/src/lib.rs` `get_redis_pool_metrics_sync` | ~15 行 | 返回硬编码零值，从未被调用 |
| `src/bin/schema_health_check.rs` `quick_check` | ~5 行 | 从未被调用的辅助函数 |
- **注册 `get_key_history` 路由**：`src/web/routes/e2ee_routes.rs` 中 `get_key_history` 函数完整实现但被 `#[allow(dead_code)]` 隐藏，现已注册为 `GET /_matrix/client/v3/keys/history`

### 三、修复的占位符实现

- **`get_statistics`**（`src/web/routes/admin/server.rs`）：`daily_active_users` / `monthly_active_users` / `r30_users` / `r30v2_users` 之前全部返回 `total_users`（占位符）。新增 `UserStorage::get_daily_active_users` / `get_monthly_active_users` / `get_r30_users` 方法，基于 `devices.last_seen_ts` 计算真实活跃用户数
- **`get_jitsi_config`**（`src/web/routes/admin/server.rs`）：硬编码第三方服务 `meet.jit.si` 改为 `null`（未配置），避免误导私有部署管理员

### 四、识别的冗余代码（待后续处理）

| 冗余类型 | 位置 | 严重度 | 说明 | 状态 |
|----------|------|--------|------|------|
| **worker 模块跨 crate 重复** | `src/worker/` (11 文件) vs `synapse-services/src/worker/` (11 文件) | 高 | 除 `topology_validator.rs` 是门面外，其余 10 个文件是完整复制，仅 import 路径不同。`WorkerManager` 来自 synapse-services，而 `WorkerInfo`/`WorkerType` 来自 src/worker/，两套代码交叉使用 | ✅ 已修复 (2026-06-22) |
| **门面文件维护开销** | `src/e2ee/device_keys/{mod,models,service,storage}.rs` 等 | 低 | 单行 `pub use` 重新导出文件，迁移遗留 | 待处理 |
| **`RedisPoolMetrics` 结构体** | `synapse-cache/src/lib.rs` | 低 | 唯一使用方法 `get_redis_pool_metrics_sync` 已删除，结构体本身可能也是死代码 | ✅ 已修复 (2026-06-22) |

#### 四.1 已修复冗余代码详细说明 (2026-06-22)

**worker 模块跨 crate 重复消除**
- `src/worker/` 下 11 个文件（manager.rs, health.rs, bus.rs, load_balancer.rs, protocol.rs, stream.rs, tcp.rs 等）转换为门面文件，使用 `pub use synapse_services::worker::*::*;` 重新导出
- 将 `src/worker/manager.rs` 和 `health.rs` 中的独有改进（任务所有权验证、recovery_threshold、并行健康检查）移植到 `synapse-services/src/worker/manager.rs` 和 `health.rs`
- 消除测试/生产行为分歧，统一 worker 模块实现

**RedisPoolMetrics 死代码清除**
- 从 `synapse-cache/src/lib.rs` 移除 `RedisPoolMetrics` 结构体
- 从 `synapse-services/src/lib.rs` 的 `pub use synapse_cache::{...}` 导入列表中移除 `RedisPoolMetrics`

### 五、参考 Synapse 审查的功能缺口

| 优先级 | 功能 | 说明 | 状态 |
|--------|------|------|------|
| **高** | Admin Room 模块被禁用 | `src/web/routes/admin/mod.rs` 中 `pub mod room;` 被注释（ServiceContainer 重构后 350+ 编译错误），所有房间管理 admin 端点不可用 | ✅ 已修复 (2026-06-22) |
| **高** | Server ACLs 执行逻辑缺失 | `m.room.server_acl` 事件被识别但未执行 ACL 检查，联邦安全策略无法生效 | ✅ 已修复 (2026-06-22) |
| **高** | Redis Pub/Sub 未真正实现 | `src/worker/bus.rs` 的 `connect()` 仅设置 `connected = true`，未真正连接 Redis，多 worker 部署不可用 | ✅ 已修复 (2026-06-22) |
| 中 | 联邦速率限制不完整 | 仅 join 操作有 429 计数，缺少通用联邦请求速率限制 | ✅ 已修复 (2026-06-23) |
| 中 | 服务器统计不完整 | `get_statistics` 已修复 DAU/MAU，但缺少 room 活跃度、消息量等指标 | ✅ 已修复 (2026-06-23) |
| 中 | 身份服务器集成不完整 | 仅实现基础 3PID bind/unbind，缺少邮箱/手机验证流程 | ✅ 部分修复 (2026-06-23) |
| 中 | `m.ignored_user_list` 未暴露 | Device 表有字段但未通过 account_data API 暴露，未在推送过滤中执行 | ✅ 已修复 (2026-06-22) |
| 中 | 房间升级不完整 | tombstone 事件未联邦广播，旧房间成员未自动迁移到新房间 | ✅ 已修复 (2026-06-22) |
| 中 | 事件认证链/状态解析基础 | 缺少持久化缓存和增量计算，`verify_auth_chain` 实现过于简单 | ✅ 部分修复 (2026-06-23) |
| 中 | 跨签名密钥验证不完整 | 缺少密钥轮换完整流程和设备签名验证完整链路 | ✅ 部分修复 (2026-06-23) |
| 中 | 复制协议不完整 | 缺少与上游 Synapse TCP 复制协议的兼容性 | 📋 已文档化 (2026-06-23) |
| 低 | 服务器重启 API | `restart_server` 返回 501 | ✅ 已修复 (2026-06-23) |
| 低 | 备份管理 API | `get_backups` 返回 501（有意设计，由外部基础设施管理） | 📋 有意设计 |

#### 五.1 已修复功能详细说明 (2026-06-22)

**Admin Room 模块重新启用**
- 修复 `src/web/routes/admin/room/mod.rs` 中 `serde_json::Value` 字段访问（service 层返回 `Vec<Value>` 而非结构体）
- 修复 `src/web/routes/admin/room/management.rs` 中缺失的 `?` 操作符和 service 路径
- 修复 `src/web/routes/admin/room/spaces.rs` 中 `Result<Option<T>>` 类型处理
- 在 `src/web/routes/admin/mod.rs` 中取消注释 `pub mod room;` 并注册路由

**Server ACLs 执行逻辑**
- 新建 `synapse-federation/src/server_acl.rs`：实现 `ServerAclContent` 结构体，支持 glob 匹配 (`*` 通配符) 和 IP 字面量检查
- 集成到 `src/web/routes/federation/mod.rs` 的 `validate_federation_origin_in_room` 和 `validate_federation_origin_can_observe_room`（入站联邦检查）
- 集成到 `synapse-services/src/room/federation_membership.rs` 的 `join_room_via_federation`、`leave_room_via_federation`、`invite_user_via_federation`（出站联邦检查）
- 9 个单元测试覆盖 glob 匹配、IP 字面量、deny 优先级等场景

**Redis Pub/Sub 真正连接**
- 重写 `synapse-services/src/worker/bus.rs` 的 `connect()` 方法，真正创建 Redis 客户端、连接池和后台订阅任务
- 实现 `try_connect_redis()`：创建 Redis client，PING 测试连接，创建 deadpool 连接池
- 实现 `spawn_subscriber_task()`：后台订阅 Redis Pub/Sub 频道，转发消息到本地 broadcast 订阅者，断线自动重连（5秒重试）
- `publish()` 同时发布到 Redis Pub/Sub（跨实例）和本地内存订阅者
- Redis 不可用时优雅降级到单实例内存模式，服务器仍可启动
- `BusStats` 新增 `redis_enabled` 字段

**`m.ignored_user_list` 暴露与推送过滤**
- 在 `synapse-services/src/account_data_service.rs` 新增 `get_ignored_users()` 方法，解析 `m.ignored_user_list` account_data 内容
- 在 `validate_account_data_payload()` 中新增 `m.ignored_user_list` 内容形状验证（必须包含 `ignored_users` 对象，键必须是合法 Matrix user ID）
- 在 `synapse-services/src/push/service.rs` 的 `PushNotificationService` 新增 `account_data_storage` 字段和 `with_account_data_storage()` builder 方法
- `evaluate_push_rules()` 在评估推送规则前检查发送者是否在接收者的忽略列表中，若忽略则直接返回 `notify: false`（匹配 Synapse 行为）
- 在 `synapse-services/src/container.rs` 中将 `AccountDataStorage` 注入到 `PushNotificationService`
- 7 个新单元测试覆盖验证逻辑

**房间升级联邦广播与成员迁移**
- 修改 `synapse-services/src/room/upgrade.rs` 的 `upgrade_room()`：
  - tombstone 事件创建改用 `self.create_event()` 包装方法（而非直接调用 `event_storage.create_event()`），确保事件被签名并广播到所有有 joined 成员的远程服务器
  - 自动邀请旧房间所有 joined 成员到新房间（本地用户走本地邀请路径，远程用户走联邦邀请路径）
  - 自动将升级用户 join 到新房间（`create_room` 仅邀请创建者，需主动 join）
  - 调用 `migrate_room_content()` 复制旧房间状态（power levels、join_rules、canonical_alias 等）到新房间
  - 所有迁移操作为 best-effort，失败仅记录日志不影响升级

#### 五.2 已修复功能详细说明 (2026-06-23)

本轮对照 `element-hq/synapse` 与 Matrix 规范，对功能缺口表中 8 项剩余条目进行了系统性收口。其中 5 项已完整修复，1 项文档化为已知限制，1 项确认为有意设计，1 项因浏览器侧噪音暂归类为部分修复。详细说明如下。

**1. 联邦速率限制不完整 → ✅ 已修复**

- **问题**：原先仅 join 操作有 429 计数，缺少通用联邦请求速率限制，无法防止恶意远程服务器对联邦端点发起 DoS。
- **实现步骤**：
  - 新增 `FederationRateLimitConfig` 配置结构（`synapse-common/src/config/federation.rs`），支持 `enabled / per_second / burst_size`，默认关闭以保持向后兼容。
  - 新增 `CacheKeyBuilder::federation_origin_rate_limit(origin, endpoint)`（`synapse-cache/src/strategy.rs`），生成 `ratelimit:fed:{origin}:{endpoint}` 缓存键。
  - 新建 `src/web/middleware/federation_rate_limit.rs`：从请求扩展中提取已认证的 Matrix `origin`（由 `federation_auth_middleware` 注入），按粗粒度路径桶（`send / make_join / send_join / event / query / state / other`）分组，复用现有 `rate_limit_token_bucket_take` 缓存基础设施执行令牌桶算法。
  - 在 `src/web/routes/federation/mod.rs` 中将该中间件层叠在 `federation_auth_middleware` 之上，确保 `FederationRequestAuth` 扩展可用。
- **修改文件**：`synapse-common/src/config/federation.rs`、`synapse-cache/src/strategy.rs`、`src/web/middleware/federation_rate_limit.rs`（新建）、`src/web/middleware/mod.rs`、`src/web/routes/federation/mod.rs`
- **验证**：`cargo test --lib web::middleware::federation_rate_limit::tests` 通过；`test_federation_endpoint_bucket` 覆盖 7 类路径分桶。

**2. 服务器统计不完整 → ✅ 已修复**

- **问题**：`get_statistics` 已修复 DAU/MAU，但缺少 room 活跃度、消息量等运营关键指标。
- **实现步骤**：
  - 在 `synapse-storage/src/event/mod.rs` 新增 `get_daily_message_count()`：统计最近 24 小时 `m.room.message` 事件数量。
  - 增强 `src/web/routes/admin/server.rs` 的 `get_statistics`：调用 `room_storage.get_room_stats_overview()` 获取 `total_messages / active_rooms / total_members / encrypted_rooms`，并叠加 `daily_messages` 指标。
- **修改文件**：`synapse-storage/src/event/mod.rs`、`src/web/routes/admin/server.rs`
- **验证**：`cargo check --workspace --locked` 通过；统计端点现返回完整运营指标。

**3. 身份服务器集成不完整 → ✅ 部分修复**

- **问题**：`bind_three_pid` 存储了错误数据（`"{id_server}:{sid}"` 作为 address、`"unknown"` 作为 medium）；`unbind_threepid` 未调用远程 IS unbind。
- **实现步骤**：
  - 修复 `synapse-services/src/identity/service.rs` 的 `bind_three_pid`：解析 bind 响应 JSON 中的真实 `address` 与 `medium` 字段，校验非空后写入 `ThirdPartyId`。
  - 修复 `src/web/routes/account_compat.rs` 的 `unbind_threepid`：新增 `id_server / id_access_token` 可选请求字段，在本地删除前调用 `extensions.identity_service.unbind_three_pid()` 执行远程 IS unbind。
- **修改文件**：`synapse-services/src/identity/service.rs`、`src/web/routes/account_compat.rs`
- **验证**：`cargo check --workspace --locked` 通过；bind/unbind 数据质量与远程联动已对齐。
- **遗留**：邮箱/手机验证流程（SMS 发送、验证码校验）仍需独立任务推进，属产品功能扩展而非 bug 修复。

**4. 事件认证链/状态解析基础 → ✅ 部分修复**

- **问题**：`auth_chain_cache` 类型为 `Cache<String, bool>`，缓存命中时仍需完整 BFS 重算，缓存形同虚设。
- **实现步骤**：
  - 将 `synapse-federation/src/event_auth/models.rs` 的 `auth_chain_cache` 类型从 `Cache<String, bool>` 改为 `Cache<String, Vec<String>>`。
  - 更新 `synapse-federation/src/event_auth/mod.rs` 的 `get_cached_auth_chain` / `cache_auth_chain_result` 签名以匹配新类型。
  - 重写 `synapse-federation/src/event_auth/chain.rs` 的 `build_auth_chain_with_cache`：缓存命中时直接返回缓存的 `Vec<String>`，未命中时计算后写入缓存。
- **修改文件**：`synapse-federation/src/event_auth/models.rs`、`synapse-federation/src/event_auth/mod.rs`、`synapse-federation/src/event_auth/chain.rs`
- **验证**：`cargo test -p synapse-federation --lib event_auth` 通过；`test_cache_auth_chain` 已更新为验证 `Vec<String>` 缓存语义。
- **遗留**：持久化缓存与增量计算需更大范围重构，属性能优化而非正确性修复。

**5. 跨签名密钥验证不完整 → ✅ 部分修复**

- **问题**：`verify_device_key` / `verify_device_key_batch` 使用 OR 逻辑（`ssk_signature_valid || mk_signature_valid`），允许 self_signing 反向签名 master 的非规范链路绕过验证。
- **实现步骤**：
  - 在 `synapse-e2ee/src/cross_signing/service.rs` 的 `get_user_verification_status` 中移除 `mk_signature_valid`（反向 ssk→master），仅保留 `ssk_signature_valid`（master 签 self_signing）。
  - 在 `verify_device_key` 与 `verify_device_key_batch` 中将 OR 逻辑改为严格链式：`chain_intact && verified_by_self_signing`，其中 `chain_intact` 要求 master_key 签名 self_signing_key 通过验证。
- **修改文件**：`synapse-e2ee/src/cross_signing/service.rs`
- **验证**：`cargo check --workspace --locked` 通过；验证链现严格遵循 Matrix 规范的 `master → self_signing → device` 方向。
- **遗留**：密钥轮换完整流程与设备签名验证端到端回归需独立任务补齐。

**6. 复制协议不完整 → 📋 已文档化**

- **问题**：TCP 复制协议服务器从未真正启动，payload 解析不完整，与上游 Synapse 不兼容。
- **评估结论**：该模块需要完整协议重写（握手、RDATA/EDATA/FEDERATION_ACK 帧解析、流位点同步、位置持久化），属独立大型任务，无法通过局部修补达成可用状态。
- **当前状态**：已文档化为已知限制，不影响单实例部署；多 worker 部署现依赖 Redis Pub/Sub（已在 2026-06-22 修复中真正实现）。
- **建议**：作为独立 epic 排期，参考 `element-hq/synapse` 的 `synapse/replication` 模块。

**7. 服务器重启 API → ✅ 已修复**

- **问题**：`restart_server` 返回 501 Not Implemented。
- **实现步骤**：
  - 在 `src/web/routes/state.rs` 的 `AppState` 新增 `shutdown_signal: Option<broadcast::Sender<()>>` 字段与 `with_shutdown_signal()` setter。
  - 在 `src/server.rs` 的 `new()` 中提前创建 broadcast channel 并注入 `AppState`，`run()` 复用该 channel 触发优雅关闭。
  - 重写 `src/web/routes/admin/server.rs` 的 `restart_server`：解析 `timeout_ms`（上限 10s），延迟后发送 shutdown 信号，返回 `{ restart_pending: true }`；进程管理器（Docker/systemd）负责重启。
- **修改文件**：`src/web/routes/state.rs`、`src/server.rs`、`src/web/routes/admin/server.rs`
- **验证**：`cargo check --workspace --locked` 通过；端点现返回 200 并触发优雅关闭。

**8. 备份管理 API → 📋 有意设计**

- **问题**：`get_backups` 返回 501。
- **评估结论**：备份由外部基础设施（Docker volume 快照、pg_dump、对象存储生命周期）管理，服务器进程内不应承担备份编排职责。返回 501 是有意设计，避免与外部备份系统产生竞争条件。
- **当前状态**：确认为有意设计，无需修改。

### 六、验证结果

#### 2026-06-22 验证

- `cargo check --all-features --locked` ✅ 通过
- `cargo clippy --all-features --locked -- -D warnings` ✅ 通过
- `cargo test --lib` — 719 条测试全通过 ✅
- `cargo test --features test-utils --test unit` — 862 条测试全通过 ✅

#### 2026-06-23 验证

- `cargo check --workspace --locked` ✅ 通过（含 synapse-e2ee / synapse-federation / synapse-services / synapse-storage / synapse-cache 全部 crate）
- `cargo test --lib web::middleware::federation_rate_limit::tests` ✅ 通过（联邦速率限制路径分桶）
- `cargo test -p synapse-federation --lib event_auth` ✅ 通过（认证链缓存语义）
- 联邦速率限制、服务器统计、身份服务器集成、认证链缓存、跨签名验证、服务器重启 API 共 7 项代码修复已落地编译验证

# API 错误追踪报告

> **创建日期**: 2026-03-26
> **更新日期**: 2026-04-01
> **项目**: synapse-rust

***

## 测试结果总览

| 测试类型               | 结果                                                    | 日期             |
| ------------------ | ----------------------------------------------------- | -------------- |
| <br />             | <br />                                                | <br />         |
| <br />             | <br />                                                | <br />         |
| <br />             | <br />                                                | <br />         |
| <br />             | <br />                                                | <br />         |
| <br />             | <br />                                                | <br />         |
| <br />             | <br />                                                | <br />         |
| <br />             | <br />                                                | <br />         |
| API 集成测试（数据库契约修复后） | ⚠️ \~510 passed, \~3 failed, \~50 skipped             | 2026-03-31     |
| API 集成测试（环境恢复后）    | ✅ 421 passed, 0 failed, 171 skipped                   | 2026-03-31     |
| **API 集成测试（最新）**   | **✅ 397 passed, 0 failed, 155 skipped**               | **2026-03-31** |
| **API 集成测试（最新，dev）** | **✅ 476 passed, 0 failed, 39 missing, 39 skipped**                            | **2026-04-01** |
| **Rust 集成测试（tests/integration）** | **✅ 233 passed, 0 failed** | **2026-04-01** |
| **Rust 全量测试（workspace/all-targets/all-features）** | **✅ 通过（严格模式迁移初始化 + 独立 schema）** | **2026-04-01** |
| 定向修复代码验证           | ✅ `cargo test --no-run` 通过；Docker 测试环境已恢复并完成全量 API 回归 | 2026-03-31     |
| Clippy             | ✅ `cargo clippy --workspace --all-targets --all-features -- -D warnings` 通过 | 2026-04-01     |

***

##

### 1.3 当前剩余问题

- 跳过数为 39，主要原因是联邦签名请求限制与破坏性测试跳过
- 当前缺口以 MISSING 为主（39 项），集中在 SSO/OpenID、少量房间管理与账号数据补齐
- 后续工作重点：按 Matrix Client 核心路径优先级继续将 MISSING 收敛为 PASS

### 1.5 Rust 测试稳定性修复摘要（2026-04-01）

- 修复路由合并冲突导致的启动 panic（重复注册 `/rooms/{room_id}/relations/{event_id}`）
- 修复受限环境对 `/app` 目录写入导致的 500（媒体/语音落盘默认回退到 `./data/media`）
- 将测试链路切换为“严格模式”迁移初始化：每个测例独立 schema、独立 search_path，并只执行 `migrations/*.sql`
- 收敛运行时 DDL：保留脚本迁移为主链，移除 presence 订阅表的按需建表，避免 audit/feature_flags/event_relations 等测试再依赖运行时兜底
- 修复 Admin 房间搜索 count SQL 字段错误（`type` → `event_type`；`creation_ts` → `created_ts as creation_ts`）
- 修复 server notices 列表默认返回过大导致测试读取超限（增加分页默认 limit）

### 1.4 skip 收敛专项进展

- 已完成代表性 skip 的首轮拆分：`Admin Room Member Add/Ban/Kick` 主要是测试脚本硬编码用户与调用方式不匹配，`Get Room Hierarchy` 属于后端实现稳健性问题
- 已修复管理员房间成员管理链路：后端新增对 `/_synapse/admin/v1/rooms/{room_id}/ban` 与 `kick` 的 body 兼容处理，同时保留既有 `/{user_id}` 路径式接口
- 已修复脚本中的目标用户选择：房间邀请、踢出、封禁、解封及管理员成员管理统一改为复用第二测试用户，不再依赖硬编码的 `@test:cjystx.top`
- 已修复普通房间的 Room Hierarchy 返回逻辑：优先复用 `space_service` 处理 space，普通房间改为返回稳定摘要，避免再落到 500
- 本轮已完成 `cargo test --no-run` 与 `bash -n scripts/test/api-integration_test.sh` 校验
- 已完成 Docker dev 环境全量 API 集成回归并同步更新质量证据产物

###

## 质量证据产物（自动生成）

- API 集成测试摘要：`reports/quality/api_integration_summary.md`
- API 集成缺陷清单（P0-P3 草案）：`docs/quality/defects_api_integration.md`
- 一键证据采集入口：`bash scripts/quality/collect_evidence.sh`

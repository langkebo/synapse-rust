# Matrix / Synapse 对标审查与优化方案

审查日期: 2026-05-29

基线:
- Matrix Specification latest: v1.18
- element-hq/synapse 最新稳定标签: v1.153.0，最新预发布标签: v1.154.0rc1
- 本仓库重点入口: `src/web/routes/assembly.rs`, `src/web/routes/handlers/versions.rs`, `src/web/routes/federation/*`, `src/web/middleware/federation_auth.rs`, `src/services/container.rs`, `src/storage/*`

## 结论摘要

项目已经具备较完整的 Matrix homeserver 骨架，并且有路由账本、schema health check、Redis/本地缓存、worker、E2EE、媒体、空间、同步、联邦等模块。但与 Matrix v1.18 和 Synapse v1.153.0 的成熟实现相比，主要短板不在“有没有路由”，而在协议声明准确性、联邦边界语义、长期运行治理、规范级事件校验、Complement 级互通测试和 worker/stream 运维模型。

本次已完成两个低风险修复:
- 联邦 `Authorization: X-Matrix ... destination=...` 不再只匹配旧配置字段 `server.name`，而是接受当前服务实际 server name、兼容旧字段、`server.server_name` 和 `federation.server_name`。这避免反向代理或委托域部署中合法联邦请求被误拒。
- `/_matrix/client/v3/capabilities` 能力响应已从 handler 内的大块重复 `cfg` 分支收口为 builder，公共能力与认证后私有/扩展能力分层生成，`widgets`、`burn-after-read`、`friends` 等 unstable/custom 声明随编译特性变化，避免禁用特性仍对外声明为可用。

## 规范与 Synapse 关键学习

Matrix Client-Server v1.18 强调:
- `/_matrix/client/versions` 是客户端能力判断入口，声明必须保守，不能把未完整实现的稳定版本写进去。
- `/_matrix/client/v3/capabilities` 应反映实际配置能力，例如 room versions、profile field 修改、密码变更、登录 token 等。
- 房间版本已经到 v12，声明 `m.room_versions` 时需要区分“支持解析/加入”和“默认创建”。
- 新增错误码和认证状态会影响 SDK 兼容性，例如 locked user、appservice device ownership 等。

Matrix Server-Server API 强调:
- 联邦请求签名使用 `method`, `uri`, `origin`, `destination`, `content` 的 canonical JSON。
- `Authorization` scheme 为 `X-Matrix`，参数包括 `origin`, `destination`, `key`, `sig`。
- 服务器密钥接口 `/_matrix/key/v2/server` 与 notary query 响应结构不同，`server_keys` query 响应需要按目标服务器聚合。
- destination 校验应判断是否属于本 homeserver，而不应绑死单一配置字段。

Synapse v1.153.0 的近期方向:
- 安全和资源治理优先: worker lock contention DoS 修复、分页 rejected events 修复、quarantined media 权限修复。
- Rust 化热点路径: Event signatures、unsigned 和 canonical JSON serializer 被迁到 Rust。
- 功能发布保守: MSC4186 sliding sync 的即时响应优化因性能问题回滚。
- 长期运行治理: 剪枝 `device_lists_changes_in_room` 旧数据、增加 quarantined media changes stream writer。
- 声明与实现同步: `capabilities.py` 从 `KNOWN_ROOM_VERSIONS` 和配置生成能力，而不是硬编码静态表。

## 发现的问题与不足

### P0: 联邦安全与互通

1. `destination` 校验过窄
- 现状: 旧实现只接受 `config.server.name`。
- 风险: `server.server_name`、`federation.server_name` 或委托域部署会被误拒。
- 状态: 已修复并补测试。

2. 联邦密钥 query/notary 语义仍需收敛
- 现状: `/_matrix/key/v2/query/{server_name}/{key_id}` 会对远端结果做 canonical 包装，但没有完整验证远端 server key JSON 自签名链路。
- 风险: 缓存污染、过期 key 接受、与 notary 响应结构边界不清。
- 建议: 抽出 `ServerKeySet` 类型，验证 `server_name`、`valid_until_ts`、`verify_keys`、`old_verify_keys` 和 `signatures` 后再缓存。

3. 事件 canonical JSON 是跨协议根基
- 现状: 项目已有 signing 模块，但缺少与 Matrix canonical JSON test vectors 的集中门禁。
- 风险: 房间事件签名、联邦 request signing、server keys signing 发生细微不兼容。
- 建议: 引入规范向量与 Synapse 行为对照测试，优先覆盖 integer、map ordering、unsigned stripping、signatures stripping。

### P1: Client-Server 能力声明

4. `/versions` 与能力声明需要治理机制
- 现状: `CLIENT_API_VERSIONS` 是静态列表，当前写到 `v1.13`，而官方 spec 已到 v1.18；Synapse 最新稳定仍谨慎声明到 v1.12。
- 风险: 过度声明导致客户端启用未完整实现能力；声明滞后导致新 SDK 误判能力。
- 建议: 建立 `SupportedMatrixVersions` 常量与覆盖测试，每次提升版本必须绑定端点覆盖、错误码、字段兼容清单。

5. `capabilities` 应从配置和实现表生成
- 现状: 已完成第一步 builder 化，room version、SSO、OpenClaw 和 feature-gated 私有能力不再散落在重复分支里；但 profile/password/3PID 等稳定能力仍是静态 `true`，尚未与配置和路由账本形成证据链。
- 风险: 配置关闭但能力仍显示启用，或实现存在但未声明。
- 建议: 下一步按 Synapse 模式把 builder 输入扩展为 `Config + FeatureFlags + RouteLedger`，并为每个稳定 capability 增加“声明证据”测试。

6. room version v12 路线不清
- 现状: 声明 stable 到 v11；已新增 room version 能力矩阵，区分 create、join/accept、parse、federation，但 v12 尚未评估和声明。
- 风险: 升房、restricted/knock/space 行为在不同房间版本下出现不一致。
- 建议: 基于现有矩阵继续补 authorization rules、event format、redaction rules、restricted join、knock、state resolution 支持字段。

### P1: 同步、设备与长期运行

7. Sliding sync 优化需要性能闸门
- 现状: 项目已有 sliding sync 路由和测试，但缺少类似 Synapse 回滚 MSC4186 的性能阈值机制。
- 风险: “更实时”的改动放大数据库查询与 worker fanout 压力。
- 建议: 为 sliding sync 增加 subscription-change benchmark、p95/p99 和 query count 快照。

8. 设备列表与 presence 数据需要生命周期治理
- 现状: 有设备同步与 presence 存储，但未见类似 Synapse `device_lists_changes_in_room` 剪枝策略的统一后台任务。
- 风险: 长期实例磁盘膨胀，`/sync` 和 E2EE key query 热点退化。
- 建议: 新增 background update: 剪枝旧 device list change、过期 presence、过期 one-time key 审计记录。

9. worker/stream writer 拓扑需要显式配置校验
- 现状: 有 worker 子系统和 replication，但 stream writer 类型、路由到 worker 的约束不够像 Synapse 那样显式。
- 风险: 管理 API 在 worker 部署下只局部生效。
- 建议: 增加 worker topology validator，启动时校验 route owner、stream writer、后台任务 owner。

### P2: 管理、可观测与测试

10. Admin API 与 Synapse 近期能力存在差距
- 重点: quarantined media changes、user reports list/fetch/delete、room details tombstoned/replacement_room。
- 建议: 优先补“审计/治理类”接口，因为它们不改变客户端协议但提升生产可运维性。

11. Complement 级互通测试不足
- 现状: 集成测试多，但多数是本仓库行为测试。
- 建议: 建立最小 Complement/Matrix SDK 兼容门禁: register/login/sync/create room/send event/federation key/server discovery/media。

12. 文档基线需要自动更新提醒
- 现状: 已有 API coverage 与 upstream research，但版本基线会快速过期。
- 建议: 增加脚本抓取 Matrix latest spec 版本与 Synapse tags，生成审查提醒，不自动改能力声明。

## 分阶段实施计划

### Phase 1: 协议边界收敛

- 完成 federation destination alias 修复。
- 为 `X-Matrix` 解析增加更完整测试: 空格、大小写、缺参、错误 scheme、destination mismatch。
- 抽出 server key response 类型并验证过期时间、server name、自签名。
- 为 canonical JSON 增加规范向量测试。

验收:
- `cargo test --test integration api_federation_signature_auth_tests -- --test-threads=1`
- `cargo test --test integration api_federation_tests -- --test-threads=1`
- 新增 canonical JSON 单元测试通过。

### Phase 2: 声明层治理

- 把 `/versions` 静态 JSON 改为 typed builder。
- 已完成 `/capabilities` 第一阶段 builder 化: 公共能力、认证私有能力、unstable feature 声明分层生成，去掉重复 feature 分支。
- 已新增 `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`，记录当前 `/versions`、`/capabilities`、room versions 和 unstable/custom feature 的声明证据与提升规则。
- 继续把 `/capabilities` builder 输入扩展到 route ledger 和稳定能力配置项。
- 持续完善 `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`，列出每个稳定版本/MSC 的支持证据。
- 禁止未绑定证据的版本号提升。

验收:
- route ledger 与 supported surface 一致。
- `/_matrix/client/versions`、`/_matrix/client/v3/capabilities` 快照测试稳定。
- `cargo test --lib web::routes::handlers::versions::tests -- --nocapture` 通过。

### Phase 3: 房间版本与事件语义

- 已建立 room version 能力矩阵，显式区分可创建、可加入/接受、可解析、可联邦接受版本。
- 明确默认房间版本、可创建版本、可加入版本、仅解析版本。
- 补充 v12 评估: authorization rules、redaction、knock/restricted join、tombstone upgrade。
- 对升级房间流程增加事务性测试，避免 Synapse v1.153.0 提到的 power level 临时突变类问题。

验收:
- room create/upgrade/join/knock/restricted join 测试覆盖版本差异。
- capabilities 中 `m.room_versions` 来自同一能力矩阵。

### Phase 4: 长期运行与 worker 治理

- 增加 background pruning: device list changes、presence、过期 upload chunks、过期 OTK 审计。
- 增加 worker topology validator。
- 为 quarantined media changes 建 stream/table/API。
- 将 request log 加入 db/ru 类标签，便于与 Synapse 运维经验接轨。

验收:
- 后台任务可重复运行、可分页观测、失败可恢复。
- worker enabled snapshot 覆盖新增 owner/stream 配置。

### Phase 5: 互通与性能门禁

- 引入最小 Complement 或 SDK smoke。
- 为 `/sync`、sliding sync、federation key fetch、media quarantine 增加 query count 与 p95/p99 基线。
- 对实验性优化设置回滚阈值，避免性能证据不足仍默认开启。

验收:
- CI 区分主门禁、扩展门禁、手动门禁。
- 性能回归阈值写入 `TESTING.md`。

## 已完成变更

- 修复 `src/web/middleware/federation_auth.rs` 中联邦 `destination` 只匹配旧字段的问题。
- `X-Matrix` 参数名解析改为大小写不敏感。
- 新增集成测试覆盖 `server.name` 与 `server.server_name` 不一致时的合法 destination。
- 新增单元测试覆盖 `Origin/Destination/Key/Sig` 参数名大小写。
- 重构 `src/web/routes/handlers/versions.rs` 中 capabilities 构建逻辑，删除重复的 widgets / burn-after-read / external-services / voice-extended 插入分支。
- 将 `/versions` 与 `/capabilities` 中 feature-gated unstable/custom 声明对齐到编译特性，避免禁用模块仍被声明为可用。
- 新增 capabilities 单元测试，覆盖未认证公共能力过滤、认证后 SSO/OpenClaw/feature-gate 声明。
- 将 `CLIENT_API_VERSIONS` 裸字符串数组升级为 `CLIENT_API_VERSION_SUPPORT` typed support table，为 legacy r0 与 stable v1 声明建立可测试结构。
- 新增 `docs/synapse-rust/SUPPORTED_MATRIX_SURFACE.md`，作为后续提升 Matrix 版本、MSC 和 capabilities 声明的证据入口。
- 删除 push provider 与 worker 模块中已弃用且命名冲突的兼容别名: `ApnsConfig`、`FcmConfig`、`WebPushConfig`、`RedisConfig`、`WorkerConfig`，统一使用 `*ProviderConfig`、`RedisBusConfig` 和 `WorkerRuntimeConfig`，减少与 `common::config::*Config` 的重复命名。
- 将 `src/common/room_versions.rs` 从单一 stable 列表升级为能力矩阵，当前 v1-v11 行为保持不变，但为 v12 或“仅解析不创建”的过渡状态预留明确模型。
- 联邦 membership 路径已接入 room version federation 维度校验，包括 `make_join`、`make_leave`、`send_join`、`send_join_v2`、`send_leave`、`send_leave_v2`、`knock`、`invite`、`invite_v2`、third-party invite 和成员查询类入口，不再对缺失或未知版本房间默认为 v10 继续处理。
- 数据库专项审查记录见 `docs/db/DB_AUDIT_AND_REMEDIATION_2026-05-29.md`: 已修复迁移/deploy 镜像漂移、Postgres search 误引用 `room_members`、schema coverage 误报和已删除冗余表 `room_children` 的过期 contract 期望。

---

## AUDIT-2026-07 后续审核与优化落地（2026-07-12）

在 2026-05-29 分阶段计划基础上，2026-07 完成一轮 7 阶段全栈审核（`docs/audit/00–07`）+ 优化落地（OPT-001~028）+ 性能复测（`docs/audit/11`）+ 回顾（`docs/audit/13_retro.md`）。交付于 PR #3（分支 `optimization/audit-2026-07`，135 commits / 409 files，保留原子提交历史不 squash）。

### 优化任务完成情况（OPT-xxx，全部 ✅）

**P0 安全（audit 07）**
- OPT-001 OIDC：JWKS 拉取失败/无匹配 kid 时 hard fail，取消 claim-only 签名回退（修复 CRITICAL 认证绕过）
- OPT-002 联邦：签名密钥派生失败日志脱敏（HIGH，私钥不再明文入日志）
- OPT-003 健康检查：仅信任 `/health`，去掉 `/versions` 回退（HIGH，不再掩盖 DB 故障）
- OPT-005 限流：register/password/refresh/keys-upload/3pid 收紧规则
- OPT-008 联邦：签名密钥持久化要求 master key，默认拒绝明文
- OPT-016 限流：XFF 按可信代理校验，取最右可信跳，防止限流绕过
- OPT-017 联邦：send_join/leave/invite/member-list/event-observe 统一 404，消除存在性泄露
- OPT-019 E2EE：`Ed25519SecretKey` 加 `ZeroizeOnDrop`
- OPT-021 OIDC：校验 id_token 的 nonce claim（重放保护）
- OPT-022 SAML：要求 response 或 assertion 至少一个签名
- OPT-024 审计：限制审计事件删除，防篡改

**E2EE / 联邦（audit 06）**
- OPT-004 server-key 缓存 TTL 收敛到 `valid_until_ts`
- OPT-006 加密房成员 leave 触发 megolm 轮换
- OPT-007 拒绝过期版本的 key-backup 恢复
- OPT-010 to-device：`INSERT..ON CONFLICT RETURNING` 原子去重，消除 TOCTOU

**存储 / 服务（audit 03/04）**
- OPT-009 account-data 用毫秒时间戳
- OPT-011 device：create/delete 包事务
- OPT-012 device：`delete_devices_batch` 批处理，消除 N+1（2×N → 2/user，见 audit 11）
- OPT-013(a~r) 18 个可空列 `i64` → `Option<i64>`
- OPT-014(0~e) ServiceContainer CancellationToken，SIGTERM 优雅停机
- OPT-015(0~d) SyncService 缓存 filter/account_data/device-list-max/room-state
- OPT-018 auth：`generate_email_verification_token` 返回 `ApiError`
- OPT-020 存储：`url_preview_cache.expires_ts → expires_at`（含迁移 + 回滚）
- OPT-023 web：sync/members/directory 统一走 `AuthenticatedUser` extractor

**架构强化（M1~M4）**
- OPT-025 events 表查询从 `RoomStorage` 迁到 `EventStorage`（三层边界）
- OPT-026 auth：区分并发 refresh 重试与重放攻击
- OPT-027 worker：失败任务加死信队列
- OPT-028 health：schema 校验失败启动即致命

### 本轮新增硬约束（回顾产出，详见 `docs/audit/13_retro.md`）

1. **安全失败必须 fail-closed。** auth/federation/health/rate-limit 路径中依赖或校验失败默认拒绝，不得降级放行；这些路径的 `fallback`/`unwrap_or`/`default-on-error` 需显式安全评审。（根因：本轮头号问题——OIDC/healthcheck/rate-limit/signing-key/SAML 均曾 fail-open）
2. **声明"绿"前，fmt / clippy / `cargo test --no-run` 必须各在 `--all-features` 下跑一遍。** cfg(test) 与 feature-gated 代码是默认门禁盲区（clippy 跳过 cfg(test)；feature-gated 模块不在默认编译）。
3. **改动进程级全局状态的测试必须模块级 Mutex 串行化**（shuffle+并行下会互相泄露）。
4. **共享池 / destructive 测试需隔离 schema 或 SerialGuard；CI 假失败先按环境排查（head 迁移 + 隔离库）再判代码。**
5. **联邦端点对"存在但无权"与"不存在"统一返 404。**

### 与 2026-05-29 Phase 计划的衔接

- Phase 1（协议边界收敛）：OPT-004/017 推进联邦密钥 TTL 与存在性语义。
- Phase 4（长期运行与 worker 治理）：OPT-014 优雅停机 + OPT-027 死信队列落地。
- Phase 5（互通与性能门禁）：`docs/audit/11` 完成 DB 层查询计划复测（7/7 命中索引，0 seq scan，N+1 消除）；但 p50/p95/p99、QPS、缓存命中率、内存仍 **NOT_CAPTURED**（缺运行中服务器 + 播种脚本），为 Phase 5 遗留前置条件，未编造。

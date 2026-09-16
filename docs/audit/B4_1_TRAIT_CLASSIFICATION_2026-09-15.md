# B4-1 / B4-2 交付物：`*StoreApi` trait 分类清单（存档）

- 日期：2026-09-15
- 依据：`docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md` §3 `B4-1` / `B4-2`
- 基线：`main @ 66339069` + 本批改动（工作树含其它会话在 `src/web/routes/**` 的 codemod，未触碰）
- 复现：`python3 scripts/ci/check_trait_ratchet.py`；分类脚本见本文 §5

---

## 1. 结论摘要

| 桶 | 定义 | 起始 | 现状 | 处置 |
|---|---|---|---|---|
| (i) | `dyn` 引用为 0（trait 无任何类型擦除用途） | 10 | **0** | 全部删除（§2） |
| (ii-a) | `dyn` + 恰好 1 个生产 impl，无 mock | 23 | **0** | 23/23 全部转换完毕（19 见 §3.3，其余 4 个见 §3.5） |
| (ii-b) | `dyn` + 1 个生产 impl + **有 mock** | 21 | 21 | **保留**：mock 就是它的存在理由；且 trait 与生产 impl 已同文件 |
| (iii) | ≥2 个生产 impl | 12 | 12 | **保留**：真实多实现 |
| — | `*StoreApi` 合计 / `pub trait` 合计 | 66 / 96 | **33 / 63** | 棘轮基线已收紧到 33 / 63 |

> **修正**：上一版把 (ii-a)/(ii-b)/(iii) 记为 32/17/7，是因为分类脚本有两个正则缺陷（见 §7），
> 漏掉了「`impl crate::x::Trait for Fake`」这种**限定路径**写法；修正后 23/21/12。

- trait 计数棘轮（新增）：`TOTAL 96 → 86`、`*StoreApi 66 → 56`（本批 −10）。
- 净代码量：`synapse-storage/src` **−739 / +4 行**（16 文件），外加 3 个测试文件 −32 行陈旧测试。
- 验证：`cargo clippy -p synapse-storage -p synapse-services --all-targets --all-features --locked -- -D warnings` → **0 警告**；
  棘轮已用注入探针实测 **RED→GREEN**（§4）。

> ⚠️ **仓库级验证被并发改动阻塞**：`cargo clippy --workspace` 当前在 `src/web/routes/mod.rs`、
  `src/web/middleware/rate_limit.rs` 与 `tests/unit/*_route_tests.rs` 上报 E0603/E0432 ——
  全部来自另一个会话正在进行的 route-manifest codemod（它把 `assembly` 模块转私有、删除手写 manifest，
  但尚未更新引用方）。这些文件**不在本批改动范围内**，本批涉及的 crate 已单独验证通过。

## 2. 已删除的 10 个 trait（(i) 桶）

删除判据（B4-2「删除前守卫」）：**全仓 `grep -rn "\b<trait>\b"` 只剩「声明 + 转发 impl + 再导出」三类命中**，
即既无 `Arc<dyn _>` 注入、也无泛型约束、也无测试消费者。删除后 4 个再导出点同步清理，
4 个只为"扁平路径 == 分组路径"而存在的迁移期测试一并删除。

| trait | 声明文件 | 生产 impl | 删除行数（trait + impl） | 同步清理 |
|---|---|---|---|---|
| `ModerationStoreApi` | `synapse-storage/src/moderation/mod.rs` | `ModerationStorage` | 23 + 33 | — （零引用，连再导出都没有） |
| `ModerationLogStoreApi` | `synapse-storage/src/moderation/mod.rs` | `ModerationLogStorage` | 24 + 31 | — |
| `E2eeAuditStoreApi` | `synapse-storage/src/e2ee_audit.rs` | `E2eeAuditStorage` | 22 + 32 | `e2ee/mod.rs` 再导出 |
| `SearchIndexStoreApi` | `synapse-storage/src/search_index.rs` | `SearchIndexStorage` | 18 + 24 | `sync/mod.rs` 再导出 |
| `VoiceStoreApi` | `synapse-storage/src/voice.rs` | `VoiceStorage` | 48 + 56 | `media/mod.rs` 再导出 |
| `MatrixRTCStoreApi` | `synapse-storage/src/matrixrtc.rs` | `MatrixRTCStorage` | 67 + 78 | `rtc/mod.rs` 再导出 |
| `StateGroupStoreApi` | `synapse-storage/src/state_groups.rs` | `StateGroupStorage` | 62 + 83 | `room/mod.rs` 再导出 + `room_domain_refactor_tests.rs` 陈旧测试 |
| `OAuthClientStoreApi` | `synapse-storage/src/oauth_client_storage.rs` | `OAuthClientStorage` | 20 + 24 | `oidc/mod.rs` 再导出 + `storage_admin_domain_refactor_tests.rs` 陈旧测试 |
| `FederationQueueStoreApi` | `synapse-storage/src/federation_queue.rs` | `FederationQueueStorage` | 24 + 31 | `infra/mod.rs` 再导出 + 同上 |
| `UrlPreviewStoreApi` | `synapse-storage/src/url_preview_storage.rs` | `UrlPreviewStorage` | 10 + 12 | `media/mod.rs` 再导出 + `storage_remaining_domains_refactor_tests.rs` 陈旧测试 |

**为什么这 10 个是零收益抽象**：它们的 doc 注释原文就是
`Trait abstraction over [XStorage] for testability` —— 但既没有任何测试用 `dyn` 注入它们，
也没有第二实现。删除后 `XStorage` 的固有方法（inherent impl）原样保留，功能零变化。

**顺带修复**：4 个文件里 `use async_trait::async_trait;` 变成未使用导入（clippy `-D warnings` 会红），
已随删除同步清理 —— 这也是"删 trait 必须连带检查导入"的固定步骤，已写进 §5 脚本。

## 3. 保留的 56 个 trait，以及为什么不删

### 3.1 (iii) 真实多实现 —— 7 个（必须保留）

| trait | 声明位置 | dyn 引用文件数 | 生产 impl | mock impl |
|---|---|---|---|---|
| `BurnAfterReadStoreApi` | `synapse-storage/src/burn_after_read.rs:100` | 1 | `BurnAfterReadStorage` | - |
| `DeviceKeyStoreApi` | `synapse-e2ee/src/device_keys/storage.rs:137` | 12 | `InMemoryDeviceKeyStore` | - |
| `DeviceListStoreApi` | `synapse-storage/src/device/mod.rs:12` | 12 | `DeviceStorage` | `InMemoryDeviceListStore` |
| `OidcSessionStoreApi` | `synapse-storage/src/oidc_session_storage.rs:107` | 3 | `OidcSessionStorage` | - |
| `OidcUserMappingStoreApi` | `synapse-storage/src/oidc_user_mapping.rs:11` | 3 | `OidcUserMappingStorage` | - |
| `ServerNotificationStoreApi` | `synapse-storage/src/server_notification/api.rs:11` | 2 | `ServerNotificationStorage` | - |
| `WidgetStoreApi` | `synapse-storage/src/widget.rs:99` | 2 | `WidgetStorage` | - |

这 7 个都有 ≥2 个生产实现（含 SDK/内存实现），正是 trait 该存在的理由。
`BurnAfterReadStoreApi` 另有 `NoopBurnStore`；`DeviceKeyStoreApi` 有 `InMemoryDeviceKeyStore`；
`DeviceListStoreApi` 有 `CountingDeviceListStore`；`OidcSessionStoreApi`/`OidcUserMappingStoreApi`/
`ServerNotificationStoreApi`/`WidgetStoreApi` 各有 `InMemory*`/`Mock*` 实现。

### 3.2 (ii-b) mock 接缝 —— 17 个（保留；trait 与生产 impl 已同文件）

| trait | 声明位置 | dyn 引用文件数 | 生产 impl | mock impl |
|---|---|---|---|---|
| `AccessTokenStoreApi` | `synapse-storage/src/token.rs:33` | 7 | `AccessTokenStorage` | `InMemoryAccessTokenStore` |
| `AdminMediaStoreApi` | `synapse-storage/src/admin_media.rs:105` | 1 | `AdminMediaStorage` | `InMemoryAdminMediaStore` |
| `AuditEventStoreApi` | `synapse-storage/src/audit.rs:101` | 5 | `AuditEventStorage` | `InMemoryAuditEventStore` |
| `BackgroundUpdateStoreApi` | `synapse-storage/src/background_update.rs:183` | 2 | `BackgroundUpdateStorage` | `InMemoryBackgroundUpdateStore` |
| `CasStoreApi` | `synapse-storage/src/cas/api.rs:9` | 2 | `CasStorage` | `InMemoryCasStore` |
| `DehydratedDeviceStoreApi` | `synapse-storage/src/dehydrated_device.rs:49` | 2 | `DehydratedDeviceStorage` | `InMemoryDehydratedDeviceStore` |
| `FilterStoreApi` | `synapse-storage/src/filter.rs:36` | 5 | `FilterStorage` | `InMemoryFilterStore` |
| `OpenIdTokenStoreApi` | `synapse-storage/src/openid_token.rs:42` | 1 | `OpenIdTokenStorage` | `InMemoryOpenIdTokenStore` |
| `PresenceStoreApi` | `synapse-storage/src/presence/api.rs:7` | 12 | `super::PresenceStorage` | `InMemoryPresenceStore` |
| `PushStoreApi` | `synapse-storage/src/push/mod.rs:17` | 2 | `PushStorage` | `InMemoryPushStore` |
| `QuarantinedMediaChangeStoreApi` | `synapse-storage/src/media/quarantine_stream.rs:12` | 3 | `QuarantinedMediaChangeStorage` | `InMemoryQuarantineMediaChangeStore` |
| `RateLimitStoreApi` | `synapse-storage/src/rate_limit.rs:19` | 1 | `RateLimitStorage` | `InMemoryRateLimitStore` |
| `RefreshTokenStoreApi` | `synapse-storage/src/refresh_token/mod.rs:253` | 6 | `RefreshTokenStorage` | `InMemoryRefreshTokenStore` |
| `RelationsStoreApi` | `synapse-storage/src/relations/mod.rs:105` | 4 | `RelationsStorage` | `InMemoryRelationsStore` |
| `RoomAccountDataStoreApi` | `synapse-storage/src/room_account_data.rs:10` | 4 | `RoomAccountDataStorage` | `InMemoryRoomAccountDataStore` |
| `RoomTagStoreApi` | `synapse-storage/src/room_tag/mod.rs:28` | 3 | `RoomTagStorage` | `InMemoryRoomTagStore` |
| `ThreepidStoreApi` | `synapse-storage/src/threepid.rs:85` | 5 | `ThreepidStorage` | `InMemoryThreepidStore` |

**B4-1 对该桶的要求是"trait/impl 合并同文件"** —— 实测**已经满足**：每个 trait 的声明与它的生产
`impl` 都在同一个存储模块里（例如 `push/mod.rs` 同时含 `PushStoreApi` 与 `impl PushStoreApi for PushStorage`），
mock 实现单独放在 `synapse-storage/src/test_mocks/`。故本桶**无需改动**，此处存档以免后续重复排查。

### 3.3 (ii-a) `dyn` + 单一生产 impl、无 mock —— 起始 23 个，**已转 19 个**

以下表格是**起始**状态（转换前）；带 `✅` 的 19 个已在本批改为 `Arc<具体类型>` 并删除 trait：

| 已转换（19） |
|---|
| `FeatureFlagStoreApi`、`QrLoginStoreApi`、`PrivacyStoreApi`、`BeaconStoreApi`、`PushNotificationStoreApi`、`RetentionStoreApi`、`CaptchaStoreApi`、`AdminFederationStoreApi`、`CallSessionStoreApi`、`MediaQuotaStoreApi`、`RegistrationTokenStoreApi`、`EventReportStoreApi`、`SpaceStoreApi`、`SamlStoreApi`、`ChunkedUploadStoreApi`、`FederationBlacklistStoreApi`、`FriendRoomStoreApi`、`ApplicationServiceStoreApi`、`StickyEventStoreApi` |

转换方式：把消费者结构体/构造函数的 `Arc<dyn …Trait>` 改为 `Arc<synapse_storage::<模块>::<具体类型>>`
（`src/web` 之外的 26 个里，19 个满足「`dyn` 只出现在 `synapse-services` / `synapse-storage`」），
再删除 trait 与转发 impl。**顺带删掉 7 个"只为装这个 trait 而存在"的 `api.rs` 空壳文件**
（`application_service/`、`event_report/`、`media_quota/`、`registration_token/`、`space/`、`saml/`、`friend_room/`
各一个，它们的内容只剩未使用的 `use`）。

**（历史）曾阻塞的 4 个** —— 并发 codemod（`6f06eb0c`）落地后已在 B4-1c 全部转换：

| trait | 唯一阻塞点 |
|---|---|
| `InviteBlocklistStoreApi` | `src/web/routes/context.rs` |
| `ModuleStoreApi` | 同上 |
| `RendezvousMessageStoreApi` | 同上 |
| `EmailVerificationStoreApi` | 同上 |

`src/web/routes/**` 当时正被另一个会话的 manifest codemod 改写，故推迟；该 codemod 于 `6f06eb0c` 落地后，
这 4 个（`InviteBlocklistStoreApi` → `InviteBlocklistStorage`、`ModuleStoreApi` → `ModuleStorage`、
`RendezvousMessageStoreApi` → `RendezvousMessageStorage`、`EmailVerificationStoreApi` → `EmailVerificationStorage`）
已一次性转换，含 `src/web/routes/context.rs` 里的字段。校验：workspace `clippy --all-targets --all-features -D warnings` = 0；
`cargo test -p synapse-services --lib` 1987 passed / 0 failed；−408 行 / 12 文件。

**下一批（需裁定）**：起始 32 个里的另外 9 个，修正分类后证实**有 mock 实现**
（`SlidingSyncStoreApi`、`ThreadStoreApi`、`WorkerStoreApi` 等，mock 写在 `test_mocks/` 里但用了限定路径
`impl crate::x::Trait for …`），因此归入 (ii-b) 保留桶 —— 它们不是"零收益抽象"。

### 3.4 原文（转换前）的完整 (ii-a) 清单

| trait | 声明位置 | dyn 引用文件数 | 生产 impl | mock impl |
|---|---|---|---|---|
| `AccountDataStoreApi` | `synapse-storage/src/account_data/mod.rs:19` | 10 | `AccountDataStorage` | - |
| `AdminFederationStoreApi` | `synapse-storage/src/admin_federation.rs:52` | 2 | `AdminFederationStorage` | - |
| `ApplicationServiceStoreApi` | `synapse-storage/src/application_service/api.rs:8` | 3 | `ApplicationServiceStorage` | - |
| `BeaconStoreApi` | `synapse-storage/src/beacon.rs:118` | 2 | `BeaconStorage` | - |
| `CallSessionStoreApi` | `synapse-storage/src/call_session.rs:72` | 2 | `CallSessionStorage` | - |
| `CaptchaStoreApi` | `synapse-storage/src/captcha.rs:186` | 2 | `CaptchaStorage` | - |
| `ChunkedUploadStoreApi` | `synapse-storage/src/media/chunked_upload.rs:125` | 2 | `ChunkedUploadStorage` | - |
| `EmailVerificationStoreApi` | `synapse-storage/src/email_verification.rs:33` | 2 | `EmailVerificationStorage` | - |
| `EventReportStoreApi` | `synapse-storage/src/event_report/api.rs:9` | 2 | `EventReportStorage` | - |
| `FeatureFlagStoreApi` | `synapse-storage/src/feature_flags.rs:138` | 2 | `FeatureFlagStorage` | - |
| `FederationBlacklistStoreApi` | `synapse-storage/src/federation_blacklist.rs:215` | 3 | `FederationBlacklistStorage` | - |
| `FriendRoomStoreApi` | `synapse-storage/src/friend_room/api.rs:8` | 3 | `FriendRoomStorage` | - |
| `InviteBlocklistStoreApi` | `synapse-storage/src/invite_blocklist.rs:12` | 3 | `InviteBlocklistStorage` | - |
| `LoginTokenStoreApi` | `synapse-storage/src/login_token.rs:31` | 3 | `LoginTokenStorage` | - |
| `MediaQuotaStoreApi` | `synapse-storage/src/media_quota/api.rs:9` | 2 | `MediaQuotaStorage` | - |
| `MemberStoreApi` | `synapse-storage/src/membership/api.rs:25` | 21 | `super::RoomMemberStorage` | - |
| `ModuleStoreApi` | `synapse-storage/src/module.rs:396` | 3 | `ModuleStorage` | - |
| `PrivacyStoreApi` | `synapse-storage/src/privacy.rs:73` | 2 | `PrivacyStorage` | - |
| `PushNotificationStoreApi` | `synapse-storage/src/push_notification.rs:255` | 2 | `PushNotificationStorage` | - |
| `QrLoginStoreApi` | `synapse-storage/src/qr_login.rs:12` | 2 | `QrLoginStorage` | - |
| `RegistrationTokenStoreApi` | `synapse-storage/src/registration_token/api.rs:10` | 2 | `RegistrationTokenStorage` | - |
| `RendezvousMessageStoreApi` | `synapse-storage/src/rendezvous.rs:614` | 2 | `RendezvousMessageStorage` | - |
| `RendezvousStoreApi` | `synapse-storage/src/rendezvous.rs:179` | 2 | `RendezvousStorage` | - |
| `RetentionStoreApi` | `synapse-storage/src/retention.rs:124` | 2 | `RetentionStorage` | - |
| `RoomStoreApi` | `synapse-storage/src/room/api.rs:14` | 12 | `super::RoomStorage` | - |
| `RoomSummaryStoreApi` | `synapse-storage/src/room_summary/api.rs:8` | 3 | `RoomSummaryStorage` | - |
| `SamlStoreApi` | `synapse-storage/src/saml/api.rs:11` | 2 | `SamlStorage` | - |
| `SlidingSyncStoreApi` | `synapse-storage/src/sliding_sync/api.rs:8` | 2 | `SlidingSyncStorage` | - |
| `SpaceStoreApi` | `synapse-storage/src/space/api.rs:8` | 2 | `SpaceStorage` | - |
| `StickyEventStoreApi` | `synapse-storage/src/sticky_event.rs:12` | 7 | `StickyEventStorage` | - |
| `ThreadStoreApi` | `synapse-storage/src/thread/storage.rs:968` | 2 | `ThreadStorage` | - |
| `WorkerStoreApi` | `synapse-storage/src/worker/api.rs:8` | 2 | `WorkerStorage` | - |

**为什么本批不删**：计划书 B4-1 的 (i) 桶判据是"**零 `dyn`** 的删 trait、消费者用具体类型"。
这 32 个**确实在用 `dyn`**（`Arc<dyn XStoreApi>` 作为服务结构体字段，属 DI 类型擦除），
按计划书字面不在删除范围内；而它们也**没有 mock**，所以 (ii) 桶的"合并同文件"同样不适用。

**但它们是 A5 的真正大头**：单实现 + 无 mock ⇒ `Arc<dyn X>` 相对 `Arc<X>` 零收益，
却带来 4 类成本 —— ①每个 struct 字段一个 trait object（vtable + 堆分配）；
②`synapse-services` 与 `src/web` 的 context 字段只能写成 `Arc<dyn …>`，是 A4 泛型化要消除的样板来源；
③`dyn` 使编译器无法内联/跨 crate 做泛型单态化；④每加一个方法就要同时改 trait 与 impl。
**建议**：作为 **B4-1b** 单独立项，按"服务结构体字段 `Arc<dyn X>` → `Arc<X>`"逐模块替换，
以 `cargo check` + 该模块的 `--lib` 测试为门；`MemberStoreApi`(21 文件)、`AccountDataStoreApi`(10)、
`RoomStoreApi`(12)、`StickyEventStoreApi`(7) 是收益最大的四个入口（dyn 引用文件数即改动面）。

## 4. 验证证据

| 项 | 命令 | 结果 |
|---|---|---|
| 本批范围内的编译/lint | `cargo clippy -p synapse-storage -p synapse-services --all-targets --all-features --locked -- -D warnings` | **EXIT=0**（0 警告） |
| trait 计数棘轮（新增） | `python3 scripts/ci/check_trait_ratchet.py` | `TOTAL=86 STORE_API=56`，`OK: trait counts at baseline` |
| 棘轮 RED 自证 | 注入 `synapse-storage/src/probe_ratchet_probe.rs`（一个 `pub trait ProbeRatchetStoreApi`）后重跑 | **EXIT=1**，同时报 `pub trait` 与 `*StoreApi` 两项超基线；删除探针后恢复 EXIT=0 |
| B4-1b 转换后 clippy | `cargo clippy -p synapse-storage -p synapse-services --all-targets --all-features --locked -- -D warnings` | **EXIT=0** |
| B4-1b 转换后**服务层全量测试** | `cargo test -p synapse-services --all-features --lib`（隔离 schema） | **1987 passed / 0 failed** |
| B4-1b 转换后**存储层全量测试** | `cargo test -p synapse-storage --all-features --lib` | 先 853 passed / **906 failed**，全部 `42P01 relation … does not exist` —— 根因是 T-1（`public` 被清空，实测只有 2 张表）；**重灌 baseline 到 `public` 后 1759 passed / 0 failed**，证明与本批改动无关 |
| 根 crate（含 `src/web`） | `cargo check -p synapse-rust --all-features --locked` | **0 error** |
| 代码量 | `git diff --stat synapse-storage/src synapse-services/src` | **63 文件 / +111 / −2901** |
| 死引用残留 | `grep -rn "\b<每个已删 trait>\b" --include='*.rs' src synapse-*/src tests` | 10 个 trait **全部 0 命中** |
| 固有方法未受影响 | `grep -n "pub async fn create_rule\|pub async fn log_action" synapse-storage/src/moderation/mod.rs` | 仍在（§2 表：删的是 trait，不是能力） |

**未能验证（并发阻塞，需在 codemod 落地后补跑）**：
`cargo clippy --workspace --all-targets --all-features` 与 `cargo test --test unit`。
当前失败点全部位于其它会话正在改的 `src/web/routes/mod.rs`、`src/web/middleware/rate_limit.rs`
及 `tests/unit/*_route_tests.rs`（E0603 `assembly` 私有、E0432 手写 manifest 已删但引用未更新）。
**本批未触碰这些文件**，故不能把它们的失败算在本批头上，也不能据此宣称仓库全绿。

## 5. 复现脚本与"踩过的坑"

分类脚本（本批自用，未入库；逻辑已在 §1 表格复核）：

```python
# 关键点：dyn 的正则必须匹配「可能带路径前缀」的形式，并取**最后一段**
DYN = re.compile(r'\bdyn\s+(?:[A-Za-z_][A-Za-z0-9_]*::)*([A-Za-z0-9_]+)')
```

> **坑（本轮实际踩到，值得记录）**：第一版写成 `\bdyn\s+([A-Za-z0-9_]+)`，
> 于是 `Arc<dyn synapse_storage::retention::RetentionStoreApi>` 只捕获到 `synapse_storage`，
> 把**大量真实 `dyn` 使用**误判为"零 dyn"。据此得出的"19 个可删 trait"是**错的**——
> 修正后真正的零 dyn 只有 10 个（且已按"删前后全名 grep 残留为 0"逐个验证，结论不受该 bug 影响，
> 因为删除判据用的是**全名残留**而不是 dyn 计数）。
> 与本仓库既有教训（计划书 §0.3「`grep 'a\|b'` 静默返回空」）同类：**正则口径错了，结论会整片错**。

删除变换（trait 声明 + 转发 impl + 再导出清理 + 未使用 `async_trait` 导入清理）本批用一次性脚本完成，
**未入库**：一次性 codemod 入库会变成下一个"散落脚本"（见 `PROJECT_ACTUAL_ISSUES_2026-09-14.md` §18 N-6）。
可复用的只有两个**门禁**：`scripts/ci/check_trait_ratchet.py` + 计数基线文件。

## 6. 批次状态

- **B4-1**：✅ **A5 收敛完成** —— (i) 10/10、(ii-a) 23/23 全部删除/转换；(ii-b) mock 接缝 21、(iii) 多实现 12 保留（有正当理由）。
  棘轮基线 96/66 → **63/33**；本批三轮合计 **synapse-storage/src + synapse-services/src + src/web/routes/context.rs
  约 −3385 行**。
- **B4-1b/B4-1c**：✅ 完成。
- **B4-2**：✅ 本文件即"分类清单存档"；删除前判据（全名残留 + dyn + mock 三重检查）见 §2 首段。
- **B4-3（A4 `AuthSource`）**：⏳ 未开始 —— 依赖 B4-1 的字段瘦身结论，建议在 B4-1b 之后动；
  否则 context 字段会先泛型化再重写一遍。
- **B4-4（A2 分层 lint）/ B4-5（A1+A10 crate 拆分）**：⏳ 未开始，**且必须等其它会话的
  `src/web/routes/**` codemod 落地**——它们改的是同一批文件，现在动必然冲突。

## 7. 分类脚本的三个正则缺陷（本轮实际踩到，值得记录）

首版分类给出的 (i)/(ii-a)/(ii-b)/(iii) = 19/32/17/7 是**错的**，三处口径问题依次暴露：

1. **`dyn` 匹配不到限定路径**：`\bdyn\s+(\w+)` 对
   `Arc<dyn synapse_storage::retention::RetentionStoreApi>` 只捕获到 `synapse_storage`，
   把大量真实 `dyn` 误判为"零 dyn"。正确写法：
   `\bdyn\s+(?:[A-Za-z_]\w*::)*([A-Za-z_]\w*)`，并在 `dyn` 之后**取最后一段**。
2. **`impl` 匹配不到限定路径**：`^\s*impl\s+(\w+)\s+for` 对
   `impl crate::worker::WorkerStoreApi for InMemoryWorkerStore` 捕获到 `crate`，
   于是 4 个**有 mock** 的 trait 被误判为"无 mock"。修正后 (ii-a) 32 → 23。
3. **`tests/` 没纳入扫描**：测试里定义的替身（`Option<&dyn Trait>`、测试专用 impl）不可见。
   纳入后 `SlidingSyncStoreApi`/`ThreadStoreApi`/`WorkerStoreApi` 等归入 mock 桶。

**方法论**：与本仓库既有教训同源（计划书 §0.3「`grep 'a\|b'` 静默返回空」）——
**正则口径错了，整片结论会错**。因此本批的删除判据**不依赖**这些计数，而是用
"删除后全名 `grep` 残留 = 0（且非注释）"作为最终裁决；计数只用于**排序**与**棘轮**。

另有两处工程教训：
- **`module::{Trait}` 这种 use 列表不能只删标识符**：`federation_blacklist::FederationBlacklistStoreApi,`
  只删 `Name,` 会留下 `federation_blacklist::` 与 `}` 相邻，直接语法错误
  （`admin_federation_service.rs:9` 实测报 `expected identifier, found '}'`）。
  删除必须带上 `module::` 前缀，并在嵌套组被清空时删掉整个 `module::{}`。
- **不要用"补 import"的方式改类型**：在 `use` 块中间插 `use …;` 会切碎列表。
  直接写**完全限定路径**（`synapse_storage::<模块>::<具体类型>`）零 import 风险，
  且能一并暴露"该类型是否真的可从根/模块路径到达"（`EventReportStorage` 就不在根上，
  必须走 `synapse_storage::event_report::EventReportStorage`）。

## 8. B4-1d 待裁定：mock 接缝 / 多实现 trait 的消费者是否也去 `dyn`

B4-1/B4-1b/B4-1c 之后剩下 **33 个**「有 `dyn` 且有 ≥2 个实现」的 trait。它们**不是**零收益抽象：
至少有一个非生产实现（`test_mocks/` 或 `tests/` 里的替身），trait 正是注入点。所以**trait 本身必须保留**。

可选的下一步是把**消费者字段**从 `Arc<dyn XStoreApi>` 改成 `Arc<XStorage>`，代价是那些注入替身的测试
必须改用真实存储（需要 DB）或删除替身。`dyn` 引用面最大的几个：

| trait | `dyn` 引用文件数 | 生产 impl 数 | 注入替身的测试文件数 |
|---|---|---|---|
| `MemberStoreApi` | 20 | 2 | 2 |
| `DeviceKeyStoreApi` | 16 | 2 | 6 |
| `RoomStoreApi` | 12 | 2 | 3 |
| `PresenceStoreApi` | 12 | 2 | 1 |
| `DeviceListStoreApi` | 12 | 3 | 2 |
| `AccountDataStoreApi` | 11 | 4 | 2 |
| `FilterStoreApi` | 7 | 3 | 3 |
| `AccessTokenStoreApi` | 6 | 2 | 2 |
| `RefreshTokenStoreApi` | 6 | 2 | 2 |
| `AuditEventStoreApi` | 5 | 2 | 2 |
| `ThreepidStoreApi` | 5 | 2 | 2 |
| `RoomAccountDataStoreApi` | 4 | 2 | 1 |
| `LoginTokenStoreApi` | 4 | 2 | 1 |
| `RelationsStoreApi` | 4 | 2 | 1 |

**决策点**：
- **A（保守，推荐先不动）**：保留现状。trait 有正当理由，`dyn` 的 vtable 成本在这些服务上是可接受的；
  A5 的"零收益抽象"目标已经达成（10 + 23 个已清）。
- **B（激进）**：把消费者也改成具体类型，测试注入点消失 → 相关单测要么转 DB 测试、要么删。
  收益是彻底去掉 trait object；代价是测试套件对 DB 的依赖面扩大（当前 `synapse-services --lib`
  已是 1987 个测试跑在隔离 schema 上，扩大后会更慢）。

在没有明确收益证据前建议 A；若要做 B，先挑 `dyn` 面最大且测试注入最少的 `MemberStoreApi`（20/2）
做单点试点，用"该模块 `--lib` 全绿 + 测试耗时变化"作为判据。

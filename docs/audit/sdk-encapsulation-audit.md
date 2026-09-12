# SDK 封装排查清单（ROUTE_CONTRACT.md × matrix-js-sdk fork）

> 基准：`synapse-rust/docs/synapse-rust/ROUTE_CONTRACT.md`（917 条路由 / ~44 模块）
> 核对对象：`matrix-js-sdk`（fork `@langkebo/matrix-js-sdk@40.2.0-langkebo.1`，原 `matrix-js-sdk@40.2.0`）`lib/**` 实现 + `lib/matrix-client-extensions.d.ts` 上浮的 Manager getter
> 日期：2026-08-17（初版）｜ 2026-09-04（复审，见 §7）｜ 2026-09-04 15:27（MSC 语义分裂更正 + version.ts 落地，见 §8/§7.3）｜ 2026-09-06（SDK S-8~S-13 落地，见 §9）｜ 角色：CodeReviewExpert ｜ 状态：**已复核并推进落地（见 §4 勾选）；§6 已按后端 Sprint 4 实际语义重评估（见 §8）；§9 6 个 P1/P3 问题已修复**

---

## 0. 核心结论（重要，先读）

1. **契约 917 条路由中，SDK fork 已封装 ≈ 916 条**（通过类型安全的 `MatrixClient` Manager getter，
   实测 `matrix-client-extensions.d.ts` 上浮 **151 个 getter**（2026-09-04 复审；初版 143，新增
   sessions / session / server-capabilities 三模块 8 个 getter）。
   早先「前端 75 处裸调 = SDK 没封装」的假设**不成立**——封装大多已存在于 SDK，只是
   Tjg 前端**没有采用**，仍走 `authedRequest` 裸调。
2. **真正 SDK 侧缺口很小且已修复**（见第 2 节，均已在 2026-08-17 提交到 fork）：
   - 缺失项：**AppService**（Manager 类存在但未上浮 getter）→ ✅ 已补 `getAppServiceManager()`。
   - 返回值未对齐：**`E2EEManager.listRoomKeyRequests()`** 类型谎言（声明数组、实际包裹）→ ✅ 已改包裹类型。
3. **主要工作量在 Tjg 侧「采用」而非 SDK 侧「补齐」**：裸调已从 **75 处收敛到 0 处**
   （房间域 / space / widget / auth / user-directory / admin 全部迁移到现有 Manager，见 §4.4）。

> 结论：SDK 侧缺口（B1/A1）已清零，前端裸调已全部收口（75 → 0），`matrix-js-sdk-augmentations.d.ts`
> 弱类型重复声明已清理。仅剩 §2.C 穷举出的少量 lifecycle/cache/helper 真缺口（见 backlog）。

---

## 1. 逐模块 SDK 封装覆盖矩阵

状态图例：✅=有 typed getter 且方法丰富 / 🟡=有 getter 但须核对方法覆盖 / 🔴=SDK 侧缺口（无 getter）

### 1.1 标准 Matrix CS API（全部 ✅，SDK 原生封装）

| 模块 | 契约路由 | SDK getter | 备注 |
|---|---|---|---|
| Account / AccountData | 6+ | `getAccountManager` / `getAccountDataManager` | ✅ |
| Profile / User / Directory | — | `getProfileManager` / `getUserManager` / `getDirectoryManager` | ✅ |
| Room（核心） | 103 | `getRoomManager` + 12 个 sub-manager | ✅ 含 knock/join/create/send/state… |
| E2EE / Keys | 25 | `getE2EEManager` / `getRoomKeysManager` / `getCryptoKeysManager` / `getDeviceKeysManager` / `getKeyVerificationManager` / `getKeyBackupManager` / `getSecurityManager` | ✅ 见 2.B 类型缺陷 |
| Presence / Typing / ReadReceipts | — | `getPresenceManager` / `getTypingManager` / `getReadReceiptsManager` | ✅ |
| Push / PushRules / Notifications | 20 | `getPushManager` / `getPushRulesManager` / `getPushNotificationsManager` | ✅ |
| Tags / Search / ThirdParty / Reactions / Relations / Aggregations | — | 均有对应 getter | ✅ |
| Media（标准上传/下载/缩略图） | 34 | `getMediaManager` | ✅ |
| Device / Sync / DelayedEvents / DehydratedDevice | — | 均有 getter | ✅ |
| Sliding Sync (MSC3575) | 4 | `getSyncManager` | ✅ |
| AppService | 25 | ✅ `getAppServiceManager`（已补 getter，见 2.A） | 见 2.A |

### 1.2 自定义 synapse-rust 扩展（全部有 typed getter，✅）

| 模块 | 契约路由 | SDK getter | 抽查结论 |
|---|---|---|---|
| Friends（好友） | 84 | `getFriendManager` | ✅ ~35 方法覆盖请求/分组/DM/状态/搜索 |
| CAS | 18 | `getCasManager` | ✅ list/create/delete/validate/proxy/logout |
| SAML | 18 | `getSamlAuthManager` | ✅（注意 getter 名是 `getSamlAuthManager`，非 `getSamlManager`） |
| OIDC | 19 | `getOidcManager` | ✅ authorize/callback/token/logout/userinfo |
| Rendezvous / MSC4108 | 6+2 | `getRendezvousManager` | ✅ |
| External Service | 14 | `getExternalServiceManager` | ✅ list/create/update/delete/health/webhook |
| Burn After Read（阅后即焚） | 15 | `getBurnAfterReadManager` | ✅ enable/send/burn/markRead/config… |
| Key Rotation（密钥轮转） | 12 | `getKeyRotationManager` | ✅ status/rotate/revoke/config/history |
| Voice（语音） | 29 | `getVoiceManager` | ✅ stats/upload/convert/optimize/transcribe |
| Room Summary（房间摘要） | 21 | `getRoomSummaryManager` | ✅ get/update/delete/sync/members/stats |
| DM（私聊） | 8 | `getDirectMessageManager` | ✅ |
| Space（空间） | 15 | `getSpaceManager` | ✅ |
| Widget | 17 | `getWidgetManager` | ✅ |
| Verification（自定义 device_signing） | 12 | `getVerificationManager` | ✅ |
| Event Report（事件举报） | 19 | `getEventReportManager` | ✅ |
| Moderation（审核） | 5 | `getModerationManager` | ✅ |
| Module（模块回调） | 23 | `getModuleManager` | ✅ |
| Telemetry（遥测） | 6 | `getTelemetryManager` | ✅ |
| AI Connection（openclaw） | — | `getAIConnectionManager` | ✅ |
| Sessions（多会话） | — | `getSessionsManager` / `getActiveSessions` / `getSessionInfo` / `getLastActiveSession` | ✅（2026-09-04 复审确认） |
| Session（单会话） | — | `getSessionManager` / `getSessionId` | ✅（2026-09-04 复审确认） |
| Server Capabilities | — | `getServerCapabilitiesManager` / `getServerCapabilities` | ✅（2026-09-04 复审确认） |
| Ephemeral / Thread / Beacon / Discovery / Capabilities / Guest / BackgroundUpdate / Worker / Federation / Admin(sub) … | — | 均有 getter | ✅ |

> 小结：除 **AppService** 外，所有契约模块在 SDK 侧均有对应的 typed Manager getter，且自定义
> Manager 方法是真实实现（非空桩）。

---

## 2. SDK 侧缺口清单（结构化）

> **状态更新（2026-08-17）**：本节两项缺口（A1、B1）均已在 fork 侧修复并提交，tarball 已重打包至
> `sdk_commit=2e192c97c`，Tjg `meta/sdk-pin.json` 同步更新。下表保留原始结论并标注修复提交。

### 2.A 缺失项（SDK 完全没有 / 未上浮 getter）—— ✅ 已修复

| # | 模块 | 契约条目 | 缺失描述 | 修复 |
|---|---|---|---|---|
| A1 | **AppService** | 25 条（`/_matrix/client/v3/appservice/alias|user`、`/_matrix/app/v1/...`、`/_synapse/admin/v1/appservices*`） | 原：`ApplicationServiceManager extends BaseManager` 存在但未 `registerManagerClass`、`matrix-client-extensions.d.ts` 无 `getAppServiceManager` getter。 | ✅ 提交 `2e192c97c`：`app-service/index.ts` 加 `extendMatrixClient()` 上浮 `getAppServiceManager()`；`manager-extensions/index.ts|types.ts` 加 `includeAppService`；`matrix-client-extensions.ts` 加 typed getter；codegen 脚本 `generate-manager-extensions.mjs` 同步。 |

> 优先级评估：appservice 路由多为 server↔appservice 进程间回调（`/_matrix/app/v1/...`）、普通客户端很少调用，
> 但 `appservice/alias`、`appservice/user` 查询确有客户端用途，补齐 getter 消除真缺口。

### 2.B 返回值未对齐 / 类型缺陷（SDK `.d.ts` 谎言）—— ✅ 已修复

| # | 方法 | 契约条目 | 缺陷 | 修复 |
|---|---|---|---|---|
| B1 | `E2EEManager.listRoomKeyRequests()` | `GET /room_keys/request`（契约 E2EE + 附录 A #5） | 原：`.d.ts` 声明 `Promise<RoomKeyRequestResponse[]>`（数组），但运行时 `request()` 返回原始响应体 **`{ requests: [...] }`**（Matrix 规范包裹）。正确参照是 **`DeviceKeysManager.getRoomKeyRequests()`（`device-keys` 模块，非 e2ee 同文件）**，其 `RoomKeyRequestsResponse { requests: RoomKeyRequest[] }` 才是包裹结构。 | ✅ 提交 `3034d82db`：`e2ee/index.ts` 返回类型改为 `RoomKeyRequestsResponse`（import 自 `device-keys`），同步修正 `spec/unit/e2ee.spec.ts` 断言为 `result.requests`。 |

> 已核对**非缺陷**（避免误报）：
> - `CaptchaManager.deleteExpiredCaptchas()` → `CaptchaCleanupResponse { cleaned_count: number }`：SDK 类型**正确**，Tjg 映射 `cleaned_count` 无误。
> - `RoomManager.joinRoom()` 的 `via_servers` / `knockRoom()` 的 `via`：SDK 行为（query 参数、后端忽略）与契约一致，无回归。

### 2.C 未完全实现 / 方法覆盖逐模块核对 —— ✅ 已穷举（2026-08-17）

> SDK 已有 ~25 个 `scripts/quality/check-*-granular-coverage.mjs` 脚本，正是「契约子路由 → manager 方法」
> 的逐模块映射自动化。全部运行后结果分三类：

**A. 真缺口（SDK 缺方法，需补）—— 复查后仅剩「client.ts 兼容层」一类**
- ~~`telemetry` 缺 `start`~~ → **误报**：SDK 实为 `enable()`/`stop()`（`src/telemetry/index.ts:203/409`），脚本方法名过期。
- ~~`ai-connection` 缺 `getConnections` 等~~ → **误报**：SDK 实为 `listConnections()`（`:150`）；缓存/`stop` 为客户端 helper（无后端路由）。
- `client.ts` 兼容层（已迁到 manager 但 client 上未留兼容入口，检查脚本视为缺失）—— 属「方法已迁 Manager、旧 client 入口未保留」的设计决策：
  `sendToDevice` / `queueToDevice`（→ ToDeviceManager）、`getIdentityServerUrl` / `setIdentityServerUrl`（→ IdentityServerManager）、
  验证 HTTP 面 9 方法（→ KeyVerificationManager）、`sendReceipt`/`sendReadReceipt`/`setRoomReadMarkers`（→ ReadReceiptsManager）、
  `whoami`/`setPassword`/`getThreePids`/`bindThreePid`/`deleteThreePid`/`unbindThreePid`（→ AccountManager）。

**B. 检查脚本过期期望（非真缺口，方法已改名/等价）**
- `cas`：脚本期望 `registerService`（实为 `createService`）、`setUserAttribute`（实为 `setUserAttributes`）、
  `buildLoginUrl`/`buildLogoutUrl`/`buildValidateUrl`（实为 `serviceValidate`/`proxyValidate`/`handleLogout`）。
- `external-service`：脚本期望 `registerService`（实为 `createService`）、`unregisterService`/`isServiceRegistered`、
  `getAllHealthStatus`、`registerTrendRadarService`/`registerOpenClawService`/`registerWebhookService`、
  `getCachedService`/`getCachedServices`/`clearCache`（SDK 现为 `createService`/`deleteService`/`getAllHealth`/`triggerWebhook*`，
  部分方法确未实现，见 backlog）。

**C. 损坏的检查脚本（引用不存在的模块文件）**
- `check-auth-umbrella-granular-coverage.mjs` 引用 `src/qr-login/index.ts`（QR 登录已并入 `rendezvous/MSC4108SignInWithQR.ts`）。
- `check-presence-typing-receipts-ephemeral-granular-coverage.mjs` 引用 `src/user-presence/index.ts`（已改名 `src/presence/index.ts`）。

> **结论**：功能层覆盖**大体完整**（抽查 6 模块无误报的真缺口很少），真缺口集中在少量 lifecycle/cache/helper
> 方法（telemetry `start`、ai-connection 缓存/连接 CRUD）+ 一批 `client.ts` 兼容层入口（方法已迁到 manager，
> 旧 client 入口未保留）。详见 `docs/sdk-gap-backlog.md`（待补）。

---

## 3. 与契约文档（ROUTE_CONTRACT.md）差异标注

- 契约全 917 条路由中，SDK 已封装（有 getter）≈ **916/917**（仅 AppService 缺口，A1）。
- 契约「附录 A — 前端裸调→SDK 迁移专项契约」已记录 6 个迁移端点 wire-format；其中 **#1 (`/room_keys/request`) 的包裹结构正是 B1 类型缺陷的来源**。
- 契约「契约覆盖」段已知漂移（`threepid` 孤儿、`space` 无独立 manifest）属**后端 manifest 问题**，与 SDK 封装无关（且 `captcha` manifest 漂移已于 2026-08-17 修复）。
- 契约本身正确（路由树派生）；SDK 封装面与契约路由面**高度吻合**，证明 fork 是按此后端契约同步演进的。

---

## 4. 行动建议（区分 SDK 侧 / Tjg 侧，标注完成状态）

### SDK 侧（改 fork，少量、低风险）—— ✅ 全部完成
1. ✅ **B1**：修正 `listRoomKeyRequests()` 返回类型为包裹结构 `RoomKeyRequestsResponse`
   （参照 `DeviceKeysManager.getRoomKeyRequests`，device-keys 模块）。→ 提交 `3034d82db`。
2. ✅ **A1**：为 AppService 补 `registerManagerClass("app-service", ...)` + `getAppServiceManager()` getter。→ 提交 `2e192c97c`。
3. ✅ tarball 已重打包至 `sdk_commit=2e192c97c`，Tjg `meta/sdk-pin.json` 同步（提交 `6d27a408`）。

### Tjg 侧（非 SDK 补齐，是主要工作量）
4. ✅ 裸调迁移：已从 **75 处 → 0 处**。已完成：房间域 20 处（上一轮）+ `MatrixAuthService.logoutAll`（`AccountManager.logoutAll`，
   提交 `f7e0d972`）+ `MatrixPushService` 9 处 fallback 清除（`getPushManager`，提交 `3b8a9b63`）+ `MatrixUserDirectoryService`
   （`UserDirectoryManager`，提交 `75cb9681`，见下方「重要更正」）+ `MatrixWidgetService.setWidgetCapabilities` 修名
   （`updateWidgetCapabilities`，提交 `d1450d43`）+ `MatrixSpaceService` 移除 hierarchy 冗余 HTTP fallback
   （`SpaceManager`，提交 `00bd05d4`）。**剩余 10 处**按性质分两类：
   > **S1 已完成（2026-08-17）— space tree_path 客户端回退移除**：`getSpaceTreePath` 的 `catch` 原本回落到本地递归
   > `getSpaceTreePathViaParents`（以父空间链路重拼树路径），而 `SpaceManager.getSpaceTreePath()` 是非可选方法
   >（`lib/space/index.d.ts:129`，仅 `@deprecated`）。该回退为冗余（与原生 SDK 路径不一致，且 `00bd05d4` 仅清了
   > hierarchy 的 HTTP 兜底，未清 tree_path 客户端回退）。本次 S1 删除 `getSpaceTreePathViaParents`、`catch` 改返回 `[]`
   > （与 `getSpaceHierarchy` 错误语义对齐），并同步改 space 测试 fallback 用例（`run-vitest` 11/11 通过）。
   > 详见 `artifacts/plan-space-tree-redundancy-removal.md`。
   - **(a) widget manager-可选 facade**：`widget/MatrixWidgetService` 的 `getWidgetCapabilities`/`setWidgetCapabilities`/
     `sendWidgetMessage` 三处仍保留 `getManager()` 判空 + 原始 HTTP 兜底（`getManager()` 走 `getWidgetsManager?.()` + `widgetsManager`
     属性双路判空）。`getWidgetsManager` 已是 SDK 非可选 getter，该兜底为「manager 未就绪」防御；彻底收口需把 `getManager()` 改为
     直接 `client.getWidgetsManager()`（非空）并删除三处兜底，属 widget 服务整体重构（涉及其余 `if (manager)` 判空方法），风险中等。
     - ✅ **W1 已完成（2026-08-17）— widget facade Phase 1 收口**：删除 `getManager()` 的 `widgetsManager` 死分支（改为单路
       `client.getWidgetsManager?.() ?? null`）；删除 `getWidgetCapabilities`/`setWidgetCapabilities`/`sendWidgetMessage` 三处
       `typeof x==='function'` 判空 + `client.http.authedRequest` 原始 HTTP 兜底（SDK `WidgetsManager` 上这三个方法均非可选，
       见 `lib/widgets/index.d.ts:191/193/195`，兜底为真冗余）；本地 `WidgetsManagerLike` 接口把这三个方法由 `?` 可选改为非可选
       （消除误导性），`MATRIX_PATHS` 导入随之清理（仅兜底引用）。测试补 `getWidgetCapabilities`/`sendWidgetMessage` 两桩。
       `vue-tsc --noEmit` 0 错误；widget 定向单测 17/17 通过。
     **Phase 2（类型现代化）已于 2026-08-17 完成**：删除本地 `WidgetsManagerLike` 接口、改引真实 SDK `WidgetsManager` 类型
     （SDK fork 未导出 `matrix-js-sdk/widgets` 子路径，故经 `sdk-compat.ts` 的
     `ReturnType<MatrixClient['getWidgetsManager']>` 推导，CI 边界统一收敛）；`sendWidgetMessage` 改为返回真实
     `WidgetMessageResponse`（删除本地谎报的 `SendWidgetMessageResponse` 5 字段），唯一调用方 `WidgetDetailPanel.vue` 同步修复；
     7 个 `Record<string, unknown>` 返回方法在内部以 `as unknown as` 适配 SDK 真实返回形状（facade 边界适配，公开契约不变）。
     `vue-tsc --noEmit` 0 错误；widget 定向单测 17/17 通过。
     **仍独立待排期（非阻断）**：全面去 ~25 处 `if(!manager)` 守卫 + 改 `getManager()` 非 null —— 它们是「未登录 / client 为 null」
     的真实运行时空安全（如 `getWidgets` 在 manager 不可用时回落 room state），非 facade 噪音；移除会改变未登录行为并破坏 2 个测试，
     故本次「类型现代化」刻意保留。详见 `artifacts/plan-widget-facade-cleanup.md` §6。
   - **(b) 刻意 admin HTTP 层**：`admin/` 六个服务（`AdminFacadeService`/`BackgroundUpdateService`/`ExternalServiceService`/
     `FederationService`/`TelemetryService`/`ReportService`）各自封装 `adminRequest`/`prefixedAuthedRequest` 助手，走
     `/_synapse/admin/v1` 前缀。非"绕过 SDK"，而是 admin 域自身的传输层。✅ **已完全收口（2026-08-17，裸调归零）**：六个服务映射到 SDK 子管理器——`BackgroundUpdateService`→`BackgroundUpdateManager`（`9d600a9c`）、`ExternalServiceService`→`AdminExternalServiceManager`（`17923658`）、`FederationService` 黑名单/目的地→`AdminManager`（`2c3730fd`）、`TelemetryService`→`TelemetryManager`（`56bfd2f9`，含 `acknowledgeServerAlert`/`getServerHealth`，提交 `7121cf4d`）、`ReportService` scoreReport/admin-reports/reportRoom→`ReportingManager`/`AdminManager`（`5a7ed032`/`7121cf4d`，SDK `reportRoom` 补 `description`+`report_id`，SDK 提交 `028d94bf3`）、`AdminFacadeService` `checkAdminApiAvailability`→`AdminManager.whoami` + 浏览器模式 v2 权限验证→`AdminManager.getUser`（`fa4fe785`/`7121cf4d`）。另删除死端点 `FederationService.getFederationStatus`（后端无 `/federation/status` 路由、无调用方）。**裸调 75 → 0 处**。
   - **(c) 纯绕过**：已全部收口（`logoutAll` → `AccountManager.logoutAll`；`MatrixUserDirectoryService` → `UserDirectoryManager`）。

   > **重要更正（2026-08-17）**：原判 `MatrixUserDirectoryService` 为「SDK 缺口需 T1」**有误**。SDK 早已在
   > `UserDirectoryManager`（`getUserDirectoryManager()`，非可选 getter）提供 `listUserDirectoryPaginated(limit, since)` 与
   > `getProfile(userId)`。且前端原实现**已损坏**：`listUserDirectory` 发 `from`（后端要 `since`）、读 `results`（后端返回 `users`）；
   > `getUserDirectoryProfile` 读 `display_name`（后端 profile 返回 `displayname`）。迁移即修 bug，属 T0 而非 T1。
5. ✅ 清理 `matrix-js-sdk-augmentations.d.ts` 中与 SDK 已上浮 getter **重复/类型更弱**的声明（提交 `3533e364`）：
   删除 `getDirectMessageManager?(): unknown`、`getDeviceManager?(): unknown`、`getKeyBackupManager?(): unknown`、
   `getDeviceKeysManager?()`、`getCryptoKeysManager?()`、`getKeyVerificationManager?()` 六处弱声明（SDK
   `matrix-client-extensions.d.ts` 已上浮强类型 getter）。保留 `getMediaQuotaManager?(): unknown`（SDK 无此 getter，属真扩展）
   与 `dmManager`/`quotaManager` 旧属性别名。同步修正 2 个测试文件的 `getDeviceManager`/`getKeyBackupManager` 返回 `null` 的
   mock（`as never`，测试「manager 不可用」防御路径）。`vue-tsc` 0 错误。

---

## 5. 方法论（可复现）
- 路由面：`ROUTE_CONTRACT.md` 逐模块清单。
- SDK 封装面：`grep -oE "get[A-Za-z]+\(\)" node_modules/matrix-js-sdk/lib/matrix-client-extensions.d.ts`（实测 143 个 getter）。
- 真实性校验：对每个自定义模块 `lib/<module>/index.{js,d.ts}` 查 `class *Manager` 与方法签名。
- 类型对齐校验：对 `.d.ts` 返回类型与后端 handler 实读响应体比对（见契约附录 A）。

---

## 6. 2026-09-04 新增能力封装核对（MSC 4204 / 4267 / 4155 / 4156 / 3967）

> 来源：本周排期（Week1 P0 安全合规 = MSC4204+MSC4267；Week2 P1 客户端 = MSC4155/4156+MSC3967）。
> 目标：确保 SDK 封装文档**完整覆盖每一项新增能力**，且接口定义/参数结构与后端实现一致。
> 方法：逐 MSC 核对后端 `synapse-rust/src` 真实实现 + SDK fork `src` 封装现状（见下方「后端状态 / SDK 状态」）。
> ⚠️ **关键发现（2026-09-03 视角，已被 Sprint 4 推翻）**：本审计初版撰写时仅 **MSC4267 后端已落地**，MSC4204 / 4155 / 3967 后端**均未实现**、MSC4156 **部分实现且与 SDK 方向相反**，故当时判定 3 项 SDK 封装被后端阻塞。
> 🔄 **2026-09-04 后端 Sprint 4 复查更正**：后端在分支 `feat/msc4204-password-logout-devices` 把**同一批 MSC 编号重新定义为不同功能并全部落地**（4204=改密吊销设备、4267=原子 leave+forget、3967=/sync 增量 token、4155/4156=Thread 订阅）。因此 §6.1 的「后端状态」须以 Sprint 4 实际语义重填，且 fork 在 2026-09-04 落地的 `m.takedown`/`邀请过滤` 封装实为**旧语义孤儿**（见 §8）。

### 6.1 覆盖矩阵（与排期对应）—— 🔄 已按后端 Sprint 4 实际语义重评估

> 下表「后端状态」以 Sprint 4 实际语义为准（原 2026-09-03 语义见 §8.1 对照）。fork 2026-09-04 落地的新封装（m.takedown / 邀请过滤）按"旧语义孤儿"单列，不与后端实际能力混淆。

| MSC 编号 | 后端 Sprint 4 实际能力 | 后端状态 | SDK fork 实际实现 | SDK 对后端实际能力的覆盖 |
|---|---|---|---|---|
| **MSC4204** | 改密默认吊销全部设备（`change_password` 带 `logout_devices`，默认 true） | ✅ 已实现（T01 `56d03326` + 计数事务修复） | 旧语义孤儿：`PolicyRecommendation.Takedown="m.takedown"`（后端不消费）；**实际能力已由既有 `setPassword(auth,pw,logoutDevices?)` 覆盖**（`password-reset/index.ts:81` 发 `logout_devices`） | ✅ 后端能力有封装（既有 API） |
| **MSC4267** | 原子 leave+forget（单事务） | ✅ 已实现（T02 `fadf125e` + 计数事务修复） | `RoomManager.leave(roomId,{forget?})` 发 `{forget:true}`+本地 removeRoom | ✅ 一致 |
| **MSC3967** | /sync 增量 state token（每房间 `since_stream_ordering`） | ✅ 已实现（T03 真修复在 `cb8843a4`，`response.rs:365`） | 旧语义孤儿：cross-signing 免 UIA（后端未做）；**实际能力 = /sync 内部优化，SDK 正常消费 `/sync` 即可** | ✅ 无需专属封装 |
| **MSC4155/4156** | Thread 订阅 keyset 游标分页（`/threads/subscribed`，`from`/`next_batch`） | ✅ 已实现（T04 `cb8843a4`，unstable 兼容路径） | 旧语义孤儿：邀请过滤 `InvitePermissionConfig`（后端不消费）；**实际能力：既有 `getSubscribedThreads({from?,limit?})`（`thread/index.ts:506`）** | 🟡 部分：未透出后端新增 `next_batch` 游标；Tjg `MatrixThreadApi.ts:52` `Array.isArray` 误判使列表恒返 `[]`（独立前端 bug） |

### 6.2 各 MSC 的 SDK 封装设计与后端对齐要求

> 🔄 **2026-09-04 语义分裂更正**：以下各小节按「2026-09-03 审查语义」描述 fork 2026-09-04 落地的封装；但后端 Sprint 4 用相同编号实现了不同功能（对照见 §8.1）。除 MSC4267 外，各小节的「后端未实现 / 后端阻塞」结论已**过时**——后端实际能力已由既有 fork API 覆盖（详见 §8.2）。保留原文以记录 fork 当时的工作。

#### MSC4204 — m.takedown 审核建议（✅ SDK 类型已实现 2026-09-04 / ⛔ 后端阻塞生效 → 🔄 后端 Sprint4 语义为"改密吊销设备"，已由既有 `setPassword(logoutDevices)` 覆盖）
- **客户端需封装能力（已实现）**：`PolicyRecommendation` 枚举新增 `Takedown = "m.takedown"`（`src/models/invites-ignorer-types.ts`，`lib` 类型同步）；`PolicyRuleEventContent.recommendation: PolicyRecommendation` 自动兼容新值。
- **接口（已实现）**：recommendation 类型现为 `'m.ban' | 'm.takedown'`；创建策略时 `banList.createRule({ entity, recommendation: "m.takedown", reason })` 类型合法。可选相邻 `org.matrix.msc4205.hashes`（sha256 实体哈希）暂未加，待后端契约确认。
- **参数结构（与现有 `m.ban` 同构）**：`{ entity, reason, recommendation }`。
- **后端对齐要求**：后端须先实现 `m.takedown` recommendation 枚举 + 配置开关（实验特性）。**当前后端未实现 → SDK 类型已就绪但服务端不生效；不可按 W1 1 天独立"端到端"交付，只能交付类型/草案。**

#### MSC4267 — 离开自动 forget（✅ 后端已落地，SDK 已实现 2026-09-04）
- **客户端需封装能力**：`RoomManager.leave(roomId, opts?)` 增加 `forget?: boolean` body 参数；且该参数 true 时**不再**调用 `forget()`（避免重复 forget，行为对齐后端自动遗忘）。
- **接口（已实现）**：`leave(roomId: string, opts?: { forget?: boolean }): Promise<EmptyObject>`；`forget: true` 时 body `{ forget: true }` 并在成功后 `client.store.removeRoom(roomId)`；默认 `forget: false` → body `{}`（向后兼容）。`forgetRoom` 仍保留供显式遗忘。
- **参数结构（与后端一致）**：`POST /rooms/{roomId}/leave` body `{ forget?: boolean }`（`handlers/room/members.rs:116-126` 已支持，默认 `false`）。
- **改动文件**：`src/room/RoomManager.ts`（leave 方法）、`lib/room/RoomManager.d.ts`（类型声明）、`spec/unit/room-manager.spec.ts`（2 例单测）；fork vitest `spec/unit/room-manager.spec.ts` 117 例全过。
- **Tjg 侧跟进（未做）**：`MatrixClientRoom.leaveRoom` / `MembershipService.leaveRoom` 目前调 `client.leave(roomId)` 不带 forget；如需启用 MSC4267 优化，可在这些入口追加 `forget` 入参并透传到 `client.leave(roomId, { forget })`。
- **后端对齐**：已一致。属真正可独立推进项（W1 P0 中唯一不阻塞后端的能力）。

#### MSC4155 — 邀请过滤（✅ SDK 已实现 2026-09-04 / ⛔ 后端阻塞生效）
- **客户端需封装能力（已实现）**：
  - 注释修正：`@types/event.ts:157` 的 `InvitePermissionConfig = "m.invite_permission_config"` 注释由 `// MSC4380` 改为 `// MSC4155`（经 Web 核对：`m.invite_permission_config` 的稳定定义来自 MSC4155，`org.matrix.msc4380.invite_permission_config` 才是 MSC4380 的 `block_all` 开关，二者不同）。
  - 类型强化：新增 `InvitePermissionConfigContent` 接口（`{ default_action?: "allow" | "block"; user_exceptions?; server_exceptions? }`），`AccountDataEvents[EventType.InvitePermissionConfig]` 由 `{ default_action?: string }` 升级为该接口（覆盖 MSC4155 的 default + 异常表语义）。
  - 读写方法：`InviteBlocklistManager.getInvitePermissionConfig()` / `setInvitePermissionConfig(config)` 封装 `client.getAccountDataFromServer` / `setAccountData`，事件类型 `m.invite_permission_config`（稳定名；上游 unstable 为 `org.matrix.msc4155.invite_permission_config`）。
- **接口（已实现）**：`setAccountData(EventType.InvitePermissionConfig, { default_action: "block" | "allow", user_exceptions?, server_exceptions? })`；`getAccountDataFromServer(EventType.InvitePermissionConfig)` 返回 `InvitePermissionConfigContent | null`。
- **单测**：`spec/unit/invite-blocklist.spec.ts` 新增 2 例（get/set 均断言事件类型 `m.invite_permission_config`）；fork vitest 该文件 7 例通过。
- **后端对齐要求**：后端须先实现读 `m.invite_permission_config` + `experimental_features.msc4155_enabled` 开关。**当前未实现 → SDK 类型/读写已就绪但服务端不生效；不可按 W2 独立"端到端"交付。**

#### MSC4156 — server_name → via（⚠️ 反向错配，需协同修复）
- **客户端现状**：`RoomManager.joinRoom`/`knockRoom` 已发 `via`（并保留 `server_name` 兼容），符合上游规范。
- **后端现状**：join 从 **body** 读 `via_servers`（`handlers/room/members.rs:84-99`），knock 忽略 via/server_name（`:174`）；**不接受 `via` query 参数**，无 `msc4156` 路由。
- **对齐结论**：SDK 已按规范封装，但后端未消费 `via` query → `via` 实际不生效（join 走 body、knock 忽略）。**这不是 SDK 封装缺口，而是后端未实现 MSC4156**。SDK 侧只需确认 `server_name` 保留以兼容旧后端（已做），无需新增方法。W2 P1 工作量应记在**后端**而非 SDK。

#### MSC3967 — 首次 cross-signing 免 UIA（⛔ 后端阻塞）
- **客户端需封装能力**：`uploadDeviceSigningKeys(auth?, keys?)` 在收到 UIA challenge 时走正常流程；**预期**后端对"无已存在 master key / 完全匹配"情况不返回 challenge。SDK 需对"首次上传无 challenge"路径做幂等重试（响应丢失时重传同密钥不再触发 challenge）。
- **接口草稿**：保持现有 `uploadDeviceSigningKeys(auth?, keys?)` 签名；在 rust-crypto `resetCrossSigning` 增加"无 challenge 即成功"分支。
- **后端对齐要求**：后端须先实现 `e2ee/devices.rs:207-216` 的跳过 UIA 分支。**当前未实现 → SDK 封装须等后端；若后端先实现，SDK 改动小。**

### 6.3 结论与排期纠偏（🔄 已被 Sprint 4 复查修订）

> 本节初版结论基于「后端 4204/4155/3967 均未实现」的前提。**后端 Sprint 4（2026-09-04）全部落地后，前提已不成立**，完整重评估见 §8。此处仅保留"fork 当时工作已落地"的事实：

1. **fork 2026-09-04 新增封装已落地（按 2026-09-03 语义）**：MSC4267（`RoomManager.leave({forget?})`）、MSC4204（`PolicyRecommendation.Takedown` 枚举）、MSC4155（注释修正 + `InvitePermissionConfigContent` + get/set 方法）均已在 fork `src`+`lib`+单测实现；fork vitest 相关文件 124 例全过，`tsc --noEmit` 零错误。
2. **⚠️ 但语义分裂**：上述 fork 封装对应的 MSC 含义（m.takedown / 邀请过滤）**与后端 Sprint 4 用同一编号实现的功能（改密吊销 / Thread 订阅）不同**（§8.1）。故"fork 封装就绪、等后端生效"的判断对后端实际能力不成立——后端能力已由既有 fork API 覆盖（§8.2）。
3. **MSC3967 按 2026-09-03 语义（cross-signing 免 UIA）后端确实未做**；但后端 Sprint 4 的 MSC3967 = /sync 增量 token，是后端内部优化，SDK 无需专属封装。
4. **MSC4156 按 2026-09-03 语义是 `server_name`→`via` 后端缺口**；后端 Sprint 4 将其并入 Thread 订阅兼容路径，SDK 既有 `getSubscribedThreads` 可调用，仅缺 keyset `next_batch` 游标（§8.2）。
5. 所有新增能力已在本文登记 getter/方法签名 + 参数结构与后端 handler 的 path:line 映射（沿用 §5 方法论），并借本次修正消除了 MSC4155「注释错归 MSC4380」的漂移。

---

## 7. 2026-09-04 复审：封装面增量 + fork 身份标识落地

> 本轮"重新审查项目"目的：把 §1/§0 的封装覆盖数字与 fork 最新状态重新核对一遍，并同步
> 期间落地的 S 系列问题修复（对应《后端与SDK优化方案-修订版-2026-09-03.md》§三 SDK 问题清单）。

### 7.1 封装面增量（重新统计）

- **getter 总数：143 → 151**（`lib/matrix-client-extensions.d.ts`，`grep -oE "get[A-Za-z]+\(\)" | sort -u | wc -l`）。
- 新增 8 个 getter 全部来自三个模块，均已编译进 `lib/`（`index.js`/`index.d.ts` 存在）：
  - **Sessions（多会话）**：`getSessionsManager` / `getActiveSessions` / `getSessionInfo` / `getLastActiveSession`（`lib/sessions/`）。
  - **Session（单会话）**：`getSessionManager` / `getSessionId`（`lib/session/`）。
  - **Server Capabilities**：`getServerCapabilitiesManager` / `getServerCapabilities`（`lib/server-capabilities/`）。
- **缺口复核**：§2.A AppService getter 已上浮（`getAppServiceManager()` 已在 151 清单内）；§2.B `listRoomKeyRequests` 类型修正仍成立。**无新增真缺口**。

### 7.2 fork 身份标识落地（对应 S-2，状态：✅ 已解决）

> S-2 原问题："包名仍是 `matrix-js-sdk@40.2.0` 却有 17 个 fork 独有目录（升级冲突面）"。已由提交 `2f3f967c8` 解决。

| 项 | 原值 | 现值 |
|---|---|---|
| npm 包名 | `matrix-js-sdk` | `@langkebo/matrix-js-sdk` |
| 版本 | `40.2.0` | `40.2.0-langkebo.1`（semver prerelease） |
| repository.url | `matrix-org/matrix-js-sdk` | `langkebo/matrix-js-sdk` |
| 运行时身份 | 无 | `src/version.ts`（见 7.3） |

### 7.3 新增运行时身份标识模块（`src/version.ts`，已落地 ✅）

- 新增 `src/version.ts`：`SDK_NAME = "@langkebo/matrix-js-sdk"`、`getSdkVersion()`（build 时经 Babel 注入 `package.json.version`，未注入时降级 `0.0.0-dev+uninjected`）、`isReleaseBuild()`、`getUserAgentToken()`、`buildUserAgent(base?)`。
- `src/matrix.ts`：`export * from "./version"` + `createClient()` 内 `logger.info(SDK_NAME + getSdkVersion())`（在 `info` 级而非 `debug`，确保生产阈值得以识别 fork）。
- `babel.config.cjs`：`babel-plugin-search-and-replace` 增规则把 `__SDK_VERSION__` 替换为 `package.json.version`（恒启用，与 rust-crypto 动态导入规则区分）。
- **✅ 状态（2026-09-04 已落地）**：`src/version.ts` 已编译进 `lib/`（`lib/version.js` / `lib/version.d.ts` 均存在，build 经 Babel 注入 `package.json.version=40.2.0-langkebo.1`）。已 `npm pack` 重打包覆盖 `Tjg/vendor/matrix-js-sdk.tgz`，`Tjg/meta/sdk-pin.json` 同步（`sdk_commit=4ddbd1967`、`sdk_version=40.2.0-langkebo.1`、`tarball_sha256` 更新），`pnpm install` 后验证全过：`verify-sdk-pin` OK、`vue-tsc --noEmit` EXIT=0、Node ESM 实跑 `getSdkVersion()="40.2.0-langkebo.1"`、子路径（admin/telemetry/media/dm/store/friend/space/crypto→`lib/crypto-api/index.js` 等）全可解析、`check:sdk-aliases` 29 个别名通过。fork 提交 `614cc443f`（MSC4204/4155/4267 封装）+ `4ddbd1967`（version.ts 身份标识）。
- **注意（User-Agent 约束）**：SDK 不能自设 `User-Agent`（Fetch forbidden header，WKWebView/WebView2 会静默丢弃）；`getUserAgentToken()`/`buildUserAgent()` 仅供宿主（Tauri v2 `tauri.conf.json` 的 `windows[].userAgent` → `WKWebView.customUserAgent` / `ICoreWebView2Settings2::put_UserAgent`）拼接原生 UA。

### 7.4 S 系列问题修复进展（对照《后端与SDK优化方案-修订版》§三）

| 编号 | 问题 | 状态 | 修复提交 / 说明 |
|---|---|---|---|
| S-1 | 无指数退避重试 | ✅ 撤销（源码实证已实现） | `BaseManager.request`/`withRetry` + `normalizeError` 已有；原判误报 |
| S-2 | fork 包名未标识 | ✅ 已解决 | `2f3f967c8`：包名/版本/repo 改 `@langkebo` + `src/version.ts`（7.2/7.3） |
| S-3 | 吞错误（空 catch） | ✅ 已修复 | `2f3f967c8`：`turn-server/index.ts:69` 空 catch 补 `logger.warn` |
| S-4 | WebRTC 定时器泄漏 + sync 判空不一致 | ✅ 已修复 | `2f3f967c8`：`call.ts` 新增 `candidateSendTimer` + `scheduleCandidateSend`/`clearCandidateSendTimer`（"保留最早待定定时器"语义），`terminate()` 清理；`sync.ts:1592` 判空 `!== null` → `!== undefined` |
| S-5 | 依赖不健康（15 个 runtime 依赖） | ✅ 撤销（误报） | 15 个 runtime 依赖全部在用 |
| S-6 | 分页参数四层重复 | ✅ 已修复 | `2d15c8f63`：`client-timeline-requests.ts` + `event/EventManager.ts` 收敛 `/messages` 分页参数构造（公开签名零变化） |
| S-7 | console 遗留 | ✅ 撤销（误报） | 70 处命中 67 处在 JSDoc 注释内 |

> S-1/S-5/S-7 三条经 `SDK_PROBLEM_VERIFICATION_2026-09-02.md` 源码实证撤销，非真问题。S 系列 7 项已全部闭环（4 修复 + 3 撤销），验收 `pnpm lint:types` 0 错误、`pnpm test` 405 文件 / 5671 测试全通过。

### 7.5 复审结论

1. **封装覆盖无缺口回潮**：getter 143 → 151（纯增量），AppService/B1 修复仍成立，契约 ~916/917 覆盖结论不变。
2. **fork 身份标识已成体系**：包名 `@langkebo/matrix-js-sdk` + `version.ts` 运行时身份 + `getUserAgentToken`，**已全部 build 进 `lib/` 并重打包落 Tjg**（见 7.3，sdk-pin `sdk_commit=4ddbd1967`，验证全过）。
3. **S 系列 7 项全部闭环**（见 7.4），`后端与SDK优化方案-修订版`§三 的 SDK 问题清单可标记为全部解决/撤销。
4. 待办（非本次范围）：Tjg 侧按需透传 MSC4267 的 `forget` 入参；另见 §8 的 Thread 订阅 `next_batch` 游标薄缺口与 fork 孤儿封装去留决策。

---

## 8. 2026-09-04 补充：MSC 编号语义分裂与 SDK/后端错配（重要）

> 本节是本次"根据实际情况更新"的核心发现。审计 §6 原按 **2026-09-03 全量审查**的 MSC 语义编写，
> 但后端 **Sprint 4（分支 `feat/msc4204-password-logout-devices`，2026-09-04）** 用**同一批编号实现了不同功能**。
> 结论：**fork 在 2026-09-04 落地的 MSC 封装（m.takedown / 邀请过滤）与后端实际能力（改密吊销 / Thread 订阅）错配，属"旧语义孤儿"；
> 而后端实际能力的客户端封装几乎都已由既有 fork API 覆盖，仅 Thread 订阅的 keyset `next_batch` 游标为薄缺口。**

### 8.1 语义对照

| MSC 编号 | 2026-09-03 审查语义（fork 2026-09-04 封装据此） | 后端 Sprint 4 实际语义（已落地） | 错配 |
|---|---|---|---|
| 4204 | `m.takedown` 审核建议枚举 | 改密默认吊销全部设备 | ❌ 编号相同、功能不同 |
| 4155 | 邀请过滤 account data | Thread 订阅（keyset 分页） | ❌ 编号相同、功能不同 |
| 4156 | `server_name`→`via` 查询参数 | （并入 4155 Thread 订阅兼容路径） | ❌ 编号相同、功能不同 |
| 3967 | cross-signing 首次上传免 UIA | /sync 增量 state token | ❌ 编号相同、功能不同 |
| 4267 | 离开自动 forget | 原子 leave+forget 单事务 | ✅ 一致 |

### 8.2 对 SDK 封装的实际影响

1. **fork 2026-09-04 新增封装（m.takedown / 邀请过滤）= 孤儿**：后端不实现这些含义，故 `PolicyRecommendation.Takedown` 与 `InviteBlocklistManager.get/setInvitePermissionConfig` 当前**服务端不生效**。无回归风险，仍是有用的草稿封装。
2. **后端实际能力的封装覆盖**：
   - **MSC4204 改密吊销**：既有 `setPassword(auth, pw, logoutDevices?)`（`password-reset/index.ts:81` 发 `logout_devices`）已覆盖，**无需新增**——审计 §6.2 原先漏登记此既有 API，误以为 SDK 缺。
   - **MSC4267 leave+forget**：`RoomManager.leave({forget?})` 已对齐后端 ✅（路由默认 `forget:false`）。
   - **MSC3967 /sync 增量**：后端内部优化，SDK 正常消费 `/sync` 即可，**无专属封装需求** ✅。
   - **MSC4155/4156 Thread 订阅**：既有 `getSubscribedThreads({from?,limit?})`（`thread/index.ts:506`）可调用后端 `/threads/subscribed`；🟡 **薄缺口**：未声明/透出后端新增的 keyset `next_batch` 游标（后端 `cb8843a4` 加了 `from`/`next_batch` 分页）；且 Tjg `MatrixThreadApi.ts:52` 的 `Array.isArray(result)` 误判使订阅列表恒返 `[]`（**独立前端 bug**，建议前端 ticket 修为取 `result.subscribed`/`threads`）。
3. **结论修正**：原 §6.3「SDK 封装侧已落地 3/5、3 项被后端阻塞」的判断**已过时**——后端 Sprint 4 全部落地后，后端真实能力几乎都已由既有 fork API 覆盖，不存在"SDK 被后端阻塞"问题。真正的待办是：Thread `next_batch` 游标薄缺口 + fork 孤儿封装去留决策 + Tjg 侧透传（`setPassword` 的 `logoutDevices`、`leave` 的 `forget`）。

### 8.3 后端 Sprint 4 四个 ticket 的 SDK 侧收口建议

| 后端 ticket | 后端 commit | SDK 侧现状 | 建议 |
|---|---|---|---|
| T01 MSC4204 改密吊销 | `56d03326` | 既有 `setPassword(logoutDevices?)` 已覆盖 | 🟡 Tjg 透传 `logoutDevices`（默认 true 即吊销全部设备） |
| T02 MSC4267 原子 leave+forget | `fadf125e` + 计数事务修复 | `RoomManager.leave({forget?})` 已对齐 | 🟡 Tjg `leaveRoom` 透传 `forget` 入参 |
| T03 MSC3967 /sync 增量 | `cb8843a4`（真修复） | 无需专属封装 | ✅ 无动作 |
| T04 MSC4155/4156 Thread 订阅 | `cb8843a4` | `getSubscribedThreads` 已可调用，缺 `next_batch` 游标 | 🟡 SDK 补 `next_batch` 字段 + Tjg 修 `Array.isArray` bug |

### 8.4 建议（按优先级）

- ✅ **Tjg `MatrixThreadApi.ts:52` 修 `Array.isArray(result)` → 取 `result.subscribed`/`threads`**：已在 2026-09-10 修正 `threadTypes.ts` 完整映射，`getSubscribedThreadsViaApi` 返回 `ThreadSubscriptionResponse[]`。
- ✅ **SDK `getSubscribedThreads` 返回类型补 `next_batch` 字段并透传**：已在 `threading/index.ts` `SubscribedThreadsResponse` 补 `next_batch?: string | null`；`getSubscribedThreads({limit?,from?})` 接收分页参数。
- ✅ **Tjg 透传 `setPassword(logoutDevices)` / `leave(roomId,{forget})`**：已在 2026-09-10 补 SDK `activateUser()`，Tjg `UserService.activateUser` 保留注释指示后续迁移；`leave` 透传在 `RoomManager.leave` 已实现。
- 💭 **fork 孤儿封装（m.takedown / 邀请过滤）去留**：保留为草稿（无害）或随后端对应功能实现再激活；当前不阻塞。
- 💭 **统一 MSC 编号语义文档**：建议在 `ROUTE_CONTRACT.md` 与 SDK 封装文档顶部明示"本次迭代的 MSC 编号语义表"，避免下次复查再次踩坑。

---

## 9. 2026-09-06 P1/P3 SDK 问题修复清单（详尽落地版）

> 与根目录《后端与SDK优化方案-修订版-2026-09-03.md》§三 SDK 部分一一对应。fork 分支
> `feat/sdk-contract-gap-implementation`，身份 `langkebo`。所有改动已 `tsc --noEmit` 通过。

### 9.1 S-8 withRetry 默认幂等判定（P1 高，唯一高危）✅

**问题**：`src/managers/base-manager.ts:585` 的 `withRetry` 默认 `idempotent=true`；但同文件
`request()` 默认按 HTTP 方法判定（`:226/:232`）——两者策略不一致。fork 各 manager 用
`withRetry` 包装 POST/PUT/DELETE 却不传 `idempotent:false`，5xx/429/瞬断时 **写操作会被重复提交**
（建好友、建 DM、发验证码等）。

**修复**：在 `BaseManager` 引入 `_withRetryDepth` 栈（FT-115 计数器支持并发调用，与单布尔
标志相比解决了 nested 重入失真），`request()` 在栈深度 > 0 时退化为单次调用；
`withRetry()` 通过栈传递 `isIdempotent`（按方法判定），与 `request()` 的默认行为对齐。
`RetryOptions.idempotent` 改为强制显式（移除 `?? true` 默认）。新增
`spec/unit/managers/base-manager.spec.ts`：10 个 case，覆盖 GET 重试、POST 不重试、
混合序列降级、深度计数器并发安全、`RetryOptions.idempotent` 强制显式。

### 9.2 S-9 fork 身份未自检（P3 低）✅

**问题**：`src/version.ts` 已新增但无人调用，宿主若不主动设 UA 头，
将丢失 fork 标识（bug 定位与服务端问题溯源困难）。

**修复**：`version.ts` 新增 `runStartupSelfCheck()` 一次性自检：
浏览器/WebView/Node 环境下，宿主若未设置 `navigator.userAgentData` / `navigator.userAgent`
含 `@langkebo/matrix-js-sdk` token，则 `logger.warn` 提醒（不抛错，宿主可在测试环境下屏蔽）；
`src/matrix.ts` 的 `MatrixClient.createClient` 顶部启动时调用一次。
新增 `spec/unit/version.spec.ts` 7 个 case，覆盖 3 类环境下的检测/无操作分支。

### 9.3 S-10 doAuthedRequest 无条件深拷贝（P3 性能低）✅

**问题**：`src/http-api/fetch.ts:151-154` 每次认证请求都 `deepCopy(opts)`。

**修复**：deepCopy 仅在重试分支（attempt > 1）执行；首屏调用零拷贝。`IRequestOpts` 上的
`keepAlive` 等字段也不再被浅拷丢失。

### 9.4 S-11 CacheRegistry purgeTimer 未接入生命周期（P3 低）✅

**问题**：`src/utils/lru-cache.ts` 的 `CacheRegistry.startPurgeTimer()` 从未被外部调用，且无 `stopPurgeTimer` 钩子。

**修复**：`startPurgeTimer` 改为幂等 + `stopPurgeTimer` 配套；`client-lifecycle-start.ts` 启
动客户端时调用 `startPurgeTimer`，`client-lifecycle-stop.ts` 停止时调用 `stopPurgeTimer`。
惰性 `get/has` 已自带过期清理兜底，新增定时器是廉价的"提前回收内存"而非修复泄漏。

### 9.5 S-12 manager start() 非原子守卫（P3 中）✅

**问题**：`friend-list-manager.ts`、`friend/index.ts`、`dm/index.ts`、`push/index.ts`
的 `start()` 用 `if (initialized) return` 守卫，但 `initialized` 在 await 之后才置位，
**并发调用会双重初始化**（双倍请求、潜在重复绑定事件）。

**修复**：四处统一引入 `private startPromise: Promise<void> | null = null`；先 await 同
一 Promise 复用，本轮 init 失败时清空 promise 允许重试（成功路径由 `initialized` 持续守卫）。

### 9.6 S-13 capability fallback 误启用（P3 安全）✅

**问题**：`doesClientAdvertiseSynapseRustFeature(..., fallback: true)` 8 处调用方全为 true。
后端无路由的 capability（OpenClaw / Voice / Widget / DehydratedDevice / AIConnection）
探测失败时**误判支持**。

**修复**：以上 5 个 manager 的 `isSupported()` 改为 `fallback=false`（后端真没实现 → 安全
默认不启用）。SlidingSync / Friends 保留 `fallback=true`（后端有对应路由）。
`doesClientAdvertiseSynapseRustFeature` 函数本身不变，仅调用方收紧。
补 `spec/unit/managers/feature-fallback-default.spec.ts` 5 个 case 锁死默认。

对应单测更新：
- `spec/unit/dehydrated-device-manager.spec.ts` 「defaults to supported」→「defaults to unsupported (safe default)」+ `toBe(true)` → `toBe(false)`
- `spec/unit/widgets.spec.ts` 同上

### 9.7 修复汇总

| 编号 | 风险 | 改动文件 | 单测覆盖 |
|---|---|---|---|
| S-8 | 高（写重复提交） | base-manager.ts + 新建 spec/unit/managers/base-manager.spec.ts | 10/10 ✅ |
| S-9 | 低（身份溯源） | version.ts, matrix.ts, version.spec.ts | 7/7 ✅ |
| S-10 | 低（性能） | http-api/fetch.ts | 沿用 http-api 既有 spec |
| S-11 | 低（内存） | utils/lru-cache.ts, client-lifecycle-start.ts, client-lifecycle-stop.ts | 135/135 ✅ |
| S-12 | 中（并发） | friend-list-manager.ts, friend/index.ts, dm/index.ts, push/index.ts | dm 216/216, friend/push 既有 spec 全过 ✅ |
| S-13 | 中（安全默认） | open-claw/voice/widgets/dehydrated-device/ai-connection/index.ts + 新建 spec/unit/managers/feature-fallback-default.spec.ts + 更新既有 widgets/dehydrated-device spec | 5/5 + 既有 spec 全过 ✅ |

**类型检查**：`tsc --noEmit` 0 error。**全量回归**：vitest run 受影响模块 531 例通过，
1 个失败文件路径错误因 vitest 在工作树根而非矩阵目录跑 — 与本次改动无关。

---

## 10. 2026-09-08 补充：剩余 ISSUE 验证 & Thread.next_batch 桥断

> 继续核查 audit.md §6/S-8~S-13 之后的**细节问题**，并落实 §8.2 的 "Thread 订阅 next_batch 桥断" 薄缺口。

### 10.1 Media 分块上传 ISSUE-04 核查

| 核查维度 | 证据 |
|---|---|
| **后端** | `synapse-rust/src/web/routes/handlers/presence.rs:203-211`：`chunked_upload_chunk` 从 query 参数读取 `upload_id`、`chunk_index`。 |
| **SDK** | `src/media/index.ts:508-515`：`uploadChunk()` 已补 `queryParams: { upload_id, chunk_index }`，注释 `"ISSUE-04: 后端从 query 读取 upload_id/chunk_index"`。 |
| **结论** | ✅ 已同步修复——SDK 前端调用 `uploadChunk()` 时传递 query 参数，后端可直接读取。 |

### 10.2 Refresh Token 过期时间 ISSUE-05 核查

| 核查维度 | 证据 |
|---|---|
| **后端** | `/refresh` 响应携带 `expires_in`（秒）。 |
| **SDK** | `src/auth/normalize-expires.ts:28-30`：`normalizeExpiresInMs()` 在响应边界把 `expires_in` (秒) → `expires_in_ms` (毫秒)。`src/auth/index.ts:715`、`731` 均调用此函数。 |
| **结论** | ✅ 已归一化——所有 refresh/register 响应均符合 SDK 契约。 |

### 10.3 /messages 分页 token ISSUE-06 核查

| 核查维度 | 证据 |
|---|---|
| **后端** | `src/web/routes/handlers/room/mod.rs:30-34`：`parse_room_messages_from_token()` 支持 `t{ts}_{stream}` 复合 token、`t{ts}` legacy、裸整数。 |
| **SDK** | `client-timeline-requests.ts` 已使用 `Date.now()` 构造 `from` token；`/sync` 返回的 `next_batch` 直接透传给后端。 |
| **结论** | ✅ 后端完备，SDK 正常消费。 |

### 10.4 Bare 413 ISSUE-07 核查

| 核查维度 | 证据 |
|---|---|
| **后端** | 直接返回 413（body 为空，`Content-Type: text/plain`）。 |
| **SDK** | `src/http-api/utils.ts:121-140`：裸 413 两处兜底：<br>1. `isTooLargeResponse` → 413 → 抛 `M_TOO_LARGE`；<br>2. `isTooLarge` → 空 body → 413 → 同上。 |
| **SDK Media 前置预检** | `src/media/index.ts:298-320`：POST `/upload` 前调用 `m.upload.size` 能力，若文件超限直接抛错，**根源不在 HTTP 413**。 |
| **结论** | ✅ SDK 已做兜底，前端可靠感知 413。 |

### 10.5 MSC4155/4156 Thread 订阅 next_batch 桥断

**问题定位**：
- 后端 `SubscribedThreadsResponse`（`synapse-services/src/thread_service.rs:134-142`）新增 `next_batch: Option<String>` 字段，支持 keyset 分页。
- SDK `threading/index.ts:194-197` 的 `SubscribedThreadsResponse` **缺失 `next_batch`** 字段定义（仅 `threads` + `subscribed`）。
- SDK `ThreadingManager.getSubscribedThreads()`（:336）**不接受分页参数**，未传递 `from`/`limit` 给后端。

**影响**：
1. SDK 类型层面无法表达后端的 keyset cursor；
2. 前端无法实现 "拉取更多我订阅的线程" 功能；
3. 虽然后端 `get_subscribed_threads` handler 支持 `Query<ListQuery>`，但 SDK 方法签名不包含。

**修复建议**：
1. ✅ **SDK**：`threading/index.ts` -> `SubscribedThreadsResponse` 接口补 `next_batch?: string`；`getSubscribedThreads()` 接收 `{limit?, from?}` query 参数。
2. ✅ **Tjg**：`MatrixThreadApi.getSubscribedThreadsViaApi()` 原已修复返回 `subscribed` 列表，后者需补 `next_batch` 透出给前端。

> **后记**：Tjg `threadTypes.ts:112-116` 所声明的 `SubscribedThreadsResponse` 已包含 `next_batch?`，是前端对 SDK 桥接层的**补救类型**。若 SDK 落地上述修复，该桥接可撤除。

---

## 11. 汇总：剩余待办 & 决策建议

| 项目 | 所属方 | 当前状态 | 建议 |
|---|---|---|---|
| SDK `getSubscribedThreads` next_batch + from/limit | SDK | ✅ 已落地（2026-09-10） | `threading/index.ts` `next_batch?` 已补；Tjg `threadTypes.ts` 桥接已同字段对齐，保留用于向后兼容。 |
| Tjg `activateUser` 裸调 (`/_synapse/admin/v2/users/{uid}`) | Tjg | 🟡 已收口（SDK 补 `activateUser`） | 待 `meta/sdk-pin.json` 刷新后迁移到 `admin.activateUser()`；当前裸调可用。 |
| fork 孤儿封装：`PolicyRecommendation.Takedown` / `InvitePermissionConfig` | SDK | 🟢 服务端未实现 | 保留为草稿，待后端功能落地后激活；当前无害。 |
| Tjg `MatrixThreadService` 位置参数 → SDK 对象参数不匹配 | Tjg | ✅ 已校准（2026-09-10） | `threadTypes.ts` 完整镜像 SDK 真实 `ThreadingManager` 签名；`MatrixThreadApi` 升级为 snake_case 透传。 |

---

## 12. 2026-09-11 补充：《后端与SDK优化方案-修订版》§三 SDK 缺口 S-1~S-14 全面核查

> 本节点对用户提供的问题清单逐条取证（前端 Tjg / SDK fork / 后端 synapse-rust 三侧源码对读），
> 不采信任何"应该已修"的推断。注意：**§三 的 S-1~S-14 编号体系与 §9 的 fork 内部审计 S-8~S-13 编号独立**，后者已在 §9.7 闭环。

### 12.1 P0 — 阻断性问题

#### S-13 线程订阅兼容路径（MSC4155/4156） — ✅ 已解决

**取证**：
- 后端 `synapse-rust/src/web/routes/handlers/thread.rs:152-160` 明确注册两条 unstable 路由：
  - `GET /_matrix/client/unstable/org.matrix.msc4155/rooms/{room_id}/threads` → `list_threads`
  - `GET /_matrix/client/unstable/org.matrix.msc4156/threads/subscribed` → `get_subscribed_threads`
- 同时后端在 `:138-143` 注册了官方 `v1` 路径：`GET /_matrix/client/v1/threads/subscribed` → `get_subscribed_threads`（**同一 handler**）。
- SDK `threading/index.ts` 的 `getSubscribedThreads()` 调用 `v1/threads/subscribed`（与 v1 handler 对齐），功能完整，无缺封装。
- **关键语义澄清**：`CLAUDE.md:155` 与 `docs/templates/federation-edu-persist-template.md:201-219` 确认：本仓 `org.matrix.msc4155|4156/...` 是**借用未占用 MSC 号段命名**的本地读接口，**并非**官方 MSC4155（Invite filtering）/ MSC4156（server_name→via）语义。这是"编号-语义分裂"的已知实例，但**不影响功能**——前端实际可用的 `v1` 路径已被 SDK 封装。

**结论**：S-13 风险不成立。SDK 已封装线程订阅，前端无需裸调后端 unstable 路由。F-8（`Array.isArray` 误判）已在 2026-09-10 修正。

#### S-14 Admin API 封装不足 — 🟡 已收口（SDK 侧补 `activateUser`）

**取证**：
- 全仓 grep `authedRequestWithPath.*_synapse` 仅剩 1 处：`UserService.activateUser()`（`src/services/matrix/admin/UserService.ts:201`）裸调 `PUT /_synapse/admin/v2/users/{uid}` + `{deactivated:false}`。
- SDK `AdminUserManager` 既有 `deactivateUser()` 走 v1 `POST .../deactivate`；既有 `createUser()` 走 v2 `PUT /v2/users/{uid}`。
- 2026-09-10 已在 SDK 补 `activateUser(userId)`（`src/admin/sub-managers/admin-user-manager.ts`），复用 `createUser` 同一条 v2 PUT 路由，发 `{deactivated:false}`，并 emit `UserActivated` 事件。
- Tjg `UserService.activateUser` 保留裸调 + 注释指示后续迁移；vendor tarball 已重打包（2026-09-11），待 `pnpm install` 链接后可直接切到 `admin.activateUser()`。

**结论**：S-14 唯一裸调点已可收口，无"前端应通过 Manager 而非裸 HTTP"的违规。其余 admin 域（Media/Federation/Report 等）均已走 SDK Manager。

### 12.2 P1 — 明确缺陷

| 编号 | 问题 | 状态 | 证据 |
|---|---|---|---|
| **S-1** | 分块上传参数通道错配（SDK 仅发 body 不带 query） | ✅ 已解决 | SDK `src/media/index.ts:508-515` `uploadChunk()` 已补 `queryParams: { upload_id, chunk_index }`；后端 `upload.rs` 从 query 读取。 |
| **S-2** | 好友在线状态不刷新（presence 扇出） | 🔴 后端缺失 | 后端 v2 sync 仅下发用户自身 presence，忽略 `since` 且每次全量（设计选择）。SDK `PresenceManager` 无相应 fan-out 接口；需后端补订阅关系 presence 下发或 SDK 侧降级轮询 `/presence/{userId}/status`。**SDK 侧不可收口**。 |
| **S-3** | refresh token 计时失效（expires_in 秒 vs expires_in_ms 毫秒） | ✅ 已解决 | SDK `src/auth/normalize-expires.ts:28-30` `normalizeExpiresInMs()` 在响应边界做 `*1000` 换算；`src/auth/index.ts:715/731`、`src/account/index.ts:169` 的 login/register/refresh 三处均调用。全库无遗漏转换点。 |
| **S-4** | 媒体超限错误不可识别（裸 413） | ✅ 已解决 | SDK `src/http-api/utils.ts:121-140` 两处裸 413 兜底映射为 `M_TOO_LARGE`；`src/media/index.ts:298-320` 上传前消费 `m.upload.size` 做客户端预检。 |
| **S-5** | 历史消息分页边界丢消息（token 纯时间戳） | ✅ 后端已修复 | 后端 `synapse-services/src/room/messaging/messages.rs:372-407`（ISSUE-06，2026-09-06 `cb8843a4`）已落地：`get_room_messages()` 用 `generate_pagination_token(origin_server_ts, stream_ordering)` 生成**复合** `t{ts}_{stream}` 游标（`end` 字段取页尾事件的 stream_ordering），同毫秒事件不再在页边界丢失；解析侧 `parse_room_messages_from_token`（`room/mod.rs:30-36`）三格式兼容。SDK 正常透传 token，无需改动。 |

### 12.3 P2 — 技术债（后端/Redis 层，SDK 无感知或不可收口）

| 编号 | 问题 | 所属方 | 状态 |
|---|---|---|---|
| **S-6** | 事件驱动替代轮询（v2 sync 250ms DB 轮询） | 后端 | 🔴 未接 EventNotifier |
| **S-7** | EventNotifier Redis 扇出未接线（`with_redis()` 全仓无人调用） | 后端 | 🔴 死代码 |
| **S-8** | presence 去重缓存读写不对称（set_raw 写 Redis，get_raw 读本地） | 后端 | 🔴 跨实例未命中 |
| **S-9** | 限流三件套 + 429 与长轮询互为掩护 | 后端 | 🔴 配置重复 |

> S-6~S-9 均为后端/Redis/部署架构问题，SDK 无法在客户端侧收口，需后端专项优化。

### 12.4 三、SDK 代码质量与设计审查（Q-1~Q-6）回应

| 建议 | 状态 | 说明 |
|---|---|---|
| Q-1 Manager getter 膨胀（120+ get*Manager） | 🟢 已落地 | `client-infra/manager-accessor.ts` 通过模块扩展实现类型安全 `client.manager<K extends ManagerName>(name): ManagerTypeMap[K]`；`manager-extensions/index.ts` 在初始化时**优先**加载该 accessor（行 270-274），再逐个 `registerManagerClass()` 注册。已验证 admin/auth/dm/friend/presence/threading/media/profile/account/serverCapabilities 等核心 Manager 均在各自 `extendMatrixClient()` 内完成注册；`spec/unit/manager-accessor.spec.ts` 覆盖注册/查找/单例/工厂四类场景。getter 群保留为兼容层，非阻断。 |
| Q-2 文档注释缺失 | 🟡 部分补全 | `version.ts`、`normalize-expires.ts` 等新增文件均含 JSDoc；既有 manager 注释仍不均。 |
| Q-3 测试覆盖不均 | 🟡 持续改善 | `real-backend` L2 测试仍为红（ISSUE-01~07 验证方案为"当前应为红"）。 |
| Q-4 HuLaClient 与 MatrixClient 双入口 | ✅ 已定位 | HuLaClient 为极简入口（5 方法），MatrixClient 为完整入口；文档已说明。 |
| Q-5 fork 维护风险（v40.2.0-langkebo.1） | 🟡 已建流程 | `meta/sdk-pin.json` + `verify-sdk-pin.mjs` 锁版本；rebase 上游流程待补文档。 |
| Q-6 SDK 到后端版本契约缺失 | 🟡 部分实现 | `supportsThreads()` 等依赖 `/versions` unstable features；`verify-sdk-pin` 在 CI 锁 tarball hash。 |

### 12.5 结论

- **S-13 / S-14**：已解决 / 已收口。
- **S-1 / S-3 / S-4 / S-5**：SDK 侧或后端已修复（代码实证）。
- **S-2**：后端缺失，SDK 不可收口，需后端专项。
- **S-6~S-9**：后端/Redis 架构债，非 SDK 责任。
- **Q-1~Q-6**：设计审查建议，部分已落地，部分为长期优化项。

**本轮实质改动（2026-09-10/11）**：
1. SDK `package.json` 补 `"./threading"` 子路径导出（消除 Tjg 手工镜像漂移根因）。
2. SDK `threading/index.ts` `SubscribedThreadsResponse` 补 `next_batch?` 字段（对齐后端 keyset 分页）；`getSubscribedThreads({limit?,from?})` 透传分页参数。
3. SDK `AdminUserManager.activateUser()` 封装（收口 S-14 裸调）。
4. Tjg `threadTypes.ts` 完整镜像 SDK `ThreadingManager` 真实签名（`ThreadSummaryResponse` 等 12 个 snake_case 类型）。
5. Tjg `MatrixThreadApi` 升级为后端原始形状透传，`threadUtils.toThreadListItem()` 做 camelCase 转换。
6. vendor tarball 重打包（2026-09-11，commit `8c688b1b6`，`sha256-dba348a264ea00d874e3e6b111500191d01b8f88ba8d90cc2d57cdfee233cff9`），`meta/sdk-pin.json` 已刷新，`pnpm verify:sdk-pin` 通过。
7. Tjg 线程测试修正 6 例 stale mock（改为 SDK snake_case 原生形状，验证 API 层 camelCase 映射）；`MatrixThreadApi/Service/threadUtils` 测试 98/98 通过；`vue-tsc --noEmit` EXIT=0。
8. **S-14 裸调用彻底收口（2026-09-11）**：Tjg `UserService.activateUser()` 由裸 `authedRequestWithPath` 改为 `admin.activateUser(userId)`（SDK `AdminUserManager` 同 v2 PUT 路由），并移除 `authedRequestWithPath` 相关 import；经 `vue-tsc --noEmit` 验证 EXIT=0。至此全仓检索 `_synapse` admin 域已无裸调点。
9. **Q-1 复核确认已落地**：`client.manager(name)` 类型安全访问器（`client-infra/manager-accessor.ts`）配 `ManagerName`/`ManagerTypeMap`（`manager-registry.ts`）；核心 Manager（admin/auth/dm/friend/presence/threading/media/profile/account/serverCapabilities）均在各自 `extendMatrixClient()` 内 `registerManagerClass()`；`spec/unit/manager-accessor.spec.ts` 7/7 通过。


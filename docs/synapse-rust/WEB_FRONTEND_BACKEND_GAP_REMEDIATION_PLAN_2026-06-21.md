# Web 前后端差距修复表

**整理日期**: 2026-06-21  
**范围**: `element.test` 当前 Web 栈、`matrix.test` 当前后端工作区、仓库内浏览器 harness 与联调证据  
**目的**: 将本轮发现的 Web/后端实现缺口整理为可排期的 `P0 / P1 / P2` 修复表，统一问题、证据、影响、建议负责人和建议验证方法

---

## 一、结论摘要

1. 当前 Web 端**不是仓库内维护的 Element 前端源码**，而是外部镜像 `vectorim/element-web:v1.12.20` 加本地配置与 `nginx` 注入层。
2. 当前后端在基础 Matrix Client API 层面可用性较好，`login / versions / capabilities / whoami / devices / pushers / sync / user_directory/search / openid-configuration` 均可返回成功响应。
3. 当前最严重的差距不在“基础登录接口不可用”，而在**Element 登录后的 E2EE / cross-signing / SSSS / dehydrated device bootstrap 闭环**虽已具备 fresh-account 真实可执行回归，但浏览器侧仍存在 widget pageerror、OIDC native flow 探测失败和若干异常日志；同时 `/keys/changes`、经典 `/sync`、`sliding-sync` 三条观察面对 cross-signing / device_lists 更新的一致性已基本收口。
4. 当前还存在一批“协议声明面已开启，但 stock Element 产品面并不完整或不稳定”的问题，典型如 `friends`、`widget`、部分 OIDC native flow。
5. 当前仓库仍缺少 Complement 级互通门禁，导致这类“仓库自测能过，但真实客户端组合不顺滑”的问题发现偏晚。

---

## 二、证据基线

### 2.1 Web 栈来源

- 当前 Web 端通过 Docker overlay 启动，直接使用 `vectorim/element-web:v1.12.20` 镜像，而不是仓库内 TS/React 源码构建产物。[docker-compose.web.yml](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docker/docker-compose.web.yml#L23-L30)
- 仓库中存在的是 `tests/element-web-harness` 浏览器级测试夹具，而不是 Element Web 应用源码。[README.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/element-web-harness/README.md#L1-L21) [package.json](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/element-web-harness/package.json)

### 2.2 本轮联调与探针

- 浏览器级验证：运行仓库自带 `tests/element-web-harness` 的 `smoke:login` 与 `test:basic`
  - `smoke:login` 已可稳定识别中文界面与数字身份模态
  - `test:basic` 已实测走通 `test1 -> test2` 的 `登录 -> 跳过数字身份验证 -> 进入主界面 -> 发起 DM -> 发送加密消息`
  - 当前剩余问题从“主流程阻断”收敛为“运行时异常和兼容性噪音”
- API 级验证：以下接口返回成功
  - `/_matrix/client/v3/login`
  - `/_matrix/client/versions`
  - `/_matrix/client/v3/capabilities`
  - `/_matrix/client/v3/account/whoami`
  - `/_matrix/client/v3/pushers`
  - `/_matrix/client/v3/devices`
  - `/_matrix/client/v3/sync?timeout=1`
  - `/_matrix/client/v3/user_directory/search`
  - `/.well-known/openid-configuration`
- UIA 级验证：`/_matrix/client/v3/keys/device_signing/upload`
  - 第一步空 body 返回 `401 M_UIA_REQUIRED`
  - 第二步带 `session + m.login.password` 返回 `200 {}`
  - 说明最小 UIA 两步并未完全损坏，问题更像完整 E2EE bootstrap 流程与 Element 的兼容性不完整

### 2.3 仓库内现有测试证据

- 浏览器 harness 目前支持 `smoke:login` 和 `test:basic`；本轮已补齐中文界面、数字身份模态、DM 弹窗和消息发送路径，但仍需继续把 pageerror / console error 收敛为正式门禁。[README.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/element-web-harness/README.md#L7-L21)
- 仓库内已新增并稳定化一批 E2EE 观察回归：`/keys/changes`、经典 `/sync`、`sliding-sync` 现在都覆盖 `cross-signing`、`device_lists.left`、`leave/rejoin`、`kick/ban`、`unban/forget`、`knock` 等关键 membership 边界；组合门现可稳定通过 `/sync device_lists` 13 条与 `sliding-sync e2ee` 12 条用例，并已沉淀出可直接复跑的三观察面入口 `bash scripts/test/run_e2ee_observability_gate.sh`。[api_e2ee_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_e2ee_tests.rs) [api_sliding_sync_contract_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_sliding_sync_contract_tests.rs) [run_e2ee_observability_gate.sh](file:///Users/ljf/Desktop/hu_ts/synapse-rust/scripts/test/run_e2ee_observability_gate.sh)
- 先前长期 `#[ignore]` 的部分 E2EE bootstrap 回归已被解开，包括 fresh-account 的 `cross-signing + SSSS + dehydrated device` 主链；剩余问题更多转向浏览器集成噪音与更高层互通门禁，而不再是完全缺少后端可执行回归。[api_e2ee_advanced_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_e2ee_advanced_tests.rs#L211-L215) [api_e2ee_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_e2ee_tests.rs)

---

## 三、P0 修复表

### P0-01 Element 登录后安全引导虽可绕过，但仍存在残余运行时异常与兼容性噪音

| 项 | 内容 |
|---|---|
| **问题** | stock Element 现已可在浏览器 harness 中绕过数字身份模态并完成主流程，但登录后的安全引导仍伴随 `Widget*Store ReferenceError`、OIDC native flow 探测失败、若干 `404` 与 `read receipt` 错误日志，说明兼容性仍未完全收敛。 |
| **证据** | 1. `tests/element-web-harness/basic-interactions.mjs` 已通过 `test1 -> test2` 的 `登录 -> 主界面 -> 发起 DM -> 发送加密消息`；2. 同次运行仍记录 `WidgetLayoutStore / WidgetMessagingStore / WidgetStore` 初始化 `ReferenceError`；3. 控制台仍出现 `Dynamic registration not supported`、若干 `404` 和 `Cannot set read receipt to a pending event`。[basic-interactions.mjs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/element-web-harness/basic-interactions.mjs) [api_e2ee_advanced_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_e2ee_advanced_tests.rs#L211-L321) [api_e2ee_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_e2ee_tests.rs#L1026-L1176) |
| **影响** | 主流程已从“阻断”下降为“可用但带噪音”，说明 P0 已有实质进展，但这些异常仍会污染真实用户体验并掩盖后续更细粒度的协议问题。 |
| **建议负责人** | 后端 E2EE 负责人 + Web 集成负责人 |
| **建议验证方法** | 1. 固化 `smoke:login` 与 `test:basic` 为必跑门禁；2. 对 `cross-signing / security summary / secret storage / dehydrated device` 逐步补浏览器级探针；3. 继续收敛 `Widget*Store` pageerror、OIDC native flow 噪音和 `read receipt` 异常。 |

### P0-02 E2EE bootstrap 相关前置条件未形成稳定可回归闭环

| 项 | 内容 |
|---|---|
| **问题** | `cross-signing + SSSS + dehydrated device` 的后端闭环已较此前显著收口，但当前缺口已从“完全缺少可执行回归”转为“浏览器侧完整 bootstrap 与组合观察门禁仍需继续稳定化”。 |
| **证据** | 1. `device_signing/upload` 的 UIA 两步已可稳定从 `401 M_UIA_REQUIRED` 走到 `200 {}`；2. fresh-account 的 `cross-signing + SSSS + dehydrated device` 主链现已有真实集成回归；3. `/keys/changes`、经典 `/sync`、`sliding-sync` 对 `cross-signing / device_lists` 更新的一致性已补齐 targeted regression，并整组通过 `/sync device_lists` 13 条与 `sliding-sync e2ee` 12 条组合门；4. `put_dehydrated_device` 仍以已有 cross-signing 和 SSSS 为前置条件，这一约束现在已被回归明确锁定。[e2ee_routes.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/e2ee_routes.rs#L576-L644) [assembly.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/assembly.rs#L322-L366) [api_e2ee_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_e2ee_tests.rs) [api_sliding_sync_contract_tests.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_sliding_sync_contract_tests.rs) |
| **影响** | 风险已经从“后端主链不可验证”下降为“真实 Web 客户端仍可能被残余 pageerror / 控制台噪音 / 更高层互通差异干扰”。如果不把组合门与浏览器级断言继续做实，后续仍可能出现“API 看起来正确，但 stock Element 体验不稳定”的回归。 |
| **建议负责人** | 后端 E2EE 负责人 |
| **建议验证方法** | 1. 继续保留并扩展 `security summary / cross-signing upload / SSSS key presence / dehydrated device status` 这条 fresh-account 流水线；2. 以 `bash scripts/test/run_e2ee_observability_gate.sh` 作为 `/keys/changes`、经典 `/sync`、`sliding-sync` 三观察面的固定组合门入口，并进一步纳入 nightly smoke；3. 浏览器 harness 中增加对安全引导完成状态与关键 console/pageerror 的显式断言。 |

---

## 四、P1 修复表

### P1-01 `/versions` 与 `/capabilities` 存在“声明已支持，但 stock Element 并不稳定可用”的超前声明

| 项 | 内容 |
|---|---|
| **问题** | 当前 `/versions` 与 `/capabilities` 会声明 `widget`、`friends`、`burn_after_read`、`voice_extended` 等能力，但对 stock Element 来说，这些能力并不都对应稳定可用的产品体验。 |
| **证据** | 历史上 `versions.rs` 曾在能力面声明 `org.matrix.msc4261.widget`、`io.hula.widget` 等，而 stock Element 会因此走入 widget 初始化链并触发 `Widget*Store` pageerror；本轮已开始收紧这些声明，仅保留真正面向当前客户端基线稳定可消费的能力面。[versions.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/versions.rs) |
| **影响** | 会造成客户端能力探测与真实可用性不一致，诱发“客户端启用了某功能路径，但后续 UI/行为不完整”的兼容性问题。 |
| **建议负责人** | 后端协议兼容负责人 |
| **建议验证方法** | 1. 逐项建立“声明即能用”的验收表；2. 对 stock Element 无法稳定消费的能力先降级或按配置关闭；3. 增加一条 capability parity 检查，把声明面和浏览器 harness 可用性关联起来。 |

### P1-02 `friends` 只做到后端 API 完整，默认 Web 产品面未完成

| 项 | 内容 |
|---|---|
| **问题** | 后端已实现 `friends` 相关 API，但默认 Web 客户端仍是官方 Element，天然没有“添加好友/好友列表/好友请求”产品入口。 |
| **证据** | 1. 后端 `friend_room` 路由完整实现了 `friends / requests / dm / groups` 等端点；2. 当前 Web 端仍直接使用官方 Element 镜像。[friend_room.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/friend_room.rs#L18-L43) [docker-compose.web.yml](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docker/docker-compose.web.yml#L23-L30) |
| **影响** | 从产品视角看，这项能力是“协议已存在、默认 Web 不可见”，容易被误判为后端缺失或前端故障。 |
| **建议负责人** | Web 产品负责人 + 后端扩展负责人 |
| **建议验证方法** | 1. 明确产品策略：维护定制 Web 前端，或接受 stock Element 不提供好友 UI；2. 若继续支持该能力，需给出正式入口而不是仅靠临时注入页；3. 验证“搜索用户 -> 发好友请求 -> 接受 -> 打开 DM”闭环。 |

### P1-03 OIDC 对官方 Element native flow 仍是部分实现

| 项 | 内容 |
|---|---|
| **问题** | OIDC discovery 可返回正常结果，但对官方 Element native flow 来说仍不完整，浏览器实测曾出现 `Dynamic registration not supported`。 |
| **证据** | 本轮 API 探针中 `/.well-known/openid-configuration` 返回 `200`；但先前浏览器控制台已出现 `Failed to get oidc native flow Error: Dynamic registration not supported`。 |
| **影响** | 用户会看到客户端在探测 OIDC 时出现失败提示，SSO 体验与能力声明之间存在落差。 |
| **建议负责人** | 后端 SSO / 身份集成负责人 |
| **建议验证方法** | 1. 明确支持矩阵：仅支持 external OIDC 还是支持 Element native flow；2. 对不支持的能力做显式降级；3. 用官方 Element 的 OIDC 登录路径做一次完整人工回归。 |

### P1-04 Widget 相关链路对 stock Element 仍不稳定

| 项 | 内容 |
|---|---|
| **问题** | 当前 `widget` 能力在后端与 capability 面上被声明，但 stock Element 在本地联调中曾复现 `WidgetLayoutStore -> WidgetUtils -> ActiveWidgetStore -> WidgetStore` 初始化异常。 |
| **证据** | 本会话浏览器控制台曾出现 `ReferenceError: Cannot access 'B' before initialization`，堆栈明确落在 `Widget*Store` 初始化链。后端当前仍会声明 `widget` 能力。[versions.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/versions.rs#L146-L150) [versions.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/handlers/versions.rs#L459-L462) |
| **影响** | 客户端初始化阶段就可能出错，进而污染登录、房间渲染和侧边栏行为，属于对 Web 端体验影响较大的不稳定点。 |
| **建议负责人** | Web 集成负责人 |
| **建议验证方法** | 1. 对 widget 能力做显式 A/B：关闭后确认主流程稳定，再逐步恢复；2. 固定 Element 版本并记录兼容矩阵；3. 浏览器 harness 增加“页面 console 不出现 widget 初始化错误”的断言。 |

---

## 五、P2 修复表

### P2-01 部分端点仍明确未实现，需纳入支持矩阵或补齐

| 项 | 内容 |
|---|---|
| **问题** | 当前仍存在明确返回“未实现”的 admin / federation 端点。 |
| **证据** | `admin server` 的 `backups` 等端点直接返回 `not implemented in this deployment`；联邦 `event_auth` 直接返回未实现。[admin/server.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/admin/server.rs#L88-L98) [keys.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/src/web/routes/federation/keys.rs#L184-L192) |
| **影响** | 如果这些端点被前端、管理台或联邦对端探测到，会形成明确的不兼容面；即便不立即补实现，也需要进入正式支持矩阵。 |
| **建议负责人** | 对应子域负责人（Admin / Federation） |
| **建议验证方法** | 1. 列出全部 `M_UNRECOGNIZED / not implemented` 路由清单；2. 对外文档同步“已支持 / 未支持 / 计划支持”；3. 对已宣称支持的端点补 focused integration test。 |

### P2-02 缺少 Complement 级互通门禁

| 项 | 内容 |
|---|---|
| **问题** | 当前互通测试主要停留在仓库内 integration、自定义 e2e 和浏览器 harness 层，缺少 Complement 级 smoke 门禁。 |
| **证据** | 现有审查文档已明确记录该问题仍延后，当前仓库未见 Complement 相关工作流或测试资产。[COMPREHENSIVE_REVIEW_2026-06-19.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/COMPREHENSIVE_REVIEW_2026-06-19.md#L719-L729) |
| **影响** | “本仓库自测可过，但与标准 Matrix 客户端/服务端组合不顺滑”的问题会持续被延后发现。 |
| **建议负责人** | QA / 兼容性负责人 |
| **建议验证方法** | 1. 建立最小 Complement smoke：`register / login / sync / create room / send event / media / federation discovery / E2EE bootstrap`；2. 将浏览器 harness 与 Complement 分层治理；3. CI 里至少保留一条 nightly 互通流水线。 |

### P2-03 浏览器 harness 覆盖面不足，尚未成为稳定的 Web 兼容基线

| 项 | 内容 |
|---|---|
| **问题** | 当前 harness 虽已能跑通主流程，但仍偏实验性，尚未把 `pageerror / console error` 收敛成稳定门禁。 |
| **证据** | harness 当前脚本较少，README 也明确标注 `test:basic` 为实验性；本轮已跑通主流程，但运行中仍会记录 widget、OIDC 和 receipt 相关异常日志。[README.md](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/element-web-harness/README.md#L7-L21) |
| **影响** | 即使后端持续迭代，也很难快速判断 stock Element 是否被回归破坏。 |
| **建议负责人** | Web 测试负责人 |
| **建议验证方法** | 1. 先把 `smoke:login` 修成稳定绿；2. 扩展到 `create room / send message / key setup complete / no console error`；3. 将截图、HTML snapshot 与 console log 标准化归档。 |

---

## 六、建议排期顺序

### 第一阶段：先恢复真实 Web 可用性

1. 修 `P0-01` 与 `P0-02`
2. 让 `smoke:login` 稳定通过
3. 让“fresh account 登录后进入主界面”成为最小验收门禁

### 第二阶段：收紧声明面与产品面差距

1. 修 `P1-01`
2. 明确 `friends / widget / oidc` 的产品支持策略
3. 把“已声明能力是否真的可用”制度化

### 第三阶段：补互通与回归测试治理

1. 修 `P2-02`
2. 修 `P2-03`
3. 把当前浏览器 harness、API contract、Complement smoke 统一到同一兼容性看板

---

## 七、文档使用建议

- 如果要用于排期会议，建议将 `P0-01`、`P0-02` 直接拆成独立 issue
- 如果要用于对外能力说明，建议同步更新 `SUPPORTED_MATRIX_SURFACE.md`
- 如果要用于后续验证，请把本文件与浏览器 harness 失败截图、console log 和 API 探针结果一起归档

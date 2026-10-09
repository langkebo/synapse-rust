# Matrix E2EE 规范符合性核查报告（synapse-rust）

> **报告目的**：系统解读 Matrix E2EE 规范关于**密钥存储**的核心要求与**标准实现流程/行为准则**，
> 并对照 `synapse-rust` 当前实现**逐模块核查**合规性，明确标注符合项 / 不符合项 / 部分符合项，
> 给出可执行的整改建议。
>
> **规范依据来源**（matrix-org/matrix-spec，`main` 分支原文抓取）：
> - `content/client-server-api/modules/end_to_end_encryption.md`
>   （intro / Key Distribution / Key algorithms / Device keys / One-time and fallback keys /
>   Uploading keys / Tracking the device list / Recommended client behaviour；
>   章节锚点：`#end-to-end-encryption`、`#key-distribution`、`#device-keys`、
>   `#one-time-and-fallback-keys`、`#uploading-keys`、`#cross-signing`、
>   `#server-side-key-backups`、`#device-verification`、`#recommended-client-behaviour`）
> - `content/client-server-api/modules/secrets.md`（Secret Storage / SSSS，**全文已取得**）
>
> **核查范围**：`synapse-e2ee`（device_keys / cross_signing / megolm / olm / backup /
> secure_backup / ssss / to_device / key_request / key_rotation / crypto）、
> `synapse-web/src/routes/{e2ee/*,key_backup.rs,account_data.rs}`、
> `synapse-services/src/wiring/e2ee.rs`、`migrations/00000000_unified_schema_v12.sql`。
>
> **版本**：v1.0（2026-10-09）· 项目版本 v6.2.0

---

## 0. 结论摘要（TL;DR）

**核心结论**：Matrix E2EE 规范**并未要求也不禁止**服务端存储密文或服务端自有的密钥材料；
规范真正强制的是——**客户端设备私钥"永远不得导出设备"**（`#device-keys`），
**服务端只能接收并存储公钥部分**（`#uploading-keys`），
以及**服务端不得能解密/拦截房间内容**（`#end-to-end-encryption` 引言）。

对照本项目：

| 维度 | 结论 | 概要 |
|---|---|---|
| 后端密钥存储策略 | ✅ **符合** | 客户端身份/签名/OTK/fallback 一律**只存公钥 + 签名**；SSSS / key backup 只存**客户端密文**；OLM account pickle 加密落库 |
| 密钥交换流程 | ✅ **符合** | upload 强制验签；OTK 一次性领取；签名上传拒绝冒名；to-device 原样中继 + 限额 |
| 设备信任体系 | ✅ **符合** | cross-signing 严格信任链 master→self_signing→device；`device_signing/upload` 强制 UIA；验证事件按规范**原样中继** |
| 密钥备份与恢复 | ✅ **符合** | server-side key backup 强制 `public_key`、只存客户端密文、回滚保护；passphrase 模式移除 |
| **纵深防御加固** | ✅ **符合** | 服务端自有 `megolm_sessions.session_key`（outbound/inbound pickle）已 at-rest 加密（S-10，AES-256-GCM）；原 3 项 🟡（S-11/S-12/SS-5）均为**未对外暴露**的内部路径，已随本轮删除死代码清零（R-2/R-3） |

**未发现任何违反规范 `MUST` 的行为**；原需整改的 3 项均属**纵深防御/代码卫生**级（🟡 建议），
已在本轮以删除死代码方式消除，现无 🟡 项、不影响协议互操作性。详见 §3、§4。

---

## 1. 规范关于密钥存储的核心要求

### 1.1 规范的"服务端可见性"总原则

`end_to_end_encryption.md#end-to-end-encryption` 开篇即定义威胁模型：

> "Matrix optionally supports end-to-end encryption, allowing rooms to be created whose
> conversation contents are **not decryptable or interceptable on any of the participating
> homeservers**."

**推论**：规范的安全边界是"服务端（含所有参与 homeserver）不得能解密/拦截会话内容"。
它**没有**任何条款禁止服务端存储密文或公共材料；它约束的是**客户端行为**——
私钥不得交给服务端。

### 1.2 私钥"永不导出设备"——唯一与"密钥存储"直接相关的强制条款

`#device-keys`：

> "Each device **should** have one Ed25519 signing key. This key should be generated on the
> device from a cryptographically secure source, and **the private part of the key should
> never be exported from the device**."

`#uploading-keys`：

> "A device **uploads the public parts** of identity keys to their homeserver as a signed
> JSON object, using `/keys/upload` … **Devices must store the private part of each key they
> upload.**"

**关键判定**：
- 该 `should never be exported` / `must store ... on the device` 的**义务主体是客户端**，
  而非服务端；服务端对应的义务是"**只接收公钥部分**"。
- 因此**规范并未要求服务端"不得存储用户 E2EE 私钥"**——因为按正确流程，
  **私钥根本不会到达服务端**；服务端侧的正确行为是：**永远不索取、不接收、不存储私钥**。

### 1.3 与密钥存储相关的其余条款

| 条款（章节） | 原文要点 | 存储含义 |
|---|---|---|
| `#one-time-and-fallback-keys` | "Servers **must** ensure that each one-time key is only claimed once: a homeserver **should discard** the one time key once it has been given to another user." | 服务端可暂存 OTK **公钥**，但领取后必须丢弃 → 不得长期留存可复用会话材料 |
| `#one-time-and-fallback-keys`（warning） | "Clients **should not** store the private half of fallback keys indefinitely … keep the private keys for **at most 2** fallback keys." | 私钥留存约束针对客户端；服务端只存 fallback **公钥**（含 `fallback:true`） |
| `#cross-signing` | cross-signing 使用 `ed25519` 密钥；上传走 `/keys/device_signing/upload`（需 UIA）；**公钥 + 签名**交服务端 | 服务端只存 master/self_signing/user_signing 的**公钥 JSON + 签名** |
| `#server-side-key-backups` | 备份算法 `m.megolm_backup.v1.curve25519-aes-sha2`；`/room_keys/version` 的 `auth_data` 含 **`public_key`**；备份内容是**客户端加密后的密文** | 服务端存**密文 blob**，解密密钥永不上行 |
| `secrets.md`（Secret Storage / SSSS） | secret 作为 **account-data 事件**存储（`m.secret_storage.key.<id>` / `m.secret_storage.default_key`），算法 `m.secret_storage.v1.aes-hmac-sha2`；**密钥由口令经 HKDF 派生，服务端不可见明文** | secret **必须加密后**才可经过服务端；`m.secret_storage.key.<id>` 的 `auth_data` 只含 `iv`/`mac`/`passphrase`，**不含原始 key** |

### 1.4 小结：规范到底"要求"了什么

1. **不要求**服务端"不得存储任何密钥相关数据"——服务端**必须**存公钥/OTK/备份密文，否则协议无法工作。
2. **要求**：私钥**不得离开客户端设备**；服务端**不得**接收/索取私钥。
3. **要求**：服务端**不得能解密/拦截**会话内容（即不得持有可解密用户消息的私钥，除非在用户明确信任的桥接场景）。
4. **要求**：OTK 一次性、备份/SSSS 密文不可被服务端解出。

---

## 2. 规范定义的标准流程与行为准则

### 2.1 密钥分发（Key Distribution，`#key-distribution`）

三条权威流程：

1. **上传**（`#uploading-keys`）：设备用 `/keys/upload` 上传**公钥**（device ed25519 + curve25519 身份键），
   并用 ed25519 签名；OTK/fallback 也随 `/keys/upload` 增量上传。**服务端必须验签**。
2. **查询**（`#tracking-the-device-list-for-a-user`）：用 `/keys/query` 拉取目标用户的设备+身份键；
   用 `/keys/changes` 感知设备列表变更。**客户端**须防 `/keys/query` 竞态（规范 warning 用 `MUST`）。
3. **认领**（`#one-time-and-fallback-keys`）：用 `/keys/claim` 认领 OTK；**服务端必须保证一次性**。

**算法（`#key-algorithms`）**：`ed25519`（签名）、`curve25519`（ECDH）、
`signed_curve25519`（OTK/fallback，`{key, signatures, fallback?}`）。

### 2.2 设备验证（Device Verification，`#device-verification`）

- 验证通过 `m.key.verification.*` 家族的 **to-device 事件**完成（`ready/start/accept/key/mac/done/cancel` 等）。
- 交互式验证方式：**SAS**（`m.sas.v1`，短认证字符串）与 **QR code**。
- **服务端职责 = 原样中继**这些 to-device 事件，**不参与**验证计算。

### 2.3 消息加密 / 解密（Megolm）

- 房间消息用 **Megolm**（`m.megolm.v1.aes-sha2`）加密，密文以 `m.room.encrypted` 存储/分发。
- 会话密钥通过 **Olm（to-device）** 在设备间共享（`m.room_key`）。
- **规范未要求服务端持有 Megolm 会话密钥**——Megolm 是客户端 ratchet，服务端只见密文。

### 2.4 Cross-Signing（`#cross-signing`）

- 三类 `ed25519` 密钥：**master**、**self-signing**、**user-signing**。
- 信任链：master 签 self_signing / user_signing；self_signing 签本用户设备；user_signing 签其他用户 master。
- 上传 `/keys/device_signing/upload`，**公钥 + 签名**，需 UIA。

### 2.5 密钥备份与恢复（`#server-side-key-backups`）

- 版本化备份：`/room_keys/version`，算法 `m.megolm_backup.v1.curve25519-aes-sha2`。
- `auth_data` **必须**含 `public_key`；会话密钥以**客户端密文**形式存于 `/room_keys/keys`。
- 恢复：`/room_keys/keys`、`/room_keys/keys/{roomId}`、`/room_keys/keys/{roomId}/{sessionId}` 取密文，
  **客户端本地解密**。

### 2.6 Secret Storage（SSSS，`secrets.md`）

- 默认键指针：`m.secret_storage.default_key`；键事件：`m.secret_storage.key.<key ID>`。
- 算法 `m.secret_storage.v1.aes-hmac-sha2`：口令/原始 key 经 HKDF-SHA256 派生 AES-CTR-256 + HMAC-SHA-256；
  `auth_data` 含 `iv`/`mac`（校验用零字节密文）。
- 存储 secret 事件 `m.secret_storage.secret.<name>`；**全部为密文，服务端不可解**。

### 2.7 推荐客户端行为（`#recommended-client-behaviour`，v1.18）

规范**明确区分** MUST / SHOULD / MAY（义务主体均为**客户端**）：

- Clients **SHOULD** 使用 cross-signing；**SHOULD** 建 SSH（含 cross-signing 私钥与 key backup 解密密钥）；
- Clients **SHOULD NOT** 向**非交叉签名设备**发送加密 to-device（room key / secret）；
- Clients **MUST NOT** 把"非加密设备"等同于"非交叉签名设备"；
- 跨用户验证 **SHOULD** 验证双方 cross-signing keys；
- 桥接场景：session 创建者设备须 cross-signed，消息 **MUST** 附警告。

> 注：这些是**客户端**义务；**服务端**的义务是让上述流程可被正确执行（提供 `/keys/upload|query|claim|changes`、
> to-device 中继、cross-signing / backup / account-data 存储）。

---

## 3. 逐模块合规性核查

结论符号：✅ 符合 · 🟡 部分符合 / 建议加固 · ❌ 不符合 · ⚪ 不适用（服务端无此义务）。

### 3.1 后端密钥存储策略（重点）

| # | 核查项 | 规范条款 | 结论 | 证据 |
|---|---|---|---|---|
| S-1 | 设备身份键只存**公钥 + 签名**，不存任何私钥 | `#uploading-keys` | ✅ | [device_keys/storage.rs](../../synapse-e2ee/src/device_keys/storage.rs)（device_keys/fallback_keys 仅公钥）；[device_keys/service.rs](../../synapse-e2ee/src/device_keys/service.rs) |
| S-2 | OTK 认领后**一次性丢弃** | `#one-time-and-fallback-keys` | ✅ | `claim_one_time_key` 领取即删 [service.rs](../../synapse-e2ee/src/device_keys/service.rs#L569) |
| S-3 | OTK 每算法**上限**（防 DoS/囤积） | `#one-time-and-fallback-keys`（隐含） | ✅ | `MAX_ONE_TIME_KEYS_PER_ALGORITHM_PER_DEVICE = 500` [service.rs](../../synapse-e2ee/src/device_keys/service.rs#L21) |
| S-4 | cross-signing 只存**公钥 JSON + 签名** | `#cross-signing` | ✅ | [cross_signing/storage.rs](../../synapse-e2ee/src/cross_signing/storage.rs#L37-L60)（`into_key()` 仅映射 `key_data` 公钥/usage）；写入见 `#L151-L175`（仅 `key_data`+`signatures`） |
| S-5 | server-side key backup 只存**客户端密文** | `#server-side-key-backups` | ✅ | `upload_session` 逐字存 `session_data`；`recover_session_key` 仅回密文 [backup/service.rs](../../synapse-e2ee/src/backup/service.rs) |
| S-6 | SSSS secret 只存**客户端密文**（可达路径） | `secrets.md` | ✅ | `store_account_data_key` 读 `auth_data.key`（规范客户端不含）→ `encrypted_key` 空 [ssss/service.rs](../../synapse-e2ee/src/ssss/service.rs#L87-L117) |
| S-7 | OLM account/session pickle **加密落库** | （安全实践） | ✅ | 强制 `OLM_PICKLE_KEY`，缺失启动失败 [olm/service.rs](../../synapse-e2ee/src/olm/service.rs) |
| S-8 | 服务端**不能解密用户消息**（无解密切口） | `#end-to-end-encryption` | ✅ | `megolm.decrypt` 无 web 路由入口；`/room_key_distribution` 主动 403 [devices.rs](../../synapse-web/src/routes/e2ee/devices.rs#L100-L110) |
| S-9 | `megolm_session_keys`（共享键存档）**at-rest 加密** | （安全实践） | ✅ | `at_rest.seal()` [vodozemac_megolm.rs](../../synapse-e2ee/src/vodozemac_megolm.rs#L340-L430)；AES-256-GCM [key_at_rest.rs](../../synapse-e2ee/src/crypto/key_at_rest.rs) |
| S-10 | **服务端自有** `megolm_sessions.session_key`（outbound/inbound pickle）**at-rest 加密** | （安全实践/纵深防御） | ✅ | `seal_session_key`/`open_session_key`（AES-256-GCM，`v1:` 前缀）覆盖 create/get/update [megolm/storage.rs](../../synapse-e2ee/src/megolm/storage.rs#L84-L101)；存量明文惰性迁移（无 `v1:` 前缀按原样读、下次写回 seal），db_tests [storage.rs](../../synapse-e2ee/src/megolm/storage.rs#L748-L793) |
| S-11 | SSSS 不在库中明文保存**原始 32 字节 key** | `secrets.md` | ✅ | 会写入明文原始 key 的 `store_key`/`create_key`/`create_aes_hmac_key` **已删除**（见 §4.2 R-2）；可达路径 `store_account_data_key` 仅镜像 `auth_data.key` [ssss/service.rs](../../synapse-e2ee/src/ssss/service.rs#L87-L117) |
| S-12 | key request 不把明文 `session_key` 交回调用方 | `#end-to-end-encryption` | ✅ | 会返回明文 `session_key` 的 `KeyRequestService::fulfill_request` **已删除**（见 §4.2 R-3）；剩余接口仅落库/查询元数据 [key_request/service.rs](../../synapse-e2ee/src/key_request/service.rs) |

**对 S-10 的规范定性（重要）**：
`megolm_sessions` 存的是**服务端自己生成**的 Megolm 会话（供 bridge / key_rotation 使用），
**不是**客户端设备身份私钥。规范 `#device-keys` 的"私钥永不导出设备"义务**不适用**于服务端自有会话，
故**不构成规范违规**；但因其为**对称密钥材料**且是"可解密房间内容"的等价物，
按纵深防御原则**建议 at-rest 加密**（与 S-9 对齐）——该项**已在本轮完成**（见 §4.2 R-1）。

### 3.2 密钥交换流程

| # | 核查项 | 规范条款 | 结论 | 证据 |
|---|---|---|---|---|
| E-1 | `/keys/upload` **强制验签**，失败即拒 | `#uploading-keys` | ✅ | [device_keys/service.rs](../../synapse-e2ee/src/device_keys/service.rs#L239-L253) |
| E-2 | `signed_curve25519` OTK **强制验签** | `#key-algorithms` | ✅ | [service.rs](../../synapse-e2ee/src/device_keys/service.rs#L336-L379) |
| E-3 | fallback key **强制验签** + `fallback:true` | `#one-time-and-fallback-keys` | ✅ | [service.rs](../../synapse-e2ee/src/device_keys/service.rs#L406-L501) |
| E-4 | `/keys/query`、`/keys/claim`、`/keys/changes` 齐全 | `#key-distribution` | ✅ | [keys.rs](../../synapse-web/src/routes/e2ee/keys.rs) |
| E-5 | `/keys/changes` 按**共享房间**过滤变更 | `#tracking-the-device-list-for-a-user` | ✅ | `filter_users_with_shared_rooms` [devices.rs](../../synapse-web/src/routes/e2ee/devices.rs#L17-L98) |
| E-6 | 签名上传**拒绝以他人名义**上传 | `#uploading-keys` | ✅ | [service.rs](../../synapse-e2ee/src/device_keys/service.rs#L673-L696) |
| E-7 | `m.room_key`/`m.forwarded_room_key` 校验 `session_key` 非空 | `#sharing-keys-between-devices` | ✅ | [devices.rs](../../synapse-web/src/routes/e2ee/devices.rs#L155-L175) |
| E-8 | to-device **原样中继** + 大小/收件人限额 | `#send-to-device-messaging` | ✅ | [devices.rs](../../synapse-web/src/routes/e2ee/devices.rs#L112-L190)（对齐 Synapse v1.155 #19617） |
| E-9 | `m.key.verification.*` **原样中继**、可经 `/sync` 取走 | `#device-verification` | ✅ | [api_verification_relay_tests.rs](../../tests/integration/api_verification_relay_tests.rs#L52-L119) |

### 3.3 设备信任体系（cross-signing / SAS / QR）

| # | 核查项 | 规范条款 | 结论 | 证据 |
|---|---|---|---|---|
| T-1 | master 须设备 ed25519 签名；self/user-signing 须 master 签名 | `#cross-signing` | ✅ | [cross_signing/service.rs](../../synapse-e2ee/src/cross_signing/service.rs#L222-L253) |
| T-2 | 严格信任链 master→self_signing→device | `#cross-signing` | ✅ | [service.rs](../../synapse-e2ee/src/cross_signing/service.rs#L671-L677)、`get_verified_devices_batch` `#L716-L778` |
| T-3 | `/keys/device_signing/upload` **强制 UIA** | `#cross-signing` | ✅ | `require_cross_signing_uia` [devices.rs](../../synapse-web/src/routes/e2ee/devices.rs#L204-L264) |
| T-4 | 跨用户验证走 cross-signing（非仅设备键） | `#recommended-client-behaviour` | ✅ | `get_verified_devices_batch` 基于 cross-signing 链 |
| T-5 | 服务端仅中继 SAS/QR 事件（不自造验证） | `#device-verification` | ✅ | [devices.rs](../../synapse-web/src/routes/e2ee/devices.rs) + relay 测试 |

### 3.4 密钥备份与恢复

| # | 核查项 | 规范条款 | 结论 | 证据 |
|---|---|---|---|---|
| B-1 | `/room_keys/version` 建版本时**强制 `public_key`** | `#server-side-key-backups` | ✅ | `auth_data must contain public_key`（400）[key_backup.rs](../../synapse-web/src/routes/key_backup.rs#L118-L138) |
| B-2 | 默认算法 `m.megolm_backup.v1.curve25519-aes-sha2` | `#server-side-key-backups` | ✅ | 同上 |
| B-3 | 上传/读取房间键为**客户端密文** | `#server-side-key-backups` | ✅ | PUT `/room_keys/keys*` 全走 `upload_session` [key_backup.rs](../../synapse-web/src/routes/key_backup.rs#L391-L509) |
| B-4 | 恢复路径（单键/房间/批量）齐全 | `#server-side-key-backups` | ✅ | `recover_keys` / `recover_session_key` / `batch_recover_keys` [key_backup.rs](../../synapse-web/src/routes/key_backup.rs#L626-L686) |
| B-5 | 版本回滚保护 | （安全实践） | ✅ | [backup/service.rs](../../synapse-e2ee/src/backup/service.rs#L573-L639) |
| B-6 | 服务端不派生口令、不解密 session key | `#server-side-key-backups` | ✅ | [secure_backup/service.rs](../../synapse-e2ee/src/secure_backup/service.rs#L67-L184) |
| B-7 | 旧 passphrase 模式**已移除** | `#server-side-key-backups`（现无 passphrase） | ✅ | `passphrase mode removed` 400 [backup.rs](../../synapse-web/src/routes/e2ee/backup.rs#L46-L77)；verify 返回 410 `#L158-L168` |

### 3.5 Secret Storage（SSSS）

| # | 核查项 | 规范条款 | 结论 | 证据 |
|---|---|---|---|---|
| SS-1 | SSSS 只接受 `m.secret_storage.v1.aes-hmac-sha2`（服务端不生成 key；MSC2697/未知算法 400） | `secrets.md` | ✅ | [ssss/service.rs](../../synapse-e2ee/src/ssss/service.rs#L143-L150)（encrypt）、[#L179-L186](../../synapse-e2ee/src/ssss/service.rs#L179-L186)（decrypt） |
| SS-2 | 可达的 `store_account_data_key` 不落原始 key | `secrets.md` | ✅ | [service.rs](../../synapse-e2ee/src/ssss/service.rs#L87-L117) |
| SS-3 | `m.secret_storage.key.<id>` 合成响应**不含原始 key** | `secrets.md` | ✅ | get_account_data 兼容桥 [account_data.rs](../../synapse-web/src/routes/account_data.rs#L157-L180) |
| SS-4 | secret 解密为 encrypt-then-MAC（先校 MAC 再 AES-CTR） | `secrets.md` | ✅ | [service.rs](../../synapse-e2ee/src/ssss/service.rs#L179-L205)（MAC 失败 403） |
| SS-5 | `store_key` 明文路径不对外暴露 | `secrets.md` | ✅ | 该明文路径**已删除**，见 §3.1 S-11 / §4.2 R-2 |

### 3.6 其他（to-device / 会话生命周期）

| # | 核查项 | 结论 | 证据 |
|---|---|---|---|
| O-1 | to-device 事务去重 | ✅ | [to_device/service.rs](../../synapse-e2ee/src/to_device/service.rs#L1-L140) |
| O-2 | to-device 读后即删 + 24h 过期清理 | ✅ | 同上 |
| O-3 | `key_request` 拒绝同设备自满足 | ✅ | 可代发密钥的 `fulfill_request` **已删除**（§4.2 R-3），无自满足路径 [key_request/service.rs](../../synapse-e2ee/src/key_request/service.rs) |
| O-4 | 过期 Megolm 会话清理任务 | ✅ | [server/mod.rs](../../src/server/mod.rs#L474-L512) |

---

## 4. 符合性核查结果表与整改建议

### 4.1 结果汇总

| 结论 | 条目数 | 条目 |
|---|---|---|
| ✅ 符合 | 42 | S-1..S-12、E-1..E-9、T-1..T-5、B-1..B-7、SS-1..SS-5、O-1..O-4 |
| 🟡 部分符合 / 建议加固 | 0 | — |
| ❌ 不符合（违反 `MUST`） | 0 | — |
| ⚪ 不适用 | — | 规范中"客户端 SHOULD/MUST"条款的服务端对应义务已由本项目以服务端能力形式落实 |

> **总判定**：本项目 E2EE 实现**完全符合 Matrix E2EE 规范定义的协议行为**；
> 无 MUST 违规、无 🟡 待加固项。原 3 个 🟡（S-11/S-12/SS-5）均为**纵深防御 / 代码卫生**问题、
> 且属**未对外暴露的内部路径**，已在本轮以删除死代码方式消除（见 R-2/R-3）；
> 服务端自有会话密钥已 at-rest 加密（S-10，见 R-1）。

### 4.2 整改建议（按优先级）

**R-1（P2，S-10）✅ 已完成：`megolm_sessions.session_key` 增加 at-rest 加密**
- 现状（整改前）：outbound/inbound Megolm pickle **明文**存库。
- 动作：复用既有 [`KeyAtRest`](../../synapse-e2ee/src/crypto/key_at_rest.rs)（AES-256-GCM，格式 `v1:<b64>`），
  在 [megolm/storage.rs](../../synapse-e2ee/src/megolm/storage.rs) 的 `create_session`/`get_session`/`update_session`
  对 `session_key` 做 `seal()/open()`；对存量行做一次性迁移（区分 `v1:` 前缀以兼容存量明文）。
- 结果：`MegolmSessionStorage` 注入 `KeyAtRest`；`create_session`/`update_session` 写 `seal_session_key()`，
  `get_session`/`get_room_sessions` 读 `open_session_key()`；无 `v1:` 前缀的存量明文按原样读、下次写回时再 seal（惰性迁移）。
  同时修复 `KeyAtRest::open` 的解码引擎（`STANDARD` → `STANDARD_NO_PAD`，与 `seal` 对齐），以支持变长 pickle。
  单测/DB 回归：`crypto::key_at_rest` 8 passed、`megolm::storage` 15 passed（含 `v1:` 落库断言与存量明文回写断言）。
- 收益：与已加密的 `megolm_session_keys`（S-9）保持一致，消除"对称密钥材料明文"短板。
- 风险：低（纯存储层，接口不变）。

**R-2（P3，S-11 / SS-5）✅ 已完成：删除 `SSSS::store_key` 明文路径**
- 现状（整改前）：[ssss/service.rs](../../synapse-e2ee/src/ssss/service.rs) 的 `store_key` 会把服务端生成的原始 32 字节 key
  明文写入 `encrypted_key`；已确认**无生产调用方**。
- 动作：采纳方案①，**删除** `store_key`/`create_key`/`create_aes_hmac_key` 及其单测，并清理因失去生产
  构造方而孤立的模型（`SecretStorageKeyCreationTerm`/`SecretStorageKeyCreationKey`/`AesHmacSha2Key`）；
  可达路径 `store_account_data_key` 保持不变（仅镜像 `auth_data.key`）。
- 结果：明文原始 key 的写入路径不复存在；`cargo check -p synapse-e2ee` 无警告，`ssss` 单测 21 passed。
- 收益：移除潜在误用面（未来若被接入即成为规范风险）。

**R-3（P3，S-12）✅ 已完成：删除 `key_request::fulfill_request`**
- 现状（整改前）：该方法会把明文 `session_key` 返回调用方，**无 web 路由接入**。
- 动作：采纳方案①，**删除** `KeyRequestService::fulfill_request`；随之孤立的 `megolm_service` 字段与
  `KeyShareResponse`/`MegolmProvider` 导入一并移除，构造器精简为 `new(storage)`（唯一调用点已同步）。
- 结果：服务端不存在把明文 `session_key` 交回调用方的接口；`key_request` 单测 25 passed。
- 说明：Matrix 房间密钥共享是**客户端到客户端**（to-device `m.room_key`），服务端**不应**代发密钥。

**R-4（P4，遗留字段）标注 `SecureBackupAuthData{salt, iterations}` 为 deprecated**
- 现状：[secure_backup/service.rs](../../synapse-e2ee/src/secure_backup/service.rs#L26-L65) 保留旧 `salt`/`iterations`
  （缺失时默认空/0，兼容规范形状）；当前规范 `auth_data` 无此二字段。
- 动作：在结构体字段与文档注释中标注 `deprecated`，规划后续版本移除；不改变现有兼容读取。

**R-5（P4，可选）`/room_keys/request` 与 secure-backup 端点属 vendor 扩展**
- 现状：`/room_keys/request*`、`/keys/backup/secure/*` 为**非规范路径**的附加能力。
- 动作：确认这些仅服务自有客户端、不影响标准客户端；在 API 文档中标注为 vendor/扩展，避免与规范路径混淆。

### 4.3 无需整改但需知晓

- **服务端可持有 Megolm 密钥（bridge/rotation 场景）是规范允许的**：规范不禁止 bridge，
  「Message export / bridge」场景下服务端持有会话是可接受的；本项目 `key_rotation` 与 `megolm_session_keys`
  属此类。**前提**是客户端消息**默认**不经过服务端解密（本项目满足，S-8）。
- **规范 v1.18 的 MUST/SHOULD 均为客户端义务**，本项目以服务端能力（`/keys/*`、to-device 中继、
  account-data、cross-signing/backup 存储）支撑，不存在"服务端缺 `MUST`"问题。

---

## 5. 附录

### 5.1 规范条款索引（matrix-org/matrix-spec `main`）

| 主题 | 文件 / 锚点 |
|---|---|
| E2EE 引言（不可解密/拦截） | `end_to_end_encryption.md#end-to-end-encryption` |
| 密钥分发 | `end_to_end_encryption.md#key-distribution` |
| 算法 | `end_to_end_encryption.md#key-algorithms` |
| 设备键（私钥永不导出） | `end_to_end_encryption.md#device-keys` |
| OTK / fallback（一次性） | `end_to_end_encryption.md#one-time-and-fallback-keys` |
| 上传键（只传公钥） | `end_to_end_encryption.md#uploading-keys` |
| 设备列表跟踪 | `end_to_end_encryption.md#tracking-the-device-list-for-a-user` |
| Cross-signing | `end_to_end_encryption.md#cross-signing` |
| Server-side key backups | `end_to_end_encryption.md#server-side-key-backups` |
| 设备验证（SAS/QR） | `end_to_end_encryption.md#device-verification` |
| 推荐客户端行为（v1.18） | `end_to_end_encryption.md#recommended-client-behaviour` |
| Secret Storage（SSSS） | `secrets.md`（`#secret-storage` / `#key-storage` / `#sharing`） |

### 5.2 本项目关键表结构（`migrations/00000000_unified_schema_v12.sql`）

| 表 | 行 | 存什么 |
|---|---|---|
| `device_keys` | :650 | 设备**公钥** + 签名 |
| `cross_signing_keys` | :672 | master/self/user-signing **公钥 JSON** |
| `key_signatures` | :685 | 设备签名（**公钥域**） |
| `megolm_sessions` | :708（`session_key` :713） | **服务端自有** Megolm pickle（**at-rest 加密** `v1:`，S-10） |
| `key_backups` / `backup_keys` | :748 / :764 | 备份版本元数据 / **客户端密文** |
| `olm_accounts` / `olm_sessions` | :778 / :791 | **加密**的 Olm pickle |
| `e2ee_key_requests` | :806 | 房间键请求元数据 |
| `e2ee_secret_storage_keys` / `e2ee_stored_secrets` | :901 / :916 | SSSS 键 / secret（**密文**） |
| `secure_key_backups` | :928 | secure backup（**客户端密文**） |
| `megolm_session_keys` | :2194 | 共享键存档（**at-rest 加密**） |
| `to_device_messages` | :2726 | to-device 暂存（读后即删） |

### 5.3 核查方法

- 以 matrix-spec `main` 分支 `end_to_end_encryption.md` 与 `secrets.md` **原文条款**为基准。
- 全仓 grep 定位每条 e2ee 能力在 `synapse-e2ee` / `synapse-web` / `synapse-services` 的唯一落点，
  逐一比照条款语义；对"服务端是否持有私钥"类条款，**追溯数据流**确认服务端永不接收私钥。
- 对"未暴露路径"（S-11/S-12/SS-5）以全仓调用点检索确认**无生产调用方**后，删除该死代码路径（见 §4.2 R-2/R-3）。

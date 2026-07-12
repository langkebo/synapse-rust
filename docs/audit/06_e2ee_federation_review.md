# synapse-rust E2EE / Federation 安全审查（最高优先级）

- 日期：2026-07-10
- 范围：`src/e2ee/`（re-export）→ 真实实现 `synapse-e2ee/src/`（58 文件）；`src/federation/` + `synapse-federation/src/`（19 文件）；canonical JSON `synapse-common/src/canonical_json.rs`；联邦 handler `src/web/routes/federation/`
- 依据：Matrix server-server / E2EE spec、`AGENTS.md`（§113 canonical JSON hot path、§114 X-Matrix 解析、§116 存在性泄露→M_NOT_FOUND）
- 方法：亲读 canonical JSON + 3 个 Explore agent 深挖 E2EE 5 项、联邦认证 4 项、存在性泄露 1 项，所有 material 声明经二次核实（megolm 调用方、缓存 TTL 均已 grep 确认）
- 一句话结论：**没有 P0——没有可直接解密、伪造签名或绕过认证的洞。核心密码学做对了：OTK 领取原子、跨签名链完整校验、canonical JSON 严格合规（含 sytest 向量）、X-Matrix 解析容错且强验签、服务器密钥自签名+valid_until_ts 全校验、旧签名密钥 grace period 正确。真正的债是 4 个 P1（1 个有机密性含义要优先）+1 个 P2：①成员离开加密房不触发 megolm 轮换（函数写好了但零调用方）②密钥备份恢复不校验版本新鲜度③远程服务器密钥缓存固定 3600s TTL 忽略 valid_until_ts（可能用已撤销密钥验签）④多个联邦端点 404-vs-403 泄露私有房间/用户存在性。**

## 总览（按 P0/P1/P2）

| 级别 | 数量 | 类别 |
|------|------|------|
| **P0（安全漏洞）** | 0 | 无 |
| **P1（合规违规）** | 4 类 | megolm 成员离开不轮换、备份版本回滚、远程密钥缓存 TTL、联邦存在性泄露 |
| **P2（性能/正确性）** | 1 类 | to_device 去重 TOCTOU 竞态 |
| ✅ 合规 | 6 项 | OTK 原子领取、跨签名链、canonical JSON、X-Matrix 验签、服务器密钥校验、签名密钥轮换 grace |

---

# 一、E2EE（synapse-e2ee/src）

## 1. 设备密钥存储/查询/分发竞态 —— ✅ 合规

OTK（one-time key）领取**原子**：`device_keys/storage.rs:598-629` 用 `WITH target AS (SELECT ... LIMIT 1) DELETE ... WHERE id IN (SELECT id FROM target) RETURNING`，PostgreSQL READ COMMITTED 下 `DELETE...RETURNING` 锁定行，两个并发领取不可能拿到同一 OTK（否则会破坏 Olm 会话安全）。dehydrated device 回退路径（`dehydrated_device.rs:262-314`）用显式 `SELECT...FOR UPDATE` 在同事务内改 JSONB。**双领防护到位。**

## 2. Megolm session 轮换 —— **P1（含机密性含义，建议优先）**

轮换触发逻辑正确：`key_rotation/service.rs:138-156` 按 `message_index >= megolm_rotation_messages`（默认 100）、`age_days >= olm_rotation_days`（默认 7 天）、绝对 `expires_at` 触发。

**问题**：`notify_member_left_encrypted_room`（`key_rotation/service.rs:221`，写 `key_rotation_pending` 行让下一 tick 触发轮换）**全库零调用方**——已二次 grep 确认只有定义处一行。成员离开加密房间时**不触发** outbound megolm 轮换，旧 session 继续用于新消息。

**安全含义**：离开的成员持有旧 megolm session key，可解密其离开后用同一 session 发的消息（前向保密在成员变更处失效）。缓解：Matrix 中 outbound session 轮换主要是**发送方客户端**职责，服务端这套是辅助/追踪；且 100 条/7 天/重启也会换 session。故定级 P1 而非 P0，但**建议按高优先级处理**——把成员离开事件接到此函数。

## 3. 跨设备签名验证 —— ✅ 合规

**完整链校验，不跳步**。`cross_signing/service.rs:689-795` 的 `verify_device_key`：设备被信任 ⟺ (a) master key 密码学签了 self-signing key（:777-783 `verify_cross_signing_signature`）**且** (b) self-signing key 签了 device key。显式排除 master→device 直签。上传方向（`upload_device_signing_key` :201-296）也校验：上传 master 时验设备 ed25519 签名、上传 self/user-signing 时验 master 签名。`verify_cross_key_signature`（:319）从 `key_json` 提取 key id 而非信任调用方传入。

## 4. 密钥备份恢复流程 —— **P1**

`backup/service.rs:446-497` 的 `recover_keys` 接受任意 `version` 参数，确认备份存在（:452-456）后直接返回 session data，**不校验该 version 是否最新、不在返回前验 `auth_data` 签名**。`batch_recover_keys`(:593)、`recover_room_keys`(:650)、`secure_backup/service.rs:225-295` 同样缺版本新鲜度校验。独立的 `verify_backup`(:520-591) 确实验 auth_data ed25519 签名，但**未作为恢复的前置强制**。

**含义**：拿到旧备份副本（DB dump/陈旧副本）的攻击者可通过恢复 API 让服务端回滚服务旧备份。缓解：session data 是 AES-GCM 加密的，攻击者仍需恢复密钥，故非 P0。修复：恢复前强制 `verify_backup` + 拒绝非最新 version。

## 5. to_device 事件投递顺序 —— **P2**

**顺序本身正确**：`to_device/storage.rs:152` 用 `nextval('to_device_stream_id_seq')` 插入，:179 `ORDER BY stream_id ASC` 取，:322 `DELETE...RETURNING` 原子投递。顺序 + 精确一次（sync 路径）OK。

**问题（P2 竞态）**：去重（`to_device/service.rs:33-68`）用 `to_device_transactions` 表按 `(sender, device, message_id)` `ON CONFLICT DO NOTHING`，但**三步无包裹事务**：`is_duplicate_transaction`(SELECT :34) → `record_transaction`(INSERT :39) → `add_message`(:58-68)。两个同 `message_id` 并发请求可能都通过 :34 的 SELECT，随后都执行 `add_message`，收件设备见**重复消息**。窗口窄但真实。修复：把三步包进一个事务，或先 `record_transaction` 拿到「是否首次」再决定是否 `add_message`。

---

# 二、Federation（synapse-federation/src + web/routes/federation）

## 6. Canonical JSON 序列化 —— ✅ 合规（亲验）

亲读 `synapse-common/src/canonical_json.rs`：键按 Unicode 码点排序、无空白、整数范围 `[-(2^53)+1, 2^53-1]` 强制、**所有浮点（含 1.0）拒绝**、U+2028/2029/FFFD 与控制字符正确转义、`remove_signatures_and_unsigned` 到位。附**大量 spec 向量测试**（sytest `40canonicaljson.pl`、Matrix v1.18 附录），含跨键序确定性、大小整数边界、码点排序。签名 payload `signing.rs:12-28` 构造 `{method,uri,origin,destination,content}` 走同一 canonical。hot path 做对了。

## 7. X-Matrix Authorization 解析 —— ✅ 合规

`src/web/middleware/federation_auth.rs:192-228` `parse_x_matrix_authorization`：
- **容错**：去除值两侧引号(:214-216)、scheme 大小写不敏感(:194)、param 名大小写不敏感(:212)、`.trim()` 空白(:205,212)。
- **严格**：`origin`/`key`/`sig` 任一缺失经 `?`(:227) 返回 None → 401(:43)。
- **强验签**：:87-93 构造 `canonical_federation_request_bytes(method,uri,origin,destination,content)`，:95-103 `verify_federation_signature`(:287-321) 调 `ed25519_dalek::VerifyingKey::verify_strict`(:314)。
- **destination 校验**：:46-59 对本地服务器名四变体校验(`is_local_federation_destination` :147-157)，不匹配 401。

## 8. server_name / verify_keys / valid_until_ts 验证 —— ✅ 合规

`src/web/routes/federation/keys.rs:395-554` `validate_server_key_response`：`server_name` 匹配请求(:397-406)、`valid_until_ts` 必须存在且严格未来(`> now_ms` :409-428)、`verify_keys` 非空且每 key 解 base64 到 32 字节(:430-461)、`old_verify_keys` 结构校验含 `expired_ts`(:463-503)、**响应自签名密码学校验**(:506-551 至少一条 self-sig 过 `verify_ed25519_signature`)。fetch 路径另加 SSRF IP 黑名单、HTTPS-only、信号量限流、失败 30s backoff。

## 9. 远程 key 缓存失效 —— **P1**

`synapse-federation/src/client.rs:402-422` `get_server_keys` 缓存**只用固定 TTL**（`KEY_CACHE_TTL_SECS=3600` :17），:406 `cached.cached_at.elapsed().as_secs() < KEY_CACHE_TTL_SECS`——**已 grep 确认 `valid_until_ts`(:27) 字段存在但不参与缓存过期**。

**含义**：远端发布 `valid_until_ts` 仅 600s 后到期的密钥，此缓存仍服务到 3600s——**超期 3000s，可能用已撤销/轮换的密钥验签**。同仓已有正确实现可参照：`keys.rs:363-365` `let ttl = configured_ttl.min(remaining_secs)`。修复：`get_server_keys` 反序列化后按 `min(3600, (valid_until_ts-now)/1000)` 定 TTL。（另：`FederationSignatureCache` 正确——轮换时 `notify_key_rotation` 失效旧 key 缓存。）

## 10. room/user 存在性泄露 —— **P1**

存在**正确范式**且已用于部分端点：`events.rs:57` `validate_federation_origin_can_observe_room` 对「房间不存在」与「无权观察」**统一返回 404**——`get_state`/`get_state_ids`/`backfill`/`keys_claim`/`keys_query`(静默过滤)/`media_download`(统一 404)/`get_user_devices`(统一 403) 均无泄露。

**问题**：多个 handler 在访问控制**之前**调 `federatable_room_version`(`membership.rs:11-23`，房间不存在即 404)，导致 404(不存在) 与 403(存在但无权) 可区分，泄露私有房间/用户存在性：

| 端点 | 文件:行 | 泄露 |
|------|---------|------|
| `send_join` | `membership.rs:435,464-465` | 404(不存在) vs 403(未授权) |
| `make_leave` / `send_leave` | `membership.rs:399,518` | 无访问控制，404 vs 200 |
| `invite` / `invite_v2` | `membership.rs:569,282` | 先查存在性再检查 |
| `get_event` | `events.rs:115,124,127` | 404(不存在) vs 403(存在但受限) |
| `room_directory_query` / `query_directory` / `get_room_hierarchy` | `events.rs:198,366,460` | 404 vs 403 私有房间 |

（`make_join` 定级轻微：Matrix 中 join 协议本质公开。）修复：把访问控制检查前置到存在性检查之前，或让 `federatable_room_version` 对不存在房间也返回统一码——全部对齐 `validate_federation_origin_can_observe_room` 的统一 404 范式。

## 11. 签名密钥轮换与 old_verify_keys —— ✅ 合规

`synapse-federation/src/key_rotation.rs:572-596` `verify_with_key_rotation` 三级回退：当前密钥 → 历史密钥（仅 `is_within_grace_period` 通过，:587-593）→ DB 查询(:631-663)。`is_within_grace_period`(:598-603) `grace_end = expires_at + grace_period_minutes`（默认 5 分钟 :20），旧密钥签名在 grace 窗内接受、窗外拒绝。`get_server_keys_response`(:680-689) 正确以 `old_verify_keys` + `expired_ts` 暴露历史密钥。

---

## 修复优先级建议

1. **P1 megolm 成员离开轮换**（§2）——把成员离开事件接到 `notify_member_left_encrypted_room`。唯一有直接机密性含义的项，建议当高优先级做。
2. **P1 远程密钥缓存 TTL**（§9）——`client.rs:get_server_keys` 按 `valid_until_ts` 收缩 TTL，一处修复，防用撤销密钥验签。照抄 `keys.rs:365` 现成模式。
3. **P1 联邦存在性泄露**（§10）——统一 404/403 语义，前置访问控制，对齐 `validate_federation_origin_can_observe_room`。改动分散但模式统一。
4. **P1 备份版本回滚**（§4）——恢复前强制 `verify_backup` + 拒绝非最新 version。
5. **P2 to_device 去重竞态**（§5）——三步包一个事务。

## 合规亮点（安全底座扎实）

- ✅ 无 P0：无可直接解密/伪造签名/绕过认证的洞。
- ✅ OTK 原子领取、跨签名链完整校验、canonical JSON 严格合规（含 sytest 向量）。
- ✅ X-Matrix 容错解析 + `verify_strict` 强验签 + destination 校验；服务器密钥自签名 + valid_until_ts + SSRF 防护全到位。
- ✅ 签名密钥轮换 grace period 正确处理 old_verify_keys。

产出：docs/audit/06_e2ee_federation_review.md

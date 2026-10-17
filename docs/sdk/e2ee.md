# 端到端加密（E2EE）

服务端只做**密钥分发与密文搬运**，不持有任何解密材料。本文覆盖密钥上传/查询/认领、
cross-signing、密钥备份、密钥轮换与脱水设备。

## 目录

- [职责边界](#职责边界)
- [上传密钥 `/keys/upload`](#上传密钥-keysupload)
- [查询密钥 `/keys/query`](#查询密钥-keysquery)
- [认领一次性密钥 `/keys/claim`](#认领一次性密钥-keysclaim)
- [设备变更 `/keys/changes`](#设备变更-keyschanges)
- [设备间消息 `sendToDevice`](#设备间消息-sendtodevice)
- [Cross-Signing](#cross-signing)
- [密钥备份 `room_keys`](#密钥备份-room_keys)
- [服务端安全备份 `keys/backup/secure`](#服务端安全备份-keysbackupsecure)
- [密钥变更历史 `/keys/history`](#密钥变更历史-keyshistory)
- [Megolm 密钥轮换（vendor）](#megolm-密钥轮换vendor)
- [脱水设备（MSC3814）](#脱水设备msc3814)
- [与 `/sync` 的对接](#与-sync-的对接)
- [实现要点](#实现要点)

---

## 职责边界

| 事项 | 谁负责 |
|---|---|
| Olm/Megolm 会话建立、加解密、签名校验 | **客户端** |
| 存放 `device_keys`、OTK、cross-signing 公钥 | 服务端 |
| 判定"该设备是否可信" | **客户端**（依据 cross-signing 签名链） |
| 备份密文（`room_keys`） | 服务端（密文，密钥只在客户端） |
| 判定"需要轮换 megolm 会话" | 服务端给**建议**（`/sync` 的 `key_rotation_needed`），轮换动作在客户端 |

---

## 上传密钥 `/keys/upload`

`POST /_matrix/client/v3/keys/upload`（另有 `/keys/upload/{device_id}`、`v1` 变体）

**认证**：是。设备 ID 取 path 参数，缺省用 token 绑定的设备。

**请求体**：

```typescript
interface KeysUploadRequest {
  device_keys?: DeviceKeys;                                // 只需在首登/密钥变化时上传
  one_time_keys?: Record<string, { key: string }>;          // "signed_curve25519:AAAAAQ": {key}
  fallback_keys?: Record<string, { key: string; fallback: true }>;
}

interface DeviceKeys {
  user_id: string;
  device_id: string;
  algorithms: string[];                                    // ["m.olm.v1.curve25519-aes-sha2","m.megolm.v1.aes-sha2"]
  keys: Record<string, string>;                            // {"curve25519:DEV": "...", "ed25519:DEV": "..."}
  signatures: Record<string, Record<string, string>>;      // {user_id: {"ed25519:DEV": sig}}
  unsigned?: { device_display_name?: string };
}
```

**响应**：`{ "one_time_key_counts": { "signed_curve25519": 42 } }`

**本仓具体行为**

| 情况 | 行为 |
|---|---|
| 完全空 body `{}` | **200** + `{"one_time_key_counts": {}}`（规范允许空体，不报 400） |
| 只传 `one_time_keys` | 允许（不会覆盖已有 `device_keys`） |
| 只传 `fallback_keys` | 允许 |
| `device_keys.keys` 为空对象 | 400 `M_BAD_JSON`（`device_keys.keys must be a non-empty object`） |
| 缺 `device_id` 且 token 无绑定设备 | 400 `M_BAD_JSON`（`Device ID required`） |

> `one_time_keys` 的值形状是 `{"key": "<base64>"}`。**没有** `fallback_key` 字段——
> fallback key 通过 `fallback_keys` 单独上传，值形如 `{"key": "...", "fallback": true}`。
> 历史文档把它写成 `{key, fallback_key}` 是错的。

**补传时机**：看 `/sync` 的 `device_one_time_keys_count`，低于阈值（通常 ≤ 一半）就补传到上限。

---

## 查询密钥 `/keys/query`

`POST /_matrix/client/v3/keys/query`

```json
{ "device_keys": { "@bob:matrix.test": [] } }
```

`[]` = 该用户**所有设备**；也可列具体 `device_id`。

**响应**：

```json
{
  "device_keys": {
    "@bob:matrix.test": {
      "BOBDEV": {
        "user_id": "@bob:matrix.test", "device_id": "BOBDEV",
        "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
        "keys": { "curve25519:BOBDEV": "...", "ed25519:BOBDEV": "..." },
        "signatures": { "@bob:matrix.test": { "ed25519:BOBDEV": "..." } },
        "unsigned": { "device_display_name": "Bob 的手机" }
      }
    }
  },
  "master_keys":        { "@bob:matrix.test": { "usage": ["master"], "keys": {...}, "signatures": {...} } },
  "self_signing_keys":  { "@bob:matrix.test": { "usage": ["self_signing"], "keys": {...} } },
  "user_signing_keys":  { "@bob:matrix.test": { "usage": ["user_signing"], "keys": {...} } },
  "failures":           { "remote.example": "Failed to query keys: timeout" },

  "verified_devices":   { "@bob:matrix.test": [ { "device_id": "BOBDEV", "verified_by_master": true,
                                                 "verified_by_self_signing": true,
                                                 "verification_method": "qr", "verified_at": 1760700000000 } ] }
}
```

**三个要点**

1. **`verified_devices` 是本仓扩展**（不在规范里）。它是**服务端记的**验证状态，
   **不能替代客户端自己的签名校验**——客户端仍须用 master key 验 self-signing key、
   再验设备签名。
2. **请求里的用户一定会出现在响应里**（哪怕没有设备也回空对象）。这是刻意的：
   `matrix-rust-sdk` 只在响应里看到某用户时才清除"dirty"标记，静默丢弃会导致
   `/keys/query` 无限重试。
3. **远端用户会经联邦实时拉取**，失败的服务器进 `failures`（键是 server_name，值是错误串）。
   客户端应把 `failures` 当作"暂时拿不到"，稍后重试，**不要**当作"该用户没有加密设备"。

`device_keys` 为空对象时，服务端返回**与我有共享房间的用户 + 自己**（等价于"我关心的所有人"）。

---

## 认领一次性密钥 `/keys/claim`

`POST /_matrix/client/v3/keys/claim`

```json
{ "one_time_keys": { "@bob:matrix.test": { "BOBDEV": "signed_curve25519" } }, "timeout": 10000 }
```

**响应**：

```json
{
  "one_time_keys": {
    "@bob:matrix.test": {
      "BOBDEV": { "signed_curve25519:AAAAAQ": { "key": "...", "signatures": { "@bob:matrix.test": { "ed25519:BOBDEV": "..." } } } }
    }
  },
  "failures": {}
}
```

- 某设备**没有可用 OTK** 时，该设备不会出现在 `one_time_keys` 里（不是报错）。
- 认领即消耗：同一份 OTK 不会被发给第二个请求者。
- `timeout` 是客户端愿意等待的毫秒数（服务端为远端用户拉取时参考）。

---

## 设备变更 `/keys/changes`

`GET /_matrix/client/v3/keys/changes?from=<token>&to=<token>`（另有 `v1`）

**响应**：

```json
{ "changed": ["@bob:matrix.test"], "left": ["@carol:matrix.test"] }
```

- 两者都是 **user_id 字符串数组**，不是 `{user: [devices]}` 映射。
  历史文档写成 `changed: Record<string, DeviceKeyInfo[]>` 是错的。
- `changed`：这些用户的设备清单/签名变了 → 重新 `/keys/query`。
- `left`：这些用户已与我无共享房间 → 可以丢弃其密钥缓存。
- 两个列表都已按"共享房间"过滤，看不到的用户不会出现。

正常运行时**不需要**轮询这个端点：`/sync` 的 `device_lists` 已包含同样的信息。

---

## 设备间消息 `sendToDevice`

| 方法 | 端点 |
|---|---|
| `PUT` | `/sendToDevice/{event_type}/{txn_id}` |
| `POST` | `/sendToDevice/{event_type}/{txn_id}` |
| `PUT`/`POST` | `/client/v1/sendToDevice/{event_type}/{txn_id}` |

```json
{
  "messages": {
    "@bob:matrix.test": {
      "BOBDEV": { "algorithm": "m.olm.v1.curve25519-aes-sha2", "sender_key": "...", "ciphertext": { } }
    },
    "@carol:matrix.test": { "*": { } }
  }
}
```

- 键是 `user_id → device_id → content`；`"*"` 表示"该用户所有设备"。
- 响应 `{}`。投递是**尽力而为**：目标设备离线时消息会在其下次 `/sync` 的 `to_device` 里出现，
  但**不保证**跨设备重放（客户端不应依赖服务端长期暂存）。
- 常用于：Olm 会话建立（`m.room.encrypted` 的 to-device 变体）、`m.room_key`、
  `m.room_key_request`、验证流程（`m.key.verification.*`）。

---

## Cross-Signing

### 上传交叉签名密钥

`POST /_matrix/client/v3/keys/device_signing/upload`

**⚠️ 需要 UIA**。不带 `auth` → **401** + `flows`/`session`。

```json
{
  "master_key":       { "user_id": "@alice:x", "usage": ["master"], "keys": { "ed25519:PUB": "PUB" }, "signatures": { } },
  "self_signing_key": { "user_id": "@alice:x", "usage": ["self_signing"], "keys": {...}, "signatures": {...} },
  "user_signing_key": { "user_id": "@alice:x", "usage": ["user_signing"], "keys": {...}, "signatures": {...} },
  "auth": { "type": "m.login.password", "password": "<当前密码>", "session": "<可选>" }
}
```

**成功响应**：`{}`（空对象）。

- 三者**至少给一个**，否则 400 `M_BAD_JSON`
  （`At least one of master_key, self_signing_key, or user_signing_key is required`）。
- 空的 `{}` 键对象会被跳过（不算提供）。
- 上传成功会唤醒长轮询的 `/sync`，客户端能立刻看到新的 cross-signing 状态。

### 上传签名

`POST /_matrix/client/v3/keys/signatures/upload`

```json
{
  "@bob:x": {
    "BOBDEV": { "user_id": "@bob:x", "device_id": "BOBDEV", "keys": {...}, "signatures": { "@alice:x": { "ed25519:USK": "sig" } } }
  }
}
```

**响应**：`{"failures": {}}`；个别签名无效时按 `user_id → device_id → 错误串` 填入 `failures`。

### 已知交叉签名状态

`POST /_matrix/client/v3/keys/device_list/update`、`POST /keys/device_list_updates`
（查询/更新设备清单版本）也在路由表中；客户端一般不必直接调用，SDK 内部用于 device-list 同步。

---

## 密钥备份 `room_keys`

标准 Matrix 密钥备份（**密文**备份，服务端无法解密）。

### 版本管理

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/room_keys/version` | 建备份 → `{"version": "1760700000"}`（Unix **秒**时间戳字符串）；体 `{"algorithm": "m.megolm_backup.v1.curve25519-aes-sha2", "auth_data": {"public_key": "..."}}` |
| `GET` | `/room_keys/version` | 最新版本（见下） |
| `GET` | `/room_keys/version/{version}` | 指定版本 |
| `PUT` | `/room_keys/version/{version}` | 更新 `auth_data`/`algorithm` |
| `DELETE` | `/room_keys/version/{version}` | 删除该版本及其密钥 |

`GET` 响应：

```json
{ "algorithm": "m.megolm_backup.v1.curve25519-aes-sha2",
  "auth_data": { "public_key": "..." },
  "count": 128,
  "etag": "1",
  "version": "1760700000" }
```

- `version` 是**创建时刻的 Unix 秒时间戳字符串**（不是自增序号），后续所有 `/room_keys/*` 调用都用它。
- `auth_data` 必须存在且含非空 `public_key`，否则 400 `M_INVALID_PARAM`。
- `algorithm` 可省略，默认 `m.megolm_backup.v1.curve25519-aes-sha2`；服务端**不做白名单校验**，原样存储。
- `etag` 用于乐观并发：写入时可带 `?version=` 与 If-Match 语义（服务端按 `etag` 校验）。
- 无任何备份版本时 `GET /room_keys/version` → **404 `M_NOT_FOUND`**（`No current backup version`）。

### 读写密钥

| 方法 | 端点 |
|---|---|
| `PUT` | `/room_keys/keys?version=N`（整批） |
| `PUT` | `/room_keys/keys/{room_id}?version=N` |
| `PUT` | `/room_keys/keys/{room_id}/{session_id}?version=N` |
| `GET` | 对应三个路径 |
| `DELETE` | 对应三个路径 |

写入体（单会话）：

```json
{ "first_message_index": 0, "forwarded_count": 0, "is_verified": true,
  "session_data": { "ciphertext": "...", "ephemeral": "...", "mac": "..." } }
```

写入响应：`{ "count": 128, "etag": "2" }`

> `session_data` 是**客户端加密后的密文**。服务端只按 `room_id/session_id` 存取，
> 不解包、不索引明文。`is_verified` 由客户端声明，服务端只透传。

### 导入 / 导出 / 恢复

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/room_keys/export?version=N` | 导出全部备份密钥（密文） |
| `POST` | `/room_keys/import?version=N` | 导入 |
| `POST` | `/room_keys/recover` | 恢复（指定 rooms/sessions） |
| `POST` | `/room_keys/batch_recover` | 批量恢复 |
| `GET` | `/room_keys/recover/{version}/{room_id}` | 按房间恢复 |
| `GET` | `/room_keys/recovery/{version}/progress` | 恢复进度 |
| `GET` | `/room_keys/verify/{version}` | 校验备份可解密 |
| `GET`/`POST`/`DELETE` | `/room_keys/request[/{request_id}]` | 向其他设备请求缺失密钥 |

所有端点都有 `/client/v1/` 与 `/client/v3/` 两套前缀，语义一致。

---

## 服务端安全备份 `keys/backup/secure`

本仓另有**按 `backup_id` 索引**的安全备份（与 `room_keys` 并存，不是替代）：

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/keys/backup/secure` | 建备份 → `{backup_id, version, algorithm, auth_data, key_count}`；需 `algorithm` + `auth_data` |
| `GET` | `/keys/backup/secure` | 列出全部（响应是 `backup_id → 对象` 的**映射**，不是数组） |
| `GET` | `/keys/backup/secure/{backup_id}` | 单个；不存在 404 |
| `DELETE` | `/keys/backup/secure/{backup_id}` | 删除 |
| `POST` | `/keys/backup/secure/{backup_id}/keys` | 存密钥 → `{count, key_count}` |
| `POST` | `/keys/backup/secure/{backup_id}/restore` | 恢复 → `{total_keys, sessions}`（**返回密文**，客户端自行解密） |
| `POST`/`GET` | `/keys/backup/secure/{backup_id}/verify` | 校验口令/密钥 |

> 两套备份的差别：`room_keys` 是 Matrix 标准面（其他客户端/SDK 认它）；
> `keys/backup/secure` 是本仓自有面（`backup_id` 语义、列表是映射）。
> **新客户端应优先用 `room_keys`**，否则用户换到 Element 等客户端会看不到备份。

---

## 密钥变更历史 `/keys/history`

`GET /_matrix/client/v3/keys/history?limit=100&from=<cursor>`

**响应**：

```json
{ "history": [ { "id": 1, "created_ts": 1760700000000, "...": "..." } ], "next_batch": "eyJ0cyI6..." }
```

- `limit` 默认 100，夹在 `1..=1000`；`next_batch` 仅在返回条数刚好等于 `limit` 时给出。
- ⚠️ 这是**服务端的密钥变更审计视图**，形状与规范里"某设备的密钥历史"不同
  （规范返回按 `device_id` 分组的 `device_keys`）。需要设备级历史请看 `/keys/query`。

---

## Megolm 密钥轮换（vendor）

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/_matrix/vendor/v1/keys/rotation/status` | 当前轮换策略与状态 |
| `POST` | `/_matrix/vendor/v1/keys/rotation/status` | 同上（POST 形态） |
| `GET` | `/_matrix/vendor/v1/keys/rotation/check` | 检查哪些房间需要轮换 |
| `POST` | `/_matrix/vendor/v1/keys/rotation/rotate` | 触发轮换 |
| `POST` | `/_matrix/vendor/v1/keys/rotation/revoke` | 吊销会话 |
| `GET` | `/_matrix/vendor/v1/keys/rotation/history/{device_id}` | 设备轮换历史 |
| `GET`/`PUT`/`POST` | `/_matrix/vendor/v1/keys/rotation/config` | 轮换配置 |

服务端侧的自动轮换由 `server.megolm_device_id` 开关控制（未配置则不启用轮换服务，
行为与之前完全一致）。`/sync` 的 `key_rotation_needed.rooms` 给出**需要客户端轮换**的房间列表。

> 轮换判定只看每个 `(room_id, sender_key)` 的**最新**记录（`DISTINCT ON`）——服务端不会因为
> 历史行"超期"而反复要求轮换。

---

## 脱水设备（MSC3814）

| 方法 | 端点 | 说明 |
|---|---|---|
| `PUT` | `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` | 上传脱水设备的密钥（体含 `device_keys`、`device_data`） |
| `GET` | `/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` | 取当前脱水设备 |
| `DELETE` | 同上 | 删除 |
| `GET` | `.../dehydrated_device/status` | 状态 |
| `GET` | `.../dehydrated_device/{device_id}/events?next_batch=` | 拉取该设备的事件（`next_batch` 为 `null` 表示没有更多） |
| `POST` | `.../dehydrated_device/{device_id}/events` | 上报已消费的事件 |

能力位：`/versions` 的 `org.matrix.msc3814`。

---

## 与 `/sync` 的对接

E2EE 客户端必须读这几个 `/sync` 字段（详见 [sync.md](./sync.md)）：

| 字段 | 用途 |
|---|---|
| `to_device.events` | 处理 `m.room.encrypted`（to-device）、`m.room_key`、验证事件 |
| `device_lists.changed` / `.left` | 触发 `/keys/query` / 丢弃缓存 |
| `device_one_time_keys_count` | 判断是否需要补传 OTK |
| `device_unused_fallback_key_types` | 判断是否需要补 fallback key |
| `key_rotation_needed.rooms` | 需要轮换 megolm 的会话 |
| `device_list_changes` | `device_lists` 的带计数投影：`{users:{uid:{change_type,device_count?}}, changed_count, left_count}`；与 `device_lists` 并列下发，不是同义副本 |

---

## 实现要点

1. **先验签再用密钥**：拿到 `device_keys` 后必须用该用户的 self-signing key（再由 master key 背书）
   验证设备签名；服务端的 `verified_devices` 只是辅助信息。
2. **OTK 低位就补**：不要等 0——两个客户端同时认领会瞬间耗尽。
3. **`failures` 不是"没有设备"**：远端服务器超时必须重试。
4. **cross-signing 上传要 UIA**：忘记带 `auth` 会拿 401，且 `flows` 里通常只有 password。
5. **备份优先 `room_keys`**：`keys/backup/secure` 是私有面。
6. **不要轮询 `/keys/changes`**：`/sync` 已经带了。
7. **`sendToDevice` 不保证送达**：重要密钥要配合 `m.room_key_request` 重试机制。

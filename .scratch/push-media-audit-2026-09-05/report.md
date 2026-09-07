# Synapse-Rust 安全审计报告：消息推送与媒体文件处理

**审计对象**：`/Users/ljf/Desktop/hu_ts/synapse-rust`（Rust / axum / tokio / PostgreSQL / Redis / `image` crate）
**审计范围**：① 消息推送逻辑（事件发布 → 推送路由 → 客户端通知）② 媒体上传（端点 → 存储路径 → 内容校验）③ 媒体下载（端点 → 鉴权 → 路径遍历防护）④ 缩略图生成（尺寸限制 / 内容校验 / 存储路径）
**审计日期**：2026-09-05
**审计结论概览**：推送链路鉴权严格、无跨用户伪造/劫持风险；媒体上传/下载/缩略图的路径穿越与内容注入防护整体扎实。发现 **1 个中等风险（缩略图尺寸上限过宽，潜在资源耗尽）** 与 **1 个低风险（HTTP pusher gateway 接受任意 URL，但当前无调用点、不可达）**。

> 风险等级说明：🔴 严重（可直接被利用、影响机密性/完整性/可用性）｜🟡 中（需特定条件或造成 DoS/资源滥用）｜🟢 低（纵深防御缺口，当前不可达或影响有限）

---

## 一、消息推送逻辑审计

### 1.1 推送端点与路由装配

| 文件 | 行号 | 说明 |
|---|---|---|
| `src/web/routes/push.rs` | 全文 | Matrix 兼容推送路由：`/pushers`（GET/POST）、`/pushrules/...`、`/notifications`、`/notifications/{id}/ack`，nest 在 `/_matrix/client/v3` 与 `/_matrix/client/r0` |
| `src/web/routes/push_notification.rs` | 全文 | 自定义推送路由：`/_matrix/client/r0/push/devices`、`/push/send`、`/push/rules`；admin 路由 `/_synapse/admin/v1/push/process`、`/cleanup` |
| `synapse-services/src/push/service.rs` | 全文 | `PushNotificationService`：FCM / APNS / WebPush / upstream 投递；`evaluate_push_rules`（L483）、`register_device`（L149）|
| `synapse-services/src/push/gateway.rs` | 92-131 | `PushGateway::send_notification(gateway_url, notification)` HTTP POST 投递 |
| `synapse-services/src/event_notifier.rs` | 全文 | `EventNotifier`：room/user `DashMap<Notify>` 槽位 + Redis Pub/Sub 跨实例扇出（仅 sync 唤醒，与推送投递无关）|
| `synapse-services/src/notifying_event_writer.rs` | 全文 | `NotifyingEventWriter`：包装 `EventWriter`，`create_event` 后 `notify_room` / `notify_user` |
| `synapse-services/src/client_push_service.rs` | 全文 | `upsert_pusher`/`delete_pusher`/`upsert_push_rule`/`get_notifications`/`ack_notification` 全部以 `auth_user.user_id` 为条件 |
| `synapse-storage/src/push/mod.rs` | 全文 | `pushers` 表 upsert：`ON CONFLICT (user_id, device_id, pushkey)`；查询 `WHERE user_id = $1 AND device_id IS NOT DISTINCT FROM $2` |
| `synapse-storage/src/push_notification.rs` | 全文 | `push_device` 表 `(user_id, device_id)` 唯一约束；`push_rules` 查询 `WHERE (user_id = $1 OR user_id = '.default')` |

### 1.2 推送鉴权链路分析

**事件 → 推送计算路径**：`NotifyingEventWriter.create_event` 写入事件后，`notify_room`/`notify_user` 仅负责唤醒 sync 长轮询（`EventNotifier`），与“哪些用户收到哪些推送”无关；真正的推送计算发生在 `PushNotificationService.evaluate_push_rules`（L483），逐用户匹配其 `push_rules`，并应用 `m.ignored_user_list` 过滤（`service.rs` 中含该逻辑及解压炸弹注释）。

**push 路由订阅鉴权**：
- `set_pusher`（`push.rs` L143-217）：校验必填字段（pushkey/app_id/kind 等），并要求 `auth_user.device_id` 存在（P-053，防无设备用户设置 pusher）；删除 pusher 同样限定为当前设备。
- 所有 pushrules 操作均传入 `auth_user.user_id`，**不存在跨用户修改接口**（`client_push_service.rs` 全文）。
- 自定义路由 `send_notification`（`push_notification.rs` L162-184）：`user_id = auth_user.user_id.clone()`（L168），只能给自己发，**防伪造他人通知**。`register_device` / `create_rule` 均绑定 `auth_user.user_id`。

**设备 token 绑定**：`pushers` 表以 `(user_id, device_id, pushkey)` 唯一约束（`push/mod.rs`），查询按 `user_id` + 当前 `device_id` 过滤 → 设备 token 强绑定 user_id，**无推送劫持风险**（无法用他人设备 token 替自己注册 pusher）。

### 1.3 推送伪造 / 劫持风险评估

| 威胁 | 结论 | 依据 |
|---|---|---|
| 修改 `push_rules` 让他人收到伪造通知 | ✅ 不可行 | 所有 pushrules 写入均绑定 `auth_user.user_id`（`client_push_service.rs`）|
| 推送劫持（设备 token 被他人绑定） | ✅ 不可行 | `pushers` 表复合唯一约束 + device_id 绑定（`push/mod.rs`）|
| HTTP pusher gateway SSRF | 🟢 低风险（不可达） | 见 1.4 |

### 1.4 发现：HTTP Pusher Gateway 接受任意 URL（🟢 低）

- **位置**：`synapse-services/src/push/gateway.rs` L92-131，`send_notification` 直接用 `self.client.post(gateway_url)` 向调用方提供的任意 URL 发 HTTP POST。
- **攻击场景**：若未来启用 HTTP pusher（用户可在 `set_pusher` 的 `data.url` 中提供 gateway 地址），攻击者可构造 `http://169.254.169.254/...`（云元数据）或内网地址，使服务器发起服务端请求伪造（SSRF），探测内网或读取实例元数据。
- **可达性**：经全代码库 grep 确认，**当前无任何调用点**读取 `pushers.data.url` 并真正发起请求（worker `WorkerType::Pusher` 仅定义、未实现外发）。因此该风险**当前不可达**，属纵深防御缺口。
- **修复建议（具体到代码行）**：在 `gateway.rs` L92 `send_notification` 入口处增加 URL 校验：
  ```rust
  // 建议新增（gateway.rs L92 之前）
  let parsed = url::Url::parse(gateway_url)
      .map_err(|_| ApiError::bad_request("invalid gateway url"))?;
  if parsed.scheme() != "https" {
      return Err(ApiError::bad_request("gateway url must be https"));
  }
  if let Some(host) = parsed.host_str() {
      if host == "localhost" || host.ends_with(".internal") || host.parse::<std::net::IpAddr>().is_ok() {
          return Err(ApiError::forbidden("gateway host not allowed"));
      }
  }
  ```
  同时在 `set_pusher`（`push.rs` L143）校验 `data.url`（若 kind 为 `http`，强制 https 且非内网地址）。

### 1.5 推送模块“已做得好的地方”（点赞）

- ✅ 所有 pushrules / pushers / notification 操作均强制绑定 `auth_user.user_id`，无跨用户越权接口。
- ✅ `send_notification` 以 `auth_user.user_id` 覆盖请求体中的 user_id（L168），杜绝“代发他人通知”伪造。
- ✅ 设备 token 以 `(user_id, device_id, pushkey)` 复合唯一约束强绑定，无劫持面。
- ✅ `set_pusher` 强制要求 `auth_user.device_id` 存在（P-053），避免无设备身份写入 pusher。
- ✅ `evaluate_push_rules` 实现 `m.ignored_user_list` 过滤与 `event_match`/`event_property_contains`（`m.mentions.user_ids` 数组校验）/`event_property_is`，匹配逻辑严谨。
- ✅ `register_device`（L149）对 `push_type` 白名单校验（fcm|apns|webpush|upstream）。

---

## 二、媒体上传审计

### 2.1 上传端点与存储路径

| 文件 | 行号 | 说明 |
|---|---|---|
| `src/web/routes/media/mod.rs` | 60-62 | `media_upload_body_limit`：单一权威来源 `config.server.max_upload_size`，不再硬编码（G-1 修复）|
| `src/web/routes/media/mod.rs` | 64-65 | 上传路由 `DefaultBodyLimit::max(upload_limit)` 覆盖 Axum 默认 2MB |
| `src/web/routes/media/upload.rs` | 25- | `parse_upload_filename`：从 query `filename` 或 `Content-Disposition` 提取 |
| `src/web/routes/media/upload.rs` | 152-167 | `chunked_upload_start`：校验 `total_size <= max_upload_size`（ISSUE-04 早期拒绝）|
| `src/web/routes/media/upload.rs` | 274-288 | `chunked_upload_progress`：校验 `progress.user_id != auth_user.user_id` → 403（防 IDOR）|
| `synapse-services/src/media_service.rs` | 156 | `upload_media`：`let media_id = random_string(32);` |
| `synapse-services/src/media_service.rs` | 65-74 | `validate_media_id`：仅允许 `[A-Za-z0-9_-+=]`，长度 1..=255，拒绝 `/ \ .. NUL 控制字符` |
| `synapse-services/src/media_service.rs` | 172-323 | `store_media_with_id`：filename sanitize（过滤控制字符/`/ \ \0`，取 200 字符）+ `media_path.join(file_name)` |
| `synapse-services/src/media_service.rs` | 614- | `get_extension_from_content_type`：仅 jpeg/png/gif/pdf/txt，默认 bin（由 content_type 派生扩展名）|
| `synapse-storage/src/media/...` | — | `local_media_repository` 元数据存储 |
| `synapse-common/src/crypto.rs` | 239-249 | `random_string`：`rand::rng()`（CSPRNG）+ 62 字符集 |

### 2.2 上传内容校验与路径构造分析

- **大小限制**：统一从 `config.server.max_upload_size` 派生（G-1，修复了此前的硬编码 50MB 不一致）；分块上传 `chunked_upload_start`（L167）早期拒绝超额声明。✅
- **MIME 类型校验**：扩展名**由 content_type 派生**（`get_extension_from_content_type`），而非信任 filename 后缀 → 即便上传 `.php` 也被存为 `.bin`，无代码执行风险。✅
- **filename → 存储路径构造**（路径注入防护核心）：
  - 提取的 filename 经过 `store_media_with_id` L182-186 字符过滤（剔除控制字符、`\0`、`/`、`\`），且文件名前缀强制为 `{media_id}_`，`media_id` 本身经 `validate_media_id` 白名单（L65-74）。
  - 落盘路径为 `self.media_path.join(&file_name)`（L195），`Path::join` 不会因 `..` 跳出目录（且 `validate_media_id` 已拒绝 `.`/`..`）。✅ **路径穿越不可行**。
- **media_id 可枚举性**：`upload_media` L156 用 `random_string(32)`（CSPRNG，62 字符集）生成 → 32 字符、约 62³² 空间，**不可预测、不可枚举**。✅
- **分块上传归属校验**：`chunked_upload_progress` L286 校验 `progress.user_id == auth_user.user_id`，防 IDOR 越权查他人上传进度。✅
- **删除鉴权**：`quota.rs` L71-78 `delete_media` 需 `AuthenticatedUser`，且 `delete_media_for_user` 校验 `uploader == user_id`。✅

### 2.3 上传模块“已做得好的地方”（点赞）

- ✅ 上传 body limit 统一由配置派生（G-1），消除大小限制不一致隐患。
- ✅ 扩展名由 content_type 派生（非 filename 后缀），杜绝可执行文件落盘。
- ✅ filename 字符过滤 + media_id 白名单 + `Path::join` 三重防护，路径穿越不可行。
- ✅ media_id 使用 CSPRNG 32 字符，不可枚举。
- ✅ 分块上传全程鉴权 + 归属校验，防 IDOR。

---

## 三、媒体下载审计

### 3.1 下载端点与鉴权

| 文件 | 行号 | 说明 |
|---|---|---|
| `src/web/routes/media/mod.rs` | — | 媒体路由装配：v1/v3/r0/r1 及 `/_matrix/client/v1/media`；**r1 旧版 download 加 `auth_middleware`**（VULN-01/02 修复注释）|
| `src/web/routes/media/download.rs` | 22-24 | `MEDIA_CONTENT_SECURITY_POLICY`：`sandbox; default-src 'none'; script-src 'none'; ...` |
| `src/web/routes/media/download.rs` | 26-38 | `SAFE_INLINE_MEDIA_TYPES`：安全类型 inline，否则 attachment；`X-Content-Type-Options: nosniff` |
| `src/web/routes/media/download.rs` | 260-264 | `thumbnail_request_params`：`width/height` `filter(|&w| w <= 10000)`，默认 800×600 |
| `src/web/routes/media/download.rs` | — | `download_media`/`get_thumbnail` 使用 `OptionalAuthenticatedUser`（公开下载，符合 Matrix spec）|
| `src/web/routes/media/download.rs` | — | `download_media_signed`：HMAC 签名校验（`verify_media_download_url`）|
| `synapse-services/src/media_service.rs` | 65-74 | `validate_media_id`：路径遍历防护核心（同 2.2）|
| `synapse-services/src/media/mod.rs` | 391-444 | `download_media_stream`：content_type 优先用 DB 存储值，否则读 1KB 前缀魔法字节嗅探；流式 `tokio::fs::File` + `ReaderStream` |

### 3.2 下载鉴权与路径遍历分析

- **公开下载**：`download_media`/`get_thumbnail` 使用 `OptionalAuthenticatedUser`（允许匿名），符合 Matrix 协议 spec（媒体 URL 可通过 mxc 公开访问）。鉴权粒度由 server 配置决定，属预期行为。✅
- **路径遍历防护**：download 路径的 `media_id` 同样经 `validate_media_id`（L65-74，`media_service.rs` 集中校验，路由层旧重复校验已删除为死代码清理），拒绝 `/ \ .. NUL 控制字符` → **无法构造 `../` 跳出媒体目录**。✅
- **响应安全头**（防存储型 XSS 关键缓解）：
  - `X-Content-Type-Options: nosniff`（`SAFE_INLINE_MEDIA_TYPES` 段落）；
  - 非安全类型强制 `Content-Disposition: attachment`；
  - `Content-Security-Policy: sandbox; default-src 'none'; script-src 'none'; ...`（`MEDIA_CONTENT_SECURITY_POLICY`）。✅ 即使返回 HTML/SVG 也不会在浏览器执行脚本。
- **签名下载**：`download_media_signed` 经 HMAC 校验（`verify_media_download_url`），防签名 URL 篡改/重放滥用。✅
- **静态文件服务**：媒体通过 `tokio::fs::File` + `ReaderStream` 流式返回，**未使用 `ServeDir`/静态根目录暴露**，避免整个目录被遍历。✅

### 3.3 下载模块“已做得好的地方”（点赞）

- ✅ media_id 集中白名单校验，路径遍历不可行。
- ✅ `nosniff` + `sandbox` CSP + `attachment` 三重响应头，有效缓解存储型 XSS。
- ✅ 签名下载 HMAC 校验防 URL 滥用。
- ✅ 流式下载，未暴露静态文件根目录。
- ✅ r1 旧版 download 已补 `auth_middleware`（历史 VULN-01/02 修复）。

---

## 四、缩略图生成审计

### 4.1 缩略图端点与处理逻辑

| 文件 | 行号 | 说明 |
|---|---|---|
| `src/web/routes/media/download.rs` | 260-264 | `thumbnail_request_params`：width/height ≤ 10000，默认 800×600 |
| `synapse-services/src/media_service.rs` | 387-438 | `get_thumbnail`：缓存文件名 `{media_id}_{width}x{height}_{method}.jpg`，`validate_media_id` 校验；命中缓存直接返回 |
| `synapse-services/src/media_service.rs` | 440-480+ | `generate_thumbnail`：用 `image::Limits` 设 `MAX_IMAGE_DIMENSION = 8192` 防解压炸弹；输出 JPEG |
| `synapse-services/src/media_service.rs` | 614- | `get_extension_from_content_type`：缩略图后缀固化为 `.jpg` |

### 4.2 缩略图风险分析

**发现：缩略图尺寸上限过宽（🟡 中）**
- **位置**：`download.rs` L261-262，`thumbnail_request_params` 中 `width`/`height` 仅限制 `<= 10000`，默认值 800×600。
- **攻击场景**：攻击者请求 `?width=10000&height=10000&method=crop`，服务器将对原始图片解码后缩放/裁剪到 10000×10000（约 1 亿像素、300MB+ 位图），该重 CPU/内存操作虽已 `spawn_blocking`（`media_service.rs` L416）避免阻塞 tokio worker，但在高并发下仍可被用于资源耗尽型 DoS（CPU/内存/磁盘缓存膨胀，缓存文件名以尺寸为 key 可填满磁盘）。
- **修复建议（具体到代码行）**：
  1. 在 `download.rs` L261-262 收紧上限（建议与配置或合理业务上限对齐，如 1024 或 2048）：
     ```rust
     // download.rs L261-262 修改
     let width = params.get("width").and_then(|v| v.as_u64()).filter(|&w| w <= 2048).unwrap_or(800) as u32;
     let height = params.get("height").and_then(|v| v.as_u64()).filter(|&h| h <= 2048).unwrap_or(600) as u32;
     ```
  2. 在 `media_service.rs` `generate_thumbnail` L452 之后追加输出像素上限保护（即使请求尺寸被放大，也限制解码后实际分配）：可复用 `Limits` 或对 `target_width*target_height` 设上限断言。
  3. 鉴于缓存 key 含尺寸（`media_service.rs` L397），建议在 `get_thumbnail` 入口（L395 之后）对 `(width, height)` 组合做白名单/配额，避免无限缓存组合耗尽磁盘。

**解压炸弹防护（已做好）**：`generate_thumbnail` L452-460 用 `image::Limits` 限制单边最大 8192 像素，超压缩比图片在分配完整位图前被拒绝，避免内存耗尽。✅

**内容校验（已做好）**：缩略图输入为经下载校验的原始媒体；解码失败返回 `bad_request`，不会回显异常内容。✅

**存储路径（已做好）**：缓存文件名 `{media_id}_{width}x{height}_{method}.jpg`（L397），`media_id` 经 `validate_media_id`，`method` 经 `ThumbnailMethod::from_str` 白名单（`get_thumbnail` L396），**无路径注入**。✅

### 4.3 缩略图模块“已做得好的地方”（点赞）

- ✅ `image::Limits` 解压炸弹防护（MAX_IMAGE_DIMENSION=8192）。
- ✅ 重 CPU 解码操作 `spawn_blocking` 隔离，不阻塞异步运行时。
- ✅ 缓存文件名 media_id 经白名单、method 经枚举解析，无路径注入。
- ✅ 缩略图输出固化为 JPEG（由 `get_thumbnail` 文件名 + `generate_thumbnail` 输出），无格式混淆风险。

---

## 五、风险汇总表

| 编号 | 模块 | 位置 | 风险 | 等级 | 可达性 |
|---|---|---|---|---|---|
| PUSH-01 | 推送 | `push/gateway.rs` L92-131 | HTTP pusher gateway 接受任意 URL（潜在 SSRF） | 🟢 低 | 不可达（无调用点）|
| MEDIA-01 | 缩略图 | `download.rs` L261-262 | 缩略图 width/height ≤ 10000，高尺寸放大导致资源耗尽 DoS | 🟡 中 | 可达 |
| — | 推送 | 全链路 | 跨用户 pushrules / 推送伪造 / 设备劫持 | — | 无风险（已确认安全）|
| — | 媒体上传 | `media_service.rs` L65-74, L172-323 | 路径穿越 / 可执行文件落盘 / media_id 枚举 | — | 无风险（已确认安全）|
| — | 媒体下载 | `download.rs` L22-38 | 路径遍历 / 存储型 XSS | — | 无风险（已确认安全）|

---

## 六、总体结论与建议优先级

1. **🟡 优先修复 MEDIA-01（缩略图尺寸上限）**：收紧 `download.rs` L261-262 的尺寸上限（建议 ≤ 2048），并为 `get_thumbnail` 缓存组合增加配额，防止资源耗尽 DoS 与磁盘膨胀。
2. **🟢 纵深加固 PUSH-01**：在 `push/gateway.rs` L92 与 `push.rs` L143 增加 gateway URL 的 https + 非内网校验，为未来启用 HTTP pusher 预留 SSRF 防护（当前虽不可达，但属低成本防御）。
3. **保持现有安全控制**：推送 user_id 绑定、media_id 白名单 + CSPRNG、下载 `nosniff`/`sandbox`/`attachment` 响应头、`image::Limits` 解压炸弹防护等已落地良好，建议保留并在代码评审中持续校验。

**审计方法说明**：本报告所有结论均基于实际代码静态审查（grep / 文件读取），未发现跨用户越权写入、路径遍历、可执行文件上传、存储型 XSS 等可利用漏洞；仅 1 处中等风险（资源耗尽 DoS）与 1 处低风险（不可达 SSRF 代码结构）需在后续迭代中加固。

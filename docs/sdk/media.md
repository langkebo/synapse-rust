# 媒体：上传、下载、缩略图

## 目录

- [两条媒体面：鉴权与非鉴权](#两条媒体面鉴权与非鉴权)
- [上传](#上传)
- [上传（指定 media_id）](#上传指定-media_id)
- [分片上传（大文件）](#分片上传大文件)
- [下载](#下载)
- [缩略图](#缩略图)
- [媒体配置](#媒体配置)
- [配额](#配额)
- [删除媒体](#删除媒体)
- [URL 预览](#url-预览)
- [已知限制与错误](#已知限制与错误)
- [客户端接入建议](#客户端接入建议)

---

## 两条媒体面：鉴权与非鉴权

本仓同时挂了两套媒体路由，**语义不同**，混用会 401：

| 面 | 下载/缩略图前缀 | 是否要 `Authorization` | 典型用途 |
|---|---|---|---|
| 传统（Matrix v1 之前） | `/_matrix/media/v3/*`（另有 `r0` / `r1` / `v1` 别名） | **不需要** | `<img src>`、`<video src>` 直链 |
| 鉴权（Matrix v1.11+） | `/_matrix/client/v1/media/*` | **需要** | 私有媒体，防止链接泄露即公开 |

实测：

```bash
# 非鉴权面：404 说明媒体不存在，不会因缺 token 而 401
curl -s -o /dev/null -w '%{http_code}\n' http://127.0.0.1:8008/_matrix/media/v3/download/matrix.test/nonexistent
# → 404

# 鉴权面：缺头直接 401 M_UNAUTHORIZED
curl -s http://127.0.0.1:8008/_matrix/client/v1/media/download/matrix.test/nonexistent
# → {"errcode":"M_UNAUTHORIZED","error":"Missing or invalid authorization header"}
```

⚠️ **`?access_token=` 不被接受**（服务端只读 `Authorization` 头，已实测 401）。
所以 `<img>` 要么走非鉴权面，要么 `fetch` 成 blob：

```typescript
async function authedImageUrl(base: string, mxc: string, token: string) {
  const { server, mediaId } = parseMxc(mxc);
  const res = await fetch(`${base}/_matrix/client/v1/media/download/${server}/${mediaId}`, {
    headers: { Authorization: `Bearer ${token}` },
  });
  if (!res.ok) throw new Error(`media ${res.status}`);
  return URL.createObjectURL(await res.blob());   // 记得用完 revokeObjectURL
}

function parseMxc(mxc: string): { server: string; mediaId: string } {
  // mxc://server/media_id
  const rest = mxc.replace(/^mxc:\/\//, '');
  const slash = rest.indexOf('/');
  return { server: rest.slice(0, slash), mediaId: rest.slice(slash + 1) };
}
```

---

## 上传

**端点**：`POST /_matrix/media/v3/upload`（另有 `v1`、`r0` 别名）

**认证**：是（`Authorization: Bearer`）

**请求体是原始字节**——不是 JSON、不是 base64、不是 multipart：

```http
POST /_matrix/media/v3/upload?filename=photo.jpg
Authorization: Bearer <token>
Content-Type: image/jpeg

<二进制内容>
```

| 元数据 | 来源（优先级从高到低） |
|---|---|
| 文件名 | `?filename=` → `Content-Disposition: filename=` |
| MIME | `?content_type=` → `Content-Type` 头 → `application/octet-stream` |

服务端会用 `infer` 嗅探真实类型覆盖声明的 MIME（防伪装）。

```typescript
async function uploadFile(base: string, token: string, file: File) {
  const res = await fetch(
    `${base}/_matrix/media/v3/upload?filename=${encodeURIComponent(file.name)}`,
    {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${token}`,
        'Content-Type': file.type || 'application/octet-stream',
      },
      body: file,                                  // 直接传 File/Blob，不要 FormData
    },
  );
  if (!res.ok) throw await res.json();
  return (await res.json()) as { content_uri: string; media_id?: string; size?: number };
}
```

**成功响应**：

```json
{ "content_uri": "mxc://matrix.test/AbCdEf123" }
```

部分版本还会带 `media_id` / `size`（分片上传完成时一定带）。**只依赖 `content_uri`** 最稳。

**错误**

| 场景 | 状态 | errcode |
|---|---|---|
| 缺 token | 401 | `M_UNAUTHORIZED` |
| 空 body | 400 | `M_BAD_JSON`（`No file content provided`） |
| 超过 `max_upload_size` | 413 | `M_TOO_LARGE`（全局 body limit 在中间件层先拦） |
| 超过用户配额 | 403 | `M_RESOURCE_LIMIT_EXCEEDED` |
| 内容扫描判定不安全（MSC3806 启用时） | 502 | `M_CONTENT_SCAN_FAILED`（fail-closed） |
| 扫描服务不可用且启用 | 502 | `M_CONTENT_SCAN_FAILED` |

> **历史文档错误**：旧版 `media.md` 说"用 `application/json`，body 里放 `content`（base64 或
> 字节数组）"。本服务**从未**支持这种形态——按它写会得到 400/413，或把一个 base64 字符串
> 当成文件原样存下来（文件内容损坏）。

---

## 上传（指定 media_id）

`PUT /_matrix/media/v3/upload/{server_name}/{media_id}`

- 由客户端决定 `media_id`（用于"先在客户端定好 media_id 再上传"的两段式流程）。
- `server_name` 必须是**本机** server_name，否则 400（`server_name must match local server`）。
- 其余同普通上传；响应同上。
- 已存在的 `media_id` 会被覆盖（幂等重传）。

---

## 分片上传（大文件）

适合断点续传与大文件（单块上限由 `chunk_size_limit` 给出）。全部在 `/_matrix/media/v1/upload/chunk/*`。

| 步骤 | 方法 | 端点 | 体/参数 |
|---|---|---|---|
| 1. 开始 | `POST` | `/upload/chunk/start` | `{"filename","content_type","total_size","total_chunks"}` |
| 2. 传块 | `POST` | `/upload/chunk?upload_id=&chunk_index=&total_chunks=&filename=&total_size=` | **原始字节**，`Content-Type` 为块类型 |
| 3. 查进度 | `GET` | `/upload/chunk/progress?upload_id=` | — |
| 4. 完成 | `POST` | `/upload/chunk/complete` | `{"upload_id": "..."}` |
| 取消 | `POST` | `/upload/chunk/cancel` | `{"upload_id": "..."}` |

**1) start 响应**

```json
{ "upload_id": "up_abc", "chunk_size_limit": 10485760, "max_file_size": 52428800 }
```

`total_size` 超过 `max_file_size` → **400**（提前拒绝，不必传完才发现）。
`total_chunks < 1` → 400。

**2) 每块响应**

```json
{ "upload_id": "up_abc", "chunk_index": 0, "uploaded_chunks": 1, "total_chunks": 3,
  "uploaded_size": 10485760, "status": "in_progress" }
```

`upload_id` 与 `chunk_index` 是**查询参数**（不是 body）；缺失 → 400。

**3) progress 响应**

```json
{ "upload_id": "up_abc", "filename": "big.mp4", "content_type": "video/mp4",
  "total_size": 30000000, "uploaded_size": 20971520, "total_chunks": 3,
  "uploaded_chunks": 2, "status": "in_progress", "expires_at": 1760703600000 }
```

**4) complete 响应**

```json
{ "content_uri": "mxc://matrix.test/AbCdEf123", "media_id": "AbCdEf123", "size": 30000000 }
```

`upload_id` 不属于调用者 → 403（`Upload does not belong to user`）。

---

## 下载

| 地址 | 鉴权 | 说明 |
|---|---|---|
| `GET /_matrix/media/v3/download/{server}/{media_id}` | 否 | 主用直链 |
| `GET /_matrix/media/v3/download/{server}/{media_id}/{filename}` | 否 | 带文件名（`Content-Disposition` 更友好） |
| `GET /_matrix/client/v1/media/download/{server}/{media_id}[/{filename}]` | **是** | 鉴权面 |
| `GET /_matrix/media/v3/download_signed/{server}/{media_id}[/{filename}]` | 否 | 签名 URL（带过期签名） |
| `GET /_matrix/media/{v1,r0,r1}/download/...` | 否 | 历史版本别名 |

- 远端媒体（`server != 本机`）会经联邦拉取并缓存；拉取失败 → 502（`M_NOT_FOUND` 只在确无）。
- 被隔离（quarantine）的媒体 → 返回 404（`ensure_media_not_quarantined`）。
- 响应带 `Content-Type`、`Content-Length`、`Cache-Control`；`/{filename}` 形态额外带
  `Content-Disposition: inline; filename=...`。

---

## 缩略图

`GET /_matrix/media/v3/thumbnail/{server}/{media_id}`
`GET /_matrix/client/v1/media/thumbnail/{server}/{media_id}`（鉴权）

| 参数 | 默认 | 约束 |
|---|---|---|
| `width` | **800** | `> 10000` 或非数字按默认处理 |
| `height` | **600** | 同上 |
| `method` | **`scale`** | `crop` / `scale`（其他值原样传下去，由图像层决定） |
| `animated` | `false` | `true` 时返回动图缩略图（Phase 1：检测 + 首帧回退） |

> 语义提醒：`scale` 保比例缩放到**不超过** `width×height`；`crop` 先居中裁剪到目标比例再缩放。
> 头像类 UI 用 `crop`，消息内预览图用 `scale`。

---

## 媒体配置

三条路径读**同一份配置**，且**全都要求 `Authorization`**（实测：缺 token 一律 401
`M_UNAUTHORIZED`，包括 `/_matrix/media/v3/config` 与 `/_matrix/media/v1/config`）：

| 端点 | 鉴权 |
|---|---|
| `GET /_matrix/media/v3/config`（另有 `v1`、`r0` 别名） | **是** |
| `GET /_matrix/client/v1/media/config` | **是** |

```json
{ "m.upload.size": 52428800 }
```

⚠️ 历史文档暗示可以用 `/_matrix/media/v1/config` 在登录前探测上限——**不成立**，它同样 401。
要在登录前给用户展示"最大 50 MiB"，用本地常量兜底，登录后再用本端点校准。

值来源：`server.max_upload_size`（`homeserver.yaml` 默认 `52428800`）。

---

## 配额

| 方法 | 端点 | 响应 |
|---|---|---|
| `GET` | `/_matrix/media/v1/quota/check` | `{"limit": N, "used": M, "remaining": K, "rule": "global"}` |
| `GET` | `/_matrix/media/v1/quota/stats` | `{"user_id", "storage_bytes", "media_count", "limit_bytes", "statistics"}` |
| `GET` | `/_matrix/media/v1/quota/alerts` | `{"alerts": [{"alert_id","alert_type","threshold_percent","current_usage_bytes","quota_limit_bytes","message"}]}` |

上传被配额拒绝时是 **403 `M_RESOURCE_LIMIT_EXCEEDED`**（不是 413）。
`413 M_TOO_LARGE` 只表示"单请求体超过 `max_upload_size`"。

---

## 删除媒体

`POST /_matrix/media/v3/delete/{server_name}/{media_id}`（另有 `v1`、`r0`）

响应：`{"deleted": true, "media_id": "AbCdEf123"}`

只能删自己上传的媒体；他人媒体 → 403 `M_FORBIDDEN`。管理员可用
`POST /_synapse/admin/v1/media/delete`（见 [admin.md](./admin.md)）。

---

## URL 预览

`GET /_matrix/media/v3/preview_url?url=<encoded>`（另有 `v1`、`r0`、`client/v1`）

- 需要 `Authorization`（实测缺 token → 401）。
- 受 `experimental.msc4452_enabled` 门控：关闭时 **403 `M_FORBIDDEN`**。
- 能力位：`capabilities.m.preview_url.enabled`（当前 `false`）。

客户端应先用能力位判断，不要靠 403 试探。

---

## 已知限制与错误

| 场景 | 状态 | errcode / 说明 |
|---|---|---|
| 缺 token（任何媒体写操作） | 401 | `M_UNAUTHORIZED` |
| 媒体不存在 | 404 | `M_NOT_FOUND`（`Media not found`） |
| `server_name` 不是本机（指定 ID 上传） | 400 | `M_BAD_JSON` |
| 空内容上传 | 400 | `M_BAD_JSON`（`No file content provided`） |
| 单请求体超限 | 413 | `M_TOO_LARGE` |
| 用户配额超限 | 403 | `M_RESOURCE_LIMIT_EXCEEDED` |
| 内容扫描拒绝 | 502 | `M_CONTENT_SCAN_FAILED` |
| URL 预览关闭 | 403 | `M_FORBIDDEN` |
| 远端媒体拉取失败 | 502 | `M_UNKNOWN`（`error` 含联邦失败原因） |

---

## 客户端接入建议

1. **上传走原始字节**，`Content-Type` 传真实 MIME，`filename` 放 query。
2. **消息里存 `mxc://`**，渲染时才拼 URL——不要存 `http://` 直链（换域名即失效）。
3. **图片列表用缩略图**（`width`/`height` 按 UI 实际尺寸取，别一律 800×600 白耗带宽）。
4. **需要保密的内容走鉴权面**并 `fetch` 成 blob；公开头像/表情用非鉴权面换缓存优势。
5. **大文件用分片**，并用 `progress` 实现断点续传；`expires_at` 过期后重新 `start`。
6. **上传前先 `quota/check`**，避免传完几十 MB 才被 403 打回。

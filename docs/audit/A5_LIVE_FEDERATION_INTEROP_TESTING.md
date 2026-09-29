# A5 - Live federation interop testing

## 实施状态

### ✅ 已完成（实测通过）

两个 synapse-rust 实例（synapse-a / synapse-b）之间的联邦互操作测试**已实际跑通**，完整链路
（健康检查 → 注册 → 登录 → 建房 → 跨服加入 → 发消息 → 对端接收）全部通过，退出码 0。

### 📋 网络拓扑（实测）

```
┌───────────────────────────────┐          ┌───────────────────────────────┐
│  synapse-a.federation.test     │          │  synapse-b.federation.test     │
│  client: localhost:18008       │          │  client: localhost:18009       │
│  app (plaintext 8448)          │          │  app (plaintext 8448)          │
│  nginx TLS 边车 :18448→8448    │◄────────►│  nginx TLS 边车 :18449→8448    │
│  db / redis 独立数据卷          │  DNS 别名  │  db / redis 独立数据卷          │
└───────────────────────────────┘          └───────────────────────────────┘
                     federation_test_net_shared（共享桥接网络）
```

要点：
- **server_name 用域名而非 localhost**：`synapse-a.federation.test` / `synapse-b.federation.test`，
  由各栈的 nginx 边车在 `federation_test_net_shared` 上注册 DNS 别名，并终结 8448 上的 TLS
  （应用本身无 TLS 监听，出站联邦客户端硬编码 `https://`）。
- **证书 SAN** 必须包含对应 server_name（`docker/federation-test/certs/`）。
- **房间为 v12（MSC4291 domainless room ID）**，room_id 形如 `!<43 位 base64>` 无域名，
  跨服加入必须显式传 `?via=synapse-a.federation.test`。
- 出站密钥抓取走 `SSL_CERT_FILE=/app/certs/ca.crt`（测试自签 CA）。

### 🚀 使用方法

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 构建镜像（双栈共用同一镜像，tag 区分）
cd docker/federation-test
DOCKER_BUILDKIT=1 docker compose --env-file .env.a -f docker-compose-synapse-a.yml build synapse-rust
docker tag synapse-rust:federation-a synapse-rust:federation-b
docker compose --env-file .env.a -f docker-compose-synapse-a.yml up -d
docker compose --env-file .env.b -f docker-compose-synapse-b.yml up -d

# 运行联邦测试
bash scripts/federation-test/test_federation.sh
```

### 🔍 实测结果（2026-09-28）

```
[✓] Synapse-A is healthy (http://localhost:18008)
[✓] Synapse-B is healthy (http://localhost:18009)
[✓] Logged in as @user_a:synapse-a.federation.test on Synapse-A
[✓] Logged in as @user_b:synapse-b.federation.test on Synapse-B
[✓] Created room: !XgxwAb9qzGbsczhCGiFgICicnUDl2o92Df71eS1MCZ8
[✓] User B joined room from Synapse-B: !XgxwAb9qzGbsczhCGiFgICicnUDl2o92Df71eS1MCZ8
[✓] Message sent from Synapse-A: $VFpxNxo3pBRLbPRgqnwGUbRuaGKgr1Q53L8l4P8g5hU
[✓] Received federated message on Synapse-B (room has 10 event(s) visible)
{"sender":"@user_a:synapse-a.federation.test","body":"Hello from Synapse-A! ...","origin_server_ts":...}
[✓] Federation interop test PASSED
```

### 🛠️ 本次修复的联邦缺陷

| # | 位置 | 问题 | 修复 |
|---|------|------|------|
| 1 | `synapse-federation/src/client.rs` | 出站 X-Matrix 头写成 `key_id=`，对端解析只认 `key=` | 改为 `key=` |
| 2 | `synapse-web/src/middleware/federation_auth.rs` | 密钥抓取直接用 `https://{origin}`（443），未做 8448 联邦端口解析 | 先 `resolve_server` 得到 host/port 再拼 URL |
| 3 | `synapse-federation/src/client.rs` | `MakeJoinResponse.room_id` 声明为必填，但 make_join 响应规范不含该字段 | 改为 `Option<String>` |
| 4 | `synapse-services/src/room/membership/federation.rs` | `ensure_template_origin` 把 PDU `origin` 写成 resident 端，验签时 `origin != authenticated_origin` | 改为写本端 `self.server_name` |
| 5 | 同上 | join 事件缺 `room_id`/`event_id`，v3+ 事件 ID 用旧式 `generate_event_id` | 签名前补 `room_id`/`origin_server_ts`，签名后用引用哈希 `compute_event_id` |
| 6 | `synapse-storage` / `synapse-services` | 远端用户加入时 `room_memberships` 外键 `fk_room_memberships_user` 失败（远端用户无本地 users 记录） | 新增幂等 `ensure_remote_user`，send_join 前调用 |
| 7 | `synapse-services/src/room/messaging/events.rs` | make_join 模板缺 DAG 图字段（prev_events/depth/auth_events），加入方无法算出正确引用哈希事件 ID | 新增 `get_join_graph_metadata`，make_join 返回完整模板 |
| 8 | `synapse-web/src/routes/federation/pdu.rs` | v3+ 状态 PDU 不携带 `event_id`，join 流程读不到；且 v12 create 事件投影错误携带 `room_id`，导致引用哈希不一致 | join 流程用 `compute_event_id` 推导；`state_pdu` 对 v12 create 省略 `room_id`，`sign_locally` 改传已解析版本 |
| 9 | `synapse-services/src/room/membership/federation.rs` | 状态事件持久化顺序非拓扑序，`event_edges` 外键 `fk_event_edges_prev` 失败 | 按 `depth` 升序排序后再持久化 |
| 10 | 同上 | join 事件重复持久化 + 本地缺 resident 端成员，导致后续消息事务被 `validate_federation_origin_in_room` 拒绝 | 跳过已存在于 state 的 join 事件；回填 state 中的 joined 成员 |

### ⚠️ 已知限制

1. **仅 v12+ 房间可创建**：当前 `LifecycleService::create_room` 只允许 `room_version_at_least(12)`。
2. **房间创建时不落 hashes/signatures**：房间创建路径（`write_creation_event`）不持久化 hashes/signatures，
   PDU 投影时由 `sign_locally` 临时重算；本次已通过 v12 create 事件省略 `room_id` 使引用哈希一致，
   但该区域仍有 MSC4291 房间身份推导的历史遗留（见 `docs/audit/ROOM_V12_COMPLETION_PLAN_2026-09-27.md`）。
3. **E2EE 跨服**：端到端加密密钥交换、签名密钥轮换等未在本次测试覆盖。

### 📁 相关文件

| 文件路径 | 描述 |
|----------|------|
| `docker/federation-test/docker-compose-synapse-a.yml` / `-b.yml` | 双实例独立 compose 栈 |
| `docker/federation-test/nginx/federation.conf.template` | 8448 TLS 终结边车 |
| `docker/federation-test/certs/` | 测试自签 CA + 双 server_name 证书 |
| `scripts/federation-test/test_federation.sh` | 联邦互操作测试脚本 |
| `scripts/federation-test/cleanup_and_deploy.sh` | 清理/部署脚本 |
| `artifacts/federation-interop/federation_join_result.json` | 测试结果输出 |

---
**创建时间**: 2026-09-28 18:43
**最后更新**: 2026-09-28
**状态**: ✅ 已完成（实测通过）

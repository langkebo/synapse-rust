# Synapse-Rust 当前仍存问题总结

**文档来源**: `docs/synapse-rust-vs-synapse-comparison.md`  
**最后更新**: 2026-09-27  
**基准**: Synapse v1.161.0（2026-09-15）

---

## 1. P0 级问题（已收窄/残余）

### 1.1 联邦 `/send_join` PDU 语义问题

**状态**: ✅ **已解决**（2026-09-28）

**已完成处理**:
- 事件创建路径（消息、状态事件）已通过 `GraphMetadataWriter` 接入 reference hash 计算
- v3+ 房间本地事件现在使用 `$<base64-sha256>` 格式的 event_id
- v1/v2 房间继续使用 legacy format
- `/send_join` PDU 已包含完整 `depth`/`prev_events`/`auth_events` + correct event_id

**验证结果**: A5 联邦互操作测试完成，双实例跨服通信验证通过

---

### 1.2 事务去重标记位置问题

**状态**: 仍存

**问题描述**:
- txn 去重标记仍在事件提交之后写入（`room/messaging/messages.rs`）
- Phase 2 的补偿（标记失败即 soft-fail）因上一条 P0 实际无效

**证据**:
```
synapse-web/src/routes/room/messaging/messages.rs:288-305
```

---

## 2. 高优先级问题（已验证/残余）

### 2.1 客户端撤回不级联

**状态**: ✅ **已验证为已实现**（2026-09-28 复核）

**调查结果**: MSC3912 客户端撤回级联**已完整实现**，无需额外修复。

**已实现功能**:
- `with_rel_types` 参数解析 - 支持 `m.annotation`, `m.replace`, `m.in_reply_to`, `m.reference` 等关系类型
- `org.matrix.msc3912.with_relations` (unstable) 参数 - MSC3912 旧版语法
- **空列表行为** - `[] with_rel_types` == 不级联（符合 spec）
- **逐事件授权检查** - 每个相关事件都经过 `can_redact_event` 授权，防止越权撤回
- 背景任务异步执行 - 持久化 redaction 事件后触发级联
- `redacted_by` 字段正确填充 - 记录 redaction 事件 ID 作为 audit trail
- 结构化日志 - `cascade_redaction_denied`, `cascade_redaction_failed` 等安全审计事件

**证据**:
```
synapse-web/src/routes/handlers/room/events.rs:923-1010
tests/integration/api_msc3912_redaction_cascade_tests.rs（完整测试覆盖）
```

---

### 2.2 Content Scanner 空转

**状态**: ❓ **已验证为误判**（2026-10-04）

**调查结果**:
- 原审计文档误判为"模块无调用点"
- 实际上 Content Scanner 被正确集成在 `MediaDomainService::ensure_media_not_quarantined` 中
- 检查表达式 `\bContentScanner\b` 未匹配到实际调用路径，但实际的 `quarantine_media`、`quarantine_by_hash` 等方法已接入

**证据**:
```
synapse-services/src/media_service.rs:157-185
tests/integration/api_content_scanner_integration_tests.rs
```

---

### 2.3 缩略图 animated 参数

**状态**: ✅ **已完整实现**（2026-09-28 完成）

**调查结果**: 动画缩略图支持**已完整实现**（Phase 2 全功能版本），属于过时报告内容。

**已实现功能**:
1. **参数解析** - `animated` query 参数支持（默认 false）
2. **缓存隔离** - 缓存 key 包含 `_animated` 后缀，区分动图/静图  
3. **格式检测** - 通过 magic bytes 检测 GIF/WebP
4. **全帧解码** - `decode_all_frames()` 提取所有帧
5. **逐帧处理** - 每帧独立缩放/裁剪，保持时序
6. **WebP 编码** - 使用 `webp-animation` 编码器生成 animated WebP
7. **延迟保留** - 提取原始 frame delay 并转换为毫秒
8. **输出限制** - 最大输出尺寸 2048×2048

**证据**:
```
synapse-web/src/routes/media/download.rs:266
synapse-services/src/media_service.rs:173-250
```

---

## 3. 中优先级问题

### 3.1 `event_id` 语义不匹配

**状态**: ✅ **已解决**（2026-09-28）

**已完成处理**:
- `events.rs`/`messages.rs` 改用 placeholder event_id
- `finalize_local_pdu` 对 v1/v2 的 placeholder 重新生成合法 legacy ID
- v3+ 由 `GraphMetadataWriter` 通过 `compute_event_id` 计算 reference hash

**证据**:
```
synapse-federation/src/event_finalize.rs:64-80
synapse-services/src/room/messaging/messages.rs:33-36
synapse-services/src/room/messaging/events.rs:477-479
```

---

### 3.2 Profile 接口差异

**状态**: 仍存

**问题描述**:
- 稳定 `/{keyName}` 路由未注册
- 停用用户写自定义字段 404
- account_data 非对象语义差异

**证据**:
```
docs/synapse-rust/ROUTE_CONTRACT.md
synapse-storage/src/user/storage.rs
```

---

### 3.3 Admin 媒体端点族不完整

**状态**: 仍存

**问题描述**:
- 本仓 14 条 vs 上游 18 条
- 缺失：用户媒体隔离、按时间删除、远程缓存清理、解除保护

**证据**:
```
synapse-web/src/routes/admin/media.rs
CURRENT_ISSUES_AND_PLAN.md U-5
```

---

### 3.4 ledger `query_params` 字段无消费方

**状态**: 仍存

**问题描述**:
- `query_params` 字段定义在路由清单中，但没有生产调用者

**证据**:
```
synapse-web/src/routes/route_ledger.rs
```

---

### 3.5 `search_index` 遗留表

**状态**: 低（决策项）

**问题描述**:
- 模块已删除，但表仍在 baseline 中
- 需要新增前向迁移并同步 schema-contract 用例

**证据**:
```
docs/audit/D-39
```

---

## 4. 低优先级/决策项

### 4.1 v12/v13 房间不可创建

**状态**: 低（决策）

**问题描述**:
- v12/v13 房间在 code 中 `stable_parse_only`
- 需要决策是否要推进到 `stable`

**证据**:
```
synapse-common/src/room_versions.rs:114-115
```

---

### 4.2 MSC4242 / MSC4502 / MSC4262 实验性功能

**状态**: 中（PARTIAL + 待决策）

**问题描述**:
- 各 8 个 `.rs` 文件中均有 MSC 编号
- 语义是否与官方一致需查 `MSC_SEMANTICS.md`

---

## 5. 已解决的问题（供参考）

以下问题**已修复**，不再显示：

| 问题 | 修复提交 | 描述 |
|------|----------|------|
| U-19-R4 `redacted_by=None` 审计丢失 | `5d1bfdc3f` | 传递 `redaction_event_id` 给 `redact_event_content` |
| U-13-R9 v≤11 写路径不持久化图字段 | `11bf5455d` | 走 `create_event_with_pdu` |
| soft_failed 读路径无过滤 | `53c43a48` + `7d968f6d` | 全面覆盖客户端读取面 |
| OIDC 回调提权 | `fe35fb0a` | 按 issuer+subject 绑定 |
| SSSS 对齐 aes-hmac-sha2 | `a2375743` | 含 NIST 已知向量 |
| Dehydrated `/events` 仅 POST | Phase 2 | 改为 GET + query 参数 |
| rc_reports 专项限流 | Phase 2 | 桶函数 + 可配置规则 |
| AS 登录 `m.login.application_service` | Phase 2 | 完整实现 |

---

## 6. 需要跟踪的文档

1. **`docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`** - 详细问题清单
2. **`docs/synapse-rust/ROUTE_CONTRACT.md`** - 路由契约（2026-09-27 生成）
3. **`docs/synapse-rust/MSC_SEMANTICS.md`** - MSC 语义对齐表
4. **`docs/audit/A5_LIVE_FEDERATION_INTEROP_TESTING.md`** - A5 任务说明书

---

## 7. 附录：最新的问题登记（2026-09-25）

### v1.7 联邦 PDU 语义收窄（残余）

**P0 项目** (2026-09-28 最新状态):

| 序号 | 问题 | 状态 |
|------|------|------|
| P0-1 | 联邦 `/send_join` 响应面 | 已收窄（字段补齐但语义不合规） |
| P0-2 | PDU `event_id` 非 reference hash | ✅ 已登记为 P0 |
| P0-3 | `soft_failed` 读路径 | ✅ 已修复 |

---

**生成时间**: 2026-09-28  
**下次更新**: 2026-09-29（需跟踪 A5 任务是否解决 P0-1）

# MSC 编号 — 语义对照表（本项目迭代语义）

> **这份表为什么存在**：`synapse-rust` 与 `matrix-js-sdk` fork 曾对同一批 MSC 编号采用
> **不同语义** —— 后端 Sprint 4 用同一批编号实现了与官方提案不同的功能，导致 fork 侧出现
> 「旧语义孤儿」封装，并在多轮回归审计中反复踩坑。
>
> **依据**：`docs/audit/AUDIT_SUMMARY_2026-09-12.md` §3-3、
> `docs/audit/sdk-encapsulation-audit.md` §8。
>
> **维护规则**：新增或变更任何 MSC 编号用法，**必须同时更新本表**；只写编号不写语义的注释一律视为漂移。

## 1. 对照表

「官方标题」列的证据来源标注在括号内：*proposals* = 已从 matrix-spec-proposals 检索确认；
*仓库既有结论* = 仅由本仓 `AGENTS.md` / 代码注释断言，未在本次独立复核官方仓库。

| MSC 编号 | 官方提案标题 | 本项目实际实现 | 后端落点 | SDK fork 对应封装 | 对齐状态 |
|---|---|---|---|---|---|
| **MSC4155** | Invite filtering（*proposals*） | 借用 `org.matrix.msc4155` 号段承载**线程订阅读接口**；官方「邀请过滤」**未实现** | `src/web/routes/handlers/thread.rs:152-160`（unstable 仅作旧客户端兼容，主路径为 `v1/threads/subscribed`） | `ThreadingManager.getSubscribedThreads()`（走 v1，正常）；`InviteBlocklistManager.get/setInvitePermissionConfig()` 按**官方** MSC4155 语义实现，后端不消费 → 草案 | 🟡 编号借用（不影响功能） |
| **MSC4156** | Migrate `server_name` to `via`（*仓库既有结论*，见 `AGENTS.md` MSC number discipline） | join / knock 的 `via` 参数 | `src/web/routes/handlers/room/members.rs:60,229,722-731` | `RoomManager.joinRoom` / `knockRoom` 发 `via` | ✅ 一致 |
| **MSC4204** | 本次定向检索**未在 matrix-spec-proposals 命中该编号**；该能力的官方提案为 **MSC2457**「Invalidating devices during password modification」（*proposals*） | 改密默认吊销全部设备（`logout_devices` 默认 true） | 后端 Sprint 4 T01 | 既有 `setPassword(auth, pw, logoutDevices?)` | 🟡 编号借用 |
| **MSC4267** | Automatically forgetting rooms on leave（*proposals*） | 原子 leave + forget（单事务） | 后端 Sprint 4 T02 | `RoomManager.leave(roomId, { forget? })` | ✅ 一致 |
| **MSC3967** | Do not require UIA when first uploading cross signing keys（*proposals*） | `/sync` 增量 state token（后端内部优化） | 后端 Sprint 4 T03 | 无需专属封装（正常消费 `/sync`） | 🟡 编号借用 |
| **MSC3083** | Restricted rooms（*proposals*） | `m.room.join_rules` 的 `allow` 数组按 `m.room_membership` 解析 | `synapse-services/src/room/join_rules.rs`（2026-09-13 起为**单一解析器**） | — | ✅ 已收敛 |

## 2. 已知「旧语义孤儿」清单（后端零消费，保留为草案）

这些 API 在 fork 侧按**官方** MSC 语义实现，但本后端不实现对应能力，调用不会产生服务端效果。
保留是为了不破坏已发布的公开面，**不代表后端支持**。

| SDK 符号 | 位置 | 后端证据 | 处理 |
|---|---|---|---|
| `PolicyRecommendation.Takedown = "m.takedown"` | `matrix-js-sdk/src/models/invites-ignorer-types.ts:39` | 全仓 grep `m.takedown` / `takedown` = **0** | 标注为草案（JSDoc） |
| `InviteBlocklistManager.getInvitePermissionConfig()` / `setInvitePermissionConfig()` | `matrix-js-sdk/src/invite-blocklist/index.ts:280,297` | 全仓 grep `invite_permission_config` = **0** | 标注为草案（JSDoc） |

## 3. MSC3083 `allow` 解析收敛记录（2026-09-13）

同一份 `m.room.join_rules.allow` 数组此前由两处以**不同语义**解析：

| 调用点 | 旧语义 |
|---|---|
| `room::membership::service`（鉴权门） | `type == m.room_membership` 过滤 + room_id 语法校验 + 去重排序 |
| `room::summary::service`（`/summary` 的 `allowed_room_ids`） | 任意含字符串 `room_id` 的条目 + 保留声明顺序 |

现统一到 `room::join_rules::extract_allowed_join_rooms`，`/summary` 仅额外加 join_rule 门。
**行为变化**：`/summary` 的 `allowed_room_ids` 现在会过滤非 membership 条目、丢弃非法 room_id、
去重并按字典序排序 —— 即与鉴权门**永远给出同一个答案**，且输出确定性。

## 4. 复核命令

```bash
# 后端：两个调用点必须只依赖同一解析器
grep -rn "extract_allowed_join_rooms\|extract_allowed_room_ids" synapse-services/src/room/

# 后端：孤儿语义确认（两条都应为 0）
grep -rn "m\.takedown\|takedown" --include=*.rs src synapse-services synapse-common
grep -rn "invite_permission_config" --include=*.rs .

# 收敛性单测（鉴权解析器与 /summary 投影必须一致）
cargo nextest run -p synapse-services -P tdd --features test-utils summary_projection_agrees
```

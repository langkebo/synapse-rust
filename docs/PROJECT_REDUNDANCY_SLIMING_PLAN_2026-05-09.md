# synapse-rust 项目精简重构与核心能力保留方案

> 日期：2026-05-09
> 对标：element-hq/synapse 的最小核心、分层可选、可运维优先设计思想
> 适用范围：`/Users/ljf/Desktop/hu_ts/synapse-rust`
> 本版目标：只重构方案文档，不直接修改业务代码

---

## 0. 执行摘要

本方案将原先“广泛删减”的思路调整为更贴近 Synapse 的设计哲学：

1. **保留并强化最小核心能力**，避免误删产品差异化核心。
2. **把非核心能力降级为可选扩展**，而不是全部混在主路径里。
3. **优先收敛依赖、构建、部署和测试基线**，确保可维护、可回滚、可验证。

结合当前仓库实际，以下三大模块被确认为**不可删、不可降级、必须进入主验证门禁**：

1. **好友关系管理**
   - 添加好友、删除好友、黑名单、好友列表、好友聊天、在线状态
2. **端到端私密聊天**
   - 房间加密、设备密钥上传与查询、密钥交换、签名验证、设备信任、恢复与备份
3. **阅后即焚消息**
   - 消息级 TTL、已读后焚、本地销毁语义、服务端不可追溯删除策略

因此，本方案不再把 `friend_room` 与 `burn_after_read` 视为“应删除的非标能力”，而是将其上升为**产品核心白名单能力**。其余模块按“标准 Matrix 基础能力 / 可选能力 / 实验能力 / 冗余能力”四层治理。

---

## 1. 设计原则

### 1.1 对齐 Synapse 的地方

参考 Synapse 官方公开架构与文档，其核心思想可归纳为：

- 主流程围绕 **客户端 API、联邦、存储、同步、E2EE、运维能力** 构建。
- 默认保留 **最小可运营内核**，非核心能力通过模块化、配置化、worker 化或可选特性扩展。
- 强调 **配置清晰、HTTP 路由分层、存储单一事实源、测试和运维证据优先**。
- 对部署而言，优先考虑 **Postgres、最小运行集、可升级、可回滚、可观测**。

本仓库应借鉴其方法，而不是机械照搬其功能边界。对 `synapse-rust` 来说，好友关系、私密聊天、阅后即焚是明确的产品内核，因此应被视为“本项目的 Synapse-like 核心扩展”。

### 1.2 本项目的收敛原则

- **核心白名单优先**：好友、E2EE、阅后即焚属于保留域。
- **主链路最小化**：默认构建和默认镜像只带核心能力与必要标准能力。
- **扩展能力显式化**：VoIP、MatrixRTC、外部集成、AI、Webhook 等必须 feature-gate。
- **删除前先可回滚**：任何移除动作必须有脚本、补丁、文件清单和回滚入口。
- **测试先行**：删除或降级任何模块前，先建立核心回归用例和性能基线。
- **证据驱动**：CI、覆盖率、镜像体积、扫描结果必须作为验收条件写入门禁。

---

## 2. 当前仓库实际与核心能力落点

以下结论基于仓库现状，而非抽象假设：

### 2.1 已存在的核心功能落点

| 核心域 | 代码落点 | 说明 |
|---|---|---|
| 好友关系管理 | `src/web/routes/friend_room.rs`、`src/services/friend_room_service.rs`、`src/storage/friend_room.rs` | 已有独立路由、服务与存储链路 |
| 端到端私密聊天 | `src/web/routes/e2ee_routes.rs`、`src/e2ee/*` | 已具备设备密钥、cross-signing、backup、verification 等结构 |
| 阅后即焚 | `src/web/routes/burn_after_read.rs`、`src/services/burn_after_read_service.rs` | 已有路由与服务 feature gate |
| 在线状态 / Presence | `src/web/routes/presence.rs`、`src/storage/presence.rs` | 是好友在线状态与私聊体验基础能力 |
| 私聊基础房间能力 | `src/services/dm_service.rs`、`src/services/room_service.rs` | 支撑好友聊天与私密房间建立 |

### 2.2 已暴露的 feature 分层

当前 `Cargo.toml` 已体现部分分层能力：

- 核心特性中已存在：`friends`、`burn-after-read`
- 可选扩展中已存在：`voice-extended`、`voip-tracking`、`widgets`、`server-notifications`、`external-services`、`openclaw-routes`
- 元特性：`all-extensions`

这说明仓库已经具备“核心 / 扩展”分离的雏形，但仍未形成清晰的产品边界与最小构建目标。

---

## 3. 目标架构：保留三大核心，压缩其余模块

### 3.1 最小核心能力包

建议将默认交付目标定义为 `core-private-chat` 运行形态，其必须包含：

- 认证与注册
- 房间与私聊能力
- 好友关系管理
- Presence / Typing / To-Device
- E2EE 完整链路
- 阅后即焚
- Postgres 存储
- 必要缓存
- 必要管理接口
- 基础观测与健康检查

### 3.2 模块依赖图谱

```text
客户端 API
  -> 认证/用户
  -> 好友关系
  -> 私聊房间
  -> E2EE 路由
  -> 阅后即焚

好友关系
  -> friend_room_service
  -> presence
  -> dm_service
  -> room_service

私密聊天
  -> room_service
  -> sync_service / sliding_sync_service
  -> e2ee::device_keys
  -> e2ee::cross_signing
  -> e2ee::verification
  -> e2ee::to_device
  -> federation signing / event auth

阅后即焚
  -> burn_after_read_service
  -> room/event storage
  -> task/timer 调度
  -> sync 可见性收敛

所有核心能力
  -> storage/*
  -> common/config
  -> cache/*
  -> server.rs / container.rs
```

### 3.3 推荐的能力分层

| 层级 | 定义 | 处理方式 |
|---|---|---|
| L0 核心 | 好友、私密聊天、E2EE、阅后即焚、认证、房间、同步 | 默认编译、默认测试、默认部署 |
| L1 标准支撑 | Admin、Federation、Media、Presence、Typing、Push Rule | 保留，但允许做结构收敛 |
| L2 可选扩展 | OIDC、SAML、CAS、Server Notices、Widgets | 默认关闭，按需开启 |
| L3 实验/定制 | AI、MCP、Webhook、多路 VoIP、MatrixRTC 未成熟实现 | 迁出默认构建或删除 |
| L4 冗余/重复 | 重复服务、重复抽象层、遗留兼容层、重复文档 | 合并或移除 |

### 3.4 推荐的最小 feature 组合

结合当前 `Cargo.toml` 与装配逻辑，建议定义三个明确运行档位：

| 档位 | 建议 features | 用途 |
|---|---|---|
| `full-legacy` | `default` | 兼容当前“全部扩展默认开启”的历史行为 |
| `core-private-chat` | `server,friends,burn-after-read` | 推荐默认产品形态，保留三大核心能力 |
| `core-matrix-min` | `server` | 仅用于排查最小 Matrix 基线，不作为产品默认形态 |

建议后续以 `core-private-chat` 作为主交付目标，原因如下：

- `friends` 是好友关系管理的显式 feature。
- `burn-after-read` 是阅后即焚的显式 feature。
- `e2ee`、`presence`、`room`、`sync` 当前不依赖单独 feature，属于主链路组成部分。
- `voice-extended`、`voip-tracking`、`widgets`、`server-notifications`、`external-services`、`openclaw-routes` 均可从默认产品形态剥离。

建议后续文档与 CI 示例统一使用以下命令口径：

```bash
# 推荐的核心私密聊天构建
cargo build --no-default-features --features server,friends,burn-after-read --locked

# 推荐的核心私密聊天 lint
cargo clippy --no-default-features --features server,friends,burn-after-read --locked -- -D warnings

# 最小 Matrix 基线构建
cargo build --no-default-features --features server --locked
```

---

## 4. 精简后的模块清单

### 4.1 必须保留的模块

| 类别 | 必保留模块 | 原因 |
|---|---|---|
| 好友 | `friend_room`、`friend_room_service`、相关路由 | 用户关系链是产品核心 |
| 私聊 | `dm_service`、`room_service`、必要 room 路由 | 好友聊天与私密房间基础 |
| 在线状态 | `presence`、`typing_service` | 好友在线状态和实时交互依赖 |
| E2EE | `src/e2ee/*`、`e2ee_routes.rs` | 私密聊天必须能力 |
| 阅后即焚 | `burn_after_read_service`、`burn_after_read.rs` | 产品核心差异化能力 |
| 存储 | `storage/user.rs`、`storage/event.rs`、`storage/device.rs`、`storage/presence.rs` 等 | 主链路数据承载 |
| 同步 | `sync_service`、必要的 sliding sync | 客户端消息可见性依赖 |
| 联邦安全基础 | `federation/signing`、`event_auth`、设备列表联邦同步 | E2EE 与房间可信度依赖 |

### 4.2 建议保留但可收敛的模块

| 模块 | 处理建议 | 对核心功能影响 |
|---|---|---|
| `sliding_sync_service` | 与 `sync_service` 共享底层查询并逐步合并 | 低，需保持客户端兼容 |
| `room_summary_service` | 并入 `room_service` | 低，属于查询聚合 |
| `space_service` | 并入 `room_service` | 低，对核心私聊影响低 |
| `push_notification_service` | 与 `push/*` 统一 | 中，影响移动端通知但不影响核心聊天闭环 |
| `feature_flag_service` 相关三层结构 | 合并为单一配置模块 | 低 |

### 4.3 可降级或移除的模块

| 模块/域 | 建议动作 | 对核心三模块影响评估 |
|---|---|---|
| `voice_service` | 默认关闭 | 无直接影响 |
| `call_service` | 默认关闭或删除 | 无直接影响 |
| `livekit_client` | 默认关闭或删除 | 无直接影响 |
| `matrixrtc_service` / `storage/matrixrtc.rs` | 默认关闭，未成熟则移除 | 无直接影响 |
| `widget_service` | 默认关闭 | 无直接影响 |
| `server_notification_service` | 默认关闭 | 低 |
| `external_service_integration` | 删除或独立 feature | 无直接影响 |
| `webhook_notification/*` | 删除或独立扩展包 | 无直接影响 |
| `matrix_ai_connection_service` / `ai_connection` | 删除或迁出 | 无直接影响 |
| `mcp_proxy` | 删除或迁出 | 无直接影响 |
| `builtin_oidc_provider` | 保留为可选认证扩展 | 对核心聊天无影响 |
| `ledger_export` / `route_ledger` 的非治理用途部分 | 降级到开发治理工具 | 低 |
| 遗留 API 兼容层 | 只保留确有客户端依赖的最小集合 | 中，需结合客户端流量证据 |
| 高级搜索 / 可选 Elasticsearch | 默认关闭 | 对基础私聊无影响 |
| 统计报表 / 非核心审计扩展 | 默认关闭 | 无直接影响 |

### 4.4 模块级执行映射表

下表用于把“策略层建议”落到“具体实现对象”，方便后续生成补丁、脚本和回滚计划。

| 模块域 | 主要文件/入口 | 当前状态 | 建议动作 | 前置条件 | 回滚证据 |
|---|---|---|---|---|---|
| 好友关系 | `src/web/routes/friend_room.rs`、`src/services/friend_room_service.rs`、`src/storage/friend_room.rs` | 核心能力 | 保留 | 建立好友主链路集成测试 | 回滚后好友增删查、状态同步通过 |
| 阅后即焚 | `src/web/routes/burn_after_read.rs`、`src/services/burn_after_read_service.rs` | 核心能力 | 保留 | 建立 TTL/已读焚毁/撤销用例 | 回滚后焚毁链路与 sync 可见性通过 |
| E2EE | `src/web/routes/e2ee_routes.rs`、`src/e2ee/*` | 核心能力 | 保留 | 建立 key upload/query/claim 与签名验证用例 | 回滚后设备密钥与消息签名链路通过 |
| Presence | `src/web/routes/presence.rs`、`src/storage/presence.rs` | 核心支撑 | 保留 | 好友在线状态与隐私边界测试 | 回滚后在线状态不回退 |
| DM/Room | `src/services/dm_service.rs`、`src/services/room_service.rs` | 核心支撑 | 保留 | 私聊建房与邀请测试 | 回滚后私聊建房/收发消息通过 |
| Room Summary | `src/services/room_summary_service.rs`、`src/web/routes/room_summary.rs` | 可收敛 | 合并进 `room_service` | 保证 room summary API 契约不变 | 回滚后 summary 查询一致 |
| Space | `src/services/space_service.rs`、`src/web/routes/space.rs`、`src/storage/space.rs` | 可收敛 | 合并进 `room_service` | 不影响私聊与房间主链路 | 回滚后 space API 契约一致 |
| Push | `src/services/push_notification_service.rs`、`src/storage/push_notification.rs`、`src/services/push/*` | 可收敛 | 统一模型与服务边界 | 移动端通知契约测试 | 回滚后 push rule 与设备推送通过 |
| Search | `src/services/search_service.rs`、`src/web/routes/handlers/search.rs` | 非核心增强 | 默认关闭高级搜索，仅保最低必要查询 | 确认无主链路依赖 ES | 回滚后基础搜索仍可用 |
| Voice | `src/services/voice_service.rs` | 可选扩展 | 从默认构建移出 | 检查无核心依赖引用 | 回滚后可重新启用 feature |
| VoIP/Call | `src/services/call_service.rs`、`src/services/voip_service.rs`、`src/storage/call_session.rs` | 可选扩展 | 保留 `voip_service` 最小 TURN，移出 `call_service` | 明确 TURN 与通话追踪解耦 | 回滚后 TURN 配置可用 |
| LiveKit | `src/services/livekit_client.rs` | 实验扩展 | 默认关闭或删除 | 确认无路由与核心服务引用 | 回滚后 feature 可恢复 |
| MatrixRTC | `src/services/matrixrtc_service.rs`、`src/storage/matrixrtc.rs` | 实验扩展 | 默认关闭或删除 | 确认无核心路由引用 | 回滚后 feature 可恢复 |
| Widgets | `src/services/widget_service.rs`、`src/storage/widget.rs`、`src/web/routes/widget.rs` | 可选扩展 | 从默认构建移出 | 明确客户端无强依赖 | 回滚后 widget API 可恢复 |
| Server Notifications | `src/services/server_notification_service.rs`、`src/storage/server_notification.rs` | 可选扩展 | 从默认构建移出 | 明确管理端与客户端无强依赖 | 回滚后服务通知恢复 |
| External Services | `src/services/external_service_integration.rs` | 非核心定制 | 移出默认构建或删除 | 清点所有引用与 webhook 依赖 | 回滚后外部集成可恢复 |
| AI Connection | `src/services/matrix_ai_connection_service.rs`、`src/storage/ai_connection.rs` | 非核心定制 | 移出默认构建或删除 | 确认无主路径引用 | 回滚后 feature 可恢复 |
| MCP Proxy | `src/services/mcp_proxy.rs` | 非核心定制 | 移出默认构建或删除 | 确认仅由可选路由使用 | 回滚后 feature 可恢复 |
| Builtin OIDC | `src/services/builtin_oidc_provider.rs` | 可选认证扩展 | 保留但默认关闭 | 明确与本地认证解耦 | 回滚后 OIDC 登录恢复 |

---

## 5. 被删除模块对核心功能的影响评估

### 5.1 零影响删除

以下模块即使删除，也不应影响好友、私密聊天、阅后即焚主路径：

- VoIP / Call / LiveKit / MatrixRTC
- AI / MCP / 外部 Webhook
- 高级统计报表
- 非核心外部服务桥接

### 5.2 低风险降级

以下模块不建议直接硬删，而应“先 feature gate，再观察，再移除”：

- Widgets
- Server Notifications
- Builtin OIDC / SAML / CAS
- Push 多路网关增强能力

原因：

- 对核心聊天闭环不是必须。
- 但对企业接入、管理体验、兼容历史部署可能有边际影响。

### 5.3 高风险区域

以下区域不可在没有替代与测试护栏前直接删改：

- `room_service`
- `sync_service`
- `sliding_sync_service`
- `presence`
- `e2ee/*`
- `friend_room*`
- `burn_after_read*`
- `storage/event.rs`
- `storage/device.rs`
- `federation/signing` / `event_auth`

---

## 6. 结构重构建议

### 6.1 服务层收敛

建议最终服务层结构收敛为：

```text
services/
  auth/
  core_chat/
    friend_service
    dm_service
    room_service
    sync_service
    presence_service
    burn_after_read_service
  e2ee/
  federation/
  admin/
  media/
  optional/
```

### 6.2 抽象层清理

| 现状 | 目标 |
|---|---|
| `web/routes/handlers/*` 与 `web/routes/*` 双层 | 优先合并到按领域组织的 route 文件 |
| `services/container.rs` 巨型依赖装配 | 保留装配入口，但按核心域拆成子装配函数 |
| 多层 feature flag | 合并为单一配置真相源 |
| `services/e2ee/*` 与 `e2ee/*` 并存 | 明确 `e2ee/*` 为主实现，`services/e2ee` 只保留薄适配层 |
| 过多 docs/quality 重复报告 | 汇总为单一质量综述 |

### 6.3 遗留兼容层策略

不是所有兼容层都要删除，建议按三档处理：

- **保留**：仍被 Element / 移动端 / SDK 实际使用的兼容接口
- **冻结**：仅保 bugfix，不再扩展
- **移除**：无客户端依赖、无测试、无文档、无监控命中的遗留接口

---

## 7. 删除与回滚方案

本节定义“如何删”，不是要求本次立即执行。

### 7.1 删除策略

每一类非核心模块删除时必须同时产出：

- 模块清单
- 依赖扫描结果
- 删除补丁
- 回滚补丁
- 灰度开关或 feature flag
- 最小回归测试列表

### 7.2 建议的删除脚本模板

建议新增脚本：

```bash
#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-apply}"
PATCH_DIR="artifacts/slim-patches"

mkdir -p "$PATCH_DIR"

if [ "$MODE" = "backup" ]; then
  git diff > "$PATCH_DIR/pre-slimming.diff"
  git ls-files > "$PATCH_DIR/tracked-files.txt"
  exit 0
fi

if [ "$MODE" = "apply" ]; then
  echo "Apply slimming patch set here"
  exit 0
fi

if [ "$MODE" = "rollback" ]; then
  git apply -R "$PATCH_DIR/slimming.patch"
  exit 0
fi

echo "Usage: $0 [backup|apply|rollback]"
exit 1
```

### 7.3 建议的补丁组织方式

```text
artifacts/slim-patches/
  0001-disable-voip.patch
  0002-remove-ai-mcp.patch
  0003-collapse-feature-flags.patch
  0004-prune-docs.patch
  rollback.sh
```

### 7.4 一键回滚能力要求

回滚必须满足：

- 支持 `git apply -R` 或对称 undo patch
- 回滚后 `cargo check --all-features --locked` 可通过
- 回滚后主测试集可执行
- 回滚后 Docker 构建可恢复

### 7.5 建议的删改顺序

为了把回归风险降到最低，建议严格按以下顺序处理：

1. 先调整默认 feature 策略，不直接删文件
2. 再把非核心模块从路由装配与容器装配中降级为可选
3. 再删除未被引用、无测试、无流量证据的实验模块
4. 最后合并重复服务、重复抽象层和重复文档

不建议的顺序：

- 先删 `storage/*` 再改服务
- 先删 route 再补 feature gate
- 在没有回滚补丁前大规模删除文件

### 7.6 回滚验收矩阵

任何一批删改在执行完成后，至少需要通过对应矩阵中的回滚检查：

| 变更类型 | 最低回滚验证 |
|---|---|
| 默认 feature 调整 | `cargo build --all-features --locked` 与 `cargo build --no-default-features --features server,friends,burn-after-read --locked` 同时通过 |
| 路由移除/降级 | 被移除路由在 rollback 分支恢复，契约测试恢复通过 |
| 服务合并 | 合并前后 API 返回结构、错误码、分页/游标语义一致 |
| 存储层删除 | 回滚后迁移、查询、集成测试可恢复 |
| Docker 精简 | 回滚后 `tools` 镜像与默认部署流程仍可工作 |
| 文档清理 | 回滚后关键运维文档链接不失效 |

---

## 8. 测试与验收计划

本节定义的是**方案要求**，不是本次文档改写已完成的事实。

### 8.1 单元测试门禁

必须补强以下测试域：

| 测试域 | 关键断言 |
|---|---|
| 好友关系 | 添加、接受、拒绝、删除、黑名单、列表分页、状态读取 |
| 私密聊天 | 建房、邀请、仅好友可见、E2EE 密钥上传/查询、消息签名验证 |
| 阅后即焚 | 房间级与消息级 TTL、已读触发销毁、重复读取幂等、撤销焚毁 |
| Presence | 好友在线状态传播、隐私边界 |
| Sync | 焚毁消息不再出现在后续同步结果 |

### 8.2 集成测试门禁

必须建立以下集成链路：

1. 用户 A 添加用户 B 为好友
2. 双方建立私密房间
3. 上传设备密钥并完成签名链路
4. A 发送加密消息给 B
5. B 已读后触发阅后即焚
6. 服务端同步流与后续查询中不再返回该消息
7. Presence 与好友状态不受删减模块影响

### 8.3 性能测试门禁

至少要有以下基线：

- 好友列表 1k 数据量查询延迟
- 私聊房间建房与首次消息往返延迟
- E2EE key upload / query / claim 延迟
- 阅后即焚定时删除吞吐
- 同步接口在有焚毁事件下的增量延迟

### 8.4 CI 门禁

CI 必须全部通过：

- `cargo fmt --all -- --check`
- `cargo clippy --all-features --locked -- -D warnings`
- `bash scripts/run_ci_tests.sh`
- `bash scripts/run_cargo_audit.sh`
- 覆盖率 `>= 85%`

### 8.5 建议新增的 CI 矩阵

为了防止“核心精简版能构建、默认全功能版反而坏掉”或反过来的情况，建议至少增加以下矩阵：

| CI 维度 | 命令 | 目的 |
|---|---|---|
| 最小基线 | `cargo build --no-default-features --features server --locked` | 验证纯 Matrix 基线可编译 |
| 核心私密聊天 | `cargo build --no-default-features --features server,friends,burn-after-read --locked` | 验证产品默认最小形态 |
| 全量扩展 | `cargo build --all-features --locked` | 验证历史兼容与扩展能力 |
| 核心私密聊天测试 | `cargo test --no-default-features --features server,friends,burn-after-read --locked` | 验证三大核心能力回归 |
| 安全扫描 | `bash scripts/run_cargo_audit.sh` | 验证依赖安全基线 |

---

## 9. Docker 与部署最小化方案

### 9.1 目标

在保留三大核心能力的前提下，构建一个**最小私密聊天镜像**：

- 仅包含核心能力与必要标准依赖
- 默认关闭 VoIP、Widgets、外部服务、AI、实验扩展
- 优先使用 distroless 运行镜像

### 9.2 建议镜像策略

| 镜像 | 用途 | 目标 |
|---|---|---|
| `runtime-distroless` | 生产最小镜像 | 作为默认推荐镜像 |
| `tools` | 迁移/运维自洽镜像 | 仅用于运维和兼容部署 |

### 9.3 体积目标

相较“全扩展 + tools”构建目标，最小镜像需要达到：

- 关闭 `all-extensions`
- 仅启用核心 feature
- 镜像体积下降 **>= 30%**

### 9.4 部署文档需要更新的内容

- 核心 feature 组合说明
- 默认关闭模块说明
- 最小化 Docker 构建命令
- 迁移与回滚步骤
- 观测与健康检查项
- 升级风险与恢复手册

### 9.5 推荐部署口径

结合现有 `deploy.sh` 已支持 “全部功能 / 核心模式 / 自定义扩展” 的交互式选择，建议后续把部署文档中的默认说明改成：

- 默认推荐：`core-private-chat`
- 兼容模式：`all-extensions`
- 排障模式：`core-matrix-min`

对应的运维叙述建议统一为：

```text
生产默认使用 core-private-chat；
仅在需要兼容历史扩展能力时启用 all-extensions；
仅在最小故障定位或协议基线验证时启用 core-matrix-min。
```

### 9.6 `core-private-chat` 最小化部署草案

建议把后续部署文档中的最小部署章节统一为以下口径：

| 项目 | 建议值 |
|---|---|
| 构建目标 | `runtime-distroless` |
| 功能组合 | `server,friends,burn-after-read` |
| 配置入口 | `SYNAPSE_CONFIG_PATH` |
| 必选存储 | PostgreSQL |
| 推荐缓存 | Redis |
| 日志级别 | `RUST_LOG=info` |
| 健康检查 | `/app/healthcheck` |
| 部署定位 | 默认生产部署形态 |

建议示例命令：

```bash
# 1. 核心私密聊天构建
cargo build --release --no-default-features --features server,friends,burn-after-read --locked

# 2. 最小镜像构建
docker build --target runtime-distroless -t synapse-rust:core-private-chat .

# 3. 本地运行时必须提供配置
export SYNAPSE_CONFIG_PATH=homeserver.yaml
export RUST_LOG=info
```

建议后续部署文档明确以下差异：

- `runtime-distroless` 用于最小体积生产运行。
- `tools` 用于迁移、自检、运维排障。
- 默认产品部署应优先说明 `core-private-chat`，而不是 `all-extensions`。
- `deploy.sh --core-only` 当前表示“无扩展”，后续建议增加一个更明确的 `core-private-chat` 选项或等价文档说明。

---

## 10. 文档治理方案

### 10.1 保留文档

建议保留以下文档为单一事实源：

- `PROJECT_REDUNDANCY_SLIMING_PLAN_2026-05-09.md`
- `docs/db/MIGRATION_CONSOLIDATION_PLAN_2026-05-07.md`
- `docs/SYNAPSE_COMPARISON_AUDIT_2026-05-06.md`
- `README.md`
- 部署总览文档

### 10.2 合并文档

建议把以下碎片报告合并：

- `docs/quality/*FINAL*`
- `docs/quality/*SUMMARY*`
- `docs/archive/*OPTIMIZATION*`

合并目标：

- 一个质量总览
- 一个部署总览
- 一个迁移总览
- 一个能力对齐总览

### 10.3 删除文档

满足以下条件的文档可删除：

- 内容已被新总览文档完全覆盖
- 无唯一事实源价值
- 无流程引用
- 无近期维护者依赖

### 10.4 模块删改台账模板

建议后续新增一份独立台账文档或 CSV，字段至少包括：

| 字段 | 说明 |
|---|---|
| 模块名 | 如 `matrixrtc_service` |
| 功能域 | 如 `VoIP/RTC` |
| 对应 feature | 如 `voip-tracking` |
| 主要文件 | 路由/服务/存储/配置入口 |
| 默认状态 | 默认开启 / 默认关闭 / 历史兼容 |
| 建议动作 | 保留 / 合并 / 降级 / 删除 |
| 删除前置条件 | 测试、监控、依赖清点 |
| 风险等级 | 高 / 中 / 低 |
| 回滚补丁名 | 如 `0004-remove-matrixrtc.patch` |
| 验收结果 | build/test/CI/镜像结果 |

建议样例：

```text
模块名: matrixrtc_service
功能域: VoIP/RTC
对应 feature: voip-tracking
主要文件: src/services/matrixrtc_service.rs, src/storage/matrixrtc.rs
默认状态: 历史兼容扩展
建议动作: 从默认构建移出，二期评估删除
删除前置条件: 确认无核心路由引用，完成 rollback patch
风险等级: 低
回滚补丁名: 0004-remove-matrixrtc.patch
验收结果: pending
```

---

## 11. 分阶段执行计划

### Phase A：定义最小核心

目标：

- 明确默认 feature 集
- 固化核心模块白名单
- 建立删除黑名单与高风险边界

输出物：

- 核心模块矩阵
- feature 组合矩阵
- 核心测试清单

### Phase B：先降级，再删除

目标：

- 将非核心能力从默认构建移出
- 保留回滚补丁
- 跑全量回归

输出物：

- 删除补丁
- 回滚补丁
- CI 报告
- 镜像对比报告

### Phase C：收敛架构

目标：

- 合并重复服务
- 简化路由与依赖装配
- 统一配置与 feature 入口

输出物：

- 架构收敛补丁
- 更新后的依赖图
- 覆盖率与性能基线报告

### Phase D：最小化交付

目标：

- 形成默认最小镜像
- 更新部署文档
- 验证回滚流程

输出物：

- 最小 Docker 镜像
- 部署文档
- 回滚演练记录

### Phase P0：立即可执行清单

以下动作不涉及重写核心业务逻辑，适合作为第一批实施项：

1. 把方案文档、README、部署文档中的默认交付形态统一为 `core-private-chat`
2. 在 CI 中增加 `server,friends,burn-after-read` 的 build/test 矩阵
3. 对 `voice-extended`、`voip-tracking`、`widgets`、`server-notifications`、`external-services`、`openclaw-routes` 做引用清点
4. 生成第一版删除清单与回滚补丁目录
5. 建立三大核心能力的冒烟集成测试

### Phase P1：低风险收缩清单

以下模块可优先从默认形态中移出：

1. `voice_service`
2. `call_service`
3. `livekit_client`
4. `matrixrtc_service`
5. `widget_service`
6. `server_notification_service`
7. `external_service_integration`
8. `matrix_ai_connection_service`
9. `mcp_proxy`

### Phase P2：结构收敛清单

以下项属于有收益但需要更细致回归的架构整理：

1. 合并 `room_summary_service` 到 `room_service`
2. 合并 `space_service` 到 `room_service`
3. 收敛 `push_notification_service` 与 `push/*`
4. 合并 feature flag 三层结构
5. 收敛 `web/routes/handlers/*` 与 `web/routes/*`
6. 拆分 `services/container.rs` 为按域装配

### Phase P3：模块级实施产物

每完成一批实际删改，必须同步产出以下材料：

1. 模块删改清单
2. 依赖影响报告
3. 删除补丁与回滚补丁
4. 核心能力回归记录
5. 镜像体积对比记录
6. CI 运行截图或日志链接

### Phase P4：交付归档格式

为了让每轮精简都可追踪、可审计、可回放，建议每个批次固定输出以下目录结构：

```text
artifacts/slimming-batch-XX/
  module-ledger.csv
  dependency-impact.md
  apply.patch
  rollback.patch
  ci-summary.md
  coverage-summary.md
  image-size-report.md
  test-evidence/
```

对应归档要求：

1. `module-ledger.csv` 记录本批次涉及的所有模块
2. `dependency-impact.md` 说明删改影响面
3. `apply.patch` 与 `rollback.patch` 必须成对存在
4. `ci-summary.md` 记录 lint/test/security-scan 结果
5. `image-size-report.md` 记录镜像体积变化
6. `test-evidence/` 保留核心能力回归日志

## 14. 附录

### 14.1 推荐的实施负责人视角检查表

每个执行者在提交精简补丁前，至少应完成以下自查：

- 是否误触好友、E2EE、阅后即焚主链路
- 是否为每个删除动作准备了回滚补丁
- 是否更新了台账
- 是否补齐或复用了现有测试
- 是否验证了 `core-private-chat` 构建
- 是否验证了 `all-features` 构建
- 是否记录了镜像体积变化

### 14.2 推荐的评审者视角检查表

每个 reviewer 在审批前，至少应确认：

- 删除目标确实不属于三大核心白名单
- feature gate 边界清晰，没有把主链路逻辑藏进可选模块
- 回滚路径真实可执行
- 文档、CI、部署口径与代码变化一致
- 没有把“尚未完成的测试/镜像/覆盖率目标”写成已完成事实

---

## 12. 最终验收标准

只有同时满足以下条件，才能认为“精简重构完成”：

1. 三大核心能力全部可用：
   - 好友关系
   - 端到端私密聊天
   - 阅后即焚
2. 删除/降级模块均有影响评估与回滚补丁
3. 默认构建不再包含非核心实验能力
4. CI 全部通过：
   - lint
   - test
   - security-scan
5. 覆盖率不低于 **85%**
6. 最小镜像相对当前主镜像体积下降 **>= 30%**
7. 部署文档、回滚文档、最小 feature 文档全部更新

---

## 13. 结论

原方案最大的问题，是把“非标准 Matrix 能力”直接等同于“应删除能力”。这不适用于 `synapse-rust` 当前产品目标。

本次重构后的结论是：

- **好友关系管理不能删**，它是本项目核心产品能力。
- **端到端私密聊天不能删**，它是主价值主张。
- **阅后即焚不能删**，它是隐私能力闭环的一部分。
- 真正应该被收缩的，是 **VoIP、多余外部集成、实验性协议扩展、重复抽象层、遗留兼容层和重复文档**。

最终方向不是做一个“功能被删空的 Matrix 服务端”，而是做一个：

- **更像 Synapse 那样稳定可运营**
- **但保留本项目私密社交产品核心竞争力**
- **且默认更小、更清晰、更容易验证和回滚**

的 `synapse-rust` 最小核心发行版。

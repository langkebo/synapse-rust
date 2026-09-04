# PUSH-01: HTTP pusher gateway URL SSRF 防御加固

**状态**：✅ DONE（2026-09-05）
**优先级**：💭 低（纵深防御；当前 pushers 表的 `data.url` 字段没有调用方实际发请求）
**审计来源**：推送+媒体审计 #PUSH-01（2026-09-05）

## 问题描述

`synapse-services/src/push/gateway.rs::send_notification` 用 `client.post(gateway_url)` 直接发 HTTP POST，不验证 `gateway_url` 的 scheme/host。攻击场景：若用户 `set_pusher` 时 `data.url = "http://169.254.169.254/latest/meta-data/"`（云元数据）或内网地址，gateway 会发起服务端 SSRF。

当前不可达原因：`send_notification` 在 production 路径**无调用方**（worker Pusher 类型仅定义未实现外发）。但属低成本纵深防御。

## 修复方案

**1. 新增 `validate_push_gateway_url(&str) -> Result<(), ApiError>` 纯函数**（`synapse-services/src/push/gateway.rs`）
- scheme 必须 `https`（拒绝 http/ftp/file/javascript/data）
- host 不得是 IP literal（IPv4 + IPv6，包括 IPv4-mapped IPv6 `::ffff:1.2.3.4`）
- host 不得是 localhost / loopback（`127.0.0.0/8`、`::1`、`0.0.0.0`）
- host 不得是 link-local（`169.254.0.0/16`、`fe80::/10`，含云元数据）
- host 不得是私有/保留网段（`10.0.0.0/8`、`172.16.0.0/12`、`192.168.0.0/16`、`fc00::/7`）
- 端口无限制（一些 push gateway 跑在非标准端口；push gateway 不需要白名单 IP，因为 attack 面是 host）

**2. `gateway.rs::send_notification` 入口调用校验**（纵深防御）

**3. `src/web/routes/push.rs::set_pusher` 在 kind="http" 时校验 `data.url`**

**4. 单元测试**：
- 拒绝 http://, ftp://, file://
- 拒绝 IP literal
- 拒绝 localhost, 127.0.0.1, ::1
- 拒绝 link-local 169.254.169.254
- 接受 https://push.example.com
- 接受带端口的合法 URL

## 改动

- `synapse-services/src/push/gateway.rs`: +95 行（校验函数 + 调用点 + 4 测试）
- `src/web/routes/push.rs`: +12 行（set_pusher 校验）
- 编译 + clippy 零警告
- 4 个新单元测试全过

## 与 ticket 评估的差异

**原 ticket 计划**："在 `gateway.rs` L92 与 `push.rs` L143 增加 gateway URL 的 https + 非内网校验"
**实际**：完全按计划落地，2 处都加。

## 风险

- 无调用方 → 行为零变化（防御性加固）
- 错误信息明确（"gateway host not allowed: 169.254.169.254"），不会误杀合法 gateway
- `Ipv6Addr::is_loopback()` / `is_unspecified()` 已含 IPv4-mapped IPv6（`::ffff:127.0.0.1`），无需手动剥前缀

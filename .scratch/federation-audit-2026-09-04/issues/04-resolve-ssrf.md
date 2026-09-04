# F-04 🟡 P3 — FederationClient resolve_server IP 字面量绕过 SSRF 防护

**审计报告**：`/Users/ljf/Desktop/hu_ts/synapse-rust/.scratch/federation-audit-2026-09-04/report.md`

## 问题描述

`synapse-federation/src/client.rs:350-388` `resolve_server` 接受 `server_name:port` 格式（如 `192.168.1.1:8448`）作为 IP 字面量直接使用，**不经过 `check_url_and_resolve` 的 IP 黑名单**。

```rust
async fn resolve_server(&self, server_name: &str) -> Result<ResolvedServer, FederationClientError> {
    // ...
    // well-known fallback returns:
    ResolvedServer {
        server_name: server_name.to_string(),
        host: server_name.to_string(),  // <-- IP 字面量直接作为 host
        port: DEFAULT_FEDERATION_PORT,
    }
}
```

## 风险

攻击者注册 server_name `192.168.1.1:8448`（或通过 DNS 劫持），诱导 synapse-rust 向内部网地址发送联邦请求。

## 修复

在 `resolve_server` 中检测 IP 字面量格式，对 IPv4/IPv6/带端口格式调用 `synapse_common::security::check_ip_blacklist`：

```rust
let host = if is_ip_literal(server_name) {
    if !is_ip_allowed(server_name, &self.config.url_preview.ip_range_blacklist) {
        return Err(FederationClientError::ServerBlocked(server_name.to_string()));
    }
    server_name.to_string()
} else {
    // well-known 流程
};
```

## 验证

- 单元测试：构造 `server_name = "127.0.0.1"` → resolve_server 返回 403
- 单元测试：构造 `server_name = "10.0.0.1"` → resolve_server 返回 403
- 单元测试：构造 `server_name = "matrix.org"` → 正常 well-known 流程

## 优先级

🟡 P3（依赖 server_name 注入场景，目前攻击面有限）

## 工作量

0.25d

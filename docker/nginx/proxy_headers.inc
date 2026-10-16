# =============================================================================
# 转发头（W-D）：被 worker-routing.conf 的每个 `location` 引入
# =============================================================================
# 反代到 synapse-rust 时必须带上这些头，否则服务端看到的是"nginx 的地址"和
# "http"，于是：
#   * 生成的 `mxc://` / 登录回调 / 联邦签名里的 baseUrl 会指向代理而不是真实域名；
#   * 限流与 `user_ips` 记录会把所有请求算到同一个 IP 上（等于把按 IP 的限流变成全局限流）；
#   * 客户端拿到的 `.well-known` 与 `public_baseurl` 不一致，联邦对端会拒绝本仓的事件签名。
# 挂载方式见 docker/nginx/README.md（把本目录挂到 /etc/nginx/conf.d）。
# =============================================================================

proxy_http_version 1.1;
proxy_set_header Host $host;
proxy_set_header X-Real-IP $remote_addr;
proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
proxy_set_header X-Forwarded-Proto $scheme;
proxy_set_header X-Forwarded-Host $host;
proxy_set_header X-Forwarded-Port $server_port;

# 客户端连接复用（与 upstream 里的 `keepalive` 配套）。
proxy_set_header Connection "";

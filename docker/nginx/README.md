# Nginx 反向代理（参考配置）

本目录提供**可用的**反代参考写法。此前这里只有这份 README，而它描述的
`nginx.conf.template` 与 `docker-entrypoint.sh` **在仓库里并不存在** —— 也就是
「仓内没有任何 nginx conf」的状态（审计 W-8）。

## 文件

| 文件 | 作用 |
|------|------|
| `worker-routing.conf` | 可被 `http { include ...; }` 引入的完整片段：`upstream` + `server`。现启用的一路是 **master**；按路由分流到 worker 的写法作为**注释**保留（为什么，见下） |
| `proxy_headers.inc` | `X-Real-IP` / `X-Forwarded-*` / `Host` 等转发头，被上面每个 `location` 引入 |

## 为什么"分流到 worker"是注释掉的

本仓的 worker 进程（`synapse_worker`）**只提供 `/metrics`**，不提供 Matrix API
（`src/bin/synapse_worker.rs` 里只有一个 `/metrics` 路由）。并且没有任何生产代码按
`WorkerType::owned_route_prefixes` 决定哪个实例 serve 哪些路径 —— 全仓检索这些 API
只有拓扑校验器和 `/topology` 响应在用，`worker.worker_app` 更是从未被读取。

⇒ 把 `/_matrix/client/*/sync` 打到 `sync_worker` 今天只会得到 502，因为服务端那一半
不存在。要真做分流，需要先让实例按角色收敛自己的 HTTP 面（审计文档里的 **W-F**）。

`worker-routing.conf` 里保留注释版的原因有两个：它是目标形态的记录；它让"路由归属模型
超前于运行时"这一差距**在看配置文件时就可见**，而不是只写在文档里。

## 用法

```bash
# 校验配置（本目录挂到 nginx 的 conf.d，include 路径才对得上）
docker run --rm -v "$PWD/docker/nginx:/etc/nginx/conf.d:ro" nginx:alpine \
    sh -c 'printf "events {}\nhttp { include /etc/nginx/conf.d/worker-routing.conf; }\n" > /tmp/nginx.conf && nginx -t -c /tmp/nginx.conf'
```

在 compose 里挂载：

```yaml
nginx:
  image: nginx:alpine
  volumes:
    - ./docker/nginx/worker-routing.conf:/etc/nginx/conf.d/worker-routing.conf:ro
    - ./docker/nginx/proxy_headers.inc:/etc/nginx/conf.d/proxy_headers.inc:ro
    # 镜像自带的 default.conf 会抢占 80 端口，需要覆盖或删掉
    - ./docker/nginx/worker-routing.conf:/etc/nginx/conf.d/default.conf:ro
```

⚠️ `upstream` 里的容器名（`synapse-rust`）在 **nginx 启动时**就要解析成功：解析不到会让
nginx **起不来**（不是请求时才 502）。所以 worker 的 upstream 不能"先写上、以后再启用"。

## 与多进程栈的关系

`docker/docker-compose.workers.yml` 起来的 worker 进程**不经过** nginx —— 它们消费 Redis
队列、向 master 注册与心跳，只从 `/metrics` 被 Prometheus 抓。nginx 只负责把 Matrix API
交给 master。端到端验证见 `scripts/test/worker_topology_smoke.sh`。

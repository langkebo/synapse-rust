# Prometheus 抓取 / Web 认证凭证

本目录存放 **部署期生成** 的凭证文件。除本 README 外，目录内容全部被
`.gitignore` 忽略（`docker/deploy/prometheus/auth/*`），**任何情况下都不要提交**。

## 文件说明

| 文件 | 用途 | 挂载点 |
|---|---|---|
| `worker-token` | Prometheus 抓取 synapse worker 指标时使用的 bearer token，以单行纯文本存放 | `/etc/prometheus/worker-token` |
| `basic_auth` | Prometheus Web UI 的 htpasswd 条目（`<user>:<apr1-hash>`） | 预留，当前未挂载 |

## 生成方式

`worker-token`：任意高熵随机串，单行、无换行符之外的空白。

```bash
openssl rand -hex 32 > worker-token && chmod 600 worker-token
```

`basic_auth`：**必须**先生成哈希再写入，不要把 `$(...)` 原样写进文件。

```bash
printf 'admin:%s\n' "$(openssl passwd -apr1 "$PROMETHEUS_ADMIN_PASSWORD")" > basic_auth
chmod 600 basic_auth
```

> ⚠️ 历史坑：本目录曾出现一份内容是字面量
> `admin:$(openssl passwd -apr1 'test1234')` 的 `basic_auth` —— 命令替换从未被
> shell 展开，文件本身不可用于认证，同时又把口令明文写进了仓库工作区。
> 生成后请用 `grep -c '\$(' basic_auth` 自检，结果必须为 `0`。

## 与 compose 的关系

`prometheus.yml` 里 `synapse-rust` 抓取任务直连 `synapse-app:9090`（网络内回环），
**不需要** `basic_auth`；`worker-token` 由 worker 抓取任务使用。目前仅
`worker-token` 被实际挂载进 `synapse-prometheus`。

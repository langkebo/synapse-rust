# synapse-rust Worker Topology Baseline

> 版本: v0.2
> 日期: 2026-06-14
> 最后更新: P1-12 topology validator 落地 + P1-13 运维文档基线对齐
> 对应代码:
> - `src/worker/types.rs`
> - `src/worker/topology_validator.rs`（新增）
> - `src/web/routes/worker.rs`
> - `/_synapse/worker/v1/topology`
> - `scripts/deployment_smoke_test.sh`（新增）

---

## 一、目标

本文档把当前仓库里已经落地的 worker 运行时基线显式化，作为 `P1-12` 的最小可运营模型文档起点。

当前目标不是声明“已达到 Synapse workers.md 的完整能力”，而是把以下信息从隐式代码知识提升为可审阅、可调用、可继续扩展的基线：

- `instance_map_keys`
- `responsibility_domains`
- `owned_route_prefixes`
- `replication_streams`
- `deployment_presets`

---

## 二、当前运行时出口

admin worker API 已新增：

- `GET /_synapse/worker/v1/topology`

该接口返回静态 topology baseline，便于：

- 管理端查看当前预设 worker 类型矩阵
- 后续补部署文档时复用同一份事实来源
- 后续增加 topology validator / smoke test 时直接消费统一数据结构

---

## 三、Worker 类型基线

| WorkerType | instance_map_keys | responsibility_domains | owned_route_prefixes | replication_streams |
|---|---|---|---|---|
| `master` | `master` | `client_http`, `federation`, `event_persistence`, `background_jobs`, `media`, `push` | `/_matrix/client/*`, `/_matrix/federation/*`, `/_matrix/media/*`, `/_synapse/admin/*`, `/_synapse/worker/*` | `events`, `worker_commands`, `worker_tasks` |
| `frontend` | `client_reader` | `client_http` | `/_matrix/client/*` | 无 |
| `background` | `background_worker` | `background_jobs` | `/_synapse/worker/*` | `worker_commands`, `worker_tasks` |
| `event_persister` | `event_persister` | `event_persistence` | `/_synapse/worker/v1/replication/*` | `events` |
| `synchrotron` | `sync_worker` | `sync_http` | `/_matrix/client/*/sync`, `/_matrix/client/v3/sync` | `events` |
| `federation_sender` | `federation_sender` | `federation_egress` | 无 | `events` |
| `federation_reader` | `federation_reader` | `federation_ingress` | `/_matrix/federation/*` | `events` |
| `media_repository` | `media_repository` | `media_http` | `/_matrix/media/*` | 无 |
| `pusher` | `pusher` | `push_delivery` | 无 | `worker_tasks` |
| `appservice` | `appservice_worker` | `appservice_dispatch` | `/_matrix/app/*` | `worker_tasks` |

说明：

- `owned_route_prefixes` 当前是"归属声明基线"，启动期 topology validator（`src/worker/topology_validator.rs`）会校验 instance_map_keys 唯一性与 Master 存在性，但 route prefix 冲突目前仅做 warning 级别报告（不阻塞启动）。
- `replication_streams` 当前是“最小预期消费/写入流”基线，还不是完整的 HTTP replication listener 拓扑实现。
- `instance_map_keys` 当前用于对齐 Synapse 风格配置心智模型，后续仍需把真实配置文件、反向代理和 listener 绑定起来。

---

## 四、Deployment Presets

### 4.1 `monolith`

适用场景：

- 单机部署
- 功能验证
- 不拆分 client / federation / media / background 职责

实例：

- `master x1`

特点：

- 所有能力由主进程承担
- 运维最简单
- 不具备按职责横向扩展能力

### 4.2 `split_minimal`

适用场景：

- 小规模多 worker 部署
- 先拆分最有价值的热点流量
- 为后续 topology validator / smoke test 提供固定样板

实例：

- `master x1`
- `client_reader x2`
- `sync_worker x1`
- `event_persister x1`
- `federation_reader x1`
- `federation_sender x1`
- `media_repository x1`
- `background_worker x1`
- `pusher x1`

特点：

- client ingress、sync、event persistence、federation、media、push、background 已具备职责拆分基线
- 当前代码已具备 `WorkerRuntimeConfig` 所需的 `worker_type`、`host`、`port`、`replication_host`、`replication_port`、`http_port` 等字段，可作为 listener 规划出口
- 仍未完成真实反向代理路由样例、replication listener 安全封装与多实例 smoke test

---

## 五、`split_minimal` Listener 规划样例

下表不是“今天已经由启动器强校验的真实拓扑”，而是基于现有 `WorkerRuntimeConfig` 字段和 `WorkerType` 责任矩阵给出的最小部署样板，供后续 deployment validator / docker compose / 运维手册复用。

| instance_name | worker_type | http listener | replication listener | 对外暴露 |
|---|---|---|---|---|
| `master` | `master` | `127.0.0.1:8008` | `127.0.0.1:9101` | 否，仅反向代理回源和 worker 内网 |
| `client_reader-1` | `frontend` | `127.0.0.1:8101` | 无 | 否，经反向代理暴露 |
| `client_reader-2` | `frontend` | `127.0.0.1:8102` | 无 | 否，经反向代理暴露 |
| `sync_worker` | `synchrotron` | `127.0.0.1:8103` | 无 | 否，经反向代理暴露 |
| `event_persister` | `event_persister` | 无 | `127.0.0.1:9102` | 否，仅内网 |
| `federation_reader` | `federation_reader` | `127.0.0.1:8449` | 无 | 是，建议由 8448 反向代理回源 |
| `federation_sender` | `federation_sender` | 无 | `127.0.0.1:9103` | 否，仅内网 |
| `media_repository` | `media_repository` | `127.0.0.1:8104` | 无 | 否，经反向代理暴露 |
| `background_worker` | `background` | `127.0.0.1:8105` | `127.0.0.1:9104` | 否，仅内网 |
| `pusher` | `pusher` | 无 | `127.0.0.1:9105` | 否，仅内网 |

说明：

- `/_matrix/client/*` 由 `client_reader-*` 与 `sync_worker` 分担，`/sync` 应优先打到 `sync_worker`。
- `/_matrix/federation/*` 由 `federation_reader` 对外承接，再由其与内网 worker 完成复制/协调。
- `/_matrix/media/*` 建议独立回源到 `media_repository`。
- `/_synapse/worker/v1/replication/*` 只应在内网开放给具备 replication 关系的 worker。

---

## 六、反向代理样例

以下为 `split_minimal` 的 Nginx 样例，用来说明 route ownership 如何落到反向代理，不应直接视为仓库现成可用配置。

```nginx
upstream synapse_client_readers {
    server 127.0.0.1:8101;
    server 127.0.0.1:8102;
}

server {
    listen 443 ssl http2;
    server_name matrix.example.com;

    location = /_matrix/client/v3/sync {
        proxy_pass http://127.0.0.1:8103;
    }

    location ^~ /_matrix/client/ {
        proxy_pass http://synapse_client_readers;
    }

    location ^~ /_matrix/media/ {
        proxy_pass http://127.0.0.1:8104;
    }

    location ^~ /_matrix/federation/ {
        proxy_pass http://127.0.0.1:8449;
    }

    location ^~ /_synapse/admin/ {
        proxy_pass http://127.0.0.1:8008;
        allow 127.0.0.1;
        deny all;
    }
}
```

最小路由原则：

- `/sync` 单独路由到 `sync_worker`
- 一般 client API 路由到 `client_reader`
- federation 入口单独回源到 `federation_reader`
- media 入口单独回源到 `media_repository`
- admin 入口只回源 `master`，且不应暴露到公网

---

## 七、Smoke Test 基线

当前仓库已提供 `scripts/deployment_smoke_test.sh`，并支持通过以下环境变量执行带鉴权的 smoke test：

- `ADMIN_ENDPOINT`
- `REPLICATION_ENDPOINT`
- `ADMIN_AUTH_HEADER="Authorization: Bearer <admin_access_token>"`
- `REPLICATION_SECRET="<worker_replication_secret>"`

当前脚本已覆盖以下检查：

| 检查项 | 目标 | 通过标准 |
|---|---|---|
| topology API | `GET /_synapse/worker/v1/topology` | 返回基线拓扑 JSON，且 worker 类型矩阵完整 |
| worker register/heartbeat | worker 管理面 | smoke worker 可注册，heartbeat 后 `GET /workers/{id}` 可见 `status=running` 且 `last_heartbeat_ts` 已推进 |
| client route ownership | 反向代理分流 | `/sync` 命中 `sync_worker`，普通 client API 命中 `client_reader` |
| media route ownership | 反向代理分流 | `/_matrix/media/*` 命中 `media_repository` |
| federation route ownership | 反向代理分流 | `/_matrix/federation/*` 命中 `federation_reader` |
| replication protection | 内网安全边界 | 公网入口不可直接访问 `/_synapse/worker/v1/replication/*` |
| replication position | worker 协调 | smoke worker 的 stream position 可写入并回读一致 |
| task claim | background/pusher | smoke task 同时覆盖显式 claim、`claim_next_task` 原子领取与 `fail_task` 不回队列语义；复抢返回冲突或空队列；claim 后从 pending 列表消失，随后可完成或失败收口 |

当前仍未自动覆盖的检查：

- `/sync` / media / federation 的真实反向代理命中验证
- 多 worker 高并发 backlog / 恢复场景下的 claim 公平性与恢复观测
- topology API 输出与实际 listener 绑定的一致性比对

建议最小执行顺序：

1. 启动 `split_minimal` 预设中的全部 worker
2. 通过 admin API 校验 worker 注册与 topology 输出
3. 执行 `ADMIN_AUTH_HEADER=... REPLICATION_SECRET=... bash scripts/deployment_smoke_test.sh`
4. 通过反向代理访问 `/sync`、普通 client API、media API、federation API
5. 验证 replication 路径仅在内网可达

---

## 八、安全边界

参考上游 Synapse `workers.md` / research 结论，当前文档明确保留以下边界：

- replication listener 默认不应暴露到公网
- 未启用共享密钥或等效认证前，不应把 worker body 路径视为可信外部接口
- `/_synapse/worker/v1/topology` 仅是 admin 可见的静态规划接口，不代表所有权已经在启动期被强校验

---

## 九、当前缺口

相对上游 Synapse worker 模型，当前仍缺：

- 真实 `instance_map -> listener -> reverse proxy` 闭环
- ~~启动期 topology validator~~ ✅ 已落地（`src/worker/topology_validator.rs`），启动时校验 instance_map_keys 唯一性、Master 必须存在、route prefix 冲突
- route owner / stream writer / background owner 的强校验
- ~~多实例状态同步 smoke test~~ ✅ 已落地首版（heartbeat / replication position / task claim）
- 反向代理样例配置
- 运维手册与故障定位手册

---

## 十、下一步

按优先级建议继续：

1. 把本文的 listener / reverse proxy / smoke test 样例转成可执行部署工件
2. ~~增加 topology validator，启动时校验 route owner / stream writer / background owner~~ ✅ 已落地（P1-12）
3. ~~增加 deployment smoke test，验证多实例下 heartbeat、replication position、task claim、topology API 一致性~~ ✅ 已落地首版
4. 继续补强 route ownership 与多 worker 并发恢复类 smoke test

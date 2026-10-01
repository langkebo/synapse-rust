# 🚀 监控栈快速启动指南（deploy 栈）

> 本文档对应**线上实际运行的监控栈**：
> `docker/deploy/docker-compose.monitoring.yml`，compose project = `synapse-monitoring`。
>
> ⚠️ **不要**使用 `scripts/deploy-monitoring.sh`，也不要使用
> `docker/docker-compose.monitoring.yml` —— 那属于另一套 `synapse-test` 栈
> （目录 `docker/`），它会抢占线上已占用的 8008 / 80 / 443 / 9090 / 9100 端口。

---

## 📍 部署目录

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy
```

---

## ⚡ 启动 / 停止

**正常路径**：由 `docker/deploy/deploy.sh` 的最后一步「启动监控栈」自动调用，一般不需要手动执行。

**手动启动**（必须在核心栈之后，因为监控栈复用核心栈创建的网络 `synapse_network`，声明为 `external`）：

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy && \
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml up -d
```

**停止**：

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy && \
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml down
```

**重启单个服务**：

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy && \
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml restart synapse-prometheus
```

> `-p synapse-monitoring` 不能省：本文件由 deploy.sh 以该 project 名启动。
> 若用目录默认 project（`deploy`）启动，会另起一套同名容器而冲突。

---

## 🔐 访问地址与登录信息

| 服务 | 地址 | 凭据 |
|------|------|------|
| **Grafana** | http://localhost:3000 | `admin` / `admin123` |
| **Prometheus** | http://localhost:9092 | 无 |
| **Alertmanager** | http://localhost:9093 | 无 |
| **node-exporter** | http://localhost:9100 | 无 |
| **alert-handler** | http://localhost:8080 | 无 |
| nginx-proxy（HTTPS 反代） | https://localhost:8443 | basic auth（`docker/deploy/nginx/auth/.htpasswd`） |

宿主端口映射（可在 `docker/deploy/.env` 覆盖）：

| 变量 | 默认值 | 说明 |
|------|--------|------|
| `GRAFANA_PORT` | `3000` | Grafana |
| `PROMETHEUS_UI_PORT` | `9092` | Prometheus UI。**不能**复用 `PROMETHEUS_PORT`，后者是 synapse-app 自身 `/metrics` 的端口 |
| `ALERTMANAGER_PORT` | `9093` | Alertmanager |
| `NODE_EXPORTER_PORT` | `9100` | node-exporter |
| `ALERT_HANDLER_PORT` | `8080` | alert-handler |

> **为什么 Prometheus 是 9092 而不是 9090**：宿主 9090 已被 `synapse-app`
> 自身的 `/metrics` 占用（`homeserver.yaml` 的 `prometheus.port = 9090`），
> 复用会让监控栈报 `Bind for 127.0.0.1:9090 failed: port is already allocated`。

### 界面语言

Grafana 默认界面语言为**中文（简体）**，配置在
`docker/deploy/grafana/grafana.ini`：

```ini
[users]
default_language = zh-Hans
```

> 必须是 `zh-Hans`。Grafana 10.4.2 镜像内置的中文 locale 目录是
> `/usr/share/grafana/public/locales/zh-Hans`，**没有 `zh-cn`**；
> 写 `zh-cn` 会被静默忽略并回落英文。该文件里也**不存在** `[locales]` 段。

`default_language` 作用于**未单独设置过语言偏好**的用户。若某个用户在
「用户设置 → 偏好设置」里显式选过语言，则以该用户的选择为准。

### 修改 Grafana 管理员密码

```bash
docker exec synapse-grafana grafana-cli admin reset-admin-password '<新密码>'
```

---

## 📊 已自动加载的仪表盘

由 `docker/deploy/grafana/provisioning/dashboards/dashboards.yml` 自动发现，
源文件目录 `docker/deploy/grafana/dashboards/`，在 Grafana 中的文件夹名为 `synapse-rust`：

| 仪表盘文件 | 内容 |
|------------|------|
| `system-overview.json` | 系统总览 |
| `backend-services.json` | 后端服务 |
| `storage-performance.json` | 存储性能 |
| `network-connections.json` | 网络连接 |
| `security-auth.json` | 安全与认证 |
| `alert-manager.json` | 告警管理 |

数据源已由 `grafana/provisioning/datasources/datasources.yml` 预置：
`Prometheus` → `http://synapse-prometheus:9090`（容器内 DNS 名与端口，注意不是宿主 9092）。

---

## 🎯 没有真实用户？如何生成模拟数据

### 步骤 1：生成模拟数据

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust && \
python3 scripts/mock-data/generate_mock_data.py
```

输出文件：`scripts/mock-data/mock_metrics/mock_synapse_metrics.prom`

### 步骤 2：让 Prometheus 抓取它

在**线上实际使用的**配置文件 `docker/deploy/prometheus/prometheus.yml`
的 `scrape_configs:` 下追加：

```yaml
  - job_name: "mock-synapse"
    file_sd_configs:
      - files:
          - '/Users/ljf/Desktop/hu_ts/synapse-rust/scripts/mock-data/mock_metrics/*.prom'
        refresh_interval: 30s
    relabel_configs:
      - target_label: instance
        replacement: "mock-synapse-dev"
```

然后重载：

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy && \
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml restart synapse-prometheus
```

> ⚠️ **不要**把 `monitoring/prometheus/prometheus-with-mock.yml` 整份覆盖到
> `docker/deploy/prometheus/prometheus.yml`。那份文件里的抓取目标是
> `synapse-rust:8008` / `db:9187` / `redis:9121` / `node-exporter:9100` /
> `grafana:3000`，属于 **`synapse-test` 那套栈**的服务名；在 deploy 栈的网络上
> 这些名字都解析不到（deploy 栈是 `synapse-app` / `synapse-node-exporter` …），
> 覆盖后会让所有真实抓取任务全部失败。
> 另外它的文件通配是 `scripts/mock-data/*.prom`，也漏了一层 `mock_metrics/`。

### 步骤 3：验证数据

```bash
curl -sG http://localhost:9092/api/v1/query \
  --data-urlencode 'query=room_operations_total' | head -c 400
```

---

## 🔄 完整操作流程示例

```bash
# 1. 进入部署目录
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy

# 2. 核心栈 + 迁移 + 监控栈，一步到位（推荐）
bash ./deploy.sh --all

# 或者只重启监控栈
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml up -d

# 3. 打开 Grafana
open http://localhost:3000
```

---

## 🛠️ 常用命令

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy

# 查看监控栈状态
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml ps

# 查看日志
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml logs -f synapse-prometheus
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml logs -f synapse-grafana
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml logs -f synapse-alertmanager

# 重启单个服务
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml restart synapse-grafana

# 清理所有监控数据（慎用，会删 grafana/prometheus 数据卷）
docker compose -p synapse-monitoring -f docker-compose.monitoring.yml down -v
```

---

## 🆘 常见问题

### Q1: Grafana 打开显示 "Page not found"

服务端是正常的（`curl -o /dev/null -w '%{http_code}' http://localhost:3000/login` 返回 200），
绝大多数情况是**浏览器缓存 / Service Worker** 造成的。

判断依据：若 Grafana 容器日志里出现形如
`GET /@vite/client -> 404 referer=http://localhost:3000/login` 的记录，说明浏览器在
`localhost:3000` 这个 origin 上缓存了**此前跑在该端口的某个 Vite 应用**的页面。
Grafana 10.4.2 用 webpack（产物是 `/public/build/*.js`），**永远不会请求 `/@vite/client`**。

处理：

1. 在 `http://localhost:3000` 上做一次**硬刷新**（macOS: `Cmd+Shift+R`）；
2. 或开发者工具 → Application → Service Workers → `Unregister`，再 Clear storage；
3. 或换一个 origin 访问，如 http://127.0.0.1:3000 。

### Q2: 登录报 "Invalid username or password"

默认账号是 `admin` / `admin123`。若已被改动，用 `grafana-cli admin reset-admin-password`
重置（见上文）。

### Q3: 启动报 `network synapse_network ... not found`

监控栈复用核心栈创建的网络，属于 `external`。先启动核心栈：

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy && \
docker compose -p synapse up -d
```

### Q4: 启动报 `Bind for 127.0.0.1:9090 failed: port is already allocated`

说明有服务去抢宿主 9090（已被 synapse-app 的 `/metrics` 占用）。
检查是否误用了 `PROMETHEUS_PORT`（应用自身端口）而不是 `PROMETHEUS_UI_PORT`（监控 UI 端口）。

### Q5: Grafana 界面还是英文

1. 确认 `docker/deploy/grafana/grafana.ini` 里是 `default_language = zh-Hans`（不是 `zh-cn`）；
2. 重启：`docker restart synapse-grafana`；
3. 你当前用户在「用户设置 → 偏好设置 → 语言」里显式选过英文的话，改回「默认」或直接选「中文（简体）」。

### Q6: 面板显示 "No data"

1. 确认 Prometheus 在跑：`docker ps | grep synapse-prometheus`
2. 检查抓取目标：http://localhost:9092/api/v1/targets
3. 数据源地址必须是容器内 DNS：`http://synapse-prometheus:9090`（不是宿主 9092）
4. 首次启动后需等待若干抓取周期才有足够历史数据

---

## 📞 相关文件

- 监控栈 compose：[docker/deploy/docker-compose.monitoring.yml](docker/deploy/docker-compose.monitoring.yml)
- Grafana 配置：[docker/deploy/grafana/grafana.ini](docker/deploy/grafana/grafana.ini)
- Prometheus 配置：[docker/deploy/prometheus/prometheus.yml](docker/deploy/prometheus/prometheus.yml)
- 部署脚本：[docker/deploy/deploy.sh](docker/deploy/deploy.sh)
- Mock 数据生成器：[scripts/mock-data/generate_mock_data.py](scripts/mock-data/generate_mock_data.py)

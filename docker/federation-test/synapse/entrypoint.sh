#!/usr/bin/env bash
# =============================================================================
# docker/federation-test/synapse/entrypoint.sh
# =============================================================================
# 真 Synapse（上游参考实现）侧的配置生成入口。
#
# 官方镜像 ghcr.io/element-hq/synapse 的 ENTRYPOINT 是 /start.py：它会读取
# SYNAPSE_CONFIG_PATH（默认 /data/homeserver.yaml），配置缺失时按环境变量生成，
# 然后执行 `python -m synapse.app.homeserver --config-path ...` 完成 schema 迁移
# 与启动。
#
# 本脚本在 B 栈里取代该 ENTRYPOINT 的「生成配置」部分：从环境变量写出一份确定
# 性的 homeserver.yaml（指向本栈的 postgres、信任测试自签 CA、关闭注册），随后
# 把控制权交回 /start.py，避免重复实现迁移/启动逻辑。
#
# 必需环境变量（来自 .env.b）：
#   SERVER_NAME, DB_USER, DB_PASSWORD, DB_NAME, MACAROON_SECRET, FORM_SECRET,
#   REGISTRATION_SECRET
# 可选：
#   PUBLIC_BASEURL, DB_HOST(默认 db), DB_PORT(默认 5432), DATA_DIR(默认 /data)
# =============================================================================

set -euo pipefail

DATA_DIR="${DATA_DIR:-/data}"
SERVER_NAME="${SERVER_NAME:?SERVER_NAME must be set}"
PUBLIC_BASEURL="${PUBLIC_BASEURL:-}"
DB_HOST="${DB_HOST:-db}"
DB_PORT="${DB_PORT:-5432}"
DB_USER="${DB_USER:?DB_USER must be set}"
DB_PASSWORD="${DB_PASSWORD:?DB_PASSWORD must be set}"
DB_NAME="${DB_NAME:?DB_NAME must be set}"
MACAROON_SECRET="${MACAROON_SECRET:?MACAROON_SECRET must be set}"
FORM_SECRET="${FORM_SECRET:?FORM_SECRET must be set}"
REGISTRATION_SECRET="${REGISTRATION_SECRET:?REGISTRATION_SECRET must be set}"

CONFIG_FILE="${DATA_DIR}/homeserver.yaml"
SIGNING_KEY_PATH="${DATA_DIR}/signing.key"
LOG_CONFIG="${DATA_DIR}/log.config"
MEDIA_STORE="${DATA_DIR}/media_store"

mkdir -p "${DATA_DIR}" "${MEDIA_STORE}"

# ── Signing key（仅首次生成，保持服务器身份稳定）────────────────────────────
# 格式：`ed25519 <key_id> <base64(32B seed)>`
if [[ ! -f "${SIGNING_KEY_PATH}" ]]; then
    KEY_B64="$(head -c 32 /dev/urandom | base64 | tr -d '\n')"
    printf 'ed25519 b0 %s\n' "${KEY_B64}" >"${SIGNING_KEY_PATH}"
    chmod 600 "${SIGNING_KEY_PATH}"
    echo "[synapse-b] generated signing key at ${SIGNING_KEY_PATH}"
fi

# ── Log config（最小可用的 stdlib logging 配置）─────────────────────────────
cat >"${LOG_CONFIG}" <<'YAML'
version: 1
formatters:
  precise:
    format: '%(asctime)s - %(name)s - %(lineno)d - %(levelname)s - %(message)s'
handlers:
  console:
    class: logging.StreamHandler
    formatter: precise
root:
  level: INFO
  handlers: [console]
disable_existing_loggers: false
YAML

# ── homeserver.yaml ────────────────────────────────────────────────────────
cat >"${CONFIG_FILE}" <<YAML
server_name: "${SERVER_NAME}"
public_baseurl: "${PUBLIC_BASEURL}"
pid_file: ${DATA_DIR}/homeserver.pid
web_client: false

# 信任测试自签 CA，否则出站联邦 TLS 握手会因证书链校验失败。
federation_custom_ca_list:
  - /certs/ca.crt

listeners:
  # 客户端 + 联邦（明文，TLS 由 nginx 边车终结）
  - port: 8008
    type: http
    tls: false
    x_forwarded: true
    bind_addresses: ['0.0.0.0']
    resources:
      - names: [client, federation]
        compress: false
  # 联邦专用（供 nginx 边车回源）
  - port: 8448
    type: http
    tls: false
    x_forwarded: true
    bind_addresses: ['0.0.0.0']
    resources:
      - names: [federation]
        compress: false

database:
  name: psycopg2
  args:
    user: "${DB_USER}"
    password: "${DB_PASSWORD}"
    database: "${DB_NAME}"
    host: "${DB_HOST}"
    port: ${DB_PORT}
    cp_min: 5
    cp_max: 10

log_config: "${LOG_CONFIG}"
media_store_path: "${MEDIA_STORE}"
signing_key_path: "${SIGNING_KEY_PATH}"
registration_shared_secret: "${REGISTRATION_SECRET}"
macaroon_secret_key: "${MACAROON_SECRET}"
form_secret: "${FORM_SECRET}"
report_stats: false
enable_registration: false

# 隔离测试环境无外网：不配置受信密钥服务器，避免启动/抓取阶段阻塞。
trusted_key_servers: []
suppress_key_server_warning: true

# 两个实例都在 Docker 私网（172.16/12），而 Synapse 默认的
# federation_ip_range_blacklist 会屏蔽：
#   * 出站联邦（B→A：连接私网对端被拒），以及
#   * 入站联邦（A→B：listener 开了 x_forwarded，Synapse 用 X-Forwarded-For
#     里的 nginx 私网地址做黑名单判断而被拒）。
# 测试环境必须清空该名单，否则双向联邦在私网下全部失败。
federation_ip_range_blacklist: []
YAML

echo "[synapse-b] generated ${CONFIG_FILE} for ${SERVER_NAME}"

# 交回官方入口：运行 schema 迁移并启动 homeserver。
exec /start.py

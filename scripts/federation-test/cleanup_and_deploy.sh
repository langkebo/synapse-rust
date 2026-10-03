#!/usr/bin/env bash
#
# scripts/federation-test/cleanup_and_deploy.sh
#
# 联邦互操作性测试：清理 + 部署脚本
#
# 功能:
#   1. 停止所有 synapse 相关容器
#   2. 删除旧的镜像和缓存
#   3. 清理数据卷
#   4. 构建新镜像
#   5. 启动双实例 (synapse-a 和 synapse-b)
#
# 用法:
#   bash scripts/federation-test/cleanup_and_deploy.sh [--all]
#   bash scripts/federation-test/cleanup_and_deploy.sh --only-a
#   bash scripts/federation-test/cleanup_and_deploy.sh --only-b
#

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DOCKER_DIR="$ROOT_DIR/docker"
FEDERATION_TEST_DIR="$DOCKER_DIR/federation-test"

COLOR_RED='\033[0;31m'
COLOR_GREEN='\033[0;32m'
COLOR_YELLOW='\033[1;33m'
COLOR_BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    printf "${COLOR_BLUE}[INFO]${NC} %s\n" "$*"
}

warn() {
    printf "${COLOR_YELLOW}[WARN]${NC} %s\n" "$*"
}

success() {
    printf "${COLOR_GREEN}[✓]${NC} %s\n" "$*"
}

error() {
    printf "${COLOR_RED}[✗]${NC} %s\n" "$*" >&2
    exit 1
}

usage() {
    cat <<EOF
Usage: $0 [OPTIONS]

Options:
  --all         Stop and clean both instances (default)
  --only-a      Only handle synapse-a instance
  --only-b      Only handle synapse-b instance
  --dry-run     Show what would be done without doing it
  -h, --help    Show this help message

Examples:
  $0 --all              Clean and redeploy both instances
  $0 --only-a           Only rebuild synapse-a
  $0 --dry-run          Preview cleanup actions

EOF
}

# Parse arguments
CLEAN_A=true
CLEAN_B=true
DRY_RUN=false

while [[ $# -gt 0 ]]; do
    case "$1" in
        --all)
            CLEAN_A=true
            CLEAN_B=true
            shift
            ;;
        --only-a)
            CLEAN_A=true
            CLEAN_B=false
            shift
            ;;
        --only-b)
            CLEAN_A=false
            CLEAN_B=true
            shift
            ;;
        --dry-run)
            DRY_RUN=true
            shift
            ;;
        -h | --help)
            usage
            exit 0
            ;;
        *)
            error "Unknown option: $1"
            ;;
    esac
done

echo "=========================================="
echo "  Synapse Federation Test Cleanup & Deploy"
echo "=========================================="
echo ""

if [[ "$DRY_RUN" == "true" ]]; then
    warn "Dry-run mode: showing planned actions only"
    echo ""
fi

# ─────────────────────────────────────────────────────────────────────────────
# Step 1: Stop all running containers
# ─────────────────────────────────────────────────────────────────────────────
log "Step 1: Stopping existing containers..."

stop_containers() {
    local name_prefix="$1"
    local compose_file="$2"

    if [[ "$DRY_RUN" == "true" ]]; then
        log "[DRY-RUN] Would run: cd $(dirname "$compose_file") && docker compose -f $(basename "$compose_file") down -v"
        return
    fi

    cd "$(dirname "$compose_file")"
    if docker compose -f "$(basename "$compose_file")" ps 2>/dev/null | grep -q "$name_prefix"; then
        log "Stopping $(basename "$compose_file") stack..."
        docker compose -f "$(basename "$compose_file")" down -v --remove-orphans || true
        success "Stopped $name_prefix containers"
    else
        log "No running containers found for $name_prefix"
    fi
}

if [[ "$CLEAN_A" == "true" ]]; then
    stop_containers "synapse-federation-a" "$FEDERATION_TEST_DIR/docker-compose-synapse-a.yml"
fi

if [[ "$CLEAN_B" == "true" ]]; then
    stop_containers "synapse-federation-b" "$FEDERATION_TEST_DIR/docker-compose-synapse-b.yml"
fi

# Also stop any legacy synapse containers
log "Checking for legacy synapse containers..."
if [[ "$DRY_RUN" == "true" ]]; then
    log "[DRY-RUN] Would check for legacy containers starting with 'synapse'"
else
    legacy_count=$(docker ps -a --filter "name=synapse" --format "{{.Names}}" | wc -l | tr -d ' ')
    if [[ "$legacy_count" -gt 0 ]]; then
        warn "Found $legacy_count legacy synapse containers. Consider cleaning them manually:"
        docker ps -a --filter "name=synapse" --format "{{.Names}}"
        warn "Run: docker rm -f $(docker ps -a --filter "name=synapse" --format "{{.Names}}" | tr '\n' ' ')"
    else
        success "No legacy synapse containers found"
    fi
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 2: Remove old images and build cache
# ─────────────────────────────────────────────────────────────────────────────
log "Step 2: Cleaning up old images..."

if [[ "$DRY_RUN" == "true" ]]; then
    log "[DRY-RUN] Would remove image: synapse-rust:latest"
    log "[DRY-RUN] Would prune unused images"
else
    # Remove specific image
    if docker images synapse-rust --format "{{.Repository}}:{{.Tag}}" | grep -q "latest"; then
        log "Removing synapse-rust:latest image..."
        docker rmi synapse-rust:latest --force || true
        success "Removed synapse-rust:latest"
    fi

    # Prune unused images (optional, can be slow)
    warn "Pruning unused Docker images (this may take a while)..."
    docker image prune -f --filter "until=24h" || true
    success "Pruned unused images"
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 3: Clean data directories
# ─────────────────────────────────────────────────────────────────────────────
log "Step 3: Cleaning data directories..."

DATA_A="$FEDERATION_TEST_DIR/../data/a"
DATA_B="$FEDERATION_TEST_DIR/../data/b"

if [[ "$DRY_RUN" == "true" ]]; then
    log "[DRY-RUN] Would remove data directories:"
    [[ "$CLEAN_A" == "true" ]] && log "[DRY-RUN]   - $DATA_A"
    [[ "$CLEAN_B" == "true" ]] && log "[DRY-RUN]   - $DATA_B"
else
    if [[ "$CLEAN_A" == "true" && -d "$DATA_A" ]]; then
        rm -rf "$DATA_A"
        mkdir -p "$DATA_A"
        success "Cleaned synapse-a data directory"
    fi

    if [[ "$CLEAN_B" == "true" && -d "$DATA_B" ]]; then
        rm -rf "$DATA_B"
        mkdir -p "$DATA_B"
        success "Cleaned synapse-b data directory"
    fi
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 4: Build new Docker images
# ─────────────────────────────────────────────────────────────────────────────
log "Step 4: Building new Docker images..."

if [[ "$DRY_RUN" == "true" ]]; then
    log "[DRY-RUN] Would run: cd $DOCKER_DIR && docker compose build"
else
    cd "$DOCKER_DIR"
    log "Building synapse-rust image (this may take 5-10 minutes)..."

    # Build with minimal features to speed up (can be adjusted)
    DOCKER_CARGO_FEATURE_ARGS="--features core-private-chat,widgets,external-services,voice-extended,cas-sso,saml-sso,friends --no-default-features" \
        docker compose build synapse-rust || {
        error "Failed to build Docker image. Check the build output above."
    }

    success "Docker image built successfully"
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 5: Generate federation signing keys
# ─────────────────────────────────────────────────────────────────────────────
log "Step 5: Generating federation signing keys..."

if [[ "$DRY_RUN" == "true" ]]; then
    log "[DRY-RUN] Would generate signing key for instance A (B uses Synapse-generated key)"
else
    # Instance A (synapse-rust) needs an explicit signing key in .env.a.
    # Instance B is real Synapse: it generates /data/signing.key itself at startup,
    # so no key material is injected here.
    if [[ "$CLEAN_A" == "true" ]]; then
        if ! command -v openssl &>/dev/null; then
            warn "openssl not found. Using placeholder key for instance A"
            FED_KEY_A="ed25519 placeholder_for_testing_only_do_not_use_in_production"
        else
            FED_KEY_A=$(openssl rand -base64 32 | tr -d '\n')
        fi
        log "Instance A federation key generated"
    fi
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 6: Prepare .env files
# ─────────────────────────────────────────────────────────────────────────────
log "Step 6: Preparing environment files..."

ENV_FILE_A="$FEDERATION_TEST_DIR/.env.a"
ENV_FILE_B="$FEDERATION_TEST_DIR/.env.b"

if [[ "$DRY_RUN" == "true" ]]; then
    log "[DRY-RUN] Would create $ENV_FILE_A and $ENV_FILE_B"
else
    # Create .env.a
    if [[ "$CLEAN_A" == "true" ]]; then
        cat >"$ENV_FILE_A" <<EOF
# Synapse-A Environment
COMPOSE_PROJECT_NAME=synapse-federation-test-a
SYNAPSE_IMAGE=synapse-rust
SYNAPSE_IMAGE_TAG=federation-a
SERVER_NAME=synapse-a.federation.test
PUBLIC_BASEURL=https://synapse-a.federation.test:18448
DB_USER=synapse
DB_PASSWORD=synapse_a_pwd
DB_NAME=synapse_a
REDIS_PASSWORD=redis_a_pwd
MACAROON_SECRET=$(openssl rand -hex 32)
FORM_SECRET=$(openssl rand -hex 16)
REGISTRATION_SECRET=$(openssl rand -hex 64)
ADMIN_SECRET=$(openssl rand -hex 32)
SECRET_KEY=$(openssl rand -hex 64)
FEDERATION_SIGNING_KEY=${FED_KEY_A:-placeholder_a}
FEDERATION_KEY_ID=ed25519:a
FEDERATION_MASTER_KEY=$(openssl rand -hex 32)
WORKER_REPLICATION_SECRET=$(openssl rand -hex 32)
TOKEN_HASH_SECRET=$(openssl rand -hex 32)
RUST_LOG=debug
TZ=UTC
EOF
        success "Created .env.a"
    fi

    # Create .env.b
    # B 端是上游真 Synapse：不再需要 rust 侧的大量密钥（TOKEN_HASH_SECRET /
    # FEDERATION_SIGNING_KEY / REDIS_PASSWORD 等），只需 DB、身份标识与 Synapse
    # 自身要求的三个 secret。签名字由 Synapse 于容器内自行生成。
    if [[ "$CLEAN_B" == "true" ]]; then
        cat >"$ENV_FILE_B" <<EOF
# Synapse-B Environment (upstream Synapse reference implementation)
COMPOSE_PROJECT_NAME=synapse-federation-test-b
SYNAPSE_UPSTREAM_IMAGE=ghcr.io/element-hq/synapse
SYNAPSE_UPSTREAM_TAG=v1.162.0
SERVER_NAME=synapse-b.federation.test
PUBLIC_BASEURL=https://synapse-b.federation.test:18449
DB_USER=synapse
DB_PASSWORD=synapse_b_pwd
DB_NAME=synapse_b
MACAROON_SECRET=$(openssl rand -hex 32)
FORM_SECRET=$(openssl rand -hex 16)
REGISTRATION_SECRET=$(openssl rand -hex 64)
TZ=UTC
EOF
        success "Created .env.b"
    fi
fi

echo ""

# ─────────────────────────────────────────────────────────────────────────────
# Step 7: Start both instances
# ─────────────────────────────────────────────────────────────────────────────
log "Step 7: Starting federation test instances..."

start_instance() {
    local name_prefix="$1"
    local compose_file="$2"
    local env_file="$3"
    local port="$4"
    local mode="${5:-build}"

    if [[ "$DRY_RUN" == "true" ]]; then
        log "[DRY-RUN] Would start ($mode): cd $(dirname "$compose_file") && docker compose --env-file $(basename "$env_file") -f $(basename "$compose_file") up -d"
        return
    fi

    cd "$(dirname "$compose_file")"
    log "Starting $name_prefix on port $port (mode: $mode)..."

    if [[ "$mode" == "pull" ]]; then
        # B 端是上游真 Synapse：拉取镜像而非本地构建。
        log "Pulling container image for $name_prefix..."
        docker compose --env-file "$(basename "$env_file")" -f "$(basename "$compose_file")" pull
    else
        # A 端是 synapse-rust：本地构建镜像。
        log "Building synapse-rust image for $name_prefix..."
        docker compose build
    fi

    # Start services
    docker compose --env-file "$(basename "$env_file")" -f "$(basename "$compose_file")" up -d --force-recreate

    # Wait for health check
    log "Waiting for $name_prefix to become healthy..."
    sleep 30

    # Check health
    if curl -s "http://localhost:$port/_matrix/client/versions" >/dev/null 2>&1; then
        success "$name_prefix is healthy on port $port"
        curl -s "http://localhost:$port/_matrix/client/versions" | jq '.' || true
    else
        warn "$name_prefix may not be ready yet. Check logs with: docker logs $name_prefix"
    fi
}

if [[ "$CLEAN_A" == "true" ]]; then
    start_instance "Synapse-A" "$FEDERATION_TEST_DIR/docker-compose-synapse-a.yml" "$ENV_FILE_A" "18008" "build"
fi

if [[ "$CLEAN_B" == "true" ]]; then
    start_instance "Synapse-B" "$FEDERATION_TEST_DIR/docker-compose-synapse-b.yml" "$ENV_FILE_B" "18009" "pull"
fi

echo ""
log "All services started!"
log ""
log "Test endpoints:"
[[ "$CLEAN_A" == "true" ]] && log "  Synapse-A: http://localhost:18008/_matrix/client/versions"
[[ "$CLEAN_B" == "true" ]] && log "  Synapse-B: http://localhost:18009/_matrix/client/versions"
log ""
log "Logs:"
[[ "$CLEAN_A" == "true" ]] && log "  docker logs synapse-federation-a"
[[ "$CLEAN_B" == "true" ]] && log "  docker logs synapse-federation-b"
log ""
log "To stop both instances:"
log "  bash $0 --all"
echo ""
success "Cleanup and deployment complete!"

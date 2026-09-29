#!/usr/bin/env bash
#
# scripts/federation-test/generate-tls-cert.sh
#
# 生成联邦测试所需的 TLS 证书
# 
# 用途:
#   - 为两个 Synapse 实例生成自签名证书
#   - 创建 CA 根证书
#   - 生成 hosts 配置
#
# 用法:
#   bash scripts/federation-test/generate-tls-cert.sh
#

set -eu

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CERT_DIR="$ROOT_DIR/docker/federation-test/certs"
HOSTS_FILE="$ROOT_DIR/docker/federation-test/hosts.conf"

mkdir -p "$CERT_DIR"

COLOR_GREEN='\033[0;32m'
COLOR_BLUE='\033[0;34m'
COLOR_YELLOW='\033[1;33m'
NC='\033[0m'

log() { printf "${COLOR_BLUE}[INFO]${NC} %s\n" "$*"; }
success() { printf "${COLOR_GREEN}[✓]${NC} %s\n" "$*"; }
warn() { printf "${COLOR_YELLOW}[WARN]${NC} %s\n" "$*"; }

# ─────────────────────────────────────────────────────────────────────────────
# Step 1: Generate CA Root Certificate
# ─────────────────────────────────────────────────────────────────────────────
log "Step 1: Generating CA root certificate..."

openssl req -x509 -newkey rsa:4096 -nodes \
    -keyout "$CERT_DIR/ca.key" \
    -out "$CERT_DIR/ca.crt" \
    -days 365 \
    -subj "/C=US/ST=Test/L=Test/O=SynapseFederationTest/CN=Synapse Federation Test CA" \
    -sha256 2>/dev/null

success "CA root certificate generated: $CERT_DIR/ca.crt"

# ─────────────────────────────────────────────────────────────────────────────
# Step 2: Generate Server Certificates
# ─────────────────────────────────────────────────────────────────────────────
log "Step 2: Generating server certificates..."

# Synapse-A certificate (localhost:18448)
openssl req -newkey rsa:4096 -nodes \
    -keyout "$CERT_DIR/synapse-a.key" \
    -out "$CERT_DIR/synapse-a.csr" \
    -subj "/C=US/ST=Test/L=Test/O=SynapseFederationTest/CN=synapse-a.federation.test" \
    -addext "subjectAltName=DNS:synapse-a.federation.test,DNS:synapse-a,DNS:localhost,IP:127.0.0.1" \
    2>/dev/null

openssl x509 -req -in "$CERT_DIR/synapse-a.csr" \
    -CA "$CERT_DIR/ca.crt" \
    -CAkey "$CERT_DIR/ca.key" \
    -CAcreateserial \
    -out "$CERT_DIR/synapse-a.crt" \
    -days 365 \
    -sha256 \
    -extfile <(printf "subjectAltName=DNS:synapse-a.federation.test,DNS:synapse-a,DNS:localhost,IP:127.0.0.1") \
    2>/dev/null

success "Synapse-A certificate generated: $CERT_DIR/synapse-a.crt"

# Synapse-B certificate (localhost:18449)
openssl req -newkey rsa:4096 -nodes \
    -keyout "$CERT_DIR/synapse-b.key" \
    -out "$CERT_DIR/synapse-b.csr" \
    -subj "/C=US/ST=Test/L=Test/O=SynapseFederationTest/CN=synapse-b.federation.test" \
    -addext "subjectAltName=DNS:synapse-b.federation.test,DNS:synapse-b,DNS:localhost,IP:127.0.0.1" \
    2>/dev/null

openssl x509 -req -in "$CERT_DIR/synapse-b.csr" \
    -CA "$CERT_DIR/ca.crt" \
    -CAkey "$CERT_DIR/ca.key" \
    -CAcreateserial \
    -out "$CERT_DIR/synapse-b.crt" \
    -days 365 \
    -sha256 \
    -extfile <(printf "subjectAltName=DNS:synapse-b.federation.test,DNS:synapse-b,DNS:localhost,IP:127.0.0.1") \
    2>/dev/null

success "Synapse-B certificate generated: $CERT_DIR/synapse-b.crt"

# ─────────────────────────────────────────────────────────────────────────────
# Step 3: Generate Hosts Configuration
# ─────────────────────────────────────────────────────────────────────────────
log "Step 3: Generating hosts configuration..."

cat > "$HOSTS_FILE" << EOF
# =============================================================================
# Federation Test Hosts Configuration
# =============================================================================
# This file maps container names to IP addresses for DNS resolution
# Add to /etc/hosts if needed for local testing:
#   sudo cat docker/federation-test/hosts.conf >> /etc/hosts
# =============================================================================

# Synapse-A
127.0.0.1 synapse-a
127.0.0.1 synapse-a.local
127.0.0.1 synapse-a.federation.test

# Synapse-B
127.0.0.1 synapse-b
127.0.0.1 synapse-b.local
127.0.0.1 synapse-b.federation.test

# Federation aliases (for Matrix protocol)
127.0.0.1 federation-a
127.0.0.1 federation-b
EOF

success "Hosts configuration generated: $HOSTS_FILE"

# ─────────────────────────────────────────────────────────────────────────────
# Step 4: Verify Certificates
# ─────────────────────────────────────────────────────────────────────────────
log "Step 4: Verifying certificates..."

echo ""
echo "=== CA Certificate ==="
openssl x509 -in "$CERT_DIR/ca.crt" -text -noout | grep -A1 "Subject:" | head -2

echo ""
echo "=== Synapse-A Certificate ==="
openssl x509 -in "$CERT_DIR/synapse-a.crt" -text -noout | grep -A1 "Subject:" | head -2
openssl x509 -in "$CERT_DIR/synapse-a.crt" -text -noout | grep "Subject Alternative Name"

echo ""
echo "=== Synapse-B Certificate ==="
openssl x509 -in "$CERT_DIR/synapse-b.crt" -text -noout | grep -A1 "Subject:" | head -2
openssl x509 -in "$CERT_DIR/synapse-b.crt" -text -noout | grep "Subject Alternative Name"

echo ""
success "All certificates generated and verified!"
echo ""
log "Certificate files:"
ls -la "$CERT_DIR"
echo ""
log "Hosts configuration:"
cat "$HOSTS_FILE"
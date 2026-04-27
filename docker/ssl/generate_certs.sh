#!/bin/bash

# SSL Certificate Generation Script for Matrix Server
# Domains: cjystx.top, matrix.cjystx.top
# User Format: @user:cjystx.top

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SSL_DIR="${SCRIPT_DIR}"
BACKUP_DIR="$SSL_DIR/backup_$(date +%Y%m%d_%H%M%S)"

# Create backup directory
mkdir -p "$BACKUP_DIR"

# Backup existing certificates
if [ -f "$SSL_DIR/server.crt" ]; then
    cp "$SSL_DIR/server.crt" "$BACKUP_DIR/"
    cp "$SSL_DIR/server.key" "$BACKUP_DIR/"
fi
if [ -f "$SSL_DIR/fullchain.pem" ]; then
    cp "$SSL_DIR/fullchain.pem" "$BACKUP_DIR/"
    cp "$SSL_DIR/privkey.pem" "$BACKUP_DIR/"
fi

echo "=== Generating new SSL certificates ==="
echo "Domains: cjystx.top, matrix.cjystx.top"
echo ""

# Generate CA private key
echo "1. Generating CA private key..."
openssl genrsa -out "$SSL_DIR/ca.key" 4096 2>/dev/null

# Generate CA certificate
echo "2. Generating CA certificate..."
openssl req -x509 -new -nodes -key "$SSL_DIR/ca.key" \
    -sha256 -days 3650 \
    -subj "/C=CN/ST=Beijing/L=Beijing/O=Synapse-Rust/CN=Synapse-Rust CA" \
    -out "$SSL_DIR/ca.crt" 2>/dev/null

# Generate server private key
echo "3. Generating server private key..."
openssl genrsa -out "$SSL_DIR/server.key" 2048 2>/dev/null

# Create SAN configuration file
cat > "$SSL_DIR/san.cnf" << 'EOF'
[req]
distinguished_name = req_distinguished_name
req_extensions = v3_req
prompt = no

[req_distinguished_name]
C = CN
ST = Beijing
L = Beijing
O = Synapse-Rust
OU = Matrix Server
CN = matrix.cjystx.top

[v3_req]
subjectAltName = @alt_names
basicConstraints = CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth, clientAuth

[alt_names]
DNS.1 = cjystx.top
DNS.2 = matrix.cjystx.top
DNS.3 = localhost
DNS.4 = *.cjystx.top
IP.1 = 127.0.0.1
IP.2 = ::1
EOF

# Generate server CSR
echo "4. Generating server CSR..."
openssl req -new -key "$SSL_DIR/server.key" \
    -config "$SSL_DIR/san.cnf" \
    -out "$SSL_DIR/server.csr" 2>/dev/null

# Create extensions file for signing
cat > "$SSL_DIR/extensions.cnf" << 'EOF'
subjectAltName = DNS:cjystx.top, DNS:matrix.cjystx.top, DNS:localhost, DNS:*.cjystx.top, IP:127.0.0.1, IP:::1
basicConstraints = CA:FALSE
keyUsage = digitalSignature, keyEncipherment
extendedKeyUsage = serverAuth, clientAuth
subjectKeyIdentifier = hash
authorityKeyIdentifier = keyid,issuer
EOF

# Sign server certificate with CA
echo "5. Signing server certificate with CA..."
openssl x509 -req -in "$SSL_DIR/server.csr" \
    -CA "$SSL_DIR/ca.crt" -CAkey "$SSL_DIR/ca.key" \
    -CAcreateserial \
    -out "$SSL_DIR/server.crt" \
    -days 365 \
    -sha256 \
    -extfile "$SSL_DIR/extensions.cnf" 2>/dev/null

# Create fullchain (server cert + CA cert)
echo "6. Creating fullchain certificate..."
cat "$SSL_DIR/server.crt" "$SSL_DIR/ca.crt" > "$SSL_DIR/fullchain.pem"

# Copy private key to privkey.pem for compatibility
cp "$SSL_DIR/server.key" "$SSL_DIR/privkey.pem"

# Set proper permissions
chmod 644 "$SSL_DIR/server.crt" "$SSL_DIR/fullchain.pem" "$SSL_DIR/ca.crt"
chmod 600 "$SSL_DIR/server.key" "$SSL_DIR/privkey.pem" "$SSL_DIR/ca.key"

# Clean up temporary files
rm -f "$SSL_DIR/server.csr" "$SSL_DIR/san.cnf" "$SSL_DIR/extensions.cnf" "$SSL_DIR/ca.srl"

echo ""
echo "=== SSL Certificate Generation Complete ==="
echo ""
echo "Generated files:"
echo "  - CA Certificate:     $SSL_DIR/ca.crt"
echo "  - Server Certificate: $SSL_DIR/server.crt"
echo "  - Server Key:         $SSL_DIR/server.key"
echo "  - Full Chain:         $SSL_DIR/fullchain.pem"
echo "  - Private Key:        $SSL_DIR/privkey.pem"
echo ""
echo "Backup saved to: $BACKUP_DIR"
echo ""

# Display certificate information
echo "=== Certificate Details ==="
openssl x509 -in "$SSL_DIR/server.crt" -noout -subject -issuer -dates -ext subjectAltName

echo ""
echo "=== Certificate Verification ==="
openssl verify -CAfile "$SSL_DIR/ca.crt" "$SSL_DIR/server.crt"

echo ""
echo "=== Done ==="

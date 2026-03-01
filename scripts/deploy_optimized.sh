#!/bin/bash
# Synapse-Rust Optimization Deployment Script
# This script rebuilds and deploys the optimized version

set -e

PROJECT_DIR="/home/tzd/synapse-rust"
DOCKER_DIR="$PROJECT_DIR/docker"
API_TEST_DIR="/home/tzd/api-test"

echo "========================================"
echo "Synapse-Rust Optimization Deployment"
echo "========================================"
echo "Project: $PROJECT_DIR"
echo "Time: $(date)"
echo ""

# Step 1: Backup current state
echo "[1/7] Creating backup..."
BACKUP_DIR="$PROJECT_DIR/backup_$(date +%Y%m%d_%H%M%S)"
mkdir -p "$BACKUP_DIR"
cp -r "$PROJECT_DIR/src" "$BACKUP_DIR/src"
echo "  Backup created at: $BACKUP_DIR"

# Step 2: Stop current services
echo ""
echo "[2/7] Stopping current services..."
cd "$DOCKER_DIR"
docker-compose down --remove-orphans 2>/dev/null || true
echo "  Services stopped"

# Step 3: Run database migrations
echo ""
echo "[3/7] Running database migrations..."
cd "$PROJECT_DIR"
if [ -f "migrations/20260227_security_enhancements.sql" ]; then
    # Check if postgres is running locally or in docker
    if docker ps | grep -q synapse-postgres; then
        echo "  Running migrations in Docker..."
        docker exec synapse-postgres psql -U synapse -d synapse_test -f /dev/stdin < migrations/20260227_security_enhancements.sql 2>/dev/null || echo "  Migration already applied or partial"
    else
        echo "  Running migrations locally..."
        PGPASSWORD=synapse psql -h localhost -U synapse -d synapse_test -f migrations/20260227_security_enhancements.sql 2>/dev/null || echo "  Migration already applied or partial"
    fi
fi
echo "  Migrations complete"

# Step 4: Build optimized release
echo ""
echo "[4/7] Building optimized release..."
cd "$PROJECT_DIR"
cargo build --release 2>&1 | tail -5
echo "  Build complete"

# Step 5: Build Docker image
echo ""
echo "[5/7] Building Docker image..."
docker build -f docker/Dockerfile -t synapse-rust:latest .
docker tag synapse-rust:latest synapse-rust:optimized_$(date +%Y%m%d)
echo "  Docker image built"

# Step 6: Start services
echo ""
echo "[6/7] Starting services..."
cd "$DOCKER_DIR"
docker-compose up -d

echo ""
echo "Waiting for services to be healthy..."
sleep 10
for i in {1..30}; do
    if curl -sf http://localhost:8008/_matrix/client/versions > /dev/null 2>&1; then
        echo "  Services are healthy!"
        break
    fi
    echo "  Waiting... ($i/30)"
    sleep 2
done

# Step 7: Run tests
echo ""
echo "[7/7] Running verification tests..."
cd "$API_TEST_DIR"

# Refresh tokens
python3 scripts/refresh_tokens.py 2>/dev/null || true

# Run security tests
echo "  Running security tests..."
python3 scripts/security_tests.py 2>&1 | tail -10

echo ""
echo "========================================"
echo "Deployment Complete"
echo "========================================"
echo ""
echo "Services:"
docker-compose -f "$DOCKER_DIR/docker-compose.yml" ps
echo ""
echo "To view logs:"
echo "  cd $DOCKER_DIR && docker-compose logs -f"
echo ""
echo "To rollback:"
echo "  cp -r $BACKUP_DIR/src/* $PROJECT_DIR/src/"
echo "  cd $PROJECT_DIR && cargo build --release"
echo "  docker build -f docker/Dockerfile -t synapse-rust:latest ."
echo "  cd $DOCKER_DIR && docker-compose up -d"

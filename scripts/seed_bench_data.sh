#!/bin/bash
# scripts/seed_bench_data.sh — seed test data for performance benchmarks
set -euo pipefail

BASE_URL="${BENCH_BASE_URL:-http://localhost:8008}"
ADMIN_TOKEN="${BENCH_ADMIN_TOKEN:-}"
TEST_ROOM_ID="!test:localhost"

register_user() {
    local username="$1"
    curl -s -X POST "${BASE_URL}/_synapse/admin/v1/register" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        -d "{\"username\": \"${username}\", \"password\": \"BenchPass123!\", \"admin\": false}"
}

echo "=== Seeding bench users ==="
BENCH_TOKEN=$(register_user "bench_user" | jq -r '.access_token')

echo "=== Creating test room ==="
ROOM_ID=$(curl -s -X POST "${BASE_URL}/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer ${BENCH_TOKEN}" \
    -H "Content-Type: application/json" \
    -d "{\"room_id\": \"${TEST_ROOM_ID}\", \"name\": \"Bench Test Room\"}" | jq -r '.room_id')
echo "Room: ${ROOM_ID}"

echo "=== Seeding 100 users for search bench ==="
for i in $(seq 1 100); do
    register_user "search_user_${i}" >/dev/null
done

echo "=== Creating large room with 50 members for sync bench ==="
LARGE_ROOM=$(curl -s -X POST "${BASE_URL}/_matrix/client/v3/createRoom" \
    -H "Authorization: Bearer ${BENCH_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"name": "Large Bench Room"}' | jq -r '.room_id')
for i in $(seq 1 50); do
    TOKEN=$(register_user "large_user_${i}" | jq -r '.access_token')
    curl -s -X POST "${BASE_URL}/_matrix/client/v3/rooms/${LARGE_ROOM}/join" \
        -H "Authorization: Bearer ${TOKEN}" >/dev/null &
done
wait

echo "=== Registering device keys ==="
curl -s -X POST "${BASE_URL}/_matrix/client/v3/keys/upload" \
    -H "Authorization: Bearer ${BENCH_TOKEN}" \
    -H "Content-Type: application/json" \
    -d '{"device_keys": {"user_id": "@bench_user:localhost", "device_id": "BENCHDEV", "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"], "keys": {"curve25519:BENCHDEV": "test", "ed25519:BENCHDEV": "test"}}}' >/dev/null

echo ""
echo "=== Seed complete ==="
echo "export BENCH_ADMIN_TOKEN=${ADMIN_TOKEN}"
echo "export BENCH_USER_TOKEN=${BENCH_TOKEN}"
echo "export BENCH_ROOM_ID=${ROOM_ID}"
echo "export BENCH_LARGE_ROOM_ID=${LARGE_ROOM}"

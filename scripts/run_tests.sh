#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

POSTGRES_CONTAINER="synapse_postgres"
POSTGRES_USER="synapse"
POSTGRES_DB="synapse_test"

echo "=========================================="
echo "Synapse Rust Test Runner"
echo "=========================================="

check_postgres() {
    if docker ps --format '{{.Names}}' | grep -q "^${POSTGRES_CONTAINER}$"; then
        echo "PostgreSQL container '${POSTGRES_CONTAINER}' is running"
        return 0
    fi
    return 1
}

start_postgres() {
    echo "Starting PostgreSQL container..."
    if ! docker start "$POSTGRES_CONTAINER" 2>/dev/null; then
        echo "Container not found or cannot start. Creating new container..."
        docker run -d \
            --name "$POSTGRES_CONTAINER" \
            -e POSTGRES_USER="$POSTGRES_USER" \
            -e POSTGRES_PASSWORD="synapse" \
            -e POSTGRES_DB="$POSTGRES_DB" \
            -p 5432:5432 \
            postgres:15 \
            || {
                echo "Failed to create PostgreSQL container"
                exit 1
            }
        echo "Waiting for PostgreSQL to be ready..."
        sleep 5
    fi
}

wait_for_postgres() {
    echo "Waiting for PostgreSQL to be ready..."
    local max_attempts=30
    local attempt=0
    while [ $attempt -lt $max_attempts ]; do
        if docker exec "$POSTGRES_CONTAINER" pg_isready -U "$POSTGRES_USER" -d "$POSTGRES_DB" >/dev/null 2>&1; then
            echo "PostgreSQL is ready!"
            return 0
        fi
        attempt=$((attempt + 1))
        echo "Attempt $attempt/$max_attempts - waiting..."
        sleep 2
    done
    echo "PostgreSQL failed to become ready"
    return 1
}

run_migrations() {
    echo "Running database migrations..."
    local migration_count=$(ls -1 "$PROJECT_ROOT/migrations/"*.sql | wc -l)
    echo "Found $migration_count migration files"

    for migration in "$PROJECT_ROOT/migrations/"*.sql; do
        if [ -f "$migration" ]; then
            local filename=$(basename "$migration")
            echo "  Running: $filename"
            docker exec -i "$POSTGRES_CONTAINER" psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" < "$migration" 2>/dev/null || true
        fi
    done
    echo "Migrations completed"
}

run_unit_tests() {
    echo ""
    echo "=========================================="
    echo "Running Unit Tests (no database required)"
    echo "=========================================="
    SQLX_OFFLINE=0 cargo test --lib 2>&1 | tail -20
}

run_all_tests() {
    echo ""
    echo "=========================================="
    echo "Running All Tests (with database)"
    echo "=========================================="
    SQLX_OFFLINE=0 cargo test 2>&1 | tail -30
}

run_ignored_tests() {
    echo ""
    echo "=========================================="
    echo "Running Ignored Integration Tests"
    echo "=========================================="
    SQLX_OFFLINE=0 cargo test -- --ignored 2>&1 | tail -30
}

usage() {
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  unit         Run unit tests only (no database required)"
    echo "  integration  Run all tests including integration tests"
    echo "  ignored      Run tests marked as ignored"
    echo "  all          Run all of the above"
    echo "  help         Show this help message"
    echo ""
    echo "Examples:"
    echo "  $0 unit          # Quick unit tests"
    echo "  $0 integration   # All tests with database"
    echo "  $0 all           # Full test suite"
}

main() {
    local command="${1:-unit}"

    case "$command" in
        unit)
            run_unit_tests
            ;;
        integration)
            if check_postgres; then
                wait_for_postgres
                run_migrations
                run_all_tests
            else
                echo "Error: PostgreSQL is not running. Start it with: docker start $POSTGRES_CONTAINER"
                exit 1
            fi
            ;;
        ignored)
            if check_postgres; then
                wait_for_postgres
                run_migrations
                run_ignored_tests
            else
                echo "Error: PostgreSQL is not running. Start it with: docker start $POSTGRES_CONTAINER"
                exit 1
            fi
            ;;
        all)
            if check_postgres; then
                wait_for_postgres
                run_migrations
            else
                start_postgres
                wait_for_postgres
                run_migrations
            fi
            run_unit_tests
            run_all_tests
            run_ignored_tests
            ;;
        help|--help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown command: $command"
            usage
            exit 1
            ;;
    esac
}

main "$@"

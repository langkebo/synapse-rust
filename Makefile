# Synapse-Rust Database Makefile
# 简化数据库迁移操作的命令行工具

.PHONY: help migrate migrate-check migrate-undo migrate-status migrate-baseline migrate-audit
.PHONY: test test-unit test-integration test-all test-coverage
.PHONY: lint fmt check
.PHONY: build build-release

# 默认目标
help:
	@echo "Synapse-Rust Database Management"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Migration Commands:"
	@echo "  migrate          - Run all pending migrations"
	@echo "  migrate-check   - Check migration status without running"
	@echo "  migrate-status   - Show migration status"
	@echo "  migrate-undo     - Undo the last migration"
	@echo "  migrate-baseline - Create baseline from current schema"
	@echo "  migrate-audit    - Show migration audit log"
	@echo ""
	@echo "Test Commands:"
	@echo "  test             - Run all tests"
	@echo "  test-unit        - Run unit tests only"
	@echo "  test-integration - Run integration tests only"
	@echo "  test-coverage    - Run tests with coverage"
	@echo ""
	@echo "Code Quality:"
	@echo "  lint             - Run linter"
	@echo "  fmt              - Format code"
	@echo "  check            - Run all checks"
	@echo ""
	@echo "Build:"
	@echo "  build            - Build debug version"
	@echo "  build-release    - Build release version"

# 数据库配置
DATABASE_URL ?= postgresql://synapse:synapse@localhost:5432/synapse
FLYWAY_URL ?= postgresql://synapse:synapse@localhost:5432/synapse
export DATABASE_URL

# Migration Commands
migrate:
	@echo "Running migrations..."
	@sqlx database create || true
	@sqlx migrate run
	@echo "Migrations completed"

migrate-check:
	@echo "Checking migration status..."
	@sqlx migrate check

migrate-status:
	@echo "Migration status:"
	@PGPASSWORD=synapse psql "$(FLYWAY_URL)" -c "SELECT version, name, success, applied_ts, executed_at FROM schema_migrations ORDER BY COALESCE(applied_ts, FLOOR(EXTRACT(EPOCH FROM executed_at) * 1000)::BIGINT) DESC NULLS LAST, id DESC LIMIT 10;"

migrate-undo:
	@echo "Undoing last migration..."
	@sqlx migrate revert
	@echo "Migration reverted"

migrate-baseline:
	@echo "Creating baseline from current schema..."
	@sqlx migrate add --source=baseline --digest=$(shell date +%Y%m%d%H%M%S) baseline
	@echo "Baseline migration created. Review and run manually if needed."

migrate-audit:
	@echo "Migration audit log:"
	@PGPASSWORD=synapse psql "$(FLYWAY_URL)" -c "SELECT version, name, description, execution_time_ms, applied_ts, executed_at, success FROM schema_migrations ORDER BY COALESCE(applied_ts, FLOOR(EXTRACT(EPOCH FROM executed_at) * 1000)::BIGINT) DESC NULLS LAST, id DESC LIMIT 20;"

# Flyway Commands (optional)
flyway-info:
	@echo "Running Flyway info..."
	@docker run --rm -v "$(PWD)/migrations:/flyway/sql" \
		-v "$(PWD)/scripts/db/flyway.conf:/flyway/conf/flyway.conf" \
		-v "$(PWD)/scripts/db/undo:/flyway/undo" \
		pxmatrix/flyway:10.4 info

flyway-migrate:
	@echo "Running Flyway migrate..."
	@docker run --rm -v "$(PWD)/migrations:/flyway/sql" \
		-v "$(PWD)/scripts/db/flyway.conf:/flyway/conf/flyway.conf" \
		-v "$(PWD)/scripts/db/undo:/flyway/undo" \
		-e FLYWAY_URL="$(FLYWAY_URL)" \
		pxmatrix/flyway:10.4 migrate

# Test Commands
test:
	@echo "Running all tests..."
	@cargo test

test-unit:
	@echo "Running unit tests..."
	@cargo test --lib

test-integration:
	@echo "Running integration tests..."
	@cargo test --test '*'

test-coverage:
	@echo "Running tests with coverage..."
	@cargo tarpaulin --out Html --out Xml --out Json --scope Unit --scope Integration

# Code Quality Commands
lint:
	@echo "Running linter..."
	@cargo clippy --all-targets --all-features -- -D warnings || true

fmt:
	@echo "Formatting code..."
	@cargo fmt --all

check: fmt lint
	@echo "Running all checks..."
	@cargo check --all-targets --all-features

# Build Commands
build:
	@echo "Building debug version..."
	@cargo build

build-release:
	@echo "Building release version..."
	@cargo build --release --all-features

# Schema Operations
schema-diff:
	@echo "Generating schema diff..."
	@python3 scripts/db/extract_schema.py --output /tmp/expected_schema.json --skip-row-counts
	@PGPASSWORD=synapse psql "$(FLYWAY_URL)" -c "SELECT 'Run drift detection manually'" || true

schema-drift:
	@echo "Running schema drift detection..."
	@python3 scripts/db/diff_schema.py /tmp/expected_schema.json /tmp/actual_schema.json || true

# Lifecycle Management
lifecycle-scan:
	@echo "Scanning for deprecated/unused migrations..."
	@python3 scripts/db/lifecycle_manager.py migrations/ --list

lifecycle-candidates:
	@echo "Finding candidates for deprecation..."
	@python3 scripts/db/lifecycle_manager.py migrations/ --candidates

# Compression
compress:
	@echo "Compressing migration scripts..."
	@python3 scripts/db/compress_migrations.py migrations/ --dry-run

compress-apply:
	@echo "Applying compression..."
	@python3 scripts/db/compress_migrations.py migrations/

# Performance Test
perf-test:
	@echo "Running performance tests..."
	@bash scripts/test/perf/run_tests.sh smoke

perf-test-baseline:
	@echo "Running baseline performance test..."
	@bash scripts/test/perf/run_tests.sh baseline

perf-test-all:
	@echo "Running all performance tests..."
	@bash scripts/test/perf/run_tests.sh all

# Cleanup
clean:
	@echo "Cleaning build artifacts..."
	@cargo clean
	@rm -rf target/

clean-migrations:
	@echo "Cleaning migration backup files..."
	@find migrations/ -name "*.backup" -delete
	@find migrations/ -name "*.orig" -delete

# Docker Database
db-start:
	@echo "Starting database..."
	@cd docker && docker compose up -d db

db-stop:
	@echo "Stopping database..."
	@cd docker && docker compose stop db

db-logs:
	@echo "Database logs:"
	@cd docker && docker compose logs -f db

db-reset: db-stop db-start
	@echo "Database reset complete"

# Help
.DEFAULT_GOAL := help

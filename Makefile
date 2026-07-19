# Synapse-Rust Database Makefile
# 简化数据库迁移操作的命令行工具

.PHONY: help migrate migrate-check migrate-undo migrate-status migrate-baseline migrate-audit
.PHONY: test test-unit test-integration test-all test-coverage test-coverage-check test-mutation test-mutation-incremental
.PHONY: lint fmt format format-check format-install format-audit format-cycle check route-lint
.PHONY: build build-release

MUTATION_BATCH_FILES ?= src/web/routes/extractors/pagination.rs src/web/routes/extractors/json.rs src/services/media/mod.rs src/web/middleware/security.rs

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
	@echo "  test                  - Run all tests"
	@echo "  test-unit             - Run unit tests only"
	@echo "  test-integration      - Run integration tests only"
	@echo "  test-coverage         - Run tests with coverage"
	@echo "  test-coverage-check   - Run tests with coverage threshold (≥25% hard floor + per-file ratchet)"
	@echo "  test-mutation         - Run batched mutation smoke tests (cargo-mutants)"
	@echo "  test-mutation-incr    - Run incremental mutation tests"
	@echo ""
	@echo "Code Quality:"
	@echo "  lint             - Run linter"
	@echo "  fmt              - Format code"
	@echo "  format           - Run repository-wide formatters"
	@echo "  format-check     - Run repository-wide format compliance checks"
	@echo "  format-install   - Install pre-commit hooks"
	@echo "  format-audit     - Generate formatting drift audit report"
	@echo "  format-cycle     - Refresh the rolling three-cycle format tracking report"
	@echo "  route-lint       - Check route→service→storage layering"
	@echo "  check            - Run all checks"
	@echo "  schema-health-check        - Run schema health check (报告模式, 不阻塞)"
	@echo "  schema-health-check-strict - Run schema health check (CI 严格模式, 失败则退出)"
	@echo "  ci-schema-health-check     - Full CI: start temp DB + apply v8 + run schema check"
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
	@cargo test --locked

test-unit:
	@echo "Running unit tests..."
	@cargo test --lib --locked

test-integration:
	@echo "Running integration tests..."
	@cargo test --locked --test '*'

test-coverage:
	@echo "Running tests with coverage (tarpaulin)..."
	@cargo tarpaulin --out Html --out Xml --out Json --include-tests --locked

test-coverage-check:
	@echo "Running tests with coverage threshold check (≥25% hard floor, per-file ratchet enforces ≥80% on TDD files)..."
	@cargo tarpaulin --fail-under 25 --out Html --out Lcov --out Json --output-dir coverage --include-tests --locked
	@python3 scripts/check_file_coverage.py \
	  --report coverage/tarpaulin-report.json \
	  --baseline artifacts/coverage_baseline.json \
	  --threshold 80 --global-floor 25 --new-file-floor 20

test-mutation:
	@echo "Running batched mutation smoke tests (cargo-mutants, nightly)..."
	@for file in $(MUTATION_BATCH_FILES); do \
		echo "==> $$file"; \
		cargo mutants --package synapse-rust --file "$$file" --timeout 30 --baseline skip -- --test-threads=2 || exit $$?; \
	done

test-mutation-incremental:
	@echo "Running incremental mutation tests on changed files..."
	@cargo mutants --incremental --timeout 30 -- --test-threads=2

# Code Quality Commands
lint:
	@echo "Running linter..."
	@cargo clippy --all-features --locked -- -D warnings

fmt:
	@echo "Formatting code..."
	@cargo fmt --all

format:
	@echo "Running repository-wide formatters..."
	@bash scripts/quality/format_write.sh

format-check:
	@echo "Running repository-wide format compliance checks..."
	@bash scripts/quality/format_check.sh

format-install:
	@echo "Installing pre-commit hooks..."
	@pre-commit install --hook-type pre-commit --hook-type pre-push

format-audit:
	@echo "Generating formatting drift audit report..."
	@python3 scripts/quality/format_audit.py --output docs/quality/FORMAT_STANDARDIZATION_AUDIT_2026-05-29.md

format-cycle:
	@echo "Refreshing three-cycle format drift tracking report..."
	@bash scripts/quality/format_check.sh
	@label=$${CYCLE_LABEL:-manual-$$(date -u +%Y-%m-%d)}; \
	base_ref=$${BASE_REF:-HEAD~1}; \
	head_ref=$${HEAD_REF:-HEAD}; \
	python3 scripts/quality/format_cycle_report.py \
		--cycle-label "$$label" \
		--base-ref "$$base_ref" \
		--head-ref "$$head_ref" \
		--compliance-status pass

route-lint:
	@echo "Checking route layering..."
	@bash scripts/quality/check_route_layering.sh

# Schema health check (M-3 CI 强制门禁)
# 默认：报告状态但允许失败（开发环境）
# 设 STRICT=1 在 CI 中以严格模式运行
schema-health-check:
	@echo "Running schema health check against current DATABASE_URL..."
	@DATABASE_URL=$${DATABASE_URL:-$(DATABASE_URL)} cargo run --quiet --bin schema_health_check --locked || \
		(echo ""; echo "⚠️  Schema health check 报告漂移（非严格模式）"; echo "  设 STRICT=1 在 CI 中以失败模式运行"; exit 0)

schema-health-check-strict:
	@echo "Running schema health check (STRICT mode)..."
	@DATABASE_URL=$${DATABASE_URL:-$(DATABASE_URL)} cargo run --quiet --bin schema_health_check --locked

ci-schema-health-check:
	@echo "Running CI schema health check (start temp DB if needed)..."
	@bash scripts/ci_schema_health_check.sh

check: fmt lint
	@echo "Running all checks..."
	@cargo check --all-features --locked

# Build Commands
build:
	@echo "Building debug version..."
	@cargo build

build-release:
	@echo "Building release version..."
	@cargo build --release --all-features

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

# Docker image build — tags both the generic `synapse-rust:latest` and the
# `${SYNAPSE_IMAGE}:${SYNAPSE_IMAGE_TAG}` pair from docker/.env so that
# `docker compose up` picks up the freshly built binary.
# See docs/synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md §1.6 for why this
# target exists.
docker-build:
	@set -eu; \
	IMAGE=$$(grep -E '^SYNAPSE_IMAGE=' docker/.env | cut -d= -f2); \
	TAG=$$(grep -E '^SYNAPSE_IMAGE_TAG=' docker/.env | cut -d= -f2); \
	BUILDER=$${SYNAPSE_BUILDX_BUILDER:-amd64builder}; \
	echo "Building $${IMAGE}:$${TAG} (also tagged synapse-rust:latest) via $${BUILDER}..."; \
	docker buildx build \
	    --builder $${BUILDER} \
	    --platform linux/amd64 \
	    -f docker/Dockerfile \
	    -t synapse-rust:latest \
	    -t $${IMAGE}:$${TAG} \
	    --load \
	    .

docker-redeploy: docker-build
	@cd docker && docker compose -f docker-compose.yml -f docker-compose.web.yml \
	    up -d --no-deps --force-recreate synapse-rust

# Help
.DEFAULT_GOAL := help

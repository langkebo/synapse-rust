# Synapse-Rust Makefile
# 构建 / 测试 / 代码质量 入口；数据库迁移的只读查询目标（执行入口见 docker/db_migrate.sh）

.PHONY: help migrate-status migrate-audit
.PHONY: test test-unit test-integration test-all test-fast test-coverage test-coverage-check test-mutation test-mutation-incremental
.PHONY: lint fmt format format-check format-install format-audit format-cycle check route-lint route-contract-check
.PHONY: build build-release

MUTATION_BATCH_FILES ?= src/web/routes/extractors/pagination.rs src/web/routes/extractors/json.rs src/services/media/mod.rs src/web/middleware/security.rs

# 默认目标
help:
	@echo "Synapse-Rust Make Targets"
	@echo ""
	@echo "Usage: make [target]"
	@echo ""
	@echo "Migration Commands (只读查询；执行入口只有一个，不在 Makefile 里):"
	@echo "  migrate-status   - Show migration status (compose 栈内 psql)"
	@echo "  migrate-audit    - Show migration audit log (compose 栈内 psql)"
	@echo "  执行/校验迁移请用唯一入口: bash docker/db_migrate.sh {init|migrate|status|validate}"
	@echo ""
	@echo "Test Commands:"
	@echo "  test                  - Run all tests"
	@echo "  test-unit             - Run unit tests only"
	@echo "  test-integration      - Run integration tests only"
	@echo "  test-fast             - Run tests with nextest (fast iteration, no coverage)"
	@echo "  test-coverage         - Run tests with coverage (llvm-cov, 分两步 + schema 复用)"
	@echo "  test-coverage-check   - Coverage threshold check (per-file ratchet; 待迁移 llvm-cov)"
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
#
# 两种连接模型，不要混用：
#
#   1) 容器 exec —— migrate-status / migrate-audit 等只读查询走 compose 栈里的
#      postgres 容器，不需要宿主端口。默认指向 dev 栈（docker/docker-compose.yml
#      的服务 `db`），查 deploy 栈覆盖变量即可。
#
#      为什么不用宿主 psql 连 localhost:5432：那个端口通常被本机 Homebrew
#      PostgreSQL 占着（上面没有 synapse 库），而 compose 栈默认根本不发布 5432；
#      即便叠加 dev-host-access overlay 也会撞端口。走容器 exec 则两者都不受影响。
#
#   2) DATABASE_URL —— schema_health_check 等**宿主进程**需要宿主可达地址，
#      必须先 `cd docker && docker compose -f docker-compose.yml
#      -f docker-compose.dev-host-access.yml up -d db` 才能用。
DATABASE_URL ?= postgresql://synapse:synapse@localhost:5432/synapse
export DATABASE_URL

# compose 栈定位（只影响走容器 exec 的查询目标；路径相对仓库根）
#   dev 栈（默认）: make migrate-status
#   deploy 栈:      make migrate-status COMPOSE_DIR=docker/deploy DB_SERVICE=postgres
COMPOSE_DIR ?= docker
COMPOSE_FILES ?= -f docker-compose.yml
DB_SERVICE ?= db
DB_USER ?= synapse
DB_NAME ?= synapse

# docker compose 调用前缀：必须在 compose 目录内执行，否则 project name 与
# 相对挂载路径都会解析错。
DC = cd $(COMPOSE_DIR) && docker compose $(COMPOSE_FILES)

# 注意不要出现反引号，否则会被 shell 当命令替换执行。
DB_CONNECT_HINT = 提示：该查询走 compose 栈内的 postgres 容器，需先启动对应栈。dev 栈执行 make db-start；查 deploy 栈请 make migrate-status COMPOSE_DIR=docker/deploy DB_SERVICE=postgres

# Migration 只读查询
#
# 迁移的**执行**入口只有一个：bash docker/db_migrate.sh {init|migrate|status|validate}
# （记账表 schema_migrations，含扩展门控）。这里刻意不提供 `make migrate`：Makefile
# 一旦转发，它自带的 DATABASE_URL 默认值就会让脚本误以为"调用方显式指定了目标"，
# 于是打到宿主自装的那台 PostgreSQL —— 那正是 H-14。
migrate-status:
	@echo "Migration status ($(COMPOSE_DIR) / $(DB_SERVICE)):"
	@$(DC) exec -T $(DB_SERVICE) psql -U $(DB_USER) -d $(DB_NAME) -c "SELECT version, name, is_success, applied_ts, executed_at FROM schema_migrations ORDER BY COALESCE(applied_ts, executed_at) DESC NULLS LAST, id DESC LIMIT 10;" || { echo "$(DB_CONNECT_HINT)"; exit 1; }

migrate-audit:
	@echo "Migration audit log ($(COMPOSE_DIR) / $(DB_SERVICE)):"
	@$(DC) exec -T $(DB_SERVICE) psql -U $(DB_USER) -d $(DB_NAME) -c "SELECT version, name, description, execution_time_ms, applied_ts, executed_at, is_success FROM schema_migrations ORDER BY COALESCE(applied_ts, executed_at) DESC NULLS LAST, id DESC LIMIT 20;" || { echo "$(DB_CONNECT_HINT)"; exit 1; }

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

# 日常快速迭代：nextest（不插桩）。注意 DB 集成测试的 schema pool 复用（方案 B）
# 是进程内优化，nextest 默认 process-per-test 不会跨进程复用，故 DB 集成测试仍
# 用 `cargo test <filter>` 更快；nextest 适合 lib/unit 纯逻辑测试。
test-fast:
	@echo "Running tests with nextest (fast, no coverage)..."
	@cargo nextest run --profile test --features "test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso" --locked

# 覆盖率测量：改用 cargo llvm-cov（tarpaulin 0.35.2 有 --implicit-test-threads bug
# + LLVM 引擎测试失败不产 lcov）。分两步（storage 单独单线程 + rest），跑完自动
# 兜底清理累积 schema。详见 scripts/run_local_coverage.sh。
test-coverage:
	@echo "Running tests with coverage (llvm-cov)..."
	@bash scripts/run_local_coverage.sh
	@python3 scripts/analyze_coverage.py

# 覆盖率阈值门禁（per-file ratchet）。check_file_coverage.py 已支持 lcov
# （--format lcov），与 test-coverage 共用 run_local_coverage.sh 产物。
test-coverage-check:
	@echo "Running tests with coverage threshold check (≥40% hard floor, per-file ratchet enforces ≥80% on TDD files)..."
	@bash scripts/run_local_coverage.sh
	@python3 scripts/check_file_coverage.py \
	  --report coverage/lcov.info \
	  --format lcov \
	  --baseline artifacts/coverage_baseline.json \
	  --threshold 80 --global-floor 40 --new-file-floor 30 \
	  --core-files artifacts/core_file_list.txt --core-threshold 70

test-cov-local:
	@echo "Running llvm-cov coverage locally (alias of test-coverage)..."
	@bash scripts/run_local_coverage.sh
	@python3 scripts/analyze_coverage.py

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

# Route Contract Drift Gate (防漂移门禁)
# 重生成 docs/synapse-rust/ROUTE_CONTRACT.md 并与已提交版本做归一化比对，
# 结构性漂移即失败。对应 CI: .github/workflows/route-contract-gate.yml
route-contract-check:
	@echo "Checking ROUTE_CONTRACT.md against source route surface..."
	@bash scripts/contract/check_route_contract.sh

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

check: fmt lint route-contract-check
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

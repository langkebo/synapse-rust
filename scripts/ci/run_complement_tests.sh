#!/usr/bin/env bash
# Run Complement black-box interop tests against synapse-rust.
#
# Usage:
#   scripts/ci/run_complement_tests.sh                 # run all interop tests
#   scripts/ci/run_complement_tests.sh TestRegisterLogin  # run a single test
#
# Environment variables:
#   COMPLEMENT_BASE_IMAGE  — name of the prebuilt synapse-rust complement image
#                            (default: complement-synapse-rust)
#   COMPLEMENT_SRC_DIR     — path to a local complement checkout
#                            (default: ./target/complement-src)
#   KEEP_IMAGES            — set to "1" to keep homeserver images after the run
#   GOFLAGS                — extra flags passed to `go test`
#
# Requirements:
#   - docker (running daemon)
#   - go ≥ 1.22
#   - rust toolchain (only if the image needs to be built)
#
# This script follows the Complement base image contract:
#   https://github.com/matrix-org/complement/blob/main/docs/homeserver-design-overview.md

set -euo pipefail

# ── Configuration ────────────────────────────────────────────────────────────

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"

COMPLEMENT_BASE_IMAGE="${COMPLEMENT_BASE_IMAGE:-complement-synapse-rust}"
COMPLEMENT_SRC_DIR="${COMPLEMENT_SRC_DIR:-${REPO_ROOT}/target/complement-src}"
COMPLEMENT_GIT_REF="${COMPLEMENT_GIT_REF:-main}"
KEEP_IMAGES_FLAG=""
if [[ "${KEEP_IMAGES:-0}" == "1" ]]; then
    KEEP_IMAGES_FLAG="-keep-images"
fi

TEST_TARGET="${1:-}"

cd "${REPO_ROOT}"

# ── Preflight checks ────────────────────────────────────────────────────────

require() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "[complement] ERROR: required command '$1' not found in PATH" >&2
        exit 1
    fi
}

require docker
require go

if ! docker info >/dev/null 2>&1; then
    echo "[complement] ERROR: docker daemon is not running" >&2
    exit 1
fi

# ── Build the complement-compatible Docker image (if missing) ────────────────

if ! docker image inspect "${COMPLEMENT_BASE_IMAGE}" >/dev/null 2>&1; then
    echo "[complement] Building ${COMPLEMENT_BASE_IMAGE} (this may take a while…)"
    docker build \
        -t "${COMPLEMENT_BASE_IMAGE}" \
        -f docker/complement/Dockerfile \
        "${REPO_ROOT}"
else
    echo "[complement] Image ${COMPLEMENT_BASE_IMAGE} already present — skipping build"
fi

# ── Ensure a Complement source checkout exists ───────────────────────────────

if [[ ! -d "${COMPLEMENT_SRC_DIR}/.git" ]]; then
    echo "[complement] Cloning matrix-org/complement @ ${COMPLEMENT_GIT_REF} into ${COMPLEMENT_SRC_DIR}"
    mkdir -p "$(dirname "${COMPLEMENT_SRC_DIR}")"
    git clone --depth 1 --branch "${COMPLEMENT_GIT_REF}" \
        https://github.com/matrix-org/complement.git \
        "${COMPLEMENT_SRC_DIR}"
else
    echo "[complement] Reusing existing complement checkout at ${COMPLEMENT_SRC_DIR}"
fi

# ── Run the interop tests ────────────────────────────────────────────────────

echo "[complement] Running interop tests against ${COMPLEMENT_BASE_IMAGE}"

TEST_PATTERN="./..."
if [[ -n "${TEST_TARGET}" ]]; then
    TEST_PATTERN="-run ^${TEST_TARGET}\$ ./..."
fi

# Complement reads COMPLEMENT_BASE_IMAGE and spawns the homeserver containers
# from that image. The Go test binary talks to the containers via the Docker
# network. We invoke `go test` from inside the complement source tree so the
# `github.com/matrix-org/complement/b` helpers resolve correctly, but we also
# point it at our local tests/complement module so our own test files run too.
cd "${REPO_ROOT}/tests/complement"

# Ensure local Go module deps are present. We use -mod=mod so missing entries
# in go.sum don't block CI; downstream PRs should still run `go mod tidy`.
GOFLAGS="${GOFLAGS:-}" go test -v -mod=mod \
    ${KEEP_IMAGES_FLAG} \
    ${TEST_PATTERN}

echo "[complement] Interop tests complete"

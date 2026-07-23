#!/usr/bin/env bash
# Pre-cache swagger-ui zip to avoid GitHub 403 rate-limiting during builds.
# utoipa-swagger-ui v9.0.2 build.rs downloads this file:
#   https://github.com/swagger-api/swagger-ui/archive/refs/tags/v5.17.14.zip
# and caches it at ~/.cache/utoipa-swagger-ui/ (when the "cache" feature is
# enabled).  The build.rs computes the cache key as the uppercase SHA-256 of
# the download URL concatenated with the crate version.

set -euo pipefail

CACHE_DIR="${HOME}/.cache/utoipa-swagger-ui"
ZIP_URL="https://github.com/swagger-api/swagger-ui/archive/refs/tags/v5.17.14.zip"
ZIP_FILENAME="v5.17.14.zip"
PKG_VERSION="9.0.2"

# Compute cache key: SHA256(url + version) -> uppercase
if command -v sha256sum &>/dev/null; then
    CACHE_KEY=$(echo -n "${ZIP_URL}${PKG_VERSION}" | sha256sum | cut -d' ' -f1)
else
    CACHE_KEY=$(echo -n "${ZIP_URL}${PKG_VERSION}" | shasum -a 256 | cut -d' ' -f1)
fi
CACHE_KEY=$(echo -n "${CACHE_KEY}" | tr '[:lower:]' '[:upper:]')

ZIP_DIR="${CACHE_DIR}/swagger-ui/${CACHE_KEY}"
ZIP_FILE="${ZIP_DIR}/${ZIP_FILENAME}"

mkdir -p "${ZIP_DIR}"

if [ -f "${ZIP_FILE}" ]; then
    echo "swagger-ui zip already cached at ${ZIP_FILE}"
    exit 0
fi

echo "Downloading swagger-ui zip..."
curl -fSL --retry 3 -o "${ZIP_FILE}" "${ZIP_URL}"
echo "Cached: ${ZIP_FILE}"

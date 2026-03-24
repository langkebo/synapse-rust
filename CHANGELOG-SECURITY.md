# Security Changelog

All notable security changes will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

### Fixed
- **CVE-2026-XXXX1**: E2EE `PICKLE_KEY` hardcoded to zero bytes. Remediated by requiring `OLM_PICKLE_KEY` environment variable with validation.
- **CVE-2026-XXXX2**: Insecure CORS wildcard configuration removed. Restricted to specific origins in `homeserver.yaml`.
- **CVE-2026-XXXX3**: Hardcoded database and JWT secrets moved to environment variable support.

### Changed
- Updated CI workflow with parallel test execution and improved caching (Swatinem/rust-cache)
- Added `.env.example` for secure configuration
- Added cache cleanup script (`scripts/clean_cache.sh`)

### Security
- Required `OLM_PICKLE_KEY` environment variable for E2EE encryption
- CORS now restricted to configured domains only
- Rate limiting enabled by default (10 req/s, burst 100)

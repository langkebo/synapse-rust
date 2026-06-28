## Why

Pre-landing review (`/review` 2026-06-26) found 12 issues across the synapse-rust codebase: missing test coverage for new fallback and security-header middleware, a CORS bug in nginx 429 rate-limit responses (already auto-fixed), maintainability debt in nginx static resource config, and a minor Matrix spec gap for method-not-allowed responses. All changes are within the existing web middleware, route assembly, and nginx config surfaces — no new services or schema changes.

## What Changes

- Add integration tests for M_UNRECOGNIZED fallback handler and security headers middleware (X-Content-Type-Options, Referrer-Policy)
- Return HTTP 405 M_UNRECOGNIZED for unsupported methods on valid Matrix paths, 404 for unknown paths
- Use pre-serialized `bytes::Bytes` for fallback response to avoid per-request allocation
- Resolve security header duplication between Rust `security_headers_middleware` and nginx server-level `add_header` directives
- Replace hardcoded content-hash filenames in nginx Element Web static resource locations with regex patterns
- DRY up duplicated Cache-Control TTL, redirect pairs, and inline binary favicon.ico in nginx

## Capabilities

### New Capabilities

- `api-error-compliance`: Matrix-compliant M_UNRECOGNIZED JSON responses — 404 for unknown paths, 405 for unsupported methods, zero-allocation response body
- `security-header-enforcement`: Integration tests proving X-Content-Type-Options, Referrer-Policy, CSP, Permissions-Policy, and HSTS headers appear in API responses
- `nginx-static-maintainability`: Regex-based static resource redirects, shared Cache-Control TTL, binary asset served from file instead of inline hex

### Modified Capabilities

<!-- No existing spec-level behavior changes. These are implementation-level improvements. -->

## Impact

- Affected code: `src/web/routes/assembly.rs`, `src/web/middleware/security.rs`, `docker/nginx/nginx.conf`, `docker/element/config.json`
- No API breakage: the fallback handler and CORS fix preserve existing behavior for all valid endpoints
- No database or schema changes
- New test code: integration tests for fallback handler + security headers

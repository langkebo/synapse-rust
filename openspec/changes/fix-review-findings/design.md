## Context

Current state after initial fixes (this branch):
- `src/web/routes/assembly.rs`: Router.fallback() returns JSON `M_UNRECOGNIZED` for unmatched routes. No test coverage.
- `src/web/middleware/security.rs`: Adds X-Content-Type-Options + Referrer-Policy to all responses via axum middleware. No integration test proves headers appear.
- `docker/nginx/nginx.conf`: Custom 429 JSON error page, static resource fallbacks for Element Web (favicon, robots.txt, apple-touch-icon, manifest, service-worker, logo SVGs). CORS headers on 429 now present (auto-fixed). Hardcoded content-hash filenames, duplicated Cache-Control TTL, inline binary favicon.

Constraints: Rust is axum 0.8 + tokio. Nginx is the TLS-terminating reverse proxy. Matrix spec requires M_UNRECOGNIZED errocodes.

## Goals / Non-Goals

**Goals:**
- Integration tests for fallback handler and security headers middleware
- Method-not-allowed → 405 with M_UNRECOGNIZED JSON (separate from 404 path-not-found)
- Pre-serialized response body for fallback (zero per-request allocation)
- Single owner for security headers (eliminate Rust/nginx duplication)
- Regex-based nginx static resource locations (survive bundle hash changes)
- DRY Cache-Control TTL and redirect pairs in nginx

**Non-Goals:**
- New Matrix API endpoints
- Database or schema changes
- Cross-service refactoring
- Production deployment of these changes

## Decisions

### 1. Fallback handler: pre-serialized `Bytes` instead of `serde_json::json!()`
`serde_json::json!()` allocates a `Value` on every miss. The response body is static. Store `Bytes::from_static(b"{\"errcode\":\"M_UNRECOGNIZED\",\"error\":\"Unrecognized request\"}")` once and clone it per response (Bytes::clone is O(1) ref-counted).

### 2. Method-not-allowed: separate `MethodRouter` fallback per route
Axum's `Router::fallback()` only catches path-not-found. When a path matches but the method doesn't, axum returns an empty-body 405. The fix: use axum's `MethodRouter::fallback()` on the top-level router (or key sub-routers) to return JSON `M_UNRECOGNIZED` with status 405. This requires wrapping the `Router` into a `MethodRouter` first, which axum 0.8 supports via `.fallback()` on the `MethodRouter`.

### 3. Security headers: pick Rust as the single owner
The Rust `security_headers_middleware` runs for ALL API responses. The nginx server-level `add_header` directives only apply in limited cases (nginx inheritance rules). Removing them from nginx and keeping them in Rust eliminates the divergence risk. Nginx already clears backend CORS headers via `proxy_hide_header` and adds its own; we do NOT remove those (nginx CORS is intentional). We only remove the security headers that overlap with Rust (X-Frame-Options, X-Content-Type-Options, X-XSS-Protection, Strict-Transport-Security, Referrer-Policy, Permissions-Policy, Content-Security-Policy).

### 4. Nginx static resources: regex location + shared variable
Replace hardcoded content hashes with regex captures (e.g., `location ~ ^/vector-icons/.*\.png$`). Use an nginx `map` or `set` for the shared Cache-Control TTL. Serve favicon.ico from the `webapp/` directory via `root`/`alias` instead of inline hex bytes.

## Risks / Trade-offs

- **Removing security headers from nginx** → If a future response bypasses the Rust middleware, headers may be missing. Mitigation: verify middleware covers all proxied paths before removing nginx headers.
- **Regex nginx locations** → Slightly slower than exact matches, but negligible for static resources (infrequent requests).
- **Method-not-allowed handling** → Axum 0.8 `MethodRouter::fallback()` requires restructuring router assembly. Risk of breaking existing route matching. Mitigation: add integration test first, then refactor.

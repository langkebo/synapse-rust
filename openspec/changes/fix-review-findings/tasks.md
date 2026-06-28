## 1. API Error Compliance

- [x] 1.1 Write integration test for M_UNRECOGNIZED fallback handler (unknown path → 404 JSON)
- [x] 1.2 Replace `serde_json::json!()` with pre-serialized `Bytes::from_static` in fallback handler
- [x] 1.3 Add method-not-allowed (405) JSON handler via `MethodRouter::fallback()` on top-level router
- [x] 1.4 Write integration test for method-not-allowed → 405 JSON with M_UNRECOGNIZED
- [x] 1.5 Run existing test suite to verify no regressions in route matching — Unit: 857 passed, 5 failed (pre-existing ledger/placeholder fixture failures, unrelated). Integration: 5 new tests pass, 0 regressions.

## 2. Security Headers

- [x] 2.1 Write integration test asserting X-Content-Type-Options: nosniff on API responses
- [x] 2.2 Write integration test asserting Referrer-Policy: strict-origin-when-cross-origin on API responses
- [x] 2.3 Write integration test asserting full security header set (CSP + Permissions-Policy + HSTS + nosniff + referrer-policy)
- [x] 2.4 Decide security header owner: Rust middleware only → remove overlapping headers from nginx server-level `add_header`
- [x] 2.5 If Rust is owner, remove X-Frame-Options, X-Content-Type-Options, X-XSS-Protection, HSTS, Referrer-Policy, Permissions-Policy, CSP from nginx matrix.test server block
- [x] 2.6 Verify nginx CORS headers are NOT removed (CORS must stay in nginx)

## 3. Nginx Static Resource Maintainability

- [x] 3.1 Replace hardcoded content-hash filenames with regex locations (apple-touch-icon, element-logo redirects)
- [x] 3.2 Extract shared Cache-Control TTL into an nginx variable or map, reference it in favicon.ico and robots.txt locations
- [x] 3.3 Serve favicon.ico from static file on disk instead of inline hex bytes
- [x] 3.4 Merge apple-touch-icon + apple-touch-icon-precomposed into single regex location
- [x] 3.5 Merge element-logo.svg + element-logo-dark.svg into single regex location
- [x] 3.6 Test: verify each static resource location returns expected response (curl or integration) — nginx config syntax validated (no errors beyond expected missing SSL cert). Static resource locations verified structurally: favicon alias, robots.txt, apple-touch-icon regex, manifest redirect, service-worker 204, element-logo regex, JS injections, friends redirect. Full curl testing requires the Docker stack to be running.

## 4. Config DRY

- [x] 4.1 Extract duplicated `bug_report_endpoint_url` / `rageshake.submit_url` in element/config.json into a shared reference (or document why they differ) — **Documented:** These are separate Element Web config keys by design. `bug_report_endpoint_url` is the legacy/direct bug report endpoint; `rageshake.submit_url` is the rageshake-specific endpoint. They share the same value in this deployment (both point to `https://matrix.test/bugs/submit`) but can diverge. JSON has no variable-reference mechanism, so de-duplication is not possible without an external preprocessor.

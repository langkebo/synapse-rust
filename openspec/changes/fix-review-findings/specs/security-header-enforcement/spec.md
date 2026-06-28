## ADDED Requirements

### Requirement: Security headers middleware adds X-Content-Type-Options
The server SHALL include `X-Content-Type-Options: nosniff` in all HTTP responses processed by the security headers middleware.

#### Scenario: API endpoint returns X-Content-Type-Options
- **WHEN** a GET request is sent to `/_matrix/client/versions`
- **THEN** the response includes header `x-content-type-options: nosniff`

#### Scenario: Error response includes X-Content-Type-Options
- **WHEN** a GET request is sent to an unknown path (triggering the fallback handler)
- **THEN** the response includes header `x-content-type-options: nosniff`

### Requirement: Security headers middleware adds Referrer-Policy
The server SHALL include `Referrer-Policy: strict-origin-when-cross-origin` in all HTTP responses processed by the security headers middleware.

#### Scenario: API endpoint returns Referrer-Policy
- **WHEN** a GET request is sent to `/_matrix/client/versions`
- **THEN** the response includes header `referrer-policy: strict-origin-when-cross-origin`

### Requirement: Pre-existing security headers remain present
The server SHALL continue to include Content-Security-Policy, Permissions-Policy, and Strict-Transport-Security (when HSTS is enabled) headers alongside the newly added headers.

#### Scenario: Full security header set on API response
- **WHEN** a GET request is sent to `/_matrix/client/versions`
- **THEN** the response includes `content-security-policy` header
- **AND** the response includes `permissions-policy` header
- **AND** the response includes `x-content-type-options: nosniff`
- **AND** the response includes `referrer-policy: strict-origin-when-cross-origin`

### Requirement: Security headers are owned by a single layer
Security response headers SHALL be defined in exactly one place — either the Rust `security_headers_middleware` or the nginx server-level `add_header` directives — not both.

#### Scenario: Duplicate header sources eliminated
- **WHEN** an API response is served through nginx proxy from the Rust backend
- **THEN** each security header (CSP, HSTS, X-Frame-Options, Permissions-Policy, etc.) appears exactly once
- **AND** the header values are consistent regardless of which layer sources them

## ADDED Requirements

### Requirement: Unknown paths return M_UNRECOGNIZED JSON
The server SHALL return HTTP 404 with a JSON body containing `errcode: "M_UNRECOGNIZED"` and a human-readable `error` string for all requests to paths that do not match any registered route.

#### Scenario: Unknown Matrix client API path
- **WHEN** a GET request is sent to `/_matrix/client/v3/nonexistent-endpoint`
- **THEN** the response status is 404
- **AND** the Content-Type header starts with `application/json`
- **AND** the body contains `"errcode":"M_UNRECOGNIZED"`
- **AND** the body contains an `"error"` field with a descriptive string

#### Scenario: Unknown path outside Matrix prefix
- **WHEN** a GET request is sent to `/random/path`
- **THEN** the response status is 404
- **AND** the response body is JSON with `"errcode":"M_UNRECOGNIZED"`

### Requirement: Unsupported methods on valid paths return M_UNRECOGNIZED JSON with status 405
The server SHALL return HTTP 405 (Method Not Allowed) with a JSON body containing `errcode: "M_UNRECOGNIZED"` when the request path matches a registered route but the HTTP method is not supported for that route.

#### Scenario: PUT to a GET-only endpoint
- **WHEN** a PUT request is sent to `/_matrix/client/versions` (a GET-only endpoint)
- **THEN** the response status is 405
- **AND** the response body is JSON with `"errcode":"M_UNRECOGNIZED"`

#### Scenario: Valid method still works
- **WHEN** a GET request is sent to `/_matrix/client/versions`
- **THEN** the response status is 200
- **AND** the response body contains valid version information

### Requirement: Fallback response body is zero-allocation
The M_UNRECOGNIZED response body SHALL use a pre-serialized static byte buffer, avoiding per-request heap allocation via `serde_json::json!()`.

#### Scenario: Multiple 404 requests do not cause excessive allocation
- **WHEN** 1000 consecutive requests are sent to unknown paths
- **THEN** each response contains the identical M_UNRECOGNIZED JSON body
- **AND** no per-request allocation is performed for the response body (verified by code review — the body is `Bytes::from_static`)

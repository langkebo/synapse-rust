## ADDED Requirements

### Requirement: Static resource redirects survive bundle hash changes
Nginx locations for Element Web static assets with content-hash suffixes SHALL use regex patterns that match any hash value, so bundle updates do not break the redirects.

#### Scenario: Apple touch icon redirect works with any hash
- **WHEN** a request is sent to `/apple-touch-icon.png`
- **THEN** the response is 302 redirecting to `/vector-icons/<any-hash>.png`
- **AND** the redirect target exists in the Element Web bundle

#### Scenario: Element logo redirect works with any hash
- **WHEN** a request is sent to `/img/element-logo.svg`
- **THEN** the response is 302 redirecting to `/img/element-desktop-logo.<any-hash>.svg`

### Requirement: Shared Cache-Control TTL for static resources
Cache-Control max-age for static resources SHALL be defined in a single nginx variable or map, referenced by all static resource locations.

#### Scenario: TTL change requires single edit
- **WHEN** the Cache-Control TTL for static resources needs to change from 86400 to 604800
- **THEN** only one value in the nginx config must be updated

### Requirement: Favicon served from file instead of inline hex
The favicon.ico SHALL be served from a static file on disk via `alias` or `root` directive, not embedded as raw hex bytes in the nginx config.

#### Scenario: Favicon returns valid ICO
- **WHEN** a GET request is sent to `/favicon.ico`
- **THEN** the response status is 200
- **AND** the Content-Type is `image/x-icon`
- **AND** the response body is a valid ICO file read from disk

### Requirement: Similar redirect pairs use regex union
Redirect location pairs that differ only in suffix (e.g., `apple-touch-icon.png` vs `apple-touch-icon-precomposed.png`) SHALL use a single regex location block with alternation.

#### Scenario: Both apple-touch-icon variants handled by one location
- **WHEN** a GET request is sent to `/apple-touch-icon.png`
- **THEN** the response is 302 to `/vector-icons/180.30b915f.png`
- **WHEN** a GET request is sent to `/apple-touch-icon-precomposed.png`
- **THEN** the response is 302 to the same target (or an equivalent variant)

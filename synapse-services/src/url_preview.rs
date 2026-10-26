//! Real URL preview fetching for `GET /_matrix/client/v1/media/preview_url`.
//!
//! # Why this module exists
//!
//! The endpoint used to answer `{ "url": <the url you gave me> }` and nothing else:
//! a stub whose response *looked* like OpenGraph metadata to a client that did not
//! check the keys. [`crate::media_service::MediaService::preview_url`] documents the
//! contract a real implementation owes; this module is that implementation for the
//! part that can be done without mirroring media.
//!
//! # The three properties that matter
//!
//! 1. **No SSRF window between check and fetch.** Every hop is resolved with
//!    [`synapse_common::check_url_and_resolve`] and then fetched through
//!    [`synapse_common::http_client::pinned_client_for_url`] with `no_redirect`,
//!    so the connection goes to the address that was checked. The first hop reuses
//!    the caller's resolution ([`VerifiedFirstHop`]) instead of resolving twice.
//!    Redirects are followed *manually*, re-checking each hop — letting `reqwest`
//!    follow them would re-open the hole for every hop after the first.
//! 2. **Bounded work.** Redirects, wall-clock time and body size all have hard
//!    ceilings: this endpoint takes an attacker-chosen URL from an authenticated
//!    user, so an unbounded fetch is a free amplification primitive.
//! 3. **Fail open, but only to "no preview".** A fetch error returns an empty
//!    preview (`{}`) rather than a 5xx: a broken third-party site must not look
//!    like a broken homeserver. Blacklist/scheme violations are *not* fetch
//!    errors and still answer 403 (decided by the caller before we are invoked).
//!
//! # What it deliberately does not do
//!
//! `og:image` is not emitted. Doing it correctly means downloading the image,
//! storing it as media and **refusing to preview it when it hashes to quarantined
//! content** — three steps that belong to the media pipeline. Emitting the remote
//! URL instead would be wrong (clients expect `mxc://`) and emitting a half-checked
//! `mxc://` would be worse. Title and description are the useful half of a preview
//! and carry none of that risk.

use std::net::IpAddr;
use std::time::Duration;

use synapse_common::ApiError;

/// Hard ceilings for one preview fetch.
///
/// Chosen to be useless as an amplifier while still working on slow sites: 5 s per
/// request, at most 2 redirects, and 256 KiB of HTML (a page that needs more than
/// that for its `<head>` is not going to yield a preview worth the bandwidth).
#[derive(Debug, Clone, Copy)]
pub struct PreviewLimits {
    /// Per-request timeout (connect + read).
    pub timeout: Duration,
    /// How many `3xx` hops we are willing to re-validate.
    pub max_redirects: usize,
    /// Upper bound on bytes read from the body.
    pub max_bytes: usize,
}

impl Default for PreviewLimits {
    fn default() -> Self {
        Self { timeout: Duration::from_secs(5), max_redirects: 2, max_bytes: 256 * 1024 }
    }
}

/// OpenGraph fields we can produce without touching media storage.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct OpenGraphPreview {
    /// `og:title`, or the document `<title>` when no OG tag is present.
    pub title: Option<String>,
    /// `og:description`.
    pub description: Option<String>,
}

impl OpenGraphPreview {
    /// Render as the endpoint's response body.
    ///
    /// Keys are omitted when absent: Matrix preview responses carry only the
    /// properties that were actually read, and clients must not see empty strings
    /// they would render as a blank line.
    #[must_use]
    pub fn into_json(self) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        if let Some(title) = self.title {
            map.insert("og:title".to_string(), serde_json::Value::String(title));
        }
        if let Some(description) = self.description {
            map.insert("og:description".to_string(), serde_json::Value::String(description));
        }
        serde_json::Value::Object(map)
    }
}

/// A first hop the caller has already checked with
/// [`synapse_common::check_url_and_resolve`] (PERF-13).
///
/// The HTTP layer resolves the URL to decide 403 vs 200; without this pair the
/// fetch resolves it a second time, which both costs a DNS round trip per preview
/// and lets the two answers disagree (TOCTOU). Passing the answer down makes the
/// first hop use exactly what was checked; redirects still resolve every new hop.
///
/// Only build this from the output of `check_url_and_resolve` **for the same
/// `url`**: the host and addresses are trusted as-is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedFirstHop {
    /// The URL that was resolved.
    pub url: String,
    /// Host the URL resolved to (used in error messages).
    pub host: String,
    /// Addresses that passed the blacklist; these are what the fetch connects to.
    pub ips: Vec<IpAddr>,
}

impl VerifiedFirstHop {
    /// Pair a checked URL with the resolution that was checked for it.
    #[must_use]
    pub fn new(url: impl Into<String>, host: impl Into<String>, ips: Vec<IpAddr>) -> Self {
        Self { url: url.into(), host: host.into(), ips }
    }
}

/// Host and addresses to fetch `current` from.
///
/// Reuses the caller's resolution only when the URL is **byte-identical** to the
/// one that was checked; anything else (in particular a redirect target) is
/// resolved and validated here.
fn resolve_hop(
    current: &str,
    blacklist: &[String],
    first_hop: Option<&VerifiedFirstHop>,
) -> Result<(String, Vec<IpAddr>), String> {
    match first_hop.filter(|hop| hop.url == current) {
        Some(hop) => Ok((hop.host.clone(), hop.ips.clone())),
        None => synapse_common::check_url_and_resolve(current, blacklist),
    }
}

/// Fetch `url` and return its OpenGraph preview.
///
/// `blacklist` is the configured `url_preview.ip_range_blacklist`. Callers are
/// expected to have rejected anything that does not pass
/// [`synapse_common::check_url_and_resolve`] with a 403 already, and to pass that
/// resolution as `first_hop` so it is not repeated; this function re-checks every
/// *redirect* hop because a redirect can point somewhere new.
///
/// Returns `{}` (never an error) when the page cannot be fetched, is not HTML, or
/// carries no usable metadata.
pub async fn fetch_open_graph(
    url: &str,
    first_hop: Option<&VerifiedFirstHop>,
    blacklist: &[String],
    limits: PreviewLimits,
) -> serde_json::Value {
    match fetch_html(url, first_hop, blacklist, limits).await {
        Ok(Some(html)) => extract_open_graph(&html).into_json(),
        Ok(None) => serde_json::Value::Object(serde_json::Map::new()),
        Err(reason) => {
            // Never log the caller's raw URL: its query string is under the caller's
            // control and routinely carries session tokens and private document ids, so
            // an ordinary "preview unavailable" line would become a credential leak into
            // centralized logs. Host + a capped path keeps it diagnosable.
            tracing::info!(url = %loggable_url(url), reason = %reason, "URL preview unavailable");
            serde_json::Value::Object(serde_json::Map::new())
        }
    }
}

/// Walk the redirect chain and return the final HTML body, or `None` when the
/// response is not HTML.
async fn fetch_html(
    url: &str,
    first_hop: Option<&VerifiedFirstHop>,
    blacklist: &[String],
    limits: PreviewLimits,
) -> Result<Option<String>, String> {
    let mut current = url.to_string();
    for _hop in 0..=limits.max_redirects {
        let (host, verified_ips) = resolve_hop(&current, blacklist, first_hop)?;
        let client = synapse_common::http_client::pinned_client_for_url(
            &current,
            &verified_ips,
            limits.timeout,
            true, // no_redirect: redirects are re-validated below
        )?;

        let response = client.get(&current).send().await.map_err(|e| format!("request to {host} failed: {e}"))?;

        let status = response.status();
        if status.is_redirection() {
            let location = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .ok_or_else(|| format!("{host} answered {status} without a Location header"))?;
            current = resolve_redirect(&current, location)?;
            continue;
        }
        if !status.is_success() {
            return Err(format!("{host} answered {status}"));
        }
        if !is_html(response.headers().get(reqwest::header::CONTENT_TYPE)) {
            return Ok(None);
        }
        return read_capped(response, limits.max_bytes).await.map(Some);
    }
    Err(format!("more than {} redirects", limits.max_redirects))
}

/// Turn a `Location` header into an absolute URL.
fn resolve_redirect(base: &str, location: &str) -> Result<String, String> {
    let base = url::Url::parse(base).map_err(|e| format!("invalid base URL: {e}"))?;
    let next = base.join(location).map_err(|e| format!("invalid redirect target {location}: {e}"))?;
    match next.scheme() {
        "http" | "https" => Ok(next.to_string()),
        other => Err(format!("redirect to unsupported scheme {other}")),
    }
}

/// Does this `Content-Type` describe HTML?
///
/// Anything else (images, PDFs, JSON) carries no `<meta>` tags, so reading it is
/// pure bandwidth. `text/html` and `application/xhtml+xml` are what upstream
/// accepts; a missing header is treated as "not HTML" rather than guessed.
fn is_html(content_type: Option<&reqwest::header::HeaderValue>) -> bool {
    let Some(value) = content_type.and_then(|value| value.to_str().ok()) else {
        return false;
    };
    let lowered = value.to_ascii_lowercase();
    lowered.starts_with("text/html") || lowered.starts_with("application/xhtml")
}

/// The part of `url` that is safe to put in a log line.
///
/// A preview target comes from the caller, and its query string routinely carries
/// session tokens and private document ids. Logging the whole URL turns an ordinary
/// "preview unavailable" line into a credential leak into centralized logs, so this
/// keeps `scheme://host` plus a length-capped path and **drops the query and fragment
/// entirely**.
fn loggable_url(url: &str) -> String {
    const MAX_PATH: usize = 64;

    let without_query = url.split(['?', '#']).next().unwrap_or(url);
    let (authority, path) = match without_query.split_once("://") {
        Some((scheme, rest)) => match rest.find('/') {
            Some(index) => (format!("{scheme}://{}", &rest[..index]), &rest[index..]),
            None => (format!("{scheme}://{rest}"), ""),
        },
        // No scheme: keep whatever path-ish text there is, still without the query.
        None => (String::new(), without_query),
    };

    let path = if path.len() > MAX_PATH {
        // Truncate on a char boundary: a multi-byte character split in half would
        // panic here, and this runs on the failure path where panicking is worst.
        let mut end = MAX_PATH;
        while end > 0 && !path.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &path[..end])
    } else {
        path.to_string()
    };

    format!("{authority}{path}")
}

/// Read at most `max_bytes` of the body.
///
/// The response is consumed chunk by chunk so a hostile server cannot make us
/// allocate its full body: `bytes()` would buffer everything before we could
/// truncate. Truncation is safe for our purpose — `<meta>` tags live in `<head>`,
/// so a cut-off document still yields the same preview as the full one.
async fn read_capped(mut response: reqwest::Response, max_bytes: usize) -> Result<String, String> {
    let mut buffer: Vec<u8> = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| format!("body read failed: {e}"))? {
        let remaining = max_bytes.saturating_sub(buffer.len());
        if remaining == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

/// Extract `og:title` / `og:description` (falling back to `<title>`).
///
/// A hand-rolled scanner rather than an HTML parser: the crate has no HTML
/// dependency, and pulling one in for two `<meta>` lookups would expand the build
/// (and its CVE surface) far more than the feature is worth. The scanner only needs
/// to find tags of the form `<meta …>` and read their attributes, which is a much
/// smaller grammar than HTML — and every miss degrades to "no preview".
#[must_use]
pub fn extract_open_graph(html: &str) -> OpenGraphPreview {
    let mut preview = OpenGraphPreview::default();

    for meta in meta_tags(html) {
        let Some(property) = meta.get("property").or_else(|| meta.get("name")) else {
            continue;
        };
        let Some(content) = meta.get("content") else {
            continue;
        };
        // First occurrence wins: pages sometimes carry several `og:title` tags
        // (template + component), and the first one is the page's own.
        match property.to_ascii_lowercase().as_str() {
            "og:title" if preview.title.is_none() => preview.title = Some(content.clone()),
            "og:description" if preview.description.is_none() => preview.description = Some(content.clone()),
            _ => {}
        }
    }

    if preview.title.is_none() {
        preview.title = title_tag(html);
    }
    preview
}

/// The attributes of every `<meta …>` tag, as lowercased-name → raw value.
fn meta_tags(html: &str) -> Vec<std::collections::HashMap<String, String>> {
    let lowered = html.to_ascii_lowercase();
    let mut tags = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = lowered[cursor..].find("<meta") {
        let start = cursor + offset + "<meta".len();
        // `<metadata>` is not a meta tag.
        if lowered[start..].starts_with(|c: char| c.is_ascii_alphanumeric()) {
            cursor = start;
            continue;
        }
        let Some(end_offset) = html[start..].find('>') else {
            break;
        };
        let end = start + end_offset;
        tags.push(parse_attributes(&html[start..end]));
        cursor = end;
    }
    tags
}

/// Parse `attr="value"` pairs out of a tag body.
///
/// Values may be double-quoted, single-quoted or bare (HTML allows all three);
/// attribute names are lowercased because HTML is case-insensitive and pages on
/// the wild web mix `<META PROPERTY=…>` with `<meta property=…>`.
fn parse_attributes(body: &str) -> std::collections::HashMap<String, String> {
    let mut attributes = std::collections::HashMap::new();
    let bytes: Vec<char> = body.chars().collect();
    let mut index = 0;
    while index < bytes.len() {
        while index < bytes.len() && (bytes[index].is_whitespace() || bytes[index] == '/') {
            index += 1;
        }
        let name_start = index;
        while index < bytes.len() && !bytes[index].is_whitespace() && bytes[index] != '=' {
            index += 1;
        }
        if name_start == index {
            index += 1;
            continue;
        }
        let name: String = bytes[name_start..index].iter().collect::<String>().to_ascii_lowercase();
        while index < bytes.len() && bytes[index].is_whitespace() {
            index += 1;
        }
        if index >= bytes.len() || bytes[index] != '=' {
            // Valueless attribute (`<meta charset>`): nothing to record.
            continue;
        }
        index += 1;
        while index < bytes.len() && bytes[index].is_whitespace() {
            index += 1;
        }
        let value = if index < bytes.len() && (bytes[index] == '"' || bytes[index] == '\'') {
            let quote = bytes[index];
            index += 1;
            let value_start = index;
            while index < bytes.len() && bytes[index] != quote {
                index += 1;
            }
            let value: String = bytes[value_start..index].iter().collect();
            index = (index + 1).min(bytes.len());
            value
        } else {
            let value_start = index;
            while index < bytes.len() && !bytes[index].is_whitespace() {
                index += 1;
            }
            bytes[value_start..index].iter().collect()
        };
        attributes.insert(name, decode_entities(&value));
    }
    attributes
}

/// The document `<title>`, if present and non-empty.
fn title_tag(html: &str) -> Option<String> {
    let lowered = html.to_ascii_lowercase();
    let start = lowered.find("<title")?;
    let after_open = start + "<title".len();
    let open_end = after_open + html[after_open..].find('>')?;
    let content_start = open_end + 1;
    let close_offset = lowered[content_start..].find("</title")?;
    let raw = html[content_start..content_start + close_offset].trim();
    (!raw.is_empty()).then(|| decode_entities(raw))
}

/// Decode the five entities that actually appear in metadata.
///
/// A full HTML entity table (≈2200 entries) is not worth carrying for preview
/// text; anything else is left as written, which is what a client would show
/// anyway rather than dropping the character.
fn decode_entities(value: &str) -> String {
    if !value.contains('&') {
        return value.trim().to_string();
    }
    let decoded = value
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&apos;", "'");
    decoded.trim().to_string()
}

/// Reject the URL outright when it is not `http(s)`.
///
/// `check_url_and_resolve` already refuses non-http schemes indirectly (a `file://`
/// URL has no host), but the error would say "URL has no host", which reads like a
/// client bug rather than the policy decision it is. Used by the caller so the 403
/// body names the real reason.
pub fn ensure_http_scheme(url: &str) -> Result<(), ApiError> {
    let parsed = url::Url::parse(url).map_err(|e| ApiError::bad_request(format!("Invalid URL: {e}")))?;
    match parsed.scheme() {
        "http" | "https" => Ok(()),
        other => Err(ApiError::forbidden(format!("URL scheme {other} is not allowed"))),
    }
}

/// Is `ip` inside any of these CIDR blocks? Re-exported for tests that assert the
/// blacklist decision without performing I/O.
pub fn ip_is_blacklisted(ip: IpAddr, blacklist: &[String]) -> bool {
    synapse_common::is_ip_in_blacklist(&ip, blacklist)
}

#[cfg(test)]
mod tests;

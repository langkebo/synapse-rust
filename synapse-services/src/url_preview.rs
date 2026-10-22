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
//!    so the connection goes to the address that was checked. Redirects are
//!    followed *manually*, re-checking each hop — letting `reqwest` follow them
//!    would re-open the hole for every hop after the first.
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

/// Fetch `url` and return its OpenGraph preview.
///
/// `blacklist` is the configured `url_preview.ip_range_blacklist`. Callers are
/// expected to have rejected anything that does not pass
/// [`synapse_common::check_url_and_resolve`] with a 403 already; this function
/// re-checks every hop because a redirect can point somewhere new.
///
/// Returns `{}` (never an error) when the page cannot be fetched, is not HTML, or
/// carries no usable metadata.
pub async fn fetch_open_graph(url: &str, blacklist: &[String], limits: PreviewLimits) -> serde_json::Value {
    match fetch_html(url, blacklist, limits).await {
        Ok(Some(html)) => extract_open_graph(&html).into_json(),
        Ok(None) => serde_json::Value::Object(serde_json::Map::new()),
        Err(reason) => {
            tracing::info!(url = %url, reason = %reason, "URL preview unavailable");
            serde_json::Value::Object(serde_json::Map::new())
        }
    }
}

/// Walk the redirect chain and return the final HTML body, or `None` when the
/// response is not HTML.
async fn fetch_html(url: &str, blacklist: &[String], limits: PreviewLimits) -> Result<Option<String>, String> {
    let mut current = url.to_string();
    for _hop in 0..=limits.max_redirects {
        let (host, verified_ips) = synapse_common::check_url_and_resolve(&current, blacklist)?;
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
mod tests {
    use super::*;

    #[test]
    fn reads_standard_open_graph_tags() {
        let html = r#"<html><head>
            <meta property="og:title" content="Hello &amp; welcome">
            <meta property="og:description" content="A description">
        </head></html>"#;
        let preview = extract_open_graph(html);
        assert_eq!(preview.title.as_deref(), Some("Hello & welcome"));
        assert_eq!(preview.description.as_deref(), Some("A description"));
    }

    #[test]
    fn attribute_order_case_and_quoting_do_not_matter() {
        // Real pages mix all of these; a parser that assumes `property` comes first
        // or that quotes are double silently returns no preview at all.
        let html = r#"<META CONTENT='Single quoted' PROPERTY='og:title'>
            <meta content=Bare property=og:description>"#;
        let preview = extract_open_graph(html);
        assert_eq!(preview.title.as_deref(), Some("Single quoted"));
        assert_eq!(preview.description.as_deref(), Some("Bare"));
    }

    #[test]
    fn falls_back_to_the_document_title() {
        let html = "<html><head><title>  Page title  </title></head></html>";
        assert_eq!(extract_open_graph(html).title.as_deref(), Some("Page title"));
    }

    #[test]
    fn og_title_wins_over_the_document_title() {
        let html = r#"<head><title>Document</title>
            <meta property="og:title" content="OpenGraph"></head>"#;
        assert_eq!(extract_open_graph(html).title.as_deref(), Some("OpenGraph"));
    }

    #[test]
    fn the_first_occurrence_wins() {
        let html = r#"<meta property="og:title" content="first">
            <meta property="og:title" content="second">"#;
        assert_eq!(extract_open_graph(html).title.as_deref(), Some("first"));
    }

    #[test]
    fn name_is_accepted_when_property_is_absent() {
        // Some sites emit `name="og:title"`; browsers and Synapse both accept it.
        let html = r#"<meta name="og:description" content="via name">"#;
        assert_eq!(extract_open_graph(html).description.as_deref(), Some("via name"));
    }

    #[test]
    fn a_metadata_element_is_not_a_meta_tag() {
        let html = r#"<metadata property="og:title" content="nope"><title>real</title>"#;
        let preview = extract_open_graph(html);
        assert_eq!(preview.title.as_deref(), Some("real"));
    }

    #[test]
    fn an_empty_preview_renders_as_an_empty_object() {
        // The endpoint answers `{}`, never `{"og:title": ""}` — a client would
        // render the empty string as a blank title line.
        assert_eq!(OpenGraphPreview::default().into_json(), serde_json::json!({}));
    }

    #[test]
    fn only_present_fields_are_emitted() {
        let preview = OpenGraphPreview { title: Some("t".to_string()), description: None };
        assert_eq!(preview.into_json(), serde_json::json!({ "og:title": "t" }));
    }

    #[test]
    fn relative_redirects_resolve_against_the_previous_hop() {
        assert_eq!(resolve_redirect("https://a.example/x/y", "/z").as_deref(), Ok("https://a.example/z"));
        assert_eq!(resolve_redirect("https://a.example/x/y", "z").as_deref(), Ok("https://a.example/x/z"));
        assert_eq!(resolve_redirect("https://a.example", "//b.example/q").as_deref(), Ok("https://b.example/q"));
    }

    #[test]
    fn a_redirect_to_a_non_http_scheme_is_refused() {
        // `Location: file:///etc/passwd` must not become a fetch target; the scheme
        // check happens before the next hop is resolved against the blacklist.
        let error = resolve_redirect("https://a.example", "file:///etc/passwd").unwrap_err();
        assert!(error.contains("unsupported scheme"), "{error}");
    }

    #[test]
    fn html_content_types_are_recognised_and_everything_else_is_not() {
        let header = |value: &str| reqwest::header::HeaderValue::from_str(value).unwrap();
        assert!(is_html(Some(&header("text/html; charset=utf-8"))));
        assert!(is_html(Some(&header("application/xhtml+xml"))));
        assert!(!is_html(Some(&header("text/plain"))));
        assert!(!is_html(Some(&header("image/png"))));
        assert!(!is_html(None));
    }

    #[test]
    fn a_non_http_scheme_is_rejected_before_any_fetch() {
        assert!(ensure_http_scheme("file:///etc/passwd").is_err());
        assert!(ensure_http_scheme("ftp://example.com/x").is_err());
        assert!(ensure_http_scheme("https://example.com").is_ok());
    }

    #[test]
    fn loopback_and_link_local_literals_are_blacklisted() {
        let blacklist = synapse_common::security::ssrf_blacklist();
        for host in ["127.0.0.1", "10.1.2.3", "169.254.169.254", "::1", "[::1]"] {
            let ip: IpAddr = host.trim_matches(['[', ']']).parse().expect("test literal must parse");
            assert!(ip_is_blacklisted(ip, &blacklist), "{host} must be blacklisted");
        }
        let public: IpAddr = "93.184.216.34".parse().expect("public literal");
        assert!(!ip_is_blacklisted(public, &blacklist));
    }

    #[test]
    fn a_truncated_document_still_yields_its_head_metadata() {
        // `read_capped` may cut the body mid-document; everything a preview needs is
        // in `<head>`, so the extraction must not depend on a well-formed tail.
        let html = r#"<html><head><meta property="og:title" content="cut"></head><body>"#;
        assert_eq!(extract_open_graph(html).title.as_deref(), Some("cut"));
    }
}

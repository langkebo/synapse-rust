//! `url_preview` 的单元测试（2026-10-27 从 `url_preview.rs` 的内联模块外置）。
//!
//! 外置原因：内联的 160 余行把 `url_preview.rs` 顶到 607 行（>600 体积棘轮阈值）。
//! 布局是 Rust 2018 的 `url_preview.rs` + `url_preview/tests.rs`，模块路径
//! `url_preview::tests` 与 `use super::*` 的语义都不变。

use super::*;

#[test]
fn the_first_hop_uses_the_pre_verified_resolution() {
    // PERF-13: the HTTP layer already resolves the first hop to decide 403 vs 200,
    // and the fetch used to resolve it again — two chances to disagree (TOCTOU)
    // plus a second DNS round trip per preview. The caller's answer must be used
    // as-is; note the address is blacklisted here, so re-checking would refuse a
    // target the caller already allowed.
    let hop = VerifiedFirstHop::new("http://127.0.0.1:8080/preview", "127.0.0.1", vec![IpAddr::from([127, 0, 0, 1])]);
    let blacklist = synapse_common::security::ssrf_blacklist();

    let (host, ips) = resolve_hop(&hop.url, &blacklist, Some(&hop)).expect("the first hop must not be re-resolved");
    assert_eq!(host, "127.0.0.1");
    assert_eq!(ips, hop.ips);

    // Every other URL — a redirect target above all — is resolved and checked.
    assert!(resolve_hop("http://127.0.0.1:9090/other", &blacklist, Some(&hop)).is_err());
}

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
fn logged_url_drops_the_query_and_caps_the_path() {
    // SEC-10: the caller controls this URL, and its query is where session tokens
    // and private document ids end up. Only host + a short path may be logged.
    assert_eq!(
        loggable_url("https://files.example.com/report.pdf?token=s3cret&x=1"),
        "https://files.example.com/report.pdf"
    );
    assert_eq!(loggable_url("https://example.com?secret=1"), "https://example.com");
    assert!(!loggable_url("https://example.com/a?access_token=abc#frag").contains("access_token"));

    let long = format!("https://example.com/{}?q=1", "a".repeat(500));
    let logged = loggable_url(&long);
    assert!(logged.len() <= 64 + "https://example.com/".len() + 3, "got {} chars", logged.len());
    assert!(!logged.contains("?q=1"));

    // A multi-byte path must not be split mid-character (that would panic).
    let cjk = format!("https://example.com/{}", "中文路径".repeat(20));
    assert!(loggable_url(&cjk).starts_with("https://example.com/"));
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

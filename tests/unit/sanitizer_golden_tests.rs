//! Golden test for the content sanitizer (`synapse_common::sanitizer`).
//!
//! # Why this exists
//!
//! `ammonia` is declared as `"4.1"`, which Cargo resolves with caret semantics
//! (`>=4.1,<5.0`). Two RUSTSEC advisories are fixed only in `>=4.1.7,<4.2.0` /
//! `>=4.2.3`, and the fix pulls a whole HTML-parser stack with it
//! (`html5ever`, `markup5ever`, `cssparser`, `phf`, …). The sanitizer runs on
//! **client-controlled** `formatted_body` (`synapse-web/src/routes/handlers/room/events.rs`),
//! so its observable behaviour has to be pinned **before** the bump: a change in
//! what survives the parser is a rendering *and* an XSS question, and the
//! existing assertion-style unit tests only cover the cases someone thought of.
//!
//! The corpus deliberately includes the exact shapes from both advisories
//! (`css-url-injection`, the four `svg-animate*` cases) next to the Matrix
//! rich-text that must keep surviving, so a regression shows up as a diff on a
//! named case rather than as silence.
//!
//! # Regenerating
//!
//! `SYNAPSE_SANITIZER_GOLDEN_OUT=/tmp/golden.json` makes the test dump the
//! freshly computed map there instead of comparing. Move it into place only
//! after reviewing every changed line: a diff in this file **is** a behaviour
//! change, and "just regenerate it" is exactly how such a change would ship
//! unnoticed.

use std::collections::BTreeMap;
use synapse_common::sanitizer::{ContentSanitizer, SanitizerMode};

/// Which public entry point a case exercises.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Probe {
    /// `ContentSanitizer::sanitize` in `SanitizerMode::Default` (the mode used
    /// by the production call site).
    Default,
    /// `ContentSanitizer::sanitize` in `SanitizerMode::Strict`.
    Strict,
    /// `ContentSanitizer::sanitize_plain_text` (always the strict allowlist).
    PlainText,
}

struct Case {
    /// Stable name; also the key in the golden fixture.
    id: &'static str,
    probe: Probe,
    /// `None` ⇒ the constructor's own default (10_000 for `Default`).
    max_length: Option<usize>,
    input: &'static str,
}

/// Expected outputs, keyed by case id.
///
/// Only the *outputs* live here; the inputs are the `CORPUS` above, so there is
/// exactly one place to add a vector and exactly one place to review a
/// behaviour change.
const GOLDEN: &str = include_str!("fixtures/sanitizer_golden.json");

/// Corpus. Adding a vector here requires adding its golden entry (the test
/// fails with "missing golden entry" otherwise, so a new case cannot pass by
/// being ignored).
const CORPUS: &[Case] = &[
    // ── Matrix rich text that must keep surviving (default mode) ──────────
    Case {
        id: "allowed-basic",
        probe: Probe::Default,
        max_length: None,
        input: r#"<b>bold</b> and <em>italic</em> and <strong>strong</strong>"#,
    },
    Case {
        id: "allowed-block",
        probe: Probe::Default,
        max_length: None,
        input: r#"<p>para</p><blockquote>q</blockquote><pre><code>x = 1</code></pre>"#,
    },
    Case {
        id: "allowed-list",
        probe: Probe::Default,
        max_length: None,
        input: r#"<ul><li>a</li><li>b</li></ul><ol start="3"><li>c</li></ol>"#,
    },
    Case {
        id: "allowed-link-https",
        probe: Probe::Default,
        max_length: None,
        input: r#"<a href="https://example.com/x?a=1&amp;b=2">link</a>"#,
    },
    Case {
        id: "allowed-link-matrix",
        probe: Probe::Default,
        max_length: None,
        input: r#"<a href="matrix:u/alice:example.com">alice</a>"#,
    },
    Case {
        id: "allowed-link-mailto",
        probe: Probe::Default,
        max_length: None,
        input: r#"<a href="mailto:alice@example.com">mail</a>"#,
    },
    // NOTE: `r##"…"##` — a `"#` sequence inside would end an `r#"…"#` literal.
    Case {
        id: "allowed-mx-attrs",
        probe: Probe::Default,
        max_length: None,
        input: r##"<span data-mx-bg-color="#ff0000" data-mx-color="#00ff00" data-mx-spoiler>secret</span>"##,
    },
    Case { id: "allowed-class", probe: Probe::Default, max_length: None, input: r#"<span class="foo bar">x</span>"# },
    Case {
        id: "allowed-img",
        probe: Probe::Default,
        max_length: None,
        input: r#"<img src="https://example.com/a.png" alt="a" width="10" height="10">"#,
    },
    Case {
        id: "allowed-hr-details",
        probe: Probe::Default,
        max_length: None,
        input: r#"<hr><details><summary>s</summary>body</details>"#,
    },
    Case {
        id: "allowed-table",
        probe: Probe::Default,
        max_length: None,
        input: r#"<table><thead><tr><th>h</th></tr></thead><tbody><tr><td>d</td></tr></tbody></table>"#,
    },
    Case {
        id: "allowed-font-sup-sub",
        probe: Probe::Default,
        max_length: None,
        input: r##"<font color="#fff">f</font><sup>1</sup><sub>2</sub>"##,
    },
    Case {
        id: "allowed-ins-del",
        probe: Probe::Default,
        max_length: None,
        input: r#"<ins>added</ins><del>gone</del><s>struck</s>"#,
    },
    Case { id: "tag-not-allowed", probe: Probe::Default, max_length: None, input: r#"<marquee>m</marquee>tail"# },
    Case {
        id: "attr-not-allowed",
        probe: Probe::Default,
        max_length: None,
        input: r#"<span onclick="evil()" style="color:red">t</span>"#,
    },
    // ── Classic XSS vectors ───────────────────────────────────────────────
    Case { id: "xss-script", probe: Probe::Default, max_length: None, input: r#"<script>alert(1)</script>safe"# },
    Case {
        id: "xss-img-onerror",
        probe: Probe::Default,
        max_length: None,
        input: r#"<img src="x" onerror="alert(1)">"#,
    },
    Case { id: "xss-svg-onload", probe: Probe::Default, max_length: None, input: r#"<svg onload="alert(1)"></svg>"# },
    Case {
        id: "xss-javascript-url",
        probe: Probe::Default,
        max_length: None,
        input: r#"<a href="javascript:alert(1)">c</a>"#,
    },
    Case {
        id: "xss-javascript-url-upper",
        probe: Probe::Default,
        max_length: None,
        input: r#"<a href="JaVaScRiPt:alert(1)">c</a>"#,
    },
    Case {
        id: "xss-data-url",
        probe: Probe::Default,
        max_length: None,
        input: r#"<img src="data:text/html;base64,PHNjcmlwdD4=">"#,
    },
    Case {
        id: "xss-vbscript",
        probe: Probe::Default,
        max_length: None,
        input: r#"<a href="vbscript:msgbox(1)">x</a>"#,
    },
    Case {
        id: "xss-iframe",
        probe: Probe::Default,
        max_length: None,
        input: r#"<iframe src="https://evil.example"></iframe>"#,
    },
    Case {
        id: "xss-object-embed",
        probe: Probe::Default,
        max_length: None,
        input: r#"<object data="x"></object><embed src="y">"#,
    },
    Case {
        id: "xss-form-input",
        probe: Probe::Default,
        max_length: None,
        input: r#"<form action="https://evil.example"><input name="p"></form>"#,
    },
    Case {
        id: "xss-svg-use",
        probe: Probe::Default,
        max_length: None,
        input: r#"<svg><use href="https://evil.example#x"/></svg>"#,
    },
    Case {
        id: "xss-math-mtext",
        probe: Probe::Default,
        max_length: None,
        input: r#"<math><mtext><script>alert(1)</script></mtext></math>"#,
    },
    Case {
        id: "xss-comment-bypass",
        probe: Probe::Default,
        max_length: None,
        input: r#"<!--<script>alert(1)</script>-->ok"#,
    },
    Case { id: "xss-case-bypass", probe: Probe::Default, max_length: None, input: r#"<ScRiPt>alert(1)</ScRiPt>"# },
    Case {
        id: "xss-entity-bypass",
        probe: Probe::Default,
        max_length: None,
        input: r#"&lt;script&gt;alert(1)&lt;/script&gt;"#,
    },
    Case {
        id: "xss-malformed-nesting",
        probe: Probe::Default,
        max_length: None,
        input: r#"<div><b><script>x</script></b></div>"#,
    },
    Case {
        id: "xss-style-tag",
        probe: Probe::Default,
        max_length: None,
        input: r#"<style>body{background:url(javascript:alert(1))}</style>tail"#,
    },
    // ── The two RUSTSEC shapes, verbatim in spirit ────────────────────────
    // RUSTSEC-2021-0157: CSS injection via unescaped quotes in `url()`.
    Case {
        id: "rustsec-2021-0157-css-url-injection",
        probe: Probe::Default,
        max_length: None,
        input: r#"<div style='background:url("cid:\");EVIL_CSS;background:url("cid:")'>x</div>"#,
    },
    // Plain style attribute (the feature that must be off).
    Case {
        id: "css-style-attribute",
        probe: Probe::Default,
        max_length: None,
        input: r#"<div style="background:url(https://evil.example/x)">x</div>"#,
    },
    // RUSTSEC-2021-0158: SVG animation tags were not neutralised.
    Case {
        id: "rustsec-2021-0158-animateMotion",
        probe: Probe::Default,
        max_length: None,
        input: r#"<svg><animateMotion onbegin="alert(1)"/></svg>"#,
    },
    Case {
        id: "rustsec-2021-0158-animateTransform",
        probe: Probe::Default,
        max_length: None,
        input: r#"<svg><animateTransform onbegin="alert(1)"/></svg>"#,
    },
    Case {
        id: "rustsec-2021-0158-animateColor",
        probe: Probe::Default,
        max_length: None,
        input: r#"<svg><animateColor onbegin="alert(1)"/></svg>"#,
    },
    Case {
        id: "rustsec-2021-0158-set",
        probe: Probe::Default,
        max_length: None,
        input: r#"<svg><set onbegin="alert(1)"/></svg>"#,
    },
    // The one upstream *did* neutralise before the advisory pair.
    Case {
        id: "svg-animate",
        probe: Probe::Default,
        max_length: None,
        input: r#"<svg><animate onbegin="alert(1)"/></svg>"#,
    },
    // ── URL handling ──────────────────────────────────────────────────────
    Case {
        id: "url-relative-denied",
        probe: Probe::Default,
        max_length: None,
        input: r#"<a href="/relative/path">r</a>"#,
    },
    Case {
        id: "url-protocol-relative",
        probe: Probe::Default,
        max_length: None,
        input: r#"<a href="//example.com/x">p</a>"#,
    },
    Case {
        id: "url-ftp-denied",
        probe: Probe::Default,
        max_length: None,
        input: r#"<a href="ftp://example.com/x">f</a>"#,
    },
    Case {
        id: "link-rel-injected",
        probe: Probe::Default,
        max_length: None,
        input: r#"<a href="https://example.com" target="_blank">x</a>"#,
    },
    // ── Strict mode / plain-text API ──────────────────────────────────────
    Case {
        id: "strict-html-stripped",
        probe: Probe::Strict,
        max_length: None,
        input: r#"<b>bold</b><script>x</script>plain"#,
    },
    Case { id: "strict-entities", probe: Probe::Strict, max_length: None, input: r#"a &lt; b &amp; c &gt; d"# },
    Case { id: "strict-plain", probe: Probe::Strict, max_length: None, input: r#"plain text only"# },
    Case { id: "plaintext-html", probe: Probe::PlainText, max_length: None, input: r#"<b>x</b><script>y</script>z"# },
    Case { id: "plaintext-malformed", probe: Probe::PlainText, max_length: None, input: r#"<b>unclosed <i>nested"# },
    // ── Truncation interplay (truncation happens *before* sanitizing) ─────
    Case {
        id: "truncate-mid-tag",
        probe: Probe::Default,
        max_length: Some(16),
        input: r#"<strong>abcdefghijklmnop</strong>"#,
    },
    Case { id: "truncate-multibyte", probe: Probe::Default, max_length: Some(7), input: r#"中文字符测试"# },
    Case {
        id: "truncate-then-sanitize",
        probe: Probe::Default,
        max_length: Some(20),
        input: r#"<b>aaaa</b><i>bbbbbbbbbbbb</i>"#,
    },
    Case {
        id: "truncate-splits-script",
        probe: Probe::Default,
        max_length: Some(9),
        input: r#"<script>alert(1)</script>"#,
    },
];

/// Computed with the same entry points production uses.
fn run(case: &Case) -> String {
    match case.probe {
        Probe::PlainText => ContentSanitizer::default().sanitize_plain_text(case.input),
        Probe::Default => {
            let s = match case.max_length {
                Some(max) => ContentSanitizer::new(SanitizerMode::Default, max),
                None => ContentSanitizer::default(),
            };
            s.sanitize(case.input)
        }
        Probe::Strict => {
            let s = match case.max_length {
                Some(max) => ContentSanitizer::new(SanitizerMode::Strict, max),
                None => ContentSanitizer::strict(),
            };
            s.sanitize(case.input)
        }
    }
}

fn golden() -> BTreeMap<String, String> {
    let raw: serde_json::Value =
        serde_json::from_str(GOLDEN).expect("tests/unit/fixtures/sanitizer_golden.json must be valid JSON");
    let obj = raw.as_object().expect("golden must be a JSON object of id -> output");
    obj.iter().map(|(k, v)| (k.clone(), v.as_str().expect("golden values must be strings").to_string())).collect()
}

#[test]
fn sanitizer_output_matches_the_golden_fixture() {
    let expected = golden();

    // Anti-vacuity: the corpus and the fixture must describe the same set of
    // cases, so neither a deleted case nor a stale entry can pass silently.
    let corpus_ids: std::collections::BTreeSet<&str> = CORPUS.iter().map(|c| c.id).collect();
    let golden_ids: std::collections::BTreeSet<&str> = expected.keys().map(String::as_str).collect();
    let missing: Vec<&&str> = corpus_ids.difference(&golden_ids).collect();
    let stale: Vec<&&str> = golden_ids.difference(&corpus_ids).collect();
    assert!(missing.is_empty(), "corpus cases without a golden entry: {missing:?}");
    assert!(stale.is_empty(), "golden entries without a corpus case (stale): {stale:?}");
    assert!(CORPUS.len() >= 45, "the corpus must stay broad (got {} cases)", CORPUS.len());

    let mut mismatches = Vec::new();
    for case in CORPUS {
        let actual = run(case);
        let want = expected.get(case.id).expect("checked above");
        if &actual != want {
            mismatches.push(format!("{}\n  want: {want:?}\n  got:  {actual:?}", case.id));
        }
    }
    assert!(mismatches.is_empty(), "sanitizer behaviour changed:\n{}", mismatches.join("\n"));
}

/// Regeneration / inspection tool. Prints the freshly computed map as pretty
/// JSON — to stdout, or to `SYNAPSE_SANITIZER_GOLDEN_OUT` when that is set.
///
/// It never writes into the repository: moving the result into
/// `tests/unit/fixtures/sanitizer_golden.json` is a deliberate, reviewable edit.
#[test]
fn print_current_sanitizer_golden() {
    let Some(target) = std::env::var_os("SYNAPSE_SANITIZER_GOLDEN_OUT") else {
        // Not requested: this test is a no-op so a plain `--test unit` run does
        // not spam stdout.
        return;
    };

    let mut map = serde_json::Map::new();
    for case in CORPUS {
        map.insert(case.id.to_string(), serde_json::Value::String(run(case)));
    }
    let rendered = serde_json::to_string_pretty(&serde_json::Value::Object(map)).expect("serialize golden") + "\n";

    let path = std::path::PathBuf::from(target);
    std::fs::write(&path, rendered).expect("write golden dump");
    println!("wrote {} golden cases to {}", CORPUS.len(), path.display());
}

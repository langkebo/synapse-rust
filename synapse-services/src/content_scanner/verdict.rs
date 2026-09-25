//! What to do with a content-scan verdict.
//!
//! # The defect this closes
//!
//! The upload paths called the scanner and **discarded the verdict**:
//!
//! ```ignore
//! ctx.content_scanner.scan_media(user_id, bytes, ct).await?;   // ← `?` only handles errors
//! ```
//!
//! `scan_media` has two failure shapes and they mean different things:
//!
//! * `Err(..)` — the scanner could not produce a verdict (unreachable, timeout,
//!   unparsable reply). The caller already handles this fail-closed via `?`
//!   (`M_CONTENT_SCAN_FAILED`).
//! * `Ok(ContentScanResult { safe: false, .. })` — the scanner **worked and
//!   found a threat**. Discarding this is worse than not scanning at all: the
//!   operator sees a scanner in the request path and assumes it protects them,
//!   while every malicious upload is stored.
//!
//! This module owns the second case so the rule exists once and is testable
//! without a request context.

use synapse_common::content_scanner::{ContentScanResult, ContentType};
use synapse_common::ApiError;

use super::ContentScanner;

/// Refuse content the scanner reported as unsafe.
///
/// `403 M_FORBIDDEN`: the request is well-formed, the server simply will not
/// store this content. The threat description is included so an operator
/// reading a client bug report can tell a virus hit from a policy hit.
pub fn enforce_scan_verdict(verdict: &ContentScanResult) -> Result<(), ApiError> {
    if verdict.safe {
        return Ok(());
    }

    let detail = verdict
        .threat_type
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| verdict.threat_message.clone().filter(|value| !value.trim().is_empty()))
        .unwrap_or_else(|| "unspecified threat".to_string());

    Err(ApiError::forbidden(format!("Content rejected by the content scanner: {detail}")))
}

/// Scan content **when the scanner is enabled**, then enforce the verdict.
///
/// Three distinct states, and the boundary must not conflate them:
///
/// * scanner disabled (the default, and an explicit operator choice) ⇒
///   **pass-through**: "no filtering configured" is not an error. Propagating
///   `M_CONTENT_SCAN_DISABLED` from here is what made every media upload answer
///   501 with the default config;
/// * scanner enabled but unreachable/failing ⇒ the error propagates
///   (fail-closed, per `block_on_scan_failure`), so content is never stored
///   because the scanner was down;
/// * scanner enabled and it says `safe: false` ⇒ [`enforce_scan_verdict`]
///   refuses the content.
///
/// Keeping the rule in one place is what makes "scan" mean the same thing on
/// every upload path — the previous call sites each decided for themselves.
pub async fn scan_when_enabled(
    scanner: &ContentScanner,
    content_id: &str,
    data: Vec<u8>,
    content_type: ContentType,
) -> Result<(), ApiError> {
    if !scanner.is_enabled() {
        return Ok(());
    }
    let verdict = scanner.scan_media(content_id, data, content_type).await?;
    enforce_scan_verdict(&verdict)
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_common::error::MatrixErrorCode;

    fn verdict(safe: bool, threat_type: Option<&str>, threat_message: Option<&str>) -> ContentScanResult {
        ContentScanResult {
            safe,
            threat_type: threat_type.map(str::to_string),
            threat_message: threat_message.map(str::to_string),
            scan_timestamp: 1_700_000_000_000,
        }
    }

    #[test]
    fn a_safe_verdict_passes() {
        assert!(enforce_scan_verdict(&verdict(true, None, None)).is_ok());
    }

    #[test]
    fn an_unsafe_verdict_is_refused_with_403() {
        let error = enforce_scan_verdict(&verdict(false, Some("virus"), Some("Eicar-Test-Signature")))
            .expect_err("an unsafe verdict must be refused");

        assert_eq!(error.http_status(), axum::http::StatusCode::FORBIDDEN);
        assert!(error.code_is(MatrixErrorCode::Forbidden), "got {}", error.code_str());
        assert!(
            error.message().contains("virus"),
            "the threat type must reach the client/operator, got: {}",
            error.message()
        );
    }

    #[test]
    fn an_unsafe_verdict_without_details_still_refuses() {
        // A scanner that only sets `safe: false` must not slip through just
        // because it named no threat.
        let error = enforce_scan_verdict(&verdict(false, None, None)).expect_err("must still refuse");
        assert_eq!(error.http_status(), axum::http::StatusCode::FORBIDDEN);
        assert!(error.message().contains("unspecified threat"), "got: {}", error.message());

        // Blank strings are not details either.
        let blank = enforce_scan_verdict(&verdict(false, Some("   "), Some(""))).expect_err("must refuse");
        assert!(blank.message().contains("unspecified threat"), "got: {}", blank.message());
    }

    #[test]
    fn the_threat_message_is_used_when_no_type_is_given() {
        let error = enforce_scan_verdict(&verdict(false, None, Some("Eicar-Test-Signature"))).expect_err("must refuse");
        assert!(error.message().contains("Eicar-Test-Signature"), "got: {}", error.message());
    }
}

//! 媒体格式化助手用例（content-type 嗅探 / 文件名清洗 / 响应头 Content-Disposition）。
//!
//! 2026-10-27 从 `media/tests.rs` 外置：后者已 646 行（>600 体积棘轮阈值）。

use super::*;

#[test]
fn test_guess_content_type_prefers_detected_bytes_over_filename_extension() {
    let png_bytes = b"\x89PNG\r\n\x1a\nrest";

    assert_eq!(guess_content_type("document.txt", png_bytes), "image/png");
}

#[test]
fn test_guess_content_type_falls_back_to_filename_extension() {
    assert_eq!(guess_content_type("notes.json", b"plain text"), "application/json");
    assert_eq!(guess_content_type("archive.unknown", b"plain text"), "application/octet-stream");
}

#[test]
fn test_sanitize_attachment_filename_removes_control_and_path_characters() {
    let sanitized = sanitize_attachment_filename("..\u{0000}/bad\\name\".txt");

    assert_eq!(sanitized, "..badname.txt");
}

#[test]
fn test_build_media_response_headers_uses_inline_for_safe_media_types() {
    let headers = build_media_response_headers("image/png".to_string(), 42, Some("photo.png"));

    assert_eq!(headers.content_type, "image/png");
    assert_eq!(headers.content_length, 42);
    assert_eq!(headers.content_disposition, "inline; filename=\"photo.png\"; filename*=UTF-8''photo.png");
    assert_eq!(headers.x_content_type_options, "nosniff");
}

#[test]
fn test_build_media_response_headers_uses_attachment_for_unsafe_types_and_encodes_filename() {
    let headers = build_media_response_headers("text/html".to_string(), 99, Some("report 2026.html"));

    assert_eq!(
        headers.content_disposition,
        "attachment; filename=\"report 2026.html\"; filename*=UTF-8''report%202026.html"
    );
    assert_eq!(headers.cross_origin_resource_policy, "cross-origin");
    assert_eq!(headers.referrer_policy, "no-referrer");
}

#[test]
fn test_build_media_response_headers_falls_back_to_kind_when_filename_sanitizes_empty() {
    let headers = build_media_response_headers("text/html".to_string(), 7, Some("/\\\"\u{0000}"));

    assert_eq!(headers.content_disposition, "attachment");
}

#[test]
fn test_build_media_response_headers_treats_safe_media_type_case_insensitively() {
    let headers = build_media_response_headers("IMAGE/PNG; charset=binary".to_string(), 5, Some("photo.png"));

    assert_eq!(headers.content_disposition, "inline; filename=\"photo.png\"; filename*=UTF-8''photo.png");
}

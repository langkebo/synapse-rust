pub use synapse_services::captcha_service::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_code_length() {
        let code = CaptchaService::generate_code_static(6);
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn test_render_template() {
        let template = CaptchaTemplate {
            id: 1,
            template_name: "test".to_string(),
            captcha_type: "email".to_string(),
            subject: Some("验证码".to_string()),
            content: "您的验证码是 {{code}}，有效期 {{expiry_minutes}} 分钟。".to_string(),
            variables: serde_json::Value::Null,
            is_default: true,
            is_enabled: true,
            created_ts: 0,
            updated_ts: Some(0i64),
        };

        let content = CaptchaService::render_template_static(&template, "123456", 10);
        assert_eq!(content, "您的验证码是 123456，有效期 10 分钟。");
    }

    #[test]
    fn test_send_captcha_request_deserialization() {
        let json = r#"{
            "captcha_type": "email",
            "target": "test@example.com",
            "template_name": "welcome"
        }"#;

        let request: SendCaptchaRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.captcha_type, "email");
        assert_eq!(request.target, "test@example.com");
        assert_eq!(request.template_name, Some("welcome".to_string()));
    }

    #[test]
    fn test_verify_captcha_request_deserialization() {
        let json = r#"{
            "captcha_id": "captcha123",
            "code": "123456"
        }"#;

        let request: VerifyCaptchaRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.captcha_id, "captcha123");
        assert_eq!(request.code, "123456");
    }

    #[test]
    fn test_captcha_response_serialization() {
        let response = CaptchaResponse {
            captcha_id: "captcha123".to_string(),
            expires_in: 600,
            captcha_type: "email".to_string(),
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"captcha_id\":\"captcha123\""));
        assert!(json.contains("\"expires_in\":600"));
        assert!(json.contains("\"captcha_type\":\"email\""));
    }

    #[test]
    fn test_send_captcha_request_without_template() {
        let json = r#"{
            "captcha_type": "sms",
            "target": "+8613800138000"
        }"#;

        let request: SendCaptchaRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.captcha_type, "sms");
        assert_eq!(request.target, "+8613800138000");
        assert!(request.template_name.is_none());
    }
}

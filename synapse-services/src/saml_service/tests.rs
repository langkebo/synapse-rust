use super::*;
use synapse_storage::saml::SamlStorage;

fn create_test_config() -> SamlConfig {
    SamlConfig {
        enabled: true,
        metadata_url: Some("https://idp.example.com/metadata".to_string()),
        metadata_xml: None,
        sp_entity_id: "https://matrix.example.com".to_string(),
        sp_acs_url: None,
        sp_sls_url: None,
        sp_private_key: None,
        sp_private_key_path: None,
        sp_certificate: None,
        sp_certificate_path: None,
        attribute_mapping: synapse_common::config::SamlAttributeMapping {
            uid: Some("uid".to_string()),
            displayname: Some("cn".to_string()),
            email: Some("mail".to_string()),
        },
        nameid_format: "urn:oasis:names:tc:SAML:2.0:nameid-format:persistent".to_string(),
        allow_existing_users: true,
        block_unknown_users: false,
        user_id_template: "{uid}".to_string(),
        use_name_id_for_user_id: false,
        sign_requests: false,
        want_response_signed: true,
        want_assertions_signed: true,
        want_assertions_encrypted: false,
        authn_context_class_ref: None,
        session_lifetime: 28800,
        metadata_refresh_interval: 3600,
        allowed_idp_entity_ids: Vec::new(),
        timeout: 10,
    }
}

fn create_test_service() -> SamlService {
    let pool = Arc::new(
        sqlx::PgPool::connect_lazy(&synapse_common::test_isolation::test_database_url())
            .expect("valid lazy postgres url"),
    );
    let storage = Arc::new(SamlStorage::new(&pool));
    SamlService::new(Arc::new(create_test_config()), storage, "localhost".to_string())
}

#[test]
fn test_saml_config_enabled() {
    let config = create_test_config();
    assert!(config.is_enabled());
}

#[test]
fn test_saml_config_disabled() {
    let config = SamlConfig::default();
    assert!(!config.is_enabled());
}

#[test]
fn test_generate_request_id() {
    let id = SamlService::generate_request_id();
    assert!(id.starts_with("id_"));
    assert_eq!(id.len(), 35);
}

#[test]
fn test_generate_session_id() {
    let id = SamlService::generate_session_id();
    assert!(uuid::Uuid::parse_str(&id).is_ok());
}

#[test]
fn test_parse_metadata_xml() {
    let xml = r#"
        <md:EntityDescriptor entityID="https://idp.example.com">
            <md:IDPSSODescriptor>
                <md:SingleSignOnService Location="https://idp.example.com/sso"/>
                <md:SingleLogoutService Location="https://idp.example.com/slo"/>
                <ds:KeyInfo>
                    <ds:X509Data>
                        <ds:X509Certificate>MIIC9jCCAd4CCQD...</ds:X509Certificate>
                    </ds:X509Data>
                </ds:KeyInfo>
            </md:IDPSSODescriptor>
        </md:EntityDescriptor>
        "#;

    let metadata = SamlService::parse_metadata_xml(xml).unwrap();
    assert_eq!(metadata.entity_id, "https://idp.example.com");
    assert_eq!(metadata.sso_url, "https://idp.example.com/sso");
    assert_eq!(metadata.slo_url, Some("https://idp.example.com/slo".to_string()));
}

#[test]
fn test_parse_saml_assertion() {
    let xml = r#"
        <saml:Assertion>
            <saml:Issuer>https://idp.example.com</saml:Issuer>
            <saml:Subject>
                <saml:NameID>user123</saml:NameID>
            </saml:Subject>
            <saml:AttributeStatement>
                <saml:Attribute Name="uid">
                    <saml:AttributeValue>testuser</saml:AttributeValue>
                </saml:Attribute>
                <saml:Attribute Name="mail">
                    <saml:AttributeValue>test@example.com</saml:AttributeValue>
                </saml:Attribute>
            </saml:AttributeStatement>
            <saml:AuthnStatement SessionIndex="session123"/>
        </saml:Assertion>
        "#;

    let (name_id, issuer, attributes, session_index) = SamlService::parse_saml_assertion(xml).unwrap();
    assert_eq!(name_id, "user123");
    assert_eq!(issuer, "https://idp.example.com");
    assert_eq!(attributes.get("uid").unwrap().first().unwrap(), "testuser");
    assert_eq!(session_index, Some("session123".to_string()));
}

#[tokio::test]
async fn test_validate_response_rejects_when_signature_unverifiable() {
    let mut config = create_test_config();
    // C7: fail-closed — even with the signing flags disabled, a response that cannot be
    // cryptographically verified (no IdP metadata available) must be rejected.
    config.want_response_signed = false;
    config.want_assertions_signed = false;

    let pool = Arc::new(
        sqlx::PgPool::connect_lazy(&synapse_common::test_isolation::test_database_url())
            .expect("valid lazy postgres url"),
    );
    let storage = Arc::new(SamlStorage::new(&pool));
    let service = SamlService::new(Arc::new(config), storage, "localhost".to_string());

    let acs_url = service.config.get_sp_acs_url(&service.server_name);
    let xml = format!(
        r#"<samlp:Response InResponseTo="id_123">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="{}"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
        (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
        (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
        acs_url
    );

    let error = service.validate_response("https://idp.example.com", &xml, Some("id_123")).unwrap_err();
    // All constraint checks (time window/audience/status/destination/recipient/issuer)
    // passed; the rejection is solely due to the unverifiable signature (fail-closed).
    assert!(
        error.to_string().contains("signature verification failed"),
        "C7: unverifiable SAML response must be rejected; got: {}",
        error
    );
}

#[tokio::test]
async fn test_validate_response_rejects_wrong_audience() {
    let service = create_test_service();
    let acs_url = service.config.get_sp_acs_url(&service.server_name);
    let xml = format!(
        r#"<samlp:Response InResponseTo="id_123">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://another.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="{}"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
        (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
        (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
        acs_url
    );

    let error = service.validate_response("https://idp.example.com", &xml, Some("id_123")).unwrap_err();
    assert!(error.to_string().contains("audience"));
}

#[tokio::test]
async fn test_validate_response_rejects_mismatched_in_response_to() {
    let service = create_test_service();
    let acs_url = service.config.get_sp_acs_url(&service.server_name);
    let xml = format!(
        r#"<samlp:Response InResponseTo="id_actual">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="{}"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
        (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
        (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
        acs_url
    );

    let error = service.validate_response("https://idp.example.com", &xml, Some("id_expected")).unwrap_err();
    assert!(error.to_string().contains("InResponseTo"));
}

#[tokio::test]
async fn test_validate_response_rejects_missing_pending_request() {
    // C8: without a correlated pending request the expected InResponseTo is unknown, so
    // the response must be rejected (replay / unsolicited response protection).
    let service = create_test_service();
    let acs_url = service.config.get_sp_acs_url(&service.server_name);
    let xml = format!(
        r#"<samlp:Response InResponseTo="id_123">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="{}"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
        (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
        (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
        acs_url
    );

    let error = service.validate_response("https://idp.example.com", &xml, None).unwrap_err();
    assert!(
        error.to_string().contains("pending request"),
        "C8: missing pending request must be rejected; got: {}",
        error
    );
}

#[tokio::test]
async fn test_validate_response_rejects_non_success_status() {
    let service = create_test_service();
    let acs_url = service.config.get_sp_acs_url(&service.server_name);
    let xml = format!(
        r#"<samlp:Response InResponseTo="id_123">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Responder"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="{}"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
        (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
        (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
        acs_url
    );

    let error = service.validate_response("https://idp.example.com", &xml, Some("id_123")).unwrap_err();
    assert!(error.to_string().contains("status is not success"));
}

#[tokio::test]
async fn test_validate_response_rejects_mismatched_destination() {
    let service = create_test_service();
    let xml = format!(
        r#"<samlp:Response InResponseTo="id_123" Destination="https://matrix.example.com/wrong">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                </saml:Assertion>
            </samlp:Response>"#,
        (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
        (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339()
    );

    let error = service.validate_response("https://idp.example.com", &xml, Some("id_123")).unwrap_err();
    assert!(error.to_string().contains("destination mismatch"));
}

#[tokio::test]
async fn test_validate_response_rejects_mismatched_recipient() {
    let service = create_test_service();
    let xml = format!(
        r#"<samlp:Response InResponseTo="id_123">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="https://matrix.example.com/invalid"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
        (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
        (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339()
    );

    let error = service.validate_response("https://idp.example.com", &xml, Some("id_123")).unwrap_err();
    assert!(error.to_string().contains("recipient mismatch"));
}

#[tokio::test]
async fn saml_requires_at_least_one_signature() {
    let mut config = create_test_config();
    config.want_response_signed = false;
    config.want_assertions_signed = false;

    let pool = Arc::new(
        sqlx::PgPool::connect_lazy(&synapse_common::test_isolation::test_database_url())
            .expect("valid lazy postgres url"),
    );
    let storage = Arc::new(SamlStorage::new(&pool));
    let mut service = SamlService::new(Arc::new(config), storage, "localhost".to_string());

    // Seed cached metadata with a dummy certificate so that verify_saml_signature
    // proceeds past the metadata/certificate checks and reaches the no-signature check.
    service.cached_metadata = Some(SamlMetadata {
        entity_id: "https://idp.example.com".to_string(),
        sso_url: "https://idp.example.com/sso".to_string(),
        slo_url: None,
        certificate: "ZHVtbXktY2VydGlmaWNhdGUtYmFzZTY0".to_string(), // "dummy-certificate-base64"
        valid_until: None,
    });

    let acs_url = service.config.get_sp_acs_url(&service.server_name);
    // XML response without any <Signature> or <ds:Signature> elements
    let xml = format!(
        r#"<samlp:Response InResponseTo="id_123">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="{}"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
        (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
        (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
        acs_url
    );

    let error = service.validate_response("https://idp.example.com", &xml, Some("id_123")).unwrap_err();
    assert!(error.to_string().contains("signature"), "Should reject unsigned SAML response; got: {}", error);
}

// ── 配置脱敏（2026-10-28 补） ─────────────────────────────────────────────────
//
// `sanitized_base_config` 把 SAML 配置交给管理端 UI，必须把私钥 / 证书 / 元数据 XML /
// 文件路径换成布尔量。漏一个字段不是"覆盖率少一点"，是私钥泄漏 —— 而这个函数此前
// **一行测试都没有**（`docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §3.1.7.9 的 A 组第一条）。
//
// 断言刻意是**反向**的：喂进去哨兵串，再断言它们一个都不许出现在序列化结果里。正向断言
// （"存在 `has_sp_private_key` 这个键"）只能证明它**写了什么**，而脱敏函数的失效形态是
// "**多写了什么**"。

const SANITIZE_PRIVATE_KEY: &str = "-----BEGIN PRIVATE KEY-----SANITIZE-PROBE-PRIVATE";
const SANITIZE_CERTIFICATE: &str = "-----BEGIN CERTIFICATE-----SANITIZE-PROBE-CERT";
const SANITIZE_METADATA_XML: &str = "<md:EntityDescriptor>SANITIZE-PROBE-METADATA</md:EntityDescriptor>";
const SANITIZE_KEY_PATH: &str = "/etc/synapse/sanitize-probe-private.pem";

/// 用给定 config 造一个 service。
///
/// `connect_lazy` **不建立连接**（只解析 URL 并挂维护任务），所以不碰库；但它会 spawn 任务
/// ⇒ 调用方必须是 `#[tokio::test]`，否则 panic 在
/// `this functionality requires a Tokio context`（实测踩过）。
fn service_with(config: SamlConfig) -> SamlService {
    let pool = Arc::new(
        sqlx::PgPool::connect_lazy(&synapse_common::test_isolation::test_database_url())
            .expect("valid lazy postgres url"),
    );
    let storage = Arc::new(SamlStorage::new(&pool));
    SamlService::new(Arc::new(config), storage, "localhost".to_string())
}

/// 配齐四个敏感字段 ⇒ 序列化结果里一个都不许出现，且三个布尔量必须为 `true`。
#[tokio::test]
async fn sanitized_base_config_never_serializes_secret_material() {
    let mut config = create_test_config();
    config.sp_private_key = Some(SANITIZE_PRIVATE_KEY.to_string());
    config.sp_private_key_path = Some(SANITIZE_KEY_PATH.to_string());
    config.sp_certificate = Some(SANITIZE_CERTIFICATE.to_string());
    config.metadata_xml = Some(SANITIZE_METADATA_XML.to_string());

    let sanitized = service_with(config).sanitized_base_config();
    let rendered = sanitized.to_string();

    for secret in [SANITIZE_PRIVATE_KEY, SANITIZE_CERTIFICATE, SANITIZE_METADATA_XML, SANITIZE_KEY_PATH] {
        assert!(!rendered.contains(secret), "脱敏视图泄漏了 `{secret}` —— 这是密钥/路径泄漏，不是格式问题：{rendered}");
    }
    assert_eq!(sanitized["has_sp_private_key"], serde_json::json!(true));
    assert_eq!(sanitized["has_sp_certificate"], serde_json::json!(true));
    assert_eq!(sanitized["has_metadata_xml"], serde_json::json!(true));
}

/// 反向对照：四个字段都为空时三个布尔量必须是 `false`。
///
/// 没有这一半，"恒 `true`"的实现也能让上一条绿 —— 而恒 `true` 会让管理端永远显示
/// "私钥已加载"，运维不会再补，直到签名失败。
#[tokio::test]
async fn sanitized_base_config_reports_absent_secrets_as_false() {
    let mut config = create_test_config();
    config.sp_private_key = None;
    config.sp_private_key_path = None;
    config.sp_certificate = None;
    config.sp_certificate_path = None;
    config.metadata_xml = None;

    let sanitized = service_with(config).sanitized_base_config();

    assert_eq!(sanitized["has_sp_private_key"], serde_json::json!(false));
    assert_eq!(sanitized["has_sp_certificate"], serde_json::json!(false));
    assert_eq!(sanitized["has_metadata_xml"], serde_json::json!(false));
}

/// 只配**路径**、没有内联内容时，布尔量同样是 `true`。
///
/// 实现里是 `a.is_some()` 或 `b.is_some()`：漏掉另一半时，"只配了 `*_path`"的部署会显示
/// "私钥未加载"，而它其实能签名。
#[tokio::test]
async fn sanitized_base_config_counts_path_only_secrets_as_present() {
    const PATH_ONLY: &str = "/etc/synapse/sanitize-probe-path-only.pem";
    let mut config = create_test_config();
    config.sp_private_key = None;
    config.sp_private_key_path = Some(PATH_ONLY.to_string());

    let sanitized = service_with(config).sanitized_base_config();

    assert_eq!(sanitized["has_sp_private_key"], serde_json::json!(true), "只配了路径也算已加载：{sanitized}");
    assert!(!sanitized.to_string().contains(PATH_ONLY), "路径本身仍然不许出现在输出里：{sanitized}");
}

/// `effective_config` = 脱敏视图 + 运行期覆盖；没有覆盖时必须**逐键等于**脱敏视图。
///
/// 它自己重建了一份 JSON 结构，一旦漏掉某个键，管理端就少显示一项而不报错。
#[tokio::test]
async fn effective_config_without_overrides_equals_the_sanitized_base() {
    let service = service_with(create_test_config());
    assert_eq!(service.effective_config(), service.sanitized_base_config());
}

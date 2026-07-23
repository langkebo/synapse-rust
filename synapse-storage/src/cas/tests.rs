use super::*;

#[test]
fn test_cas_ticket_creation() {
    let ticket = CasTicket {
        id: 1,
        ticket_id: "ST-12345678".to_string(),
        user_id: "@alice:example.com".to_string(),
        service_url: "https://app.example.com".to_string(),
        created_ts: 1234567800000,
        expires_at: 1234567890000,
        consumed_ts: None,
        consumed_by: None,
        is_valid: true,
    };
    assert_eq!(ticket.ticket_id, "ST-12345678");
    assert!(ticket.is_valid);
}

#[test]
fn test_cas_proxy_ticket_creation() {
    let ticket = CasProxyTicket {
        id: 1,
        proxy_ticket_id: "PT-12345678".to_string(),
        user_id: "@alice:example.com".to_string(),
        service_url: "https://app.example.com".to_string(),
        pgt_url: Some("https://pgt.example.com".to_string()),
        created_ts: 1234567800000,
        expires_at: 1234567890000,
        consumed_ts: None,
        is_valid: true,
    };
    assert_eq!(ticket.proxy_ticket_id, "PT-12345678");
    assert!(ticket.is_valid);
}

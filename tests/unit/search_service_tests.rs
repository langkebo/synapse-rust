#![cfg(test)]

use synapse_rust::common::ApiError;
use synapse_rust::services::search_service::SearchService;
use tokio::runtime::Runtime;

#[test]
fn test_search_service_disabled() {
    let search_service = SearchService::new("http://localhost:9200", false, "synapse_test");
    assert!(!search_service.is_enabled());

    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let result = search_service
            .index_message(
                "$event1",
                "!room1:localhost",
                "@alice:localhost",
                "Hello",
                "m.room.message",
                Some("m.text"),
                1000,
            )
            .await;
        assert!(result.is_ok());

        let result = search_service
            .search_messages("@alice:localhost", "Hello", 10, None)
            .await;
        assert!(result.is_err());
        match result {
            Err(ApiError::Internal(msg)) => assert_eq!(msg, "Elasticsearch is disabled"),
            _ => panic!("Expected Internal error"),
        }
    });
}

#[test]
fn test_search_service_invalid_url() {
    let search_service = SearchService::new("http://invalid-host", true, "synapse_test");
    assert!(search_service.is_enabled());
}

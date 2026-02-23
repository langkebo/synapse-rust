#![cfg(test)]

use synapse_rust::common::ApiError;
    use synapse_rust::services::search_service::SearchService;
    use tokio::runtime::Runtime;

    #[test]
    fn test_search_service_disabled() {
        let search_service = SearchService::new("http://localhost:9200", false);
        assert!(!search_service.is_enabled());

        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let result = search_service
                .index_message(1, "session1", "@alice:localhost", "Hello", 1000)
                .await;
            assert!(result.is_ok()); // Should return Ok(()) if disabled

            let result = search_service
                .search_messages("@alice:localhost", "Hello", 10)
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
        // Even with invalid URL, if enabled is true, it might fail during Transport::single_node or first use
        let _search_service = SearchService::new("invalid_url", true);
        // Depending on implementation, it might still be "enabled" but fail on calls
        // In current implementation, Transport::single_node might fail
    }

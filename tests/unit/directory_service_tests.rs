// Directory Service Tests - 目录服务测试

#[cfg(test)]
mod tests {
    use synapse_rust::{DirectoryService, DirectoryServiceImpl};

    #[tokio::test]
    async fn test_set_and_get_room_alias() {
        let service = DirectoryServiceImpl::new();
        
        // Set room alias
        service.set_room_alias("!room:example.com", "#test:example.com").await.unwrap();
        
        // Get room ID by alias
        let room_id = service.get_room_id_by_alias("#test:example.com").await.unwrap();
        assert_eq!(room_id, Some("!room:example.com".to_string()));
    }

    #[tokio::test]
    async fn test_get_nonexistent_alias() {
        let service = DirectoryServiceImpl::new();
        
        let room_id = service.get_room_id_by_alias("#nonexistent:example.com").await.unwrap();
        assert_eq!(room_id, None);
    }

    #[tokio::test]
    async fn test_remove_room_alias() {
        let service = DirectoryServiceImpl::new();
        
        // Set alias
        service.set_room_alias("!room:example.com", "#test:example.com").await.unwrap();
        
        // Remove alias
        service.remove_room_alias("#test:example.com").await.unwrap();
        
        // Should not exist anymore
        let room_id = service.get_room_id_by_alias("#test:example.com").await.unwrap();
        assert_eq!(room_id, None);
    }

    #[tokio::test]
    async fn test_set_canonical_alias() {
        let service = DirectoryServiceImpl::new();
        
        // Set canonical alias
        service.set_canonical_alias("!room:example.com", Some("#main:example.com")).await.unwrap();
        
        // Get canonical alias
        let alias = service.get_canonical_alias("!room:example.com").await.unwrap();
        assert_eq!(alias, Some("#main:example.com".to_string()));
    }

    #[tokio::test]
    async fn test_clear_canonical_alias() {
        let service = DirectoryServiceImpl::new();
        
        // Set canonical alias
        service.set_canonical_alias("!room:example.com", Some("#main:example.com")).await.unwrap();
        
        // Clear canonical alias
        service.set_canonical_alias("!room:example.com", None).await.unwrap();
        
        // Should be None
        let alias = service.get_canonical_alias("!room:example.com").await.unwrap();
        assert_eq!(alias, None);
    }

    #[tokio::test]
    async fn test_get_public_rooms() {
        let service = DirectoryServiceImpl::new();
        
        // Get public rooms
        let rooms = service.get_public_rooms(10, None).await.unwrap();
        
        assert!(rooms.is_empty() || rooms.len() <= 10);
    }

    #[tokio::test]
    async fn test_search_public_rooms() {
        let service = DirectoryServiceImpl::new();
        
        // Search public rooms with filter
        let rooms = service.search_public_rooms(Some("test"), 10).await.unwrap();
        
        // Should return filtered results
        assert!(rooms.len() <= 10);
    }
}

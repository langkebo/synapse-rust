pub use synapse_services::directory_service::*;

#[cfg(test)]
mod tests {
    use super::{DirectoryRoom, DirectoryService};

    #[tokio::test]
    async fn root_directory_service_reexport_keeps_alias_round_trip() {
        let service = DirectoryService::new();

        service.set_room_alias("!room:example.com", "#test:example.com").await.unwrap();

        let room_id = service.get_room_id_by_alias("#test:example.com").await.unwrap();
        assert_eq!(room_id, Some("!room:example.com".to_string()));
    }

    #[tokio::test]
    async fn root_directory_service_reexport_keeps_public_room_search() {
        let service = DirectoryService::new();

        service
            .add_public_room(DirectoryRoom {
                room_id: "!room1:example.com".to_string(),
                name: Some("Test Room".to_string()),
                topic: Some("A test topic".to_string()),
                avatar_url: None,
                member_count: 10,
                world_readable: true,
                guest_can_join: true,
            })
            .await;

        service
            .add_public_room(DirectoryRoom {
                room_id: "!room2:example.com".to_string(),
                name: Some("Another Room".to_string()),
                topic: None,
                avatar_url: None,
                member_count: 5,
                world_readable: true,
                guest_can_join: false,
            })
            .await;

        let rooms = service.search_public_rooms(Some("test"), 10).await.unwrap();
        assert_eq!(rooms.len(), 1);
        assert_eq!(rooms[0].room_id, "!room1:example.com");
    }
}

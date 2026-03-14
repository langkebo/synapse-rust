use crate::services::cache::room_cache::{
    CachedRoomSummary, CachedRoomMember, CachedPresence, RoomSummaryCache,
};

fn create_test_cache() -> RoomSummaryCache {
    RoomSummaryCache::new(300)
}

#[tokio::test]
async fn test_set_and_get_summary() {
    let cache = create_test_cache();
    
    let summary = CachedRoomSummary {
        room_id: "!room1:example.com".to_string(),
        name: Some("Test Room".to_string()),
        avatar_url: None,
        is_direct: false,
        is_encrypted: false,
        member_count: 5,
        joined_members: 5,
        unread_notifications: 2,
        highlight_count: 1,
        last_event_ts: Some(1234567890),
        cached_at: 0,
    };
    
    cache.set_summary(summary.clone()).await;
    
    let retrieved = cache.get_summary(&summary.room_id).await;
    assert!(retrieved.is_some());
    
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.room_id, summary.room_id);
    assert_eq!(retrieved.name, summary.name);
    assert_eq!(retrieved.member_count, summary.member_count);
}

#[tokio::test]
async fn test_get_summary_not_found() {
    let cache = create_test_cache();
    
    let retrieved = cache.get_summary("!nonexistent:example.com").await;
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_set_summaries_batch() {
    let cache = create_test_cache();
    
    let summaries = vec![
        CachedRoomSummary {
            room_id: "!room1:example.com".to_string(),
            name: Some("Room 1".to_string()),
            avatar_url: None,
            is_direct: true,
            is_encrypted: true,
            member_count: 2,
            joined_members: 2,
            unread_notifications: 0,
            highlight_count: 0,
            last_event_ts: None,
            cached_at: 0,
        },
        CachedRoomSummary {
            room_id: "!room2:example.com".to_string(),
            name: Some("Room 2".to_string()),
            avatar_url: None,
            is_direct: true,
            is_encrypted: true,
            member_count: 2,
            joined_members: 2,
            unread_notifications: 3,
            highlight_count: 1,
            last_event_ts: Some(1234567890),
            cached_at: 0,
        },
    ];
    
    cache.set_summaries_batch(summaries.clone()).await;
    
    let room_ids: Vec<String> = summaries.iter().map(|s| s.room_id.clone()).collect();
    let retrieved = cache.get_summaries_batch(&room_ids).await;
    
    assert_eq!(retrieved.len(), 2);
}

#[tokio::test]
async fn test_invalidate_summary() {
    let cache = create_test_cache();
    
    let summary = CachedRoomSummary {
        room_id: "!room1:example.com".to_string(),
        name: Some("Test Room".to_string()),
        avatar_url: None,
        is_direct: false,
        is_encrypted: false,
        member_count: 1,
        joined_members: 1,
        unread_notifications: 0,
        highlight_count: 0,
        last_event_ts: None,
        cached_at: 0,
    };
    
    cache.set_summary(summary.clone()).await;
    
    let retrieved = cache.get_summary(&summary.room_id).await;
    assert!(retrieved.is_some());
    
    cache.invalidate_summary(&summary.room_id).await;
    
    let retrieved = cache.get_summary(&summary.room_id).await;
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_set_and_get_members() {
    let cache = create_test_cache();
    
    let members = vec![
        CachedRoomMember {
            user_id: "@user1:example.com".to_string(),
            displayname: Some("User 1".to_string()),
            avatar_url: None,
            membership: "join".to_string(),
            cached_at: 0,
        },
        CachedRoomMember {
            user_id: "@user2:example.com".to_string(),
            displayname: Some("User 2".to_string()),
            avatar_url: None,
            membership: "join".to_string(),
            cached_at: 0,
        },
    ];
    
    cache.set_members("!room1:example.com", members.clone()).await;
    
    let retrieved = cache.get_members("!room1:example.com").await;
    assert!(retrieved.is_some());
    
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.len(), 2);
    assert_eq!(retrieved[0].user_id, members[0].user_id);
}

#[tokio::test]
async fn test_invalidate_members() {
    let cache = create_test_cache();
    
    let members = vec![CachedRoomMember {
        user_id: "@user1:example.com".to_string(),
        displayname: None,
        avatar_url: None,
        membership: "join".to_string(),
        cached_at: 0,
    }];
    
    cache.set_members("!room1:example.com", members).await;
    
    let retrieved = cache.get_members("!room1:example.com").await;
    assert!(retrieved.is_some());
    
    cache.invalidate_members("!room1:example.com").await;
    
    let retrieved = cache.get_members("!room1:example.com").await;
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_set_and_get_presence() {
    let cache = create_test_cache();
    
    let presence = CachedPresence {
        user_id: "@user1:example.com".to_string(),
        presence: "online".to_string(),
        status_msg: Some("Working".to_string()),
        last_active_ts: Some(1234567890),
        cached_at: 0,
    };
    
    cache.set_presence(presence.clone()).await;
    
    let retrieved = cache.get_presence(&presence.user_id).await;
    assert!(retrieved.is_some());
    
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.user_id, presence.user_id);
    assert_eq!(retrieved.presence, presence.presence);
}

#[tokio::test]
async fn test_get_presence_batch() {
    let cache = create_test_cache();
    
    let presences = vec![
        CachedPresence {
            user_id: "@user1:example.com".to_string(),
            presence: "online".to_string(),
            status_msg: None,
            last_active_ts: None,
            cached_at: 0,
        },
        CachedPresence {
            user_id: "@user2:example.com".to_string(),
            presence: "offline".to_string(),
            status_msg: None,
            last_active_ts: None,
            cached_at: 0,
        },
    ];
    
    for p in &presences {
        cache.set_presence(p.clone()).await;
    }
    
    let user_ids: Vec<String> = presences.iter().map(|p| p.user_id.clone()).collect();
    let retrieved = cache.get_presence_batch(&user_ids).await;
    
    assert_eq!(retrieved.len(), 2);
}

#[tokio::test]
async fn test_clear_cache() {
    let cache = create_test_cache();
    
    let summary = CachedRoomSummary {
        room_id: "!room1:example.com".to_string(),
        name: Some("Test".to_string()),
        avatar_url: None,
        is_direct: false,
        is_encrypted: false,
        member_count: 1,
        joined_members: 1,
        unread_notifications: 0,
        highlight_count: 0,
        last_event_ts: None,
        cached_at: 0,
    };
    
    cache.set_summary(summary).await;
    
    let stats = cache.stats().await;
    assert_eq!(stats.summary_count, 1);
    
    cache.clear().await;
    
    let stats = cache.stats().await;
    assert_eq!(stats.summary_count, 0);
    assert_eq!(stats.member_cache_count, 0);
    assert_eq!(stats.presence_count, 0);
}

#[tokio::test]
async fn test_cache_stats() {
    let cache = create_test_cache();
    
    let stats = cache.stats().await;
    assert_eq!(stats.summary_count, 0);
    assert_eq!(stats.member_cache_count, 0);
    assert_eq!(stats.presence_count, 0);
    
    let summary = CachedRoomSummary {
        room_id: "!room1:example.com".to_string(),
        name: None,
        avatar_url: None,
        is_direct: false,
        is_encrypted: false,
        member_count: 1,
        joined_members: 1,
        unread_notifications: 0,
        highlight_count: 0,
        last_event_ts: None,
        cached_at: 0,
    };
    cache.set_summary(summary).await;
    
    let stats = cache.stats().await;
    assert_eq!(stats.summary_count, 1);
}

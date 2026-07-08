use super::*;

fn create_test_space() -> Space {
    Space {
        space_id: "!test_space:localhost".to_string(),
        room_id: "!test_space:localhost".to_string(),
        name: Some("Test Space".to_string()),
        topic: Some("A test space".to_string()),
        avatar_url: None,
        creator: "@test:localhost".to_string(),
        join_rule: "invite".to_string(),
        visibility: Some("private".to_string()),
        created_ts: 1234567890,
        updated_ts: None,
        is_public: false,
        parent_space_id: None,
        room_type: None,
    }
}

fn create_test_space_child() -> SpaceChild {
    SpaceChild {
        id: 1,
        space_id: "!test_space:localhost".to_string(),
        room_id: "!child_room:localhost".to_string(),
        sender: "@test:localhost".to_string(),
        is_suggested: false,
        via_servers: vec!["localhost".to_string()],
        added_ts: 1234567890,
        order: None,
        suggested: None,
        added_by: None,
        removed_ts: None,
    }
}

fn create_test_space_member() -> SpaceMember {
    SpaceMember {
        space_id: "!test_space:localhost".to_string(),
        user_id: "@test:localhost".to_string(),
        membership: "join".to_string(),
        joined_ts: 1234567890,
        updated_ts: None,
        left_ts: None,
        inviter: None,
    }
}

#[test]
fn test_space_serialization() {
    let space = create_test_space();
    let json = serde_json::to_string(&space).unwrap();
    let deserialized: Space = serde_json::from_str(&json).unwrap();

    assert_eq!(space.space_id, deserialized.space_id);
    assert_eq!(space.name, deserialized.name);
    assert_eq!(space.topic, deserialized.topic);
    assert_eq!(space.creator, deserialized.creator);
    assert_eq!(space.join_rule, deserialized.join_rule);
    assert_eq!(space.visibility, deserialized.visibility);
    assert_eq!(space.is_public, deserialized.is_public);
}

#[test]
fn test_space_child_serialization() {
    let child = create_test_space_child();
    let json = serde_json::to_string(&child).unwrap();
    let deserialized: SpaceChild = serde_json::from_str(&json).unwrap();

    assert_eq!(child.space_id, deserialized.space_id);
    assert_eq!(child.room_id, deserialized.room_id);
    assert_eq!(child.via_servers, deserialized.via_servers);
    assert_eq!(child.sender, deserialized.sender);
    assert_eq!(child.is_suggested, deserialized.is_suggested);
}

#[test]
fn test_space_member_serialization() {
    let member = create_test_space_member();
    let json = serde_json::to_string(&member).unwrap();
    let deserialized: SpaceMember = serde_json::from_str(&json).unwrap();

    assert_eq!(member.space_id, deserialized.space_id);
    assert_eq!(member.user_id, deserialized.user_id);
    assert_eq!(member.membership, deserialized.membership);
    assert_eq!(member.joined_ts, deserialized.joined_ts);
}

#[test]
fn test_create_space_request() {
    let request = CreateSpaceRequest {
        room_id: "!room:localhost".to_string(),
        name: Some("New Space".to_string()),
        topic: Some("Description".to_string()),
        avatar_url: None,
        creator: "@user:localhost".to_string(),
        join_rule: Some("public".to_string()),
        visibility: Some("public".to_string()),
        is_public: Some(true),
        parent_space_id: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: CreateSpaceRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(request.room_id, deserialized.room_id);
    assert_eq!(request.name, deserialized.name);
    assert_eq!(request.creator, deserialized.creator);
    assert_eq!(request.join_rule, deserialized.join_rule);
}

#[test]
fn test_add_child_request() {
    let request = AddChildRequest {
        space_id: "!space:localhost".to_string(),
        room_id: "!child:localhost".to_string(),
        sender: "@user:localhost".to_string(),
        is_suggested: true,
        via_servers: vec!["server1.com".to_string(), "server2.com".to_string()],
    };

    let json = serde_json::to_string(&request).unwrap();
    let deserialized: AddChildRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(request.space_id, deserialized.space_id);
    assert_eq!(request.room_id, deserialized.room_id);
    assert_eq!(request.via_servers.len(), deserialized.via_servers.len());
    assert_eq!(request.is_suggested, deserialized.is_suggested);
}

#[test]
fn test_space_hierarchy() {
    let space = create_test_space();
    let child = create_test_space_child();
    let member = create_test_space_member();

    let hierarchy = SpaceHierarchy { space, children: vec![child], members: vec![member] };

    let json = serde_json::to_string(&hierarchy).unwrap();
    let deserialized: SpaceHierarchy = serde_json::from_str(&json).unwrap();

    assert_eq!(hierarchy.space.space_id, deserialized.space.space_id);
    assert_eq!(hierarchy.children.len(), deserialized.children.len());
    assert_eq!(hierarchy.members.len(), deserialized.members.len());
}

#[test]
fn test_space_summary() {
    let summary = SpaceSummary {
        id: 1,
        space_id: "!space:localhost".to_string(),
        summary: serde_json::json!({"key": "value"}),
        children_count: 5,
        member_count: 10,
        updated_ts: 1234567890,
    };

    let json = serde_json::to_string(&summary).unwrap();
    let deserialized: SpaceSummary = serde_json::from_str(&json).unwrap();

    assert_eq!(summary.space_id, deserialized.space_id);
    assert_eq!(summary.children_count, deserialized.children_count);
    assert_eq!(summary.member_count, deserialized.member_count);
}

#[test]
fn test_space_event() {
    let event = SpaceEvent {
        event_id: "$event:localhost".to_string(),
        space_id: "!space:localhost".to_string(),
        event_type: "m.space.child".to_string(),
        sender: "@user:localhost".to_string(),
        content: serde_json::json!({"room_id": "!child:localhost"}),
        state_key: Some("!child:localhost".to_string()),
        origin_server_ts: 1234567890,
        processed_ts: None,
    };

    let json = serde_json::to_string(&event).unwrap();
    let deserialized: SpaceEvent = serde_json::from_str(&json).unwrap();

    assert_eq!(event.event_id, deserialized.event_id);
    assert_eq!(event.event_type, deserialized.event_type);
    assert_eq!(event.sender, deserialized.sender);
}

#[test]
fn test_update_space_request_builders_all_fields() {
    let req = UpdateSpaceRequest::new()
        .name("My Space")
        .topic("A topic")
        .avatar_url("mxc://localhost/abc")
        .join_rule("public")
        .visibility("public")
        .is_public(true);
    assert_eq!(req.name, Some("My Space".to_string()));
    assert_eq!(req.topic, Some("A topic".to_string()));
    assert_eq!(req.avatar_url, Some("mxc://localhost/abc".to_string()));
    assert_eq!(req.join_rule, Some("public".to_string()));
    assert_eq!(req.visibility, Some("public".to_string()));
    assert_eq!(req.is_public, Some(true));
}

#[test]
fn test_update_space_request_new_is_empty() {
    let req = UpdateSpaceRequest::new();
    assert!(req.name.is_none());
    assert!(req.topic.is_none());
    assert!(req.avatar_url.is_none());
    assert!(req.join_rule.is_none());
    assert!(req.visibility.is_none());
    assert!(req.is_public.is_none());
}

#[test]
fn test_space_hierarchy_request_fields() {
    let req = SpaceHierarchyRequest {
        space_id: "!space:localhost".to_string(),
        max_depth: 3,
        suggested_only: true,
        limit: Some(50),
        from: Some("token".to_string()),
    };
    assert_eq!(req.space_id, "!space:localhost");
    assert_eq!(req.max_depth, 3);
    assert!(req.suggested_only);
    assert_eq!(req.limit, Some(50));
    assert_eq!(req.from, Some("token".to_string()));
}

#[test]
fn test_space_child_info_fields() {
    let info = SpaceChildInfo {
        space_id: "!parent:localhost".to_string(),
        room_id: "!child:localhost".to_string(),
        via_servers: vec!["localhost".to_string()],
        is_suggested: true,
        is_space: false,
        depth: 2,
    };
    assert_eq!(info.space_id, "!parent:localhost");
    assert_eq!(info.room_id, "!child:localhost");
    assert_eq!(info.depth, 2);
    assert!(info.is_suggested);
    assert!(!info.is_space);
}

#[test]
fn test_space_hierarchy_node_structure() {
    let space = create_test_space();
    let node = SpaceHierarchyNode { space: space.clone(), children: vec![], depth: 0 };
    assert_eq!(node.depth, 0);
    assert_eq!(node.space.space_id, space.space_id);
    assert!(node.children.is_empty());
}

#[test]
fn test_space_hierarchy_response_fields() {
    let resp = SpaceHierarchyResponse { rooms: vec![], next_batch: Some("next".to_string()) };
    assert!(resp.rooms.is_empty());
    assert_eq!(resp.next_batch, Some("next".to_string()));
}

use super::*;

#[derive(Debug, Deserialize, Validate)]
pub struct CreateSpaceBody {
    #[validate(length(min = 1, max = 255))]
    pub room_id: String,
    #[validate(length(max = 255))]
    pub name: Option<String>,
    #[validate(length(max = 1000))]
    pub topic: Option<String>,
    #[validate(length(max = 2048))]
    pub avatar_url: Option<String>,
    #[validate(length(max = 50))]
    pub join_rule: Option<String>,
    #[validate(length(max = 50))]
    pub visibility: Option<String>,
    pub is_public: Option<bool>,
    #[validate(length(max = 255))]
    pub parent_space_id: Option<String>,
}

impl CreateSpaceBody {
    pub fn into_request(self, creator: String) -> crate::storage::space::CreateSpaceRequest {
        crate::storage::space::CreateSpaceRequest {
            room_id: self.room_id,
            name: self.name,
            topic: self.topic,
            avatar_url: self.avatar_url,
            creator,
            join_rule: self.join_rule,
            visibility: self.visibility,
            is_public: self.is_public,
            parent_space_id: self.parent_space_id,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct AddChildBody {
    #[validate(length(min = 1, max = 255))]
    pub room_id: String,
    #[validate(length(max = 100))]
    pub via_servers: Vec<String>,
    pub suggested: Option<bool>,
}

impl AddChildBody {
    pub fn into_request(
        self,
        space_id: String,
        sender: String,
    ) -> crate::storage::space::AddChildRequest {
        crate::storage::space::AddChildRequest {
            space_id,
            room_id: self.room_id,
            sender,
            is_suggested: self.suggested.unwrap_or(false),
            via_servers: self.via_servers,
        }
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct UpdateSpaceBody {
    #[validate(length(max = 255))]
    pub name: Option<String>,
    #[validate(length(max = 1000))]
    pub topic: Option<String>,
    #[validate(length(max = 2048))]
    pub avatar_url: Option<String>,
    #[validate(length(max = 50))]
    pub join_rule: Option<String>,
    #[validate(length(max = 50))]
    pub visibility: Option<String>,
    pub is_public: Option<bool>,
}

impl UpdateSpaceBody {
    pub fn into_request(self) -> crate::storage::space::UpdateSpaceRequest {
        let mut request = crate::storage::space::UpdateSpaceRequest::new();

        if let Some(name) = self.name {
            request = request.name(name);
        }
        if let Some(topic) = self.topic {
            request = request.topic(topic);
        }
        if let Some(avatar_url) = self.avatar_url {
            request = request.avatar_url(avatar_url);
        }
        if let Some(join_rule) = self.join_rule {
            request = request.join_rule(join_rule);
        }
        if let Some(visibility) = self.visibility {
            request = request.visibility(visibility);
        }
        if let Some(is_public) = self.is_public {
            request = request.is_public(is_public);
        }

        request
    }
}

#[derive(Debug, Deserialize, Validate)]
pub struct InviteUserBody {
    #[validate(length(min = 1, max = 255))]
    pub user_id: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct PaginationQuery {
    #[validate(range(min = 0, max = 1000))]
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct SearchQuery {
    #[serde(alias = "search_term")]
    #[validate(length(min = 1, max = 500))]
    pub query: String,
    #[validate(range(min = 0, max = 100))]
    pub limit: Option<i64>,
}

#[derive(Debug, Deserialize, Validate)]
pub struct HierarchyQuery {
    #[validate(range(min = 1, max = 20))]
    pub max_depth: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct SpaceResponse {
    pub space_id: String,
    pub room_id: String,
    pub name: Option<String>,
    pub topic: Option<String>,
    pub avatar_url: Option<String>,
    pub creator: String,
    pub join_rule: String,
    pub visibility: Option<String>,
    pub is_public: bool,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub parent_space_id: Option<String>,
}

impl From<crate::storage::space::Space> for SpaceResponse {
    fn from(space: crate::storage::space::Space) -> Self {
        Self {
            space_id: space.space_id,
            room_id: space.room_id,
            name: space.name,
            topic: space.topic,
            avatar_url: space.avatar_url,
            creator: space.creator,
            join_rule: space.join_rule,
            visibility: space.visibility,
            is_public: space.is_public,
            created_ts: space.created_ts,
            updated_ts: space.updated_ts,
            parent_space_id: space.parent_space_id,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SpaceChildResponse {
    pub space_id: String,
    pub room_id: String,
    pub via_servers: Vec<String>,
    pub sender: String,
    pub is_suggested: bool,
    pub added_ts: i64,
}

impl From<crate::storage::space::SpaceChild> for SpaceChildResponse {
    fn from(child: crate::storage::space::SpaceChild) -> Self {
        Self {
            space_id: child.space_id,
            room_id: child.room_id,
            via_servers: child.via_servers,
            sender: child.sender,
            is_suggested: child.is_suggested,
            added_ts: child.added_ts,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SpaceMemberResponse {
    pub space_id: String,
    pub user_id: String,
    pub membership: String,
    pub joined_ts: i64,
    pub inviter: Option<String>,
}

impl From<crate::storage::space::SpaceMember> for SpaceMemberResponse {
    fn from(member: crate::storage::space::SpaceMember) -> Self {
        Self {
            space_id: member.space_id,
            user_id: member.user_id,
            membership: member.membership,
            joined_ts: member.joined_ts,
            inviter: member.inviter,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SpaceHierarchyResponse {
    pub space: SpaceResponse,
    pub children: Vec<SpaceChildResponse>,
    pub members: Vec<SpaceMemberResponse>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_space_body() {
        let body = CreateSpaceBody {
            room_id: "!room:example.com".to_string(),
            name: Some("My Space".to_string()),
            topic: Some("A test space".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            join_rule: Some("invite".to_string()),
            visibility: Some("private".to_string()),
            is_public: Some(false),
            parent_space_id: None,
        };

        assert_eq!(body.room_id, "!room:example.com");
        assert!(body.name.is_some());
    }

    #[test]
    fn test_add_child_body() {
        let body = AddChildBody {
            room_id: "!child:example.com".to_string(),
            via_servers: vec!["example.com".to_string()],
            suggested: Some(true),
        };

        assert_eq!(body.room_id, "!child:example.com");
        assert!(!body.via_servers.is_empty());
    }

    #[test]
    fn test_create_space_body_into_request_preserves_fields() {
        let body = CreateSpaceBody {
            room_id: "!room:example.com".to_string(),
            name: Some("My Space".to_string()),
            topic: Some("A test space".to_string()),
            avatar_url: Some("mxc://example.com/avatar".to_string()),
            join_rule: Some("invite".to_string()),
            visibility: Some("private".to_string()),
            is_public: Some(false),
            parent_space_id: Some("!parent:example.com".to_string()),
        };

        let request = body.into_request("@alice:example.com".to_string());

        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.creator, "@alice:example.com");
        assert_eq!(
            request.parent_space_id.as_deref(),
            Some("!parent:example.com")
        );
    }

    #[test]
    fn test_create_space_body_into_request_preserves_absent_optional_fields() {
        let body = CreateSpaceBody {
            room_id: "!room:example.com".to_string(),
            name: None,
            topic: None,
            avatar_url: None,
            join_rule: None,
            visibility: None,
            is_public: None,
            parent_space_id: None,
        };

        let request = body.into_request("@alice:example.com".to_string());

        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.creator, "@alice:example.com");
        assert!(request.name.is_none());
        assert!(request.topic.is_none());
        assert!(request.avatar_url.is_none());
        assert!(request.join_rule.is_none());
        assert!(request.visibility.is_none());
        assert_eq!(request.is_public, None);
        assert!(request.parent_space_id.is_none());
    }

    #[test]
    fn test_add_child_body_into_request_defaults_suggested_to_false() {
        let body = AddChildBody {
            room_id: "!child:example.com".to_string(),
            via_servers: vec!["example.com".to_string()],
            suggested: None,
        };

        let request = body.into_request(
            "!space:example.com".to_string(),
            "@bob:example.com".to_string(),
        );

        assert_eq!(request.space_id, "!space:example.com");
        assert_eq!(request.sender, "@bob:example.com");
        assert!(!request.is_suggested);
    }

    #[test]
    fn test_add_child_body_into_request_preserves_explicit_suggested_and_via_servers() {
        let body = AddChildBody {
            room_id: "!child:example.com".to_string(),
            via_servers: vec!["example.com".to_string(), "backup.example.com".to_string()],
            suggested: Some(true),
        };

        let request = body.into_request(
            "!space:example.com".to_string(),
            "@bob:example.com".to_string(),
        );

        assert_eq!(request.room_id, "!child:example.com");
        assert_eq!(request.space_id, "!space:example.com");
        assert_eq!(
            request.via_servers,
            vec!["example.com".to_string(), "backup.example.com".to_string()]
        );
        assert!(request.is_suggested);
    }

    #[test]
    fn test_update_space_body() {
        let body = UpdateSpaceBody {
            name: Some("Updated Name".to_string()),
            topic: Some("Updated topic".to_string()),
            avatar_url: None,
            join_rule: Some("public".to_string()),
            visibility: Some("public".to_string()),
            is_public: Some(true),
        };

        assert!(body.name.is_some());
        assert!(body.avatar_url.is_none());
    }

    #[test]
    fn test_update_space_body_into_request_preserves_optional_fields() {
        let body = UpdateSpaceBody {
            name: Some("Updated Name".to_string()),
            topic: Some("Updated topic".to_string()),
            avatar_url: Some("mxc://example.com/updated".to_string()),
            join_rule: Some("public".to_string()),
            visibility: Some("public".to_string()),
            is_public: Some(true),
        };

        let request = body.into_request();

        assert_eq!(request.name.as_deref(), Some("Updated Name"));
        assert_eq!(request.topic.as_deref(), Some("Updated topic"));
        assert_eq!(
            request.avatar_url.as_deref(),
            Some("mxc://example.com/updated")
        );
        assert_eq!(request.join_rule.as_deref(), Some("public"));
        assert_eq!(request.visibility.as_deref(), Some("public"));
        assert_eq!(request.is_public, Some(true));
    }

    #[test]
    fn test_update_space_body_into_request_preserves_absent_optional_fields() {
        let body = UpdateSpaceBody {
            name: None,
            topic: None,
            avatar_url: None,
            join_rule: None,
            visibility: None,
            is_public: None,
        };

        let request = body.into_request();

        assert!(request.name.is_none());
        assert!(request.topic.is_none());
        assert!(request.avatar_url.is_none());
        assert!(request.join_rule.is_none());
        assert!(request.visibility.is_none());
        assert_eq!(request.is_public, None);
    }

    #[test]
    fn test_pagination_query() {
        let query = PaginationQuery {
            limit: Some(100),
            offset: Some(0),
        };

        assert_eq!(query.limit, Some(100));
        assert_eq!(query.offset, Some(0));
    }

    #[test]
    fn test_search_query() {
        let query = SearchQuery {
            query: "test space".to_string(),
            limit: Some(10),
        };

        assert_eq!(query.query, "test space");
        assert_eq!(query.limit, Some(10));
    }

    #[test]
    fn test_hierarchy_query() {
        let query = HierarchyQuery { max_depth: Some(3) };

        assert_eq!(query.max_depth, Some(3));
    }

    #[test]
    fn test_space_response_structure() {
        let response = SpaceResponse {
            space_id: "space_123".to_string(),
            room_id: "!room:example.com".to_string(),
            name: Some("Test Space".to_string()),
            topic: None,
            avatar_url: None,
            creator: "@admin:example.com".to_string(),
            join_rule: "invite".to_string(),
            visibility: Some("private".to_string()),
            is_public: false,
            created_ts: 1234567890,
            updated_ts: None,
            parent_space_id: None,
        };

        assert_eq!(response.space_id, "space_123");
        assert!(!response.is_public);
    }

    #[test]
    fn test_invite_user_body() {
        let body = InviteUserBody {
            user_id: "@alice:example.com".to_string(),
        };

        assert!(body.user_id.starts_with('@'));
    }
}

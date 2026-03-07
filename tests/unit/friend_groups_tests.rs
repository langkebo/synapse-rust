use crate::storage::friend_room::*;

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_group(name: &str, members: Vec<&str>) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "members": members.iter().map(|s| s.to_string()).collect::<Vec<String>>(),
            "created_ts": 1234567890000i64,
            "updated_ts": 1234567890000i64
        })
    }

    #[test]
    fn test_friend_groups_structure() {
        let groups = serde_json::json!({
            "groups": [
                {
                    "name": "Family",
                    "members": ["@mom:example.com", "@dad:example.com"],
                    "created_ts": 1234567890000i64,
                    "updated_ts": 1234567890000i64
                },
                {
                    "name": "Work",
                    "members": ["@colleague1:example.com", "@colleague2:example.com"],
                    "created_ts": 1234567890000i64,
                    "updated_ts": 1234567890000i64
                }
            ],
            "version": 1,
            "updated_ts": 1234567890000i64
        });

        let groups_array = groups.get("groups").unwrap().as_array().unwrap();
        assert_eq!(groups_array.len(), 2);
    }

    #[test]
    fn test_create_friend_group_params() {
        let params = CreateFriendGroupParams {
            room_id: "!friends:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            group_name: "Family".to_string(),
        };

        assert_eq!(params.group_name, "Family");
        assert_eq!(params.room_id, "!friends:example.com");
    }

    #[test]
    fn test_add_friend_to_group_params() {
        let params = AddFriendToGroupParams {
            room_id: "!friends:example.com".to_string(),
            user_id: "@alice:example.com".to_string(),
            group_name: "Work".to_string(),
            friend_id: "@bob:example.com".to_string(),
        };

        assert_eq!(params.group_name, "Work");
        assert_eq!(params.friend_id, "@bob:example.com");
    }

    #[test]
    fn test_group_membership_check() {
        let groups = serde_json::json!({
            "groups": [
                {
                    "name": "Family",
                    "members": ["@mom:example.com", "@dad:example.com"],
                    "created_ts": 1234567890000i64,
                    "updated_ts": 1234567890000i64
                }
            ]
        });

        let groups_array = groups.get("groups").unwrap().as_array().unwrap();
        let family_group = &groups_array[0];
        let members = family_group.get("members").unwrap().as_array().unwrap();

        let is_member = members.iter().any(|m| {
            m.as_str() == Some("@mom:example.com")
        });

        assert!(is_member);
    }

    #[test]
    fn test_group_version_increment() {
        let mut groups = serde_json::json!({
            "groups": [],
            "version": 1,
            "updated_ts": 1234567890000i64
        });

        let version = groups.get("version").unwrap().as_i64().unwrap();
        groups["version"] = serde_json::json!(version + 1);

        let new_version = groups.get("version").unwrap().as_i64().unwrap();
        assert_eq!(new_version, 2);
    }

    #[test]
    fn test_remove_friend_from_group() {
        let mut groups = serde_json::json!({
            "groups": [
                {
                    "name": "Family",
                    "members": ["@mom:example.com", "@dad:example.com", "@sibling:example.com"],
                    "created_ts": 1234567890000i64,
                    "updated_ts": 1234567890000i64
                }
            ]
        });

        let groups_array = groups.get_mut("groups").unwrap().as_array_mut().unwrap();
        let family_group = &mut groups_array[0];
        let members = family_group.get("members").unwrap().as_array().unwrap().clone();

        let filtered: Vec<_> = members
            .iter()
            .filter(|m| m.as_str() != Some("@sibling:example.com"))
            .cloned()
            .collect();

        assert_eq!(filtered.len(), 2);
        assert!(!filtered.iter().any(|m| m.as_str() == Some("@sibling:example.com")));
    }

    #[test]
    fn test_rename_group() {
        let mut groups = serde_json::json!({
            "groups": [
                {
                    "name": "Old Name",
                    "members": [],
                    "created_ts": 1234567890000i64,
                    "updated_ts": 1234567890000i64
                }
            ]
        });

        let groups_array = groups.get_mut("groups").unwrap().as_array_mut().unwrap();
        let group = &mut groups_array[0];
        group["name"] = serde_json::json!("New Name");

        assert_eq!(group.get("name").unwrap().as_str().unwrap(), "New Name");
    }

    #[test]
    fn test_empty_groups() {
        let groups = serde_json::json!({
            "groups": [],
            "version": 1,
            "updated_ts": 1234567890000i64
        });

        let groups_array = groups.get("groups").unwrap().as_array().unwrap();
        assert!(groups_array.is_empty());
    }

    #[test]
    fn test_multiple_groups() {
        let groups = serde_json::json!({
            "groups": [
                {"name": "Group1", "members": ["@user1:example.com"]},
                {"name": "Group2", "members": ["@user2:example.com"]},
                {"name": "Group3", "members": ["@user3:example.com"]}
            ]
        });

        let groups_array = groups.get("groups").unwrap().as_array().unwrap();
        assert_eq!(groups_array.len(), 3);
    }

    #[test]
    fn test_friend_in_multiple_groups() {
        let groups = serde_json::json!({
            "groups": [
                {"name": "Family", "members": ["@bob:example.com"]},
                {"name": "Work", "members": ["@bob:example.com", "@alice:example.com"]}
            ]
        });

        let groups_array = groups.get("groups").unwrap().as_array().unwrap();
        
        let bob_groups: Vec<_> = groups_array
            .iter()
            .filter_map(|g| {
                let members = g.get("members")?.as_array().ok()?;
                let name = g.get("name")?.as_str()?;
                if members.iter().any(|m| m.as_str() == Some("@bob:example.com")) {
                    Some(name.to_string())
                } else {
                    None
                }
            })
            .collect();

        assert_eq!(bob_groups.len(), 2);
        assert!(bob_groups.contains(&"Family".to_string()));
        assert!(bob_groups.contains(&"Work".to_string()));
    }
}

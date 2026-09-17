

## Step 3: friend_room_service/groups.rs 完整改造 ✅

### 已完成
- **所有公开方法返回类型升级**: ApiResult<T> → Result<T, FriendRoomError>
- **9 个方法完成转换**:
  - query_user_friends → Result<Vec<String>, FriendRoomError>
  - create_friend_group → Result<serde_json::Value, FriendRoomError>
  - add_friend_to_group → Result<(), FriendRoomError>
  - remove_friend_from_group → Result<(), FriendRoomError>
  - get_friend_groups → Result<Vec<serde_json::Value>, FriendRoomError>
  - update_friend_group_name → Result<(), FriendRoomError>
  - delete_friend_group → Result<(), FriendRoomError>
  - get_friend_group_info → Result<serde_json::Value, FriendRoomError>
  - get_groups_for_user → Result<Vec<serde_json::Value>, FriendRoomError>
- **错误处理统一**:
  - ApiError::Database → FriendRoomError::Database (sqlx::Error)
  - ApiError::NotFound → FriendRoomError::NotFound (String)
  - ApiError::BadRequest → FriendRoomError::InvalidInput (String)
  - ApiError::Forbidden → FriendRoomError::NotAuthorized (String)
- **Federation 调用适配**: query_remote_friends 错误转 FriendRoomError::Internal
- **编译状态**: cargo check --lib 通过

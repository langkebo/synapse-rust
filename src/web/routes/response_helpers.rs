use axum::{http::StatusCode, Json};
use serde_json::{json, Value};

use super::AppState;
use crate::common::ApiError;

pub(crate) fn require_found<T>(value: Option<T>, message: &'static str) -> Result<T, ApiError> {
    value.ok_or_else(|| ApiError::not_found(message))
}

pub(crate) fn json_from<T, U>(value: T) -> Json<U>
where
    U: From<T>,
{
    Json(U::from(value))
}

pub(crate) fn json_vec_from<T, U>(items: Vec<T>) -> Json<Vec<U>>
where
    U: From<T>,
{
    Json(items.into_iter().map(U::from).collect())
}

pub(crate) fn created_json_from<T, U>(value: T) -> (StatusCode, Json<U>)
where
    U: From<T>,
{
    (StatusCode::CREATED, json_from(value))
}

pub(crate) fn created_json<T>(value: T) -> (StatusCode, Json<T>) {
    (StatusCode::CREATED, Json(value))
}

pub(crate) fn empty_json() -> Json<Value> {
    Json(json!({}))
}

pub(crate) fn status_json(status: &'static str) -> Json<Value> {
    Json(json!({ "status": status }))
}

pub(crate) async fn filter_users_with_shared_rooms(
    state: &AppState,
    current_user_id: &str,
    requested_users: &[String],
) -> Vec<String> {
    let mut allowed = vec![current_user_id.to_string()];

    for user_id in requested_users {
        if user_id == current_user_id {
            continue;
        }

        let shared = state
            .services
            .member_storage
            .share_common_room(current_user_id, user_id)
            .await
            .unwrap_or(false);

        if shared {
            allowed.push(user_id.clone());
        }
    }

    allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Source(i32);

    #[derive(Debug, PartialEq, serde::Serialize)]
    struct Target {
        value: i32,
    }

    impl From<Source> for Target {
        fn from(value: Source) -> Self {
            Self { value: value.0 }
        }
    }

    #[test]
    fn test_require_found_returns_inner_value() {
        let value = require_found(Some(Source(1)), "missing").expect("value should exist");

        assert_eq!(value, Source(1));
    }

    #[test]
    fn test_require_found_returns_not_found_error() {
        let error = require_found::<Source>(None, "missing").expect_err("missing should fail");

        match error {
            ApiError::NotFound(message) => assert!(message.contains("missing")),
            other => panic!("expected not found error, got {:?}", other),
        }
    }

    #[test]
    fn test_json_from_converts_value() {
        let Json(value) = json_from::<_, Target>(Source(7));

        assert_eq!(value, Target { value: 7 });
    }

    #[test]
    fn test_json_vec_from_converts_all_items() {
        let Json(values) = json_vec_from::<_, Target>(vec![Source(1), Source(2)]);

        assert_eq!(values, vec![Target { value: 1 }, Target { value: 2 }]);
    }

    #[test]
    fn test_created_json_from_sets_created_status() {
        let (status, Json(value)) = created_json_from::<_, Target>(Source(9));

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(value, Target { value: 9 });
    }

    #[test]
    fn test_created_json_sets_created_status_without_conversion() {
        let (status, Json(value)) = created_json(Source(11));

        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(value, Source(11));
    }

    #[test]
    fn test_empty_json_returns_empty_object() {
        let Json(value) = empty_json();

        assert_eq!(value, json!({}));
    }

    #[test]
    fn test_status_json_returns_status_object() {
        let Json(value) = status_json("ok");

        assert_eq!(value, json!({ "status": "ok" }));
    }
}

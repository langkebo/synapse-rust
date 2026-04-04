use crate::common::error::ApiError;
use crate::storage::thread::{
    CreateThreadReplyParams, CreateThreadRootParams, ThreadListParams, ThreadReadReceipt,
    ThreadReply, ThreadRoot, ThreadStorage, ThreadSubscription, ThreadSummary,
};
use std::sync::Arc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateThreadRequest {
    pub room_id: String,
    pub root_event_id: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateReplyRequest {
    pub room_id: String,
    pub thread_id: String,
    pub event_id: String,
    pub root_event_id: String,
    pub content: serde_json::Value,
    pub in_reply_to_event_id: Option<String>,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetThreadRequest {
    pub room_id: String,
    pub thread_id: String,
    pub include_replies: bool,
    pub reply_limit: Option<i32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ListThreadsRequest {
    pub room_id: String,
    pub limit: Option<i32>,
    pub from: Option<String>,
    pub include_all: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubscribeRequest {
    pub room_id: String,
    pub thread_id: String,
    pub user_id: String,
    pub notification_level: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarkReadRequest {
    pub room_id: String,
    pub thread_id: String,
    pub user_id: String,
    pub event_id: String,
    pub origin_server_ts: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThreadListResponse {
    pub threads: Vec<ThreadSummary>,
    pub next_batch: Option<String>,
    pub total: i32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ThreadDetailResponse {
    pub root: ThreadRoot,
    pub replies: Vec<ThreadReply>,
    pub reply_count: i32,
    pub participants: Vec<String>,
    pub summary: Option<ThreadSummary>,
    pub user_receipt: Option<ThreadReadReceipt>,
    pub user_subscription: Option<ThreadSubscription>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UnreadThreadsResponse {
    pub threads: Vec<ThreadReadReceipt>,
    pub total_unread: i32,
    pub total_threads: i32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SubscribedThreadsResponse {
    pub threads: Vec<ThreadSummary>,
    pub subscribed: Vec<ThreadSubscription>,
}

#[derive(Clone)]
pub struct ThreadService {
    storage: Arc<ThreadStorage>,
}

impl ThreadService {
    pub fn new(storage: Arc<ThreadStorage>) -> Self {
        Self { storage }
    }

    pub async fn create_thread(
        &self,
        sender: &str,
        request: CreateThreadRequest,
    ) -> Result<ThreadRoot, ApiError> {
        info!(
            room_id = %request.room_id,
            root_event_id = %request.root_event_id,
            sender = %sender,
            "Creating new thread"
        );

        let thread_id = format!("${}", uuid::Uuid::new_v4().simple());

        let params = CreateThreadRootParams {
            room_id: request.room_id,
            root_event_id: request.root_event_id,
            sender: sender.to_string(),
            thread_id: Some(thread_id.clone()),
        };

        let thread_root = self.storage.create_thread_root(params).await.map_err(|e| {
            warn!(error = %e, "Failed to create thread root");
            ApiError::internal(format!("Failed to create thread: {}", e))
        })?;

        self.storage
            .create_thread_relation(
                &thread_root.room_id,
                &thread_root.root_event_id,
                &thread_root.root_event_id,
                "m.thread",
                thread_root.thread_id.as_deref(),
                false,
            )
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to create thread relation");
                ApiError::internal(format!("Failed to create thread relation: {}", e))
            })?;

        debug!(thread_id = %thread_id, "Thread created successfully");
        Ok(thread_root)
    }

    pub async fn add_reply(
        &self,
        sender: &str,
        request: CreateReplyRequest,
    ) -> Result<ThreadReply, ApiError> {
        info!(
            room_id = %request.room_id,
            thread_id = %request.thread_id,
            event_id = %request.event_id,
            sender = %sender,
            "Adding reply to thread"
        );

        let thread_root = self
            .storage
            .get_thread_root(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get thread root: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Thread not found"))?;

        if thread_root.is_fetched {
            return Err(ApiError::bad_request(
                "Thread is frozen and cannot accept new replies",
            ));
        }

        let params = CreateThreadReplyParams {
            room_id: request.room_id,
            thread_id: request.thread_id,
            event_id: request.event_id,
            root_event_id: request.root_event_id,
            sender: sender.to_string(),
            in_reply_to_event_id: request.in_reply_to_event_id,
            content: request.content,
            origin_server_ts: request.origin_server_ts,
        };

        let reply = self
            .storage
            .create_thread_reply(params)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to create thread reply");
                ApiError::internal(format!("Failed to create reply: {}", e))
            })?;

        self.storage
            .create_thread_relation(
                &reply.room_id,
                &reply.event_id,
                &reply.root_event_id,
                "m.thread",
                Some(&reply.thread_id),
                false,
            )
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to create reply relation");
                ApiError::internal(format!("Failed to create reply relation: {}", e))
            })?;

        debug!(event_id = %reply.event_id, "Reply added successfully");
        Ok(reply)
    }

    pub async fn get_thread(
        &self,
        request: GetThreadRequest,
        user_id: Option<&str>,
    ) -> Result<ThreadDetailResponse, ApiError> {
        debug!(
            room_id = %request.room_id,
            thread_id = %request.thread_id,
            "Getting thread details"
        );

        let root = self
            .storage
            .get_thread_root(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get thread root: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Thread not found"))?;

        let replies = if request.include_replies {
            self.storage
                .get_thread_replies(
                    &request.room_id,
                    &request.thread_id,
                    request.reply_limit,
                    None,
                )
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get replies: {}", e)))?
        } else {
            vec![]
        };

        let reply_count = self
            .storage
            .get_reply_count(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get reply count: {}", e)))?;

        let participants = self
            .storage
            .get_thread_participants(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get participants: {}", e)))?;

        let summary = self
            .storage
            .get_thread_summary(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get summary: {}", e)))?;

        let (user_receipt, user_subscription) = if let Some(uid) = user_id {
            let receipt = self
                .storage
                .get_read_receipt(&request.room_id, &request.thread_id, uid)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get receipt: {}", e)))?;

            let subscription = self
                .storage
                .get_thread_subscription(&request.room_id, &request.thread_id, uid)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get subscription: {}", e)))?;

            (receipt, subscription)
        } else {
            (None, None)
        };

        Ok(ThreadDetailResponse {
            root,
            replies,
            reply_count,
            participants,
            summary,
            user_receipt,
            user_subscription,
        })
    }

    pub async fn list_threads(
        &self,
        request: ListThreadsRequest,
    ) -> Result<ThreadListResponse, ApiError> {
        debug!(
            room_id = %request.room_id,
            limit = ?request.limit,
            "Listing threads"
        );

        let params = ThreadListParams {
            room_id: request.room_id,
            limit: request.limit,
            from: request.from,
            include_all: request.include_all,
        };

        let roots = self
            .storage
            .list_thread_roots(params)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to list threads: {}", e)))?;

        let mut summaries = Vec::new();
        for root in &roots {
            if let Some(summary) = self
                .storage
                .get_thread_summary(&root.room_id, root.thread_id.as_deref().unwrap_or_default())
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get summary: {}", e)))?
            {
                summaries.push(summary);
            } else {
                summaries.push(ThreadSummary {
                    id: root.id,
                    room_id: root.room_id.clone(),
                    thread_id: root.thread_id.clone().unwrap_or_default(),
                    root_event_id: root.root_event_id.clone(),
                    root_sender: root.sender.clone(),
                    root_content: serde_json::json!({}),
                    root_origin_server_ts: root.created_ts,
                    latest_event_id: root.last_reply_event_id.clone(),
                    latest_sender: root.last_reply_sender.clone(),
                    latest_content: None,
                    latest_origin_server_ts: root.last_reply_ts,
                    reply_count: root.reply_count as i32,
                    participants: root.participants.clone().unwrap_or(serde_json::json!([])),
                    is_frozen: root.is_fetched,
                    created_ts: root.created_ts,
                    updated_ts: root.updated_ts.unwrap_or(root.created_ts),
                });
            }
        }

        let next_batch = roots.last().and_then(|r| r.thread_id.clone());
        let total = summaries.len() as i32;

        Ok(ThreadListResponse {
            threads: summaries,
            next_batch,
            total,
        })
    }

    pub async fn list_all_threads(
        &self,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<ThreadListResponse, ApiError> {
        let roots = self
            .storage
            .list_all_thread_roots(limit, from)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to list global threads: {}", e)))?;

        let mut summaries = Vec::new();
        for root in &roots {
            let thread_id = root.thread_id.clone().unwrap_or_default();
            if let Some(summary) = self
                .storage
                .get_thread_summary(&root.room_id, &thread_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get thread summary: {}", e)))?
            {
                summaries.push(summary);
            } else {
                summaries.push(ThreadSummary {
                    id: root.id,
                    room_id: root.room_id.clone(),
                    thread_id,
                    root_event_id: root.root_event_id.clone(),
                    root_sender: root.sender.clone(),
                    root_content: serde_json::json!({}),
                    root_origin_server_ts: root.created_ts,
                    latest_event_id: root.last_reply_event_id.clone(),
                    latest_sender: root.last_reply_sender.clone(),
                    latest_content: None,
                    latest_origin_server_ts: root.last_reply_ts,
                    reply_count: root.reply_count as i32,
                    participants: root.participants.clone().unwrap_or(serde_json::json!([])),
                    is_frozen: root.is_fetched,
                    created_ts: root.created_ts,
                    updated_ts: root.updated_ts.unwrap_or(root.created_ts),
                });
            }
        }

        let next_batch = roots.last().and_then(|r| r.thread_id.clone());
        let total = summaries.len() as i32;

        Ok(ThreadListResponse {
            threads: summaries,
            next_batch,
            total,
        })
    }

    pub async fn subscribe(
        &self,
        request: SubscribeRequest,
    ) -> Result<ThreadSubscription, ApiError> {
        let thread_root = self
            .storage
            .get_thread_root(&request.room_id, &request.thread_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get thread root: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Thread not found"))?;

        if thread_root.is_fetched {
            return Err(ApiError::bad_request("Cannot subscribe to a frozen thread"));
        }

        let valid_levels = ["all", "mentions", "none"];
        if !valid_levels.contains(&request.notification_level.as_str()) {
            return Err(ApiError::bad_request("Invalid notification level"));
        }

        self.storage
            .subscribe_to_thread(
                &request.room_id,
                &request.thread_id,
                &request.user_id,
                &request.notification_level,
            )
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to subscribe to thread");
                ApiError::internal(format!("Failed to subscribe: {}", e))
            })
    }

    pub async fn unsubscribe(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<(), ApiError> {
        self.storage
            .unsubscribe_from_thread(room_id, thread_id, user_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to unsubscribe from thread");
                ApiError::internal(format!("Failed to unsubscribe: {}", e))
            })
    }

    pub async fn mute_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<ThreadSubscription, ApiError> {
        self.storage
            .mute_thread(room_id, thread_id, user_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to mute thread");
                ApiError::internal(format!("Failed to mute thread: {}", e))
            })
    }

    pub async fn mark_read(&self, request: MarkReadRequest) -> Result<ThreadReadReceipt, ApiError> {
        self.storage
            .update_read_receipt(
                &request.room_id,
                &request.thread_id,
                &request.user_id,
                &request.event_id,
                request.origin_server_ts,
            )
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to mark thread as read");
                ApiError::internal(format!("Failed to mark as read: {}", e))
            })
    }

    pub async fn get_unread_threads(
        &self,
        user_id: &str,
        room_id: Option<&str>,
    ) -> Result<UnreadThreadsResponse, ApiError> {
        let threads = self
            .storage
            .get_threads_with_unread(user_id, room_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to get unread threads");
                ApiError::internal(format!("Failed to get unread threads: {}", e))
            })?;

        let total_unread = threads.len() as i32;
        let total_threads = threads.len() as i32;

        Ok(UnreadThreadsResponse {
            threads,
            total_unread,
            total_threads,
        })
    }

    pub async fn get_subscribed_threads(
        &self,
        user_id: &str,
        limit: Option<i32>,
    ) -> Result<SubscribedThreadsResponse, ApiError> {
        let subscriptions = self
            .storage
            .get_user_thread_subscriptions(user_id, limit)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get subscriptions: {}", e)))?;

        let mut threads = Vec::new();
        for subscription in &subscriptions {
            if let Some(summary) = self
                .storage
                .get_thread_summary(&subscription.room_id, &subscription.thread_id)
                .await
                .map_err(|e| {
                    ApiError::internal(format!("Failed to get subscribed thread: {}", e))
                })?
            {
                threads.push(summary);
            }
        }

        Ok(SubscribedThreadsResponse {
            threads,
            subscribed: subscriptions,
        })
    }

    pub async fn delete_thread(&self, room_id: &str, thread_id: &str) -> Result<(), ApiError> {
        self.storage
            .delete_thread(room_id, thread_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to delete thread");
                ApiError::internal(format!("Failed to delete thread: {}", e))
            })
    }

    pub async fn get_thread_statistics(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<crate::storage::thread::ThreadStatistics>, ApiError> {
        self.storage
            .get_thread_statistics(room_id, thread_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to get thread statistics");
                ApiError::internal(format!("Failed to get statistics: {}", e))
            })
    }

    pub async fn search_threads(
        &self,
        room_id: &str,
        query: &str,
        limit: Option<i32>,
    ) -> Result<Vec<ThreadSummary>, ApiError> {
        self.storage
            .search_threads(room_id, query, limit)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to search threads");
                ApiError::internal(format!("Failed to search threads: {}", e))
            })
    }

    pub async fn freeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), ApiError> {
        self.storage
            .freeze_thread(room_id, thread_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to freeze thread");
                ApiError::internal(format!("Failed to freeze thread: {}", e))
            })
    }

    pub async fn unfreeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), ApiError> {
        self.storage
            .unfreeze_thread(room_id, thread_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to unfreeze thread");
                ApiError::internal(format!("Failed to unfreeze thread: {}", e))
            })
    }

    pub async fn redact_reply(&self, room_id: &str, event_id: &str) -> Result<(), ApiError> {
        self.storage
            .mark_reply_redacted(room_id, event_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to redact reply");
                ApiError::internal(format!("Failed to redact reply: {}", e))
            })
    }

    pub async fn edit_reply(&self, room_id: &str, event_id: &str) -> Result<(), ApiError> {
        self.storage
            .mark_reply_edited(room_id, event_id)
            .await
            .map_err(|e| {
                warn!(error = %e, "Failed to edit reply");
                ApiError::internal(format!("Failed to edit reply: {}", e))
            })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_create_thread_request() {
        let request = super::CreateThreadRequest {
            room_id: "!room:example.com".to_string(),
            root_event_id: "$event:example.com".to_string(),
        };
        assert_eq!(request.room_id, "!room:example.com");
        assert_eq!(request.root_event_id, "$event:example.com");
    }

    #[test]
    fn test_create_reply_request() {
        let request = super::CreateReplyRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            event_id: "$reply:example.com".to_string(),
            root_event_id: "$root:example.com".to_string(),
            content: serde_json::json!({"msgtype": "m.text", "body": "Reply"}),
            in_reply_to_event_id: Some("$prev:example.com".to_string()),
            origin_server_ts: 1234567890,
        };
        assert_eq!(request.thread_id, "$thread:example.com");
        assert!(request.in_reply_to_event_id.is_some());
    }

    #[test]
    fn test_get_thread_request() {
        let request = super::GetThreadRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            include_replies: true,
            reply_limit: Some(100),
        };
        assert!(request.include_replies);
        assert_eq!(request.reply_limit, Some(100));
    }

    #[test]
    fn test_list_threads_request() {
        let request = super::ListThreadsRequest {
            room_id: "!room:example.com".to_string(),
            limit: Some(50),
            from: Some("batch_token".to_string()),
            include_all: false,
        };
        assert_eq!(request.limit, Some(50));
        assert!(!request.include_all);
    }

    #[test]
    fn test_subscribe_request() {
        let request = super::SubscribeRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            notification_level: "all".to_string(),
        };
        assert_eq!(request.notification_level, "all");
    }

    #[test]
    fn test_mark_read_request() {
        let request = super::MarkReadRequest {
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            event_id: "$event:example.com".to_string(),
            origin_server_ts: 1234567890,
        };
        assert_eq!(request.user_id, "@user:example.com");
    }

    #[test]
    fn test_thread_list_response_serialization() {
        let response = super::ThreadListResponse {
            threads: vec![],
            next_batch: Some("token".to_string()),
            total: 0,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("threads"));
        assert!(json.contains("total"));
    }

    #[test]
    fn test_unread_threads_response() {
        let response = super::UnreadThreadsResponse {
            threads: vec![],
            total_unread: 5,
            total_threads: 10,
        };
        assert_eq!(response.total_unread, 5);
        assert_eq!(response.total_threads, 10);
    }

    #[test]
    fn test_notification_levels() {
        let valid_levels = ["all", "mentions", "none"];
        for level in valid_levels {
            assert!(matches!(level, "all" | "mentions" | "none"));
        }
    }

    #[test]
    fn test_thread_root_structure() {
        let root = crate::storage::thread::ThreadRoot {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: Some("$thread:example.com".to_string()),
            root_event_id: "$root:example.com".to_string(),
            sender: "@user:example.com".to_string(),
            reply_count: 5,
            last_reply_event_id: Some("$last:example.com".to_string()),
            last_reply_sender: Some("@replier:example.com".to_string()),
            last_reply_ts: Some(1234567890),
            participants: Some(serde_json::json!(["@user:example.com"])),
            is_fetched: false,
            created_ts: 1234567800,
            updated_ts: Some(1234567890),
        };
        assert_eq!(root.reply_count, 5);
        assert!(!root.is_fetched);
    }

    #[test]
    fn test_thread_reply_structure() {
        let reply = crate::storage::thread::ThreadReply {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            event_id: "$reply:example.com".to_string(),
            root_event_id: "$root:example.com".to_string(),
            sender: "@user:example.com".to_string(),
            in_reply_to_event_id: Some("$prev:example.com".to_string()),
            content: serde_json::json!({"msgtype": "m.text"}),
            origin_server_ts: 1234567890,
            is_redacted: false,
            is_edited: false,
            created_ts: 1234567890,
        };
        assert!(!reply.is_redacted);
        assert!(!reply.is_edited);
    }

    #[test]
    fn test_thread_subscription_structure() {
        let subscription = crate::storage::thread::ThreadSubscription {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            notification_level: "all".to_string(),
            is_muted: false,
            subscribed_ts: 1234567890,
            updated_ts: 1234567890,
        };
        assert_eq!(subscription.notification_level, "all");
        assert!(!subscription.is_muted);
    }

    #[test]
    fn test_thread_read_receipt_structure() {
        let receipt = crate::storage::thread::ThreadReadReceipt {
            id: 1,
            room_id: "!room:example.com".to_string(),
            thread_id: "$thread:example.com".to_string(),
            user_id: "@user:example.com".to_string(),
            last_read_event_id: Some("$event:example.com".to_string()),
            last_read_ts: 1234567890,
            unread_count: 3,
            updated_ts: 1234567890,
        };
        assert_eq!(receipt.unread_count, 3);
    }
}

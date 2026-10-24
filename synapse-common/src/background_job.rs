//! Background job runner (fire-and-forget tasks with panic capture).

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
/// Represents BackgroundJob; see per-variant docs.
pub enum BackgroundJob {
    /// `SendEmail` variant.
    SendEmail {
        /// Recipient email address.
        to: String,
        /// Email subject line.
        subject: String,
        /// Email body content.
        body: String,
    },
    /// `ProcessMedia` variant.
    ProcessMedia {
        /// Local media repository file id.
        file_id: String,
    },
    /// `FederationTransaction` variant.
    FederationTransaction {
        /// Federation transaction id.
        txn_id: String,
        /// Destination homeserver.
        destination: String,
    },
    /// `Generic` variant.
    Generic {
        /// Job name.
        name: String,
        /// Arbitrary JSON payload.
        payload: serde_json::Value,
    },
    /// `RedactEvent` variant.
    RedactEvent {
        /// Room id where the event lives.
        room_id: String,
        /// Event id to redact.
        event_id: String,
        /// Optional reason for the redaction.
        reason: Option<String>,
    },
    /// `DelayedEventProcessing` variant.
    DelayedEventProcessing {
        /// Event id whose processing is delayed.
        event_id: String,
    },
}

/// worker 消费一个 job 时**实际能做到什么**。
///
/// 存在理由（2026-10-24 实测）：`src/bin/synapse_worker.rs` 的 handler 里有 4 个分支
/// 只是 `sleep(...)` + 一行"已完成"的日志，例如
/// `ProcessMedia` 打的是 "Thumbnails generated for {id}"，但它一张缩略图都没生成。
/// 于是"作业被消费了""结果完成"与"什么都没做"在指标与日志里完全一致 ——
/// 这正是审计里最容易被读成"已实现"的形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerSupport {
    /// worker 真的有实现。
    Handled,
    /// worker 不做实事，但结果**由别的路径保证**（懒生成 / 另一个调度器）。
    RedundantNoOp,
    /// 没有实现，也没有替代路径 —— 消费掉等于丢弃。
    Unimplemented,
}

impl BackgroundJob {
    /// 该 job 在 `synapse_worker` 里被处理到什么程度。
    pub fn worker_support(&self) -> WorkerSupport {
        match self {
            Self::SendEmail { .. } | Self::RedactEvent { .. } => WorkerSupport::Handled,
            Self::ProcessMedia { .. } => WorkerSupport::RedundantNoOp,
            Self::DelayedEventProcessing { .. } => WorkerSupport::RedundantNoOp,
            Self::FederationTransaction { .. } | Self::Generic { .. } => WorkerSupport::Unimplemented,
        }
    }

    /// 分类的**理由**，用于日志与文档。
    pub const fn worker_support_reason(&self) -> &'static str {
        match self {
            Self::SendEmail { .. } => "worker 通过 SMTP 真实投递（process_send_email_job）",
            Self::RedactEvent { .. } => "worker 调用 event_storage.redact_event_content 真实擦除事件内容",
            Self::ProcessMedia { .. } => {
                "worker 只做预热；缩略图由 media_service::get_thumbnail 的 cache-miss 路径懒生成"
            }
            Self::DelayedEventProcessing { .. } => "由 room/scheduled_dispatch 的调度器处理，worker 这一支是冗余的",
            Self::FederationTransaction { .. } => "仓内无生产者（federation 由主服务同步发送），worker 无实现",
            Self::Generic { .. } => {
                "仓内唯一生产者是 device_sync::notify_device_revocation，而 worker 对该 name 无实现"
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_send_email_job() {
        let job = BackgroundJob::SendEmail {
            to: "user@example.com".to_string(),
            subject: "Test".to_string(),
            body: "Hello".to_string(),
        };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("user@example.com"));
        assert!(json.contains("Test"));
    }

    #[test]
    fn test_process_media_job() {
        let job = BackgroundJob::ProcessMedia { file_id: "abc123".to_string() };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("abc123"));
    }

    #[test]
    fn test_federation_transaction_job() {
        let job = BackgroundJob::FederationTransaction {
            txn_id: "txn123".to_string(),
            destination: "matrix.org".to_string(),
        };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("txn123"));
        assert!(json.contains("matrix.org"));
    }

    #[test]
    fn test_generic_job() {
        let job =
            BackgroundJob::Generic { name: "test_task".to_string(), payload: serde_json::json!({"key": "value"}) };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("test_task"));
    }

    #[test]
    fn test_redact_event_job() {
        let job = BackgroundJob::RedactEvent {
            room_id: "!room:example.com".to_string(),
            event_id: "$event".to_string(),
            reason: Some("spam".to_string()),
        };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("!room:example.com"));
        assert!(json.contains("spam"));
    }

    #[test]
    fn test_redact_event_job_no_reason() {
        let job = BackgroundJob::RedactEvent {
            room_id: "!room:example.com".to_string(),
            event_id: "$event".to_string(),
            reason: None,
        };
        let json = serde_json::to_string(&job).unwrap();
        assert!(json.contains("!room:example.com"));
    }

    #[test]
    fn test_job_deserialization() {
        let json = r#"{"SendEmail":{"to":"user@test.com","subject":"Hi","body":"Hello"}}"#;
        let job: BackgroundJob = serde_json::from_str(json).unwrap();
        match job {
            BackgroundJob::SendEmail { to, subject, body } => {
                assert_eq!(to, "user@test.com");
                assert_eq!(subject, "Hi");
                assert_eq!(body, "Hello");
            }
            _ => panic!("Expected SendEmail variant"),
        }
    }

    /// 分类必须覆盖每个变体，且**不许**把没实现的标成 `Handled`。
    #[test]
    fn worker_support_is_explicit_for_every_variant() {
        use WorkerSupport::{Handled, RedundantNoOp, Unimplemented};

        let cases = [
            (BackgroundJob::SendEmail { to: "a@b.c".into(), subject: "s".into(), body: "b".into() }, Handled),
            (BackgroundJob::RedactEvent { room_id: "!r:x".into(), event_id: "$e".into(), reason: None }, Handled),
            (BackgroundJob::ProcessMedia { file_id: "f".into() }, RedundantNoOp),
            (BackgroundJob::DelayedEventProcessing { event_id: "$e".into() }, RedundantNoOp),
            (BackgroundJob::FederationTransaction { txn_id: "t".into(), destination: "x".into() }, Unimplemented),
            (
                BackgroundJob::Generic { name: "notify_device_revocation".into(), payload: serde_json::Value::Null },
                Unimplemented,
            ),
        ];

        for (job, expected) in cases {
            assert_eq!(job.worker_support(), expected, "`{job:?}` 的分类变了：改分类必须同时改 worker 实现");
            assert!(!job.worker_support_reason().is_empty(), "`{job:?}` 必须给出分类理由");
        }
    }
}

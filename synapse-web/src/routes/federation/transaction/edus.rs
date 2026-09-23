//! EDU (Ephemeral Data Unit) processing for inbound federation transactions.
//!
//! Extracted from `transaction.rs` to reduce the monolithic `send_transaction`
//! handler. This module handles the full lifecycle of inbound EDU dispatch:
//! semaphore acquisition, origin-level permitting, presence backoff, per-type
//! rate limiting, and aggregate metric reporting.

use crate::federation::EduDispatcher;
use crate::routes::context::FederationContext;
use serde_json::Value;
use synapse_common::*;

/// Counters accumulated during EDU processing, reported via tracing at the end.
#[derive(Default, Clone, Debug)]
pub(crate) struct EduProcessingStats {
    pub edus_processed: usize,
    pub total_processed: usize,
    pub total_dropped: usize,
    pub total_errored: usize,
}

/// Process inbound EDUs for a federation transaction.
///
/// This is the extracted EDU-handling sub-routine of `send_transaction`. It:
/// - Acquires the global EDU semaphore (with configurable timeout)
/// - Acquires an origin-level permit (anti-flooding)
/// - Applies presence backoff if active
/// - Iterates up to `inbound_edus_max_per_txn` EDUs, respecting per-type
///   rate limits for presence updates
/// - Dispatches each EDU via `EduDispatcher`
/// - Returns aggregate stats for metric reporting by the caller
///
/// NOTE: This function has multiple parameters because it directly maps
/// the inline EDU processing block from `transaction.rs`; the parameter
/// count is inherent to the federation EDU dispatch operation.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn process_inbound_edus(
    ctx: &FederationContext,
    origin: &str,
    txn_id: &str,
    request_id: &str,
    edus: &[Value],
    process_inbound_presence_edus: bool,
    inbound_edus_max_per_txn: usize,
    inbound_presence_updates_max_per_txn: usize,
) -> Result<EduProcessingStats, ApiError> {
    let mut stats = EduProcessingStats::default();
    let mut total_processed = 0usize;
    let mut total_dropped = 0usize;
    let mut total_errored = 0usize;

    super::super::increment_gauge(ctx, "federation_inbound_edu_in_flight");

    let result =
        async {
            let (_global_permit, wait_ms) = super::super::acquire_with_timeout(
                ctx.federation_inbound_edu_semaphore.clone(),
                ctx.config.federation.inbound_edu_acquire_timeout_ms,
            )
            .await?;
            super::super::observe_histogram(ctx, "federation_inbound_edu_wait_ms", wait_ms as f64);

            let _origin_permit = super::acquire_origin_edu_permit(ctx, origin).await?.0;

            let backoff_ms = super::get_presence_backoff_remaining_ms(ctx, origin).await;
            if backoff_ms.is_some() {
                super::super::increment_counter(ctx, "federation_inbound_presence_backoff_total");
                ::tracing::debug!(
                    "Skipping presence EDU processing for origin {} due to backoff {backoff_ms:?}ms",
                    origin,
                );
            }

            for edu in edus.iter().take(inbound_edus_max_per_txn) {
                stats.edus_processed += 1;
                let edu_type_str = edu.get("edu_type").and_then(|v| v.as_str()).unwrap_or("");

                if edu_type_str == "m.presence" && !process_inbound_presence_edus {
                    continue;
                }
                if edu_type_str == "m.presence" && super::get_presence_backoff_remaining_ms(ctx, origin).await.is_some()
                {
                    continue;
                }

                let remaining = if edu_type_str == "m.presence" {
                    inbound_presence_updates_max_per_txn.saturating_sub(total_processed)
                } else {
                    inbound_edus_max_per_txn
                };

                if remaining == 0 {
                    continue;
                }

                match EduDispatcher::dispatch(ctx, origin, edu, remaining).await {
                    Some(result) => {
                        total_processed += result.processed;
                        total_dropped += result.dropped;
                        total_errored += result.errored;
                        if result.errored > 0 {
                            break;
                        }
                    }
                    None => {
                        ::tracing::trace!(
                            request_id = %request_id,
                            txn_id = %txn_id,
                            origin = %origin,
                            edu_type = edu_type_str,
                            "Skipping unknown EDU type"
                        );
                    }
                }
            }
            Ok::<(), ApiError>(())
        }
        .await;

    stats.total_processed = total_processed;
    stats.total_dropped = total_dropped;
    stats.total_errored = total_errored;

    if let Err(error) = result {
        if error.is_rate_limited() {
            super::super::increment_counter(ctx, "federation_inbound_edu_limited_total");
        } else {
            super::super::increment_counter(ctx, "federation_inbound_edu_error_total");
            ::tracing::warn!(
                request_id = %request_id,
                txn_id = %txn_id,
                origin = %origin,
                error = %error,
                "Failed to process inbound EDUs"
            );
        }
    }

    super::super::decrement_gauge(ctx, "federation_inbound_edu_in_flight");

    Ok(stats)
}

/// Log the EDU processing summary (extracted to keep `send_transaction` focused).
pub(crate) fn log_edu_summary(
    request_id: &str,
    txn_id: &str,
    origin: &str,
    pdu_count: usize,
    edu_count: usize,
    stats: &EduProcessingStats,
) {
    ::tracing::debug!(
        request_id = %request_id,
        txn_id = %txn_id,
        origin = %origin,
        pdu_count = pdu_count,
        edu_count = edu_count,
        edus_processed = stats.edus_processed,
        edu_updates_processed = stats.total_processed,
        edu_updates_dropped = stats.total_dropped,
        edu_updates_errored = stats.total_errored,
        "Inbound federation EDU processing summary"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edu_processing_stats_default() {
        // Default::default() should initialize all counters to 0
        let stats = EduProcessingStats::default();
        assert_eq!(stats.edus_processed, 0);
        assert_eq!(stats.total_processed, 0);
        assert_eq!(stats.total_dropped, 0);
        assert_eq!(stats.total_errored, 0);
    }

    #[test]
    fn test_edu_processing_stats_clone() {
        // Stats should be clonable for aggregation across tasks
        let stats1 = EduProcessingStats { edus_processed: 10, total_processed: 20, total_dropped: 5, total_errored: 2 };
        let stats2 = stats1.clone();
        assert_eq!(stats1.edus_processed, stats2.edus_processed);
        assert_eq!(stats1.total_processed, stats2.total_processed);
        assert_eq!(stats1.total_dropped, stats2.total_dropped);
        assert_eq!(stats1.total_errored, stats2.total_errored);
    }

    #[test]
    fn test_edu_processing_stats_debug_format() {
        // Debug impl should produce a readable representation
        let stats = EduProcessingStats { edus_processed: 5, total_processed: 10, total_dropped: 1, total_errored: 0 };
        let debug_str = format!("{stats:?}");
        assert!(debug_str.contains("EduProcessingStats"));
        assert!(debug_str.contains("edus_processed"));
        assert!(debug_str.contains("total_processed"));
    }

    #[test]
    fn test_edu_processing_stats_field_names() {
        // Verify the struct has the expected field names (will fail at compile time
        // if the struct definition changes)
        let stats = EduProcessingStats { edus_processed: 1, total_processed: 2, total_dropped: 3, total_errored: 4 };

        assert_eq!(stats.edus_processed, 1);
        assert_eq!(stats.total_processed, 2);
        assert_eq!(stats.total_dropped, 3);
        assert_eq!(stats.total_errored, 4);
    }
}

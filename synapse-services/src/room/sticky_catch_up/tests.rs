//! Tests for the MSC4354 catch-up loop — see `../sticky_catch_up.rs` for the four contracts.
//!
//! Kept as a sibling file (declared with `#[path]` in the parent) because the loop plus
//! its fake seam crosses the 600-line ceiling (`scripts/ci/check_file_size_ratchet.py`).

use super::*;
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;

/// Every call the loop makes against the seam, in order.
#[derive(Debug, Clone, PartialEq)]
enum Call {
    Seed,
    Destinations(i64),
    Rows { room_id: String, after: i64, limit: i64 },
    Send { destination: String, edu: serde_json::Value },
    Advance { room_id: String, destination: String, revision: i64 },
    Reset { room_id: String, destination: String },
}

/// In-memory seam: scripted replies + a call log.
///
/// `rows` is a flat list (the cases all use one room) and `pending_rows` applies
/// the same `revision > after` / `LIMIT` contract the SQL does.
///
/// `cursors` mirrors `room_sticky_event_destinations`: `pending_destinations` reproduces
/// the SQL's two predicates (an owned row past the cursor, `ORDER BY room_id, destination`,
/// `LIMIT`), and `advance_cursor` moves the cursor like `GREATEST`. Without that, the fake
/// would answer every tick with the full backlog and rotation could not be observed.
struct FakeIo {
    server_name: String,
    is_writer: bool,
    seed_error: Option<String>,
    cursors: Mutex<HashMap<(String, String), i64>>,
    destinations_error: Option<String>,
    rows: Vec<StickyDeliveryRow>,
    sends: Mutex<VecDeque<Result<bool, String>>>,
    advance_error: Option<String>,
    reset_error: Option<String>,
    calls: Mutex<Vec<Call>>,
}

impl FakeIo {
    fn new() -> Self {
        Self {
            server_name: "a.example".to_string(),
            is_writer: true,
            seed_error: None,
            cursors: Mutex::new(HashMap::new()),
            destinations_error: None,
            rows: Vec::new(),
            sends: Mutex::new(VecDeque::new()),
            advance_error: None,
            reset_error: None,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn with_destinations(mut self, destinations: Vec<PendingStickyDelivery>) -> Self {
        let cursors =
            destinations.into_iter().map(|item| ((item.room_id, item.destination), item.last_revision)).collect();
        self.cursors = Mutex::new(cursors);
        self
    }

    fn with_rows(mut self, rows: Vec<StickyDeliveryRow>) -> Self {
        self.rows = rows;
        self
    }

    fn with_sends(mut self, sends: Vec<Result<bool, String>>) -> Self {
        self.sends = Mutex::new(sends.into());
        self
    }

    fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    /// EDUs handed to `send_edu`, in order.
    fn sent_edus(&self) -> Vec<serde_json::Value> {
        self.calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::Send { edu, .. } => Some(edu),
                _ => None,
            })
            .collect()
    }

    /// `(destination, revision)` of every cursor advance, in order.
    fn advances(&self) -> Vec<(String, i64)> {
        self.calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::Advance { destination, revision, .. } => Some((destination, revision)),
                _ => None,
            })
            .collect()
    }

    fn row_fetches(&self) -> Vec<i64> {
        self.calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::Rows { after, .. } => Some(after),
                _ => None,
            })
            .collect()
    }
}

#[async_trait::async_trait]
impl StickyCatchUpIo for FakeIo {
    fn server_name(&self) -> &str {
        &self.server_name
    }

    fn is_events_writer(&self) -> bool {
        self.is_writer
    }

    async fn seed_cursors(&self) -> Result<u64, String> {
        self.calls.lock().unwrap().push(Call::Seed);
        match &self.seed_error {
            Some(e) => Err(e.clone()),
            None => Ok(0),
        }
    }

    async fn pending_destinations(&self, limit: i64) -> Result<Vec<PendingStickyDelivery>, String> {
        self.calls.lock().unwrap().push(Call::Destinations(limit));
        if let Some(e) = &self.destinations_error {
            return Err(e.clone());
        }

        let cursors = self.cursors.lock().unwrap();
        let mut pending: Vec<PendingStickyDelivery> = cursors
            .iter()
            .filter(|(_, cursor)| self.rows.iter().any(|row| row.revision > **cursor))
            .map(|((room_id, destination), cursor)| PendingStickyDelivery {
                room_id: room_id.clone(),
                destination: destination.clone(),
                last_revision: *cursor,
            })
            .collect();
        // Same stable order as the SQL, so "which pairs get the cap" is deterministic.
        pending.sort_by(|a, b| (&a.room_id, &a.destination).cmp(&(&b.room_id, &b.destination)));
        pending.truncate(limit.max(0) as usize);
        Ok(pending)
    }

    async fn pending_rows(
        &self,
        room_id: &str,
        after_revision: i64,
        limit: i64,
    ) -> Result<Vec<StickyDeliveryRow>, String> {
        self.calls.lock().unwrap().push(Call::Rows { room_id: room_id.to_string(), after: after_revision, limit });
        Ok(self.rows.iter().filter(|row| row.revision > after_revision).take(limit as usize).cloned().collect())
    }

    async fn send_edu(&self, destination: &str, edu: &serde_json::Value) -> Result<bool, String> {
        self.calls.lock().unwrap().push(Call::Send { destination: destination.to_string(), edu: edu.clone() });
        self.sends.lock().unwrap().pop_front().unwrap_or(Ok(true))
    }

    async fn advance_cursor(&self, room_id: &str, destination: &str, revision: i64) -> Result<(), String> {
        self.calls.lock().unwrap().push(Call::Advance {
            room_id: room_id.to_string(),
            destination: destination.to_string(),
            revision,
        });
        match &self.advance_error {
            Some(e) => Err(e.clone()),
            None => {
                // `GREATEST`, like the SQL: a stale writer must never push the cursor back.
                let mut cursors = self.cursors.lock().unwrap();
                let slot = cursors.entry((room_id.to_string(), destination.to_string())).or_insert(0);
                *slot = (*slot).max(revision);
                Ok(())
            }
        }
    }

    async fn reset_cursor(&self, room_id: &str, destination: &str) -> Result<(), String> {
        self.calls
            .lock()
            .unwrap()
            .push(Call::Reset { room_id: room_id.to_string(), destination: destination.to_string() });
        match &self.reset_error {
            Some(e) => Err(e.clone()),
            None => {
                self.cursors.lock().unwrap().insert((room_id.to_string(), destination.to_string()), 0);
                Ok(())
            }
        }
    }
}

fn destination(room_id: &str, destination: &str, last_revision: i64) -> PendingStickyDelivery {
    PendingStickyDelivery { room_id: room_id.to_string(), destination: destination.to_string(), last_revision }
}

fn pin(revision: i64) -> StickyDeliveryRow {
    StickyDeliveryRow {
        revision,
        event_id: format!("$event{revision}"),
        event_type: "m.room.topic".to_string(),
        is_sticky: true,
        sticky_duration_ms: Some(3_600_000),
        expires_at: None,
    }
}

fn tombstone(revision: i64) -> StickyDeliveryRow {
    StickyDeliveryRow {
        revision,
        event_id: format!("$event{revision}"),
        event_type: "m.room.topic".to_string(),
        is_sticky: false,
        sticky_duration_ms: None,
        expires_at: None,
    }
}

/// (1) 全部确认 ⇒ 游标推进到最新一条，EDU 按行序发出。
#[tokio::test]
async fn tick_advances_cursor_to_last_confirmed_row() {
    let io = FakeIo::new().with_destinations(vec![destination("!r:a", "b.example", 0)]).with_rows(vec![
        pin(1),
        pin(2),
        tombstone(3),
    ]);

    run_catch_up_tick(&io).await;

    assert_eq!(io.sent_edus().len(), 3, "one EDU per pending row");
    assert_eq!(io.advances(), vec![("b.example".to_string(), 3)], "cursor stops at the last row");
    assert_eq!(io.row_fetches(), vec![0], "a short batch ends the drain");
    assert_eq!(io.sent_edus()[0]["room_id"], "!r:a");
    assert_eq!(io.sent_edus()[0]["sender"], "a.example");
}

/// (2) 第 2 条返回 `Ok(false)`（无出站 client）⇒ 只推进到第 1 条且立即停下。
#[tokio::test]
async fn unconfirmed_send_stops_before_advancing() {
    let io = FakeIo::new()
        .with_destinations(vec![destination("!r:a", "b.example", 0)])
        .with_rows(vec![pin(1), pin(2), pin(3)])
        .with_sends(vec![Ok(true), Ok(false)]);

    run_catch_up_tick(&io).await;

    assert_eq!(io.sent_edus().len(), 2, "the unconfirmed row is not re-sent this tick");
    assert_eq!(io.advances(), vec![("b.example".to_string(), 1)], "cursor stays behind the unconfirmed row");
}

/// (3) 首条 `Err` ⇒ 零推进、零后续行。
#[tokio::test]
async fn send_error_does_not_advance() {
    let io = FakeIo::new()
        .with_destinations(vec![destination("!r:a", "b.example", 0)])
        .with_rows(vec![pin(1), pin(2)])
        .with_sends(vec![Err("connection refused".to_string())]);

    run_catch_up_tick(&io).await;

    assert_eq!(io.sent_edus().len(), 1);
    assert!(io.advances().is_empty(), "a failed send must never move the cursor");
}

/// (4) 满批 ⇒ 继续下一页；下一页不满 ⇒ 停下。游标每批推进一次。
#[tokio::test]
async fn full_batch_continues_to_the_next_page() {
    let rows: Vec<_> = (1..=CATCH_UP_BATCH + 1).map(pin).collect();
    let io = FakeIo::new().with_destinations(vec![destination("!r:a", "b.example", 0)]).with_rows(rows);

    run_catch_up_tick(&io).await;

    assert_eq!(io.row_fetches(), vec![0, CATCH_UP_BATCH], "a full batch keeps the drain going");
    assert_eq!(io.sent_edus().len() as i64, CATCH_UP_BATCH + 1);
    assert_eq!(
        io.advances(),
        vec![("b.example".to_string(), CATCH_UP_BATCH), ("b.example".to_string(), CATCH_UP_BATCH + 1)]
    );
}

/// (5) 墓碑与过期行都投影成「移除」，活跃置顶保留时长。
#[tokio::test]
async fn tombstones_and_expired_rows_are_replayed_as_removals() {
    let mut expired = pin(2);
    expired.expires_at = Some(1); // 1970 ⇒ 必然过期
    let io = FakeIo::new().with_destinations(vec![destination("!r:a", "b.example", 0)]).with_rows(vec![
        pin(1),
        expired,
        tombstone(3),
    ]);

    run_catch_up_tick(&io).await;

    let sent = io.sent_edus();
    assert_eq!(sent[0]["content"]["is_sticky"], true, "a live pin keeps its state");
    assert_eq!(sent[0]["content"]["sticky_duration_ms"], 3_600_000);
    assert_eq!(sent[1]["content"]["is_sticky"], false, "an expired pin must be sent as a removal");
    assert_eq!(sent[1]["content"]["sticky_duration_ms"], serde_json::Value::Null);
    assert_eq!(sent[2]["content"]["is_sticky"], false, "the stored tombstone is replayed as-is");
}

/// (6) 播种失败 ⇒ 本轮一条 EDU 都不发。
#[tokio::test]
async fn seed_failure_skips_the_whole_tick() {
    let mut io = FakeIo::new().with_destinations(vec![destination("!r:a", "b.example", 0)]).with_rows(vec![pin(1)]);
    io.seed_error = Some("db down".to_string());

    run_catch_up_tick(&io).await;

    assert_eq!(io.calls(), vec![Call::Seed], "nothing past the seed failure");
}

/// (7) 列待补发对失败 ⇒ 同样零发送。
#[tokio::test]
async fn destination_listing_failure_sends_nothing() {
    let mut io = FakeIo::new().with_rows(vec![pin(1)]);
    io.destinations_error = Some("db down".to_string());

    run_catch_up_tick(&io).await;

    assert!(io.sent_edus().is_empty());
    assert!(io.advances().is_empty());
}

/// (8) 非 `events` 写所有者 ⇒ **零 io 调用**（契约 4 的运行时判据）。
#[tokio::test]
async fn non_writer_instance_is_inert() {
    let mut io =
        FakeIo::new().with_destinations(vec![destination("!r:a", "b.example", 0)]).with_rows(vec![pin(1), pin(2)]);
    io.is_writer = false;

    run_catch_up_tick(&io).await;

    assert_eq!(
        io.calls(),
        Vec::<Call>::new(),
        "a non-writer instance must not seed, list, send or advance — that is what single-writer means"
    );
}

/// (9) 入房补发：先归零游标，再从 0 全量重放。
#[tokio::test]
async fn backfill_resets_then_replays_the_whole_room() {
    let io = FakeIo::new().with_rows(vec![pin(1), pin(2), tombstone(3)]);

    backfill_to_destination(&io, "!r:a", "b.example").await;

    let calls = io.calls();
    assert_eq!(
        calls[0],
        Call::Reset { room_id: "!r:a".to_string(), destination: "b.example".to_string() },
        "the cursor must go back to 0 before replaying"
    );
    assert_eq!(io.sent_edus().len(), 3);
    assert_eq!(io.advances(), vec![("b.example".to_string(), 3)]);
}

/// (10) 目标就是本机 ⇒ 零调用；重置失败 ⇒ 不 drain。
#[tokio::test]
async fn backfill_short_circuits_on_self_and_on_reset_failure() {
    let self_target = FakeIo::new().with_rows(vec![pin(1)]);
    backfill_to_destination(&self_target, "!r:a", "a.example").await;
    assert_eq!(self_target.calls(), Vec::<Call>::new(), "never backfill to ourselves");

    let mut broken = FakeIo::new().with_rows(vec![pin(1)]);
    broken.reset_error = Some("db down".to_string());
    backfill_to_destination(&broken, "!r:a", "b.example").await;
    assert_eq!(broken.calls().len(), 1, "only the failed reset");
    assert!(broken.sent_edus().is_empty());
}

/// (11) destination 数超过每 tick 上限 ⇒ 多轮收敛，每对恰好一次（无饥饿、无重复）。
///
/// 单个 destination 的分页由 (4) 覆盖；这里补的是**跨 destination 的轮转**：
/// `pending_sticky_destinations` 每次只给 `LIMIT 32` 个对，排在前面的对推进后不再满足
/// 「仍有积压」谓词，下一轮自然轮到剩下的——这是「不饿死」的全部机制。
#[tokio::test]
async fn destinations_beyond_the_per_tick_cap_rotate_without_starvation() {
    const TOTAL: i64 = CATCH_UP_DESTINATIONS_PER_TICK + 8;

    // Spread over several rooms so the `(room_id, destination)` ordering is exercised.
    let pairs: Vec<_> =
        (0..TOTAL).map(|i| destination(&format!("!r{:02}:a", i / 10), &format!("peer{i:02}.example"), 0)).collect();
    let io = FakeIo::new().with_destinations(pairs).with_rows(vec![pin(1)]);

    run_catch_up_tick(&io).await;
    let first_tick = io.advances().len();
    assert_eq!(
        first_tick as i64, CATCH_UP_DESTINATIONS_PER_TICK,
        "one tick serves at most the cap — if the fake ignored LIMIT every pair would be done here"
    );

    run_catch_up_tick(&io).await;

    let advances = io.advances();
    assert_eq!(advances.len() as i64, TOTAL, "two ticks cover the whole backlog");
    let mut served: Vec<String> = advances
        .into_iter()
        .map(|(destination, revision)| {
            assert_eq!(revision, 1, "each pair stops at the only row");
            destination
        })
        .collect();
    let before_dedup = served.len();
    served.sort();
    served.dedup();
    assert_eq!(served.len(), before_dedup, "no destination is served twice");
    assert_eq!(served.len() as i64, TOTAL, "no destination is starved");
}

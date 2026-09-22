//! DB-backed pagination performance benchmarks (E4).
//!
//! ## Why this file exists
//!
//! `benches/performance_api_benchmarks.rs` contains two **in-memory
//! simulations** (`pagination_offset_deep_page` / `pagination_keyset_deep_page`)
//! that `scripts/check_pagination_benchmark.py` compares on every benchmark run.
//! Those functions do not touch a database: one walks a synthetic `Vec`, the
//! other binary-searches it. The simulated margin is ~1500x, so **no real
//! `synapse-storage` SQL regression can ever move the ratio below the 30%
//! threshold** — that check is a smoke test for the compute path, not a
//! pagination guard (see
//! `docs/archive/GATE_INTEGRITY_FOLLOWUP_2026-09-19_LOG.md` §9, row E4).
//!
//! This benchmark measures the real thing instead:
//!
//! * the production keyset query, called through
//!   [`synapse_rust::storage::event::EventStorage::get_room_events_paginated_cursor`]
//!   (the exact SQL `/messages` uses), on a **real migrated `events` table**,
//!   at a deep cursor;
//! * the naive `LIMIT/OFFSET` shape that ISSUE-06 replaced, against the same
//!   fixture, for the same page;
//! * the query plan of the production keyset predicate, so a lost index is
//!   reported as such.
//!
//! It emits one machine-readable line on stderr:
//!
//! ```text
//! [perf] pagination rows=... deep_offset=... keyset_deep_us=... \
//!        keyset_shallow_us=... shallow_over_deep_x=... offset_deep_us=... \
//!        gain_x=... index_scan=1 correct=1
//! ```
//!
//! `scripts/ci/pagination_perf_gate.sh` parses that line and fails the build
//! unless the real keyset page is at least `PAGINATION_MIN_GAIN` (default 2x)
//! faster than the offset page, the plan still uses an ordered index, the
//! keyset page is the *same* page as the offset query, and the shallow page is
//! not more than `PAGINATION_MAX_SHALLOW_RATIO` (default 4x) slower than the
//! deep page.
//!
//! ## The alias-shadowing bug this bench found (now fixed)
//!
//! The first measurement of this bench exposed a real production defect: the
//! keyset SQL wrote `ORDER BY origin_server_ts DESC, stream_ordering DESC`
//! unqualified, while `ROOM_EVENT_COLS` selects
//! `COALESCE(origin_server_ts, 0) AS origin_server_ts`. Postgres resolves a
//! bare `ORDER BY` name against an **output** column before an input column, so
//! the sort key became `COALESCE(origin_server_ts, 0)` — `EXPLAIN` printed
//! `Sort Key: (COALESCE(origin_server_ts, '0'::bigint))` — and
//! `idx_events_room_ts_stream` could no longer supply the order. The planner
//! then sorted the *candidate* set ("every row above the cursor"), so a
//! shallower page was genuinely slower and the no-cursor page (the first
//! `/messages` call) was the worst case:
//!
//! ```text
//!   page                          plan (production ROOM_EVENT_COLS)                       exec (force_generic_plan)
//!   BEFORE no cursor (shallowest) Sort(COALESCE(ts,0)) <- Bitmap Heap Scan 30000 rows      18.2 ms
//!   BEFORE 1% cursor              Sort(COALESCE(ts,0)) <- Bitmap Heap Scan 29701 rows      10.6 ms
//!   BEFORE 90% deep cursor        Sort(COALESCE(ts,0)) <- Bitmap Heap Scan  3000 rows       1.2 ms
//!   AFTER  no cursor              Index Scan using idx_events_room_ts_stream (100 rows)     0.049 ms
//!   AFTER  1% cursor              Index Scan using idx_events_room_ts_stream (100 rows)     0.044 ms
//!   AFTER  90% deep cursor        Index Scan using idx_events_room_ts_stream (100 rows)     0.052 ms
//! ```
//!
//! (150k-row local fixture, 30k-row target room + 60 x 2000 filler rooms,
//! `ANALYZE`d, `plan_cache_mode=force_generic_plan`.)
//!
//! `synapse-storage/src/event/pagination.rs` now qualifies every keyset sort
//! key (`ORDER BY events.origin_server_ts, events.stream_ordering`), so the
//! index supplies the order at every depth and the first page is no longer the
//! worst case. `keyset_shallow_us` is therefore **gated**:
//! `PAGINATION_MAX_SHALLOW_RATIO` (default 4x, with an absolute
//! `PAGINATION_SHALLOW_BREACH_FLOOR_US` floor) requires the first page to be no
//! more than a small factor slower than the deep page. Reverting the
//! qualification measured ~10-15x (18.2 ms vs 1.2 ms) and reddens the gate;
//! healthy it is ~0.6-1.4x. The deep and shallow pages are sampled
//! **interleaved** so a transient load spike cannot inflate the ratio on a
//! healthy tree (that failure mode was observed once: 3.2 ms vs 0.32 ms).
//!
//! The `index_scan` probe EXPLAINs the *production shape* — the same
//! `ROOM_EVENT_COLS` select list, the same row-value predicate and the same
//! qualified `ORDER BY` — and requires an ordered `Index Scan` with no `Sort`
//! node. The previous probe used a narrow `SELECT event_id`, which cannot see
//! this bug at all: with no `COALESCE` output column the bare `ORDER BY` binds
//! to the input column, so the proxy stayed green while production sorted
//! every row above the cursor. A proxy shape is exactly how a gate keeps
//! passing while production is broken.
//!
//! The OFFSET baseline selects the same `ROOM_EVENT_COLS` **and the same
//! qualified `ORDER BY`** as the production keyset query, so both sides decode
//! identical rows and neither side is accidentally penalised by the
//! alias-shadowing plan.
//!
//! Sessions pin `plan_cache_mode=force_generic_plan` (see
//! [`connect_bench_pool`]): with the server default (`auto`) Postgres flips
//! this prepared statement between custom and generic plans and the measured
//! keyset cost swings ~10x between runs, which would make any threshold
//! noise. Both sides of the comparison use the same mode.
//!
//! ## Fixture
//!
//! One target room with `TARGET_EVENTS` events plus `OTHER_ROOMS` rooms with
//! `OTHER_ROOM_EVENTS` events each. The other rooms exist so the `room_id`
//! predicate is selective enough for the planner to prefer
//! `idx_events_room_ts_stream` over a sequential scan — the same shape a real
//! homeserver has (many rooms, one deep-paginated room). `ANALYZE` is run
//! before measuring so the planner has real statistics.
//!
//! ## Running
//!
//! Requires a database with the migration baseline applied (the
//! `benchmark.yml::pagination-perf-gate` job seeds one):
//!
//! ```bash
//! BENCHMARK_DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_bench \
//! PAGINATION_GATE_ONLY=1 BENCH_REQUIRE=pagination_db \
//!   cargo bench --locked --bench performance_pagination_benchmarks
//! ```
//!
//! Without `PAGINATION_GATE_ONLY=1` the criterion benchmarks run as usual.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use criterion::{black_box, criterion_group, Criterion};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

use synapse_rust::storage::event::{EventStorage, RoomEvent, ROOM_EVENT_COLS};

/// Room that is deep-paginated.
const TARGET_ROOM: &str = "!bench_pagination:localhost";
/// Prefix shared by the filler rooms that make the target room selective.
const OTHER_ROOM_PREFIX: &str = "!bench_pagination_other";
/// Events in the target room.
const TARGET_EVENTS: i64 = 30_000;
/// Number of filler rooms.
const OTHER_ROOMS: i64 = 10;
/// Events per filler room.
const OTHER_ROOM_EVENTS: i64 = 12_000;
/// Page size, matching `/messages`.
const PAGE_LIMIT: i64 = 100;
/// Fraction of the target room skipped before the measured page (0.90 = deep).
const DEEP_FRACTION: f64 = 0.90;
/// Discarded warm-up iterations per query shape.
const WARMUP_SAMPLES: usize = 5;
/// Recorded iterations per query shape.
const RECORDED_SAMPLES: usize = 15;
/// Base timestamp/stream ordering of the target room fixture.
const TARGET_TS_BASE: i64 = 1_700_000_000_000;
const TARGET_STREAM_BASE: i64 = 5_000_000;
/// Base timestamp/stream ordering of the filler-room fixture.
const OTHER_TS_BASE: i64 = 1_600_000_000_000;
const OTHER_STREAM_BASE: i64 = 4_000_000;
/// Benchmark group registered by the DB-backed measurement.
const PAGINATION_GROUP: &str = "pagination_db";

/// Group names that actually reached their measurement.
static EXECUTED_GROUPS: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// Registers `group` as executed.
fn require_bench_group(group: &'static str) {
    if let Ok(mut executed) = EXECUTED_GROUPS.lock() {
        executed.push(group);
    }
    if required_groups().iter().any(|r| r == group) {
        eprintln!("[bench-guard] required group `{group}` executed");
    }
}

/// Parses `BENCH_REQUIRE` (comma/space separated) into group names.
fn required_groups() -> Vec<String> {
    std::env::var("BENCH_REQUIRE")
        .unwrap_or_default()
        .split([',', ' '])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// After all measurements had a chance to register, verify that every group
/// named in `BENCH_REQUIRE` actually executed.
///
/// A skipped DB-backed measurement must not look like a passing gate: the
/// gate script would otherwise find no `[perf]` line and could be tempted to
/// pass. `scripts/ci/pagination_perf_gate.sh` requires `pagination_db`.
fn enforce_required_groups() {
    let required = required_groups();
    if required.is_empty() {
        return;
    }
    let executed = EXECUTED_GROUPS.lock().map(|g| g.clone()).unwrap_or_default();
    let missing: Vec<&String> = required.iter().filter(|r| !executed.iter().any(|e| e == r)).collect();
    if !missing.is_empty() {
        eprintln!(
            "BENCH_REQUIRE: required benchmark group(s) did not execute: {}.\n\
             Executed groups: {:?}.\n\
             A required benchmark silently skipped — this is a false green. Set \
             BENCHMARK_DATABASE_URL to a reachable benchmark database (default \
             postgresql://synapse:synapse@localhost:5432/synapse_bench).",
            missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "),
            executed
        );
        std::process::exit(1);
    }
    eprintln!("BENCH_REQUIRE: all required group(s) executed: {}", required.join(", "));
}

fn bench_database_url() -> String {
    std::env::var("BENCHMARK_DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://synapse:synapse@localhost:5432/synapse_bench".to_string())
}

/// Connects to the benchmark database. Returns `None` when unreachable so the
/// caller can skip without panicking; the gate's `BENCH_REQUIRE` turns that
/// skip into a non-zero exit.
fn connect_bench_pool(rt: &Runtime) -> Option<Arc<sqlx::PgPool>> {
    let url = bench_database_url();
    // Pin the plan mode. With the server default (`auto`) Postgres flips this
    // exact prepared keyset statement between custom and generic plans and its
    // measured cost swings between ~2ms (bitmap index range on
    // `origin_server_ts`) and ~19ms (bitmap scan of the whole room + sort)
    // across identical runs — enough noise to make any threshold unreliable.
    // `force_generic_plan` is also what `auto` settles on for this statement
    // once the prepared statement is reused (which is exactly how sqlx issues
    // it in production), and both sides of the comparison use the same mode.
    let options = match url.parse::<sqlx::postgres::PgConnectOptions>() {
        Ok(options) => options.options([("plan_cache_mode", "force_generic_plan")]),
        Err(e) => {
            eprintln!("[perf] invalid BENCHMARK_DATABASE_URL `{url}`: {e}; skipping DB-backed pagination benches");
            return None;
        }
    };
    let pool = rt.block_on(async move {
        sqlx::postgres::PgPoolOptions::new().max_connections(4).connect_with(options).await.map(Arc::new)
    });
    match pool {
        Ok(p) => Some(p),
        Err(e) => {
            eprintln!("[perf] cannot reach benchmark database: {e}; skipping DB-backed pagination benches");
            None
        }
    }
}

/// Identifier of the last row skipped before the measured deep page.
fn deep_cursor_index() -> i64 {
    let deep_offset = (TARGET_EVENTS as f64 * DEEP_FRACTION).round() as i64;
    TARGET_EVENTS - deep_offset + 1
}

fn cursor_ts() -> i64 {
    TARGET_TS_BASE + deep_cursor_index()
}

fn cursor_stream() -> i64 {
    TARGET_STREAM_BASE + deep_cursor_index()
}

fn deep_offset() -> i64 {
    (TARGET_EVENTS as f64 * DEEP_FRACTION).round() as i64
}

fn other_room_id(room: i64) -> String {
    format!("{OTHER_ROOM_PREFIX}{room}:localhost")
}

/// Makes the fixture idempotent: removes any previous run's rows first so a
/// re-run cannot double-seed and quietly change the measured depth.
async fn seed_fixture(pool: &Arc<sqlx::PgPool>) -> Result<(), sqlx::Error> {
    let other_like = format!("{OTHER_ROOM_PREFIX}%");

    // Also clear rows by this bench's *event-id* namespace: a fixture left by an
    // earlier run (or hand-seeded) can carry the same `$benchpag*` /
    // `$benchother*` ids under different room_ids, and the INSERT below would then
    // die on `pk_events` — failing closed, but a trap for local reruns (measured
    // 2026-09-19).
    let bench_event_like = "$bench%".to_string();
    sqlx::query("DELETE FROM events WHERE room_id = $1 OR room_id LIKE $2 OR event_id LIKE $3")
        .bind(TARGET_ROOM)
        .bind(&other_like)
        .bind(&bench_event_like)
        .execute(&**pool)
        .await?;
    sqlx::query("DELETE FROM rooms WHERE room_id = $1 OR room_id LIKE $2")
        .bind(TARGET_ROOM)
        .bind(&other_like)
        .execute(&**pool)
        .await?;

    sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, 0) ON CONFLICT (room_id) DO NOTHING")
        .bind(TARGET_ROOM)
        .execute(&**pool)
        .await?;
    for room in 1..=OTHER_ROOMS {
        sqlx::query("INSERT INTO rooms (room_id, created_ts) VALUES ($1, 0) ON CONFLICT (room_id) DO NOTHING")
            .bind(other_room_id(room))
            .execute(&**pool)
            .await?;
    }

    // Target room: unique, strictly increasing origin_server_ts / stream_ordering
    // so the deep cursor position is exactly reproducible.
    sqlx::query(
        r#"INSERT INTO events
             (event_id, room_id, sender, event_type, content, origin_server_ts, stream_ordering, soft_failed)
           SELECT '$benchpag' || i || ':localhost', $1, '@bench:localhost', 'm.room.message',
                  '{"body":"seed"}'::jsonb, $2 + i, $3 + i, FALSE
           FROM generate_series(1, $4) AS i"#,
    )
    .bind(TARGET_ROOM)
    .bind(TARGET_TS_BASE)
    .bind(TARGET_STREAM_BASE)
    .bind(TARGET_EVENTS)
    .execute(&**pool)
    .await?;

    // Filler rooms: keep the target room a minority of the table so the
    // planner picks `idx_events_room_time` instead of a sequential scan.
    sqlx::query(
        r#"INSERT INTO events
             (event_id, room_id, sender, event_type, content, origin_server_ts, stream_ordering, soft_failed)
           SELECT '$benchother' || r || '_' || i || ':localhost', $1 || r || ':localhost',
                  '@bench:localhost', 'm.room.message', '{"body":"seed"}'::jsonb,
                  $2 + r * 1000000 + i, $3 + r * 1000000 + i, FALSE
           FROM generate_series(1, $4) AS r, generate_series(1, $5) AS i"#,
    )
    .bind(OTHER_ROOM_PREFIX)
    .bind(OTHER_TS_BASE)
    .bind(OTHER_STREAM_BASE)
    .bind(OTHER_ROOMS)
    .bind(OTHER_ROOM_EVENTS)
    .execute(&**pool)
    .await?;

    sqlx::query("ANALYZE events").execute(&**pool).await?;
    Ok(())
}

/// Median of non-empty samples; `f64::NAN` for an empty input.
fn median(mut samples: Vec<f64>) -> f64 {
    if samples.is_empty() {
        return f64::NAN;
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    samples[samples.len() / 2]
}

/// Times one production keyset page and returns microseconds.
async fn time_keyset(storage: &EventStorage, from: Option<(i64, Option<i64>)>) -> Result<f64, sqlx::Error> {
    let started = Instant::now();
    let rows = storage.get_room_events_paginated_cursor(TARGET_ROOM, from, PAGE_LIMIT, "b").await?;
    black_box(&rows);
    Ok(started.elapsed().as_secs_f64() * 1_000_000.0)
}

/// Median microseconds for the deep and the shallowest production keyset page.
///
/// The two shapes are sampled **interleaved** (in both orders), not in two
/// sequential blocks: with block sampling a transient load spike that lands on
/// the shallow block inflates `shallow/deep` and looks exactly like the ORDER BY
/// regression (measured on an otherwise healthy tree: 3.2 ms shallow vs 0.32 ms
/// deep → 9.9x). Interleaved, a spike hits both shapes and the ratio stays at
/// the ~1x the fix produces.
async fn sample_keyset_pair(
    storage: &EventStorage,
    deep_from: Option<(i64, Option<i64>)>,
) -> Result<(f64, f64), sqlx::Error> {
    let mut warmup = 0;
    while warmup < WARMUP_SAMPLES {
        black_box(storage.get_room_events_paginated_cursor(TARGET_ROOM, deep_from, PAGE_LIMIT, "b").await?);
        black_box(storage.get_room_events_paginated_cursor(TARGET_ROOM, None, PAGE_LIMIT, "b").await?);
        warmup += 1;
    }
    let mut deep = Vec::with_capacity(RECORDED_SAMPLES);
    let mut shallow = Vec::with_capacity(RECORDED_SAMPLES);
    for i in 0..RECORDED_SAMPLES {
        if i % 2 == 0 {
            deep.push(time_keyset(storage, deep_from).await?);
            shallow.push(time_keyset(storage, None).await?);
        } else {
            shallow.push(time_keyset(storage, None).await?);
            deep.push(time_keyset(storage, deep_from).await?);
        }
    }
    Ok((median(deep), median(shallow)))
}

/// The naive `LIMIT/OFFSET` page that ISSUE-06 replaced. It is not production
/// code any more; it exists here only as the baseline the keyset query must
/// beat. It selects the *same* `ROOM_EVENT_COLS` and the same qualified
/// `ORDER BY` as the production keyset query, so the two sides decode
/// identical rows in the same order and neither is accidentally penalised by
/// the alias-shadowing plan.
async fn fetch_offset_page(pool: &Arc<sqlx::PgPool>) -> Result<Vec<RoomEvent>, sqlx::Error> {
    sqlx::query_as(&format!(
        "SELECT {ROOM_EVENT_COLS} FROM events WHERE room_id = $1 \
         ORDER BY events.origin_server_ts DESC, events.stream_ordering DESC LIMIT $2 OFFSET $3"
    ))
    .bind(TARGET_ROOM)
    .bind(PAGE_LIMIT)
    .bind(deep_offset())
    .fetch_all(&**pool)
    .await
}

async fn sample_offset(pool: &Arc<sqlx::PgPool>) -> Result<Vec<f64>, sqlx::Error> {
    let mut warmup = 0;
    let mut samples = Vec::with_capacity(RECORDED_SAMPLES);
    while warmup < WARMUP_SAMPLES {
        black_box(fetch_offset_page(pool).await?);
        warmup += 1;
    }
    for _ in 0..RECORDED_SAMPLES {
        let started = Instant::now();
        black_box(fetch_offset_page(pool).await?);
        samples.push(started.elapsed().as_secs_f64() * 1_000_000.0);
    }
    Ok(samples)
}

/// `true` when the production keyset deep-page query is served by the ordered
/// composite index (`idx_events_room_ts_stream`) — i.e. the index supplies the
/// sort and no `Sort`/`Seq Scan` node remains.
///
/// The probe must EXPLAIN the *same shape* the benchmark measures: the same
/// `ROOM_EVENT_COLS` select list, the same row-value predicate and the same
/// qualified `ORDER BY events.origin_server_ts DESC, events.stream_ordering
/// DESC`. A narrow `SELECT event_id` proxy stays green under the
/// alias-shadowing bug (no `COALESCE` output column, so the bare `ORDER BY`
/// binds to the input column), which is how the gate previously passed while
/// production sorted every row above the cursor. Literals are fine here:
/// `EXPLAIN` is a utility statement and this probe is about the access path.
async fn keyset_plan_uses_index(pool: &Arc<sqlx::PgPool>) -> Result<bool, sqlx::Error> {
    let sql = format!(
        "EXPLAIN (FORMAT TEXT) SELECT {ROOM_EVENT_COLS} FROM events \
         WHERE room_id = '{TARGET_ROOM}' \
           AND (origin_server_ts, stream_ordering) < ({}, {}) \
         ORDER BY events.origin_server_ts DESC, events.stream_ordering DESC LIMIT {PAGE_LIMIT}",
        cursor_ts(),
        cursor_stream()
    );
    let plan: Vec<String> = sqlx::query_scalar(&sql).fetch_all(&**pool).await?;
    // Match the index by name so a forward or backward ordered scan both
    // count; `Bitmap Index Scan on ...` intentionally does not match.
    let ordered_index_scan = plan.iter().any(|line| line.contains("using idx_events_room_ts_stream"));
    let needs_sort = plan.iter().any(|line| line.contains("Sort"));
    Ok(ordered_index_scan && !needs_sort)
}

struct Measurement {
    rows: i64,
    keyset_deep_us: f64,
    keyset_shallow_us: f64,
    offset_deep_us: f64,
    index_scan: bool,
    correct: bool,
}

fn emit_perf_line(m: &Measurement) {
    let gain = if m.keyset_deep_us > 0.0 { m.offset_deep_us / m.keyset_deep_us } else { 0.0 };
    let shallow_over_deep = if m.keyset_deep_us > 0.0 { m.keyset_shallow_us / m.keyset_deep_us } else { 0.0 };
    eprintln!(
        "[perf] pagination rows={} target_room_events={TARGET_EVENTS} deep_offset={} page_limit={PAGE_LIMIT} \
         keyset_deep_us={:.1} keyset_shallow_us={:.1} shallow_over_deep_x={shallow_over_deep:.2} \
         offset_deep_us={:.1} gain_x={gain:.2} index_scan={} correct={}",
        m.rows,
        deep_offset(),
        m.keyset_deep_us,
        m.keyset_shallow_us,
        m.offset_deep_us,
        u8::from(m.index_scan),
        u8::from(m.correct),
    );
}

async fn measure(pool: &Arc<sqlx::PgPool>) -> Result<Measurement, sqlx::Error> {
    let storage = EventStorage::new(pool, "localhost".to_string());

    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE room_id = $1 OR room_id LIKE $2")
        .bind(TARGET_ROOM)
        .bind(format!("{OTHER_ROOM_PREFIX}%"))
        .fetch_one(&**pool)
        .await?;

    // Correctness first: if the keyset page and the offset page are not the
    // same page, the timing comparison is meaningless (and a "fast" keyset
    // query that returns the wrong rows must not pass the gate).
    let keyset_page = storage
        .get_room_events_paginated_cursor(TARGET_ROOM, Some((cursor_ts(), Some(cursor_stream()))), PAGE_LIMIT, "b")
        .await?;
    let keyset_ids: Vec<String> = keyset_page.iter().map(|row| row.event_id.clone()).collect();
    let offset_ids: Vec<String> = fetch_offset_page(pool).await?.into_iter().map(|row| row.event_id).collect();
    let correct = !keyset_ids.is_empty() && keyset_ids == offset_ids;

    let index_scan = keyset_plan_uses_index(pool).await?;

    // Deep and shallow pages are sampled interleaved: the shallowest page is the
    // first `/messages` call (`from = None`) and must stay within
    // `PAGINATION_MAX_SHALLOW_RATIO` (with an absolute breach floor) of the deep
    // page. With the ORDER BY alias-shadowing bug it was ~10-15x slower (18.2 ms vs
    // 1.2 ms); after the fix both are plain index scans (~1x). Interleaving keeps a
    // transient load spike from inflating the ratio on a healthy tree. See the
    // module docs.
    let (keyset_deep_us, keyset_shallow_us) =
        sample_keyset_pair(&storage, Some((cursor_ts(), Some(cursor_stream())))).await?;
    let offset_deep_us = median(sample_offset(pool).await?);

    Ok(Measurement { rows, keyset_deep_us, keyset_shallow_us, offset_deep_us, index_scan, correct })
}

fn benchmark_db_pagination(c: &mut Criterion) {
    let rt = Runtime::new().expect("pagination bench runtime must be constructible");
    let Some(pool) = connect_bench_pool(&rt) else {
        return;
    };
    if let Err(e) = rt.block_on(seed_fixture(&pool)) {
        eprintln!("[perf] cannot seed pagination fixture: {e}; skipping DB-backed pagination benches");
        return;
    }
    require_bench_group(PAGINATION_GROUP);

    match rt.block_on(measure(&pool)) {
        Ok(m) => emit_perf_line(&m),
        Err(e) => eprintln!("[perf] pagination measurement failed: {e}"),
    }

    let storage = EventStorage::new(&pool, "localhost".to_string());
    let deep_from = Some((cursor_ts(), Some(cursor_stream())));
    c.bench_function("pagination_keyset_deep_page_db", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(storage.get_room_events_paginated_cursor(TARGET_ROOM, deep_from, PAGE_LIMIT, "b").await)
            })
        });
    });
    c.bench_function("pagination_offset_deep_page_db", |b| {
        b.iter(|| rt.block_on(async { black_box(fetch_offset_page(&pool).await) }));
    });
}

criterion_group!(
    name = pagination_benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(10))
        .warm_up_time(Duration::from_secs(2));
    targets = benchmark_db_pagination
);

/// Gate mode: measure once, emit the `[perf]` line, skip criterion.
fn run_gate_mode() {
    let rt = match Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("[perf] pagination bench runtime failed: {e}");
            return;
        }
    };
    let Some(pool) = connect_bench_pool(&rt) else {
        return;
    };
    if let Err(e) = rt.block_on(seed_fixture(&pool)) {
        eprintln!("[perf] cannot seed pagination fixture: {e}");
        return;
    }
    require_bench_group(PAGINATION_GROUP);
    match rt.block_on(measure(&pool)) {
        Ok(m) => emit_perf_line(&m),
        Err(e) => eprintln!("[perf] pagination measurement failed: {e}"),
    }
}

/// Explicit `main` so the required-group check runs after everything had a
/// chance to register, and so the gate can skip criterion entirely.
fn main() {
    let gate_only = std::env::var("PAGINATION_GATE_ONLY").ok().as_deref() == Some("1");
    if gate_only {
        run_gate_mode();
    } else {
        pagination_benches();
    }
    enforce_required_groups();
}

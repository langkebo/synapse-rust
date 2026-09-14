//! Static guards for the unified test-isolation design (plan
//! `2026-09-13-unify-test-isolation`, Task 6).
//!
//! ## Failure modes these guards exist to catch
//!
//! Two crates grew independent schema-per-test fixtures, and both failed
//! *silently*:
//!
//! * `synapse-storage::test_isolation` replayed the whole v11 baseline
//!   statement-by-statement on **every** call (~4.4s per test, 99% of fixture
//!   cost, tail timeouts under parallel load).
//! * `synapse-services::test_utils::prepare_isolated_test_pool` created an
//!   *empty* schema and let the runtime `DatabaseInitService` fill it. That
//!   initializer does not create every baseline table (notably the retention
//!   tables), so unqualified queries fell back to the shared `public` schema
//!   through `search_path = <schema>, public`, producing order-dependent
//!   cross-test failures.
//!
//! The shared implementation lives in `synapse-common::test_isolation`
//! (fingerprint-named template + one-round-trip clone + inventory check), and
//! both fixtures delegate to it.
//!
//! A third, quieter failure mode: the template name is a **content
//! fingerprint** (`FNV-1a 64`) of the `baseline_sql` string the fixture passes
//! in. `v11 ++ extensions`, with no separator, hashes to `7c3a89659a56940f`
//! (2026-09-14: changed from `bec240fb79ed438b` when the burn_after_read retry
//! columns were folded into v11 — the fingerprint follows baseline content).
//! Inserting a separator (`"\n"`) or swapping the order mints a *second*
//! template, so the suite silently rebuilds the entire baseline once more and
//! stops sharing the template the database already has. Guard 5 therefore
//! re-evaluates each fixture's `concat!` and hashes the resulting string,
//! rather than pattern-matching its text.
//!
//! These tests are static: they read the fixture sources (and, through
//! `include_str!`, the two baseline migrations), so they need no database and
//! no `TEST_DATABASE_URL`.

use std::fs;

const STORAGE: &str = "synapse-storage/src/test_isolation.rs";
const SERVICES: &str = "synapse-services/src/test_utils.rs";
/// ROOT crate's test utils. Unlike storage/services, it does not call
/// `synapse_common::test_isolation::ensure_template_schema` because the CI
/// script `scripts/ci/prepare_test_db.sh` builds the template schema ahead
/// of time (`test_template_ci`). ROOT only delegates clone.
const ROOT: &str = "src/test_utils.rs";
const COMMON: &str = "synapse-common/src/test_isolation.rs";
const COMMON_LIB: &str = "synapse-common/src/lib.rs";

/// The two baseline migrations, compiled in. Guard 5 hashes these to pin the
/// template the database already holds.
const V11: &str = include_str!("../../migrations/00000000_unified_schema_v11.sql");
const EXTENSIONS: &str = include_str!("../../migrations/00000001_extensions_v10.sql");

/// `v11 ++ extensions`, no separator: the baseline the shared template was
/// built from. Any other string mints a SECOND template.
///
/// This is a **content** hash, so it changes whenever either baseline migration
/// is legitimately edited (e.g. dropping dead tables). When that happens, update
/// the constant to the newly reported `left:` value — the guard then goes back to
/// doing its real job, which is catching a wrong *concatenation*: a separator
/// hashes to `a05fa4488475fe1d` and a reversal to `4137af770181767b`, and neither
/// is what any legitimate migration edit produces as long as the two files are
/// still concatenated v11-then-extensions with nothing between them.
const EXPECTED_BASELINE_FINGERPRINT: &str = "8737d9a5f8413d51";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{path} must be readable: {error}"))
}

/// The non-`#[cfg(test)]` half of a fixture: the code that actually runs when a
/// test calls the fixture.
///
/// Guard 1 needs this because `synapse-storage`'s `#[cfg(test)]` module
/// legitimately unit-tests the shared `split_sql_statements` parser against
/// literal SQL. That is not a per-test baseline replay, so a whole-file
/// `contains` check would fail on correct code.
fn production_half(src: &str) -> &str {
    &src[..src.find("#[cfg(test)]").unwrap_or(src.len())]
}

/// Column-0 spellings that start (or end) a top-level item, used to bound the
/// slice returned by [`item_body`].
///
/// The keyword allowlist is a heuristic and can never be complete, so a
/// column-0 `}` is also a stop: rustfmt closes every top-level item's body with
/// `}` in column 0, while inner blocks are indented. That single rule keeps the
/// window honest even for an item form this list has not learned yet
/// (`macro_rules!`, `extern `, `unsafe `, `pub(crate) `, …).
const ITEM_HEADS: [&str; 20] = [
    "pub ",
    "pub(",
    "fn ",
    "async ",
    "impl ",
    "struct ",
    "enum ",
    "mod ",
    "use ",
    "const ",
    "static ",
    "type ",
    "trait ",
    "macro_rules!",
    "extern ",
    "unsafe ",
    "#[",
    "///",
    "//!",
    "}",
];

/// Slice `src` from the line containing `marker` up to (but not including) the
/// next top-level item or the column-0 `}` that closes the item.
///
/// Guard 4 must inspect **only** the body of `prepare_isolated_test_pool`:
/// `synapse-services/src/test_utils.rs` also owns the unrelated shared-pool
/// path (`prepare_shared_test_pool` -> `init_template_schema`), which
/// legitimately calls `DatabaseInitService`. A whole-file `contains` check
/// would therefore fail on correct code. Stopping at the column-0 `}` (or, for
/// an item that has none, the next column-0 keyword) keeps the window tight
/// without hand-maintained line numbers, and still includes the function's last
/// statement before its closing brace.
fn item_body<'a>(src: &'a str, marker: &str) -> &'a str {
    let start = src.find(marker).unwrap_or_else(|| panic!("source must contain `{marker}`"));
    // Walk line by line after the marker's own line; stop at the first
    // non-empty, non-indented line that starts a top-level item.
    let after_marker_line = src[start..].find('\n').map_or(src.len(), |i| start + i + 1);
    let mut end = src.len();
    let mut pos = after_marker_line;
    while pos < src.len() {
        let line_end = src[pos..].find('\n').map_or(src.len(), |i| pos + i + 1);
        let trimmed = src[pos..line_end].trim_end_matches(['\n', '\r']);
        let unindented = !trimmed.is_empty() && !trimmed.starts_with(' ') && !trimmed.starts_with('\t');
        if unindented && ITEM_HEADS.iter().any(|head| trimmed.starts_with(head)) {
            end = pos;
            break;
        }
        pos = line_end;
    }
    &src[start..end]
}

/// Remove `//` line comments and `/* */` block comments, leaving string
/// literals intact.
///
/// Guard 4 must test *code*, not prose: `prepare_isolated_test_pool` carries an
/// in-body note saying the `DatabaseInitService` block "is deliberately GONE",
/// and a naive `contains` would fail on that comment while a real re-added call
/// would be equally invisible under a comment-only check.
fn strip_rust_comments(src: &str) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(chars.len());
            continue;
        }
        if c == '"' {
            out.push(c);
            i += 1;
            while i < chars.len() {
                out.push(chars[i]);
                if chars[i] == '\\' && i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if chars[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Extract the contents of the first `concat!(...)` in `src` that joins
/// `include_str!` migrations.
fn baseline_concat_body(path: &str) -> String {
    let src = read(path);
    for (offset, _) in src.match_indices("concat!(") {
        let open = offset + "concat!".len();
        let body = balanced_parens(&src, open);
        if body.contains("include_str!") {
            return body.to_string();
        }
    }
    panic!("{path} must build its baseline with a `concat!` of `include_str!` migrations");
}

/// Return the text between the `(` at `open` (byte index) and its matching `)`.
///
/// Skips over string literals so a `)` inside a path can never close the block
/// early.
fn balanced_parens(src: &str, open: usize) -> &str {
    let bytes = src.as_bytes();
    assert_eq!(bytes[open], b'(', "balanced_parens must start on an opening parenthesis");
    let mut depth = 0usize;
    let mut i = open;
    while i < bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open + 1..i];
                }
            }
            b'"' => {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i += 2;
                        continue;
                    }
                    if bytes[i] == b'"' {
                        break;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    panic!("unbalanced parentheses in source");
}

/// FNV-1a 64 over `bytes` — the hash `synapse_common::test_isolation` uses to
/// name the baseline template.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// [`fnv1a64`] as the 16 lowercase hex chars that end the template schema name.
fn fingerprint_hex(sql: &str) -> String {
    format!("{:016x}", fnv1a64(sql.as_bytes()))
}

/// Read the Rust string literal starting at byte `open` (which must be `"`),
/// returning its decoded contents and the byte index just past the closing `"`.
///
/// Only escapes that can plausibly appear in a baseline `concat!` are decoded;
/// anything else panics rather than silently hashing the wrong string.
fn read_string_literal(path: &str, src: &str, open: usize) -> (String, usize) {
    let bytes = src.as_bytes();
    assert_eq!(bytes.get(open), Some(&b'"'), "{path}: expected a string literal at byte {open}");
    let mut decoded = String::new();
    let mut i = open + 1;
    let mut chunk_start = i;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                decoded.push_str(&src[chunk_start..i]);
                return (decoded, i + 1);
            }
            b'\\' => {
                decoded.push_str(&src[chunk_start..i]);
                i += 1;
                let escaped = *bytes.get(i).unwrap_or_else(|| panic!("{path}: unterminated string literal"));
                match escaped {
                    b'n' => decoded.push('\n'),
                    b'r' => decoded.push('\r'),
                    b't' => decoded.push('\t'),
                    b'0' => decoded.push('\0'),
                    b'\\' => decoded.push('\\'),
                    b'"' => decoded.push('"'),
                    b'\'' => decoded.push('\''),
                    other => panic!(
                        "{path}: unsupported escape `\\{}` in a baseline string literal; teach \
                         `read_string_literal` about it so the fingerprint stays honest",
                        char::from(other)
                    ),
                }
                i += 1;
                chunk_start = i;
            }
            _ => i += 1,
        }
    }
    panic!("{path}: unterminated string literal");
}

/// Evaluate a fixture's baseline `concat!` the way the compiler does:
/// `include_str!("…")` arguments become the contents of the file they name
/// (resolved relative to the fixture), and string literals contribute their
/// decoded text. Whitespace, commas and everything else is ignored, exactly as
/// `concat!` ignores it.
///
/// Reading the fixture's *expression* — rather than asserting its text shape —
/// is what makes guard 5 property-based: a leading, embedded or trailing
/// separator, a reversed order, or a pointer to a different migration all
/// change the resulting string and therefore its fingerprint. A rename that
/// keeps the content and the order intact does not, so a harmless rename
/// passes.
fn fixture_baseline_sql(path: &str) -> String {
    let body = baseline_concat_body(path);
    let directory = std::path::Path::new(path).parent().expect("fixture path must have a parent");
    let bytes = body.as_bytes();
    let mut sql = String::new();
    let mut i = 0;
    while i < bytes.len() {
        if body[i..].starts_with("include_str!(") {
            let open = i + "include_str!".len();
            let close = body[open..]
                .find(')')
                .map_or_else(|| panic!("{path}: unterminated include_str! invocation"), |offset| open + offset);
            let (argument, _) = read_string_literal(path, &body, open + 1);
            let resolved = directory.join(argument);
            sql.push_str(&read(resolved.to_str().expect("migration path must be UTF-8")));
            i = close + 1;
            continue;
        }
        if bytes[i] == b'"' {
            let (literal, next) = read_string_literal(path, &body, i);
            sql.push_str(&literal);
            i = next;
            continue;
        }
        i += 1;
    }
    sql
}

/// Guard 1: every fixture delegates the shared work to `synapse-common`.
///
/// If any fixture goes back to its own implementation — `synapse-storage`
/// replaying the baseline per test, `synapse-services` filling an empty schema
/// with `DatabaseInitService`, `src/test_utils.rs` resurrecting its ~170-line
/// hand-rolled clone — the other guards' invariants no longer describe the code
/// that actually runs, and the divergence starts again.
///
/// The positive `contains` checks alone are not enough: a fixture can keep
/// calling the shared module *and* re-add the historical per-test baseline
/// replay (`for stmt in …split_sql_statements(baseline_sql) { … }`), which is
/// the exact regression this plan removed. Both replay signals are therefore
/// asserted absent, on the production half of each fixture (see
/// [`production_half`]).
///
/// `ROOT` is the only fixture that does NOT delegate `ensure_template_schema`:
/// the CI script `scripts/ci/prepare_test_db.sh` seeds the pinned
/// `TEST_DB_TEMPLATE_SCHEMA` template directly, so ROOT's template is a
/// guarantee of the environment, not of this function. ROOT must still
/// delegate clone — a hand-rolled clone there is the third implementation this
/// guard exists to prevent.
#[test]
fn every_fixture_delegates_clone_to_the_shared_module() {
    for path in [ROOT, STORAGE, SERVICES] {
        let src = read(path);
        assert!(
            src.contains("synapse_common::test_isolation::clone_schema_from_template"),
            "{path} must clone its per-test schema from the shared template module"
        );
    }

    for path in [STORAGE, SERVICES] {
        let src = read(path);
        assert!(
            src.contains("synapse_common::test_isolation::ensure_template_schema"),
            "{path} must build/reuse its isolated template through the shared module"
        );
    }

    for path in [ROOT, STORAGE, SERVICES] {
        let src = read(path);
        let production = production_half(&src);
        assert!(
            !production.contains("split_sql_statements"),
            "{path} must not replay the baseline per test (it must delegate to the shared clone)"
        );
        assert!(
            !production.contains("for stmt in"),
            "{path} must not loop over baseline statements per test (it must delegate to the shared \
             clone); a fixture has no legitimate reason for that loop"
        );
    }
}

/// Guard 1b: no fixture carries a second clone implementation.
///
/// The shared template builder is the sole owner of
/// `CREATE TABLE %I.%I (LIKE %I.%I INCLUDING ALL)` — the single-round-trip
/// table copy. A fixture that re-introduces that literal has resurrected the
/// hand-rolled clone: the one this plan removed from `src/test_utils.rs` was a
/// 170-line DO block that combined that `LIKE ... INCLUDING ALL` with an index
/// rename loop, a seed-copy loop, a sequence re-bind loop and a view-recreation
/// loop, and it silently diverged from the shared implementation on foreign
/// keys, triggers and functions. Re-adding it here would be a regression of
/// the exact §2 finding.
///
/// `COMMON` is deliberately excluded: it *is* the implementation and must keep
/// the literal (Guard 2 asserts that). The exclusion list is a design choice,
/// not an oversight.
#[test]
fn no_fixture_resurrects_a_hand_rolled_clone() {
    for path in [ROOT, STORAGE, SERVICES] {
        let src = read(path);
        let production = production_half(&src);
        assert!(
            !production.contains("LIKE %I.%I INCLUDING ALL"),
            "{path} must not contain the shared clone's `LIKE ... INCLUDING ALL` DDL: a fixture \
             that emits it is a second clone implementation and diverges from the shared module"
        );
        assert!(
            !production.contains("CREATE SEQUENCE IF NOT EXISTS"),
            "{path} must not hand-roll sequence re-binding — the shared clone owns that"
        );
        assert!(
            !production.contains("pg_get_triggerdef"),
            "{path} must not hand-roll trigger cloning — the shared clone owns that"
        );
    }
}

/// Guard 2: the shared module exists, is exported, and keeps its mechanisms.
///
/// Deleting or renaming any of these symbols is how the unification gets
/// undone: an unexported module (or a dropped advisory lock / readiness
/// marker) reintroduces concurrent template builds, and a dropped
/// single-round-trip `LIKE ... INCLUDING ALL` reintroduces the per-statement
/// replay this plan removed.
#[test]
fn the_shared_module_exists_is_exported_and_keeps_its_mechanisms() {
    let lib = read(COMMON_LIB);
    assert!(lib.contains("pub mod test_isolation;"), "synapse-common must export the shared test-isolation module");

    let src = read(COMMON);
    for symbol in ["ensure_template_schema", "clone_schema_from_template", "pg_advisory_lock"] {
        assert!(src.contains(symbol), "{COMMON} must keep `{symbol}`");
    }
    assert!(
        src.contains("CREATE TABLE %I.%I (LIKE %I.%I INCLUDING ALL)"),
        "{COMMON} must keep the single-round-trip `LIKE ... INCLUDING ALL` clone"
    );
}

/// Guard 3: the per-test hot path is one clone, not a statement-by-statement
/// baseline replay.
///
/// `build_template` legitimately parses the baseline once with
/// `split_sql_statements` when it builds the shared template — that call is the
/// one-time cost the unification bought. The hazard is a *second* call site on
/// the per-test path: that is the 4.4s-per-test replay coming back, and because
/// it would sit next to the clone it is easy to add by accident.
///
/// This deliberately does not use the weak literal
/// `for stmt in split_sql_statements(baseline_sql)` check: the real one-time
/// call passes `&baseline_sql`, so that literal is vacuously absent and would
/// guard nothing. Instead: exactly one production call site, it must be inside
/// `build_template`, and the clone path must not mention the parser at all.
#[test]
fn the_clone_path_does_not_replay_the_baseline_per_test() {
    let src = read(COMMON);
    let production = production_half(&src);

    assert!(
        production.contains("CREATE TABLE %I.%I (LIKE %I.%I INCLUDING ALL)"),
        "the per-test hot path must be the single `LIKE ... INCLUDING ALL` clone"
    );

    let calls = production.matches("split_sql_statements(").count();
    let definitions = production.matches("pub fn split_sql_statements(").count();
    assert_eq!(definitions, 1, "{COMMON} must still define the parser exactly once");
    assert_eq!(
        calls - definitions,
        1,
        "{COMMON}: the production code must invoke `split_sql_statements` exactly once — the \
         one-time template build. A second call site is a per-test baseline replay (the ~4.4s/test \
         DDL storm this plan removed)."
    );

    // Positive control: the single call lives in the one-time template build,
    // so the count above is not satisfied by some unrelated call site. The
    // exact shape inside `build_template` is deliberately NOT pinned: a future
    // one-time `raw_sql(baseline)` optimisation would still be a single build
    // of the template, not a per-test replay, and must not fail this guard.
    let build = item_body(production, "async fn build_template");
    assert!(
        build.contains("split_sql_statements"),
        "the single `split_sql_statements` invocation must be inside the one-time `build_template`"
    );

    for marker in ["fn clone_statement", "pub async fn clone_schema_from_template"] {
        let body = item_body(production, marker);
        assert!(
            !body.contains("split_sql_statements"),
            "`{marker}` is on the per-test path and must clone the template, never replay the \
             baseline statement-by-statement"
        );
        assert!(!body.contains("for stmt in"), "`{marker}` must not loop over baseline statements per test");
    }
}

/// Guard 4: `prepare_isolated_test_pool` does not fill the schema with the
/// runtime initializer.
///
/// `DatabaseInitService` does not create every baseline table (notably the
/// retention tables), so an isolated schema built that way makes unqualified
/// queries resolve against the shared `public` schema through
/// `search_path = <schema>, public` — the order-dependent `media::tests` /
/// `*::db_tests` failures.
///
/// Scoped to the function body on purpose: the same file's unrelated shared-pool
/// path (`prepare_shared_test_pool` -> `init_template_schema`) still uses the
/// runtime initializer and must keep doing so. A whole-file `contains` check
/// would fail on correct code.
#[test]
fn prepare_isolated_test_pool_does_not_use_the_runtime_initializer() {
    let src = read(SERVICES);
    let body = item_body(&src, "pub async fn prepare_isolated_test_pool");

    // Non-vacuity controls: prove the window really is the whole function body
    // and not an empty or truncated slice. `ensure_test_schema_contract(...)`
    // runs mid-body, and `register_schema_cleanup(...)` / `Ok(pool)` are its
    // last two statements, immediately before the column-0 `}` that ends the
    // window. Requiring all three means the window reaches the true end of the
    // function, so a real call injected as the final statement cannot hide.
    assert!(
        body.contains("synapse_common::test_isolation::clone_schema_from_template"),
        "the extracted window must cover `prepare_isolated_test_pool`'s body"
    );
    assert!(
        body.contains("ensure_test_schema_contract")
            && body.contains("register_schema_cleanup")
            && body.contains("Ok(pool)"),
        "the extracted window must reach the end of `prepare_isolated_test_pool`: its final \
         statements are `ensure_test_schema_contract(...)`, then `register_schema_cleanup(...)`, \
         then `Ok(pool)`"
    );

    assert!(
        !strip_rust_comments(body).contains("DatabaseInitService"),
        "`prepare_isolated_test_pool` must not fill the isolated schema with the runtime \
         initializer: it does not create every baseline table, so queries silently fall back to \
         the shared `public` schema through search_path"
    );
}

/// Guard 5: the baseline string each fixture feeds the template builder must be
/// exactly `v11 ++ extensions` — that order, with nothing between or around
/// them.
///
/// The template schema name is `test_isolation_template_<FNV-1a 64 of the
/// baseline string>`. `v11 ++ extensions` (no separator) hashes to
/// `7c3a89659a56940f`; `v11 ++ "\n" ++ extensions` hashes to `a05fa4488475fe1d`
/// and the reverse to `4137af770181767b`. Either variant silently builds a
/// *second* full template, so the suite pays the whole baseline rebuild again
/// while believing it is sharing a template.
///
/// This is a property assertion, not a text heuristic. The repository-side
/// check pins the migrations' bytes and order via `include_str!`; the
/// fixture-side check re-evaluates each fixture's `concat!` (string literals
/// included) and hashes the result, so a leading or trailing separator — which
/// an adjacency check misses — is caught exactly like an embedded one. A
/// reversal, a third migration, or a pointer at a different file are all caught
/// the same way; a rename that preserves content and order is not.
#[test]
fn baseline_fingerprint_is_v11_then_extensions_with_no_separator() {
    assert_eq!(
        fingerprint_hex(&format!("{V11}{EXTENSIONS}")),
        EXPECTED_BASELINE_FINGERPRINT,
        "the baseline fingerprint changed: the two migrations must be concatenated as v11 then \
         extensions with no separator, or a SECOND template is minted (with a separator: \
         a05fa4488475fe1d; reversed: 4137af770181767b)"
    );

    for path in [STORAGE, SERVICES] {
        let baseline = fixture_baseline_sql(path);
        assert_eq!(
            fingerprint_hex(&baseline),
            EXPECTED_BASELINE_FINGERPRINT,
            "{path}: the baseline string passed to `ensure_template_schema` does not hash to the \
             expected template. It must be v11 ++ extensions in that order with nothing between or \
             around them — a separator hashes to a05fa4488475fe1d, a reversal to 4137af770181767b \
             — otherwise the suite silently builds and keeps a SECOND full template instead of \
             reusing the existing one."
        );
    }
}

/// Guard 6: the seed allowlist must stay equal to what the baseline actually seeds.
///
/// `SeedSource::Everything` (what every production call site passes) copies
/// whatever the template holds; `SeedSource::Only(SEED_REFERENCE_TABLES)` copies
/// a written-down list. Those two agree **only** because the baseline's `INSERT`
/// statements happen to target exactly the allowlisted tables. That is an
/// empirical fact about today's migrations, so it has to be re-checked whenever
/// the migrations change — nothing else in the build notices a new seed.
///
/// The failure this prevents: a migration adds `INSERT INTO some_new_table`, the
/// template is rebuilt under a new fingerprint, and from then on full clones
/// carry a row that allowlist clones lack. Every test in the allowlist lane then
/// diverges from the full lane with no compiler error and no red test — the exact
/// silent fork that made the previous multi-implementation fixtures disagree.
///
/// It parses the migrations through the same `include_str!` constants Guard 5
/// hashes, so an `INSERT` added to either file is seen even if it is added to a
/// file the fixture `concat!` does not mention yet.
#[test]
fn the_seed_allowlist_matches_what_the_baseline_seeds() {
    const SEED_REFERENCE_TABLES: &[&str] = synapse_common::test_isolation::SEED_REFERENCE_TABLES;

    let mut seeded = Vec::new();
    for (name, sql) in [("00000000_unified_schema_v11.sql", V11), ("00000001_extensions_v10.sql", EXTENSIONS)] {
        for statement in insert_statements(sql) {
            let rest = statement.split_once("INTO").map_or_else(
                || panic!("{name}: INSERT statement without INTO: {}", &statement[..80.min(statement.len())]),
                |(_, rest)| rest,
            );
            let table = rest
                .trim_start()
                .split(|c: char| c.is_whitespace() || c == '(')
                .next()
                .unwrap_or("")
                .trim_matches('"')
                .to_ascii_lowercase();
            assert!(
                !table.is_empty(),
                "{name}: could not parse the table out of: {}",
                &statement[..80.min(statement.len())]
            );
            seeded.push(table);
        }
    }
    seeded.sort();
    seeded.dedup();

    let declared: Vec<&str> = SEED_REFERENCE_TABLES.to_vec();
    let mut declared_sorted = declared.clone();
    declared_sorted.sort();
    declared_sorted.dedup();

    assert_eq!(
        seeded, declared_sorted,
        "the baseline's seeded tables and `synapse_common::test_isolation::SEED_REFERENCE_TABLES` \
         have diverged. A migration now seeds rows into a table the allowlist does not name, so \
         `SeedSource::Everything` clones and `SeedSource::Only(SEED_REFERENCE_TABLES)` clones no \
         longer start from the same data. Update the constant to match the migrations."
    );

    // `users` is the specific trap this guard exists for: the hardcoded
    // `@admin:localhost` seed was removed from v11 (DB-04), so re-adding a users
    // INSERT — or re-adding it to the allowlist out of old habit — must be a
    // deliberate, visible decision rather than a silent fixture fork.
    assert!(
        !declared_sorted.contains(&"users"),
        "`users` must not be in the seed allowlist: a freshly migrated database has no users row, \
         and most DB tests assert an empty users table"
    );
}

/// Every `INSERT INTO ...` statement in `sql`, comment lines and contents of
/// `'...'` string literals ignored.
///
/// Deliberately simple rather than a full SQL lexer: it only has to be exact for
/// the baseline files, and being simple means a reader can check it. Comment
/// stripping matters because v11 mentions a removed `INSERT` in prose:
/// "Hardcoded admin INSERT moved to scripts/create-default-admin.sql".
fn insert_statements(sql: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in sql.lines() {
        let line = raw.trim_start();
        if line.starts_with("--") || line.is_empty() {
            continue;
        }
        if !line.to_ascii_uppercase().starts_with("INSERT INTO") {
            continue;
        }
        // Drop single-quoted literals (VALUES rows can contain `--`, `;`, `INTO`).
        let mut cleaned = String::with_capacity(line.len());
        let mut in_string = false;
        for c in line.chars() {
            match (in_string, c) {
                (false, '\'') => in_string = true,
                (true, '\'') => in_string = false,
                (false, _) => cleaned.push(c),
                (true, _) => {}
            }
        }
        out.push(cleaned);
    }
    out
}

/// Guard 7: exactly one type in the repository clones a schema-by-schema copy.
///
/// `SeedSource::Only(SEED_REFERENCE_TABLES)` is exported from the shared module
/// precisely so the root fixture no longer needs a private clone. Before this
/// guard the root crate carried its own ~170-line clone (table `LIKE` + index
/// rename + seed copy + sequence copy + view recreation) which silently diverged
/// from the shared one — it copied no foreign keys, triggers or functions and
/// swallowed every per-index and per-view error. That is the failure mode
/// `AGENTS.md` rule 2 exists for, and nothing in the build noticed it.
///
/// Uniqueness is not enough on its own: asserting `>= 1` would leave a tree with
/// no clause at all unguarded. Asserting `== 1` also fails loudly if the shared
/// clause is ever reformatted (`seed_where_clause(seeds)`, more arguments, a
/// rename), which is why the assertion message spells out how to repair it.
#[test]
fn exactly_one_place_builds_the_schema_clone() {
    const GUARD_FILE: &str = "tests/unit/test_isolation_unification_tests.rs";
    // The interpolation marker is what makes the reader route through
    // `seed_where_clause` -> `SeedSource`; it is not a syntax coincidence.
    const MARKER: &str = "AND {seed_where}";

    let mut hits: Vec<String> = Vec::new();
    let mut visited = 0usize;
    let mut stack = vec![std::path::PathBuf::from(".")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                let skip = matches!(name.as_str(), "target" | ".git" | ".claude" | "vendor" | "node_modules");
                if !skip {
                    stack.push(path);
                }
                continue;
            }
            if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                continue;
            }
            let rel = path.to_string_lossy().trim_start_matches("./").to_string();
            if rel == GUARD_FILE {
                continue;
            }
            if !rel.starts_with("src") && !rel.ends_with("src/test_utils.rs") && !rel.contains("/src/") {
                continue;
            }
            visited += 1;
            let Ok(content) = fs::read_to_string(&path) else {
                continue;
            };
            if content.contains(MARKER) {
                hits.push(rel);
            }
        }
    }

    assert!(visited > 100, "the scan found only {visited} .rs files under src/ and workspace crates — the walk is broken, so this guard would pass vacuously");
    assert_eq!(
        hits,
        vec![COMMON.to_string()],
        "the schema-clone SQL must be built in exactly one place ({COMMON}, via `seed_where_clause`). \
         Found it in {hits:?}. A second builder means a second clone implementation that will drift — \
         route the new caller through `synapse_common::test_isolation::clone_schema_from_template` \
         instead. If the shared clause was renamed or reseeded, update MARKER in this guard."
    );
}

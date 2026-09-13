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
//! in. `v11 ++ extensions`, with no separator, hashes to `bec240fb79ed438b`.
//! Inserting a separator (`"\n"`) or swapping the order mints a *second*
//! template, so the suite silently rebuilds the entire baseline once more and
//! stops sharing the template the database already has.
//!
//! These tests are static: they read the fixture sources, so they need no
//! database and no `TEST_DATABASE_URL`.

use std::fs;

const STORAGE: &str = "synapse-storage/src/test_isolation.rs";
const SERVICES: &str = "synapse-services/src/test_utils.rs";
const COMMON: &str = "synapse-common/src/test_isolation.rs";
const COMMON_LIB: &str = "synapse-common/src/lib.rs";

fn read(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("{path} must be readable: {error}"))
}

/// Column-0 spellings that start a new top-level item (including its doc
/// comment / attributes), used to bound the slice returned by [`item_body`].
const ITEM_HEADS: [&str; 15] = [
    "pub ", "fn ", "async ", "impl ", "struct ", "enum ", "mod ", "use ", "const ", "static ", "type ", "trait ", "#[",
    "///", "//!",
];

/// Slice `src` from the line containing `marker` up to (but not including) the
/// next top-level item.
///
/// Guard 4 must inspect **only** the body of `prepare_isolated_test_pool`:
/// `synapse-services/src/test_utils.rs` also owns the unrelated shared-pool
/// path (`prepare_shared_test_pool` -> `init_template_schema`), which
/// legitimately calls `DatabaseInitService`. A whole-file `contains` check
/// would therefore fail on correct code. Bounding at the next column-0 item
/// (here: the doc comment of `prepare_shared_test_pool`) keeps the window
/// tight without hand-maintained line numbers.
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

/// The two `include_str!("...")` arguments of the baseline `concat!`, if they
/// are adjacent: nothing but whitespace and commas may sit between them.
///
/// This is the strongest form that stays readable. `concat!` only accepts
/// literals, so *any* injected separator must appear as a token between the two
/// invocations; rejecting every non-whitespace/non-comma token rejects `"\n"`,
/// `'\n'`, `1`, `concat!(...)` and friends alike. Also pins the order
/// (`v11` then `extensions`), because a reversal is the third way to fork the
/// fingerprint-named template.
fn assert_adjacent_migrations(path: &str, concat_body: &str) {
    let mut includes: Vec<(usize, usize, &str)> = Vec::new();
    let mut from = 0;
    while let Some(relative) = concat_body[from..].find("include_str!(") {
        let start = from + relative;
        let open = start + "include_str!".len();
        let close = concat_body[open..]
            .find(')')
            .map_or_else(|| panic!("{path}: unterminated include_str! invocation"), |i| open + i);
        includes.push((start, close + 1, concat_body[open + 1..close].trim()));
        from = close + 1;
    }

    assert_eq!(
        includes.len(),
        2,
        "{path}: the baseline `concat!` must join exactly the two baseline migrations \
         (v11 + extensions); found {} `include_str!` arguments",
        includes.len()
    );

    let between = &concat_body[includes[0].1..includes[1].0];
    let residue: String = between.chars().filter(|c| !c.is_whitespace() && *c != ',').collect();
    assert!(
        residue.is_empty(),
        "{path}: the two baseline `include_str!` migrations must be directly adjacent. Found the \
         unexpected token `{residue}` between them. Any separator (for example `\"\\n\"`) changes \
         the baseline string, which changes the FNV-1a fingerprint, which silently mints a SECOND \
         template schema instead of reusing the one the database already has."
    );

    assert!(
        includes[0].2.contains("00000000_unified_schema_v11.sql"),
        "{path}: the baseline must be built as `v11` FIRST; reversing the order also forges a new \
         fingerprint. Found `{}`",
        includes[0].2
    );
    assert!(
        includes[1].2.contains("00000001_extensions_v10.sql"),
        "{path}: the baseline must append `extensions` SECOND. Found `{}`",
        includes[1].2
    );
}

/// Guard 1: both fixtures delegate the shared work to `synapse-common`.
///
/// If either fixture goes back to its own implementation — `synapse-storage`
/// replaying the baseline per test, `synapse-services` filling an empty schema
/// with `DatabaseInitService` — the other guards' invariants no longer describe
/// the code that actually runs, and the divergence starts again.
#[test]
fn both_fixtures_delegate_to_the_shared_module() {
    for path in [STORAGE, SERVICES] {
        let src = read(path);
        assert!(
            src.contains("synapse_common::test_isolation::ensure_template_schema"),
            "{path} must build/reuse its isolated template through the shared module"
        );
        assert!(
            src.contains("synapse_common::test_isolation::clone_schema_from_template"),
            "{path} must clone its per-test schema from the shared template module"
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
    let production = &src[..src.find("#[cfg(test)]").unwrap_or(src.len())];

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
    // so the count above is not satisfied by some unrelated call site.
    let build = item_body(production, "async fn build_template");
    assert!(
        build.contains("for stmt in split_sql_statements"),
        "the one-time `build_template` must keep replaying the baseline statement-by-statement"
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

    // Non-vacuity controls: prove the window really is the function body and
    // not an empty or truncated slice.
    assert!(
        body.contains("synapse_common::test_isolation::clone_schema_from_template"),
        "the extracted window must cover `prepare_isolated_test_pool`'s body"
    );
    assert!(
        body.contains("ensure_test_schema_contract"),
        "the extracted window must run to the end of `prepare_isolated_test_pool`"
    );

    assert!(
        !strip_rust_comments(body).contains("DatabaseInitService"),
        "`prepare_isolated_test_pool` must not fill the isolated schema with the runtime \
         initializer: it does not create every baseline table, so queries silently fall back to \
         the shared `public` schema through search_path"
    );
}

/// Guard 5: neither fixture may inject a separator (or swap the order) between
/// the two baseline `include_str!` migrations.
///
/// The template schema name is `test_isolation_template_<FNV-1a 64 of the
/// baseline string>`. `v11 ++ extensions` (no separator) hashes to
/// `bec240fb79ed438b`; `v11 ++ "\n" ++ extensions` hashes to `a05fa4488475fe1d`
/// and the reverse to `4137af770181767b`. Either variant silently builds a
/// *second* full template, so the suite pays the whole baseline rebuild again
/// while believing it is sharing a template.
///
/// Residual limit: this reads the fixture source text rather than hashing the
/// migrations, so a separator introduced *outside* the `concat!` — for example
/// wrapping the result in `format!("{}\\n{}", ...)` in the fixture — would slip
/// past. The compile-time `concat!` is the only place a literal can be added
/// today; the migration files themselves are not guarded here.
#[test]
fn baseline_concat_keeps_the_migrations_adjacent() {
    for path in [STORAGE, SERVICES] {
        let concat_body = baseline_concat_body(path);
        assert_adjacent_migrations(path, &concat_body);
    }
}

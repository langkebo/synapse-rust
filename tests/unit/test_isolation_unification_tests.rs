//! Static guards for the unified test-isolation design (plan
//! `2026-09-13-unify-test-isolation`, Task 6).
//!
//! ## Failure modes these guards exist to catch
//!
//! Two crates grew independent schema-per-test fixtures, and both failed
//! *silently*:
//!
//! * `synapse-storage::test_isolation` replayed the whole v12 baseline
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
/// The shared `synapse-test-utils` crate (formerly the ROOT crate's
/// `src/test_utils.rs`). Unlike storage/services, it does not call
/// `synapse_common::test_isolation::ensure_template_schema` because the CI
/// script `scripts/ci/prepare_test_db.sh` builds the template schema ahead
/// of time (`test_template_ci`). It only delegates clone.
const ROOT: &str = "synapse-test-utils/src/lib.rs";
const COMMON: &str = "synapse-common/src/test_isolation.rs";
const COMMON_LIB: &str = "synapse-common/src/lib.rs";
/// `synapse-e2ee`'s DB-backed verification tests. They use the shared
/// `IsolatedTestPool` and therefore must feed it the same baseline bytes as
/// every other caller, or they mint a second template.
const E2EE: &str = "synapse-e2ee/src/verification/service.rs";

/// The baseline migration, compiled in. Guard 5 hashes it to pin the template
/// the database already holds.
///
/// The former `00000001_extensions_v10.sql` was a byte-for-byte no-op duplicate
/// of objects this baseline already defines (14 tables + 1 index, all
/// `IF NOT EXISTS`) and was deleted, so the baseline is the single source.
const V12: &str = include_str!("../../migrations/00000000_unified_schema_v12.sql");

/// The v12 baseline content the shared template was built from. Any other string
/// mints a SECOND template.
///
/// This is a **content** hash, so it changes whenever the baseline is
/// legitimately edited (e.g. dropping dead tables). When that happens, update
/// the constant to the newly reported `left:` value — the guard then goes back to
/// doing its real job: pinning each fixture's input to the **single** v12
/// baseline, byte for byte. `00000001_extensions_v10.sql` no longer exists (it
/// was a no-op duplicate of objects v12 already defines and was deleted), so
/// there is no concatenation left to preserve; a leading or trailing separator,
/// an extra `include_str!`, or a pointer to a different migration all change the
/// hash and would silently build a *second* template. The old two-file values
/// (`a05fa4488475fe1d` for `v11 ++ "\n" ++ extensions`, `4137af770181767b` for the
/// reversal) are kept only as history from before the single-baseline
/// consolidation.
// 2026-09-18：`e2ee_audit_log.device_id` 改为可空（用户级审计事件没有单一设备），
// 基线内容变化 ⇒ 按上面这段说明把常量更新为新报告的 `left:` 值。
// 2026-09-18（第二次）：`ck_room_memberships_valid` 加入 'forget' 并把 15 处
// 约束守卫改为 schema 级判断，基线内容再次变化 ⇒ 按本守卫说明更新为最新 `left:` 值。
// 2026-09-18（第三次）：删除 `moderation_actions` / `moderation_rules` /
// `moderation_logs` 三张表及其索引（moderation 域整体下线），基线内容变化 ⇒
// 更新为最新 `left:` 值。旧值 42002cba863738f8 对应的陈旧模板
// `test_isolation_template_42002cba863738f8` 会在下次模板重建时被清理。
// 2026-09-19（`d77d1fcf`，Task 7 收口）：更正基线内 to_device 索引注释（不再引用
// 从未存在的 idx_to_device_recipient / idx_to_device_stream）—— 只动注释也会改变
// 内容哈希，而该提交没有同步本常量，守卫在 `212dad12` 上因此是红的。
// Task 7 按本守卫说明把常量更新为新报告的 `left:` 值 `45483ffa0a28b5d5`；旧值
// `4b492b815f02197b` 对应的 `test_isolation_template_4b492b815f02197b` 会在下次
// 模板重建时被清理。
// 2026-09-19（`98a90a58`，E2EE 优化批次）：给 `megolm_key_shares` 加
// `recipient_user_id`、主键改三元组并新增 `idx_megolm_key_shares_recipient` ——
// 基线内容变化，但该提交同样没同步本常量，于是 `main` 上的 `--test unit` 是红的
// （复现：left=b6a8b06fb13d22f9 / right=45483ffa0a28b5d5）。
// 这里按本守卫自身的说明更新为新报告的 `left:` 值 `b6a8b06fb13d22f9`。
// 纪律（已经踩过三次：`d77d1fcf`、本次、以及 Task 8 期间的工作树状态）：
// **改 `migrations/` 后必须跑一次本守卫**，哪怕只改注释 —— 模板指纹按文件字节哈希，
// 内容一变常量就必须同步，否则每个新库都会铸出第二份模板。
const EXPECTED_BASELINE_FINGERPRINT: &str = "b6a8b06fb13d22f9";

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
///
/// The marker must be a **real attribute** (the only non-whitespace text on its
/// line), not a mention inside a doc comment — `synapse-storage/src/test_isolation.rs`
/// documents its `#[cfg(test)]` reachability in its module docs, and a naive
/// `find` would truncate the production half to the first 7 lines, silently
/// disabling every negative assertion this guard makes on that file. That is
/// the exact failure mode `AGENTS.md` rule 8 describes for guards that stop
/// measuring what they claim to measure.
fn production_half(src: &str) -> &str {
    for (i, _) in src.match_indices("#[cfg(test)]") {
        let line_start = src[..i].rfind('\n').map_or(0, |newline| newline + 1);
        if src[line_start..i].trim().is_empty() {
            return &src[..i];
        }
    }
    src
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

/// Returns the inner expression text so that `fixture_baseline_sql`
/// can parse `include_str!` paths from it.
fn baseline_concat_body(path: &str) -> String {
    let src = read(path);

    // Try to find a `concat!(include_str!(...))` wrapper first.
    for (offset, _) in src.match_indices("concat!(") {
        let open = offset + "concat!".len();
        let body = balanced_parens(&src, open);
        if body.contains("include_str!") {
            return body.to_string();
        }
    }

    // Fall back to bare `include_str!(...)` — fixtures may use direct
    // include_str! without a concat! wrapper. Only the first occurrence is
    // considered, matching the `concat!` search above.
    if let Some((offset, _)) = src.match_indices("include_str!(").next() {
        let open = offset + "include_str!".len();
        let close = src[open..]
            .find(')')
            .map_or_else(|| panic!("{path}: unterminated include_str! invocation"), |offset| open + offset);
        // Return the raw `include_str!(...)` text so fixture_baseline_sql
        // can parse the path argument and resolve it against the directory.
        return src[offset..close + 1].to_string();
    }

    panic!("{path} must build its baseline with a `concat!` or `include_str!` of migrations");
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
///
/// `STORAGE` no longer names either primitive: the per-test pool lifecycle
/// (`IsolatedTestPool`) moved into `COMMON`, because a `#[cfg(test)]` module in
/// a dependency crate is invisible to sibling crates' fixtures — which is how
/// `synapse-e2ee` ended up hand-rolling a pool against `public`. STORAGE is now
/// a thin adapter and is asserted to go through the shared pool instead.
#[test]
fn every_fixture_delegates_clone_to_the_shared_module() {
    // The shared module is the one implementation: it owns both the template
    // builder and the clone. Asserting them on COMMON keeps the guard honest
    // after the pool moved here.
    let common = read(COMMON);
    for symbol in ["pub async fn ensure_template_schema", "pub async fn clone_schema_from_template"] {
        assert!(common.contains(symbol), "{COMMON} must own `{symbol}` — it is the single implementation");
    }

    // storage now reaches the primitives through the shared pool wrapper.
    let storage = read(STORAGE);
    assert!(
        storage.contains("IsolatedTestPool"),
        "{STORAGE} must build its per-test pool through the shared `IsolatedTestPool` (the shared \
         module is the single implementation of the pool lifecycle)"
    );

    // services drives the shared primitives directly.
    let services = read(SERVICES);
    for symbol in [
        "synapse_common::test_isolation::clone_schema_from_template",
        "synapse_common::test_isolation::ensure_template_schema",
    ] {
        assert!(services.contains(symbol), "{SERVICES} must delegate to `{symbol}`");
    }

    // ROOT cannot delegate template building (the CI script pre-builds the
    // pinned template) but must delegate the clone.
    let root = read(ROOT);
    assert!(
        root.contains("synapse_common::test_isolation::clone_schema_from_template"),
        "{ROOT} must clone its per-test schema from the shared template module"
    );

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
/// exactly the **single** v12 baseline file — that file's bytes, with nothing
/// added around them or between them.
///
/// The template schema name is `test_isolation_template_<FNV-1a 64 of the
/// baseline string>`. The v12-only input hashes to the constant below
/// (`45483ffa0a28b5d5`). Extra text — a leading or trailing separator, an
/// additional `include_str!`, a pointer at a different migration — changes the
/// hash and silently builds a *second* full template, so the suite pays the whole
/// baseline rebuild again while believing it is sharing a template. Historical
/// two-file values, kept only as history: `7c3a89659a56940f`
/// (`v11 ++ extensions`), `a05fa4488475fe1d` (with a `"\n"` separator) and
/// `4137af770181767b` (reversed).
///
/// This is a property assertion, not a text heuristic. The repository-side
/// check pins the migration's bytes via `include_str!`; the
/// fixture-side check re-evaluates each fixture's `concat!` (string literals
/// included) and hashes the result, so a leading or trailing separator — which
/// an adjacency check misses — is caught exactly like an embedded one. Extra
/// bytes, a third migration, or a pointer at a different file are all caught
/// the same way; a rename that preserves content is not.
#[test]
fn baseline_fingerprint_is_the_single_v12_source() {
    assert_eq!(
        fingerprint_hex(V12),
        EXPECTED_BASELINE_FINGERPRINT,
        "the baseline fingerprint changed: the fixture must hash the v12 baseline byte-for-byte, \
         or a SECOND template is minted"
    );

    for path in [STORAGE, SERVICES, E2EE] {
        let baseline = fixture_baseline_sql(path);
        assert_eq!(
            fingerprint_hex(&baseline),
            EXPECTED_BASELINE_FINGERPRINT,
            "{path}: the baseline string passed to `ensure_template_schema` does not hash to the \
             expected template. It must be the v12 baseline with nothing added or \
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
    for (name, sql) in [("00000000_unified_schema_v12.sql", V12)] {
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

/// Backticked `snake_case` identifiers in `doc` that have the shape of a Rust
/// test name pinned by the P1-D design document.
///
/// The document also backticks helpers, constants, catalog objects and table
/// names, and gives no machine-readable marker for "this one is a test", so
/// shape is the only available signal. Every test name the document pins is a
/// full descriptive sentence of five or more underscore-separated words, while
/// the non-test identifiers top out at four (`clone_schema_from_template`,
/// `pg_get_serial_sequence`, `truncate_and_reseed_schema`,
/// `test_isolation_template_<hex>`, `sqlx_ratio_gate_tests`). An identifier with
/// digits or a `pg_` prefix is a catalog/fingerprint name, never a test.
///
/// The rule is deliberately conservative: it can miss a hypothetical four-word
/// test name pinned in prose, but it never false-positives on the document's
/// helpers — a guard that goes red on a correct document gets deleted.
fn pinned_test_names(doc: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = doc;
    while let Some(open) = rest.find('`') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('`') else { break };
        let candidate = &after[..close];
        rest = &after[close + 1..];
        let test_shaped = candidate.split('_').count() >= 5
            && !candidate.starts_with("pg_")
            && candidate.chars().all(|c| c.is_ascii_lowercase() || c == '_');
        if test_shaped {
            names.push(candidate.to_string());
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Whether `sources` declares `fn name` at a real word boundary.
///
/// A bare `contains("fn name")` would also match a longer function whose name
/// merely starts with `name` (`fn foo_bar` for a pinned `foo`), which would let
/// a renamed test pass the guard.
fn declares_fn(sources: &str, name: &str) -> bool {
    let needle = format!("fn {name}");
    let mut from = 0;
    while let Some(found) = sources[from..].find(&needle) {
        let end = from + found + needle.len();
        let boundary = sources[end..].chars().next().is_none_or(|c| !(c.is_ascii_alphanumeric() || c == '_'));
        if boundary {
            return true;
        }
        from = end;
    }
    false
}

/// Concatenated source of `tests/` and every workspace crate's `src/`, plus the
/// number of files read (the non-vacuity control for guard 8).
fn pinned_test_sources() -> (String, usize) {
    let mut roots = vec![std::path::PathBuf::from("tests")];
    if let Ok(entries) = fs::read_dir(".") {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let src = entry.path().join("src");
            if name.starts_with("synapse-") && src.is_dir() {
                roots.push(src);
            }
        }
    }
    let mut sources = String::new();
    let mut visited = 0usize;
    for root in roots {
        collect_rs_sources(&root, &mut sources, &mut visited);
    }
    (sources, visited)
}

/// Append every `.rs` file under `dir` (recursively) to `out`, counting reads.
fn collect_rs_sources(dir: &std::path::Path, out: &mut String, visited: &mut usize) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_sources(&path, out, visited);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            *visited += 1;
            if let Ok(content) = fs::read_to_string(&path) {
                out.push_str(&content);
                out.push('\n');
            }
        }
    }
}

/// Guard 8: every test name the P1-D design document pins must exist in Rust.
///
/// `docs/audit/P1D_seed_allowlist_design_2026-09-14.md` is the written contract
/// for the seed allowlist. It used to promise `seed_reference_tables_match_baseline`
/// (the real guard is `the_seed_allowlist_matches_what_the_baseline_seeds`) and
/// `allowlist_clone_matches_full_clone_row_for_row` (folded into
/// `allowlist_clone_copies_only_the_allowlisted_rows`). Neither promised name was
/// ever declared in Rust, so the next reader who grepped the document found
/// nothing and could not tell whether the contract had been dropped (sweep D2).
///
/// This guard extracts the backticked test-shaped names and asserts each is
/// declared somewhere under `tests/` or `synapse-*/src/`. It is read-only and
/// needs no database.
#[test]
fn every_test_name_the_design_document_pins_exists() {
    const DESIGN_DOC: &str = "docs/audit/P1D_seed_allowlist_design_2026-09-14.md";

    let pinned = pinned_test_names(&read(DESIGN_DOC));

    assert!(
        pinned.len() >= 2,
        "{DESIGN_DOC} must pin at least two test names, otherwise this guard checks nothing; parsed \
         {pinned:?}"
    );
    for anchor in
        ["allowlist_clone_copies_only_the_allowlisted_rows", "the_seed_allowlist_matches_what_the_baseline_seeds"]
    {
        assert!(
            pinned.iter().any(|name| name.as_str() == anchor),
            "{DESIGN_DOC} must pin `{anchor}` — the landed guard for this responsibility. Parsed \
             {pinned:?}. If the test was renamed, update the design document with it."
        );
    }

    let (sources, visited) = pinned_test_sources();
    assert!(
        visited > 100,
        "the source scan only visited {visited} .rs files under tests/ and synapse-*/src/; the walk \
         is broken, so this guard would pass vacuously"
    );

    let missing: Vec<&String> = pinned.iter().filter(|name| !declares_fn(&sources, name)).collect();
    assert!(
        missing.is_empty(),
        "{DESIGN_DOC} pins test names that no Rust source declares: {missing:?}. A design document \
         that points at a test which does not exist is worse than no pin at all (sweep D2): the next \
         reader greps for it, finds nothing, and cannot tell whether the contract was dropped. Name \
         the test that actually exists, or record in the document why the responsibility was folded \
         into another test."
    );
}

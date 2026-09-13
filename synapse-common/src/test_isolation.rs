//! Shared test-isolation fixture: template schema + single-round-trip clone.
//!
//! ## Why this module exists
//!
//! Two crates grew separate schema-per-test fixtures:
//!
//! * `synapse-storage::test_isolation` replayed the whole v11 baseline
//!   statement-by-statement **on every call** (measured: `baseline_replay`
//!   median 4.416s, 99% of fixture cost), then was changed to clone a template.
//! * `synapse-services::test_utils::prepare_isolated_test_pool` created an
//!   *empty* schema and let the runtime `DatabaseInitService` fill it. That
//!   initializer does not create every baseline table (e.g. the retention
//!   tables), so queries silently fell back to the shared `public` schema via
//!   `search_path = <schema>, public` and tests leaked state into each other.
//!
//! One implementation, two callers. The baseline SQL is passed in by the
//! caller because the migration files live at the workspace root and are not
//! reachable from this crate via `include_str!` relative paths.

/// Marker table written into the template only after a complete build.
pub const TEMPLATE_READY_TABLE: &str = "_synapse_test_template_ready";

/// FNV-1a 64-bit fingerprint of the baseline SQL, as 16 hex chars.
pub fn baseline_fingerprint(baseline_sql: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in baseline_sql.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

/// Name of the shared template schema for the given baseline content.
pub fn template_schema_name(baseline_sql: &str) -> String {
    format!("test_isolation_template_{}", baseline_fingerprint(baseline_sql))
}

/// First non-empty line of a statement, for error context.
pub fn first_line(s: &str) -> &str {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("")
}

/// Removes `COPY ... FROM stdin;` ... `\.` blocks (seed data).
pub fn strip_copy_blocks(sql: &str) -> String {
    let mut out = String::new();
    let mut in_copy = false;
    for line in sql.split_inclusive('\n') {
        let t = line.trim_start();
        if !in_copy && t.starts_with("COPY") && t.contains("FROM stdin") {
            in_copy = true;
            continue;
        }
        if in_copy {
            if t.starts_with("\\.") {
                in_copy = false;
            }
            continue;
        }
        out.push_str(line);
    }
    out
}

/// Splits SQL text into individual statements, correctly handling:
/// - `--` line comments and `/* */` block comments (skipped, not emitted)
/// - `'...'` string literals (with `''` escapes)
/// - `"..."` quoted identifiers (with `""` escapes)
/// - `$$...$$` / `$tag$...$tag$` dollar-quoted bodies (functions, DO blocks)
/// - `;` statement terminators outside all of the above
///
/// Unlike a naive `split(';')`, a chunk that begins with a comment line is not
/// dropped together with the statements that follow it.
pub fn split_sql_statements(sql: &str) -> Vec<String> {
    let chars: Vec<char> = sql.chars().collect();
    let n = chars.len();
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut i = 0;

    while i < n {
        let c = chars[i];

        // `--` line comment: skip to end of line.
        if c == '-' && i + 1 < n && chars[i + 1] == '-' {
            while i < n && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        // `/* ... */` block comment.
        if c == '/' && i + 1 < n && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < n && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i = (i + 2).min(n);
            continue;
        }

        // `'...'` string literal with `''` escapes.
        if c == '\'' {
            current.push(c);
            i += 1;
            while i < n {
                if chars[i] == '\'' {
                    if i + 1 < n && chars[i + 1] == '\'' {
                        current.push('\'');
                        current.push('\'');
                        i += 2;
                        continue;
                    }
                    current.push('\'');
                    i += 1;
                    break;
                }
                current.push(chars[i]);
                i += 1;
            }
            continue;
        }

        // `"..."` quoted identifier with `""` escapes.
        if c == '"' {
            current.push(c);
            i += 1;
            while i < n {
                if chars[i] == '"' {
                    if i + 1 < n && chars[i + 1] == '"' {
                        current.push('"');
                        current.push('"');
                        i += 2;
                        continue;
                    }
                    current.push('"');
                    i += 1;
                    break;
                }
                current.push(chars[i]);
                i += 1;
            }
            continue;
        }

        // `$tag$ ... $tag$` dollar-quoted body.
        if c == '$' {
            let mut j = i + 1;
            while j < n && chars[j] != '$' {
                j += 1;
            }
            if j < n {
                let tag: String = chars[i..=j].iter().collect();
                let tag_len = tag.len();
                current.push_str(&tag);
                let body_start = j + 1;
                i = body_start;
                let mut found = false;
                while i + tag_len <= n {
                    if chars[i..i + tag_len].iter().collect::<String>() == tag {
                        // Push the body AND the closing tag, not just the tag.
                        let seg: String = chars[body_start..i + tag_len].iter().collect();
                        current.push_str(&seg);
                        i += tag_len;
                        found = true;
                        break;
                    }
                    i += 1;
                }
                if !found {
                    let seg: String = chars[body_start..].iter().collect();
                    current.push_str(&seg);
                    i = n;
                }
                continue;
            }
            // Lone `$` — keep as ordinary character.
            current.push(c);
            i += 1;
            continue;
        }

        // Statement terminator outside strings/comments/dollar bodies.
        if c == ';' {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                statements.push(current);
            }
            current = String::new();
            i += 1;
            continue;
        }

        current.push(c);
        i += 1;
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(current);
    }

    statements
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_stable_and_content_sensitive() {
        let a = baseline_fingerprint("CREATE TABLE users (id text);");
        let b = baseline_fingerprint("CREATE TABLE users (id text);");
        let c = baseline_fingerprint("CREATE TABLE users (id bigint);");
        assert_eq!(a, b, "same content must yield same fingerprint");
        assert_ne!(a, c, "changed content must yield a different fingerprint");
        assert_eq!(a.len(), 16, "fingerprint must be 16 hex chars");
    }

    #[test]
    fn template_name_embeds_the_fingerprint() {
        let sql = "CREATE TABLE users (id text);";
        let name = template_schema_name(sql);
        assert_eq!(name, format!("test_isolation_template_{}", baseline_fingerprint(sql)));
        assert!(name.starts_with("test_isolation_template_"));
    }

    #[test]
    fn split_handles_functions_do_blocks_and_comments() {
        let sql = r#"
CREATE TABLE IF NOT EXISTS users (
    user_id TEXT NOT NULL
);

CREATE OR REPLACE FUNCTION f()
RETURNS TRIGGER AS $$
BEGIN
    RETURN NEW; -- inner semicolon
END;
$$ LANGUAGE plpgsql;
"#;
        let stmts = split_sql_statements(sql);
        let heads: Vec<&str> = stmts.iter().map(|s| first_line(s)).collect();
        assert_eq!(heads, vec!["CREATE TABLE IF NOT EXISTS users (", "CREATE OR REPLACE FUNCTION f()"]);
    }

    #[test]
    fn strip_copy_blocks_removes_seed_data() {
        let sql = "CREATE TABLE t (id int);\nCOPY t (id) FROM stdin;\n1\n2\n\\.\nCREATE INDEX i ON t (id);\n";
        let out = strip_copy_blocks(sql);
        assert!(!out.contains("FROM stdin"));
        assert!(!out.contains("\n1\n"));
        assert!(out.contains("CREATE INDEX i ON t (id);"));
    }
}

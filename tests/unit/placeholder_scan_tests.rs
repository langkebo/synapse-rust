#![allow(clippy::unwrap_used, clippy::expect_used)]
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Collect `*.rs` files under `dir`, **failing loudly if `dir` is not a
/// directory**.
///
/// Regression context (measured 2026-09-19): this used to be
/// `if let Ok(entries) = fs::read_dir(dir) { … }`, so a scan root that no longer
/// existed produced an *empty* file list and both guards below passed on an
/// empty set. That is exactly what happened: the roots were
/// `src/web/routes[/handlers]`, which stopped existing once the HTTP surface
/// moved to the `synapse-web` crate (commit `4ca7a635`), so this whole file was
/// dead for an unknown number of releases while reporting green. A guard whose
/// input set can silently become empty is not a guard.
fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| {
        panic!(
            "placeholder_scan: scan root {} is not a readable directory ({error}). \
             If the code moved, fix the path here — do NOT let the scan silently examine nothing.",
            dir.display()
        )
    });
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Assert the scan actually saw a plausible amount of code.
///
/// A path typo that still resolves (e.g. an empty directory) must not turn the
/// guards into no-ops; the crate has dozens of route files.
fn assert_scan_is_non_trivial(dir: &Path, files: &[PathBuf]) {
    assert!(
        files.len() >= 20,
        "placeholder_scan: only {} .rs file(s) found under {} — the scan is not looking at the \
         route sources it is supposed to police (expected dozens)",
        files.len(),
        dir.display()
    );
}

fn load_allowlist_entries(file: &Path) -> Vec<String> {
    let content = fs::read_to_string(file).expect("allowlist should be readable");

    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(ToOwned::to_owned)
        .collect()
}

#[test]
fn test_no_placeholder_auth_user_ignores_in_handlers() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let handlers_dir = root.join("synapse-web").join("src").join("routes").join("handlers");
    let mut files = Vec::new();
    collect_rs_files(&handlers_dir, &mut files);
    files.sort();
    assert_scan_is_non_trivial(&handlers_dir, &files);

    let mut violations = Vec::new();
    for file in files {
        let content = match fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if content.contains("let _ = auth_user;") {
            violations.push(file.display().to_string());
        }
    }

    assert!(violations.is_empty(), "Found placeholder-style auth ignores in handlers:\n{}", violations.join("\n"));
}

#[test]
fn test_empty_json_successes_are_allowlisted() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let routes_dir = root.join("synapse-web").join("src").join("routes");
    let allowlist_file = root.join("scripts").join("shell_routes_allowlist.txt");
    let allowlist = load_allowlist_entries(&allowlist_file);
    let mut files = Vec::new();
    collect_rs_files(&routes_dir, &mut files);
    files.sort();
    assert_scan_is_non_trivial(&routes_dir, &files);

    let mut violations = Vec::new();
    let mut hits: std::collections::HashSet<String> = std::collections::HashSet::new();
    for file in files {
        let content = match fs::read_to_string(&file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        for (index, line) in content.lines().enumerate() {
            if !line.contains("Ok(empty_json())") {
                continue;
            }

            let relative = file
                .strip_prefix(&routes_dir)
                .expect("file should live under routes_dir")
                .to_string_lossy()
                .replace('\\', "/");
            let entry = format!("{}:{}", relative, index + 1);
            hits.insert(entry.clone());
            if !allowlist.iter().any(|item| item == &entry) {
                violations.push(entry);
            }
        }
    }

    assert!(
        violations.is_empty(),
        "Found Ok(empty_json()) matches missing from shell route allowlist:\n{}",
        violations.join("\n")
    );

    // A one-way allowlist rots: entries for code that was deleted or reflowed
    // stay behind and keep "covering" positions that no longer exist. Measured
    // 2026-09-19, 26 of 37 entries matched nothing (the file had not been touched
    // since the routes moved crates). Requiring an exact match both ways keeps
    // the file a description of reality instead of a growing pile of
    // exemptions — the same "no 'keep it just in case'" rule the repo applies to
    // code (AGENTS.md iron law 1).
    let stale: Vec<&String> = allowlist.iter().filter(|entry| !hits.contains(*entry)).collect();
    assert!(
        stale.is_empty(),
        "shell_routes_allowlist.txt lists entries that match no `Ok(empty_json())` site. Delete \
         them (or fix the line number if the site moved):\n{}",
        stale.iter().map(|entry| entry.as_str()).collect::<Vec<_>>().join("\n")
    );
}

fn python_status(script: &Path, args: &[&str]) -> std::process::ExitStatus {
    Command::new("python3").arg(script).args(args).status().expect("failed to spawn python3")
}

fn run_python(script: &Path, args: &[&str]) {
    let status = python_status(script, args);
    assert!(status.success(), "python3 {script:?} {args:?} failed with {status}");
}

/// E8: the OpenAPI `route-table.json` generator must be byte-deterministic and
/// its output shape pinned. The artifact itself is now regenerated and gated:
/// `docs/openapi/route-table.json` currently holds **948** routes (golden lane ×
/// default profile; its count is reconciled against
/// `tests/unit/fixtures/ledger_export/default.json` by
/// `scripts/contract/check_route_surface_reconciliation.py`, BE-1). Historically
/// it drifted to **1047** on 2026-09-30 (1130 before that) and was **1033** on
/// 2026-09-25 (1146 before that) — each a same-shaped **83**-route leak of
/// non-default-feature routes, see the dated entries below. It comes from a fresh
/// default-feature export (`generated_at` 2026-09-16T00:00:00Z); two no-gate
/// routes that had been missing from it are
/// `GET /_matrix/client/v3/auth/{auth_type}/fallback/web`
/// (assembly::auth_compat) and `GET /_synapse/admin/v1/rate-limit-status`
/// (admin::server). The `openapi-artifact` job in `.github/workflows/ci.yml`
/// now runs `gen_route_table.py --check --ledger <fresh default-feature
/// export>` **before** the generation step that overwrites the file. What this
/// test pins is the generator's own contract: same input → byte-identical
/// output, entries sorted by `(path, method, registered_by)`, keys in a fixed
/// order. A change to that shape requires a deliberate update of the pinned
/// bytes below (AGENTS.md iron law 8). The `--check` red path is exercised too,
/// so the mechanism is proven rather than declared.
///
/// The output also carries the source ledger's coordinate — `ledger_schema_version`,
/// `lane`, `profile_flags` — so the artifact self-describes the `(lane, profile)`
/// slice it was cut from (BE-1). The minimal input ledger below omits
/// `profile_flags`, so `lane` and `profile_flags` pin as `null`.
///
/// Measured 2026-09-25 with the exact CI commands (`cargo build --bin
/// synapse_ledger_export`, i.e. the crate's default features):
///
/// - committed `docs/openapi/route-table.json` — **1033** routes. The file had
///   drifted to **1146**, which no default-feature export could reproduce: the
///   delta was exactly the 83 feature-gated routes the comment below warns
///   about, i.e. an all-extensions export had been committed. Since CI only
///   triggers on `main`/PRs (this work happens on `opt/consolidated`), that red
///   was latent. The 2026-09-25 regeneration restored the CI contract;
/// - the CI export (`--profile=default`, fixed timestamp) — **1033** routes,
///   byte-matching the committed artifact;
/// - an all-extensions build with `--profile=default` — **1116** routes: the
///   extra 83 are feature-gated modules that must not appear in the default
///   artifact, which is why the gate is fed from a default-feature build;
/// - `scripts/api_test/ledger.json` uses the Docker feature set (1096 routes as
///   of 2026-09-25, when `export_ledger.sh` was unbroken) and is deliberately
///   **not** the route-table gate's source.
///
/// 2026-09-30（C74）第二次同型漂移与重生成：committed artifact 已是 **1130** 路由，
/// 而 fresh default-feature 导出是 **1047** ⇒ 差值 83 恰好又是"非默认特性路由"
/// （`/voice/*`（voice/voip 特性）、SAML/CAS 回调（`saml-sso`/`cas-sso`）等），同时含 2 条
/// 新增的默认特性 admin 路由（`GET /_synapse/admin/info`、`GET /_synapse/admin/v1/account/{user_id}`）。
/// CI 的 `openapi-artifact` 因此常红（`--check` 报 839 行差异）；按 CI 的同一条命令重生成后
/// `--check` 与 `client.yaml --check` 双绿。**判据**：本文件的 route 数必须等于
/// `cargo build --bin synapse_ledger_export` 的默认特性导出的 route 数（不是 all-features、
/// 也不是 `ledger.json` 的 Docker 特性集）。
#[test]
fn test_route_table_generator_is_deterministic_and_shape_pinned() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = root.join("scripts").join("api_test").join("gen_route_table.py");
    let tmp = std::env::temp_dir().join(format!("synapse-route-table-shape-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).expect("create temp dir");

    // Intentionally unsorted input with a missing optional field, so the sort
    // order and the `auth: null` / `path_params: []` defaults are both part of
    // what the pin protects.
    let ledger = tmp.join("ledger.json");
    fs::write(
        &ledger,
        r#"{
  "schema_version": "1",
  "generated_at": "2020-01-01T00:00:00Z",
  "state_profile": "default",
  "entries": [
    {"method": "POST", "path": "/b", "registered_by": "mod::two", "path_params": ["id"], "query_params": []},
    {"method": "GET", "path": "/a", "registered_by": "mod::one"}
  ]
}
"#,
    )
    .expect("write ledger");

    let out1 = tmp.join("out1.json");
    let out2 = tmp.join("out2.json");
    run_python(&script, &["--ledger", ledger.to_str().unwrap(), "--output", out1.to_str().unwrap()]);
    run_python(&script, &["--ledger", ledger.to_str().unwrap(), "--output", out2.to_str().unwrap()]);
    let first = fs::read(&out1).expect("read out1");
    let second = fs::read(&out2).expect("read out2");
    assert_eq!(first, second, "route-table generation is not byte-deterministic for the same ledger");

    // Pinned bytes: updating this literal is the deliberate acknowledgement that
    // the route-table artifact's shape/order changed.
    const PINNED: &str = r#"{
  "schema_version": "1",
  "ledger_schema_version": "1",
  "lane": null,
  "generated_at": "2020-01-01T00:00:00Z",
  "source": "synapse_ledger_export",
  "profile": "default",
  "profile_flags": null,
  "total_routes": 2,
  "_meta": {
    "generated_by": "gen_route_table.py",
    "note": "本文件由 CI 自动生成，禁止手改。如需刷新: python3 scripts/api_test/gen_route_table.py"
  },
  "routes": [
    {
      "method": "GET",
      "path": "/a",
      "registered_by": "mod::one",
      "path_params": [],
      "query_params": [],
      "auth": null
    },
    {
      "method": "POST",
      "path": "/b",
      "registered_by": "mod::two",
      "path_params": [
        "id"
      ],
      "query_params": [],
      "auth": null
    }
  ]
}
"#;
    assert_eq!(
        String::from_utf8(first).expect("utf-8"),
        PINNED,
        "gen_route_table.py output shape/order changed. If intended, update the PINNED literal in \
         this test in the same commit — this pin is what forces a deliberate update."
    );

    // `--check` green on a matching reference...
    run_python(&script, &["--ledger", ledger.to_str().unwrap(), "--check", "--expected", out1.to_str().unwrap()]);
    // ...and red once the reference drifts, so a gate that cannot fail is not
    // what we are pinning here.
    fs::write(&out1, "{}\n").expect("mutate reference");
    let status = python_status(
        &script,
        &["--ledger", ledger.to_str().unwrap(), "--check", "--expected", out1.to_str().unwrap()],
    );
    assert!(!status.success(), "gen_route_table.py --check must fail when the reference artifact differs");

    let _ = fs::remove_dir_all(&tmp);
}

/// E9: `EXTRACT_STRICT=1` must go red for a **stale** unresolved-allowlist entry
/// (one that matches no construct the parser still cannot follow), not only for
/// a new one.
///
/// Why this file: it already guards the *other* allowlist
/// (`shell_routes_allowlist.txt`) in both directions for exactly this reason —
/// a one-way allowlist can only grow, so stale entries keep "covering" blind
/// spots that are gone and quietly absorb the next regression. The extractor's
/// allowlist was measured 2026-09-19 with 5 stale entries out of 21 while
/// `EXTRACT_STRICT=1` still exited 0 (they were printed as a `note:` on stdout
/// and never entered `strict_failures`).
///
/// The committed allowlist legitimately has stale entries if this test fails its
/// second half — prune them rather than relaxing the assertion. The first half
/// injects a synthetic stale entry through `EXTRACT_UNRESOLVED_ALLOWLIST` so the
/// red path is exercised without touching the committed file.
#[test]
fn test_extract_unresolved_allowlist_stale_entry_is_red() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = root.join("scripts").join("contract").join("extract_registered.py");
    let real_allow = root.join("scripts").join("contract").join("extract_unresolved_allowlist.txt");
    let tmp = std::env::temp_dir().join(format!("synapse-unresolved-allow-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).expect("create temp dir");

    let stale_allow = tmp.join("stale_allowlist.txt");
    let mut content = fs::read_to_string(&real_allow).expect("read committed allowlist");
    content.push_str("\nambiguous fn this_construct_does_not_exist (99 defs)\n");
    fs::write(&stale_allow, content).expect("write injected allowlist");

    let mut stale_cmd = Command::new("python3");
    stale_cmd.arg(&script).env("EXTRACT_STRICT", "1").env("EXTRACT_UNRESOLVED_ALLOWLIST", &stale_allow);
    let stale = stale_cmd.output().expect("spawn extract_registered.py");
    let stale_stderr = String::from_utf8_lossy(&stale.stderr);
    assert!(!stale.status.success(), "a stale allowlist entry must fail EXTRACT_STRICT=1");
    assert!(
        stale_stderr.contains("stale unresolved-allowlist entries match nothing"),
        "the gate must name a stale allowlist entry as a strict failure, got stderr:\n{stale_stderr}"
    );

    // The committed allowlist must itself be free of stale entries, otherwise no
    // one could satisfy the gate. (This checks only the E9-specific message;
    // other strict failures in the repo are unrelated to this ratchet.)
    let mut real_cmd = Command::new("python3");
    real_cmd.arg(&script).env("EXTRACT_STRICT", "1").env("EXTRACT_UNRESOLVED_ALLOWLIST", &real_allow);
    let real = real_cmd.output().expect("spawn extract_registered.py");
    let real_stderr = String::from_utf8_lossy(&real.stderr);
    assert!(
        !real_stderr.contains("stale unresolved-allowlist entries"),
        "the committed extract_unresolved_allowlist.txt contains stale entries; prune them:\n{real_stderr}"
    );

    let _ = fs::remove_dir_all(&tmp);
}

/// `docs/openapi/route-table.json` 的 `generated_at` 必须等于
/// `scripts/api_test/artifact_common.py::FIXED_TIMESTAMP`。
///
/// ## 为什么需要这一条
///
/// CI 的 `openapi-artifact` job 用
/// `--timestamp="$(python3 -c '… print(artifact_common.FIXED_TIMESTAMP)')"` 做一次
/// **全新的 default-feature 导出**，再 `gen_route_table.py --check` 与 committed
/// 文件逐字节比对。而 golden 车道的夹具
/// `tests/unit/fixtures/ledger_export/{default,worker,all}.json` 钉的是**另一个**
/// 时间戳 `2026-05-02T00:00:00Z`（见 `ledger_export_tests.rs`）。
///
/// 于是"从夹具重生 `route-table.json`"是个陷阱：路由内容完全正确，只有
/// `generated_at` 一行从 `2026-09-16T00:00:00Z` 变成 `2026-05-02T00:00:00Z`，
/// 本地所有门禁都绿（没有一个比对这一行），CI 的 `--check` 却红。
/// **2026-10-29 实测踩到**：`eb2658f38` 从夹具重生后变成 05-02，此后
/// `openapi-artifact` 连续两轮红在 `Verify committed route-table.json is in sync`，
/// 而同一 job 里刚修好的 `_workspace_version()` 根本没机会被执行到。
///
/// 断言取**两边相等**而不是写死 09-16：这样改 `FIXED_TIMESTAMP` 的人必须同批
/// 重生产物，改产物的人也不会把常量落单。
#[test]
fn committed_route_table_carries_the_ci_fixed_timestamp() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    let raw = fs::read_to_string(root.join("docs/openapi/route-table.json")).expect("read committed route-table.json");
    let artifact: serde_json::Value =
        serde_json::from_str(&raw).expect("docs/openapi/route-table.json must be valid JSON");
    let generated_at = artifact
        .get("generated_at")
        .and_then(serde_json::Value::as_str)
        .expect("route-table.json must carry a `generated_at` string");

    let common = fs::read_to_string(root.join("scripts/api_test/artifact_common.py"))
        .expect("read scripts/api_test/artifact_common.py");
    let literal = common
        .lines()
        .find_map(|line| line.trim().strip_prefix("FIXED_TIMESTAMP = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .expect("artifact_common.py must define FIXED_TIMESTAMP as a single quoted string literal");

    assert_eq!(
        generated_at, literal,
        "committed route-table.json 的 generated_at 与 CI 的 FIXED_TIMESTAMP 不一致。\n\
         修法：用 CI 那条命令重生 ——\n\
         \x20 cargo run --bin synapse_ledger_export -- --profile=default \\\n\
         \x20   --timestamp=\"$(python3 -c 'import sys; sys.path.insert(0, \"scripts/api_test\"); \
         import artifact_common; print(artifact_common.FIXED_TIMESTAMP)')\" \
         --output=/tmp/ledger_default_ci.json\n\
         \x20 python3 scripts/api_test/gen_route_table.py --ledger /tmp/ledger_default_ci.json\n\
         不要用 tests/unit/fixtures/ledger_export/default.json 重生 —— 那个夹具钉的是 golden 车道的时间戳。"
    );
}

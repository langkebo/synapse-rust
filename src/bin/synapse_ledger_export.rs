//! Offline route-ledger exporter.
#![cfg_attr(test, allow(clippy::panic))]
//!
//! Thin CLI around `synapse_rust::web::routes::ledger_export`. Emits the
//! `(method, path, registered_by)` manifest the server would wire up for
//! a given feature profile, as deterministic JSON. Consumed downstream
//! by `matrix-js-sdk/scripts/contract-sync.mjs` per
//! `matrix-js-sdk/docs/api-contract/LEDGER_DRIVEN_SDK_PLAN_2026-05-02.md`.
//!
//! Usage:
//!     cargo run --bin synapse_ledger_export -- --profile=default
//!     cargo run --bin synapse_ledger_export -- --profile=worker > out.json
//!     cargo run --bin synapse_ledger_export -- --profile=openclaw --output=/tmp/openclaw.json
//!     cargo run --features all-extensions --bin synapse_ledger_export -- --profile=all
//!
//! Schema documented at `docs/synapse-rust/LEDGER_EXPORT_SCHEMA.md` and
//! frozen at schema_version = "1".

use std::io::Write;
use std::process::ExitCode;

use synapse_rust::web::routes::ledger_export::{build_artifact, profile_for_name, render};

#[derive(Debug)]
struct CliArgs {
    profile: String,
    output: Option<String>,
    commit: Option<String>,
    /// Fixed timestamp override — lets golden-file tests and CI jobs
    /// produce deterministic artifacts without stubbing the system clock.
    timestamp: Option<String>,
    help: bool,
}

fn parse_args(raw: &[String]) -> Result<CliArgs, String> {
    let mut profile = "default".to_string();
    let mut output: Option<String> = None;
    let mut commit: Option<String> = None;
    let mut timestamp: Option<String> = None;
    let mut help = false;

    let mut i = 1;
    while i < raw.len() {
        let arg = &raw[i];
        let (key, inline_val) = match arg.split_once('=') {
            Some((k, v)) => (k, Some(v.to_string())),
            None => (arg.as_str(), None),
        };
        let take_value = |i: &mut usize| -> Result<String, String> {
            if let Some(v) = inline_val.clone() {
                Ok(v)
            } else {
                *i += 1;
                raw.get(*i).cloned().ok_or_else(|| format!("missing value for {key}"))
            }
        };
        match key {
            "--help" | "-h" => help = true,
            "--profile" => profile = take_value(&mut i)?,
            "--output" | "-o" => output = Some(take_value(&mut i)?),
            "--commit" => commit = Some(take_value(&mut i)?),
            "--timestamp" => timestamp = Some(take_value(&mut i)?),
            other => return Err(format!("unknown argument: {other}")),
        }
        i += 1;
    }

    Ok(CliArgs { profile, output, commit, timestamp, help })
}

fn print_help() {
    println!(
        "synapse_ledger_export — export route ledger as JSON for SDK contract sync

Usage:
    synapse_ledger_export [--profile=NAME] [--output=PATH] [--commit=SHA] [--timestamp=ISO]

Profiles:
    default   conditional surfaces off (oidc_enabled/worker_enabled/saml_enabled/openclaw_enabled all false)
    oidc      oidc_enabled = true
    worker    worker_enabled = true
    saml      saml_enabled + oidc_enabled = true
    openclaw  openclaw_enabled = true
    all       every flag true

Note:
    Feature-gated routes only appear if this binary is compiled with the matching
    cargo features. For a full extension ledger, prefer:
        cargo run --features all-extensions --bin synapse_ledger_export -- --profile=all

Flags:
    --profile=NAME    named profile preset (default: default)
    --output=PATH     write to file instead of stdout
    --commit=SHA      record in `synapse_rust_commit` field
    --timestamp=ISO   fixed generated_at (for golden tests / CI artifacts)
    --help            print this message

Schema: docs/synapse-rust/LEDGER_EXPORT_SCHEMA.md
"
    );
}

fn missing_feature_warnings(profile: &str) -> Vec<&'static str> {
    // `mut` needed when feature-specific warnings are pushed; unused otherwise.
    #[allow(unused_mut)]
    let mut warnings = Vec::new();

    if matches!(profile, "all" | "saml") {
        #[cfg(not(feature = "saml-sso"))]
        warnings.push("requested profile enables SAML flags, but this binary was built without `saml-sso`");
    }

    if profile == "all" {
        #[cfg(not(feature = "cas-sso"))]
        warnings.push("requested profile implies CAS coverage, but this binary was built without `cas-sso`");
        #[cfg(not(feature = "openclaw-routes"))]
        warnings
            .push("requested profile implies OpenClaw coverage, but this binary was built without `openclaw-routes`");
        #[cfg(not(feature = "external-services"))]
        warnings.push("requested profile implies external-service coverage, but this binary was built without `external-services`");
        #[cfg(not(feature = "voice-extended"))]
        warnings.push("requested profile implies voice coverage, but this binary was built without `voice-extended`");
        #[cfg(not(feature = "widgets"))]
        warnings.push("requested profile implies widget coverage, but this binary was built without `widgets`");
    }

    if profile == "openclaw" {
        #[cfg(not(feature = "openclaw-routes"))]
        warnings.push("requested profile enables OpenClaw flags, but this binary was built without `openclaw-routes`");
    }

    warnings
}

fn run(args: CliArgs) -> Result<(), String> {
    if args.help {
        print_help();
        return Ok(());
    }

    let flags = profile_for_name(&args.profile).ok_or_else(|| {
        format!("unknown profile '{}' (expected default / oidc / worker / saml / openclaw / all)", args.profile)
    })?;

    for warning in missing_feature_warnings(&args.profile) {
        eprintln!("warning: {warning}");
    }

    let generated_at = args.timestamp.unwrap_or_else(|| chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string());

    let artifact = build_artifact(&args.profile, &flags, args.commit, generated_at);
    let rendered = render(&artifact);

    match args.output {
        Some(path) => std::fs::write(&path, rendered.as_bytes()).map_err(|e| format!("failed to write {path}: {e}"))?,
        None => {
            let mut stdout = std::io::stdout().lock();
            if let Err(error) = stdout.write_all(rendered.as_bytes()) {
                if is_broken_pipe(&error) {
                    return Ok(());
                }
                return Err(format!("failed to write stdout: {error}"));
            }
            if let Err(error) = stdout.flush() {
                if is_broken_pipe(&error) {
                    return Ok(());
                }
                return Err(format!("failed to flush stdout: {error}"));
            }
        }
    }
    Ok(())
}

fn is_broken_pipe(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::BrokenPipe
}

fn main() -> ExitCode {
    let raw: Vec<String> = std::env::args().collect();
    let args = match parse_args(&raw) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!();
            print_help();
            return ExitCode::from(2);
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_accepts_separate_and_equals_forms() {
        let a = parse_args(&["bin".into(), "--profile".into(), "worker".into()])
            .unwrap_or_else(|e| panic!("worker profile args should parse: {e}"));
        assert_eq!(a.profile, "worker");
        let b = parse_args(&["bin".into(), "--profile=worker".into()])
            .unwrap_or_else(|e| panic!("worker profile args with equals form should parse: {e}"));
        assert_eq!(b.profile, "worker");
    }

    #[test]
    fn parse_args_rejects_unknown_flag() {
        assert!(parse_args(&["bin".into(), "--bogus".into()]).is_err());
    }

    #[test]
    fn parse_args_rejects_missing_value() {
        assert!(parse_args(&["bin".into(), "--profile".into()]).is_err());
    }

    #[test]
    fn parse_args_accepts_output_separate_and_equals_forms() {
        let a = parse_args(&["bin".into(), "--output".into(), "out.json".into()])
            .unwrap_or_else(|e| panic!("separate output arg should parse: {e}"));
        assert_eq!(a.output.as_deref(), Some("out.json"));

        let b = parse_args(&["bin".into(), "--output=out.json".into()])
            .unwrap_or_else(|e| panic!("inline output arg should parse: {e}"));
        assert_eq!(b.output.as_deref(), Some("out.json"));
    }

    #[test]
    fn parse_args_rejects_missing_output_value() {
        assert!(parse_args(&["bin".into(), "--output".into()]).is_err());
    }

    #[test]
    fn broken_pipe_detection_matches_error_kind() {
        let err = std::io::Error::from(std::io::ErrorKind::BrokenPipe);
        assert!(is_broken_pipe(&err));
    }

    #[test]
    fn unknown_profile_is_an_error() {
        let args = CliArgs {
            profile: "this-does-not-exist".into(),
            output: None,
            commit: None,
            timestamp: Some("2026-05-02T00:00:00Z".into()),
            help: false,
        };
        assert!(run(args).is_err());
    }

    #[test]
    fn help_flag_short_circuits() {
        let args = CliArgs { profile: "default".into(), output: None, commit: None, timestamp: None, help: true };
        // Must not error and must not try to write to /tmp.
        assert!(run(args).is_ok());
    }
}

# Ledger Export Schema (v1)

> Canonical, frozen specification of the JSON artefact produced by
> `synapse_ledger_export`. Consumed downstream by
> `matrix-js-sdk/scripts/contract-sync.mjs` per
> `matrix-js-sdk/docs/api-contract/LEDGER_DRIVEN_SDK_PLAN_2026-05-02.md`
> §2.1 / §5.1. **Schema versioning is strict**: breaking existing keys
> is a MAJOR bump; new optional fields are MINOR. Consumers pin MAJOR.

## 0. Producer

```
cargo run --bin synapse_ledger_export -- --profile=NAME [--output=PATH] [--commit=SHA] [--timestamp=ISO]
```

- Builds against the library — no database connection, no Axum server,
  no config file.
- Fails with a non-zero exit only on invalid CLI args or IO errors.
- Emits one JSON document per invocation; stdout unless `--output` is
  set.

## 1. Envelope

Top-level keys in the order they appear in the serialized output:

| Key | Type | Notes |
|-----|------|-------|
| `schema_version` | string | Pinned to `"1"`. A MAJOR change here invalidates any consumer still pinned to the previous value. |
| `generated_at` | string (RFC 3339 UTC, `YYYY-MM-DDTHH:MM:SSZ`) | System clock unless `--timestamp` overrides. Golden tests always pass a fixed timestamp. |
| `synapse_rust_commit` | string, optional | 40-hex git SHA when running under CI; omitted via `serde(skip_serializing_if = "Option::is_none")` when not supplied. |
| `state_profile` | enum (see §2) | Name of the preset passed to `--profile`. |
| `profile_flags` | object (see §3) | The four booleans that drive conditional surfaces. |
| `entry_count` | integer | Equals `entries.len()`. Producer asserts. |
| `entries` | array of `LedgerEntry` (see §4) | Sorted ascending by `(path, method, registered_by)`. |

No unknown top-level keys are emitted. Consumers MUST ignore unknown
keys to allow MINOR additions.

## 2. `state_profile` presets

The producer recognises these preset names. Adding a preset requires
updating this table **and** the `profile_for_name` match in
`src/bin/synapse_ledger_export.rs` **and** the SDK-side `generated/`
layout declared in the plan §2.2.

| Name | `oidc_enabled` | `worker_enabled` | `saml_enabled` | `openclaw_enabled` | Purpose |
|------|---------------|------------------|----------------|-------------------|---------|
| `default`  | false | false | false | false | Minimal conditional surface. Canonical "base" manifest. |
| `oidc`     | true  | false | false | false | External/builtin OIDC enabled. |
| `worker`   | false | true  | false | false | Worker body routes reachable. |
| `saml`     | true  | false | true* | false | SAML requested; implies `oidc_enabled` because `oidc::oidc_enabled()` treats SAML as an OIDC provider. |
| `openclaw` | false | false | false | true* | Experimental openclaw + ai_connection surface requested. |
| `all`      | true  | true  | true* | true* | Union of runtime profiles, intersected with the cargo features compiled into the exporter binary. |

Consumers typically pin one of `default`, `worker`, or `all` as
their "canonical" manifest and surface deltas explicitly.

`*` means "enabled if the exporter binary was compiled with the matching feature".
For example, `cargo run --bin synapse_ledger_export -- --profile=all` built without
`--features all-extensions` will still emit `state_profile: "all"`, but feature-gated
booleans such as `saml_enabled` / `openclaw_enabled` will be `false`, and compile-time-only
route modules such as `cas`, `widget`, or `external_service` will not appear in `entries`.

## 3. `profile_flags` object

```
{
  "oidc_enabled":     <bool>,
  "worker_enabled":   <bool>,
  "saml_enabled":     <bool>,
  "openclaw_enabled": <bool>
}
```

Every field is REQUIRED. Names map 1-to-1 to fields on the Rust-side
`ProfileFlags` struct, which in turn mirrors the boolean reads each
`RouteModule::manifest_for_profile` impl performs. These booleans describe the
effective conditional surface of the compiled exporter binary, not merely the
requested preset name. A `ProfileFlags` projection can always be reconstructed
from a live `AppState` via `ProfileFlags::from_state`, guaranteeing offline
artefacts match live-router enumeration.

## 4. `LedgerEntry` object

```
{
  "method":        "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD" | "OPTIONS",
  "path":          "/...",
  "registered_by": "<module-namespace>",
  "path_params":   ["name1", "name2", ...]
}
```

All fields REQUIRED.

- `method`: uppercase ASCII, produced by Rust's `http::Method::as_str()`.
- `path`: the absolute Axum path as registered, including `{name}`
  placeholders. Never rewritten by the producer.
- `registered_by`: stable module namespace, e.g. `"key_backup"`,
  `"assembly::capabilities"`, `"worker"`, `"worker_body"`. New values
  appear only when a new router module or inline compat block is
  declared; renames are breaking.
- `path_params`: ordered list of `{name}` captures extracted from
  `path`. Empty array for paths without placeholders. Guarantees
  every placeholder in `path` appears exactly once here (extraction
  test in-crate).

### Query and auth metadata

Added in schema v1.1 (MINOR bump).

- `query_params`: array of strings. Empty array if not specified.
- `auth`: optional string. One of: `"user"`, `"admin"`, `"optional"`, `"federation"`, `"none"`.

Modules can opt-in to these fields via `with_query_params()` and `with_auth()` on `RouteEntry`.

## 5. Entry ordering

Entries are sorted by:

1. `path` lexicographic ascending
2. then `method` lexicographic ascending
3. then `registered_by` lexicographic ascending

The sort is applied after the raw manifest is collected from
`declared_route_manifest_for_profile(flags)`; a `sort_by` test in the
crate verifies the invariant. Consumers MAY rely on this order for
stable diffs.

## 6. Formatting

- JSON: UTF-8 encoded, LF line endings.
- Indentation: two spaces (matches the SDK-side prettier config).
- Trailing newline at EOF.
- Object keys printed in the declaration order of the Rust structs
  (see §1 table).
- No extra whitespace beyond what `serde_json::ser::PrettyFormatter`
  with a two-space indent produces.

Any consumer comparing two artefacts byte-for-byte is expected to see
zero diff across re-runs with the same profile, commit, and fixed
timestamp.

## 7. Golden-file contract

`tests/ledger_export/golden_roundtrip.rs` owns the authoritative
round-trip: it builds an artefact, renders it, re-parses it, and
asserts byte-identity against checked-in fixtures for the
`default`, `worker`, and `openclaw` profiles. Updating a fixture is a
deliberate act — it signals either a legitimate ledger change or a
schema-incompatible regression.

Run locally:

```
cargo test --test ledger_export golden_roundtrip
```

## 8. Change log

| Date | Schema version | Change | PR |
|------|----------------|--------|-----|
| 2026-05-02 | 1.0 | Initial release. Phase A D1 per LEDGER_DRIVEN_SDK_PLAN. | — |

## 9. Relationship to the live router

The live router uses `assembly::create_router` which calls
`declared_route_manifest_for(&AppState)` to validate against
duplicates. `declared_route_manifest_for` itself delegates to
`declared_route_manifest_for_profile(&ProfileFlags::from_state(state))`,
which is exactly what this binary calls. That means:

- Any route visible to `cargo run --bin synapse_ledger_export --
  --profile=default` is a route that a live server configured with
  `profile_flags` off would also expose — by construction, not by
  convention.
- Any route omitted because the exporter binary was built without a matching
  cargo feature is likewise absent from the live server built from the same
  artifact, even if the preset name is `all`.
- If a future `RouteModule::manifest_for_profile` impl starts reading
  something outside `ProfileFlags`, the plan requires either
  extending `ProfileFlags` or overriding `manifest_for(&AppState)` to
  stay in sync. Either path preserves the parity guarantee.

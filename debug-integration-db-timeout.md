# Debug Session: integration-db-timeout [OPEN]

## Scope
- Symptom: focused integration tests fail before assertions with PostgreSQL connection/setup timeout during shared-template-schema initialization.
- Affected commands:
  - `cargo test --features test-utils --test integration default_vs_aggressive_event_thresholds -- --nocapture`
  - `cargo test --features test-utils --test integration default_capacity_limit_handles_ninth_service -- --nocapture`
- Constraint: do not modify business logic during initial evidence collection.

## Hypotheses
1. PostgreSQL is not listening on `localhost:5432`.
2. PostgreSQL is reachable, but expected databases/users/passwords do not match local state.
3. Integration test setup is exhausting the pool or blocking on shared template schema creation.
4. Test configuration selects an unexpected database URL or timeout configuration.

## Evidence Log
- 2026-06-13: Session initialized.
- 2026-06-13: `pg_isready -h localhost -p 5432` returns `accepting connections`, so PostgreSQL is listening.
- 2026-06-13: direct `psql` attempts to `postgresql://synapse:synapse@localhost:5432/synapse` and both `synapse_test` variants fail with `FATAL: sorry, too many clients already`.
- 2026-06-13: `tests/integration/mod.rs` uses `prepare_shared_test_pool()` by default and reports failures during shared-template-schema setup before test assertions run.
- 2026-06-13: `src/test_utils.rs` shows default test pool settings `TEST_DB_MAX_CONNECTIONS=16` and shared clone concurrency `4`.
- 2026-06-13: local process inspection shows about `98` PostgreSQL backends in `startup waiting` for `synapse` plus one long-lived `DROP DATABASE` backend, consistent with client-slot exhaustion.
- 2026-06-13: after restarting PostgreSQL, connection-slot exhaustion disappeared and a lower-level configuration issue surfaced: local Homebrew PostgreSQL only had role `ljf` and no `synapse`/`synapse_test`.
- 2026-06-13: created local role `synapse` and database `synapse_test`; verified `postgresql://synapse:d3948c491e7dfaccc848b3568bf1bee7@localhost:5432/synapse_test` connects successfully.
- 2026-06-13: `bash docker/db_migrate.sh migrate` against `synapse_test` still fails during `00000000_unified_schema_v10.sql` with `ERROR: relation "rooms" does not exist`.
- 2026-06-13: direct `psql -f migrations/00000000_unified_schema_v10.sql` reproduction confirms the first hard failure at `user_directory` line 228, where `fk_user_directory_room` references `rooms(room_id)` before `rooms` is created later in the same file.
- 2026-06-13: after moving `rooms` before the first foreign-key reference, empty-db bootstrap progressed further and exposed additional baseline drift in `00000000_unified_schema_v10.sql`.
- 2026-06-13: removed two invalid `idx_users_name_trgm` definitions that referenced a non-existent `users.name` column; one of them also used an illegal subquery predicate in a partial index.
- 2026-06-13: removed invalid `idx_key_rotation_config_room` because `key_rotation_config` only has `key` and `value`.
- 2026-06-13: corrected `registration_captcha(session_id)` index to `registration_captcha(captcha_id)`, matching storage-layer queries.
- 2026-06-13: corrected `rendezvous_sessions` index target to the actual table name `rendezvous_session`.
- 2026-06-13: corrected `qr_login_codes` index target to the actual table name `qr_login_transactions`; after this, direct empty-db `psql -f migrations/00000000_unified_schema_v10.sql` completes without a new SQL error tail.
- 2026-06-13: aligned `docker/db_migrate.sh` `schema_migrations` bookkeeping with v10 direction (`executed_at` as BIGINT ms, `is_success` naming), but local migrate still does not complete because the unified baseline keeps surfacing additional relation drift.
- 2026-06-13: after finishing the local `synapse_test` migrate/validate path, a focused integration rerun that set only `TEST_DATABASE_URL` still surfaced broad `sqlx` compile failures. Repeating the same compile with both `DATABASE_URL` and `TEST_DATABASE_URL` proved the errors were environment-driven, not a stable workspace-wide regression.
- 2026-06-13: `cargo test --features test-utils --test integration default_vs_aggressive_event_thresholds --no-run` succeeds once both `DATABASE_URL` and `TEST_DATABASE_URL` point at `synapse_test`.
- 2026-06-13: the first runtime failure after compile recovery was a real test-isolation issue: `test_appservice_scheduler_persists_different_backlog_state_for_default_vs_aggressive_event_thresholds` reused fixed exclusive room namespace regexes across two scenarios in the same shared test pool and hit an application service namespace conflict.
- 2026-06-13: fixed the integration test by introducing a per-scenario `scenario_id` into the exclusive room namespace regex and room IDs, preventing cross-scenario collisions in the shared template schema flow.
- 2026-06-13: after that test-isolation fix, both focused integration commands return exit code `0` when run with `DATABASE_URL=... TEST_DATABASE_URL=... TEST_DB_TEMPLATE_SCHEMA=public`, indicating the scheduler-focused runtime verification path is restored.

## Next Steps
1. Update the audit/report trail to replace the stale "compile-time sqlx blocker" conclusion with the corrected environment-aware result.
2. Keep the local integration recipe explicit: set both `DATABASE_URL` and `TEST_DATABASE_URL`, plus `TEST_DB_TEMPLATE_SCHEMA=public`.
3. Return to the `P0-02` decision path with the focused integration evidence restored.

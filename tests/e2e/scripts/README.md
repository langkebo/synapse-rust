# End-to-end scripts

These scripts exercise a live homeserver over HTTPS (default
`https://localhost:8448`, override with `BASE=...`). They are intentionally
written in shell + `curl` so they can run against any build of the server —
including the packaged Docker image — without needing the project's Rust test
harness.

## key_backup.sh

Walks the full Matrix `/room_keys/*` lifecycle:

1. Register a fresh user and capture the access token.
2. Assert `GET /room_keys/version` returns 404 `M_NOT_FOUND`.
3. Create a backup version and capture `version`.
4. Assert `GET /room_keys/version` returns the spec-shaped flat latest-backup
   object (`algorithm, auth_data, count, etag, version`).
5. Assert `GET /room_keys/version/{v}` returns the same shape.
6. `PUT /room_keys/keys/{room}/{session}?version=…` — single session.
7. `PUT /room_keys/keys/{room}?version=…` — room batch (keyed map).
8. `PUT /room_keys/keys?version=…` — all-rooms batch (nested keyed map).
9. `GET /room_keys/version` — `count` must equal `3`.
10. `GET /room_keys/keys?version=…` — full tree round-trip.
11. `GET /room_keys/keys/{room}?version=…` — `is_verified` round-trips true,
    `session_data` is the raw ciphertext object (no wrapping).
12. `GET /room_keys/keys/{room}/{session}?version=…` — bare `KeyBackupData`.
13. `PUT /room_keys/version/{v}` — update auth_data.
14. `DELETE /room_keys/keys/{room}/{session}?version=…`.
15. `DELETE /room_keys/version/{v}`.
16. `GET /room_keys/version` after delete — must return 404 again.

Run:

```sh
bash tests/e2e/scripts/key_backup.sh
```

Environment overrides:

- `BASE`: base URL (default `https://localhost:8448`).

# DB-04: 消除 events CASCADE 级联 + 安全清理默认 admin 账号

**What to build:** Remove the `ON DELETE CASCADE` FK from `events.room_id` → `rooms.room_id` (currently in DO$$ block of `00000000_unified_schema_v10.sql`), replacing it with application-level cascading via a new migration. Also remove the hardcoded default admin account from the baseline schema, moving it to a post-deploy script.

**Blocked by:** None (can start immediately)

## Steps

### 3a. Remove events CASCADE FK

The current FK is defined in the DO$$ block (around line 4295-4297):

```sql
ALTER TABLE events ADD CONSTRAINT fk_events_room_id
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;
```

Create migration `migrations/YYYYMMDDHHMMSS_remove_events_cascade.sql`:

```sql
-- Remove CASCADE FK from events.room_id
-- Application code will handle cascading event deletion before room deletion
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_room_id;

-- Re-add as SET NULL (events.room_id already allows NULL per baseline schema)
ALTER TABLE events ADD CONSTRAINT fk_events_room_id
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE SET NULL;
```

Create the migration using `cargo sqlx migrate add remove_events_cascade_fk`.

### 3b. Audit room deletion code paths

Find all places that delete a room:

```bash
grep -rn "DELETE FROM rooms\|drop_room\|delete_room" synapse-storage/src/ synapse-services/src/
```

For each path, verify that it first deletes (or archives) all related events. If any path is missing the pre-deletion step, add it.

### 3c. Move default admin account to post-deploy script

In `00000000_unified_schema_v10.sql`, find and remove the admin INSERT (around line 4471-4480). Create a new file `scripts/setup-default-admin.sh`:

```bash
#!/bin/bash
# Post-deploy script: create default admin account
# Run ONLY on fresh database initialization
# WARNING: Delete this account in production before opening to users

psql "$DATABASE_URL" <<-EOSQL
    INSERT INTO users (user_id, username, password_hash, is_admin, must_change_password, created_ts)
    SELECT
        '@admin:localhost',
        'admin',
        '\$argon2id\$v=19\$m=65536,t=3,p=1\$...',  -- placeholder, force change
        TRUE,
        TRUE,
        EXTRACT(EPOCH FROM NOW()) * 1000 AS created_ts
    WHERE NOT EXISTS (SELECT 1 FROM users WHERE username = 'admin');
EOSQL
```

Add this to the deployment checklist and mark it as a required manual step.

### 3d. Verify

```bash
cargo sqlx migrate run
# Verify the FK changed:
psql -c "\d events" | grep fk_events_room_id
# Should show: ... REFERENCES rooms(room_id) ON DELETE SET NULL
```

### 3e. Commit

```bash
git add migrations/ scripts/setup-default-admin.sh
git commit -m "fix(schema): remove events CASCADE FK, move default admin to deploy script"
```

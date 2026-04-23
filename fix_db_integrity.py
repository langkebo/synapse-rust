import psycopg2
conn = psycopg2.connect("postgresql://synapse:synapse@localhost:5432/synapse_test")
conn.autocommit = True
cur = conn.cursor()

indexes = [
    ("idx_room_children_parent_suggested", "room_children", "parent_room_id, suggested_order"),
    ("idx_room_children_child", "room_children", "child_room_id"),
    ("idx_retention_cleanup_queue_status_origin", "retention_cleanup_queue", "status, origin_server_ts"),
    ("idx_retention_cleanup_logs_room_started", "retention_cleanup_logs", "room_id, started_ts"),
    ("idx_deleted_events_index_room_ts", "deleted_events_index", "room_id, event_ts"),
]

for idx_name, table, cols in indexes:
    try:
        cur.execute(f"CREATE INDEX IF NOT EXISTS {idx_name} ON {table}({cols})")
        print(f"Created index {idx_name}")
    except Exception as e:
        print(f"Index {idx_name}: {e}")

# Clean orphan data before adding FK constraints
orphan_cleanups = [
    ("room_children", "parent_room_id", "rooms", "room_id"),
    ("room_children", "child_room_id", "rooms", "room_id"),
    ("retention_cleanup_queue", "room_id", "rooms", "room_id"),
    ("retention_cleanup_logs", "room_id", "rooms", "room_id"),
    ("deleted_events_index", "room_id", "rooms", "room_id"),
    ("room_summary_state", "room_id", "rooms", "room_id"),
    ("room_summary_stats", "room_id", "rooms", "room_id"),
    ("room_summary_update_queue", "room_id", "rooms", "room_id"),
    ("retention_stats", "room_id", "rooms", "room_id"),
]

for table, col, ref_table, ref_col in orphan_cleanups:
    try:
        cur.execute(f"DELETE FROM {table} WHERE {col} NOT IN (SELECT {ref_col} FROM {ref_table})")
        deleted = cur.rowcount
        if deleted > 0:
            print(f"Cleaned {deleted} orphan rows from {table}.{col}")
    except Exception as e:
        print(f"Clean orphan {table}.{col}: {e}")

# Now add FK constraints
constraints = [
    ("room_children", "fk_room_children_parent", "ALTER TABLE room_children ADD CONSTRAINT fk_room_children_parent FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE"),
    ("room_children", "fk_room_children_child", "ALTER TABLE room_children ADD CONSTRAINT fk_room_children_child FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE"),
    ("retention_cleanup_queue", "fk_retention_cleanup_queue_room", "ALTER TABLE retention_cleanup_queue ADD CONSTRAINT fk_retention_cleanup_queue_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE"),
    ("retention_cleanup_logs", "fk_retention_cleanup_logs_room", "ALTER TABLE retention_cleanup_logs ADD CONSTRAINT fk_retention_cleanup_logs_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE"),
    ("deleted_events_index", "fk_deleted_events_index_room", "ALTER TABLE deleted_events_index ADD CONSTRAINT fk_deleted_events_index_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE"),
    ("room_summary_state", "fk_room_summary_state_room", "ALTER TABLE room_summary_state ADD CONSTRAINT fk_room_summary_state_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE"),
    ("room_summary_stats", "fk_room_summary_stats_room", "ALTER TABLE room_summary_stats ADD CONSTRAINT fk_room_summary_stats_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE"),
    ("room_summary_update_queue", "fk_room_summary_update_queue_room", "ALTER TABLE room_summary_update_queue ADD CONSTRAINT fk_room_summary_update_queue_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE"),
    ("retention_stats", "fk_retention_stats_room", "ALTER TABLE retention_stats ADD CONSTRAINT fk_retention_stats_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE"),
]

for table, name, sql in constraints:
    try:
        cur.execute(sql)
        print(f"Added constraint {name}")
    except Exception as e:
        if "already exists" in str(e):
            print(f"Constraint {name} already exists")
        else:
            print(f"Constraint {name}: {e}")

# Add unique constraint for room_children
try:
    cur.execute("ALTER TABLE room_children ADD CONSTRAINT uq_room_children_parent_child UNIQUE (parent_room_id, child_room_id)")
    print("Added uq_room_children_parent_child")
except Exception as e:
    if "already exists" in str(e):
        print("uq_room_children_parent_child already exists")
    else:
        print(f"uq_room_children_parent_child: {e}")

# Add unique constraint for retention_cleanup_queue
try:
    cur.execute("ALTER TABLE retention_cleanup_queue ADD CONSTRAINT uq_retention_cleanup_queue_room_event UNIQUE (room_id, event_id)")
    print("Added uq_retention_cleanup_queue_room_event")
except Exception as e:
    if "already exists" in str(e):
        print("uq_retention_cleanup_queue_room_event already exists")
    else:
        print(f"uq_retention_cleanup_queue_room_event: {e}")

# Add unique constraint for room_summary_state
try:
    cur.execute("ALTER TABLE room_summary_state ADD CONSTRAINT uq_room_summary_state_room_type_state UNIQUE (room_id, state_type, state_key)")
    print("Added uq_room_summary_state_room_type_state")
except Exception as e:
    if "already exists" in str(e):
        print("uq_room_summary_state_room_type_state already exists")
    else:
        print(f"uq_room_summary_state_room_type_state: {e}")

cur.close()
conn.close()
print("Done!")

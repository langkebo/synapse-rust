import psycopg2
conn = psycopg2.connect("postgresql://synapse:synapse@localhost:5432/synapse_test")
conn.autocommit = True
cur = conn.cursor()

cur.execute("SELECT conname FROM pg_constraint WHERE conrelid = 'room_summary_state'::regclass AND contype = 'u'")
rows = cur.fetchall()
print("Unique constraints:", [r[0] for r in rows])

cur.execute("SELECT indexname FROM pg_indexes WHERE tablename = 'room_summary_state'")
rows = cur.fetchall()
print("Indexes:", [r[0] for r in rows])

cur.execute("SELECT column_name FROM information_schema.columns WHERE table_name = 'room_summary_state' AND table_schema = 'public' ORDER BY ordinal_position")
rows = cur.fetchall()
print("Columns:", [r[0] for r in rows])

cur.execute("SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND tablename IN ('room_children','retention_cleanup_queue','retention_cleanup_logs','retention_stats','deleted_events_index','device_trust_status','cross_signing_trust','verification_requests','verification_sas','verification_qr','moderation_actions')")
rows = cur.fetchall()
print("Existing tables:", [r[0] for r in rows])

cur.close()
conn.close()

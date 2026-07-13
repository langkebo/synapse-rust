-- Append-only guard: prevent deletion from audit_events unless
-- the session variable synapse.allow_audit_delete is explicitly set
-- to 'true'. Only the retention cleanup job should bypass this.
-- OPT-024, audit 07 #11.

CREATE OR REPLACE FUNCTION prevent_audit_delete()
RETURNS TRIGGER AS $$
BEGIN
  IF current_setting('synapse.allow_audit_delete', true) IS DISTINCT FROM 'true' THEN
    RAISE EXCEPTION 'audit_events is append-only: deletes are forbidden';
  END IF;
  RETURN OLD;
END;
$$ LANGUAGE plpgsql;

DO $$ BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM pg_trigger WHERE tgname = 'trg_prevent_audit_delete'
  ) THEN
    CREATE TRIGGER trg_prevent_audit_delete
      BEFORE DELETE ON audit_events
      FOR EACH ROW EXECUTE FUNCTION prevent_audit_delete();
  END IF;
END $$;

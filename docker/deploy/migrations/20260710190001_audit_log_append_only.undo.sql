-- Rollback: remove the append-only guard on audit_events.

DROP TRIGGER IF EXISTS trg_prevent_audit_delete ON audit_events;
DROP FUNCTION IF EXISTS prevent_audit_delete();

use sqlx::{Pool, Postgres, Row};

#[derive(Debug, Clone, sqlx::FromRow)]
struct ForeignKeyInfo {
    constraint_name: String,
    table_name: String,
    column_name: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
struct IndexInfo {
    indexname: String,
    tablename: String,
}

struct OrphanDiagnosticSpec {
    key: &'static str,
    count_query: &'static str,
    sample_query: &'static str,
}

struct DatabaseIntegrityChecker {
    pool: Pool<Postgres>,
}

impl DatabaseIntegrityChecker {
    fn new(pool: Pool<Postgres>) -> Self {
        Self { pool }
    }

    async fn check_foreign_keys(&self) -> Result<Vec<ForeignKeyInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, ForeignKeyInfo>(
            r#"
            SELECT
                tc.constraint_name,
                tc.table_name,
                kcu.column_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
                AND tc.table_schema = current_schema()
            ORDER BY tc.table_name, tc.constraint_name
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    async fn check_indexes(&self, table_name: &str) -> Result<Vec<IndexInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, IndexInfo>(
            r#"
            SELECT indexname, tablename
            FROM pg_indexes
            WHERE schemaname = current_schema() AND tablename = $1
            ORDER BY indexname
            "#,
        )
        .bind(table_name)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    async fn fetch_orphan_samples(
        &self,
        sample_query: &str,
    ) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(sample_query).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .map(|row| row.get::<serde_json::Value, _>("sample"))
            .collect())
    }

    async fn build_orphan_entry(
        &self,
        spec: &OrphanDiagnosticSpec,
    ) -> Result<serde_json::Value, sqlx::Error> {
        let count: i64 = sqlx::query_scalar(spec.count_query)
            .fetch_one(&self.pool)
            .await?;
        let samples = if count > 0 {
            self.fetch_orphan_samples(spec.sample_query).await?
        } else {
            Vec::new()
        };

        Ok(serde_json::json!({
            "count": count,
            "samples": samples,
        }))
    }

    async fn check_orphan_data(&self) -> Result<serde_json::Value, sqlx::Error> {
        let orphan_events: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM events e
            WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = e.room_id)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let orphan_memberships: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM room_memberships rm
            WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = rm.user_id)
               OR NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rm.room_id)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let orphan_tokens: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM access_tokens at
            WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = at.user_id)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let orphan_memberships_missing_user: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM room_memberships rm
            WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = rm.user_id)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let orphan_memberships_missing_room: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM room_memberships rm
            WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rm.room_id)
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let room_contract_specs = [
            OrphanDiagnosticSpec {
                key: "room_summary_state",
                count_query: r#"
                    SELECT COUNT(*) FROM room_summary_state rss
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rss.room_id)
                "#,
                sample_query: r#"
                    SELECT jsonb_build_object(
                        'id', rss.id,
                        'room_id', rss.room_id,
                        'event_type', rss.event_type,
                        'state_key', rss.state_key,
                        'event_id', rss.event_id,
                        'updated_ts', rss.updated_ts
                    ) AS sample
                    FROM room_summary_state rss
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rss.room_id)
                    ORDER BY rss.updated_ts DESC, rss.id DESC
                    LIMIT 5
                "#,
            },
            OrphanDiagnosticSpec {
                key: "room_summary_stats",
                count_query: r#"
                    SELECT COUNT(*) FROM room_summary_stats rss
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rss.room_id)
                "#,
                sample_query: r#"
                    SELECT jsonb_build_object(
                        'id', rss.id,
                        'room_id', rss.room_id,
                        'total_events', rss.total_events,
                        'total_messages', rss.total_messages,
                        'last_updated_ts', rss.last_updated_ts
                    ) AS sample
                    FROM room_summary_stats rss
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rss.room_id)
                    ORDER BY rss.last_updated_ts DESC, rss.id DESC
                    LIMIT 5
                "#,
            },
            OrphanDiagnosticSpec {
                key: "room_summary_update_queue",
                count_query: r#"
                    SELECT COUNT(*) FROM room_summary_update_queue rsuq
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rsuq.room_id)
                "#,
                sample_query: r#"
                    SELECT jsonb_build_object(
                        'id', rsuq.id,
                        'room_id', rsuq.room_id,
                        'event_id', rsuq.event_id,
                        'event_type', rsuq.event_type,
                        'status', rsuq.status,
                        'created_ts', rsuq.created_ts
                    ) AS sample
                    FROM room_summary_update_queue rsuq
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rsuq.room_id)
                    ORDER BY rsuq.created_ts DESC, rsuq.id DESC
                    LIMIT 5
                "#,
            },
            OrphanDiagnosticSpec {
                key: "room_children",
                count_query: r#"
                    SELECT COUNT(*) FROM room_children rc
                    WHERE NOT EXISTS (SELECT 1 FROM rooms parent WHERE parent.room_id = rc.parent_room_id)
                       OR NOT EXISTS (SELECT 1 FROM rooms child WHERE child.room_id = rc.child_room_id)
                "#,
                sample_query: r#"
                    SELECT jsonb_build_object(
                        'id', rc.id,
                        'parent_room_id', rc.parent_room_id,
                        'child_room_id', rc.child_room_id,
                        'parent_missing', NOT EXISTS (SELECT 1 FROM rooms parent WHERE parent.room_id = rc.parent_room_id),
                        'child_missing', NOT EXISTS (SELECT 1 FROM rooms child WHERE child.room_id = rc.child_room_id),
                        'updated_ts', rc.updated_ts
                    ) AS sample
                    FROM room_children rc
                    WHERE NOT EXISTS (SELECT 1 FROM rooms parent WHERE parent.room_id = rc.parent_room_id)
                       OR NOT EXISTS (SELECT 1 FROM rooms child WHERE child.room_id = rc.child_room_id)
                    ORDER BY rc.updated_ts DESC NULLS LAST, rc.id DESC
                    LIMIT 5
                "#,
            },
            OrphanDiagnosticSpec {
                key: "retention_cleanup_queue",
                count_query: r#"
                    SELECT COUNT(*) FROM retention_cleanup_queue rcq
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rcq.room_id)
                "#,
                sample_query: r#"
                    SELECT jsonb_build_object(
                        'id', rcq.id,
                        'room_id', rcq.room_id,
                        'event_id', rcq.event_id,
                        'event_type', rcq.event_type,
                        'status', rcq.status,
                        'origin_server_ts', rcq.origin_server_ts
                    ) AS sample
                    FROM retention_cleanup_queue rcq
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rcq.room_id)
                    ORDER BY rcq.origin_server_ts DESC, rcq.id DESC
                    LIMIT 5
                "#,
            },
            OrphanDiagnosticSpec {
                key: "retention_cleanup_logs",
                count_query: r#"
                    SELECT COUNT(*) FROM retention_cleanup_logs rcl
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rcl.room_id)
                "#,
                sample_query: r#"
                    SELECT jsonb_build_object(
                        'id', rcl.id,
                        'room_id', rcl.room_id,
                        'status', rcl.status,
                        'started_ts', rcl.started_ts,
                        'completed_ts', rcl.completed_ts
                    ) AS sample
                    FROM retention_cleanup_logs rcl
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rcl.room_id)
                    ORDER BY rcl.started_ts DESC, rcl.id DESC
                    LIMIT 5
                "#,
            },
            OrphanDiagnosticSpec {
                key: "retention_stats",
                count_query: r#"
                    SELECT COUNT(*) FROM retention_stats rs
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rs.room_id)
                "#,
                sample_query: r#"
                    SELECT jsonb_build_object(
                        'id', rs.id,
                        'room_id', rs.room_id,
                        'total_events', rs.total_events,
                        'events_expired', rs.events_expired,
                        'next_cleanup_ts', rs.next_cleanup_ts
                    ) AS sample
                    FROM retention_stats rs
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rs.room_id)
                    ORDER BY rs.next_cleanup_ts DESC NULLS LAST, rs.id DESC
                    LIMIT 5
                "#,
            },
            OrphanDiagnosticSpec {
                key: "deleted_events_index",
                count_query: r#"
                    SELECT COUNT(*) FROM deleted_events_index dei
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = dei.room_id)
                "#,
                sample_query: r#"
                    SELECT jsonb_build_object(
                        'id', dei.id,
                        'room_id', dei.room_id,
                        'event_id', dei.event_id,
                        'deletion_ts', dei.deletion_ts,
                        'reason', dei.reason
                    ) AS sample
                    FROM deleted_events_index dei
                    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = dei.room_id)
                    ORDER BY dei.deletion_ts DESC, dei.id DESC
                    LIMIT 5
                "#,
            },
        ];

        let mut room_contract_orphans = serde_json::Map::new();
        let mut room_contract_orphan_total = 0_i64;
        for spec in room_contract_specs {
            let entry = self.build_orphan_entry(&spec).await?;
            room_contract_orphan_total += entry
                .get("count")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            room_contract_orphans.insert(spec.key.to_string(), entry);
        }

        let orphan_event_samples = self
            .fetch_orphan_samples(
                r#"
                SELECT jsonb_build_object(
                    'event_id', e.event_id,
                    'room_id', e.room_id,
                    'event_type', e.event_type,
                    'origin_server_ts', e.origin_server_ts
                ) AS sample
                FROM events e
                WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = e.room_id)
                ORDER BY e.origin_server_ts DESC NULLS LAST, e.event_id
                LIMIT 5
                "#,
            )
            .await?;

        let orphan_membership_user_samples = self
            .fetch_orphan_samples(
                r#"
                SELECT jsonb_build_object(
                    'event_id', rm.event_id,
                    'room_id', rm.room_id,
                    'user_id', rm.user_id,
                    'membership', rm.membership,
                    'sender', rm.sender
                ) AS sample
                FROM room_memberships rm
                WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = rm.user_id)
                ORDER BY rm.event_id DESC
                LIMIT 5
                "#,
            )
            .await?;

        let orphan_membership_room_samples = self
            .fetch_orphan_samples(
                r#"
                SELECT jsonb_build_object(
                    'event_id', rm.event_id,
                    'room_id', rm.room_id,
                    'user_id', rm.user_id,
                    'membership', rm.membership,
                    'sender', rm.sender
                ) AS sample
                FROM room_memberships rm
                WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rm.room_id)
                ORDER BY rm.event_id DESC
                LIMIT 5
                "#,
            )
            .await?;

        let orphan_token_samples = self
            .fetch_orphan_samples(
                r#"
                SELECT jsonb_build_object(
                    'id', at.id,
                    'user_id', at.user_id,
                    'device_id', at.device_id,
                    'created_ts', at.created_ts,
                    'expires_at', at.expires_at
                ) AS sample
                FROM access_tokens at
                WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = at.user_id)
                ORDER BY at.created_ts DESC, at.id DESC
                LIMIT 5
                "#,
            )
            .await?;

        Ok(serde_json::json!({
            "orphan_events": orphan_events,
            "orphan_memberships": orphan_memberships,
            "orphan_tokens": orphan_tokens,
            "total_orphans": orphan_events + orphan_memberships + orphan_tokens,
            "membership_breakdown": {
                "missing_user": {
                    "count": orphan_memberships_missing_user,
                    "samples": orphan_membership_user_samples,
                },
                "missing_room": {
                    "count": orphan_memberships_missing_room,
                    "samples": orphan_membership_room_samples,
                }
            },
            "event_samples": orphan_event_samples,
            "token_samples": orphan_token_samples,
            "room_contract_orphans_total": room_contract_orphan_total,
            "room_contract_orphans": room_contract_orphans
        }))
    }

    async fn check_table_exists(&self, table_name: &str) -> Result<bool, sqlx::Error> {
        let exists: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM information_schema.tables 
                WHERE table_schema = current_schema() AND table_name = $1
            )
            "#,
        )
        .bind(table_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(exists.unwrap_or(false))
    }

    async fn check_column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, sqlx::Error> {
        let exists: Option<bool> = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM information_schema.columns 
                WHERE table_schema = current_schema() 
                  AND table_name = $1 
                  AND column_name = $2
            )
            "#,
        )
        .bind(table_name)
        .bind(column_name)
        .fetch_optional(&self.pool)
        .await?;

        Ok(exists.unwrap_or(false))
    }

    async fn verify_field_naming(&self) -> Result<serde_json::Value, sqlx::Error> {
        let timestamp_fields: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND column_name LIKE '%_ts'
              AND data_type = 'bigint'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let bool_fields_with_is: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND (column_name LIKE 'is_%' OR column_name LIKE 'has_%')
              AND data_type = 'boolean'
            "#,
        )
        .fetch_one(&self.pool)
        .await?;

        let inconsistent_timestamp: Vec<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT table_name, column_name, data_type
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND column_name ~ '(created|updated|expires|last_seen|joined)'
              AND column_name !~ '_ts$'
              AND data_type = 'bigint'
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(serde_json::json!({
            "timestamp_fields_count": timestamp_fields,
            "boolean_fields_with_prefix_count": bool_fields_with_is,
            "inconsistent_timestamp_fields": inconsistent_timestamp
        }))
    }

    async fn check_audit_critical_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        let critical_indexes = vec![
            "idx_room_summary_state_room",
            "idx_room_summary_update_queue_status_priority_created",
            "idx_room_children_parent_suggested",
            "idx_room_children_child",
            "idx_retention_cleanup_queue_status_origin",
            "idx_retention_cleanup_logs_room_started",
            "idx_deleted_events_index_room_ts",
            "idx_device_trust_status_user_level",
            "idx_cross_signing_trust_user_trusted",
            "idx_device_verification_request_user_device_pending",
            "idx_device_verification_request_expires_pending",
            "idx_verification_requests_to_user_state",
        ];

        let mut missing = Vec::new();
        for index_name in critical_indexes {
            let exists: Option<bool> = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM pg_indexes
                    WHERE schemaname = current_schema() AND indexname = $1
                )
                "#,
            )
            .bind(index_name)
            .fetch_optional(&self.pool)
            .await?;

            if !exists.unwrap_or(false) {
                missing.push(index_name.to_string());
            }
        }

        Ok(missing)
    }

    async fn check_audit_critical_constraints(&self) -> Result<Vec<String>, sqlx::Error> {
        let critical_constraints = vec![
            (
                "room_summary_state",
                "uq_room_summary_state_room_type_state",
            ),
            ("room_summary_state", "fk_room_summary_state_room"),
            ("room_summary_stats", "fk_room_summary_stats_room"),
            (
                "room_summary_update_queue",
                "fk_room_summary_update_queue_room",
            ),
            ("room_children", "uq_room_children_parent_child"),
            ("room_children", "fk_room_children_parent"),
            ("room_children", "fk_room_children_child"),
            (
                "retention_cleanup_queue",
                "uq_retention_cleanup_queue_room_event",
            ),
            ("retention_cleanup_queue", "fk_retention_cleanup_queue_room"),
            ("retention_cleanup_logs", "fk_retention_cleanup_logs_room"),
            ("retention_stats", "fk_retention_stats_room"),
            ("deleted_events_index", "uq_deleted_events_index_room_event"),
            ("deleted_events_index", "fk_deleted_events_index_room"),
            ("device_trust_status", "uq_device_trust_status_user_device"),
            ("cross_signing_trust", "uq_cross_signing_trust_user_target"),
        ];

        let mut missing = Vec::new();
        for (table_name, constraint_name) in critical_constraints {
            let exists: Option<bool> = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM information_schema.table_constraints
                    WHERE table_schema = current_schema()
                      AND table_name = $1
                      AND constraint_name = $2
                )
                "#,
            )
            .bind(table_name)
            .bind(constraint_name)
            .fetch_optional(&self.pool)
            .await?;

            if !exists.unwrap_or(false) {
                missing.push(format!("{}.{}", table_name, constraint_name));
            }
        }

        Ok(missing)
    }

    async fn get_migration_status(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query(
            r#"
            SELECT version, name, applied_ts, description
            FROM schema_migrations
            ORDER BY applied_ts DESC
            LIMIT 10
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut result = Vec::new();
        for row in rows {
            result.push(serde_json::json!({
                "version": row.get::<Option<String>, _>("version"),
                "name": row.get::<Option<String>, _>("name"),
                "applied_ts": row.get::<Option<i64>, _>("applied_ts"),
                "description": row.get::<Option<String>, _>("description")
            }));
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT_RATE_LIMITS_MIGRATION_SQL: &str = include_str!(
        "../../migrations/20260413000001_align_report_rate_limits_schema_contract.sql"
    );
    const TO_DEVICE_STREAM_ID_SEQ_MIGRATION_SQL: &str =
        include_str!("../../migrations/20260409090000_to_device_stream_id_seq.sql");

    async fn ensure_public_schema_contract_repairs(
        pool: &Pool<Postgres>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS idx_verification_requests_to_user_state
            ON verification_requests(to_user, state, updated_ts DESC)
            "#,
        )
        .execute(pool)
        .await?;

        async fn ensure_constraint(
            pool: &Pool<Postgres>,
            table_name: &str,
            constraint_name: &str,
            alter_sql: &str,
        ) -> Result<(), sqlx::Error> {
            let exists: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM information_schema.table_constraints
                    WHERE table_schema = current_schema()
                      AND table_name = $1
                      AND constraint_name = $2
                )
                "#,
            )
            .bind(table_name)
            .bind(constraint_name)
            .fetch_one(pool)
            .await?;

            if !exists {
                sqlx::query(alter_sql).execute(pool).await?;
            }

            Ok(())
        }

        ensure_constraint(
            pool,
            "room_summary_state",
            "fk_room_summary_state_room",
            "ALTER TABLE room_summary_state ADD CONSTRAINT fk_room_summary_state_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE",
        )
        .await?;
        ensure_constraint(
            pool,
            "room_summary_stats",
            "fk_room_summary_stats_room",
            "ALTER TABLE room_summary_stats ADD CONSTRAINT fk_room_summary_stats_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE",
        )
        .await?;
        ensure_constraint(
            pool,
            "room_summary_update_queue",
            "fk_room_summary_update_queue_room",
            "ALTER TABLE room_summary_update_queue ADD CONSTRAINT fk_room_summary_update_queue_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE",
        )
        .await?;
        ensure_constraint(
            pool,
            "room_children",
            "fk_room_children_parent",
            "ALTER TABLE room_children ADD CONSTRAINT fk_room_children_parent FOREIGN KEY (parent_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE",
        )
        .await?;
        ensure_constraint(
            pool,
            "room_children",
            "fk_room_children_child",
            "ALTER TABLE room_children ADD CONSTRAINT fk_room_children_child FOREIGN KEY (child_room_id) REFERENCES rooms(room_id) ON DELETE CASCADE",
        )
        .await?;
        ensure_constraint(
            pool,
            "retention_cleanup_queue",
            "fk_retention_cleanup_queue_room",
            "ALTER TABLE retention_cleanup_queue ADD CONSTRAINT fk_retention_cleanup_queue_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE",
        )
        .await?;
        ensure_constraint(
            pool,
            "retention_cleanup_logs",
            "fk_retention_cleanup_logs_room",
            "ALTER TABLE retention_cleanup_logs ADD CONSTRAINT fk_retention_cleanup_logs_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE",
        )
        .await?;
        ensure_constraint(
            pool,
            "retention_stats",
            "fk_retention_stats_room",
            "ALTER TABLE retention_stats ADD CONSTRAINT fk_retention_stats_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE",
        )
        .await?;
        ensure_constraint(
            pool,
            "deleted_events_index",
            "fk_deleted_events_index_room",
            "ALTER TABLE deleted_events_index ADD CONSTRAINT fk_deleted_events_index_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE",
        )
        .await?;

        Ok(())
    }

    async fn execute_to_device_stream_id_seq_migration(
        pool: &Pool<Postgres>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(TO_DEVICE_STREAM_ID_SEQ_MIGRATION_SQL)
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn execute_report_rate_limits_migration(
        pool: &Pool<Postgres>,
    ) -> Result<(), sqlx::Error> {
        sqlx::raw_sql(REPORT_RATE_LIMITS_MIGRATION_SQL)
            .execute(pool)
            .await?;
        Ok(())
    }

    async fn connect_integrity_pool() -> Option<Pool<Postgres>> {
        match synapse_rust::test_utils::prepare_isolated_test_pool().await {
            Ok(pool) => {
                let pool = (*pool).clone();
                if let Err(error) = ensure_public_schema_contract_repairs(&pool).await {
                    eprintln!(
                        "Database integrity setup warning: unable to apply public schema contract repairs: {}",
                        error
                    );
                }
                Some(pool)
            }
            Err(error) => {
                eprintln!(
                    "Skipping database integrity tests: unable to prepare isolated schema: {}",
                    error
                );
                None
            }
        }
    }

    async fn connect_empty_integrity_pool() -> Option<Pool<Postgres>> {
        match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
            Ok(pool) => Some((*pool).clone()),
            Err(error) => {
                eprintln!(
                    "Skipping migration regression tests: unable to prepare empty isolated schema: {}",
                    error
                );
                None
            }
        }
    }

    #[test]
    fn test_integrity_checker_struct() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _guard = runtime.enter();
        let checker =
            DatabaseIntegrityChecker::new(Pool::connect_lazy("postgres://localhost/test").unwrap());
        let _ = DatabaseIntegrityChecker::check_foreign_keys;
        let _ = DatabaseIntegrityChecker::check_indexes;
        let _ = DatabaseIntegrityChecker::check_orphan_data;
        let _ = DatabaseIntegrityChecker::check_table_exists;
        let _ = DatabaseIntegrityChecker::check_column_exists;
        let _ = DatabaseIntegrityChecker::verify_field_naming;
        let _ = DatabaseIntegrityChecker::get_migration_status;

        assert!(std::mem::size_of_val(&checker) > 0);
    }

    #[test]
    fn test_foreign_key_info_struct() {
        let info = ForeignKeyInfo {
            constraint_name: "fk_users_devices".to_string(),
            table_name: "devices".to_string(),
            column_name: "user_id".to_string(),
        };

        assert_eq!(info.constraint_name, "fk_users_devices");
        assert_eq!(info.table_name, "devices");
        assert_eq!(info.column_name, "user_id");
    }

    #[test]
    fn test_index_info_struct() {
        let info = IndexInfo {
            indexname: "idx_users_username".to_string(),
            tablename: "users".to_string(),
        };

        assert_eq!(info.indexname, "idx_users_username");
        assert_eq!(info.tablename, "users");
    }

    #[tokio::test]
    async fn test_audit_critical_indexes_exist() {
        let Some(pool) = connect_integrity_pool().await else {
            return;
        };

        let checker = DatabaseIntegrityChecker::new(pool);
        let missing = checker
            .check_audit_critical_indexes()
            .await
            .expect("Failed to check audit critical indexes");

        assert!(
            missing.is_empty(),
            "Missing audit-critical indexes: {:?}",
            missing
        );
    }

    #[tokio::test]
    async fn test_audit_critical_constraints_exist() {
        let Some(pool) = connect_integrity_pool().await else {
            return;
        };

        if let Err(error) = ensure_public_schema_contract_repairs(&pool).await {
            let checker = DatabaseIntegrityChecker::new(pool.clone());
            let orphan_data = checker
                .check_orphan_data()
                .await
                .expect("Failed to inspect orphan data after contract repair failure");
            panic!(
                "Unable to apply public schema contract repairs before constraint audit: {}. Orphan data summary: {}",
                error,
                orphan_data
            );
        }

        let checker = DatabaseIntegrityChecker::new(pool);
        let missing = checker
            .check_audit_critical_constraints()
            .await
            .expect("Failed to check audit critical constraints");

        assert!(
            missing.is_empty(),
            "Missing audit-critical constraints: {:?}",
            missing
        );
    }

    #[tokio::test]
    async fn test_orphan_data_diagnostics_query_executes() {
        let Some(pool) = connect_integrity_pool().await else {
            return;
        };

        let checker = DatabaseIntegrityChecker::new(pool);
        let orphan_data = checker
            .check_orphan_data()
            .await
            .expect("Failed to execute orphan data diagnostics");

        let diagnostics = orphan_data
            .as_object()
            .expect("Expected orphan data diagnostics to be a JSON object");

        assert!(
            diagnostics.contains_key("room_contract_orphans"),
            "Expected room_contract_orphans diagnostics entry"
        );
        assert!(
            diagnostics.contains_key("membership_breakdown"),
            "Expected membership_breakdown diagnostics entry"
        );
        assert!(
            diagnostics.contains_key("event_samples"),
            "Expected event_samples diagnostics entry"
        );
        assert!(
            diagnostics.contains_key("token_samples"),
            "Expected token_samples diagnostics entry"
        );
    }

    #[tokio::test]
    async fn test_verification_requests_pending_index_survives_full_migration_chain() {
        let Some(pool) = connect_integrity_pool().await else {
            return;
        };

        let migration_recorded: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM schema_migrations
                WHERE version = '20260406000001'
            )
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to check schema_migrations for verification_requests index restore");

        assert!(
            migration_recorded,
            "Expected migration 20260406000001 to be recorded in schema_migrations"
        );

        let index_definition: Option<String> = sqlx::query_scalar(
            r#"
            SELECT indexdef
            FROM pg_indexes
            WHERE schemaname = current_schema()
              AND tablename = 'verification_requests'
              AND indexname = 'idx_verification_requests_to_user_state'
            "#,
        )
        .fetch_optional(&pool)
        .await
        .expect("Failed to inspect verification_requests pending index definition");

        let index_definition = index_definition
            .expect("Expected idx_verification_requests_to_user_state after full migration chain");
        let normalized_definition = index_definition.to_ascii_lowercase();

        assert!(
            normalized_definition.contains("(to_user, state, updated_ts desc)"),
            "Unexpected verification_requests pending index definition: {}",
            index_definition
        );
    }

    #[tokio::test]
    async fn test_report_rate_limits_schema_contract_survives_full_migration_chain() {
        let Some(pool) = connect_integrity_pool().await else {
            return;
        };

        let migration_recorded: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM schema_migrations
                WHERE version = '20260413000001'
            )
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to check schema_migrations for report_rate_limits contract migration");

        assert!(
            migration_recorded,
            "Expected migration 20260413000001 to be recorded in schema_migrations"
        );

        let column_names: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT column_name
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'report_rate_limits'
            ORDER BY ordinal_position
            "#,
        )
        .fetch_all(&pool)
        .await
        .expect("Failed to inspect report_rate_limits columns");

        assert!(
            column_names
                .iter()
                .any(|column_name| column_name == "last_report_at"),
            "Expected last_report_at column in report_rate_limits, got {:?}",
            column_names
        );
        assert!(
            column_names
                .iter()
                .any(|column_name| column_name == "blocked_until_at"),
            "Expected blocked_until_at column in report_rate_limits, got {:?}",
            column_names
        );
        assert!(
            column_names
                .iter()
                .any(|column_name| column_name == "block_reason"),
            "Expected block_reason column in report_rate_limits, got {:?}",
            column_names
        );
        assert!(
            !column_names
                .iter()
                .any(|column_name| column_name == "last_report_ts"),
            "Did not expect legacy last_report_ts column in report_rate_limits, got {:?}",
            column_names
        );
        assert!(
            !column_names
                .iter()
                .any(|column_name| column_name == "blocked_until"),
            "Did not expect legacy blocked_until column in report_rate_limits, got {:?}",
            column_names
        );
    }

    #[tokio::test]
    async fn test_to_device_stream_id_seq_migration_handles_empty_table_and_repeat_runs() {
        let Some(pool) = connect_empty_integrity_pool().await else {
            return;
        };

        sqlx::query(
            r#"
            CREATE TABLE to_device_messages (
                stream_id BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create to_device_messages table");

        execute_to_device_stream_id_seq_migration(&pool)
            .await
            .expect("Failed to apply to_device_stream_id_seq migration for empty table");
        execute_to_device_stream_id_seq_migration(&pool)
            .await
            .expect("Failed to reapply to_device_stream_id_seq migration for empty table");

        let sequence_exists: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM pg_class c
                JOIN pg_namespace n ON n.oid = c.relnamespace
                WHERE c.relkind = 'S'
                  AND n.nspname = current_schema()
                  AND c.relname = 'to_device_stream_id_seq'
            )
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("Failed to verify to_device_stream_id_seq existence");

        assert!(sequence_exists, "Expected to_device_stream_id_seq to exist");

        let next_value: i64 = sqlx::query_scalar("SELECT nextval('to_device_stream_id_seq')")
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch next value from to_device_stream_id_seq");

        assert_eq!(
            next_value, 1,
            "Expected empty-table migration to keep next sequence value at 1"
        );
    }

    #[tokio::test]
    async fn test_to_device_stream_id_seq_migration_advances_from_existing_stream_ids() {
        let Some(pool) = connect_empty_integrity_pool().await else {
            return;
        };

        sqlx::query(
            r#"
            CREATE TABLE to_device_messages (
                stream_id BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create to_device_messages table");

        sqlx::query(
            r#"
            INSERT INTO to_device_messages (stream_id)
            VALUES (3), (7), (11)
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to seed to_device_messages stream ids");

        execute_to_device_stream_id_seq_migration(&pool)
            .await
            .expect("Failed to apply to_device_stream_id_seq migration for seeded table");
        execute_to_device_stream_id_seq_migration(&pool)
            .await
            .expect("Failed to reapply to_device_stream_id_seq migration for seeded table");

        let next_value: i64 = sqlx::query_scalar("SELECT nextval('to_device_stream_id_seq')")
            .fetch_one(&pool)
            .await
            .expect("Failed to fetch next value from to_device_stream_id_seq");

        assert_eq!(
            next_value, 12,
            "Expected sequence to continue after the maximum existing stream_id"
        );
    }

    #[tokio::test]
    async fn test_report_rate_limits_migration_repairs_legacy_columns() {
        let Some(pool) = connect_empty_integrity_pool().await else {
            return;
        };

        sqlx::query(
            r#"
            CREATE TABLE report_rate_limits (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL UNIQUE,
                report_count INTEGER DEFAULT 0,
                is_blocked BOOLEAN DEFAULT FALSE,
                blocked_until BIGINT,
                last_report_ts BIGINT,
                created_ts BIGINT NOT NULL
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("Failed to create legacy report_rate_limits table");

        execute_report_rate_limits_migration(&pool)
            .await
            .expect("Failed to apply report_rate_limits migration to legacy schema");
        execute_report_rate_limits_migration(&pool)
            .await
            .expect("Failed to reapply report_rate_limits migration to legacy schema");

        let column_names: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT column_name
            FROM information_schema.columns
            WHERE table_schema = current_schema()
              AND table_name = 'report_rate_limits'
            ORDER BY ordinal_position
            "#,
        )
        .fetch_all(&pool)
        .await
        .expect("Failed to inspect repaired report_rate_limits columns");

        assert!(
            column_names
                .iter()
                .any(|column_name| column_name == "last_report_at"),
            "Expected last_report_at after repair, got {:?}",
            column_names
        );
        assert!(
            column_names
                .iter()
                .any(|column_name| column_name == "blocked_until_at"),
            "Expected blocked_until_at after repair, got {:?}",
            column_names
        );
        assert!(
            column_names
                .iter()
                .any(|column_name| column_name == "block_reason"),
            "Expected block_reason after repair, got {:?}",
            column_names
        );
        assert!(
            column_names
                .iter()
                .any(|column_name| column_name == "updated_ts"),
            "Expected updated_ts after repair, got {:?}",
            column_names
        );
        assert!(
            !column_names
                .iter()
                .any(|column_name| column_name == "last_report_ts"),
            "Did not expect legacy last_report_ts after repair, got {:?}",
            column_names
        );
        assert!(
            !column_names
                .iter()
                .any(|column_name| column_name == "blocked_until"),
            "Did not expect legacy blocked_until after repair, got {:?}",
            column_names
        );
    }

    #[tokio::test]
    async fn test_public_schema_contract_repairs_apply_cleanly() {
        let Some(pool) = connect_integrity_pool().await else {
            return;
        };

        if let Err(error) = ensure_public_schema_contract_repairs(&pool).await {
            let checker = DatabaseIntegrityChecker::new(pool);
            let orphan_data = checker.check_orphan_data().await.expect(
                "Failed to inspect orphan data after public schema contract repair failure",
            );
            panic!(
                "Public schema contract repairs cannot be applied cleanly: {}. Orphan data summary: {}",
                error,
                orphan_data
            );
        }
    }
}

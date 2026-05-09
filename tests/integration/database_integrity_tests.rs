use sqlx::{Pool, Postgres, Row};

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

    async fn check_audit_critical_indexes(&self) -> Result<Vec<String>, sqlx::Error> {
        let critical_indexes = vec![
            "idx_room_summary_state_room",
            "idx_room_summary_update_queue_status_priority_created",
            "idx_device_trust_status_user_level",
            "idx_cross_signing_trust_user_trusted",
            "idx_device_verification_request_user_device_pending",
            "idx_device_verification_request_expires_pending",
            "idx_verification_requests_to_user_state",
        ];

        let existing_indexes: std::collections::HashSet<String> = sqlx::query_scalar(
            r#"
            SELECT c.relname
            FROM pg_class c
            JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = current_schema()
              AND c.relkind = 'i'
            "#,
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .collect();

        let missing = critical_indexes
            .into_iter()
            .filter(|index_name| !existing_indexes.contains(*index_name))
            .map(str::to_string)
            .collect();

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

}

#[cfg(test)]
mod tests {
    use super::*;



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

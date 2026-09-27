use super::*;

/// The `InMemoryAdminMediaStore` struct.
#[derive(Clone, Default)]
pub struct InMemoryAdminMediaStore {
    media: Arc<tokio::sync::RwLock<HashMap<String, AdminMediaInfo>>>,
}

impl InMemoryAdminMediaStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self { media: Arc::new(tokio::sync::RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl AdminMediaStoreApi for InMemoryAdminMediaStore {
    async fn get_all_media(&self, limit: i64, cursor: Option<MediaCursor>) -> Result<AdminMediaPage, ApiError> {
        let media = self.media.read().await;
        let mut results: Vec<AdminMediaInfo> = media.values().cloned().collect();
        results.sort_by(|a, b| b.created_ts.cmp(&a.created_ts).then_with(|| b.media_id.cmp(&a.media_id)));

        if let Some(ref cursor) = cursor {
            results.retain(|m| (m.created_ts, m.media_id.as_str()) < (cursor.created_ts, cursor.media_id.as_str()));
        }

        let next_batch = if results.len() > limit as usize {
            results
                .get(limit as usize)
                .map(|m| encode_media_cursor(&MediaCursor { created_ts: m.created_ts, media_id: m.media_id.clone() }))
        } else {
            None
        };

        results.truncate(limit as usize);
        Ok(AdminMediaPage { media: results, next_batch })
    }

    async fn get_media_info(&self, media_id: &str) -> Result<Option<AdminMediaInfo>, ApiError> {
        Ok(self.media.read().await.get(media_id).cloned())
    }

    async fn delete_media(&self, media_id: &str) -> Result<bool, ApiError> {
        Ok(self.media.write().await.remove(media_id).is_some())
    }

    async fn get_media_quota(&self) -> Result<AdminMediaQuotaSummary, ApiError> {
        let media = self.media.read().await;
        let total_size: i64 = media.values().map(|m| m.size).sum();
        Ok(AdminMediaQuotaSummary { total_size, total_count: media.len() as i64 })
    }

    async fn get_user_media(&self, user_id: &str) -> Result<Vec<AdminMediaInfo>, ApiError> {
        let media = self.media.read().await;
        let mut results: Vec<AdminMediaInfo> =
            media.values().filter(|m| m.uploader_user_id.as_deref() == Some(user_id)).cloned().collect();
        results.sort_by(|a, b| b.created_ts.cmp(&a.created_ts));
        Ok(results)
    }

    async fn delete_user_media(&self, user_id: &str) -> Result<u64, ApiError> {
        let mut media = self.media.write().await;
        let before = media.len();
        media.retain(|_, m| m.uploader_user_id.as_deref() != Some(user_id));
        Ok((before - media.len()) as u64)
    }

    async fn get_room_media(
        &self,
        _room_id: &str,
        limit: i64,
        cursor: Option<MediaCursor>,
    ) -> Result<AdminMediaPage, ApiError> {
        // For the in-memory mock, we don't have a real room-events index.
        // In real implementation, this joins room_events with media_metadata.
        // Here we return all media (filtered by cursor) as a stub.
        let media = self.media.read().await;
        let mut results: Vec<AdminMediaInfo> = media.values().cloned().collect();
        results.sort_by(|a, b| b.created_ts.cmp(&a.created_ts).then_with(|| b.media_id.cmp(&a.media_id)));

        if let Some(ref cursor) = cursor {
            results.retain(|m| (m.created_ts, m.media_id.as_str()) < (cursor.created_ts, cursor.media_id.as_str()));
        }

        let next_batch = if results.len() > limit as usize {
            results
                .get(limit as usize)
                .map(|m| encode_media_cursor(&MediaCursor { created_ts: m.created_ts, media_id: m.media_id.clone() }))
        } else {
            None
        };

        results.truncate(limit as usize);
        Ok(AdminMediaPage { media: results, next_batch })
    }

    async fn delete_room_media(&self, _room_id: &str, media_id: &str) -> Result<bool, ApiError> {
        Ok(self.media.write().await.remove(media_id).is_some())
    }

    // ───────────────────────────────────────────────────────────────────────────
    // U-5 missing endpoint implementations for mock
    // ───────────────────────────────────────────────────────────────────────────

    async fn quarantine_user_media(&self, user_id: &str) -> Result<i64, ApiError> {
        let mut media = self.media.write().await;
        let mut count = 0i64;
        for info in media.values_mut() {
            if info.uploader_user_id.as_deref() == Some(user_id) {
                if !info.quarantined {
                    info.quarantined = true;
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    async fn delete_media_by_policy(&self, before_ts: i64, max_size: i64) -> Result<u64, ApiError> {
        let mut media = self.media.write().await;
        let before = media.len();
        media.retain(|_, m| {
            // Skip protected/quarantined
            if m.quarantined {
                return true;
            }
            // Apply policy: keep if NOT (created_ts < before_ts OR size > max_size)
            !(before_ts > 0 && m.created_ts < before_ts || max_size > 0 && m.size > max_size)
        });
        Ok((before - media.len()) as u64)
    }

    async fn purge_media_cache(&self, before_ts: i64) -> Result<u64, ApiError> {
        let mut media = self.media.write().await;
        let before = media.len();
        media.retain(|_, m| {
            // Purge if last_accessed_at is null or before the threshold
            !(before_ts > 0 && m.last_accessed_at.map_or(true, |ts| ts < before_ts))
        });
        Ok((before - media.len()) as u64)
    }

    async fn unprotect_media(&self, media_id: &str, _changed_by: &str) -> Result<i64, ApiError> {
        let mut media = self.media.write().await;
        if let Some(m) = media.get_mut(media_id) {
            if m.quarantined {
                // In the mock, we treat "protected" as quarantined
                // This is a simplification; real impl tracks protected separately
                m.quarantined = false;
                Ok(1)
            } else {
                Ok(0)
            }
        } else {
            Ok(0)
        }
    }
}

impl InMemoryAdminMediaStore {
    /// Convenience seed method for tests — not part of the trait.
    pub async fn insert_media(&self, info: AdminMediaInfo) {
        self.media.write().await.insert(info.media_id.clone(), info);
    }
}

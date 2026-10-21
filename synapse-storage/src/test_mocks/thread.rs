use super::*;
use synapse_common::current_timestamp_millis;

/// The `InMemoryThreadStore` struct.
#[allow(clippy::type_complexity)]
#[derive(Clone, Default)]
pub struct InMemoryThreadStore {
    roots: Arc<tokio::sync::RwLock<Vec<crate::thread::ThreadRoot>>>,
    replies: Arc<tokio::sync::RwLock<Vec<crate::thread::ThreadReply>>>,
    subscriptions: Arc<tokio::sync::RwLock<HashMap<(String, String, String), crate::thread::ThreadSubscription>>>,
    read_receipts: Arc<tokio::sync::RwLock<HashMap<(String, String, String), crate::thread::ThreadReadReceipt>>>,
    relations: Arc<tokio::sync::RwLock<Vec<crate::thread::ThreadRelation>>>,
    summaries: Arc<tokio::sync::RwLock<HashMap<(String, String), crate::thread::ThreadSummary>>>,
    statistics: Arc<tokio::sync::RwLock<HashMap<(String, String), crate::thread::ThreadStatistics>>>,
    frozen: Arc<tokio::sync::RwLock<HashSet<(String, String)>>>,
    next_id: Arc<std::sync::atomic::AtomicI64>,
}

impl InMemoryThreadStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self {
            roots: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            replies: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            subscriptions: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            read_receipts: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            relations: Arc::new(tokio::sync::RwLock::new(Vec::new())),
            summaries: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            statistics: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
            frozen: Arc::new(tokio::sync::RwLock::new(HashSet::new())),
            next_id: Arc::new(std::sync::atomic::AtomicI64::new(1)),
        }
    }

    fn next_id_val(&self) -> i64 {
        self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}

#[async_trait::async_trait]
impl crate::thread::ThreadStoreApi for InMemoryThreadStore {
    async fn create_thread_root(
        &self,
        params: crate::thread::CreateThreadRootParams,
    ) -> Result<crate::thread::ThreadRoot, sqlx::Error> {
        let now = current_timestamp_millis();
        let root = crate::thread::ThreadRoot {
            id: self.next_id_val(),
            room_id: params.room_id.clone(),
            root_event_id: params.root_event_id.clone(),
            sender: params.sender.clone(),
            thread_id: params.thread_id.clone(),
            reply_count: Some(0),
            last_reply_event_id: None,
            last_reply_sender: None,
            last_reply_ts: None,
            participants: Some(serde_json::json!([params.sender])),
            is_fetched: false,
            created_ts: now,
            updated_ts: None,
        };
        self.roots.write().await.push(root.clone());
        Ok(root)
    }

    async fn get_thread_root(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<crate::thread::ThreadRoot>, sqlx::Error> {
        Ok(self
            .roots
            .read()
            .await
            .iter()
            .find(|r| r.room_id == room_id && r.thread_id.as_deref() == Some(thread_id))
            .cloned())
    }

    async fn get_thread_root_by_event(
        &self,
        room_id: &str,
        root_event_id: &str,
    ) -> Result<Option<crate::thread::ThreadRoot>, sqlx::Error> {
        Ok(self.roots.read().await.iter().find(|r| r.room_id == room_id && r.root_event_id == root_event_id).cloned())
    }

    async fn list_thread_roots(
        &self,
        params: crate::thread::ThreadListParams,
    ) -> Result<Vec<crate::thread::ThreadRoot>, sqlx::Error> {
        let limit = params.limit.unwrap_or(50) as usize;
        let roots = self.roots.read().await;
        let mut filtered: Vec<&crate::thread::ThreadRoot> = roots
            .iter()
            .filter(|r| r.room_id == params.room_id)
            .filter(|r| match &params.participated_by {
                // include=participated：与真实 SQL 同义 —— 根发送者或任一回复者在
                // `participants` 里即算参与过。
                Some(user_id) => r
                    .participants
                    .as_ref()
                    .and_then(|value| value.as_array())
                    .is_some_and(|participants| participants.iter().any(|p| p.as_str() == Some(user_id.as_str()))),
                None => true,
            })
            .filter(|r| match (&params.from, &r.thread_id) {
                (Some(from), Some(tid)) => tid.as_str() > from.as_str(),
                _ => true,
            })
            .collect();
        filtered.sort_by(|a, b| a.thread_id.as_deref().unwrap_or("").cmp(b.thread_id.as_deref().unwrap_or("")));
        Ok(filtered.into_iter().take(limit).cloned().collect())
    }

    async fn list_all_thread_roots(
        &self,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<crate::thread::ThreadRoot>, sqlx::Error> {
        let limit = limit.unwrap_or(50) as usize;
        let roots = self.roots.read().await;
        let mut filtered: Vec<&crate::thread::ThreadRoot> = roots
            .iter()
            .filter(|r| match (&from, &r.thread_id) {
                (Some(from), Some(tid)) => tid.as_str() > from.as_str(),
                _ => true,
            })
            .collect();
        filtered.sort_by(|a, b| a.thread_id.as_deref().unwrap_or("").cmp(b.thread_id.as_deref().unwrap_or("")));
        Ok(filtered.into_iter().take(limit).cloned().collect())
    }

    async fn create_thread_reply(
        &self,
        params: crate::thread::CreateThreadReplyParams,
    ) -> Result<crate::thread::ThreadReply, sqlx::Error> {
        let now = current_timestamp_millis();
        let reply = crate::thread::ThreadReply {
            id: self.next_id_val(),
            room_id: params.room_id.clone(),
            thread_id: params.thread_id.clone(),
            event_id: params.event_id.clone(),
            root_event_id: params.root_event_id.clone(),
            sender: params.sender.clone(),
            in_reply_to_event_id: params.in_reply_to_event_id.clone(),
            content: params.content.clone(),
            origin_server_ts: params.origin_server_ts,
            is_edited: false,
            is_redacted: false,
            redaction_reason: None,
            created_ts: now,
        };
        self.replies.write().await.push(reply.clone());

        // Update the matching thread root: bump reply_count, refresh last reply
        // metadata, and merge sender into the participants JSON array.
        let mut roots = self.roots.write().await;
        for root in roots.iter_mut() {
            if root.room_id == params.room_id && root.thread_id.as_deref() == Some(params.thread_id.as_str()) {
                root.reply_count = Some(root.reply_count.unwrap_or(0) + 1);
                root.last_reply_event_id = Some(params.event_id.clone());
                root.last_reply_sender = Some(params.sender.clone());
                root.last_reply_ts = Some(params.origin_server_ts);
                root.updated_ts = Some(now);
                let mut parts: Vec<String> = root
                    .participants
                    .as_ref()
                    .and_then(|v| serde_json::from_str(v.to_string().as_str()).ok())
                    .unwrap_or_default();
                if !parts.iter().any(|p| p == &params.sender) {
                    parts.push(params.sender.clone());
                }
                root.participants =
                    Some(serde_json::Value::Array(parts.into_iter().map(serde_json::Value::String).collect()));
                break;
            }
        }
        Ok(reply)
    }

    async fn get_thread_replies(
        &self,
        room_id: &str,
        thread_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<crate::thread::ThreadReply>, sqlx::Error> {
        let limit = limit.unwrap_or(50) as usize;
        let replies = self.replies.read().await;
        let mut filtered: Vec<&crate::thread::ThreadReply> = replies
            .iter()
            .filter(|r| r.room_id == room_id && r.thread_id == thread_id)
            .filter(|r| match &from {
                Some(from) => r.event_id.as_str() > from.as_str(),
                None => true,
            })
            .collect();
        filtered.sort_by(|a, b| a.event_id.cmp(&b.event_id));
        Ok(filtered.into_iter().take(limit).cloned().collect())
    }

    async fn get_thread_replies_page(
        &self,
        room_id: &str,
        thread_id: &str,
        direction: &str,
        limit: Option<i32>,
        from: Option<&str>,
    ) -> Result<Option<Vec<crate::thread::ThreadReply>>, sqlx::Error> {
        let limit = limit.unwrap_or(50) as usize;
        let replies = self.replies.read().await;

        let mut matched: Vec<crate::thread::ThreadReply> =
            replies.iter().filter(|r| r.room_id == room_id && r.thread_id == thread_id).cloned().collect();
        matched.sort_by(|a, b| {
            let ordering = a.origin_server_ts.cmp(&b.origin_server_ts).then_with(|| a.event_id.cmp(&b.event_id));
            if direction == "b" {
                ordering.reverse()
            } else {
                ordering
            }
        });

        let anchor = match from {
            Some(event_id) => match matched.iter().find(|r| r.event_id == event_id) {
                Some(anchor) => Some((anchor.origin_server_ts, anchor.event_id.clone())),
                None => return Ok(None),
            },
            None => None,
        };

        let page: Vec<crate::thread::ThreadReply> = matched
            .into_iter()
            .filter(|r| match &anchor {
                Some((ts, event_id)) => {
                    let key = (r.origin_server_ts, r.event_id.as_str());
                    if direction == "b" {
                        key < (*ts, event_id.as_str())
                    } else {
                        key > (*ts, event_id.as_str())
                    }
                }
                None => true,
            })
            .take(limit)
            .collect();

        Ok(Some(page))
    }

    async fn get_reply_count(&self, room_id: &str, thread_id: &str) -> Result<i32, sqlx::Error> {
        let count =
            self.replies.read().await.iter().filter(|r| r.room_id == room_id && r.thread_id == thread_id).count()
                as i32;
        Ok(count)
    }

    async fn get_thread_participants(&self, room_id: &str, thread_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let roots = self.roots.read().await;
        let root = roots.iter().find(|r| r.room_id == room_id && r.thread_id.as_deref() == Some(thread_id));
        Ok(root
            .and_then(|r| r.participants.as_ref())
            .and_then(|v| {
                v.as_array().map(|arr| arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
            })
            .unwrap_or_default())
    }

    async fn subscribe_to_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        notification_level: &str,
    ) -> Result<crate::thread::ThreadSubscription, sqlx::Error> {
        let now = current_timestamp_millis();
        let sub = crate::thread::ThreadSubscription {
            id: self.next_id_val(),
            room_id: room_id.to_string(),
            thread_id: thread_id.to_string(),
            user_id: user_id.to_string(),
            notification_level: notification_level.to_string(),
            is_muted: false,
            is_pinned: false,
            subscribed_ts: now,
            updated_ts: now,
        };
        self.subscriptions
            .write()
            .await
            .insert((room_id.to_string(), thread_id.to_string(), user_id.to_string()), sub.clone());
        Ok(sub)
    }

    async fn unsubscribe_from_thread(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        self.subscriptions.write().await.remove(&(room_id.to_string(), thread_id.to_string(), user_id.to_string()));
        Ok(())
    }

    async fn mute_thread(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<crate::thread::ThreadSubscription, sqlx::Error> {
        let mut subs = self.subscriptions.write().await;
        let key = (room_id.to_string(), thread_id.to_string(), user_id.to_string());
        let sub = subs.entry(key).or_insert_with(|| {
            let now = current_timestamp_millis();
            crate::thread::ThreadSubscription {
                id: 0,
                room_id: room_id.to_string(),
                thread_id: thread_id.to_string(),
                user_id: user_id.to_string(),
                notification_level: "none".to_string(),
                is_muted: false,
                is_pinned: false,
                subscribed_ts: now,
                updated_ts: now,
            }
        });
        sub.is_muted = true;
        sub.updated_ts = current_timestamp_millis();
        Ok(sub.clone())
    }

    async fn get_thread_subscription(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<crate::thread::ThreadSubscription>, sqlx::Error> {
        Ok(self
            .subscriptions
            .read()
            .await
            .get(&(room_id.to_string(), thread_id.to_string(), user_id.to_string()))
            .cloned())
    }

    async fn get_user_thread_subscriptions(
        &self,
        user_id: &str,
        limit: Option<i32>,
        from: Option<String>,
    ) -> Result<Vec<crate::thread::ThreadSubscription>, sqlx::Error> {
        let limit = limit.unwrap_or(50) as usize;
        let subs = self.subscriptions.read().await;
        let mut filtered: Vec<&crate::thread::ThreadSubscription> =
            subs.values().filter(|s| s.user_id == user_id).collect();
        // `from` is a `thread_id` keyset cursor (lexicographic, ASC).
        if let Some(from) = from {
            filtered.retain(|s| s.thread_id > from);
        }
        filtered.sort_by(|a, b| a.thread_id.cmp(&b.thread_id));
        Ok(filtered.into_iter().take(limit).cloned().collect())
    }

    async fn update_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
        event_id: &str,
        origin_server_ts: i64,
    ) -> Result<crate::thread::ThreadReadReceipt, sqlx::Error> {
        let now = current_timestamp_millis();
        let key = (room_id.to_string(), thread_id.to_string(), user_id.to_string());
        let mut receipts = self.read_receipts.write().await;
        let receipt = receipts.entry(key).or_insert_with(|| crate::thread::ThreadReadReceipt {
            id: 0,
            room_id: room_id.to_string(),
            thread_id: thread_id.to_string(),
            user_id: user_id.to_string(),
            last_read_event_id: None,
            last_read_ts: now,
            unread_count: 0,
            updated_ts: now,
        });
        receipt.id = if receipt.id == 0 { self.next_id_val() } else { receipt.id };
        receipt.last_read_event_id = Some(event_id.to_string());
        receipt.last_read_ts = origin_server_ts;
        receipt.unread_count = 0;
        receipt.updated_ts = now;
        Ok(receipt.clone())
    }

    async fn get_read_receipt(
        &self,
        room_id: &str,
        thread_id: &str,
        user_id: &str,
    ) -> Result<Option<crate::thread::ThreadReadReceipt>, sqlx::Error> {
        Ok(self
            .read_receipts
            .read()
            .await
            .get(&(room_id.to_string(), thread_id.to_string(), user_id.to_string()))
            .cloned())
    }

    async fn increment_unread_count(&self, room_id: &str, thread_id: &str, user_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        let key = (room_id.to_string(), thread_id.to_string(), user_id.to_string());
        let mut receipts = self.read_receipts.write().await;
        let receipt = receipts.entry(key).or_insert_with(|| crate::thread::ThreadReadReceipt {
            id: self.next_id_val(),
            room_id: room_id.to_string(),
            thread_id: thread_id.to_string(),
            user_id: user_id.to_string(),
            last_read_event_id: None,
            last_read_ts: now,
            unread_count: 0,
            updated_ts: now,
        });
        receipt.unread_count += 1;
        receipt.updated_ts = now;
        Ok(())
    }

    async fn create_thread_relation(
        &self,
        room_id: &str,
        event_id: &str,
        relates_to_event_id: &str,
        relation_type: &str,
        thread_id: Option<&str>,
        is_falling_back: bool,
    ) -> Result<crate::thread::ThreadRelation, sqlx::Error> {
        let now = current_timestamp_millis();
        let relation = crate::thread::ThreadRelation {
            id: self.next_id_val(),
            room_id: room_id.to_string(),
            event_id: event_id.to_string(),
            relates_to_event_id: relates_to_event_id.to_string(),
            relation_type: relation_type.to_string(),
            thread_id: thread_id.map(|s| s.to_string()),
            is_falling_back,
            created_ts: now,
        };
        self.relations.write().await.push(relation.clone());
        Ok(relation)
    }

    async fn mark_reply_edited(&self, room_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        let mut replies = self.replies.write().await;
        for reply in replies.iter_mut() {
            if reply.room_id == room_id && reply.event_id == event_id {
                reply.is_edited = true;
            }
        }
        Ok(())
    }

    async fn mark_reply_redacted(
        &self,
        room_id: &str,
        event_id: &str,
        reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        let mut replies = self.replies.write().await;
        for reply in replies.iter_mut() {
            if reply.room_id == room_id && reply.event_id == event_id {
                reply.is_redacted = true;
                reply.content = serde_json::json!({});
                reply.redaction_reason = reason.map(ToOwned::to_owned);
            }
        }
        Ok(())
    }

    async fn delete_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        // Remove thread root(s) matching the (room_id, thread_id) pair.
        self.roots.write().await.retain(|r| !(r.room_id == room_id && r.thread_id.as_deref() == Some(thread_id)));
        // Remove all replies belonging to the thread.
        self.replies.write().await.retain(|r| !(r.room_id == room_id && r.thread_id == thread_id));
        // Remove all subscriptions for the thread.
        self.subscriptions.write().await.retain(|(rid, tid, _), _| !(rid == room_id && tid == thread_id));
        // Remove read receipts for the thread.
        self.read_receipts.write().await.retain(|(rid, tid, _), _| !(rid == room_id && tid == thread_id));
        // Remove thread relations associated with the thread.
        self.relations.write().await.retain(|r| !(r.room_id == room_id && r.thread_id.as_deref() == Some(thread_id)));
        // Remove cached summary/statistics entries.
        self.summaries.write().await.remove(&(room_id.to_string(), thread_id.to_string()));
        self.statistics.write().await.remove(&(room_id.to_string(), thread_id.to_string()));
        // Unfreeze if frozen.
        self.frozen.write().await.remove(&(room_id.to_string(), thread_id.to_string()));
        Ok(())
    }

    async fn get_threads_with_unread(
        &self,
        user_id: &str,
        room_id: Option<&str>,
    ) -> Result<Vec<crate::thread::ThreadReadReceipt>, sqlx::Error> {
        let receipts = self.read_receipts.read().await;
        Ok(receipts
            .values()
            .filter(|r| r.user_id == user_id && r.unread_count > 0)
            .filter(|r| match room_id {
                Some(rid) => r.room_id == rid,
                None => true,
            })
            .cloned()
            .collect())
    }

    async fn get_thread_summary(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<crate::thread::ThreadSummary>, sqlx::Error> {
        Ok(self.summaries.read().await.get(&(room_id.to_string(), thread_id.to_string())).cloned())
    }

    async fn get_thread_summaries(
        &self,
        room_id: &str,
        thread_ids: &[String],
    ) -> Result<Vec<crate::thread::ThreadSummary>, sqlx::Error> {
        let summaries = self.summaries.read().await;
        Ok(thread_ids
            .iter()
            .filter_map(|thread_id| summaries.get(&(room_id.to_string(), thread_id.clone())).cloned())
            .collect())
    }

    async fn get_thread_statistics(
        &self,
        room_id: &str,
        thread_id: &str,
    ) -> Result<Option<crate::thread::ThreadStatistics>, sqlx::Error> {
        Ok(self.statistics.read().await.get(&(room_id.to_string(), thread_id.to_string())).cloned())
    }

    async fn search_threads(
        &self,
        params: crate::thread::ThreadSearchParams,
    ) -> Result<Vec<crate::thread::ThreadSearchRow>, sqlx::Error> {
        let limit = params.limit.unwrap_or(20) as usize;
        let q = params.query.to_lowercase();
        let summaries = self.summaries.read().await;

        let mut rows: Vec<crate::thread::ThreadSearchRow> = summaries
            .values()
            .filter(|s| s.room_id == params.room_id)
            .filter(|s| {
                s.thread_id.to_lowercase().contains(&q)
                    || s.root_event_id.to_lowercase().contains(&q)
                    || s.root_sender.to_lowercase().contains(&q)
            })
            .map(|s| crate::thread::ThreadSearchRow {
                id: s.id,
                room_id: s.room_id.clone(),
                thread_id: s.thread_id.clone(),
                root_event_id: s.root_event_id.clone(),
                root_sender: s.root_sender.clone(),
                root_content: s.root_content.clone(),
                root_origin_server_ts: s.root_origin_server_ts,
                latest_event_id: s.latest_event_id.clone(),
                latest_sender: s.latest_sender.clone(),
                latest_content: s.latest_content.clone(),
                latest_origin_server_ts: s.latest_origin_server_ts,
                reply_count: s.reply_count,
                participants: s.participants.clone(),
                is_frozen: s.is_frozen,
                created_ts: s.created_ts,
                updated_ts: s.updated_ts,
                // mock 不做相关性打分 ⇒ 恒为 0.0。于是"排序 + 游标谓词"退化成
                // `(sort_ts DESC, thread_id ASC)` 的普通 keyset，而这与**生产 SQL 在各行
                // 相关性相等时的行为逐字一致**（生产侧用的是同一套三段谓词，只在第一段
                // `rel < cursor.rel` 上因 0.0 而恒假）。所以下面的实现可以安全地
                // 用来验证分页语义本身（不漏行、不重复），但不能用来验证"排序质量"。
                search_relevance: 0.0,
                sort_ts: s.updated_ts,
            })
            .collect();

        // 与生产一致的顺序：`search_relevance DESC`（此处恒等）、`sort_ts DESC`、`thread_id ASC`。
        rows.sort_by(|a, b| b.sort_ts.cmp(&a.sort_ts).then_with(|| a.thread_id.cmp(&b.thread_id)));

        if let Some(cursor) = &params.cursor {
            rows.retain(|row| {
                row.search_relevance < cursor.search_relevance
                    || (row.search_relevance == cursor.search_relevance && row.sort_ts < cursor.sort_ts)
                    || (row.search_relevance == cursor.search_relevance
                        && row.sort_ts == cursor.sort_ts
                        && row.thread_id > cursor.thread_id)
            });
        }

        rows.truncate(limit);
        Ok(rows)
    }

    async fn freeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        self.frozen.write().await.insert((room_id.to_string(), thread_id.to_string()));
        // Mirror the production storage: `ThreadService::add_reply` /
        // `subscribe` gate on `ThreadRoot.is_fetched`, so the mock must
        // flip that field on the actual root or the freeze is invisible to
        // the service layer (see P1 #19634).
        let now = current_timestamp_millis();
        for root in self.roots.write().await.iter_mut() {
            if root.room_id == room_id && root.thread_id.as_deref() == Some(thread_id) {
                root.is_fetched = true;
                root.updated_ts = Some(now);
            }
        }
        Ok(())
    }

    async fn unfreeze_thread(&self, room_id: &str, thread_id: &str) -> Result<(), sqlx::Error> {
        self.frozen.write().await.remove(&(room_id.to_string(), thread_id.to_string()));
        let now = current_timestamp_millis();
        for root in self.roots.write().await.iter_mut() {
            if root.room_id == room_id && root.thread_id.as_deref() == Some(thread_id) {
                root.is_fetched = false;
                root.updated_ts = Some(now);
            }
        }
        Ok(())
    }
}

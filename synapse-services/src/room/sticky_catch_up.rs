//! MSC4354 sticky 事件的联邦**补发**（UG-7 catch-up）。
//!
//! 置顶变更本身是「增量 EDU 推送」：`broadcast_edu_to_room` 只发给当下在线的远端，
//! 发失败就丢在 `federation_queue` 里等重放，没有任何位点。两个缺口因此一直漏：
//!
//! - **#20165 长期断连**：对端离线期间的变更不在内存重放窗口里就永久丢失。
//! - **#20233 新成员**：加入房间的远端服务器拿不到房间**已有**的置顶。
//!
//! 这里用 `room_sticky_events.revision` + `room_sticky_event_destinations` 的
//! per-destination 游标做对账补发：周期性地把「本机 owner 的、revision 大于对端已确认
//! 位点」的行按序直发过去；只有事务确认送达才推进游标，失败就停在原地等下个周期。
//!
//! 三条契约（改动前务必读完）：
//! 1. **只补本机 owner 的行**。别人 owner 的行由他们自己的服务器对账，这里转发会造成
//!    同一行的多源推送与反环判断失效。SQL 里已按 `user_id = 本机 OR user_id LIKE '%:本机'` 过滤。
//! 2. **墓碑也补**。`is_sticky = false` 的行是「清除」这一事实本身，漏发它会让对端永久
//!    保留一条已撤销的置顶。过期行同样按 `is_sticky = false` 补发。
//! 3. **未确认不推进**。`send_edu_directly` 返回 `Ok(false)`（没有出站 client）或 `Err`
//!    都必须停在原地——它的返回值就是这里唯一可信的送达凭据。

use super::service::RoomService;
use synapse_common::current_timestamp_millis;
use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_storage::sticky_event::StickyDeliveryRow;

/// 单个 destination 一轮最多补发的行数（一次事务一行的上限）。
///
/// 取小值是为了让「一个巨大的积压」不会把维护循环的一轮拖到几分钟：每轮只做这么多，
/// 剩下的下个周期继续，游标保证不重复也不跳过。
const CATCH_UP_BATCH: i64 = 128;

/// 一个周期最多对账多少个 `(room, destination)` 对。
///
/// 排序（`ORDER BY room_id, destination`）保证长期挨饿的组合最终会被轮到，而不是
/// 每次都从同一批开始。
const CATCH_UP_DESTINATIONS_PER_TICK: i64 = 32;

/// 构造 MSC4354 sticky 事件 EDU。
///
/// 出站广播（`RoomService::broadcast_sticky_event_update`）与补发共用这一份，
/// 两条路径的字段一旦分叉，对端就会把同一条置顶看成两种变更。
///
/// `sticky_duration_ms`（可空）随行携带，对端按**自己的时钟**推算绝对到期时间，
/// 因此跨机时钟偏差的影响被 1h 上限（`MAX_STICKY_DURATION_MS`）而不是无界放大。
pub(crate) fn build_sticky_edu(
    server_name: &str,
    room_id: &str,
    event_id: &str,
    event_type: &str,
    is_sticky: bool,
    sticky_duration_ms: Option<i64>,
) -> serde_json::Value {
    serde_json::json!({
        "edu_type": "org.matrix.msc4354.sticky_event",
        "room_id": room_id,
        "sender": server_name,
        "content": {
            "event_type": event_type,
            "event_id": event_id,
            "is_sticky": is_sticky,
            "sticky_duration_ms": sticky_duration_ms,
            "ts": current_timestamp_millis()
        }
    })
}

impl RoomService {
    /// 维护循环里的一个补发周期：先播种游标，再把有积压的 destination 推一遍。
    ///
    /// 整个过程 best-effort：失败只记日志，绝不向调用方返回错误——它跑在后台循环里，
    /// 抛错只会让循环带着 panic 退出，而补发的语义本来就是「下个周期再来」。
    pub async fn run_sticky_catch_up_tick(&self) {
        let Some(broadcaster) = self.infra.event_broadcaster.as_ref() else {
            return;
        };

        // 游标表上线前就已加入的远端、以及「本机首次成为 owner」的房间，都要有一行
        // 起点为 0 的游标，否则它们永远不会进入待补发列表。播种是幂等的。
        match self.sticky_event_storage.seed_destination_cursors(&self.server_name).await {
            Ok(0) => {}
            Ok(seeded) => tracing::debug!(seeded, "sticky catch-up: seeded destination cursors"),
            Err(e) => {
                tracing::warn!("sticky catch-up: failed to seed destination cursors: {}", e);
                return;
            }
        }

        let pending = match self
            .sticky_event_storage
            .pending_sticky_destinations(&self.server_name, CATCH_UP_DESTINATIONS_PER_TICK)
            .await
        {
            Ok(pending) => pending,
            Err(e) => {
                tracing::warn!("sticky catch-up: failed to list pending destinations: {}", e);
                return;
            }
        };

        for item in pending {
            self.drain_destination(broadcaster, &item.room_id, &item.destination, item.last_revision).await;
        }
    }

    /// 新成员定向补发（#20233）：`destination` 刚加入 `room_id` 时把房间已有置顶推过去。
    ///
    /// 游标先归零再对账，覆盖「离开又回来」的场景——对方上次的位点已经和本地状态
    /// 脱节，只有全量才安全。best-effort，不阻断 join。
    pub async fn backfill_sticky_events_to_destination(&self, room_id: &str, destination: &str) {
        if destination == self.server_name.as_str() {
            return;
        }
        let Some(broadcaster) = self.infra.event_broadcaster.as_ref() else {
            return;
        };

        if let Err(e) = self.sticky_event_storage.reset_destination_cursor(room_id, destination).await {
            tracing::warn!(
                room_id = %room_id,
                destination = %destination,
                "sticky catch-up: failed to reset destination cursor: {}",
                e
            );
            return;
        }

        let sent = self.drain_destination(broadcaster, room_id, destination, 0).await;
        tracing::info!(
            room_id = %room_id,
            destination = %destination,
            sent,
            "sticky catch-up: backfilled existing sticky events for a new destination"
        );
    }

    /// 把 `(room_id, destination)` 从 `start_revision` 开始的行按序补发，返回补发条数。
    ///
    /// 游标只在**整批确认**后推进一次：中途失败时已经发出去的行下轮会重发，接收侧按
    /// `(room_id, user_id, event_type)` 覆盖写，重复无害；而「没发出去却推进」才会漏。
    async fn drain_destination(
        &self,
        broadcaster: &EventBroadcaster,
        room_id: &str,
        destination: &str,
        start_revision: i64,
    ) -> usize {
        let mut cursor = start_revision;
        let mut sent = 0usize;

        loop {
            let rows = match self
                .sticky_event_storage
                .pending_sticky_rows(room_id, &self.server_name, cursor, CATCH_UP_BATCH)
                .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::warn!(
                        room_id = %room_id,
                        destination = %destination,
                        "sticky catch-up: failed to read pending rows: {}",
                        e
                    );
                    break;
                }
            };
            if rows.is_empty() {
                break;
            }

            let full_batch = rows.len() as i64 == CATCH_UP_BATCH;
            let mut blocked = false;
            for row in &rows {
                let edu = sticky_edu_for_row(&self.server_name, room_id, row);
                match broadcaster.send_edu_directly(destination, &edu, &self.server_name).await {
                    Ok(true) => {
                        cursor = row.revision;
                        sent += 1;
                    }
                    Ok(false) => {
                        // 本机没有出站 client：不是对端的错，位点必须留在原地。
                        blocked = true;
                        break;
                    }
                    Err(e) => {
                        tracing::warn!(
                            room_id = %room_id,
                            destination = %destination,
                            revision = row.revision,
                            "sticky catch-up: failed to deliver sticky EDU: {}",
                            e
                        );
                        blocked = true;
                        break;
                    }
                }
            }

            if cursor > start_revision {
                if let Err(e) = self.sticky_event_storage.advance_destination_cursor(room_id, destination, cursor).await
                {
                    tracing::warn!(
                        room_id = %room_id,
                        destination = %destination,
                        revision = cursor,
                        "sticky catch-up: failed to advance destination cursor: {}",
                        e
                    );
                }
            }

            // 一批没发满 ⇒ 已经追上队尾；中途被挡住 ⇒ 等下一轮。
            if blocked || !full_batch {
                break;
            }
        }

        sent
    }
}

/// 把一行补发记录投影成 EDU：过期行发成移除通知。
///
/// 对端只按 `is_sticky` 维护状态，不会自己看 `expires_at`；如果这里照抄 `is_sticky = true`
/// 而只把 `sticky_duration_ms` 留空，对端会把它当成「永不过期的置顶」——正好与事实相反。
fn sticky_edu_for_row(server_name: &str, room_id: &str, row: &StickyDeliveryRow) -> serde_json::Value {
    let expired = row.is_sticky && row.expires_at.is_some_and(|expires_at| expires_at <= current_timestamp_millis());
    let is_sticky = row.is_sticky && !expired;
    build_sticky_edu(
        server_name,
        room_id,
        &row.event_id,
        &row.event_type,
        is_sticky,
        if is_sticky { row.sticky_duration_ms } else { None },
    )
}

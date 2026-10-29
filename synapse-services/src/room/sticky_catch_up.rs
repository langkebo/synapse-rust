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
//! 四条契约（改动前务必读完）：
//! 1. **只补本机 owner 的行**。别人 owner 的行由他们自己的服务器对账，这里转发会造成
//!    同一行的多源推送与反环判断失效。SQL 里已按 `user_id = 本机 OR user_id LIKE '%:本机'` 过滤。
//! 2. **墓碑也补**。`is_sticky = false` 的行是「清除」这一事实本身，漏发它会让对端永久
//!    保留一条已撤销的置顶。过期行同样按 `is_sticky = false` 补发。
//! 3. **未确认不推进**。`send_edu_directly` 返回 `Ok(false)`（没有出站 client）或 `Err`
//!    都必须停在原地——它的返回值就是这里唯一可信的送达凭据。
//! 4. **只有 `events` 写所有者跑周期 tick**。补发是提交后的 fan-out（联邦 EDU）且推进的是
//!    多实例共享的游标；每个实例都跑会重复发送并让两个进程抢同一位点。判定必须是
//!    **运行时行为**（非所有者零 io 调用），而不是调用点上的一句注释 —— 见 [`run_catch_up_tick`]。
//!
//! 逻辑与 io 之间隔着 [`StickyCatchUpIo`]：正确性全在「顺序 / 确认 / 批次边界」上，
//! 这三件事不需要真数据库和真联邦客户端即可断言（见文件末尾的用例）。

use super::service::RoomService;
use synapse_common::current_timestamp_millis;
use synapse_federation::event_broadcaster::EventBroadcaster;
use synapse_storage::sticky_event::{PendingStickyDelivery, StickyDeliveryRow, StickyEventStorage};

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

/// 补发循环依赖的全部外部能力。
///
/// 抽这一层的唯一理由是**可测**：现有存储层用例覆盖了 SQL，但「调度层」的三条契约
/// （未确认不推进 / 墓碑与过期投影 / 批次边界）在任何真 io 组合下都难以稳定复现。
/// 有了 seam，用例可以精确指定「第 2 条发送返回 `Ok(false)`」这类边界。
///
/// 归属判定（[`StickyCatchUpIo::is_events_writer`]）也放在这里：单写者约束必须是
/// *可观察的行为*（非所有者零 io 调用），而不是调用点的静态声明。
#[async_trait::async_trait]
pub trait StickyCatchUpIo: Send + Sync {
    /// 本机 server name：EDU 的 `sender`，也用于「destinations 里是否有我自己」的短路。
    fn server_name(&self) -> &str;

    /// 本实例是否拥有 `events` 写流（`worker.stream_writers.events`）。
    fn is_events_writer(&self) -> bool;

    /// 播种 `last_revision = 0` 的游标行，返回新建条数（幂等）。
    async fn seed_cursors(&self) -> Result<u64, String>;

    /// 仍有未确认积压的 `(room, destination)` 对。
    async fn pending_destinations(&self, limit: i64) -> Result<Vec<PendingStickyDelivery>, String>;

    /// `room_id` 中本机 owner 且 `revision > after_revision` 的行，按 revision 升序。
    async fn pending_rows(
        &self,
        room_id: &str,
        after_revision: i64,
        limit: i64,
    ) -> Result<Vec<StickyDeliveryRow>, String>;

    /// 直发一条 EDU。`Ok(false)` 表示本机没有出站 client，**不等于**已送达。
    async fn send_edu(&self, destination: &str, edu: &serde_json::Value) -> Result<bool, String>;

    /// 记录对端已确认的最高 revision（实现必须单调）。
    async fn advance_cursor(&self, room_id: &str, destination: &str, revision: i64) -> Result<(), String>;

    /// 把对端游标归零，使下一次 drain 全量重放。
    async fn reset_cursor(&self, room_id: &str, destination: &str) -> Result<(), String>;
}

/// [`StickyCatchUpIo`] 的生产实现：真实存储 + 真实广播器。
struct LiveStickyCatchUpIo<'a> {
    storage: &'a StickyEventStorage,
    broadcaster: &'a EventBroadcaster,
    server_name: &'a str,
    owns_events: bool,
}

#[async_trait::async_trait]
impl StickyCatchUpIo for LiveStickyCatchUpIo<'_> {
    fn server_name(&self) -> &str {
        self.server_name
    }

    fn is_events_writer(&self) -> bool {
        self.owns_events
    }

    async fn seed_cursors(&self) -> Result<u64, String> {
        self.storage.seed_destination_cursors(self.server_name).await.map_err(|e| e.to_string())
    }

    async fn pending_destinations(&self, limit: i64) -> Result<Vec<PendingStickyDelivery>, String> {
        self.storage.pending_sticky_destinations(self.server_name, limit).await.map_err(|e| e.to_string())
    }

    async fn pending_rows(
        &self,
        room_id: &str,
        after_revision: i64,
        limit: i64,
    ) -> Result<Vec<StickyDeliveryRow>, String> {
        self.storage
            .pending_sticky_rows(room_id, self.server_name, after_revision, limit)
            .await
            .map_err(|e| e.to_string())
    }

    async fn send_edu(&self, destination: &str, edu: &serde_json::Value) -> Result<bool, String> {
        self.broadcaster.send_edu_directly(destination, edu, self.server_name).await.map_err(|e| e.to_string())
    }

    async fn advance_cursor(&self, room_id: &str, destination: &str, revision: i64) -> Result<(), String> {
        self.storage.advance_destination_cursor(room_id, destination, revision).await.map_err(|e| e.to_string())
    }

    async fn reset_cursor(&self, room_id: &str, destination: &str) -> Result<(), String> {
        self.storage.reset_destination_cursor(room_id, destination).await.map_err(|e| e.to_string())
    }
}

/// 维护循环里的一个补发周期：先播种游标，再把有积压的 destination 推一遍。
///
/// 整个过程 best-effort：失败只记日志，绝不向调用方返回错误——它跑在后台循环里，
/// 抛错只会让循环带着 panic 退出，而补发的语义本来就是「下个周期再来」。
///
/// **第一件事是问归属**：本实例不是 `events` 写所有者时直接返回，一行 SQL 都不发。
/// 这是契约 4 的唯一执行点；调用点（`src/server/mod.rs` 的后台循环）因此不需要
/// 自己再判一次——判据只有一份，且可以用假 io 断言「非所有者零调用」。
pub async fn run_catch_up_tick(io: &dyn StickyCatchUpIo) {
    if !io.is_events_writer() {
        return;
    }

    // 游标表上线前就已加入的远端、以及「本机首次成为 owner」的房间，都要有一行
    // 起点为 0 的游标，否则它们永远不会进入待补发列表。播种是幂等的。
    match io.seed_cursors().await {
        Ok(0) => {}
        Ok(seeded) => tracing::debug!(seeded, "sticky catch-up: seeded destination cursors"),
        Err(e) => {
            tracing::warn!("sticky catch-up: failed to seed destination cursors: {}", e);
            return;
        }
    }

    let pending = match io.pending_destinations(CATCH_UP_DESTINATIONS_PER_TICK).await {
        Ok(pending) => pending,
        Err(e) => {
            tracing::warn!("sticky catch-up: failed to list pending destinations: {}", e);
            return;
        }
    };

    for item in pending {
        drain_destination(io, &item.room_id, &item.destination, item.last_revision).await;
    }
}

/// 新成员定向补发（#20233）：`destination` 刚加入 `room_id` 时把房间已有置顶推过去。
///
/// 游标先归零再对账，覆盖「离开又回来」的场景——对方上次的位点已经和本地状态
/// 脱节，只有全量才安全。best-effort，不阻断 join。
///
/// 这里**不**加单写者门：它由处理该 join 的实例直接触发，而不是周期性后台任务
/// （契约 4 只约束会自己重复执行的 tick）。
pub async fn backfill_to_destination(io: &dyn StickyCatchUpIo, room_id: &str, destination: &str) {
    if destination == io.server_name() {
        return;
    }

    if let Err(e) = io.reset_cursor(room_id, destination).await {
        tracing::warn!(
            room_id = %room_id,
            destination = %destination,
            "sticky catch-up: failed to reset destination cursor: {}",
            e
        );
        return;
    }

    let sent = drain_destination(io, room_id, destination, 0).await;
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
async fn drain_destination(io: &dyn StickyCatchUpIo, room_id: &str, destination: &str, start_revision: i64) -> usize {
    let mut cursor = start_revision;
    let mut sent = 0usize;

    loop {
        let rows = match io.pending_rows(room_id, cursor, CATCH_UP_BATCH).await {
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
            let edu = sticky_edu_for_row(io.server_name(), room_id, row);
            match io.send_edu(destination, &edu).await {
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
            if let Err(e) = io.advance_cursor(room_id, destination, cursor).await {
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

impl RoomService {
    /// Whether this instance owns the `events` write stream.
    ///
    /// The MSC4354 catch-up tick is a *repeating* post-commit fan-out: a second instance
    /// running it duplicates every EDU and races on the shared per-destination cursor.
    /// The ruling is delegated to [`crate::worker::write_streams::owns_events_dispatch`]
    /// so this loop and the HTTP write-forwarding middleware cannot disagree.
    pub(crate) fn is_events_writer(&self) -> bool {
        self.owns_events
    }

    /// 维护循环入口：把 `self` 适配成 [`StickyCatchUpIo`] 后交给 [`run_catch_up_tick`]。
    pub async fn run_sticky_catch_up_tick(&self) {
        let Some(broadcaster) = self.infra.event_broadcaster.as_ref() else {
            return;
        };
        let io = LiveStickyCatchUpIo {
            storage: &self.sticky_event_storage,
            broadcaster,
            server_name: &self.server_name,
            owns_events: self.is_events_writer(),
        };
        run_catch_up_tick(&io).await;
    }

    /// 新成员定向补发入口（#20233）。
    pub async fn backfill_sticky_events_to_destination(&self, room_id: &str, destination: &str) {
        let Some(broadcaster) = self.infra.event_broadcaster.as_ref() else {
            return;
        };
        let io = LiveStickyCatchUpIo {
            storage: &self.sticky_event_storage,
            broadcaster,
            server_name: &self.server_name,
            owns_events: self.is_events_writer(),
        };
        backfill_to_destination(&io, room_id, destination).await;
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

#[cfg(test)]
#[path = "sticky_catch_up/tests.rs"]
mod tests;

//! 单点直发（UG-7 catch-up）。
//!
//! `broadcast_edu` 的返回值对"补发"这种需要推进位点的调用方不可用：batch 模式下它
//! 只把 EDU 塞进通道就返回 `Ok(())`，没有 federation client 时同样返回 `Ok(())`。
//! 补发据此推进游标就会把没发出去的行当成已送达，永久漏发。这里提供一条绕过 batch
//! 的同步路径，把"到底发出去了没有"如实报给调用方。

use super::{EventBroadcaster, FederationBroadcastError};
use crate::client::FederationTransaction;
use synapse_common::current_timestamp_millis;

impl EventBroadcaster {
    /// 直接向单个 `destination` 同步发送一条 EDU，**不经过 batch 通道**。
    ///
    /// 返回 `Ok(true)` 表示对端事务已确认；`Ok(false)` 表示本机此刻没有出站
    /// client（测试装配或 client 未注入），调用方应保留位点等下个周期重试。
    ///
    /// 绕过 batch 是有意的：batch 只保证"入队"，而补发必须拿到真实送达结果才能
    /// 推进游标。MSC4354 的 EDU 在接收侧按 `(room_id, user_id, event_type)` 覆盖写，
    /// 因此与 batch 通道并发产生重复 EDU 不会造成状态分叉。
    pub async fn send_edu_directly(
        &self,
        destination: &str,
        edu: &serde_json::Value,
        origin: &str,
    ) -> Result<bool, FederationBroadcastError> {
        if destination == self.server_name.as_str() {
            // 自己不是自己的 federation 目的地，视作已送达。
            return Ok(true);
        }

        let Some(client) = self.federation_client.as_ref() else {
            ::tracing::debug!(
                destination = %destination,
                "sticky catch-up: no federation client, keeping the cursor where it is"
            );
            return Ok(false);
        };

        let transaction = FederationTransaction {
            transaction_id: format!("edu_{}_{}", current_timestamp_millis(), uuid::Uuid::new_v4()),
            origin: origin.to_string(),
            origin_server_ts: current_timestamp_millis(),
            destination: destination.to_string(),
            pdus: vec![],
            edus: vec![edu.clone()],
        };

        client
            .send_transaction(destination, &transaction)
            .await
            .map_err(|e| FederationBroadcastError::SendFailed(e.to_string()))?;

        Ok(true)
    }
}

//! Generic event broadcasting abstraction for synapse-rust.
//!
//! This module re-exports the [`EventBroadcaster`] trait and [`BroadcastError`]
//! from `synapse_common::traits`, where the canonical definitions live.
//!
//! # Implementations
//!
//! | Implementation | Location | Purpose |
//! |----------------|----------|---------|
//! | [`crate::event_notifier::EventNotifier`] | `services/event_notifier.rs` | Local sync wake-up — instantly unblocks long-polling `/sync` and sliding-sync connections when new data arrives for a room or user. |
//! | [`synapse_federation::event_broadcaster::EventBroadcaster`] | `federation/event_broadcaster.rs` | Federation outbound — batches and sends PDU/EDU transactions to remote homeservers with retry and persistence. |
//! | [`crate::worker::bus::WorkerBus`] | `worker/bus.rs` | Inter-worker messaging — pub/sub channel for replication commands, stream positions, and worker coordination. |
//!
//! # When to use which
//!
//! * **Client sync wake-up** → `EventNotifier` — it is optimised for
//!   per-room/per-user `tokio::sync::Notify` signalling with optional Redis
//!   cross-instance fan-out.
//! * **Sending events to remote servers** → `federation::EventBroadcaster` —
//!   it handles batching, back-off, DB persistence, and retry for federation
//!   transactions.
//! * **Worker-to-worker / replication commands** → `WorkerBus` — it provides
//!   topic-based pub/sub with `tokio::sync::broadcast` semantics and Redis
//!   bus integration.

pub use synapse_common::traits::{BroadcastError, EventBroadcaster};

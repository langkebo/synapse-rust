/// The `create` module.
pub mod create;
/// The `create_events` module.
pub mod create_events;
/// Graph metadata for the linear event sequence a room creation emits.
pub(crate) mod creation_graph;
/// The `service` module.
pub mod service;
#[cfg(test)]
mod tests;
pub mod upgrade;

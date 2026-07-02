// Re-export config types directly from synapse_common
pub use synapse_common::config::auth::*;
pub use synapse_common::config::builtin_oidc::*;
pub use synapse_common::config::database::*;
pub use synapse_common::config::error::*;
pub use synapse_common::config::experimental::*;
pub use synapse_common::config::federation::*;
pub use synapse_common::config::identity::*;
pub use synapse_common::config::logging::*;
pub use synapse_common::config::performance::*;
pub use synapse_common::config::policy_server::*;
pub use synapse_common::config::push::*;
pub use synapse_common::config::rate_limit::*;
pub use synapse_common::config::retention::*;
pub use synapse_common::config::search::*;
pub use synapse_common::config::security::*;
pub use synapse_common::config::server::*;
pub use synapse_common::config::sms::*;
pub use synapse_common::config::smtp::*;
pub use synapse_common::config::translate::*;
pub use synapse_common::config::voip::*;
pub use synapse_common::config::worker::*;
pub use synapse_common::config::Config;
pub use synapse_common::ConfigManager;

#[cfg(test)]
mod tests;

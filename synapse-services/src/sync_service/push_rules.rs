//! Re-export of push-rule helpers from `synapse_common::push_rules`.
//!
//! The single source of truth for default push-rule generation lives in
//! `synapse_common::push_rules`. This module exists to preserve the historical
//! `synapse_services::sync_service::push_rules::*` import path used by the
//! sync service callers; new code should import from
//! `synapse_common::push_rules` directly.

pub use synapse_common::push_rules::{default_push_rules_for_user, get_default_push_rules, merge_default_push_rules};

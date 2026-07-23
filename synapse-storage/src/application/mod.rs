//! Application service storage domain group.
//!
//! Re-exports application-service-related storage modules under a single
//! namespace so that new application service storage modules can be added here
//! without touching `lib.rs`.
//!
//! Consumers should prefer `synapse_storage::application::ApplicationServiceStorage`
//! over the flat `synapse_storage::ApplicationServiceStorage`.

pub use crate::application_service::{
    ApplicationService, ApplicationServiceEvent, ApplicationServiceNamespace, ApplicationServiceState,
    ApplicationServiceStorage, ApplicationServiceStoreApi, ApplicationServiceTransaction, ApplicationServiceUser,
    NamespaceRule, Namespaces, RegisterApplicationServiceRequest, UpdateApplicationServiceRequest,
};

// P7.3: module (third-party module / spam checker registry) is an
// application-service-related storage module — group it under `application::`
// so it is flat-re-exported via `pub use application::*;` rather than via an
// explicit flat re-export in lib.rs.
pub use crate::module::*;

pub mod backend;
pub mod chunked_upload;
pub mod filesystem;
pub mod models;
pub mod quarantine_stream;
pub mod s3;

pub use backend::*;
pub use chunked_upload::*;
pub use models::*;
pub use quarantine_stream::*;

// Media domain group — re-exports media_quota types under `media::`.
// Consumers should prefer `synapse_storage::media::MediaQuotaStorage` over
// the flat `synapse_storage::MediaQuotaStorage`.
pub use crate::media_quota::{
    CreateQuotaConfigRequest, MediaQuotaAlert, MediaQuotaConfig, MediaQuotaStorage, MediaQuotaStoreApi, MediaUsageLog,
    QuotaCheckResult, ServerMediaQuota, SetUserQuotaRequest, UpdateUsageRequest, UserMediaQuota,
};

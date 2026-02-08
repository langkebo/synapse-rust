use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BackgroundJob {
    SendEmail {
        to: String,
        subject: String,
        body: String,
    },
    ProcessMedia {
        file_id: String,
    },
    FederationTransaction {
        txn_id: String,
        destination: String,
    },
    // General purpose task for migration
    Generic {
        name: String,
        payload: serde_json::Value,
    },
}

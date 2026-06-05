use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Evidence {
    pub id: String,
    pub assertion_id: String,
    pub source: String,
    pub detail: String,
    pub created_at: DateTime<Utc>,
}

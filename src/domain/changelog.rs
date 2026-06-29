use std::fmt::{Display, Formatter};
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangelogAction {
    Assert,
    Retract,
    CascadeMark,
    Depend,
    Verify,
    Sync,
    DeleteEntity,
    Migrate,
}

impl Display for ChangelogAction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Assert => "assert",
            Self::Retract => "retract",
            Self::CascadeMark => "cascade_mark",
            Self::Depend => "depend",
            Self::Verify => "verify",
            Self::Sync => "sync",
            Self::DeleteEntity => "delete_entity",
            Self::Migrate => "migrate",
        })
    }
}

impl FromStr for ChangelogAction {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "assert" => Ok(Self::Assert),
            "retract" => Ok(Self::Retract),
            "cascade_mark" => Ok(Self::CascadeMark),
            "depend" => Ok(Self::Depend),
            "verify" => Ok(Self::Verify),
            "sync" => Ok(Self::Sync),
            "delete_entity" => Ok(Self::DeleteEntity),
            "migrate" => Ok(Self::Migrate),
            _ => Err("invalid changelog action"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangelogEntry {
    pub id: String,
    pub action: ChangelogAction,
    pub target_id: String,
    pub detail: String,
    pub timestamp: DateTime<Utc>,
}

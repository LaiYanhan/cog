use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::str::FromStr;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Assertion {
    pub id: String,
    pub entity_id: String,
    pub kind: AssertionKind,
    pub claim: String,
    pub status: AssertionStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub retraction_reason: Option<String>,
}

#[allow(dead_code)]
impl Assertion {
    pub fn short_id(&self) -> &str {
        if self.id.len() >= 8 {
            &self.id[..8]
        } else {
            &self.id
        }
    }

    pub fn is_active(&self) -> bool {
        self.status == AssertionStatus::Active
    }

    pub fn is_retracted(&self) -> bool {
        self.status == AssertionStatus::Retracted
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum AssertionKind {
    Contract,
    Intent,
    Invariant,
    Fragility,
    Correction,
}

impl Display for AssertionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Contract => "contract",
            Self::Intent => "intent",
            Self::Invariant => "invariant",
            Self::Fragility => "fragility",
            Self::Correction => "correction",
        })
    }
}

impl FromStr for AssertionKind {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "contract" => Ok(Self::Contract),
            "intent" => Ok(Self::Intent),
            "invariant" => Ok(Self::Invariant),
            "fragility" => Ok(Self::Fragility),
            "correction" => Ok(Self::Correction),
            _ => Err("invalid assertion kind"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssertionStatus {
    Active,
    Retracted,
    Uncertain,
}

impl Display for AssertionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Active => "active",
            Self::Retracted => "retracted",
            Self::Uncertain => "uncertain",
        })
    }
}

impl FromStr for AssertionStatus {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "retracted" => Ok(Self::Retracted),
            "uncertain" => Ok(Self::Uncertain),
            _ => Err("invalid assertion status"),
        }
    }
}

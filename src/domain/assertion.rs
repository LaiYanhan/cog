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

/// Return the first 8 characters of an ID string for display.
pub fn short_id(id: &str) -> &str {
    if id.len() >= 8 { &id[..8] } else { id }
}
impl Assertion {
    pub fn is_active(&self) -> bool {
        self.status == AssertionStatus::Active
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn short_id_long_uuid() {
        assert_eq!(short_id("12345678-90ab-cdef-1234-567890abcdef"), "12345678");
    }

    #[test]
    fn short_id_exact_8_chars() {
        assert_eq!(short_id("12345678"), "12345678");
    }

    #[test]
    fn short_id_shorter_than_8() {
        assert_eq!(short_id("abc"), "abc");
    }

    #[test]
    fn short_id_empty() {
        assert_eq!(short_id(""), "");
    }

    #[test]
    fn kind_from_str_roundtrip() {
        for (variant, s) in [
            (AssertionKind::Contract, "contract"),
            (AssertionKind::Intent, "intent"),
            (AssertionKind::Invariant, "invariant"),
            (AssertionKind::Fragility, "fragility"),
            (AssertionKind::Correction, "correction"),
        ] {
            assert_eq!(<AssertionKind as FromStr>::from_str(s), Ok(variant));
            assert_eq!(format!("{}", variant), s);
        }
    }

    #[test]
    fn status_from_str_roundtrip() {
        for (variant, s) in [
            (AssertionStatus::Active, "active"),
            (AssertionStatus::Retracted, "retracted"),
            (AssertionStatus::Uncertain, "uncertain"),
        ] {
            assert_eq!(<AssertionStatus as FromStr>::from_str(s), Ok(variant));
            assert_eq!(format!("{}", variant), s);
        }
    }
}

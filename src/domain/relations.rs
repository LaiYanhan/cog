use std::fmt::{Display, Formatter};
use std::str::FromStr;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

use crate::domain::Entity;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, ValueEnum)]
pub enum EntityRelationKind {
    Contains,
    Calls,
    Uses,
}

impl Display for EntityRelationKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Contains => "contains",
            Self::Calls => "calls",
            Self::Uses => "uses",
        })
    }
}

impl FromStr for EntityRelationKind {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "contains" => Ok(Self::Contains),
            "calls" => Ok(Self::Calls),
            "uses" => Ok(Self::Uses),
            _ => Err("invalid entity relation kind"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AssertionRelationKind {
    DependsOn,
}

impl Display for AssertionRelationKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("depends_on")
    }
}

impl FromStr for AssertionRelationKind {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "depends_on" => Ok(Self::DependsOn),
            _ => Err("invalid assertion relation kind"),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum RelationDirection {
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RelatedEntity {
    pub entity: Entity,
    pub kind: EntityRelationKind,
    pub direction: RelationDirection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityRelation {
    pub id: String,
    pub from_entity: String,
    pub to_entity: String,
    pub kind: EntityRelationKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AssertionRelation {
    pub id: String,
    pub from_assertion: String,
    pub to_assertion: String,
    pub kind: AssertionRelationKind,
}

use std::fmt::{self, Display, Formatter};
use clap::ValueEnum;

use serde::{Deserialize, Serialize};

/// Metadata about an entity extracted during tree-sitter scanning.
///
/// Stored alongside the entity and updated after each scan.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EntityMetrics {
    /// Lines of code (end_line - start_line + 1) from tree-sitter node range.
    pub line_count: Option<u32>,
    /// Number of entities that depend on this one (computed post-scan via BFS).
    pub fan_in: Option<u32>,
    /// Number of entities this one depends on (computed post-scan via BFS).
    pub fan_out: Option<u32>,
    /// Visibility of the symbol.
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    #[default]
    Private,
    Public,
    /// Rust-specific: `pub(crate)`, `pub(super)`, etc.
    Restricted,
}

impl Visibility {
    /// Whether this visibility is public. Used by risk assessment and display filtering.
    #[allow(dead_code)]
    pub fn is_public(self) -> bool {
        matches!(self, Visibility::Public)
    }
}

impl Display for Visibility {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Visibility::Public => write!(f, "public"),
            Visibility::Private => write!(f, "private"),
            Visibility::Restricted => write!(f, "restricted"),
        }
    }
}

impl EntityMetrics {
    /// A zero-metrics instance used for manually created entities.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Create from scan data with line count and visibility.
    /// Wired when tree-sitter line-count extraction is implemented.
    #[allow(dead_code)]
    pub fn from_scan(line_count: u32, visibility: Visibility) -> Self {
        Self {
            line_count: Some(line_count),
            fan_in: None,
            fan_out: None,
            visibility,
        }
    }

    /// Risk heuristic: high fan_in + high line_count → high risk.
    pub fn risk_level(&self) -> RiskLevel {
        let fan = self.fan_in.unwrap_or(0);
        let loc = self.line_count.unwrap_or(0);
        match (fan, loc) {
            (f, _) if f >= 20 => RiskLevel::High,
            (f, l) if f >= 5 && l >= 100 => RiskLevel::Medium,
            _ => RiskLevel::Low,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl Display for RiskLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RiskLevel::Low => write!(f, "low"),
            RiskLevel::Medium => write!(f, "medium"),
            RiskLevel::High => write!(f, "high"),
        }
    }
}

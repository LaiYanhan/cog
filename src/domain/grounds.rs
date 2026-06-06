use serde::{Deserialize, Serialize};

/// Grounds represents the evidence source for an assertion.
/// Format: `source:detail` where source is a non-empty label and detail is the description.
/// If no colon separator is found, the entire string becomes the detail with source="note".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Grounds {
    pub source: String,
    pub detail: String,
}

impl Grounds {
    pub fn parse(raw: &str) -> Self {
        match raw.split_once(':') {
            Some((source, detail)) if !source.is_empty() && !detail.is_empty() => Self {
                source: source.to_string(),
                detail: detail.to_string(),
            },
            _ => Self {
                source: "note".to_string(),
                detail: raw.to_string(),
            },
        }
    }

    /// Validates the grounds format: must have `source:detail` with non-empty parts.
    pub fn validate_format(&self) -> anyhow::Result<()> {
        if self.source.is_empty() || self.detail.is_empty() {
            anyhow::bail!("grounds must have non-empty source and detail");
        }
        Ok(())
    }
}

impl std::fmt::Display for Grounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.source, self.detail)
    }
}

impl From<&str> for Grounds {
    fn from(s: &str) -> Self {
        Self::parse(s)
    }
}

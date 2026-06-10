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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_code_grounds() {
        let g = Grounds::parse("code:auth::login");
        assert_eq!(g.source, "code");
        assert_eq!(g.detail, "auth::login");
    }

    #[test]
    fn parse_plan_grounds() {
        let g = Grounds::parse("plan:refactor");
        assert_eq!(g.source, "plan");
        assert_eq!(g.detail, "refactor");
    }

    #[test]
    fn parse_no_colon_falls_back_to_note() {
        let g = Grounds::parse("just a note");
        assert_eq!(g.source, "note");
        assert_eq!(g.detail, "just a note");
    }

    #[test]
    fn parse_empty_source_falls_back() {
        let g = Grounds::parse(":detail");
        assert_eq!(g.source, "note");
        assert_eq!(g.detail, ":detail");
    }

    #[test]
    fn parse_empty_detail_falls_back() {
        let g = Grounds::parse("source:");
        assert_eq!(g.source, "note");
        assert_eq!(g.detail, "source:");
    }

    #[test]
    fn parse_colon_in_detail() {
        let g = Grounds::parse("code:a::b::c");
        assert_eq!(g.source, "code");
        assert_eq!(g.detail, "a::b::c");
    }

    #[test]
    fn validate_format_ok() {
        let g = Grounds::parse("code:x");
        assert!(g.validate_format().is_ok());
    }

    #[test]
    fn display_roundtrip() {
        let g = Grounds::parse("code:auth::login");
        assert_eq!(format!("{}", g), "code:auth::login");
    }
}
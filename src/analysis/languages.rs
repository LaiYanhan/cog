use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Python,
    Rust,
    JavaScript,
    C,
    Go,
    Java,
}

impl Language {
    pub fn from_extension(ext: &str) -> Option<Language> {
        match ext {
            "py" => Some(Language::Python),
            "rs" => Some(Language::Rust),
            "js" | "jsx" | "mjs" => Some(Language::JavaScript),
            "c" | "h" => Some(Language::C),
            "go" => Some(Language::Go),
            "java" => Some(Language::Java),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Language::Python => "python",
            Language::Rust => "rust",
            Language::JavaScript => "javascript",
            Language::C => "c",
            Language::Go => "go",
            Language::Java => "java",
        }
    }
}

pub fn language_for_path(path: &Path) -> Option<Language> {
    let ext = path.extension()?.to_str()?;
    Language::from_extension(ext)
}

impl std::str::FromStr for Language {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "python" | "py" => Ok(Language::Python),
            "rust" | "rs" => Ok(Language::Rust),
            "javascript" | "js" | "jsx" => Ok(Language::JavaScript),
            "c" => Ok(Language::C),
            "go" => Ok(Language::Go),
            "java" => Ok(Language::Java),
            other => Err(format!("unknown language: {other}")),
        }
    }
}

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::domain::Entity;

use super::extractors::{self, FileScan};
use super::languages::Language;
use super::pool::ParserPool;
use super::report::ScanReport;

// ── ScanConfig ────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct ScanConfig {
    pub root: PathBuf,
    pub languages: Option<Vec<Language>>,
}

// ── Scanner ───────────────────────────────────────────────────────────────

/// Deterministic, zero-LLM scanner of the code space.
///
/// Walks the filesystem, parses source files via tree-sitter, and produces a
/// [`ScanReport`] with raw definitions, imports, and pre-built [`Entity`] objects.
pub struct Scanner {
    pool: ParserPool,
}

impl Scanner {
    pub fn new() -> Self {
        Self {
            pool: ParserPool::new(),
        }
    }

    /// Execute a full scan.  Returns a [`ScanReport`] — pure data, no
    /// Repository interaction.
    pub fn scan(&mut self, config: &ScanConfig) -> Result<ScanReport> {
        let mut result = ScanReport::default();
        super::walker::walk_and_scan(&config.root, config, &mut self.pool, &mut result)?;

        // Collect detected languages from file counts.
        let mut langs: Vec<Language> = result
            .files_by_language
            .keys()
            .filter_map(|name| name.parse().ok())
            .collect();
        langs.sort_by_key(|l: &Language| l.as_str());
        result.languages_detected = langs;

        // Build Entity objects from definitions.
        for def in &result.definitions {
            let entity =
                Entity::from_scan(def.qualified_name.clone(), def.kind, Default::default());
            result.entities.push(entity);
        }

        Ok(result)
    }
}

// ── scan_file ─────────────────────────────────────────────────────────────

/// Parse a single source file and extract definitions + imports.
pub fn scan_file(
    path: &Path,
    lang: Language,
    root: &Path,
    pool: &mut ParserPool,
) -> Result<FileScan> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

    let parser = pool.acquire(lang)?;

    let tree = parser.parse(&source, None).context("parse failed")?;

    let module_qname = qualified_name_from_path(path, root, lang);

    let root_node = tree.root_node();
    let mut cursor = root_node.walk();

    let (definitions, imports, calls) =
        extractors::extract(&root_node, &source, lang, &module_qname, &mut cursor);

    Ok(FileScan {
        path: path.to_path_buf(),
        definitions,
        imports,
        calls,
    })
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Convert a file path to a module qualified name.
/// `src/auth/login.rs` relative to root → `auth::login`.
/// For Python, strip `.py` extension; for others strip any extension.
fn qualified_name_from_path(path: &Path, root: &Path, lang: Language) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);

    // Strip extension
    let without_ext = match lang {
        Language::Python => rel.with_extension(""),
        _ => {
            if let Some(stem) = rel.file_stem() {
                rel.with_file_name(stem)
            } else {
                rel.to_path_buf()
            }
        }
    };

    // Convert path separators to ::
    without_ext
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "::")
}

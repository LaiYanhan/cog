use super::{c, go, java, javascript, python, rust};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tree_sitter::{Language as TsLanguage, Node, Parser, TreeCursor};

use crate::model::types::EntityKind;

use super::languages::{self, Language};

#[derive(Debug, Clone)]
pub struct Definition {
    pub kind: EntityKind,
    pub qualified_name: String,
    pub parent: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub module_path: String,
    pub imported_names: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FileScan {
    pub path: PathBuf,
    pub definitions: Vec<Definition>,
    pub imports: Vec<Import>,
}
#[derive(Debug, Default)]
pub struct ScanResult {
    pub files_scanned: usize,
    pub files_by_language: HashMap<String, usize>,
    pub definitions: Vec<Definition>,
    pub imports: Vec<Import>,
    pub file_scans: Vec<FileScan>,
}

#[derive(Debug, Default)]
pub struct ScanConfig {
    pub root: PathBuf,
    pub max_depth: Option<usize>,
    pub languages: Option<Vec<Language>>,
}

pub struct Scanner;

impl Scanner {
    pub fn scan(config: &ScanConfig) -> Result<ScanResult> {
        let mut result = ScanResult::default();
        let mut parser = Parser::new();
        walk_and_scan(&config.root, 0, config, &mut parser, &mut result)?;
        Ok(result)
    }
}

const SKIP_DIRS: &[&str] = &["target", "node_modules", "__pycache__", "build", "dist"];

fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || SKIP_DIRS.contains(&name)
}

fn walk_and_scan(
    dir: &Path,
    depth: usize,
    config: &ScanConfig,
    parser: &mut Parser,
    result: &mut ScanResult,
) -> Result<()> {
    if config.max_depth.is_some_and(|max| depth > max) {
        return Ok(());
    }

    let entries =
        std::fs::read_dir(dir).with_context(|| format!("reading dir {}", dir.display()))?;

    for entry in entries {
        let entry = entry?;
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if should_skip_dir(&name_str) {
                continue;
            }
            walk_and_scan(&entry.path(), depth + 1, config, parser, result)?;
        } else if file_type.is_file() {
            let path = entry.path();
            let Some(lang) = languages::language_for_path(&path) else {
                continue;
            };
            if let Some(ref allowed) = config.languages
                && !allowed.contains(&lang)
            {
                continue;
            }

            let scan = scan_file(&path, lang, &config.root, parser)?;
            *result
                .files_by_language
                .entry(lang.to_string().to_owned())
                .or_insert(0) += 1;
            result.files_scanned += 1;
            result.definitions.extend(scan.definitions.clone());
            result.imports.extend(scan.imports.clone());
            result.file_scans.push(scan);
        }
    }

    Ok(())
}

fn scan_file(path: &Path, lang: Language, root: &Path, parser: &mut Parser) -> Result<FileScan> {
    let source =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;

    let ts_lang = ts_language(lang);
    parser
        .set_language(&ts_lang)
        .context("setting parser language")?;

    let tree = parser.parse(&source, None).context("parse failed")?;

    let module_qname = qualified_name_from_path(path, root, lang);

    let root_node = tree.root_node();
    let mut cursor = root_node.walk();

    let (definitions, imports) = extract(&root_node, &source, lang, &module_qname, &mut cursor);

    Ok(FileScan {
        path: path.to_path_buf(),
        definitions,
        imports,
    })
}

fn ts_language(lang: Language) -> TsLanguage {
    match lang {
        Language::Python => TsLanguage::from(tree_sitter_python::LANGUAGE),
        Language::Rust => TsLanguage::from(tree_sitter_rust::LANGUAGE),
        Language::JavaScript => TsLanguage::from(tree_sitter_javascript::LANGUAGE),
        Language::C => TsLanguage::from(tree_sitter_c::LANGUAGE),
        Language::Go => TsLanguage::from(tree_sitter_go::LANGUAGE),
        Language::Java => TsLanguage::from(tree_sitter_java::LANGUAGE),
    }
}

/// Convert a file path to a module qualified name.
/// `src/auth/login.rs` relative to root → `auth::login`
/// For Python, strip `.py` extension; for others strip any extension.
fn qualified_name_from_path(path: &Path, root: &Path, lang: Language) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let rel_str = rel.to_string_lossy();

    // Strip common source-directory prefixes for all languages
    let stripped = rel_str
        .strip_prefix("src/")
        .or_else(|| rel_str.strip_prefix("lib/"))
        .or_else(|| rel_str.strip_prefix("pkg/"))
        .unwrap_or(&rel_str);

    let without_ext = if lang == Language::Python {
        stripped.strip_suffix(".py").unwrap_or(stripped).to_owned()
    } else if let Some(slash_pos) = stripped.rfind('/') {
        let dir = &stripped[..=slash_pos];
        let file_part = &stripped[slash_pos + 1..];
        let stem = file_part
            .rsplit_once('.')
            .map(|(s, _)| s)
            .unwrap_or(file_part);
        format!("{dir}{stem}")
    } else {
        stripped
            .rsplit_once('.')
            .map(|(s, _)| s.to_owned())
            .unwrap_or_else(|| stripped.to_owned())
    };

    without_ext.replace(['/', '\\'], "::")
}

pub(crate) fn node_text(node: &Node, source: &str) -> String {
    node.utf8_text(source.as_bytes()).unwrap_or("").to_owned()
}
fn extract<'a>(
    root: &Node<'a>,
    source: &str,
    lang: Language,
    module_qname: &str,
    cursor: &mut TreeCursor<'a>,
) -> (Vec<Definition>, Vec<Import>) {
    match lang {
        Language::Python => python::extract_python(root, source, module_qname, cursor),
        Language::Rust => rust::extract_rust(root, source, module_qname, cursor),
        Language::JavaScript => javascript::extract_js(root, source, module_qname, cursor),
        Language::C => c::extract_c(root, source, module_qname, cursor),
        Language::Go => go::extract_go(root, source, module_qname, cursor),
        Language::Java => java::extract_java(root, source, module_qname, cursor),
    }
}

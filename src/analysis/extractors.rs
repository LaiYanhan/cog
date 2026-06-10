mod c;
mod go;
mod java;
mod javascript;
mod python;
mod rust;

use std::path::PathBuf;

use tree_sitter::{Language as TsLanguage, Node, TreeCursor};

use crate::domain::EntityKind;

use super::languages::Language;

pub use c::extract_c;
pub use go::extract_go;
pub use java::extract_java;
pub use javascript::extract_js;
pub use python::extract_python;
pub use rust::extract_rust;

// ── Shared types ──────────────────────────────────────────────────────────

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

/// A function/method call extracted from source code.
#[derive(Debug, Clone)]
pub struct Call {
    /// The callee name as it appears at the call site (simple name —
    /// `decode`, `format_code`, `__init__`). Not a qualified name.
    pub callee_name: String,
    /// The qualified name of the enclosing function/method (the caller).
    pub caller_qname: String,
}

#[derive(Debug, Clone)]
pub struct FileScan {
    pub path: PathBuf,
    pub definitions: Vec<Definition>,
    pub imports: Vec<Import>,
    pub calls: Vec<Call>,
}

// ── Language utilities ────────────────────────────────────────────────────

pub(crate) fn ts_language(lang: Language) -> TsLanguage {
    match lang {
        Language::Python => TsLanguage::from(tree_sitter_python::LANGUAGE),
        Language::Rust => TsLanguage::from(tree_sitter_rust::LANGUAGE),
        Language::JavaScript => TsLanguage::from(tree_sitter_javascript::LANGUAGE),
        Language::C => TsLanguage::from(tree_sitter_c::LANGUAGE),
        Language::Go => TsLanguage::from(tree_sitter_go::LANGUAGE),
        Language::Java => TsLanguage::from(tree_sitter_java::LANGUAGE),
    }
}

pub(crate) fn node_text(node: &Node, source: &str) -> String {
    node.utf8_text(source.as_bytes()).unwrap_or("").to_owned()
}

// ── Call extraction helpers (shared across language extractors) ─────────────

type CallExtractor = fn(&Node, &str) -> Option<String>;

/// Walk a function/method body looking for the block node, then delegate to
/// `walk_for_calls` for recursive call extraction.
pub(crate) fn extract_calls_from_body(
    func_node: &Node,
    source: &str,
    caller_qname: &str,
    calls: &mut Vec<Call>,
    block_kinds: &[&str],
    extract: CallExtractor,
) {
    let mut cursor = func_node.walk();
    for child in func_node.children(&mut cursor) {
        if block_kinds.contains(&child.kind()) {
            walk_for_calls(&child, source, caller_qname, calls, extract);
            break;
        }
    }
}

/// Recursively walk an AST subtree.  At each node, if `extract` returns a
/// callee name, record the call and skip deeper recursion into this subtree.
pub(crate) fn walk_for_calls(
    node: &Node,
    source: &str,
    caller_qname: &str,
    calls: &mut Vec<Call>,
    extract: CallExtractor,
) {
    if let Some(callee) = extract(node, source)
        && !callee.is_empty() {
            calls.push(Call {
                callee_name: callee,
                caller_qname: caller_qname.to_string(),
            });
        }
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        walk_for_calls(&child, source, caller_qname, calls, extract);
    }
}

// ── Extraction dispatch ───────────────────────────────────────────────────

pub fn extract<'a>(
    root: &Node<'a>,
    source: &str,
    lang: Language,
    module_qname: &str,
    cursor: &mut TreeCursor<'a>,
) -> (Vec<Definition>, Vec<Import>, Vec<Call>) {
    match lang {
        Language::Python => extract_python(root, source, module_qname, cursor),
        Language::Rust => extract_rust(root, source, module_qname, cursor),
        Language::JavaScript => extract_js(root, source, module_qname, cursor),
        Language::C => extract_c(root, source, module_qname, cursor),
        Language::Go => extract_go(root, source, module_qname, cursor),
        Language::Java => extract_java(root, source, module_qname, cursor),
    }
}

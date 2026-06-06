use std::collections::HashMap;

use crate::domain::Entity;

use super::extractors::{Definition, FileScan, Import};
use super::languages::Language;

/// Report produced by a scan of the code space.
///
/// Pure data — callers (e.g., the `init` command) are responsible for
/// writing entities and relations into the Repository.
#[derive(Debug, Default)]
pub struct ScanReport {
    /// Number of source files successfully parsed.
    pub files_scanned: usize,
    /// File counts keyed by language name.
    pub files_by_language: HashMap<String, usize>,
    /// Raw definitions extracted from source code.
    pub definitions: Vec<Definition>,
    /// Raw imports found in source code.
    pub imports: Vec<Import>,
    /// Per-file scan detail (path, definitions, imports).
    pub file_scans: Vec<FileScan>,
    /// Entities generated from scanned definitions (with UUIDs, Scan origin).
    pub entities: Vec<Entity>,
    /// Languages detected during the scan.
    pub languages_detected: Vec<Language>,
}

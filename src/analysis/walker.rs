use std::path::Path;

use anyhow::Result;

use ignore::WalkBuilder;

use super::languages;
use super::pool::ParserPool;
use super::report::ScanReport;
use super::scanner::{ScanConfig, scan_file};

/// Recursively walk a directory tree, scanning every source file found.
///
/// Uses `ignore::WalkBuilder` (ripgrep's walker) to respect `.gitignore`,
/// `.git/info/exclude`, global gitignore, and hidden-file conventions.
pub fn walk_and_scan(
    dir: &Path,
    config: &ScanConfig,
    pool: &mut ParserPool,
    result: &mut ScanReport,
) -> Result<()> {
    let mut builder = WalkBuilder::new(dir);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true);

    for entry in builder.build() {
        let entry = entry?;

        // Skip directories — WalkBuilder handles pruning internally.
        if !entry.file_type().is_some_and(|ft| ft.is_file()) {
            continue;
        }

        let path = entry.path();
        let Some(lang) = languages::language_for_path(path) else {
            continue;
        };
        if let Some(ref allowed) = config.languages
            && !allowed.contains(&lang)
        {
            continue;
        }

        let scan = scan_file(path, lang, &config.root, pool)?;
        *result
            .files_by_language
            .entry(lang.as_str().to_owned())
            .or_insert(0) += 1;
        result.files_scanned += 1;
        result.definitions.extend(scan.definitions.clone());
        result.imports.extend(scan.imports.clone());
        result.file_scans.push(scan);
    }

    Ok(())
}

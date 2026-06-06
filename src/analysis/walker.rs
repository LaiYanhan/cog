use std::path::Path;

use anyhow::{Context, Result};

use super::languages;
use super::pool::ParserPool;
use super::report::ScanReport;
use super::scanner::{ScanConfig, scan_file};

const SKIP_DIRS: &[&str] = &["target", "node_modules", "__pycache__", "build", "dist"];

fn should_skip_dir(name: &str) -> bool {
    name.starts_with('.') || SKIP_DIRS.contains(&name)
}

/// Recursively walk a directory tree, scanning every source file found.
pub fn walk_and_scan(
    dir: &Path,
    depth: usize,
    config: &ScanConfig,
    pool: &mut ParserPool,
    result: &mut ScanReport,
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
            walk_and_scan(&entry.path(), depth + 1, config, pool, result)?;
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

            let scan = scan_file(&path, lang, &config.root, pool)?;
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

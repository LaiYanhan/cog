use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;

use crate::analysis::{Language, ScanConfig, Scanner};
use crate::command::CommandOutput;
use crate::domain::{
    ChangelogAction, EntityKind, EntityOrigin, EntityRelationKind, SyncReport, parent_qname,
};
use crate::format::{self, OutputFormat};
use crate::repo::Repository;

/// Strips common source-directory prefixes from a relative path, then converts
/// `a/b/c.rs` → `a::b::c` (without extension).
pub(crate) fn path_to_qualified(relative: &std::path::Path) -> String {
    let s = relative.to_string_lossy();

    // Strip common source prefixes: src/, lib/, pkg/
    let stripped = s
        .strip_prefix("src/")
        .or_else(|| s.strip_prefix("lib/"))
        .or_else(|| s.strip_prefix("pkg/"))
        .unwrap_or(&s);

    // Remove file extension
    let without_ext = stripped
        .rsplit_once('.')
        .map(|(base, _)| base)
        .unwrap_or(stripped);

    without_ext.replace('/', "::")
}

/// Increment a counter in a string-keyed map using the EntityKind's Display output.
fn inc_kind(counts: &mut HashMap<String, usize>, kind: EntityKind) {
    *counts.entry(kind.to_string()).or_default() += 1;
}

/// Look up a kind counter by EntityKind.
fn get_kind(counts: &HashMap<String, usize>, kind: EntityKind) -> usize {
    counts.get(kind.to_string().as_str()).copied().unwrap_or(0)
}

/// Sync the cognitive model with the codebase.
///
/// Derives scan root from the DB path (`<project>/.cog/cog.db` → `<project>`),
/// runs a full tree-sitter scan, then:
/// - Creates entities and structural relations (contains/calls/uses) for all
///   discovered code.  Idempotent — safe to re-run any time.
/// - Removes stale Scan-origin entities (code deleted) **unless** they have
///   assertions recorded against them.
/// - With `--dry-run`: reports what *would* change without writing.
pub fn execute(
    repo: &dyn Repository,
    db_path: &std::path::Path,
    dry_run: bool,
    languages: Option<Vec<String>>,
    output: OutputFormat,
) -> Result<CommandOutput> {
    // Derive scan root from DB path: <project_root>/.cog/cog.db → <project_root>
    let scan_root = db_path
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let lang_filters: Option<Vec<Language>> = languages.as_ref().map(|langs| {
        langs
            .iter()
            .filter_map(|l| l.parse().ok())
            .collect::<Vec<_>>()
    });

    let config = ScanConfig {
        root: scan_root.clone(),
        languages: lang_filters,
    };

    let result = Scanner::new().scan(&config)?;

    if result.files_scanned == 0 {
        return Ok(CommandOutput::with_exit_code(
            format!("No source files found at {}", scan_root.display()),
            1,
        ));
    }

    // ── Dry-run path: just summarise ──────────────────────────────────
    if dry_run {
        let mut entity_counts: HashMap<String, usize> = HashMap::new();
        for def in &result.definitions {
            *entity_counts.entry(def.kind.to_string()).or_default() += 1;
        }
        let report = SyncReport {
            files_scanned: result.files_scanned,
            files_by_language: result.files_by_language.clone(),
            entities_created: 0,
            entities_removed: 0,
            relations_created: 0,
            entity_counts_by_kind: entity_counts,
            stale_entities: vec![],
            stale_skipped: vec![],
            dry_run: true,
            has_drift: false,
            after_entities: 0,
            after_assertions: 0,
        };
        return Ok(CommandOutput::success(format::emit_report(&report, output)));
    }

    // ── Build entity hierarchy ────────────────────────────────────────
    let mut dir_entities: HashMap<String, String> = HashMap::new(); // qualified_name → entity_id
    let mut file_entities: HashMap<String, String> = HashMap::new(); // qualified_name → entity_id
    let mut def_entity_ids: HashMap<String, String> = HashMap::new(); // qualified_name → entity_id
    let mut contains_count: usize = 0;
    let mut uses_count: usize = 0;

    // Collect all directory segments using the same prefix-stripping as path_to_qualified.
    let mut all_dirs: Vec<String> = Vec::new();
    for file_scan in &result.file_scans {
        let rel = file_scan
            .path
            .strip_prefix(&scan_root)
            .unwrap_or(&file_scan.path);
        let parent_rel = rel.parent().unwrap_or(std::path::Path::new(""));
        let parent_qname = path_to_qualified(parent_rel);
        if parent_qname.is_empty() {
            continue;
        }
        let mut current = String::new();
        for segment in parent_qname.split("::") {
            if !current.is_empty() {
                current.push_str("::");
            }
            current.push_str(segment);
            all_dirs.push(current.clone());
        }
    }
    all_dirs.sort();
    all_dirs.dedup();

    // Create directory Module entities + contains hierarchy
    for dir_qname in &all_dirs {
        let entity = repo.upsert_entity(dir_qname, EntityKind::Module, EntityOrigin::Scan)?;
        dir_entities.insert(dir_qname.clone(), entity.id.clone());

        if let Some(parent) = parent_qname(dir_qname)
            && !parent.is_empty()
            && let Some(parent_id) = dir_entities.get(parent)
        {
            repo.add_entity_relation(parent_id, &entity.id, EntityRelationKind::Contains)?;
            contains_count += 1;
        }
    }

    // Create file Module entities + directory → file contains
    for file_scan in &result.file_scans {
        let rel = file_scan
            .path
            .strip_prefix(&scan_root)
            .unwrap_or(&file_scan.path);
        let file_qname = path_to_qualified(rel);

        let entity = repo.upsert_entity(&file_qname, EntityKind::Module, EntityOrigin::Scan)?;
        file_entities.insert(file_qname.clone(), entity.id.clone());

        if let Some(parent_rel) = rel.parent() {
            let parent_qname = path_to_qualified(parent_rel);
            if !parent_qname.is_empty()
                && let Some(parent_id) = dir_entities
                    .get(&parent_qname)
                    .or_else(|| file_entities.get(&parent_qname))
            {
                repo.add_entity_relation(parent_id, &entity.id, EntityRelationKind::Contains)?;
                contains_count += 1;
            }
        }
    }

    let mut kind_counts: HashMap<String, usize> = HashMap::new();
    let mut def_count: usize = 0;

    for def in &result.definitions {
        let entity = repo.upsert_entity(&def.qualified_name, def.kind, EntityOrigin::Scan)?;
        def_entity_ids.insert(def.qualified_name.clone(), entity.id.clone());
        def_count += 1;
        inc_kind(&mut kind_counts, def.kind);

        // Determine parent: explicit parent field, or fall back to containing file
        let parent_qname = def
            .parent
            .as_deref()
            .map(|p| {
                if p.contains("::") {
                    p.to_string()
                } else {
                    parent_qname(&def.qualified_name)
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| p.to_string())
                }
            })
            .or_else(|| {
                result
                    .file_scans
                    .iter()
                    .find(|fs| {
                        fs.definitions
                            .iter()
                            .any(|d| d.qualified_name == def.qualified_name)
                    })
                    .and_then(|fs| {
                        let rel = fs.path.strip_prefix(&scan_root).unwrap_or(&fs.path);
                        let file_qname = path_to_qualified(rel);
                        file_entities
                            .contains_key(&file_qname)
                            .then_some(file_qname)
                    })
            });

        if let Some(pqname) = &parent_qname
            && let Some(parent_id) = def_entity_ids
                .get(pqname)
                .or_else(|| file_entities.get(pqname))
        {
            repo.add_entity_relation(parent_id, &entity.id, EntityRelationKind::Contains)?;
            contains_count += 1;
        }
    }

    // Build index: module_path → containing file's qualified name
    let mut import_file_map: HashMap<String, String> = HashMap::new();
    for file_scan in &result.file_scans {
        let rel = file_scan
            .path
            .strip_prefix(&scan_root)
            .unwrap_or(&file_scan.path);
        let file_qname = path_to_qualified(rel);
        for imp in &file_scan.imports {
            import_file_map
                .entry(imp.module_path.clone())
                .or_insert_with(|| file_qname.clone());
        }
    }

    // Create uses relations for imports (only if target entity exists)
    for import in &result.imports {
        let file_qname = match import_file_map.get(&import.module_path) {
            Some(fq) => fq.clone(),
            None => continue,
        };
        let from_id = match file_entities.get(&file_qname) {
            Some(id) => id.clone(),
            None => continue,
        };

        if let Some(target_id) = def_entity_ids.get(&import.module_path) {
            repo.add_entity_relation(&from_id, target_id, EntityRelationKind::Uses)?;
            uses_count += 1;
        }

        for name in &import.imported_names {
            let qualified = format!("{}::{}", import.module_path, name);
            if let Some(target_id) = def_entity_ids.get(&qualified) {
                repo.add_entity_relation(&from_id, target_id, EntityRelationKind::Uses)?;
                uses_count += 1;
            }
        }
    }

    // ── Drift cleanup: remove Scan-origin entities no longer in code ──
    // Build the set of all names that exist in the current scan.
    let mut scanned_names: HashSet<String> = result
        .definitions
        .iter()
        .map(|d| d.qualified_name.clone())
        .collect();

    // Also include module entities (directories + files) — these are created
    // by init/sync and must be in the scanned set for stale detection.
    for file_scan in &result.file_scans {
        let rel = file_scan
            .path
            .strip_prefix(&scan_root)
            .unwrap_or(&file_scan.path);
        let file_qname = path_to_qualified(rel);
        scanned_names.insert(file_qname.clone());

        // Walk up directory ancestors using the original relative path,
        // applying path_to_qualified to each directory (not the already-stripped
        // file_qname, which loses the src/ prefix for top-level files).
        let mut dir_ancestor = rel.parent();
        while let Some(dir_path) = dir_ancestor {
            let dir_qname = path_to_qualified(dir_path);
            if dir_qname.is_empty() {
                break;
            }
            scanned_names.insert(dir_qname);
            dir_ancestor = dir_path.parent();
        }
    }

    let auto_scanned_names = repo.get_scanned_entity_names()?;

    let mut stale_names: Vec<String> = auto_scanned_names
        .iter()
        .filter(|name| !scanned_names.contains(*name))
        .cloned()
        .collect();
    stale_names.sort();

    let mut removed: Vec<String> = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    for name in &stale_names {
        // Safety check: don't auto-delete entities with assertions.
        // The agent may have recorded knowledge claims against them.
        if let Ok(Some(entity)) = repo.get_entity_by_name(name) {
            let assertions = repo
                .get_assertions_for_entity(&entity.id)
                .unwrap_or_default();
            if !assertions.is_empty() {
                skipped.push(name.clone());
                continue;
            }
        }
        if repo.delete_entity(name)? {
            removed.push(name.clone());
        }
    }

    let entities_created = dir_entities.len() + file_entities.len() + def_count;
    let has_drift =
        !removed.is_empty() || (!stale_names.is_empty() && stale_names.len() != removed.len());

    // ── Changelog ──────────────────────────────────────────────────────
    repo.append_changelog(
        ChangelogAction::Sync,
        "*",
        &format!(
            "created={} removed={} relations={}",
            entities_created,
            removed.len(),
            contains_count + uses_count,
        ),
    )?;

    // ── Compute fan_in/fan_out metrics ───────────────────────────────
    {
        let all_entities = repo.list_entities()?;
        let relations = repo.list_entity_relations()?;

        let mut fan_counts: HashMap<&str, (u32, u32)> = HashMap::new();
        for entity in &all_entities {
            fan_counts.entry(&entity.id).or_insert((0, 0));
        }
        for rel in &relations {
            if let Some(from) = fan_counts.get_mut(rel.from_entity.as_str()) {
                from.1 += 1;
            }
            if let Some(to) = fan_counts.get_mut(rel.to_entity.as_str()) {
                to.0 += 1;
            }
        }

        for entity in &all_entities {
            if let Some(&(fan_in, fan_out)) = fan_counts.get(entity.id.as_str())
                && (fan_in > 0 || fan_out > 0)
            {
                let mut metrics = entity.metrics.clone();
                metrics.fan_in = Some(fan_in);
                metrics.fan_out = Some(fan_out);
                let _ = repo.update_entity_metrics(&entity.id, &metrics);
            }
        }
    }

    // ── Build report ──────────────────────────────────────────────────
    let mut entity_counts: HashMap<String, usize> = HashMap::new();
    let module_count =
        get_kind(&kind_counts, EntityKind::Module) + dir_entities.len() + file_entities.len();
    if module_count > 0 {
        entity_counts.insert("module".to_string(), module_count);
    }
    for kind in &[
        EntityKind::Type,
        EntityKind::Function,
        EntityKind::Method,
        EntityKind::Field,
    ] {
        let count = get_kind(&kind_counts, *kind);
        if count > 0 {
            entity_counts.insert(kind.to_string(), count);
        }
    }

    let after_entities = repo.list_entities()?.len();
    let after_assertions = repo.list_assertions()?.len();

    let report = SyncReport {
        files_scanned: result.files_scanned,
        files_by_language: result.files_by_language.clone(),
        entities_created,
        entities_removed: removed.len(),
        relations_created: contains_count + uses_count,
        entity_counts_by_kind: entity_counts,
        stale_entities: removed,
        stale_skipped: skipped,
        dry_run: false,
        has_drift,
        after_entities,
        after_assertions,
    };
    let mut out = CommandOutput::success(format::emit_report(&report, output));
    out.has_drift = has_drift;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use std::fs;

    use super::execute;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin};
    use crate::format::OutputFormat;
    use crate::repo::{Repository, SqliteRepository};
    use tempfile::tempdir;

    fn make_test_project(root: &std::path::Path) -> Result<()> {
        let src = root.join("src");
        fs::create_dir_all(&src)?;
        fs::write(src.join("main.rs"), "fn hello() {}\n")?;
        Ok(())
    }

    #[test]
    fn dry_run_reports_without_writing() -> Result<()> {
        let tmp = tempdir()?;
        make_test_project(tmp.path())?;
        let store = SqliteRepository::open_in_memory()?;
        // db_path must look like <project>/.cog/cog.db so scan_root = <project>
        let db_path = tmp.path().join(".cog").join("cog.db");
        let output = execute(
            &store,
            &db_path,
            true, // dry_run
            None,
            OutputFormat::Text,
        )?;
        assert_eq!(output.exit_code, 0);
        let entities = store.list_entities()?;
        assert!(entities.is_empty());
        Ok(())
    }

    #[test]
    fn idempotent_rerun_does_not_duplicate_relations() -> Result<()> {
        let tmp = tempdir()?;
        make_test_project(tmp.path())?;
        let store = SqliteRepository::open_in_memory()?;
        let db_path = tmp.path().join(".cog").join("cog.db");
        // First run
        let out1 = execute(&store, &db_path, false, None, OutputFormat::Text)?;
        assert_eq!(out1.exit_code, 0);
        let relations_after_first = store.list_entity_relations()?.len();

        // Second run (idempotent)
        let out2 = execute(&store, &db_path, false, None, OutputFormat::Text)?;
        assert_eq!(out2.exit_code, 0);
        let relations_after_second = store.list_entity_relations()?.len();

        assert_eq!(
            relations_after_first, relations_after_second,
            "idempotent re-run must not duplicate relations"
        );
        Ok(())
    }

    #[test]
    fn stale_entity_with_assertions_is_not_auto_deleted() -> Result<()> {
        let tmp = tempdir()?;
        make_test_project(tmp.path())?;
        let store = SqliteRepository::open_in_memory()?;
        // Create a Scan-origin entity manually (simulating old scan result)
        let entity =
            store.upsert_entity("old::vanished_fn", EntityKind::Function, EntityOrigin::Scan)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "does something",
            "code:old::vanished_fn",
            None,
        )?;

        // Run sync — this entity doesn't exist in the codebase, but has assertions
        let output = execute(
            &store,
            &tmp.path().join(".cog").join("cog.db"),
            false,
            None,
            OutputFormat::Text,
        )?;
        assert_eq!(output.exit_code, 0);

        // Entity should still exist (skipped due to assertions)
        assert!(store.get_entity_by_name("old::vanished_fn")?.is_some());
        Ok(())
    }

    #[test]
    fn stale_entity_without_assertions_is_removed() -> Result<()> {
        let tmp = tempdir()?;
        make_test_project(tmp.path())?;
        let store = SqliteRepository::open_in_memory()?;
        store.upsert_entity("old::gone_fn", EntityKind::Function, EntityOrigin::Scan)?;

        execute(
            &store,
            &tmp.path().join(".cog").join("cog.db"),
            false,
            None,
            OutputFormat::Text,
        )?;

        // Entity should be removed (stale, no assertions)
        assert!(store.get_entity_by_name("old::gone_fn")?.is_none());
        Ok(())
    }
}

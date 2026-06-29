use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use anyhow::Result;

use crate::analysis::extractors::{Definition, FileScan, Import};
use crate::analysis::report::ScanReport;
use crate::analysis::{Language, ScanConfig, Scanner};
use crate::command::CommandOutput;
use crate::domain::{
    Assertion, AssertionStatus, ChangelogAction, EntityKind, EntityOrigin, EntityRelationKind,
    SyncReport, ancestors, last_segment, parent_qname, path_to_qualified,
};
use crate::format::{self, OutputFormat};
use crate::repo::Repository;

// SyncContext: shared mutable state for the entity-building phases

/// Accumulator for entity IDs and relation counts produced during a sync.
struct SyncContext {
    dir_entities: HashMap<String, String>, // qualified_name → entity_id
    file_entities: HashMap<String, String>, // qualified_name → entity_id
    def_entity_ids: HashMap<String, String>, // qualified_name → entity_id
    contains_count: usize,
    uses_count: usize,
    calls_count: usize,
    kind_counts: HashMap<String, usize>,
    def_count: usize,
}

impl SyncContext {
    fn new() -> Self {
        Self {
            dir_entities: HashMap::new(),
            file_entities: HashMap::new(),
            def_entity_ids: HashMap::new(),
            contains_count: 0,
            uses_count: 0,
            calls_count: 0,
            kind_counts: HashMap::new(),
            def_count: 0,
        }
    }

    fn get_kind(&self, kind: EntityKind) -> usize {
        self.kind_counts
            .get(kind.to_string().as_str())
            .copied()
            .unwrap_or(0)
    }

    fn relations_created(&self) -> usize {
        self.contains_count + self.uses_count + self.calls_count
    }
    /// Create directory Module entities and their contains hierarchy.
    fn create_dir_entities(
        &mut self,
        repo: &dyn Repository,
        file_scans: &[FileScan],
        scan_root: &std::path::Path,
    ) -> Result<()> {
        // Collect all directory ancestors using the same prefix-stripping as path_to_qualified.
        let mut all_dirs: Vec<String> = Vec::new();
        for file_scan in file_scans {
            let rel = file_scan
                .path
                .strip_prefix(scan_root)
                .unwrap_or(&file_scan.path);
            let parent_rel = rel.parent().unwrap_or(std::path::Path::new(""));
            let parent_qname = path_to_qualified(parent_rel);
            all_dirs.extend(ancestors(&parent_qname));
            if !parent_qname.is_empty() {
                all_dirs.push(parent_qname);
            }
        }
        all_dirs.sort();
        all_dirs.dedup();

        for dir_qname in &all_dirs {
            let entity_id = upsert_and_track(self, repo, dir_qname, EntityKind::Module)?;
            self.dir_entities
                .insert(dir_qname.clone(), entity_id.clone());

            if let Some(parent) = parent_qname(dir_qname)
                && !parent.is_empty()
                && let Some(parent_id) = self.dir_entities.get(parent)
            {
                repo.add_entity_relation(parent_id, &entity_id, EntityRelationKind::Contains)?;
                self.contains_count += 1;
            }
        }
        Ok(())
    }

    /// Create file Module entities and directory → file contains relations.
    fn create_file_entities(
        &mut self,
        repo: &dyn Repository,
        file_scans: &[FileScan],
        scan_root: &std::path::Path,
    ) -> Result<()> {
        for file_scan in file_scans {
            let rel = file_scan
                .path
                .strip_prefix(scan_root)
                .unwrap_or(&file_scan.path);
            let file_qname = path_to_qualified(rel);

            let entity_id = upsert_and_track(self, repo, &file_qname, EntityKind::Module)?;
            self.file_entities
                .insert(file_qname.clone(), entity_id.clone());

            if let Some(parent_rel) = rel.parent() {
                let pqname = path_to_qualified(parent_rel);
                if !pqname.is_empty()
                    && let Some(parent_id) = self
                        .dir_entities
                        .get(&pqname)
                        .or_else(|| self.file_entities.get(&pqname))
                {
                    repo.add_entity_relation(parent_id, &entity_id, EntityRelationKind::Contains)?;
                    self.contains_count += 1;
                }
            }
        }
        Ok(())
    }

    /// Create definition entities and their contains relations to parents.
    fn create_definition_entities(
        &mut self,
        repo: &dyn Repository,
        definitions: &[Definition],
        file_scans: &[FileScan],
        scan_root: &std::path::Path,
    ) -> Result<()> {
        for def in definitions {
            let entity_id = upsert_and_track(self, repo, &def.qualified_name, def.kind)?;
            self.def_entity_ids
                .insert(def.qualified_name.clone(), entity_id.clone());
            self.def_count += 1;

            // Determine parent: explicit parent field, or fall back to containing file
            let pqname = def
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
                    file_scans
                        .iter()
                        .find(|fs| {
                            fs.definitions
                                .iter()
                                .any(|d| d.qualified_name == def.qualified_name)
                        })
                        .and_then(|fs| {
                            let rel = fs.path.strip_prefix(scan_root).unwrap_or(&fs.path);
                            let file_qname = path_to_qualified(rel);
                            self.file_entities
                                .contains_key(&file_qname)
                                .then_some(file_qname)
                        })
                });

            if let Some(pqname) = &pqname
                && let Some(parent_id) = self
                    .def_entity_ids
                    .get(pqname)
                    .or_else(|| self.file_entities.get(pqname))
            {
                repo.add_entity_relation(parent_id, &entity_id, EntityRelationKind::Contains)?;
                self.contains_count += 1;
            }
        }
        Ok(())
    }

    /// Create uses relations for imports (only if target entity exists).
    fn create_import_relations(
        &mut self,
        repo: &dyn Repository,
        imports: &[Import],
        file_scans: &[FileScan],
        scan_root: &std::path::Path,
    ) -> Result<()> {
        // Build index: module_path → containing file's qualified name
        let mut import_file_map: HashMap<String, String> = HashMap::new();
        for file_scan in file_scans {
            let rel = file_scan
                .path
                .strip_prefix(scan_root)
                .unwrap_or(&file_scan.path);
            let file_qname = path_to_qualified(rel);
            for imp in &file_scan.imports {
                import_file_map
                    .entry(imp.module_path.clone())
                    .or_insert_with(|| file_qname.clone());
            }
        }

        for import in imports {
            let file_qname = match import_file_map.get(&import.module_path) {
                Some(fq) => fq.clone(),
                None => continue,
            };
            let from_id = match self.file_entities.get(&file_qname) {
                Some(id) => id.clone(),
                None => continue,
            };

            if let Some(target_id) = self.def_entity_ids.get(&import.module_path) {
                repo.add_entity_relation(&from_id, target_id, EntityRelationKind::Uses)?;
                self.uses_count += 1;
            }

            for name in &import.imported_names {
                let qualified = format!("{}::{}", import.module_path, name);
                if let Some(target_id) = self.def_entity_ids.get(&qualified) {
                    repo.add_entity_relation(&from_id, target_id, EntityRelationKind::Uses)?;
                    self.uses_count += 1;
                }
            }
        }
        Ok(())
    }

    /// Create `Calls` relations from extracted call sites.
    ///
    /// Matches callee simple names to definition entities in the scan.  If a
    /// callee name appears in multiple entities the last one wins — this is
    /// imprecise but covers the common case of a single codebase naming
    /// convention.
    fn create_call_relations(
        &mut self,
        repo: &dyn Repository,
        file_scans: &[FileScan],
    ) -> Result<()> {
        // Build simple-name → entity_id index for callee resolution
        let mut name_index: HashMap<&str, &str> = HashMap::new();
        for (qname, id) in &self.def_entity_ids {
            name_index.insert(last_segment(qname), id);
        }
        let mut seen: HashSet<(String, String)> = HashSet::new();
        for file_scan in file_scans {
            for call in &file_scan.calls {
                // caller must be a known definition entity
                let from_id = match self.def_entity_ids.get(&call.caller_qname) {
                    Some(id) => id,
                    None => continue,
                };
                // callee is resolved by simple name
                let to_id = match name_index.get(call.callee_name.as_str()) {
                    Some(id) => (*id).to_string(),
                    None => continue,
                };
                // No self-edges
                if *from_id == to_id {
                    continue;
                }
                // Deduplicate across multiple call sites in the same function
                if !seen.insert(((*from_id).to_string(), to_id.clone())) {
                    continue;
                }
                repo.add_entity_relation(from_id, &to_id, EntityRelationKind::Calls)?;
                self.calls_count += 1;
            }
        }
        Ok(())
    }
}

// Standalone phase functions

/// Shared helper: upsert a Scan-origin entity and return its id.
fn upsert_and_track(
    _ctx: &SyncContext,
    repo: &dyn Repository,
    qname: &str,
    kind: EntityKind,
) -> Result<String> {
    let entity = repo.upsert_entity(qname, kind, EntityOrigin::Scan)?;
    Ok(entity.id)
}
/// Build the set of all qualified names that exist in the current scan,
/// including definitions, files, and directory ancestors.
///
/// Shared by `sync_cmd::execute` (drift cleanup) and `verify::check_scan_diff`
/// (stale/unmodeled detection).
pub(crate) fn collect_scanned_names(
    result: &ScanReport,
    scan_root: &std::path::Path,
) -> HashSet<String> {
    let mut names: HashSet<String> = result
        .definitions
        .iter()
        .map(|d| d.qualified_name.clone())
        .collect();

    for file_scan in &result.file_scans {
        let rel = file_scan
            .path
            .strip_prefix(scan_root)
            .unwrap_or(&file_scan.path);
        let file_qname = path_to_qualified(rel);
        names.insert(file_qname.clone());

        // Walk up directory ancestors using the original relative path,
        // applying path_to_qualified to each directory (not the already-stripped
        // file_qname, which loses the src/ prefix for top-level files).
        let mut dir_ancestor = rel.parent();
        while let Some(dir_path) = dir_ancestor {
            let dir_qname = path_to_qualified(dir_path);
            if dir_qname.is_empty() {
                break;
            }
            names.insert(dir_qname);
            dir_ancestor = dir_path.parent();
        }
    }

    names
}

/// Delete stale entities, protecting those that have assertions.
///
/// Returns `(deleted_names, protected_names)`.  Shared by `sync_cmd`
/// (drift cleanup) and `verify --scan --clean`.
pub(crate) fn delete_stale_protected(
    repo: &dyn Repository,
    names: &[String],
) -> Result<(Vec<String>, Vec<String>)> {
    let mut deleted = Vec::new();
    let mut protected = Vec::new();
    for name in names {
        // Safety check: don't auto-delete entities with assertions.
        // The agent may have recorded knowledge claims against them.
        let has_assertions = match repo.get_entity_by_name(name) {
            Ok(Some(entity)) => !repo
                .get_assertions_for_entity(&entity.id)
                .unwrap_or_default()
                .is_empty(),
            _ => false,
        };
        if has_assertions {
            protected.push(name.clone());
        } else if repo.delete_entity(name)? {
            deleted.push(name.clone());
        }
    }
    Ok((deleted, protected))
}

/// Remove Scan-origin entities no longer present in the codebase.
///
/// Returns `(removed, skipped)` where `skipped` are stale entities that
/// were retained because they have assertions recorded against them.
fn remove_stale_entities(
    repo: &dyn Repository,
    scanned_names: &HashSet<String>,
) -> Result<(Vec<String>, Vec<String>)> {
    let mut stale_names: Vec<String> = repo
        .get_scanned_entity_names()?
        .iter()
        .filter(|name| !scanned_names.contains(*name))
        .cloned()
        .collect();
    stale_names.sort();
    delete_stale_protected(repo, &stale_names)
}

/// Compute fan_in/fan_out metrics for all entities and persist them.
fn compute_fan_metrics(repo: &dyn Repository) -> Result<()> {
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

    Ok(())
}

/// Results of post-sync drift detection — produced by the drift-cleanup phase
/// and consumed together by [`build_report`].
struct DriftResult {
    removed: Vec<String>,
    skipped: Vec<String>,
    affected_assertions: Vec<(String, Assertion)>,
    unresolved_provisional: Vec<String>,
}

/// Assemble the final `SyncReport` from phase results.
fn build_report(
    result: &ScanReport,
    ctx: &SyncContext,
    drift: &DriftResult,
    before_entities: usize,
    after_entities: usize,
    after_assertions: usize,
) -> SyncReport {
    let has_drift =
        !drift.removed.is_empty() || !drift.skipped.is_empty() || after_entities != before_entities;

    let mut entity_counts: HashMap<String, usize> = HashMap::new();
    let module_count =
        ctx.get_kind(EntityKind::Module) + ctx.dir_entities.len() + ctx.file_entities.len();
    if module_count > 0 {
        entity_counts.insert("module".to_string(), module_count);
    }
    for kind in &[
        EntityKind::Type,
        EntityKind::Function,
        EntityKind::Method,
        EntityKind::Field,
    ] {
        let count = ctx.get_kind(*kind);
        if count > 0 {
            entity_counts.insert(kind.to_string(), count);
        }
    }

    SyncReport {
        files_scanned: result.files_scanned,
        files_by_language: result.files_by_language.clone(),
        // Genuinely-new entities, not upserts. after = before + created - removed,
        // so created = after - before + removed. Signed math: removed can exceed
        // created when only deletions happen.
        entities_created: (after_entities as i64 - before_entities as i64
            + drift.removed.len() as i64)
            .max(0) as usize,
        entities_removed: drift.removed.len(),
        relations_created: ctx.relations_created(),
        entity_counts_by_kind: entity_counts,
        stale_entities: drift.removed.clone(),
        stale_skipped: drift.skipped.clone(),
        dry_run: false,
        has_drift,
        after_entities,
        after_assertions,
        affected_assertions: drift.affected_assertions.clone(),
        unresolved_provisional: drift.unresolved_provisional.clone(),
    }
}

// Command entry point

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
        // Empty project — e.g. the design phase before any code exists. The model
        // is already open; treat empty as a valid initial state (exit 0) so the
        // from-scratch workflow can bootstrap via `cog assert ... --grounds plan:`,
        // instead of dead-locking the workflow on a sync that can never find files.
        let msg = if dry_run {
            format!(
                "No source files found at {} — nothing to scan yet.",
                scan_root.display()
            )
        } else {
            format!(
                "Model initialized at {}.\nNo source files found yet — start recording design with:\n  cog assert <entity> --kind <contract|intent|invariant|fragility|correction> --claim \"...\" --grounds \"plan:<doc>\"",
                db_path.display()
            )
        };
        return Ok(CommandOutput::success(msg));
    }

    // Dry-run path: just summarise
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
            affected_assertions: vec![],
            unresolved_provisional: vec![],
        };
        return Ok(CommandOutput::success(format::emit_report(&report, output)));
    }

    // Snapshot entity count before upserts to detect new entities
    let before_entities = repo.list_entities()?.len();
    // Create entities and structural relations
    let mut ctx = SyncContext::new();
    ctx.create_dir_entities(repo, &result.file_scans, &scan_root)?;
    ctx.create_file_entities(repo, &result.file_scans, &scan_root)?;
    ctx.create_definition_entities(repo, &result.definitions, &result.file_scans, &scan_root)?;
    ctx.create_import_relations(repo, &result.imports, &result.file_scans, &scan_root)?;
    ctx.create_call_relations(repo, &result.file_scans)?;

    // Drift cleanup
    let scanned_names = collect_scanned_names(&result, &scan_root);
    let (removed, skipped) = remove_stale_entities(repo, &scanned_names)?;
    // Snapshot the entity count now (stable from here on — fan/provisional steps
    // below don't add rows) to report genuinely-new entities, not upserts.
    let after_entities = repo.list_entities()?.len();
    let entities_created =
        (after_entities as i64 - before_entities as i64 + removed.len() as i64).max(0) as usize;

    // Collect assertions on stale-skipped entities (they have assertions that may need review)
    let mut affected_assertions: Vec<(String, Assertion)> = Vec::new();
    for name in &skipped {
        if let Ok(Some(entity)) = repo.get_entity_by_name(name)
            && let Ok(assertions) = repo.get_assertions_for_entity(&entity.id)
        {
            for a in assertions {
                if a.status == AssertionStatus::Active {
                    affected_assertions.push((name.clone(), a));
                }
            }
        }
    }

    // Changelog
    repo.append_changelog(
        ChangelogAction::Sync,
        "*",
        &format!(
            "created={} removed={} relations={}",
            entities_created,
            removed.len(),
            ctx.relations_created(),
        ),
    )?;

    // Compute fan_in/fan_out metrics
    compute_fan_metrics(repo)?;

    // Detect unresolved provisional entities: Experiment-origin entities not
    // found in the codebase (agent committed experiment but didn't implement).
    let unresolved_provisional: Vec<String> = repo
        .get_experiment_entity_names()?
        .into_iter()
        .filter(|name| !scanned_names.contains(name))
        .collect();

    let drift = DriftResult {
        removed,
        skipped,
        affected_assertions,
        unresolved_provisional,
    };

    // Build report
    let after_assertions = repo.list_assertions()?.len();
    let report = build_report(
        &result,
        &ctx,
        &drift,
        before_entities,
        after_entities,
        after_assertions,
    );

    let mut out = CommandOutput::success(format::emit_report(&report, output));
    out.has_drift = report.has_drift;
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

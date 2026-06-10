use std::collections::HashSet;
use std::path::Path;

use anyhow::Result;

use crate::analysis::{ScanConfig, Scanner};
use crate::command::CommandOutput;
use crate::domain::{
    AssertionStatus, ChangelogAction, VerificationIssue, VerificationIssueKind, VerificationReport,
};
use crate::format::{self, OutputFormat};
use crate::repo::Repository;

pub fn execute(
    repo: &dyn Repository,
    scope: Option<&str>,
    clean: bool,
    scan_path: Option<&Path>,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let all_entities = repo.list_entities()?;
    let scope_prefix = scope.unwrap_or_default();

    // Pre-compute full model name set for unmodeled detection (avoids re-querying)
    let all_model_names: HashSet<&str> = all_entities
        .iter()
        .map(|e| e.qualified_name.as_str())
        .collect();

    // Filter entities by scope for structural checks
    let entities: Vec<_> = all_entities
        .iter()
        .filter(|e| e.qualified_name.starts_with(scope_prefix))
        .collect();
    let checked_count = entities.len();

    let (issues, cleaned) =
        check_entity_structural_issues(repo, &entities, &all_model_names, clean)?;

    let (stale_names, unmodeled_count, scan_issues) =
        check_scan_diff(repo, scan_path, scope_prefix, &all_model_names, clean)?;

    repo.append_changelog(
        ChangelogAction::Verify,
        scope.unwrap_or("*"),
        &format!("issues={}", issues.len()),
    )?;

    let stale_count = stale_names.len();
    let has_scan_diff = unmodeled_count > 0 || stale_count > 0;
    let scan_cleaned = clean && stale_count > 0;

    let success = issues.is_empty()
        || (issues
            .iter()
            .all(|i| i.kind == VerificationIssueKind::IsolatedEntity)
            && cleaned > 0);

    // Success condition for scan results:
    // - No scan diff at all → ok
    // - Only unmodeled (advisory) but no stale → still ok
    // - Any stale entity → failure
    let overall_ok = success && (!has_scan_diff || (unmodeled_count > 0 && stale_count == 0))
        || (success && scan_cleaned);

    let total_cleaned = cleaned + if clean { stale_count } else { 0 };
    let report = VerificationReport {
        checked_count,
        issues,
        cleaned_count: total_cleaned,
        scan_issues,
        success: overall_ok,
    };

    let exit_code: i32 = if overall_ok { 0 } else { 1 };
    Ok(CommandOutput::with_exit_code(
        format::emit_report(&report, output),
        exit_code,
    ))
}

fn check_entity_structural_issues(
    repo: &dyn Repository,
    entities: &[&crate::domain::Entity],
    all_model_names: &HashSet<&str>,
    clean: bool,
) -> Result<(Vec<VerificationIssue>, usize)> {
    let mut issues = Vec::new();
    let mut cleaned: usize = 0;

    for entity in entities {
        let assertions = repo.get_assertions_for_entity(&entity.id)?;
        let relation_count = repo.count_relations_for_entity(&entity.id)?;

        let active_count = assertions
            .iter()
            .filter(|a| a.status == AssertionStatus::Active)
            .count();
        if active_count == 0 && relation_count == 0 {
            issues.push(VerificationIssue {
                kind: VerificationIssueKind::IsolatedEntity,
                entity_name: Some(entity.qualified_name.clone()),
                assertion_id: None,
                detail: "entity has no assertions and no relations".to_string(),
            });
            if clean {
                repo.delete_entity(&entity.qualified_name)?;
                cleaned += 1;
            }
        }

        // Detect orphan Manual-origin entities: they have assertions
        // (so IsolatedEntity doesn't catch them) but no relations.
        if entity.origin == crate::domain::EntityOrigin::Manual
            && relation_count == 0
            && active_count > 0
        {
            issues.push(VerificationIssue {
                kind: VerificationIssueKind::OrphanManualEntity,
                entity_name: Some(entity.qualified_name.clone()),
                assertion_id: None,
                detail: format!(
                    "Manual entity with {} active assertion(s) but no relations",
                    active_count
                ),
            });
        }
        for assertion in &assertions {
            if assertion.status != AssertionStatus::Active {
                continue;
            }

            let evidence = repo.get_evidence_for_assertion(&assertion.id)?;
            if evidence.is_empty() {
                issues.push(VerificationIssue {
                    kind: VerificationIssueKind::MissingEvidence,
                    entity_name: Some(entity.qualified_name.clone()),
                    assertion_id: Some(assertion.id.clone()),
                    detail: "active assertion has no evidence".to_string(),
                });
            }

            for ev in &evidence {
                if ev.source == "code" && !all_model_names.contains(ev.detail.as_str()) {
                    issues.push(VerificationIssue {
                        kind: VerificationIssueKind::DanglingGrounds,
                        entity_name: Some(entity.qualified_name.clone()),
                        assertion_id: Some(assertion.id.clone()),
                        detail: format!(
                            "grounds \"code:{}\" references entity not in model",
                            ev.detail
                        ),
                    });
                }
            }

            for dependency in repo.get_dependencies(&assertion.id)? {
                if dependency.status == AssertionStatus::Retracted {
                    issues.push(VerificationIssue {
                        kind: VerificationIssueKind::DependencyOnRetracted,
                        entity_name: Some(entity.qualified_name.clone()),
                        assertion_id: Some(assertion.id.clone()),
                        detail: format!(
                            "depends on retracted assertion {}",
                            crate::domain::short_id(&dependency.id)
                        ),
                    });
                } else if dependency.status == AssertionStatus::Uncertain {
                    issues.push(VerificationIssue {
                        kind: VerificationIssueKind::DependencyOnUncertain,
                        entity_name: Some(entity.qualified_name.clone()),
                        assertion_id: Some(assertion.id.clone()),
                        detail: format!(
                            "depends on uncertain assertion {}",
                            crate::domain::short_id(&dependency.id)
                        ),
                    });
                }
            }
        }
    }

    Ok((issues, cleaned))
}

fn check_scan_diff(
    repo: &dyn Repository,
    scan_path: Option<&Path>,
    scope_prefix: &str,
    all_model_names: &HashSet<&str>,
    clean: bool,
) -> Result<(Vec<String>, usize, Vec<String>)> {
    let mut unmodeled_count: usize = 0;
    let mut stale_names: Vec<String> = Vec::new();

    if let Some(path) = scan_path {
        let config = ScanConfig {
            root: path.to_path_buf(),
            ..Default::default()
        };
        let scan_result = Scanner::new().scan(&config)?;

        // Build the full set of scanned entity names: definitions + file/directory modules.
        // This mirrors what sync_cmd creates, so stale detection works for both
        // function/type entities and structural module entities.
        let path_to_qualified = crate::command::sync_cmd::path_to_qualified;
        let mut scanned_names: HashSet<String> = scan_result
            .definitions
            .iter()
            .map(|d| d.qualified_name.clone())
            .collect();
        for file_scan in &scan_result.file_scans {
            let rel = file_scan.path.strip_prefix(path).unwrap_or(&file_scan.path);
            // File module
            let file_qname = path_to_qualified(rel);
            if !file_qname.is_empty() {
                scanned_names.insert(file_qname.clone());
            }
            // Directory modules: split parent path into cumulative :: segments
            if let Some(parent) = rel.parent() {
                let parent_qname = path_to_qualified(parent);
                let mut current = String::new();
                for segment in parent_qname.split("::") {
                    if !current.is_empty() {
                        current.push_str("::");
                    }
                    current.push_str(segment);
                    scanned_names.insert(current.clone());
                }
            }
        }

        // Collect qualified names of auto-scanned entities (single query, no N+1)
        let auto_scanned_names = repo.get_scanned_entity_names()?;

        // Stale: auto-scanned entities not found in current scan
        for name in &auto_scanned_names {
            if !scanned_names.contains(name) {
                stale_names.push(name.clone());
            }
        }

        // Unmodeled: scanned definitions not in the model.
        // When scoped, restrict scanned_names to the scope prefix so that entities
        // outside the scope (which exist in the model but aren't in the filtered set)
        // are not falsely reported as unmodeled.
        unmodeled_count = scanned_names
            .iter()
            .filter(|name| {
                let in_scope = scope_prefix.is_empty() || name.starts_with(scope_prefix);
                in_scope && !all_model_names.contains(name.as_str())
            })
            .count();

        // Clean stale entities if requested
        if clean {
            for name in &stale_names {
                repo.delete_entity(name)?;
            }
        }
    }

    let scan_issues = build_scan_diff_messages(unmodeled_count, &stale_names, clean);

    Ok((stale_names, unmodeled_count, scan_issues))
}

fn build_scan_diff_messages(
    unmodeled_count: usize,
    stale_names: &[String],
    clean: bool,
) -> Vec<String> {
    let stale_count = stale_names.len();
    let mut scan_issues = Vec::new();
    if unmodeled_count > 0 {
        scan_issues.push(format!(
            "Scan diff: {} unmodeled definition(s) not in model (run cog init to add)",
            unmodeled_count,
        ));
    }
    if stale_count > 0 && !clean {
        scan_issues.push(format!(
            "Scan diff: {} stale auto-scanned {} no longer in code (run cog verify --scan --clean to remove)",
            stale_count,
            entities_word(stale_count),
        ));
        for name in stale_names {
            scan_issues.push(format!("  - {name}"));
        }
    } else if stale_count > 0 && clean {
        scan_issues.push(format!(
            "Scan diff: cleaned {} stale auto-scanned {}",
            stale_count,
            entities_word(stale_count),
        ));
    }
    scan_issues
}

/// Returns "entity" for count 1, "entities" otherwise.
fn entities_word(count: usize) -> &'static str {
    if count == 1 { "entity" } else { "entities" }
}

#[cfg(test)]
mod tests {
    use crate::repo::Repository;
    use std::fs;

    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::command::sync_cmd;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin, EntityRelationKind};
    use crate::repo::SqliteRepository;

    #[test]
    fn reports_isolated_entity_issue() -> Result<()> {
        let tmp = tempdir()?;
        let store = SqliteRepository::open(&tmp.path().join("cog.db"))?;
        store.upsert_entity("orphan", EntityKind::Module, EntityOrigin::Manual)?;

        let output = execute(&store, None, false, None, crate::format::OutputFormat::Text)?;
        assert_eq!(output.exit_code, 1);
        assert!(output.text.contains("IsolatedEntity"));
        Ok(())
    }

    #[test]
    fn passes_when_structure_is_consistent() -> Result<()> {
        let tmp = tempdir()?;
        let store = SqliteRepository::open(&tmp.path().join("cog.db"))?;
        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns token",
            "code:auth::login",
            None,
        )?;
        // Add a relation so the Manual entity is not flagged as orphan
        let module = store.upsert_entity("auth", EntityKind::Module, EntityOrigin::Scan)?;
        store.add_entity_relation(&module.id, &entity.id, EntityRelationKind::Contains)?;

        let output = execute(
            &store,
            Some("auth"),
            false,
            None,
            crate::format::OutputFormat::Text,
        )?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("verify: ok"));
        Ok(())
    }

    /// Creates a minimal Rust source tree in a temp dir for scan testing.
    fn make_rust_project(dir: &std::path::Path, functions: &[&str]) {
        let src_dir = dir.join("src");
        fs::create_dir_all(&src_dir).unwrap();
        let code: String = functions
            .iter()
            .map(|f| format!("fn {}() {{}}\n", f))
            .collect();
        fs::write(src_dir.join("main.rs"), code).unwrap();
    }

    #[test]
    fn scan_detects_stale_entity() -> Result<()> {
        let tmp = tempdir()?;
        let db_path = tmp.path().join(".cog").join("cog.db");
        std::fs::create_dir_all(db_path.parent().unwrap())?;
        let store = SqliteRepository::open(&db_path)?;

        // Create a Rust project with one function and init (populates store with origin=Scan)
        make_rust_project(tmp.path(), &["hello"]);
        sync_cmd::execute(
            &store,
            &db_path,
            false,
            None,
            crate::format::OutputFormat::Text,
        )?;

        // Verify: entity is in the model AND in the scan → ok
        let output = execute(
            &store,
            None,
            false,
            Some(tmp.path()),
            crate::format::OutputFormat::Text,
        )?;
        assert_eq!(output.exit_code, 0, "first verify should pass");

        // Remove the function from source
        fs::write(tmp.path().join("src/main.rs"), "")?;

        // Now verify should detect the stale scanned entity
        let output = execute(
            &store,
            None,
            false,
            Some(tmp.path()),
            crate::format::OutputFormat::Text,
        )?;
        assert_eq!(output.exit_code, 1);
        assert!(output.text.contains("stale"));

        Ok(())
    }

    #[test]
    fn scan_detects_unmodeled_definitions() -> Result<()> {
        let tmp = tempdir()?;
        let store = SqliteRepository::open(&tmp.path().join("cog.db"))?;

        // Scan against an empty model — all definitions should be unmodeled
        make_rust_project(tmp.path(), &["alpha", "beta"]);
        let output = execute(
            &store,
            None,
            false,
            Some(tmp.path()),
            crate::format::OutputFormat::Text,
        )?;

        // Should report unmodeled (advisory) but return ok since no stale entities
        assert!(output.text.contains("unmodeled"));
        assert_eq!(output.exit_code, 0);

        Ok(())
    }

    #[test]
    fn scan_clean_removes_stale_entities() -> Result<()> {
        let tmp = tempdir()?;
        let db_path = tmp.path().join(".cog").join("cog.db");
        std::fs::create_dir_all(db_path.parent().unwrap())?;
        let store = SqliteRepository::open(&db_path)?;

        // Create project and sync (populates model with Scan-origin entities)
        make_rust_project(tmp.path(), &["hello"]);
        sync_cmd::execute(
            &store,
            &db_path,
            false,
            None,
            crate::format::OutputFormat::Text,
        )?;

        // Remove the function
        fs::write(tmp.path().join("src/main.rs"), "")?;

        // Run verify --scan --clean — should detect and clean the stale entity
        let output = execute(
            &store,
            None,
            true,
            Some(tmp.path()),
            crate::format::OutputFormat::Text,
        )?;
        assert!(
            output.text.contains("cleaned"),
            "expected 'cleaned' in: {}",
            output.text
        );

        // The file module entity may now be isolated (its child was deleted).
        // Clean it explicitly before the final verification.
        execute(&store, None, true, None, crate::format::OutputFormat::Text)?;

        // Verify again — should be clean now
        let output = execute(
            &store,
            None,
            false,
            Some(tmp.path()),
            crate::format::OutputFormat::Text,
        )?;
        assert!(output.text.contains("verify: ok"));

        Ok(())
    }

    #[test]
    fn scoped_scan_does_not_report_out_of_scope_as_unmodeled() -> Result<()> {
        let tmp = tempdir()?;
        let db_path = tmp.path().join(".cog").join("cog.db");
        std::fs::create_dir_all(db_path.parent().unwrap())?;
        let store = SqliteRepository::open(&db_path)?;

        // Create two source files with functions in different scopes
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("auth.rs"), "fn login() {}\nfn logout() {}\n")?;
        fs::write(src.join("db.rs"), "fn connect() {}\n")?;

        // Init to populate the model with all scanned entities
        sync_cmd::execute(
            &store,
            &db_path,
            false,
            None,
            crate::format::OutputFormat::Text,
        )?;

        // Remove db.rs — db::connect becomes stale, but it's out of the auth scope
        fs::remove_file(src.join("db.rs"))?;

        // Verify with --scope auth — should NOT report unmodeled for out-of-scope entities
        let output = execute(
            &store,
            Some("auth"),
            false,
            Some(tmp.path()),
            crate::format::OutputFormat::Text,
        )?;
        assert!(
            !output.text.contains("unmodeled"),
            "out-of-scope entities should not be reported as unmodeled: {}",
            output.text
        );

        Ok(())
    }

    #[test]
    fn reports_dangling_grounds_when_entity_missing() -> Result<()> {
        let tmp = tempdir()?;
        let store = SqliteRepository::open(&tmp.path().join("cog.db"))?;
        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "delegates to validate_credentials",
            "code:auth::validate_credentials",
            None,
        )?;

        // auth::validate_credentials does NOT exist in the model → dangling grounds
        let output = execute(&store, None, false, None, crate::format::OutputFormat::Text)?;
        assert_eq!(output.exit_code, 1);
        assert!(output.text.contains("DanglingGrounds"));
        assert!(
            output.text.contains("auth::validate_credentials"),
            "should name the missing entity: {}",
            output.text
        );
        Ok(())
    }

    #[test]
    fn passes_when_code_grounds_reference_existing_entity() -> Result<()> {
        let tmp = tempdir()?;
        let store = SqliteRepository::open(&tmp.path().join("cog.db"))?;
        let login =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        let validate = store.upsert_entity(
            "auth::validate_credentials",
            EntityKind::Function,
            EntityOrigin::Manual,
        )?;
        store.add_entity_relation(&login.id, &validate.id, EntityRelationKind::Calls)?;
        store.create_assertion(
            &login.id,
            AssertionKind::Contract,
            "delegates to validate_credentials",
            "code:auth::validate_credentials",
            None,
        )?;

        let output = execute(&store, None, false, None, crate::format::OutputFormat::Text)?;
        assert!(output.text.contains("verify: ok"));
        assert_eq!(output.exit_code, 0);
        Ok(())
    }
}

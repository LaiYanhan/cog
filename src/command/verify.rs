use std::collections::HashSet;
use std::fmt::Write;
use std::path::Path;

use anyhow::Result;

use crate::analysis::{ScanConfig, Scanner};
use crate::command::CommandOutput;
use crate::model::{
    AssertionStatus, Changelog, ChangelogAction, Store, VerificationIssue, VerificationIssueKind,
};

pub fn execute(
    store: &Store,
    scope: Option<&str>,
    clean: bool,
    scan_path: Option<&Path>,
) -> Result<CommandOutput> {
    let mut issues = Vec::new();
    let mut cleaned: usize = 0;
    let all_entities = store.list_entities()?;
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

    for entity in &entities {
        let assertions = store.get_assertions_for_entity(&entity.id)?;
        let relation_count = store.count_relations_for_entity(&entity.id)?;

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
                store.delete_entity(&entity.qualified_name)?;
                cleaned += 1;
            }
        }

        for assertion in &assertions {
            if assertion.status != AssertionStatus::Active {
                continue;
            }

            let evidence = store.get_evidence_for_assertion(&assertion.id)?;
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

            for dependency in store.get_dependencies(&assertion.id)? {
                if dependency.status == AssertionStatus::Retracted {
                    issues.push(VerificationIssue {
                        kind: VerificationIssueKind::DependencyOnRetracted,
                        entity_name: Some(entity.qualified_name.clone()),
                        assertion_id: Some(assertion.id.clone()),
                        detail: format!(
                            "depends on retracted assertion {}",
                            crate::format::short_id(&dependency.id)
                        ),
                    });
                } else if dependency.status == AssertionStatus::Uncertain {
                    issues.push(VerificationIssue {
                        kind: VerificationIssueKind::DependencyOnUncertain,
                        entity_name: Some(entity.qualified_name.clone()),
                        assertion_id: Some(assertion.id.clone()),
                        detail: format!(
                            "depends on uncertain assertion {}",
                            crate::format::short_id(&dependency.id)
                        ),
                    });
                }
            }
        }
    }

    // Scan diff: compare model against actual code
    let mut unmodeled_count: usize = 0;
    let mut stale_names: Vec<String> = Vec::new();

    if let Some(path) = scan_path {
        let config = ScanConfig {
            root: path.to_path_buf(),
            ..Default::default()
        };
        let scan_result = Scanner::scan(&config)?;

        // Build the full set of scanned entity names: definitions + file/directory modules.
        // This mirrors what init_cmd creates, so stale detection works for both
        // function/type entities and structural module entities.
        let path_to_qualified = crate::command::init_cmd::path_to_qualified;
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
        let auto_scanned_names = store.get_scanned_entity_names()?;

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
                store.delete_entity(name)?;
            }
        }
    }

    Changelog::append(
        store,
        ChangelogAction::Verify,
        scope.unwrap_or("*"),
        &format!("issues={}", issues.len()),
    )?;

    let clean_note = if cleaned > 0 || (clean && !stale_names.is_empty()) {
        let total_cleaned = cleaned + if clean { stale_names.len() } else { 0 };
        format!(", cleaned={}", total_cleaned)
    } else {
        String::new()
    };

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
    // - Only unmodeled (advisory: "you could model these") but no stale → still ok
    // - Any stale entity → failure (model is out of sync with code)
    if success && (!has_scan_diff || (unmodeled_count > 0 && stale_count == 0)) {
        let mut msg = format!(
            "verify: ok (checked {} entities: isolated, missing evidence, dangling dependencies, dangling grounds{})",
            checked_count, clean_note,
        );
        if unmodeled_count > 0 {
            let _ = write!(
                msg,
                "\nScan diff: {} unmodeled definition(s) not in model (run cog init to add)",
                unmodeled_count
            );
        }
        return Ok(CommandOutput::success(msg));
    }
    if success && scan_cleaned {
        return Ok(CommandOutput::success(format!(
            "verify: ok (checked {} entities{})\nScan diff: cleaned {} stale {}",
            checked_count,
            clean_note,
            stale_count,
            entities_word(stale_count),
        )));
    }

    let mut report = String::new();
    if !issues.is_empty() {
        let _ = writeln!(
            report,
            "verify: found {} issue(s){}",
            issues.len(),
            clean_note
        );
        for issue in &issues {
            let _ = writeln!(
                report,
                "- {:?} entity={} assertion={} detail={}",
                issue.kind,
                issue.entity_name.as_deref().unwrap_or("-"),
                issue
                    .assertion_id
                    .as_deref()
                    .map(crate::format::short_id)
                    .unwrap_or("-"),
                issue.detail,
            );
        }
    }

    if has_scan_diff {
        if unmodeled_count > 0 {
            let _ = writeln!(
                report,
                "Scan diff: {} unmodeled definition(s) not in model (run cog init to add)",
                unmodeled_count,
            );
        }
        if stale_count > 0 && !clean {
            let _ = writeln!(
                report,
                "Scan diff: {} stale auto-scanned {} no longer in code (run cog verify --scan --clean to remove)",
                stale_count,
                entities_word(stale_count),
            );
            for name in &stale_names {
                let _ = writeln!(report, "  - {}", name);
            }
        } else if stale_count > 0 && clean {
            let _ = writeln!(
                report,
                "Scan diff: cleaned {} stale auto-scanned {}",
                stale_count,
                entities_word(stale_count),
            );
        }
    }

    Ok(CommandOutput::with_exit_code(report, 1))
}

/// Returns "entity" for count 1, "entities" otherwise.
fn entities_word(count: usize) -> &'static str {
    if count == 1 { "entity" } else { "entities" }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::command::init_cmd;
    use crate::model::{AssertionKind, EntityKind, EntityOrigin, EntityRelationKind, Store};

    #[test]
    fn reports_isolated_entity_issue() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        store.upsert_entity("orphan", EntityKind::Module, EntityOrigin::Manual)?;

        let output = execute(&store, None, false, None)?;
        assert_eq!(output.exit_code, 1);
        assert!(output.text.contains("IsolatedEntity"));
        Ok(())
    }

    #[test]
    fn passes_when_structure_is_consistent() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns token",
            "code:auth::login",
            None,
        )?;

        let output = execute(&store, Some("auth"), false, None)?;
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
        let store = Store::open(&tmp.path().join("cog.db"))?;

        // Create a Rust project with one function and init (populates store with origin=Scan)
        make_rust_project(tmp.path(), &["hello"]);
        init_cmd::execute(&store, &tmp.path().to_path_buf(), false, None, None)?;

        // Verify: entity is in the model AND in the scan → ok
        let output = execute(&store, None, false, Some(tmp.path()))?;
        assert_eq!(output.exit_code, 0, "first verify should pass");

        // Remove the function from source
        fs::write(tmp.path().join("src/main.rs"), "")?;

        // Now verify should detect the stale scanned entity
        let output = execute(&store, None, false, Some(tmp.path()))?;
        assert_eq!(output.exit_code, 1);
        assert!(output.text.contains("stale"));

        Ok(())
    }

    #[test]
    fn scan_detects_unmodeled_definitions() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;

        // Scan against an empty model — all definitions should be unmodeled
        make_rust_project(tmp.path(), &["alpha", "beta"]);
        let output = execute(&store, None, false, Some(tmp.path()))?;

        // Should report unmodeled (advisory) but return ok since no stale entities
        assert!(output.text.contains("unmodeled"));
        assert_eq!(output.exit_code, 0);

        Ok(())
    }

    #[test]
    fn scan_clean_removes_stale_entities() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;

        // Create project and init
        make_rust_project(tmp.path(), &["hello"]);
        init_cmd::execute(&store, &tmp.path().to_path_buf(), false, None, None)?;

        // Remove the function
        fs::write(tmp.path().join("src/main.rs"), "")?;

        // Run verify --scan --clean
        let output = execute(&store, None, true, Some(tmp.path()))?;
        assert!(
            output.text.contains("cleaned"),
            "expected 'cleaned' in: {}",
            output.text
        );

        // Verify again — should be clean now
        let output = execute(&store, None, false, Some(tmp.path()))?;
        assert!(output.text.contains("verify: ok"));

        Ok(())
    }

    #[test]
    fn scoped_scan_does_not_report_out_of_scope_as_unmodeled() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;

        // Create two source files with functions in different scopes
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("auth.rs"), "fn login() {}\nfn logout() {}\n")?;
        fs::write(src.join("db.rs"), "fn connect() {}\n")?;

        // Init to populate the model with all scanned entities
        init_cmd::execute(&store, &tmp.path().to_path_buf(), false, None, None)?;

        // Remove db.rs — db::connect becomes stale, but it's out of the auth scope
        fs::remove_file(src.join("db.rs"))?;

        // Verify with --scope auth — should NOT report unmodeled for out-of-scope entities
        let output = execute(&store, Some("auth"), false, Some(tmp.path()))?;
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
        let store = Store::open(&tmp.path().join("cog.db"))?;
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
        let output = execute(&store, None, false, None)?;
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
        let store = Store::open(&tmp.path().join("cog.db"))?;
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

        let output = execute(&store, None, false, None)?;
        assert!(output.text.contains("verify: ok"));
        assert_eq!(output.exit_code, 0);
        Ok(())
    }
}

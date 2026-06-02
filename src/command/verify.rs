use std::fmt::Write;

use anyhow::Result;

use crate::command::CommandOutput;
use crate::model::{
    AssertionStatus, Changelog, ChangelogAction, Store, VerificationIssue, VerificationIssueKind,
};

pub fn execute(store: &Store, scope: Option<&str>, clean: bool) -> Result<CommandOutput> {
    let mut issues = Vec::new();
    let mut cleaned: usize = 0;
    let all_entities = store.list_entities()?;
    let scope_prefix = scope.unwrap_or_default();

    // Filter entities by scope
    let entities: Vec<_> = all_entities
        .into_iter()
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

            let evidence_count = store.count_evidence_for_assertion(&assertion.id)?;
            if evidence_count == 0 {
                issues.push(VerificationIssue {
                    kind: VerificationIssueKind::MissingEvidence,
                    entity_name: Some(entity.qualified_name.clone()),
                    assertion_id: Some(assertion.id.clone()),
                    detail: "active assertion has no evidence".to_string(),
                });
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

    Changelog::append(
        store,
        ChangelogAction::Verify,
        scope.unwrap_or("*"),
        &format!("issues={}", issues.len()),
    )?;

    let clean_note = if cleaned > 0 {
        format!(", cleaned={}", cleaned)
    } else {
        String::new()
    };

    if issues.is_empty()
        || (issues
            .iter()
            .all(|i| i.kind == VerificationIssueKind::IsolatedEntity)
            && cleaned > 0)
    {
        return Ok(CommandOutput::success(format!(
            "verify: ok (checked {} entities: isolated, missing evidence, dangling dependencies{})",
            checked_count, clean_note,
        )));
    }

    let mut report = String::new();
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

    Ok(CommandOutput::with_exit_code(report, 1))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::model::{AssertionKind, EntityKind, Store};

    #[test]
    fn reports_isolated_entity_issue() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        store.upsert_entity("orphan", EntityKind::Module)?;

        let output = execute(&store, None, false)?;
        assert_eq!(output.exit_code, 1);
        assert!(output.text.contains("IsolatedEntity"));
        Ok(())
    }

    #[test]
    fn passes_when_structure_is_consistent() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let entity = store.upsert_entity("auth::login", EntityKind::Function)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns token",
            "code:src/auth.rs:1",
            None,
        )?;

        let output = execute(&store, Some("auth"), false)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("verify: ok"));
        Ok(())
    }
}

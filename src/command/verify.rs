use std::fmt::Write;

use anyhow::Result;

use crate::command::CommandOutput;
use crate::model::{
    AssertionStatus, Changelog, ChangelogAction, Store, VerificationIssue, VerificationIssueKind,
};

pub fn execute(store: &Store, scope: Option<&str>) -> Result<CommandOutput> {
    let mut issues = Vec::new();
    let entities = store.list_entities()?;
    let scope_prefix = scope.unwrap_or_default();

    for entity in entities
        .into_iter()
        .filter(|entity| scope.is_none_or(|_| entity.qualified_name.starts_with(scope_prefix)))
    {
        let assertions = store.get_assertions_for_entity(&entity.id)?;
        let relation_count = store.count_relations_for_entity(&entity.id)?;

        if assertions.is_empty() && relation_count == 0 {
            issues.push(VerificationIssue {
                kind: VerificationIssueKind::IsolatedEntity,
                entity_name: Some(entity.qualified_name.clone()),
                assertion_id: None,
                detail: "entity has no assertions and no relations".to_string(),
            });
        }

        for assertion in assertions {
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
                        detail: format!("depends on retracted assertion {}", dependency.id),
                    });
                } else if dependency.status == AssertionStatus::Uncertain {
                    issues.push(VerificationIssue {
                        kind: VerificationIssueKind::DependencyOnUncertain,
                        entity_name: Some(entity.qualified_name.clone()),
                        assertion_id: Some(assertion.id.clone()),
                        detail: format!("depends on uncertain assertion {}", dependency.id),
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

    if issues.is_empty() {
        return Ok(CommandOutput::success("verify: ok"));
    }

    let mut report = String::new();
    let _ = writeln!(report, "verify: found {} issue(s)", issues.len());
    for issue in &issues {
        let _ = writeln!(
            report,
            "- {:?} entity={} assertion={} detail={}",
            issue.kind,
            issue.entity_name.as_deref().unwrap_or("-"),
            issue.assertion_id.as_deref().unwrap_or("-"),
            issue.detail
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

        let output = execute(&store, None)?;
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

        let output = execute(&store, Some("auth"))?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("verify: ok"));
        Ok(())
    }
}

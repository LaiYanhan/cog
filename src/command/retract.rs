use anyhow::{Result, anyhow};

use crate::command::CommandOutput;
use crate::domain::StatusMessage;
use crate::format::{self, OutputFormat, TextRenderer};
use crate::repo::Repository;
use crate::space::CascadeEngine;

pub fn execute(
    repo: &dyn Repository,
    id: &str,
    reason: &str,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let resolved = repo.resolve_assertion_id(id)?;
    let result = CascadeEngine::retract(repo, &resolved, reason)?;

    // Gather entity name and remaining active assertions with evidence
    let entity = repo
        .get_entity(&result.retracted.entity_id)?
        .ok_or_else(|| anyhow!("entity not found: {}", result.retracted.entity_id))?;
    let entity_name = entity.qualified_name.clone();

    let raw_assertions = repo.get_assertions_for_entity(&entity.id)?;
    // Only show non-retracted assertions (Active + Uncertain) in the remaining list
    let mut remaining: Vec<(crate::domain::Assertion, Vec<crate::domain::Evidence>)> = Vec::new();
    for a in &raw_assertions {
        if a.status != crate::domain::AssertionStatus::Retracted {
            let ev = repo.get_evidence_for_assertion(&a.id)?;
            remaining.push((a.clone(), ev));
        }
    }

    let msg = TextRenderer::cascade_report(&result, &entity_name, &remaining);
    Ok(CommandOutput::success(format::emit_report(
        &StatusMessage { message: msg },
        output,
    )))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::execute;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin};
    use crate::format::OutputFormat;
    use crate::repo::{Repository, SqliteRepository};

    #[test]
    fn retracts_and_reports_affected_assertions() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        let base = store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "base",
            "note:base",
            None,
        )?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Invariant,
            "dependent",
            "note:dep",
            Some(&base.id),
        )?;

        let output = execute(&store, &base.id, "invalid", OutputFormat::Text)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("Retracted"));
        assert!(output.text.contains("Cascade:"));
        Ok(())
    }
}

use anyhow::Result;

use crate::command::CommandOutput;
use crate::format::{self, OutputFormat};
use crate::repo::Repository;
use crate::space::ImpactEngine;

pub fn execute(repo: &dyn Repository, entity: &str, output: OutputFormat) -> Result<CommandOutput> {
    let impact = ImpactEngine::analyze(repo, entity)?;
    let mut out = CommandOutput::success(format::emit_report(&impact, output));
    out.metrics = Some(serde_json::json!({
        "downstream": impact.downstream_entities.len(),
        "affected_assertions": impact.affected_assertions.len(),
        "blind_downstream": impact.blind_downstream,
        "downstream_coverage": impact.downstream_coverage,
        "risk_score": impact.risk_assessment.as_ref().map(|r| r.risk_score)
    }));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use crate::repo::Repository;
    use anyhow::Result;

    use super::execute;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin, EntityRelationKind};
    use crate::format::OutputFormat;
    use crate::repo::SqliteRepository;

    #[test]
    fn reports_downstream_impact() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        let a = store.upsert_entity("A", EntityKind::Module, EntityOrigin::Manual)?;
        let b = store.upsert_entity("B", EntityKind::Module, EntityOrigin::Manual)?;
        // Impact now only follows Calls + Uses edges (not Contains).
        store.add_entity_relation(&b.id, &a.id, EntityRelationKind::Calls)?;
        store.create_assertion(&a.id, AssertionKind::Contract, "a", "note:a", None)?;
        let output = execute(&store, "A", OutputFormat::Text)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("Blind dependents (1): B"));
        // "B [module]" format is only for covered entities.
        Ok(())
    }

    #[test]
    fn contains_edges_not_used_for_impact() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        let a = store.upsert_entity("Parent", EntityKind::Module, EntityOrigin::Manual)?;
        let b = store.upsert_entity("Child", EntityKind::Type, EntityOrigin::Manual)?;
        // Contains is structural — impact should NOT follow it
        store.add_entity_relation(&a.id, &b.id, EntityRelationKind::Contains)?;
        store.create_assertion(&a.id, AssertionKind::Contract, "c", "note", None)?;
        let output = execute(&store, "Parent", OutputFormat::Text)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("Impact for: Parent"));
        // Child should NOT appear because Contains is not a dependency edge
        assert!(!output.text.contains("Child [type]"));
        // Should indicate no dependents found
        assert!(output.text.contains("No dependents found"));
        Ok(())
    }
}

use anyhow::Result;

use crate::command::CommandOutput;
use crate::format::{self, OutputFormat};
use crate::repo::Repository;
use crate::space::ImpactEngine;

pub fn execute(repo: &dyn Repository, entity: &str, output: OutputFormat) -> Result<CommandOutput> {
    let impact = ImpactEngine::analyze(repo, entity)?;
    Ok(CommandOutput::success(format::emit_report(&impact, output)))
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
        store.add_entity_relation(&a.id, &b.id, EntityRelationKind::Contains)?;
        store.create_assertion(&a.id, AssertionKind::Contract, "a", "note:a", None)?;
        let output = execute(&store, "A", OutputFormat::Text)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("impact_from: A"));
        assert!(output.text.contains("B [module]"));
        Ok(())
    }
}

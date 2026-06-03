use anyhow::Result;

use crate::command::CommandOutput;
use crate::format;
use crate::model::{ImpactResult, Store};

pub fn execute(store: &Store, entity: &str) -> Result<CommandOutput> {
    let impact = ImpactResult::analyze(store, entity)?;
    Ok(CommandOutput::success(format::impact_report(&impact)))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::model::{AssertionKind, EntityKind, EntityOrigin, EntityRelationKind, Store};

    #[test]
    fn reports_downstream_impact() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let a = store.upsert_entity("A", EntityKind::Module, EntityOrigin::Manual)?;
        let b = store.upsert_entity("B", EntityKind::Module, EntityOrigin::Manual)?;
        store.add_entity_relation(&a.id, &b.id, EntityRelationKind::Contains)?;
        store.create_assertion(&a.id, AssertionKind::Contract, "a", "note:a", None)?;

        let output = execute(&store, "A")?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("impact_from: A"));
        assert!(output.text.contains("B [module]"));
        Ok(())
    }
}

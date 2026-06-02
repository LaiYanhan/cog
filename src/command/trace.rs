use anyhow::Result;

use crate::command::CommandOutput;
use crate::format;
use crate::model::{Store, TraceResult};

pub fn execute(store: &Store, entity: &str) -> Result<CommandOutput> {
    let trace = TraceResult::trace(store, entity)?;
    Ok(CommandOutput::success(format::trace_report(&trace)))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::model::{AssertionKind, EntityKind, Store};

    #[test]
    fn reports_trace_tree() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let entity = store.upsert_entity("auth::login", EntityKind::Function)?;
        let root = store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "root",
            "note:root",
            None,
        )?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Invariant,
            "child",
            "note:child",
            Some(&root.id),
        )?;

        let output = execute(&store, "auth::login")?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("trace_entity: auth::login"));
        assert!(output.text.contains("child"));
        Ok(())
    }
}

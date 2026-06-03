use anyhow::Result;

use crate::command::CommandOutput;
use crate::format;
use crate::model::Store;

pub fn execute(store: &Store) -> Result<CommandOutput> {
    let stats = store.stats()?;
    Ok(CommandOutput::success(format::stats_report(&stats)))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::model::{AssertionKind, EntityKind, EntityOrigin, Store};

    #[test]
    fn returns_model_stats() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns token",
            "code:src/auth.rs:5",
            None,
        )?;

        let output = execute(&store)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("entities: 1"));
        assert!(output.text.contains("assertions: 1"));
        Ok(())
    }
}

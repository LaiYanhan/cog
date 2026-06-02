use anyhow::Result;

use crate::command::CommandOutput;
use crate::format;
use crate::model::{CascadeResult, Store};

pub fn execute(store: &Store, id: &str, reason: &str) -> Result<CommandOutput> {
    let result = CascadeResult::retract(store, id, reason)?;
    Ok(CommandOutput::success(format::cascade_report(&result)))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::model::{AssertionKind, EntityKind, Store};

    #[test]
    fn retracts_and_reports_affected_assertions() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let entity = store.upsert_entity("auth::login", EntityKind::Function)?;
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

        let output = execute(&store, &base.id, "invalid")?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("retracted:"));
        assert!(output.text.contains("affected:"));
        Ok(())
    }
}

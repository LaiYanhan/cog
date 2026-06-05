use anyhow::Result;

use crate::command::CommandOutput;
use crate::format;
use crate::repo::SqliteRepository;
use crate::space::CascadeEngine;

pub fn execute(store: &SqliteRepository, id: &str, reason: &str) -> Result<CommandOutput> {
    let resolved = store.resolve_assertion_id(id)?;
    let result = CascadeEngine::retract(store, &resolved, reason)?;
    Ok(CommandOutput::success(format::cascade_report(&result)))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin};
    use crate::repo::SqliteRepository;

    #[test]
    fn retracts_and_reports_affected_assertions() -> Result<()> {
        let tmp = tempdir()?;
        let store = SqliteRepository::open(&tmp.path().join("cog.db"))?;
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

        let output = execute(&store, &base.id, "invalid")?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("retracted:"));
        assert!(output.text.contains("affected:"));
        Ok(())
    }
}

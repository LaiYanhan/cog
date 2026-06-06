use anyhow::Result;

use crate::command::CommandOutput;
use crate::format::{self, OutputFormat};
use crate::repo::{Repository, SqliteRepository};
use crate::space::CascadeEngine;

pub fn execute(
    store: &SqliteRepository,
    id: &str,
    reason: &str,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let resolved = store.resolve_assertion_id(id)?;
    let result = CascadeEngine::retract(store, &resolved, reason)?;
    Ok(CommandOutput::success(format::emit_report(&result, output)))
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
        assert!(output.text.contains("retracted:"));
        assert!(output.text.contains("affected:"));
        Ok(())
    }
}

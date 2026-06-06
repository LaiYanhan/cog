use anyhow::Result;

use crate::command::CommandOutput;
use crate::format::{self, OutputFormat};
use crate::repo::Repository;

pub fn execute(repo: &dyn Repository, output: OutputFormat) -> Result<CommandOutput> {
    let stats = repo.stats()?;
    Ok(CommandOutput::success(format::emit_report(&stats, output)))
}

#[cfg(test)]
mod tests {
    use crate::repo::Repository;
    use anyhow::Result;

    use super::execute;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin};
    use crate::format::OutputFormat;
    use crate::repo::SqliteRepository;

    #[test]
    fn returns_model_stats() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns token",
            "code:auth::login",
            None,
        )?;

        let output = execute(&store, OutputFormat::Text)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("entities: 1"));
        assert!(output.text.contains("assertions: 1"));
        Ok(())
    }
}

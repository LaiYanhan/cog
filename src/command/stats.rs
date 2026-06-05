use anyhow::Result;

use crate::command::CommandOutput;
use crate::format;
use crate::repo::Repository;

pub fn execute(repo: &dyn Repository) -> Result<CommandOutput> {
    let stats = repo.stats()?;
    Ok(CommandOutput::success(format::stats_report(&stats)))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin};
    use crate::repo::SqliteRepository;

    #[test]
    fn returns_model_stats() -> Result<()> {
        let tmp = tempdir()?;
        let store = SqliteRepository::open(&tmp.path().join("cog.db"))?;
        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns token",
            "code:auth::login",
            None,
        )?;

        let output = execute(&store)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("entities: 1"));
        assert!(output.text.contains("assertions: 1"));
        Ok(())
    }
}

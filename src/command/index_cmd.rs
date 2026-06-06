use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::{EntityIndex, EntityKind, EntityOrigin};
use crate::format::{self, OutputFormat};
use crate::repo::Repository;

pub fn execute(
    repo: &dyn Repository,
    kind: Option<EntityKind>,
    origin: Option<EntityOrigin>,
    prefix: Option<&str>,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let entities = repo.list_entities_filtered(kind, origin, prefix)?;
    let report = EntityIndex { entities };
    Ok(CommandOutput::success(format::emit_report(&report, output)))
}

#[cfg(test)]
mod tests {
    use crate::repo::Repository;
    use anyhow::Result;

    use super::execute;
    use crate::domain::{EntityKind, EntityOrigin};
    use crate::format::OutputFormat;
    use crate::repo::SqliteRepository;

    #[test]
    fn lists_entities() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        store.upsert_entity("auth", EntityKind::Module, EntityOrigin::Manual)?;
        let output = execute(&store, None, None, None, OutputFormat::Text)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("auth"));
        Ok(())
    }

    #[test]
    fn filters_by_kind() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        store.upsert_entity("auth", EntityKind::Module, EntityOrigin::Manual)?;
        store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Scan)?;

        let output = execute(
            &store,
            Some(EntityKind::Function),
            None,
            None,
            OutputFormat::Text,
        )?;
        assert!(output.text.contains("auth::login"));
        assert!(!output.text.contains("auth]"));
        Ok(())
    }

    #[test]
    fn filters_by_prefix() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.upsert_entity("db::connect", EntityKind::Function, EntityOrigin::Manual)?;
        let output = execute(&store, None, None, Some("auth"), OutputFormat::Text)?;
        assert!(output.text.contains("auth::login"));
        assert!(!output.text.contains("db::connect"));
        Ok(())
    }
}

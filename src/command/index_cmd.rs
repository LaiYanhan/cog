use anyhow::Result;

use crate::command::CommandOutput;
use crate::format;
use crate::model::{EntityKind, EntityOrigin, Store};

pub fn execute(
    store: &Store,
    kind: Option<EntityKind>,
    origin: Option<EntityOrigin>,
    prefix: Option<&str>,
) -> Result<CommandOutput> {
    let entities = store.list_entities_filtered(kind, origin, prefix)?;
    Ok(CommandOutput::success(format::entity_index_with_counts(
        &entities,
    )))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::model::{EntityKind, EntityOrigin, Store};

    #[test]
    fn lists_entities() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        store.upsert_entity("auth", EntityKind::Module, EntityOrigin::Manual)?;

        let output = execute(&store, None, None, None)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("auth"));
        Ok(())
    }

    #[test]
    fn filters_by_kind() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        store.upsert_entity("auth", EntityKind::Module, EntityOrigin::Manual)?;
        store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Scan)?;

        // Filter to functions only
        let output = execute(&store, Some(EntityKind::Function), None, None)?;
        assert!(output.text.contains("auth::login"));
        assert!(!output.text.contains("auth]"));
        Ok(())
    }

    #[test]
    fn filters_by_prefix() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.upsert_entity("db::connect", EntityKind::Function, EntityOrigin::Manual)?;

        let output = execute(&store, None, None, Some("auth"))?;
        assert!(output.text.contains("auth::login"));
        assert!(!output.text.contains("db::connect"));
        Ok(())
    }
}

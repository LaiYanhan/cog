use anyhow::Result;

use crate::command::CommandOutput;
use crate::format;
use crate::model::Store;

pub fn execute(store: &Store) -> Result<CommandOutput> {
    let entities = store.list_entities_with_counts()?;
    Ok(CommandOutput::success(format::entity_index_with_counts(
        &entities,
    )))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::model::{EntityKind, Store};

    #[test]
    fn lists_entities() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        store.upsert_entity("auth", EntityKind::Module)?;

        let output = execute(&store)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("auth"));
        Ok(())
    }
}

use anyhow::Result;

use crate::command::CommandOutput;
use crate::format::{self, OutputFormat};
use crate::repo::Repository;
use crate::space::TraceEngine;

pub fn execute(repo: &dyn Repository, entity: &str, output: OutputFormat) -> Result<CommandOutput> {
    let trace = TraceEngine::trace(repo, entity)?;
    Ok(CommandOutput::success(format::emit_report(&trace, output)))
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
    fn reports_trace_tree() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
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

        let output = execute(&store, "auth::login", OutputFormat::Text)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("trace_entity: auth::login"));
        assert!(output.text.contains("child"));
        Ok(())
    }
}

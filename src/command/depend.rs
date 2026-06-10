use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::{ChangelogAction, EntityRelationKind, StatusMessage};
use crate::format::TextRenderer;
use crate::format::{self, OutputFormat};
use crate::repo::Repository;

pub fn execute(
    repo: &dyn Repository,
    entity_a: &str,
    entity_b: &str,
    kind: EntityRelationKind,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let left = repo.ensure_manual_entity(entity_a)?;
    let right = repo.ensure_manual_entity(entity_b)?;

    repo.add_entity_relation(&left.id, &right.id, kind)?;
    repo.append_changelog(
        ChangelogAction::Depend,
        &left.id,
        &format!(
            "from={} to={} kind={}",
            left.qualified_name, right.qualified_name, kind
        ),
    )?;

    let related = repo.get_related_entities(&left.id)?;
    let msg = TextRenderer::dependency_report(&left, &right, kind, &related);
    Ok(CommandOutput::success(format::emit_report(
        &StatusMessage { message: msg },
        output,
    )))
}

#[cfg(test)]
mod tests {
    use crate::repo::Repository;
    use anyhow::Result;

    use super::execute;
    use crate::domain::EntityRelationKind;
    use crate::format::OutputFormat;
    use crate::repo::SqliteRepository;

    #[test]
    fn records_entity_dependency() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;

        let output = execute(
            &store,
            "auth::login",
            "AuthToken",
            EntityRelationKind::Uses,
            OutputFormat::Text,
        )?;

        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("dependency recorded"));

        let left = store
            .get_entity_by_name("auth::login")?
            .expect("left entity should exist");
        let related = store.get_related_entities(&left.id)?;
        assert_eq!(related.len(), 1);
        assert_eq!(related[0].entity.qualified_name, "AuthToken");
        Ok(())
    }
}

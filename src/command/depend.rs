use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::{ChangelogAction, EntityKind, EntityOrigin, EntityRelationKind, StatusMessage};
use crate::format::{self, OutputFormat};
use crate::repo::Repository;

pub fn execute(
    repo: &dyn Repository,
    entity_a: &str,
    entity_b: &str,
    kind: EntityRelationKind,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let left = repo.upsert_entity(entity_a, EntityKind::infer(entity_a), EntityOrigin::Manual)?;
    let right = repo.upsert_entity(entity_b, EntityKind::infer(entity_b), EntityOrigin::Manual)?;

    repo.add_entity_relation(&left.id, &right.id, kind)?;
    repo.append_changelog(
        ChangelogAction::Depend,
        &left.id,
        &format!(
            "from={} to={} kind={}",
            left.qualified_name, right.qualified_name, kind
        ),
    )?;

    let msg = format::dependency_recorded(&left, &right, kind);
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

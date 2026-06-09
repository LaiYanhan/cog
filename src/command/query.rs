use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::{AssertionStatus, QueryCard};
use crate::format::{self, OutputFormat, TextRenderer};
use crate::repo::Repository;

pub fn execute(
    repo: &dyn Repository,
    entity: &str,
    all: bool,
    compact: bool,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let entity_record = repo.resolve_entity(entity)?;
    let assertions = repo.get_assertions_for_entity(&entity_record.id)?;
    let assertions_with_evidence = assertions
        .into_iter()
        .filter(|a| all || a.status == AssertionStatus::Active)
        .map(|assertion| {
            let evidences = repo.get_evidence_for_assertion(&assertion.id)?;
            Ok((assertion, evidences))
        })
        .collect::<Result<Vec<_>>>()?;

    if compact {
        let text = TextRenderer::query_compact(&entity_record, &assertions_with_evidence);
        Ok(CommandOutput::success(text))
    } else {
        let related = repo.get_related_entities(&entity_record.id)?;
        let card = QueryCard {
            entity: entity_record,
            assertions: assertions_with_evidence,
            related,
        };
        Ok(CommandOutput::success(format::emit_report(&card, output)))
    }
}

#[cfg(test)]
mod tests {
    use crate::repo::Repository;
    use anyhow::Result;

    use super::execute;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin, EntityRelationKind};
    use crate::format::OutputFormat;
    use crate::repo::SqliteRepository;

    #[test]
    fn returns_assertions_and_related_entities() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;

        let login =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        let token = store.upsert_entity("AuthToken", EntityKind::Type, EntityOrigin::Manual)?;
        store.add_entity_relation(&login.id, &token.id, EntityRelationKind::Uses)?;
        store.create_assertion(
            &login.id,
            AssertionKind::Contract,
            "returns option token",
            "code:auth::login",
            None,
        )?;

        let output = execute(&store, "auth::login", false, false, OutputFormat::Text)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("auth::login"));
        assert!(output.text.contains("returns option token"));
        assert!(output.text.contains("AuthToken"));
        Ok(())
    }
}

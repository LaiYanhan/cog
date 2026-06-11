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
    relations: bool,
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

        // Build assertion count map for related target entities —
        // enables the relation summary to distinguish blind vs asserted.
        let target_ids: Vec<String> = related.iter().map(|r| r.entity.id.clone()).collect();
        let all_related_assertions = repo.get_assertions_for_entities(&target_ids)?;
        let related_assertion_counts: std::collections::HashMap<String, usize> =
            all_related_assertions
                .iter()
                .filter(|a| a.status == AssertionStatus::Active)
                .fold(std::collections::HashMap::new(), |mut acc, a| {
                    *acc.entry(a.entity_id.clone()).or_insert(0) += 1;
                    acc
                });

        let card = QueryCard {
            entity: entity_record,
            assertions: assertions_with_evidence,
            related,
            related_assertion_counts,
            relations_detail: relations,
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

        let output = execute(
            &store,
            "auth::login",
            false,
            false,
            true,
            OutputFormat::Text,
        )?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("auth::login"));
        assert!(output.text.contains("returns option token"));
        assert!(output.text.contains("AuthToken"));
        Ok(())
    }
}

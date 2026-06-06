use anyhow::{Result, anyhow};

use crate::command::CommandOutput;
use crate::domain::grounds::Grounds;
use crate::domain::{AssertionKind, ChangelogAction, EntityKind, EntityOrigin, StatusMessage};
use crate::format::{self, OutputFormat};
use crate::repo::Repository;

pub fn execute(
    repo: &dyn Repository,
    entity: &str,
    kind: AssertionKind,
    claim: &str,
    grounds: &str,
    depends_on: Option<&str>,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let resolved_depends_on = depends_on
        .map(|id| repo.resolve_assertion_id(id))
        .transpose()?;

    if let Some(dependency_id) = &resolved_depends_on {
        let dependency = repo
            .get_assertion(dependency_id)?
            .ok_or_else(|| anyhow!("depends-on assertion not found: {dependency_id}"))?;
        if dependency.id != *dependency_id {
            return Err(anyhow!("unexpected dependency lookup mismatch"));
        }
    }

    // Validate grounds format before storing
    Grounds::parse(grounds).validate_format()?;

    let entity_record =
        repo.upsert_entity(entity, EntityKind::infer(entity), EntityOrigin::Manual)?;
    let assertion = repo.create_assertion(
        &entity_record.id,
        kind,
        claim,
        grounds,
        resolved_depends_on.as_deref(),
    )?;

    repo.append_changelog(
        ChangelogAction::Assert,
        &assertion.id,
        &format!("entity={entity} kind={kind} claim={claim}"),
    )?;

    let msg = format::assertion_created(&assertion, &entity_record, resolved_depends_on.as_deref());
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
    use crate::domain::AssertionKind;
    use crate::format::OutputFormat;
    use crate::repo::SqliteRepository;

    #[test]
    fn creates_assertion_and_evidence() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;

        let output = execute(
            &store,
            "auth::login",
            AssertionKind::Contract,
            "returns token on success",
            "code:auth::login",
            None,
            OutputFormat::Text,
        )?;

        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("assertion created"));

        let entity = store
            .get_entity_by_name("auth::login")?
            .expect("entity should exist");
        let assertions = store.get_assertions_for_entity(&entity.id)?;
        assert_eq!(assertions.len(), 1);

        let evidences = store.get_evidence_for_assertion(&assertions[0].id)?;
        assert_eq!(evidences.len(), 1);
        Ok(())
    }
}

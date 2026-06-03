use anyhow::{Result, anyhow};

use crate::command::CommandOutput;
use crate::format;
use crate::model::{AssertionStatus, Store};

pub fn execute(store: &Store, entity: &str, all: bool) -> Result<CommandOutput> {
    let entity_record = store
        .get_entity_by_name(entity)?
        .ok_or_else(|| anyhow!("entity not found: {entity}"))?;
    let assertions = store.get_assertions_for_entity(&entity_record.id)?;
    let assertions_with_evidence = assertions
        .into_iter()
        .filter(|a| all || a.status == AssertionStatus::Active)
        .map(|assertion| {
            let evidences = store.get_evidence_for_assertion(&assertion.id)?;
            Ok((assertion, evidences))
        })
        .collect::<Result<Vec<_>>>()?;
    let related = store.get_related_entities(&entity_record.id)?;

    Ok(CommandOutput::success(format::query_report(
        &entity_record,
        &assertions_with_evidence,
        &related,
    )))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::model::{AssertionKind, EntityKind, EntityOrigin, EntityRelationKind, Store};

    #[test]
    fn returns_assertions_and_related_entities() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;

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

        let output = execute(&store, "auth::login", false)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("entity: auth::login"));
        assert!(output.text.contains("returns option token"));
        assert!(output.text.contains("AuthToken"));
        Ok(())
    }
}

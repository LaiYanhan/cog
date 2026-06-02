use anyhow::{Result, anyhow};

use crate::command::{CommandOutput, infer_entity_kind};
use crate::model::{AssertionKind, Changelog, ChangelogAction, Store};

pub fn execute(
    store: &Store,
    entity: &str,
    kind: AssertionKind,
    claim: &str,
    grounds: &str,
    depends_on: Option<&str>,
) -> Result<CommandOutput> {
    let resolved_depends_on = depends_on
        .map(|id| store.resolve_assertion_id(id))
        .transpose()?;

    if let Some(ref dependency_id) = resolved_depends_on {
        let dependency = store
            .get_assertion(dependency_id)?
            .ok_or_else(|| anyhow!("depends-on assertion not found: {dependency_id}"))?;
        if dependency.id != *dependency_id {
            return Err(anyhow!("unexpected dependency lookup mismatch"));
        }
    }

    let entity_record = store.upsert_entity(entity, infer_entity_kind(entity))?;
    let assertion = store.create_assertion(&entity_record.id, kind, claim, grounds, resolved_depends_on.as_deref())?;

    Changelog::append(
        store,
        ChangelogAction::Assert,
        &assertion.id,
        &format!("entity={} kind={} claim={}", entity, kind, claim),
    )?;

    let mut out = String::new();
    out.push_str("assertion created\n");
    out.push_str(&format!("- id: {} ({})\n", crate::format::short_id(&assertion.id), assertion.id));
    out.push_str(&format!("- entity: {}\n", entity_record.qualified_name));
    out.push_str(&format!("- kind: {}\n", assertion.kind));
    out.push_str(&format!("- claim: {}\n", assertion.claim));
    if let Some(ref dep) = resolved_depends_on {
        out.push_str(&format!("- depends_on: {}\n", crate::format::short_id(dep)));
    }

    Ok(CommandOutput::success(out))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::model::{AssertionKind, Store};

    #[test]
    fn creates_assertion_and_evidence() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;

        let output = execute(
            &store,
            "auth::login",
            AssertionKind::Contract,
            "returns token on success",
            "code:src/auth.rs:10",
            None,
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

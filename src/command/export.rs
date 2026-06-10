use std::fmt::Write;

use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::{ExportFormat, ModelSnapshot};
use crate::repo::Repository;

pub fn execute(repo: &dyn Repository, format: ExportFormat) -> Result<CommandOutput> {
    let snapshot = ModelSnapshot {
        entities: repo.list_entities()?,
        assertions: repo.list_assertions()?,
        evidences: repo.list_evidences()?,
        entity_relations: repo.list_entity_relations()?,
        assertion_relations: repo.list_assertion_relations()?,
        changelog: repo.list_changelog_entries()?,
    };

    let text = match format {
        ExportFormat::Json => serde_json::to_string_pretty(&snapshot)?,
        ExportFormat::Toml => toml::to_string_pretty(&snapshot)?,
        ExportFormat::Dot => snapshot_to_dot(&snapshot),
    };

    Ok(CommandOutput::success(text))
}

fn snapshot_to_dot(snapshot: &ModelSnapshot) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "digraph cognitive_model {{");
    let _ = writeln!(out, "  rankdir=LR;");

    for entity in &snapshot.entities {
        let _ = writeln!(
            out,
            "  \"entity:{}\" [shape=box,label=\"{}\\n{}\"];",
            entity.id, entity.qualified_name, entity.kind
        );
    }

    for assertion in &snapshot.assertions {
        let _ = writeln!(
            out,
            "  \"assertion:{}\" [shape=ellipse,label=\"{}\\n{}\"];",
            assertion.id,
            assertion.kind,
            assertion.claim.replace('"', "\\\"")
        );
        let _ = writeln!(
            out,
            "  \"entity:{}\" -> \"assertion:{}\" [label=\"has\"];",
            assertion.entity_id, assertion.id
        );
    }

    for relation in &snapshot.entity_relations {
        let _ = writeln!(
            out,
            "  \"entity:{}\" -> \"entity:{}\" [label=\"{}\"];",
            relation.from_entity, relation.to_entity, relation.kind
        );
    }

    for relation in &snapshot.assertion_relations {
        let _ = writeln!(
            out,
            "  \"assertion:{}\" -> \"assertion:{}\" [label=\"{}\"];",
            relation.from_assertion, relation.to_assertion, relation.kind
        );
    }

    let _ = writeln!(out, "}}");
    out
}

#[cfg(test)]
mod tests {
    use crate::repo::Repository;

    use anyhow::Result;

    use super::execute;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin, ExportFormat};
    use crate::repo::SqliteRepository;

    #[test]
    fn exports_json_snapshot() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns token",
            "code:auth::login",
            None,
        )?;

        let output = execute(&store, ExportFormat::Json)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("\"entities\""));
        assert!(output.text.contains("auth::login"));
        Ok(())
    }

    #[test]
    fn exports_dot_snapshot() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        let entity =
            store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns token",
            "code:auth::login",
            None,
        )?;

        let output = execute(&store, ExportFormat::Dot)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("digraph cognitive_model"));
        assert!(output.text.contains("has"));
        Ok(())
    }
}

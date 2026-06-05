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
    let mut out = String::from("digraph cognitive_model {\n");
    out.push_str("  rankdir=LR;\n");

    for entity in &snapshot.entities {
        out.push_str(&format!(
            "  \"entity:{}\" [shape=box,label=\"{}\\n{}\"];\n",
            entity.id, entity.qualified_name, entity.kind
        ));
    }

    for assertion in &snapshot.assertions {
        out.push_str(&format!(
            "  \"assertion:{}\" [shape=ellipse,label=\"{}\\n{}\"];\n",
            assertion.id,
            assertion.kind,
            assertion.claim.replace('"', "\\\"")
        ));
        out.push_str(&format!(
            "  \"entity:{}\" -> \"assertion:{}\" [label=\"has\"];\n",
            assertion.entity_id, assertion.id
        ));
    }

    for relation in &snapshot.entity_relations {
        out.push_str(&format!(
            "  \"entity:{}\" -> \"entity:{}\" [label=\"{}\"];\n",
            relation.from_entity, relation.to_entity, relation.kind
        ));
    }

    for relation in &snapshot.assertion_relations {
        out.push_str(&format!(
            "  \"assertion:{}\" -> \"assertion:{}\" [label=\"{}\"];\n",
            relation.from_assertion, relation.to_assertion, relation.kind
        ));
    }

    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin, ExportFormat};
    use crate::repo::SqliteRepository;

    #[test]
    fn exports_json_snapshot() -> Result<()> {
        let tmp = tempdir()?;
        let store = SqliteRepository::open(&tmp.path().join("cog.db"))?;
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
        let tmp = tempdir()?;
        let store = SqliteRepository::open(&tmp.path().join("cog.db"))?;
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

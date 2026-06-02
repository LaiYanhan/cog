use anyhow::Result;
use serde::Serialize;

use crate::command::CommandOutput;
use crate::model::{ExportFormat, Store};

#[derive(Debug, Serialize)]
struct ExportSnapshot {
    entities: Vec<crate::model::Entity>,
    assertions: Vec<crate::model::Assertion>,
    evidences: Vec<crate::model::Evidence>,
    entity_relations: Vec<crate::model::EntityRelation>,
    assertion_relations: Vec<crate::model::AssertionRelation>,
    changelog: Vec<crate::model::ChangelogEntry>,
}

pub fn execute(store: &Store, format: ExportFormat) -> Result<CommandOutput> {
    let snapshot = ExportSnapshot {
        entities: store.list_entities()?,
        assertions: store.list_assertions()?,
        evidences: store.list_evidences()?,
        entity_relations: store.list_entity_relations()?,
        assertion_relations: store.list_assertion_relations()?,
        changelog: store.list_changelog_entries()?,
    };

    let text = match format {
        ExportFormat::Json => serde_json::to_string_pretty(&snapshot)?,
        ExportFormat::Toml => toml::to_string_pretty(&snapshot)?,
        ExportFormat::Dot => snapshot_to_dot(&snapshot),
    };

    Ok(CommandOutput::success(text))
}

fn snapshot_to_dot(snapshot: &ExportSnapshot) -> String {
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
    use crate::model::{AssertionKind, EntityKind, ExportFormat, Store};

    #[test]
    fn exports_json_snapshot() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let entity = store.upsert_entity("auth::login", EntityKind::Function)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns token",
            "code:src/auth.rs:1",
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
        let store = Store::open(&tmp.path().join("cog.db"))?;
        let entity = store.upsert_entity("auth::login", EntityKind::Function)?;
        store.create_assertion(
            &entity.id,
            AssertionKind::Contract,
            "returns token",
            "code:src/auth.rs:1",
            None,
        )?;

        let output = execute(&store, ExportFormat::Dot)?;
        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("digraph cognitive_model"));
        assert!(output.text.contains("has"));
        Ok(())
    }
}

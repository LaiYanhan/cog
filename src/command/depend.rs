use anyhow::Result;

use crate::command::{CommandOutput, infer_entity_kind};
use crate::model::{Changelog, ChangelogAction, EntityRelationKind, Store};

pub fn execute(
    store: &Store,
    entity_a: &str,
    entity_b: &str,
    kind: EntityRelationKind,
) -> Result<CommandOutput> {
    let left = store.upsert_entity(
        entity_a,
        infer_entity_kind(entity_a),
        crate::model::EntityOrigin::Manual,
    )?;
    let right = store.upsert_entity(
        entity_b,
        infer_entity_kind(entity_b),
        crate::model::EntityOrigin::Manual,
    )?;

    store.add_entity_relation(&left.id, &right.id, kind)?;
    Changelog::append(
        store,
        ChangelogAction::Depend,
        &left.id,
        &format!(
            "from={} to={} kind={}",
            left.qualified_name, right.qualified_name, kind
        ),
    )?;

    Ok(CommandOutput::success(format!(
        "dependency recorded\n- from: {}\n- to: {}\n- kind: {}",
        left.qualified_name, right.qualified_name, kind
    )))
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use tempfile::tempdir;

    use super::execute;
    use crate::model::{EntityRelationKind, Store};

    #[test]
    fn records_entity_dependency() -> Result<()> {
        let tmp = tempdir()?;
        let store = Store::open(&tmp.path().join("cog.db"))?;

        let output = execute(&store, "auth::login", "AuthToken", EntityRelationKind::Uses)?;

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

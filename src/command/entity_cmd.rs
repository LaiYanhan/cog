use anyhow::Result;

use crate::command::CommandOutput;
use crate::model::{Changelog, ChangelogAction, Store};

pub fn execute(store: &Store, qualified_name: &str) -> Result<CommandOutput> {
    let entity = store.get_entity_by_name(qualified_name)?;
    let entity = match entity {
        Some(e) => e,
        None => {
            return Ok(CommandOutput::with_exit_code(
                format!("entity not found: {qualified_name}"),
                1,
            ));
        }
    };

    let assertion_count = store.get_assertions_for_entity(&entity.id)?.len();
    let relation_count = store.count_relations_for_entity(&entity.id)? as usize;

    let mut details = String::new();
    details.push_str(&format!(
        "deleting entity: {} [{}]\n",
        entity.qualified_name, entity.kind
    ));
    if assertion_count > 0 {
        details.push_str(&format!("  assertions: {}\n", assertion_count));
    }
    if relation_count > 0 {
        details.push_str(&format!("  relations: {}\n", relation_count));
    }

    let deleted = store.delete_entity(qualified_name)?;
    if !deleted {
        return Ok(CommandOutput::with_exit_code(
            format!("entity not found: {qualified_name}"),
            1,
        ));
    }

    Changelog::append(
        store,
        ChangelogAction::DeleteEntity,
        &entity.id,
        &format!(
            "deleted entity {} with {} assertions, {} relations",
            qualified_name, assertion_count, relation_count
        ),
    )?;

    details.push_str("entity deleted");
    Ok(CommandOutput::success(details))
}

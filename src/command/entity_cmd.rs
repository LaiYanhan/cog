use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::ChangelogAction;
use crate::repo::Repository;

pub fn execute(repo: &dyn Repository, qualified_name: &str) -> Result<CommandOutput> {
    let entity = repo.get_entity_by_name(qualified_name)?;
    let entity = match entity {
        Some(e) => e,
        None => {
            return Ok(CommandOutput::with_exit_code(
                format!("entity not found: {qualified_name}"),
                1,
            ));
        }
    };

    let assertion_count = repo.get_assertions_for_entity(&entity.id)?.len();
    let relation_count = repo.count_relations_for_entity(&entity.id)? as usize;

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

    let deleted = repo.delete_entity(qualified_name)?;
    if !deleted {
        return Ok(CommandOutput::with_exit_code(
            format!("entity not found: {qualified_name}"),
            1,
        ));
    }

    repo.append_changelog(
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

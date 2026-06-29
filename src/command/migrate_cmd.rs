use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::ChangelogAction;
use crate::repo::Repository;

/// `cog migrate <from> <to>` — re-assign every assertion and entity relation
/// from the `from` entity onto the `to` entity, then delete `from`.
///
/// Reconciles the design/code namespace split: design-phase assertions recorded
/// against a Manual entity (e.g. `minilang::lexer::Lexer`) become orphans once
/// sync produces the real path-named entity (e.g. `src::lexer::Lexer`). Migrate
/// moves the knowledge onto the entity the code actually lives under.
pub fn execute(repo: &dyn Repository, from: &str, to: &str) -> Result<CommandOutput> {
    if from == to {
        return Ok(CommandOutput::with_exit_code(
            format!("source and target are the same: {from}"),
            1,
        ));
    }

    let src = match repo.resolve_entity(from) {
        Ok(e) => e,
        Err(e) => {
            return Ok(CommandOutput::with_exit_code(
                format!("source entity not found: {from}\n{e:#}"),
                1,
            ));
        }
    };
    let dst = match repo.resolve_entity(to) {
        Ok(e) => e,
        Err(e) => {
            return Ok(CommandOutput::with_exit_code(
                format!("target entity not found: {to}\n{e:#}"),
                1,
            ));
        }
    };
    if src.id == dst.id {
        return Ok(CommandOutput::with_exit_code(
            format!("source and target resolve to the same entity: {from}"),
            1,
        ));
    }

    let (moved_assertions, moved_relations) = repo.transfer_entity(&src.id, &dst.id)?;

    repo.append_changelog(
        ChangelogAction::Migrate,
        &dst.id,
        &format!(
            "migrated {} -> {} ({} assertions, {} relations); source deleted",
            src.qualified_name, dst.qualified_name, moved_assertions, moved_relations
        ),
    )?;

    let mut text = String::new();
    text.push_str(&format!(
        "Migrated {} assertion(s) and {} relation(s):\n  {} -> {}\n",
        moved_assertions, moved_relations, src.qualified_name, dst.qualified_name
    ));
    text.push_str("Source entity deleted.\n");
    text.push_str(&format!("Next: cog query {}", dst.qualified_name));
    Ok(CommandOutput::success(text))
}

use std::fmt::Write;

use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::grounds::Grounds;
use crate::domain::{AssertionKind, ChangelogAction, StatusMessage};
use crate::format::TextRenderer;
use crate::format::{self, OutputFormat};
use crate::repo::Repository;
use crate::space::CascadeEngine;
/// Bundled input for `execute`.
pub struct AssertInput<'a> {
    pub entity: &'a str,
    pub kind: AssertionKind,
    pub claim: &'a str,
    pub grounds: &'a str,
    pub depends_on: Option<&'a str>,
    /// Specific assertion ID to retract before creating this one.
    pub replace_id: Option<&'a str>,
    /// Create alongside existing active assertions (for different aspects).
    pub force: bool,
    pub output: OutputFormat,
}
/// Gate: if the entity already has active assertions of `kind`, require
/// `--replace <id>` or `--force`. Returns the IDs of any auto-retracted assertions.
/// Returns `Err(output)` with exit code 1 when blocked; caller should return it.
fn enforce_kind_gate(
    repo: &dyn Repository,
    entity: &crate::domain::Entity,
    kind: AssertionKind,
    replace_id: Option<&str>,
    force: bool,
) -> Result<Vec<String>, CommandOutput> {
    let existing = repo
        .get_assertions_for_entity(&entity.id)
        .map_err(|e| CommandOutput::with_exit_code(e.to_string(), 1))?;
    let active_same_kind: Vec<&crate::domain::Assertion> = existing
        .iter()
        .filter(|a| a.is_active() && a.kind == kind)
        .collect();

    if active_same_kind.is_empty() {
        return Ok(Vec::new());
    }

    if let Some(id) = replace_id {
        // Retract the specific assertion by ID.
        let resolved = repo
            .resolve_assertion_id(id)
            .map_err(|e| CommandOutput::with_exit_code(e.to_string(), 1))?;
        let target = repo
            .get_assertion(&resolved)
            .map_err(|e| CommandOutput::with_exit_code(e.to_string(), 1))?
            .ok_or_else(|| {
                CommandOutput::with_exit_code(format!("assertion not found: {id}"), 1)
            })?;
        if !target.is_active() {
            return Err(CommandOutput::with_exit_code(
                format!("assertion {id} is already retracted"),
                1,
            ));
        }
        if target.entity_id != entity.id || target.kind != kind {
            return Err(CommandOutput::with_exit_code(
                format!(
                    "assertion {id} is not a {} assertion on {}",
                    kind, entity.qualified_name
                ),
                1,
            ));
        }
        let reason = format!("replaced by newer {kind} assertion");
        // Use cascade engine so dependents of the replaced assertion are notified
        CascadeEngine::retract(repo, &target.id, &reason)
            .map_err(|e| CommandOutput::with_exit_code(e.to_string(), 1))?;
        return Ok(vec![target.id.clone()]);
    }

    if force {
        return Ok(Vec::new());
    }

    // Blocked — list existing and require --replace <id> or --force.
    let plural = if active_same_kind.len() == 1 { "" } else { "s" };
    let mut msg = format!(
        "{} already has {} active {} assertion{}:\n",
        entity.qualified_name,
        active_same_kind.len(),
        kind,
        plural
    );
    for a in &active_same_kind {
        let _ = std::fmt::Write::write_fmt(
            &mut msg,
            format_args!("  {}: \"{}\"\n", crate::domain::short_id(&a.id), a.claim),
        );
    }
    msg.push_str("Avoid contradictory assertions on the same entity.\n");
    msg.push_str(&format!(
        "  --replace {}   Replace that one and create this new assertion\n",
        crate::domain::short_id(&active_same_kind[0].id)
    ));
    msg.push_str("  --force          Create alongside (only if this describes a different aspect)");
    Err(CommandOutput::with_exit_code(msg, 1))
}

fn format_created_message(
    assertion: &crate::domain::Assertion,
    entity: &crate::domain::Entity,
    existing: &[(crate::domain::Assertion, Vec<crate::domain::Evidence>)],
    same_kind_count: usize,
    retracted_ids: &[String],
) -> String {
    if retracted_ids.is_empty() {
        return TextRenderer::assertion_created(assertion, entity, existing, same_kind_count);
    }
    let mut msg = format!(
        "Created {} [{}] on {}\n  \"{}\"\n\n",
        crate::domain::short_id(&assertion.id),
        assertion.kind,
        entity.qualified_name,
        assertion.claim
    );
    for rid in retracted_ids {
        let _ = writeln!(
            &mut msg,
            "Auto-retracted {} (replaced).",
            crate::domain::short_id(rid)
        );
    }
    msg
}

pub fn execute(repo: &dyn Repository, input: AssertInput<'_>) -> Result<CommandOutput> {
    // Validate dependency, if any.
    if let Some(dep_id) = input.depends_on {
        let resolved = repo.resolve_assertion_id(dep_id)?;
        if repo.get_assertion(&resolved)?.is_none() {
            anyhow::bail!("dependency assertion {dep_id} not found");
        }
    }

    Grounds::parse(input.grounds).validate_format()?;

    let entity = match repo.resolve_entity(input.entity) {
        Ok(e) => e,
        Err(_) => repo.ensure_manual_entity(input.entity)?,
    };

    let retracted_ids =
        match enforce_kind_gate(repo, &entity, input.kind, input.replace_id, input.force) {
            Ok(ids) => ids,
            Err(output) => return Ok(output),
        };

    let resolved_depends_on = input
        .depends_on
        .map(|id| repo.resolve_assertion_id(id))
        .transpose()?;

    let assertion = repo.create_assertion(
        &entity.id,
        input.kind,
        input.claim,
        input.grounds,
        resolved_depends_on.as_deref(),
    )?;

    repo.append_changelog(
        ChangelogAction::Assert,
        &assertion.id,
        &format!(
            "entity={} kind={} claim={}",
            input.entity, input.kind, input.claim
        ),
    )?;

    let raw = repo.get_assertions_for_entity(&entity.id)?;
    let existing: Vec<_> = raw
        .iter()
        .map(|a| {
            let ev = repo.get_evidence_for_assertion(&a.id)?;
            Ok((a.clone(), ev))
        })
        .collect::<Result<_>>()?;
    let same_kind = existing
        .iter()
        .filter(|(a, _)| a.is_active() && a.kind == input.kind)
        .count();

    let mut msg = format_created_message(&assertion, &entity, &existing, same_kind, &retracted_ids);

    // Density warning: if entity participates in relations and model coverage is low, nudge.
    if let Ok(rel_count) = repo.count_relations_for_entity(&entity.id)
        && rel_count > 0
        && let Ok(stats) = repo.stats()
        && stats.entities > 0
    {
        let coverage = stats.covered_entities as f64 / stats.entities as f64;
        if coverage < 0.5 {
            let pct = (coverage * 100.0) as u32;
            let _ = writeln!(
                msg,
                "\nWarning: only {pct}% of entities have assertions. Consider: cog impact {}",
                input.entity
            );
        }
    }

    Ok(CommandOutput::success(format::emit_report(
        &StatusMessage { message: msg },
        input.output,
    )))
}

#[cfg(test)]
mod tests {
    use crate::domain::AssertionKind;
    use crate::format::OutputFormat;
    use crate::repo::Repository;
    use crate::repo::SqliteRepository;
    use anyhow::Result;

    use super::{AssertInput, execute};

    #[test]
    fn creates_assertion_and_evidence() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;

        let output = execute(
            &store,
            AssertInput {
                entity: "auth::login",
                kind: AssertionKind::Contract,
                claim: "returns token on success",
                grounds: "code:auth::login",
                depends_on: None,
                replace_id: None,
                force: false,
                output: OutputFormat::Text,
            },
        )?;

        assert_eq!(output.exit_code, 0);
        assert!(
            output.text.contains("Created"),
            "expected 'Created' in: {}",
            output.text
        );

        let entity = store
            .get_entity_by_name("auth::login")?
            .expect("entity should exist");
        let assertions = store.get_assertions_for_entity(&entity.id)?;
        assert_eq!(assertions.len(), 1);

        let evidences = store.get_evidence_for_assertion(&assertions[0].id)?;
        assert_eq!(evidences.len(), 1);
        Ok(())
    }

    #[test]
    fn rejects_duplicate_kind_without_flag() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;

        // First assertion succeeds.
        execute(
            &store,
            AssertInput {
                entity: "auth::login",
                kind: AssertionKind::Contract,
                claim: "returns token",
                grounds: "code:auth::login",
                depends_on: None,
                replace_id: None,
                force: false,
                output: OutputFormat::Text,
            },
        )?;

        // Second assertion same kind, no flag → rejected.
        let output = execute(
            &store,
            AssertInput {
                entity: "auth::login",
                kind: AssertionKind::Contract,
                claim: "returns token on success",
                grounds: "code:auth::login",
                depends_on: None,
                replace_id: None,
                force: false,
                output: OutputFormat::Text,
            },
        )?;

        assert_eq!(output.exit_code, 1);
        assert!(
            output
                .text
                .contains("already has 1 active contract assertion")
        );
        assert!(output.text.contains("--replace"));
        Ok(())
    }

    #[test]
    fn replace_retracts_old_creates_new() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;

        // First assertion.
        execute(
            &store,
            AssertInput {
                entity: "auth::login",
                kind: AssertionKind::Contract,
                claim: "returns token",
                grounds: "code:auth::login",
                depends_on: None,
                replace_id: None,
                force: false,
                output: OutputFormat::Text,
            },
        )?;

        // Capture the first assertion's ID for --replace use.
        let entity = store
            .get_entity_by_name("auth::login")?
            .expect("entity should exist");
        let assertions = store.get_assertions_for_entity(&entity.id)?;
        let first_id = assertions[0].id.clone();

        // Replace it.
        let output = execute(
            &store,
            AssertInput {
                entity: "auth::login",
                kind: AssertionKind::Contract,
                claim: "returns token on success",
                grounds: "code:auth::login",
                depends_on: None,
                replace_id: Some(&first_id),
                force: false,
                output: OutputFormat::Text,
            },
        )?;

        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("Auto-retracted"));

        // Only one active assertion remains.
        let entity = store
            .get_entity_by_name("auth::login")?
            .expect("entity should exist");
        let assertions = store.get_assertions_for_entity(&entity.id)?;
        let active: Vec<_> = assertions.iter().filter(|a| a.is_active()).collect();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].claim, "returns token on success");
        Ok(())
    }

    #[test]
    fn force_allows_duplicate_kind() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;

        // First assertion.
        execute(
            &store,
            AssertInput {
                entity: "auth::login",
                kind: AssertionKind::Fragility,
                claim: "slow with large inputs",
                grounds: "code:auth::login",
                depends_on: None,
                replace_id: None,
                force: false,
                output: OutputFormat::Text,
            },
        )?;

        // Second assertion same kind with --force.
        let output = execute(
            &store,
            AssertInput {
                entity: "auth::login",
                kind: AssertionKind::Fragility,
                claim: "thread-unsafe",
                grounds: "code:auth::login",
                depends_on: None,
                replace_id: None,
                force: true,
                output: OutputFormat::Text,
            },
        )?;

        assert_eq!(output.exit_code, 0);
        assert!(output.text.contains("WARNING"));

        // Both active.
        let entity = store
            .get_entity_by_name("auth::login")?
            .expect("entity should exist");
        let assertions = store.get_assertions_for_entity(&entity.id)?;
        let active: Vec<_> = assertions.iter().filter(|a| a.is_active()).collect();
        assert_eq!(active.len(), 2);
        Ok(())
    }
}

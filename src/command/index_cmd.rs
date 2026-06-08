use std::collections::HashMap;

use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::{
    Entity, EntityIndex, EntityKind, EntityOrigin, IndexCoverage, ModuleCoverage, TopUncovered,
};
use crate::format::{self, OutputFormat};
use crate::repo::Repository;

/// Build a coverage summary from a (entity, assertion_count) list.
fn build_coverage_summary(all: &[(Entity, usize)]) -> IndexCoverage {
    let total = all.len();
    let covered = all.iter().filter(|(_, n)| *n > 0).count();
    let pct = if total > 0 {
        (covered as f64) / (total as f64) * 100.0
    } else {
        100.0
    };

    // Group by top-level module prefix (first :: segment)
    let mut modules: HashMap<String, (usize, usize)> = HashMap::new();
    for (entity, count) in all {
        let prefix = entity
            .qualified_name
            .split("::")
            .next()
            .unwrap_or("")
            .to_string();
        let entry = modules.entry(prefix).or_insert((0, 0));
        entry.1 += 1;
        if *count > 0 {
            entry.0 += 1;
        }
    }

    let mut module_list: Vec<ModuleCoverage> = modules
        .into_iter()
        .map(|(path, (cvd, tot))| ModuleCoverage {
            path,
            covered: cvd,
            total: tot,
        })
        .collect();
    module_list.sort_by(|a, b| {
        (b.total - b.covered)
            .cmp(&(a.total - a.covered))
            .then_with(|| a.path.cmp(&b.path))
    });

    // Top uncovered: entities with 0 assertions, sorted by fan_out descending
    let mut uncovered: Vec<&Entity> = all
        .iter()
        .filter(|(_, n)| *n == 0)
        .map(|(e, _)| e)
        .collect();
    uncovered.sort_by_key(|e| -(e.metrics.fan_out.unwrap_or(0) as i64));

    let top_uncovered: Vec<TopUncovered> = uncovered
        .into_iter()
        .take(10)
        .map(|e| TopUncovered {
            entity_name: e.qualified_name.clone(),
            entity_kind: e.kind.to_string(),
            assertions: 0,
            dependents: e.metrics.fan_out.unwrap_or(0) as usize,
        })
        .collect();

    IndexCoverage {
        covered,
        total,
        pct,
        modules: module_list,
        top_uncovered,
    }
}

pub fn execute(
    repo: &dyn Repository,
    kind: Option<EntityKind>,
    origin: Option<EntityOrigin>,
    prefix: Option<&str>,
    verbose: bool,
    uncovered: bool,
    output: OutputFormat,
) -> Result<CommandOutput> {
    let all_entities = repo.list_entities_filtered(kind, origin, prefix)?;

    // --uncovered: filter to only entities without assertions
    if uncovered {
        let filtered: Vec<_> = all_entities.into_iter().filter(|(_, n)| *n == 0).collect();
        let report = EntityIndex {
            entities: filtered,
            summary_mode: false,
            coverage_summary: None,
        };
        return Ok(CommandOutput::success(format::emit_report(&report, output)));
    }

    // --verbose or any explicit filter: full listing
    if verbose || kind.is_some() || origin.is_some() || prefix.is_some() {
        let report = EntityIndex {
            entities: all_entities,
            summary_mode: false,
            coverage_summary: None,
        };
        return Ok(CommandOutput::success(format::emit_report(&report, output)));
    }

    // Default: summary mode
    let coverage = build_coverage_summary(&all_entities);
    let report = EntityIndex {
        entities: all_entities,
        summary_mode: true,
        coverage_summary: Some(coverage),
    };
    Ok(CommandOutput::success(format::emit_report(&report, output)))
}

#[cfg(test)]
mod tests {
    use crate::repo::Repository;
    use anyhow::Result;

    use super::execute;
    use crate::domain::{AssertionKind, EntityKind, EntityOrigin};
    use crate::format::OutputFormat;
    use crate::repo::SqliteRepository;

    #[test]
    fn summary_mode_is_default() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Scan)?;
        store.upsert_entity("auth::logout", EntityKind::Function, EntityOrigin::Scan)?;
        let output = execute(&store, None, None, None, false, false, OutputFormat::Text)?;
        assert!(output.text.contains("Coverage"));
        assert!(output.text.contains("By module"));
        Ok(())
    }

    #[test]
    fn verbose_restores_full_listing() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Scan)?;
        let output = execute(
            &store,
            None,
            None,
            None,
            true, // verbose
            false,
            OutputFormat::Text,
        )?;
        assert!(output.text.contains("auth::login"));
        Ok(())
    }

    #[test]
    fn uncovered_filters_zero_assertions() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        store.upsert_entity("covered", EntityKind::Function, EntityOrigin::Manual)?;
        let covered = store.get_entity_by_name("covered")?.unwrap();
        store.create_assertion(
            &covered.id,
            AssertionKind::Contract,
            "does stuff",
            "code:covered",
            None,
        )?;
        store.upsert_entity("bare", EntityKind::Function, EntityOrigin::Manual)?;
        let output = execute(
            &store,
            None,
            None,
            None,
            false,
            true, // uncovered
            OutputFormat::Text,
        )?;
        assert!(output.text.contains("bare"));
        assert!(!output.text.contains("covered"));
        Ok(())
    }

    #[test]
    fn filters_by_prefix() -> Result<()> {
        let store = SqliteRepository::open_in_memory()?;
        store.upsert_entity("auth::login", EntityKind::Function, EntityOrigin::Manual)?;
        store.upsert_entity("db::connect", EntityKind::Function, EntityOrigin::Manual)?;
        let output = execute(
            &store,
            None,
            None,
            Some("auth"),
            false,
            false,
            OutputFormat::Text,
        )?;
        assert!(output.text.contains("auth::login"));
        assert!(!output.text.contains("db::connect"));
        Ok(())
    }
}

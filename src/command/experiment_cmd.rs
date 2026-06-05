use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::AssertionKind;
use crate::experiment::{self, Experiment, ExperimentOp};
use crate::repo::Repository;

pub fn start(
    repo: &dyn Repository,
    entity: &str,
    description: Option<String>,
    max_nodes: usize,
    cog_dir: &std::path::Path,
) -> Result<CommandOutput> {
    let desc = description.unwrap_or_else(|| format!("experiment on {entity}"));
    let experiment = Experiment::start(repo, entity, desc, max_nodes)?;
    experiment::save(&experiment, cog_dir)?;
    let id_short = &experiment.id[..8];
    Ok(CommandOutput::success(format!(
        "experiment started: {} (focus: {})\n\
         loaded {} entities, {} assertions\n\
         boundary: {} entities\n\
         saved to disk — use 'cog experiment save --id {id_short}' to re-persist",
        experiment.id,
        experiment.entity_focus,
        experiment.entities.len(),
        experiment.assertions.len(),
        experiment.boundary_entities.len(),
    )))
}

pub fn hypothesize(
    id: &str,
    entity: &str,
    kind: AssertionKind,
    claim: &str,
    grounds: &str,
    cog_dir: &std::path::Path,
) -> Result<CommandOutput> {
    let mut experiment = experiment::load(id, cog_dir)?;
    let op = ExperimentOp::HypotheticalAssertion {
        entity_name: entity.to_string(),
        kind,
        claim: claim.to_string(),
        grounds: grounds.to_string(),
        depends_on: None,
    };
    experiment.hypothesize(op);
    experiment::save(&experiment, cog_dir)?;
    let id_short = &experiment.id[..8];
    Ok(CommandOutput::success(format!(
        "hypothesis added to experiment {id_short} ({} ops total)",
        experiment.ops.len()
    )))
}

pub fn evaluate(id: &str, cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let mut experiment = experiment::load(id, cog_dir)?;
    experiment.evaluate()?;
    let report = experiment.report();
    experiment::save(&experiment, cog_dir)?;
    let id_short = &experiment.id[..8];
    let mut text = format!(
        "experiment {id_short} evaluated\n\
         risk score: {:.2}\n\
         affected assertions: {}\n",
        report.risk_score, report.affected_count,
    );
    if report.contradictions.is_empty() {
        text.push_str("no contradictions detected\n");
    } else {
        text.push_str(&format!("{} contradictions:\n", report.contradictions.len()));
        for c in &report.contradictions {
            text.push_str(&format!(
                "  - new: {}\n    existing: {}\n    reason: {}\n",
                c.new_claim, c.existing_claim, c.reason,
            ));
        }
    }
    Ok(CommandOutput::success(text))
}

pub fn report(id: &str, cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let experiment = experiment::load(id, cog_dir)?;
    let report = experiment.report();
    let mut text = format!(
        "experiment {}\n\
         description: {}\n\
         focus: {}\n\
         status: {:?}\n\
         ops: {}\n\
         risk score: {:.2}\n\
         affected: {}\n",
        &experiment.id[..8],
        report.description,
        report.entity_focus,
        experiment.status,
        report.ops_count,
        report.risk_score,
        report.affected_count,
    );
    if report.contradictions.is_empty() {
        text.push_str("contradictions: none\n");
    } else {
        text.push_str(&format!("{} contradictions:\n", report.contradictions.len()));
        for c in &report.contradictions {
            text.push_str(&format!(
                "  - new: {}\n    existing: {}\n    reason: {}\n",
                c.new_claim, c.existing_claim, c.reason,
            ));
        }
    }
    if report.boundary_entities.is_empty() {
        text.push_str("boundary: none\n");
    } else {
        text.push_str(&format!("boundary: {} entities\n", report.boundary_entities.len()));
    }
    Ok(CommandOutput::success(text))
}

pub fn commit(
    repo: &dyn Repository,
    id: &str,
    cog_dir: &std::path::Path,
) -> Result<CommandOutput> {
    let mut experiment = experiment::load(id, cog_dir)?;
    let commit_report = experiment.commit(repo)?;
    experiment::remove(id, cog_dir)?;
    let mut text = format!(
        "experiment {} committed\n\
         applied: {} ops, skipped: {}\n",
        &id[..8],
        commit_report.ops_applied,
        commit_report.ops_skipped,
    );
    for detail in &commit_report.details {
        text.push_str(&format!("  {detail}\n"));
    }
    Ok(CommandOutput::success(text))
}

pub fn discard(id: &str, cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let experiment = experiment::load(id, cog_dir)?;
    let id_short = experiment.id[..8].to_string();
    experiment.discard();
    experiment::remove(id, cog_dir)?;
    Ok(CommandOutput::success(format!(
        "experiment {id_short} discarded"
    )))
}

pub fn list(cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let ids = experiment::list(cog_dir)?;
    if ids.is_empty() {
        return Ok(CommandOutput::success("no saved experiments"));
    }
    let mut text = format!("{} experiment(s):\n", ids.len());
    for id in &ids {
        let short = if id.len() >= 8 { &id[..8] } else { id };
        text.push_str(&format!("  {short} ({id})\n"));
    }
    Ok(CommandOutput::success(text))
}

pub fn save(id: &str, cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let experiment = experiment::load(id, cog_dir)?;
    experiment::save(&experiment, cog_dir)?;
    let id_short = &experiment.id[..8];
    Ok(CommandOutput::success(format!(
        "experiment {id_short} saved ({} ops)",
        experiment.ops.len()
    )))
}

pub fn load(id: &str, cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let experiment = experiment::load(id, cog_dir)?;
    let id_short = &experiment.id[..8];
    Ok(CommandOutput::success(format!(
        "experiment {id_short} loaded\n\
         description: {}\n\
         focus: {}\n\
         status: {:?}\n\
         ops: {}\n\
         entities: {}\n\
         assertions: {}",
        experiment.description,
        experiment.entity_focus,
        experiment.status,
        experiment.ops.len(),
        experiment.entities.len(),
        experiment.assertions.len(),
    )))
}

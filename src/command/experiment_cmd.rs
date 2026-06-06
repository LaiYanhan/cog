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
    // Auto-persist as unsaved draft so it survives across CLI invocations.
    experiment::save(&experiment, cog_dir)?;
    let id_short = &experiment.id[..8];
    Ok(CommandOutput::success(format!(
        "experiment started: {id_short} (focus: {})\n\
         loaded {} entities, {} assertions\n\
         boundary: {} entities\n\
         status: draft (unsaved)\n\
         use 'cog experiment save --id {id_short}' to checkpoint",
        experiment.entity_focus,
        experiment.structure.entities.len(),
        experiment.semantic.assertions.len(),
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
    let op = ExperimentOp::Assertion {
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

pub fn hypothesize_delete(
    id: &str,
    entity: &str,
    cog_dir: &std::path::Path,
) -> Result<CommandOutput> {
    let mut experiment = experiment::load(id, cog_dir)?;
    let op = ExperimentOp::Delete {
        entity_name: entity.to_string(),
    };
    experiment.hypothesize(op);
    experiment::save(&experiment, cog_dir)?;
    Ok(CommandOutput::success(format!(
        "staged hypothetical delete of entity {entity} on experiment {}",
        &id[..8.min(id.len())]
    )))
}

pub fn hypothesize_relation(
    id: &str,
    from: &str,
    to: &str,
    kind: crate::domain::EntityRelationKind,
    cog_dir: &std::path::Path,
) -> Result<CommandOutput> {
    let mut experiment = experiment::load(id, cog_dir)?;
    let op = ExperimentOp::Relation {
        from_entity: from.to_string(),
        to_entity: to.to_string(),
        kind,
    };
    experiment.hypothesize(op);
    experiment::save(&experiment, cog_dir)?;
    Ok(CommandOutput::success(format!(
        "staged hypothetical relation {from} -> {to} ({kind}) on experiment {}",
        &id[..8.min(id.len())]
    )))
}

pub fn evaluate(id: &str, cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let mut experiment = experiment::load(id, cog_dir)?;
    let report = experiment.evaluate()?;
    // Cache risk score for serialization and mark as evaluated
    experiment.risk_score = Some(report.risk_score);
    experiment.contradictions = report.contradictions.clone();
    experiment.mark_evaluated()?;
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
        text.push_str(&format!(
            "{} contradictions:\n",
            report.contradictions.len()
        ));
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
    let report = experiment.evaluate()?;
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
        text.push_str(&format!(
            "{} contradictions:\n",
            report.contradictions.len()
        ));
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
        text.push_str(&format!(
            "boundary: {} entities\n",
            report.boundary_entities.len()
        ));
    }
    Ok(CommandOutput::success(text))
}

pub fn commit(repo: &dyn Repository, id: &str, cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let experiment = experiment::load(id, cog_dir)?;
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
        return Ok(CommandOutput::success("no experiments"));
    }
    let mut text = format!("{} experiment(s):\n", ids.len());
    for id in &ids {
        let short = if id.len() >= 8 { &id[..8] } else { id };
        let saved_tag = match experiment::load(id, cog_dir) {
            Ok(exp) if exp.saved => " [saved]".to_string(),
            Ok(exp) => format!(" [draft, {:?}]", exp.status),
            Err(_) => String::new(),
        };
        text.push_str(&format!("  {short}{saved_tag}\n"));
    }
    Ok(CommandOutput::success(text))
}

pub fn save(id: &str, cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let mut experiment = experiment::load(id, cog_dir)?;
    let was_saved = experiment.saved;
    experiment.mark_saved();
    experiment::save(&experiment, cog_dir)?;
    let id_short = &experiment.id[..8];
    let status = if was_saved {
        "checkpoint updated"
    } else {
        "saved"
    };
    Ok(CommandOutput::success(format!(
        "experiment {id_short} {status} ({} ops, status: {:?})",
        experiment.ops.len(),
        experiment.status,
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
        experiment.structure.entities.len(),
        experiment.semantic.assertions.len(),
    )))
}

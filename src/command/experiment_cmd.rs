use anyhow::Result;

use crate::command::CommandOutput;
use crate::domain::{AssertionKind, ScoutAction, ScoutSuggestion};
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

/// Generate scout suggestions from an experiment evaluation report.
/// Sources: contradictions → [verify], blind entities → [assert], boundary entities → [read].
fn generate_scout_suggestions(
    report: &crate::experiment::report::ExperimentReport,
) -> Vec<ScoutSuggestion> {
    let mut scouts = Vec::new();
    for c in &report.contradictions {
        scouts.push(ScoutSuggestion {
            entity_name: report.entity_focus.clone(),
            entity_kind: "entity".to_string(),
            reason: format!("contradiction: {}", c.reason),
            action: ScoutAction::Verify,
        });
    }
    for name in &report.blind_entities {
        if !scouts.iter().any(|s| s.entity_name == *name) {
            scouts.push(ScoutSuggestion {
                entity_name: name.clone(),
                entity_kind: "entity".to_string(),
                reason: "blind (no assertions)".to_string(),
                action: ScoutAction::Assert,
            });
        }
    }
    for name in &report.boundary_entities {
        if !scouts.iter().any(|s| s.entity_name == *name) {
            scouts.push(ScoutSuggestion {
                entity_name: name.clone(),
                entity_kind: "entity".to_string(),
                reason: "boundary entity (partial data)".to_string(),
                action: ScoutAction::Read,
            });
        }
    }
    scouts
}

pub fn evaluate(id: &str, cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let mut experiment = experiment::load(id, cog_dir)?;
    let report = experiment.evaluate()?;
    experiment.risk_score = Some(report.risk_score);
    experiment.contradictions = report.contradictions.clone();
    experiment.mark_evaluated()?;
    experiment::save(&experiment, cog_dir)?;

    let id_short = &experiment.id[..8];
    let risk_label = if report.risk_score >= 0.8 {
        "HIGH"
    } else if report.risk_score >= 0.5 {
        "MEDIUM"
    } else {
        "LOW"
    };

    let mut text = format!(
        "experiment {id_short} evaluated: \"{}\"\n\n\
         Evaluation:\n  Risk: {risk_label} ({:.2})\n  Contradictions: {}\n  Affected assertions: {}\n  Cascade: {} assertions -> uncertain\n",
        experiment.description,
        report.risk_score,
        report.contradictions.len(),
        report.affected_count,
        report.cascade_count,
    );

    if !report.affected_assertions.is_empty() {
        text.push_str("\nAffected:\n");
        for a in &report.affected_assertions {
            let _ = std::fmt::Write::write_fmt(
                &mut text,
                format_args!("  - {}: \"{}\"\n", &a.assertion.id[..8], a.assertion.claim),
            );
        }
    }

    if !report.contradictions.is_empty() {
        text.push_str(&format!(
            "  {} contradictions:\n",
            report.contradictions.len()
        ));
        for c in &report.contradictions {
            text.push_str(&format!(
                "    - new: {}\n      existing: {}\n      reason: {}\n",
                c.new_claim, c.existing_claim, c.reason,
            ));
        }
    }

    let scouts = generate_scout_suggestions(&report);
    if !scouts.is_empty() {
        text.push_str("\nScout before implementing:\n");
        for s in &scouts {
            let tag = match s.action {
                ScoutAction::Read => "read",
                ScoutAction::Assert => "assert",
                ScoutAction::Verify => "verify",
            };
            text.push_str(&format!(
                "  [{}] {} [{}] — {}\n",
                tag, s.entity_name, s.entity_kind, s.reason
            ));
        }
    }

    let _ = std::fmt::Write::write_fmt(
        &mut text,
        format_args!(
            "\nNext: Discard: cog experiment discard {}\nNext: Adjust hypothesis: cog experiment hypothesize {} ...\n",
            id_short, id_short
        ),
    );
    if report.contradictions.is_empty() {
        let _ = std::fmt::Write::write_fmt(
            &mut text,
            format_args!(
                "Next: Safe to proceed: cog experiment commit {}\n",
                id_short
            ),
        );
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

pub fn report(id: &str, cog_dir: &std::path::Path) -> Result<CommandOutput> {
    let experiment = experiment::load(id, cog_dir)?;
    let report = experiment.evaluate()?;
    let mut text = format!(
        "experiment {}\ndescription: {}\nfocus: {}\nstatus: {:?}\nops: {}\nrisk score: {:.2}\naffected: {}\n",
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
    let scouts = generate_scout_suggestions(&report);
    if !scouts.is_empty() {
        text.push_str("scout suggestions:\n");
        for s in &scouts {
            text.push_str(&format!(
                "  [{}] {}\n",
                match s.action {
                    ScoutAction::Read => "read",
                    ScoutAction::Assert => "assert",
                    ScoutAction::Verify => "verify",
                },
                s.entity_name,
            ));
        }
    }
    Ok(CommandOutput::success(text))
}
/// Arguments for the `experiment try` subcommand.
pub struct TryArgs<'a> {
    pub entity: String,
    pub kind: AssertionKind,
    pub claim: String,
    pub grounds: String,
    pub desc: Option<String>,
    pub depends_on: Option<String>,
    pub cog_dir: &'a std::path::Path,
}

pub fn try_experiment(repo: &dyn Repository, args: &TryArgs<'_>) -> Result<CommandOutput> {
    let entity = &args.entity;
    let description = match &args.desc {
        Some(d) => d.clone(),
        None => format!("{}: {}", args.entity, args.claim),
    };
    let mut experiment = Experiment::start(repo, entity, description, 500)?;
    let op = ExperimentOp::Assertion {
        entity_name: args.entity.clone(),
        kind: args.kind,
        claim: args.claim.clone(),
        grounds: args.grounds.clone(),
        depends_on: args.depends_on.clone(),
    };
    experiment.hypothesize(op);
    let report = experiment.evaluate()?;
    experiment.risk_score = Some(report.risk_score);
    experiment.contradictions = report.contradictions.clone();
    experiment.mark_evaluated()?;
    experiment::save(&experiment, args.cog_dir)?;

    let id_short = &experiment.id[..8];
    let mut text = format!(
        "Experiment {}: \"{}\"\n\nHypothesis:\n  + [{}] {}: \"{}\"\n\nEvaluation:\n",
        id_short, experiment.description, args.kind, entity, args.claim
    );

    let risk_label = if report.risk_score >= 0.8 {
        "HIGH"
    } else if report.risk_score >= 0.5 {
        "MEDIUM"
    } else {
        "LOW"
    };
    let _ = std::fmt::Write::write_fmt(
        &mut text,
        format_args!(
            "  Risk: {} ({:.2})\n  Contradictions: {}\n  Affected assertions: {}\n  Cascade: {} assertions -> uncertain\n",
            risk_label,
            report.risk_score,
            report.contradictions.len(),
            report.affected_count,
            report.cascade_count,
        ),
    );

    let scouts = generate_scout_suggestions(&report);
    if !scouts.is_empty() {
        text.push_str("\nScout before implementing:\n");
        for s in &scouts {
            let tag = match s.action {
                ScoutAction::Read => "read",
                ScoutAction::Assert => "assert",
                ScoutAction::Verify => "verify",
            };
            text.push_str(&format!(
                "  [{}] {} [{}] — {}\n",
                tag, s.entity_name, s.entity_kind, s.reason
            ));
        }
    }

    let _ = std::fmt::Write::write_fmt(
        &mut text,
        format_args!(
            "\nNext: Discard: cog experiment discard {}\nNext: Adjust hypothesis: cog experiment hypothesize {} --kind correction --claim \"...\" --grounds \"code:{}\"\n",
            id_short, id_short, entity
        ),
    );
    if report.contradictions.is_empty() {
        let _ = std::fmt::Write::write_fmt(
            &mut text,
            format_args!(
                "Next: Safe to proceed: cog experiment commit {}\n",
                id_short
            ),
        );
    }

    Ok(CommandOutput::success(text))
}

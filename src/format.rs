use std::fmt::Write;

use crate::model::{
    Assertion, CascadeReason, CascadeResult, Entity, Evidence, ImpactResult, ModelStats,
    RelatedEntity, RelationDirection, TraceAssertion, TraceResult,
};

pub fn short_id(id: &str) -> &str {
    if id.len() >= 8 {
        &id[..8]
    } else {
        id
    }
}

pub fn entity_brief(entity: &Entity) -> String {
    format!("{} [{}]", entity.qualified_name, entity.kind)
}

pub fn assertion_detail(assertion: &Assertion, entity_name: &str, evidences: &[Evidence]) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "- {} [{}] {}|{}: {}",
        short_id(&assertion.id), entity_name, assertion.kind, assertion.status, assertion.claim
    );

    if evidences.is_empty() {
        out.push_str("  evidence: (none)\n");
    } else {
        for evidence in evidences {
            let _ = writeln!(out, "  evidence: {}:{}", evidence.source, evidence.detail);
        }
    }

    out
}

pub fn assertion_oneline(assertion: &Assertion) -> String {
    format!(
        "{} [{}|{}] {}",
        short_id(&assertion.id), assertion.kind, assertion.status, assertion.claim
    )
}

pub fn query_report(
    entity: &Entity,
    assertions: &[(Assertion, Vec<Evidence>)],
    related: &[RelatedEntity],
) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "entity: {} [{}]", entity.qualified_name, entity.kind);

    out.push_str("assertions:\n");
    if assertions.is_empty() {
        out.push_str("(none)\n");
    } else {
        for (assertion, evidences) in assertions {
            out.push_str(&assertion_detail(assertion, &entity.qualified_name, evidences));
        }
    }

    out.push_str("related_entities:\n");
    if related.is_empty() {
        out.push_str("(none)");
    } else {
        for entry in related {
            let direction = match entry.direction {
                RelationDirection::Outgoing => "out",
                RelationDirection::Incoming => "in",
            };
            let _ = writeln!(
                out,
                "- ({}) {} --{}--> {}",
                direction, entity.qualified_name, entry.kind, entry.entity.qualified_name
            );
        }
    }

    out
}

pub fn cascade_report(result: &CascadeResult) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "retracted: {} {}",
        short_id(&result.retracted.id),
        result.retracted.claim
    );
    if let Some(reason) = &result.retracted.retraction_reason {
        let _ = writeln!(out, "reason: {reason}");
    }

    if result.affected.is_empty() {
        out.push_str("affected: (none)");
    } else {
        out.push_str("affected:\n");
        for affected in &result.affected {
            let reason = match affected.cascade_reason {
                CascadeReason::MarkedUncertain => "marked_uncertain",
                CascadeReason::GroundWeakened => "ground_weakened",
            };
            let _ = writeln!(
                out,
                "- {} [{}] {}",
                short_id(&affected.assertion.id),
                reason,
                affected.assertion.claim
            );
        }
    }

    out
}

pub fn impact_report(result: &ImpactResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "impact_from: {}", result.entity.qualified_name);

    out.push_str("downstream_entities:\n");
    if result.downstream_entities.is_empty() {
        out.push_str("(none)\n");
    } else {
        for entity in &result.downstream_entities {
            let _ = writeln!(out, "- {}", entity_brief(entity));
        }
    }

    out.push_str("affected_assertions:\n");
    if result.affected_assertions.is_empty() {
        out.push_str("(none)");
    } else {
        for assertion in &result.affected_assertions {
            let _ = writeln!(out, "- {}", assertion_oneline(assertion));
        }
    }

    out
}

pub fn trace_report(result: &TraceResult) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "trace_entity: {}", result.entity.qualified_name);
    if result.assertions.is_empty() {
        out.push_str("assertions: (none)");
        return out;
    }

    out.push_str("assertions:\n");
    for assertion in &result.assertions {
        write_trace_assertion(&mut out, assertion, 0);
    }
    out
}

fn write_trace_assertion(out: &mut String, node: &TraceAssertion, depth: usize) {
    let indent = "  ".repeat(depth);
    let _ = writeln!(
        out,
        "{indent}- {} [{}|{}] {}",
        short_id(&node.assertion.id), node.assertion.kind, node.assertion.status, node.assertion.claim
    );
    if node.evidences.is_empty() {
        let _ = writeln!(out, "{indent}  evidence: (none)");
    } else {
        for evidence in &node.evidences {
            let _ = writeln!(
                out,
                "{indent}  evidence: {}:{}",
                evidence.source, evidence.detail
            );
        }
    }

    for dependency in &node.dependencies {
        write_trace_assertion(out, dependency, depth + 1);
    }
}

pub fn stats_report(stats: &ModelStats) -> String {
    format!(
        "entities: {}\nassertions: {}\nactive_assertions: {}\nuncertain_assertions: {}\nretracted_assertions: {}\nevidences: {}\ncorrections: {}",
        stats.entities,
        stats.assertions,
        stats.active_assertions,
        stats.uncertain_assertions,
        stats.retracted_assertions,
        stats.evidences,
        stats.corrections
    )
}

pub fn entity_index(entities: &[Entity]) -> String {
    if entities.is_empty() {
        return "(no entities)".to_string();
    }

    let mut out = String::new();
    for entity in entities {
        let _ = writeln!(out, "- {}", entity_brief(entity));
    }
    out
}

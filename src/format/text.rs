use std::fmt::Write;

use crate::domain::*;
use crate::repo::branch::BranchInfo;
use crate::repo::diff::{DiffItem, DiffSummary, ModelDiff};

pub struct TextRenderer;

impl TextRenderer {
    pub fn short_id(id: &str) -> &str {
        if id.len() >= 8 { &id[..8] } else { id }
    }

    pub fn entity_brief(entity: &Entity) -> String {
        format!("{} [{}]", entity.qualified_name, entity.kind)
    }

    pub fn assertion_detail(
        assertion: &Assertion,
        entity_name: &str,
        evidences: &[Evidence],
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "- {} [{}] {}|{}: {}",
            Self::short_id(&assertion.id),
            entity_name,
            assertion.kind,
            assertion.status,
            assertion.claim
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
            Self::short_id(&assertion.id),
            assertion.kind,
            assertion.status,
            assertion.claim
        )
    }

    pub fn query_report(
        entity: &Entity,
        assertions: &[(Assertion, Vec<Evidence>)],
        related: &[RelatedEntity],
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "entity: {} [{}]", entity.qualified_name, entity.kind);

        // Show metrics if any are set
        let m = &entity.metrics;
        let has_metrics = m.fan_in.is_some() || m.fan_out.is_some() || m.line_count.is_some();
        if has_metrics {
            let mut parts = Vec::new();
            if let Some(fi) = m.fan_in { parts.push(format!("fan_in={fi}")); }
            if let Some(fo) = m.fan_out { parts.push(format!("fan_out={fo}")); }
            if let Some(lc) = m.line_count { parts.push(format!("lines={lc}")); }
            parts.push(format!("risk={}", m.risk_level()));
            let _ = writeln!(out, "metrics: {}", parts.join(", "));
        }

        out.push_str("assertions:\n");
        if assertions.is_empty() {
            out.push_str("(none)\n");
        } else {
            for (assertion, evidences) in assertions {
                out.push_str(&Self::assertion_detail(
                    assertion,
                    &entity.qualified_name,
                    evidences,
                ));
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

    pub fn cascade_report(result: &CascadeReport) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "retracted: {} {}",
            Self::short_id(&result.retracted.id),
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
                    Self::short_id(&affected.assertion.id),
                    reason,
                    affected.assertion.claim
                );
            }
        }

        out
    }

    pub fn impact_report(result: &ImpactCard) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "impact_from: {}", result.entity.qualified_name);

        out.push_str("downstream_entities:\n");
        if result.downstream_entities.is_empty() {
            out.push_str("(none)\n");
        } else {
            for entity in &result.downstream_entities {
                let _ = writeln!(out, "- {}", Self::entity_brief(entity));
            }
        }

        out.push_str("affected_assertions:\n");
        if result.affected_assertions.is_empty() {
            out.push_str("(none)");
        } else {
            for assertion in &result.affected_assertions {
                let _ = writeln!(out, "- {}", Self::assertion_oneline(assertion));
            }
        }

        out
    }

    pub fn trace_report(result: &TraceTree) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "trace_entity: {}", result.entity.qualified_name);
        if result.assertions.is_empty() {
            out.push_str("assertions: (none)\n");
        } else {
            out.push_str("assertions:\n");
            for assertion in &result.assertions {
                Self::write_trace_assertion(&mut out, assertion, 0);
            }
        }

        if !result.related_entities.is_empty() {
            out.push_str("entity_relations:\n");
            for rel in &result.related_entities {
                let arrow = match rel.direction {
                    RelationDirection::Outgoing => "-->",
                    RelationDirection::Incoming => "<--",
                };
                let _ = writeln!(
                    out,
                    "- ({}) {} {} {} [{}]",
                    match rel.direction {
                        RelationDirection::Outgoing => "out",
                        RelationDirection::Incoming => "in",
                    },
                    result.entity.qualified_name,
                    arrow,
                    rel.entity.qualified_name,
                    rel.kind,
                );
            }
        }

        out
    }

    fn write_trace_assertion(out: &mut String, node: &TraceAssertion, depth: usize) {
        let indent = "  ".repeat(depth);
        let _ = writeln!(
            out,
            "{indent}- {} [{}|{}] {}",
            Self::short_id(&node.assertion.id),
            node.assertion.kind,
            node.assertion.status,
            node.assertion.claim
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
            Self::write_trace_assertion(out, dependency, depth + 1);
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

    pub fn entity_index_with_counts(entities: &[(Entity, usize)]) -> String {
        if entities.is_empty() {
            return "(no entities)".to_string();
        }

        let mut out = String::new();
        for (entity, count) in entities {
            let _ = writeln!(out, "- {} [{}]", entity.qualified_name, count);
        }
        out
    }

    pub fn diff_summary(summary: &DiffSummary) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "diff: {} change(s)", summary.total);

        let mut row = |label: &str, count: usize, sign: &str| {
            if count > 0 {
                let _ = writeln!(out, "  {label}: {sign}{count}");
            }
        };

        row("entities", summary.entities_added, "+");
        row("entities", summary.entities_removed, "-");
        row("assertions", summary.assertions_added, "+");
        row("assertions", summary.assertions_removed, "-");
        row("assertions", summary.assertions_changed, "~");
        row("evidences", summary.evidences_added, "+");
        row("evidences", summary.evidences_removed, "-");
        row("entity_relations", summary.entity_relations_added, "+");
        row("entity_relations", summary.entity_relations_removed, "-");
        row(
            "assertion_relations",
            summary.assertion_relations_added,
            "+",
        );
        row(
            "assertion_relations",
            summary.assertion_relations_removed,
            "-",
        );
        out
    }

    pub fn diff_item_detail(index: usize, item: &DiffItem) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "[{}] {}", index, Self::item_label(item));

        match item {
            DiffItem::EntityAdded(e) | DiffItem::EntityRemoved(e) => {
                let _ = writeln!(out, "  id: {}", Self::short_id(&e.id));
                let _ = writeln!(out, "  name: {}", e.qualified_name);
                let _ = writeln!(out, "  kind: {}", e.kind);
            }
            DiffItem::AssertionAdded(a) | DiffItem::AssertionRemoved(a) => {
                let _ = writeln!(out, "  id: {}", Self::short_id(&a.id));
                let _ = writeln!(out, "  kind: {}", a.kind);
                let _ = writeln!(out, "  status: {}", a.status);
                let _ = writeln!(out, "  claim: {}", a.claim);
            }
            DiffItem::AssertionChanged(change) => {
                let _ = writeln!(out, "  id: {}", Self::short_id(&change.before.id));
                let _ = writeln!(out, "  fields: {}", change.changed_fields.join(", "));
                let _ = writeln!(out, "  before:");
                let _ = writeln!(out, "    kind: {}", change.before.kind);
                let _ = writeln!(out, "    status: {}", change.before.status);
                let _ = writeln!(out, "    claim: {}", change.before.claim);
                let _ = writeln!(out, "  after:");
                let _ = writeln!(out, "    kind: {}", change.after.kind);
                let _ = writeln!(out, "    status: {}", change.after.status);
                let _ = writeln!(out, "    claim: {}", change.after.claim);
            }
            DiffItem::EvidenceAdded(e) | DiffItem::EvidenceRemoved(e) => {
                let _ = writeln!(out, "  id: {}", Self::short_id(&e.id));
                let _ = writeln!(out, "  source: {}", e.source);
                let _ = writeln!(out, "  detail: {}", e.detail);
            }
            DiffItem::EntityRelationAdded(r) | DiffItem::EntityRelationRemoved(r) => {
                let _ = writeln!(out, "  id: {}", Self::short_id(&r.id));
                let _ = writeln!(out, "  from: {}", Self::short_id(&r.from_entity));
                let _ = writeln!(out, "  to: {}", Self::short_id(&r.to_entity));
                let _ = writeln!(out, "  kind: {}", r.kind);
            }
            DiffItem::AssertionRelationAdded(r) | DiffItem::AssertionRelationRemoved(r) => {
                let _ = writeln!(out, "  id: {}", Self::short_id(&r.id));
                let _ = writeln!(out, "  from: {}", Self::short_id(&r.from_assertion));
                let _ = writeln!(out, "  to: {}", Self::short_id(&r.to_assertion));
                let _ = writeln!(out, "  kind: {}", r.kind);
            }
        }
        out
    }

    pub fn merge_plan(diff: &ModelDiff) -> String {
        let items = diff.items();
        if items.is_empty() {
            return "merge: no changes to apply".to_string();
        }

        let mut out = String::new();
        let _ = writeln!(out, "merge: {} item(s) pending", items.len());
        for (i, item) in items.iter().enumerate() {
            let _ = writeln!(out, "  [{}] [pending] {}", i, Self::item_label(item));
        }
        out
    }

    pub fn item_label(item: &DiffItem) -> String {
        match item {
            DiffItem::EntityAdded(e) => format!("+entity {} [{}]", e.qualified_name, e.kind),
            DiffItem::EntityRemoved(e) => format!("-entity {} [{}]", e.qualified_name, e.kind),
            DiffItem::AssertionAdded(a) => {
                format!("+assertion {}|{}: {}", a.kind, a.status, a.claim)
            }
            DiffItem::AssertionRemoved(a) => {
                format!("-assertion {}|{}: {}", a.kind, a.status, a.claim)
            }
            DiffItem::AssertionChanged(c) => {
                format!(
                    "~assertion {} ({} → {})",
                    Self::short_id(&c.before.id),
                    c.before.status,
                    c.after.status
                )
            }
            DiffItem::EvidenceAdded(e) => format!("+evidence {}:{}", e.source, e.detail),
            DiffItem::EvidenceRemoved(e) => format!("-evidence {}:{}", e.source, e.detail),
            DiffItem::EntityRelationAdded(r) => {
                format!(
                    "+entity_relation {} --{}--> {}",
                    Self::short_id(&r.from_entity),
                    r.kind,
                    Self::short_id(&r.to_entity)
                )
            }
            DiffItem::EntityRelationRemoved(r) => {
                format!(
                    "-entity_relation {} --{}--> {}",
                    Self::short_id(&r.from_entity),
                    r.kind,
                    Self::short_id(&r.to_entity)
                )
            }
            DiffItem::AssertionRelationAdded(r) => {
                format!(
                    "+dep {} → {}",
                    Self::short_id(&r.from_assertion),
                    Self::short_id(&r.to_assertion)
                )
            }
            DiffItem::AssertionRelationRemoved(r) => {
                format!(
                    "-dep {} → {}",
                    Self::short_id(&r.from_assertion),
                    Self::short_id(&r.to_assertion)
                )
            }
        }
    }

    pub fn branch_list_report(branches: &[BranchInfo]) -> String {
        if branches.is_empty() {
            return "branches: (none)".to_string();
        }
        let mut out = String::new();
        for b in branches {
            let size_kb = b.size_bytes as f64 / 1024.0;
            let modified = b
                .modified
                .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "?".to_string());
            let _ = writeln!(out, "- {} ({}KB, {})", b.name, size_kb as u64, modified);
        }
        out
    }

    pub fn assertion_created(
        assertion: &Assertion,
        entity: &Entity,
        depends_on: Option<&str>,
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "assertion created");
        let _ = writeln!(
            out,
            "- id: {} ({})",
            Self::short_id(&assertion.id),
            assertion.id
        );
        let _ = writeln!(out, "- entity: {}", entity.qualified_name);
        let _ = writeln!(out, "- kind: {}", assertion.kind);
        let _ = writeln!(out, "- claim: {}", assertion.claim);
        if let Some(dep) = depends_on {
            let _ = writeln!(out, "- depends_on: {}", Self::short_id(dep));
        }
        out
    }

    pub fn dependency_recorded(from: &Entity, to: &Entity, kind: EntityRelationKind) -> String {
        format!(
            "dependency recorded\n- from: {}\n- to: {}\n- kind: {}\n",
            from.qualified_name, to.qualified_name, kind
        )
    }
}

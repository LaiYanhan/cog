use std::fmt::Write;

use crate::domain::*;

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
            if let Some(fi) = m.fan_in {
                parts.push(format!("fan_in={fi}"));
            }
            if let Some(fo) = m.fan_out {
                parts.push(format!("fan_out={fo}"));
            }
            if let Some(lc) = m.line_count {
                parts.push(format!("lines={lc}"));
            }
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

        if let Some(risk) = &result.risk_assessment {
            out.push_str(&format!(
                "\nrisk: {} ({:.2})\n  downstream: {} | assertions: {} | fragile: {}\n  {}\n",
                if risk.risk_score >= 0.8 {
                    "HIGH"
                } else if risk.risk_score >= 0.5 {
                    "MEDIUM"
                } else {
                    "LOW"
                },
                risk.risk_score,
                risk.downstream_count,
                risk.active_assertions,
                risk.fragile_assertions,
                risk.summary,
            ));
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

    pub fn init_report(report: &InitReport) -> String {
        let mut out = String::new();
        let lang_summary = {
            let mut entries: Vec<_> = report.files_by_language.iter().collect();
            entries.sort_by(|a, b| b.1.cmp(a.1));
            entries
                .iter()
                .map(|(lang, count)| format!("{lang}: {count}"))
                .collect::<Vec<_>>()
                .join(", ")
        };

        if report.dry_run {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!(
                    "DRY RUN — no changes written\n\nScanned {} files ({})\n",
                    report.files_scanned, lang_summary
                ),
            );
            let def_count: usize = report.entity_counts_by_kind.values().sum();
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!("Would create {} definitions\n\n", def_count),
            );
        } else {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!(
                    "Scanned {} files ({})\nCreated {} entities, {} relations\n\n",
                    report.files_scanned,
                    lang_summary,
                    report.entities_created,
                    report.relations_created,
                ),
            );
        }

        let kind_order = ["module", "type", "function", "method", "field"];
        for &kind_name in &kind_order {
            if let Some(&count) = report.entity_counts_by_kind.get(kind_name)
                && count > 0
            {
                let label = kind_name;
                let pad = 10usize.saturating_sub(label.len());
                let pad_str = " ".repeat(pad);
                let _ = std::fmt::Write::write_fmt(
                    &mut out,
                    format_args!("  {label}:{pad_str}{count}\n"),
                );
            }
        }

        if !report.dry_run {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!("\nNext: cog index | cog trace <entity>\n"),
            );
        }
        out
    }

    pub fn verification_report(report: &VerificationReport) -> String {
        let mut out = String::new();
        if report.success {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!(
                    "verify: ok (checked {} entities, {} cleaned)\n",
                    report.checked_count, report.cleaned_count,
                ),
            );
        } else {
            let _ = std::fmt::Write::write_fmt(
                &mut out,
                format_args!("verify: found {} issue(s)\n", report.issues.len(),),
            );
            for issue in &report.issues {
                let _ = std::fmt::Write::write_fmt(
                    &mut out,
                    format_args!(
                        "- {:?} entity={} assertion={} detail={}\n",
                        issue.kind,
                        issue.entity_name.as_deref().unwrap_or("-"),
                        issue
                            .assertion_id
                            .as_deref()
                            .map(|id| if id.len() >= 8 { &id[..8] } else { id })
                            .unwrap_or("-"),
                        issue.detail,
                    ),
                );
            }
        }
        for line in &report.scan_issues {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{line}\n"));
        }
        out
    }
}

// ── Renderable impls ──────────────────────────────────────────────────────

use crate::format::Renderable;

impl Renderable for CascadeReport {
    fn render_text(&self) -> String {
        TextRenderer::cascade_report(self)
    }
}

impl Renderable for ImpactCard {
    fn render_text(&self) -> String {
        TextRenderer::impact_report(self)
    }
}

impl Renderable for TraceTree {
    fn render_text(&self) -> String {
        TextRenderer::trace_report(self)
    }
}

impl Renderable for ModelStats {
    fn render_text(&self) -> String {
        TextRenderer::stats_report(self)
    }
}

impl Renderable for QueryCard {
    fn render_text(&self) -> String {
        TextRenderer::query_report(&self.entity, &self.assertions, &self.related)
    }
}

impl Renderable for EntityIndex {
    fn render_text(&self) -> String {
        TextRenderer::entity_index_with_counts(&self.entities)
    }
}

impl Renderable for InitReport {
    fn render_text(&self) -> String {
        TextRenderer::init_report(self)
    }
}

impl Renderable for VerificationReport {
    fn render_text(&self) -> String {
        TextRenderer::verification_report(self)
    }
}

impl Renderable for StatusMessage {
    fn render_text(&self) -> String {
        self.message.clone()
    }
}

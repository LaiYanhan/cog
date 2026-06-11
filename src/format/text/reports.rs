use std::fmt::Write;

use crate::domain::*;

use super::TextRenderer;

impl TextRenderer {
    pub fn query_report(
        entity: &Entity,
        assertions: &[(Assertion, Vec<Evidence>)],
        related: &[RelatedEntity],
        related_assertion_counts: &std::collections::HashMap<String, usize>,
        relations_detail: bool,
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "{} [{}]", entity.qualified_name, entity.kind);

        // Count active/retracted
        let (active, retracted): (Vec<_>, Vec<_>) =
            assertions.iter().partition(|(a, _)| a.is_active());
        let total = assertions.len();

        if total > 0 {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "assertions ({} active, {} retracted):",
                active.len(),
                retracted.len()
            );
            for (assertion, evidences) in assertions {
                let _ = writeln!(
                    out,
                    "  [{}] {}: {}",
                    assertion.kind,
                    crate::domain::short_id(&assertion.id),
                    assertion.claim
                );
                if let Some(ev) = evidences.first() {
                    let _ = writeln!(out, "    grounds: {}", ev.source);
                }
            }
        }
        if !related.is_empty() {
            if relations_detail {
                Self::render_relations_full(&mut out, related);
            } else {
                Self::render_relations_summary(&mut out, related, related_assertion_counts);
            }
        }

        out
    }

    /// Full relation listing (with `--relations` flag).
    fn render_relations_full(out: &mut String, related: &[RelatedEntity]) {
        let _ = writeln!(out);
        let _ = writeln!(out, "relations ({}):", related.len());
        for entry in related {
            match entry.direction {
                RelationDirection::Outgoing => {
                    let _ = writeln!(
                        out,
                        "  -> {} {} [{}]",
                        entry.kind, entry.entity.qualified_name, entry.entity.kind
                    );
                }
                RelationDirection::Incoming => {
                    let _ = writeln!(
                        out,
                        "  <- {} {} [{}]",
                        entry.kind, entry.entity.qualified_name, entry.entity.kind
                    );
                }
            }
        }
    }

    /// Assertion-aware relation summary.
    ///
    /// Groups relations by (direction, kind). Within each group, target entities
    /// with active assertions are expanded (qualified name + kind + count),
    /// while blind entities are collapsed into a compact one-liner.
    fn render_relations_summary(
        out: &mut String,
        related: &[RelatedEntity],
        assertion_counts: &std::collections::HashMap<String, usize>,
    ) {
        use std::collections::HashMap;

        // Group by (direction, kind) — keep full entity refs for rendering.
        let mut groups: HashMap<(RelationDirection, EntityRelationKind), Vec<&RelatedEntity>> =
            HashMap::new();
        for entry in related {
            groups
                .entry((entry.direction, entry.kind))
                .or_default()
                .push(entry);
        }

        let _ = writeln!(out);
        let _ = writeln!(out, "relations ({}):", related.len());

        // Stable output order for the 6 known (direction, kind) combinations.
        const ORDER: &[(RelationDirection, EntityRelationKind)] = &[
            (RelationDirection::Outgoing, EntityRelationKind::Calls),
            (RelationDirection::Outgoing, EntityRelationKind::Contains),
            (RelationDirection::Outgoing, EntityRelationKind::Uses),
            (RelationDirection::Incoming, EntityRelationKind::Calls),
            (RelationDirection::Incoming, EntityRelationKind::Contains),
            (RelationDirection::Incoming, EntityRelationKind::Uses),
        ];

        let mut rendered = std::collections::HashSet::new();
        for key in ORDER {
            if let Some(entries) = groups.get(key) {
                Self::render_relation_group(out, entries, *key, assertion_counts);
                rendered.insert(key);
            }
        }
        // Catch-all for any combos not in the explicit order (future-proof).
        for (key, entries) in &groups {
            if rendered.contains(&key) {
                continue;
            }
            Self::render_relation_group(out, entries, *key, assertion_counts);
        }
    }

    /// Render one (direction, kind) group.
    ///
    /// If any target has assertions → multi-line: expand asserted, collapse blind.
    /// If all blind → compact one-liner (unchanged from old pure-summary).
    fn render_relation_group(
        out: &mut String,
        entries: &[&RelatedEntity],
        key: (RelationDirection, EntityRelationKind),
        assertion_counts: &std::collections::HashMap<String, usize>,
    ) {
        let (dir, kind) = key;
        let arrow = match dir {
            RelationDirection::Outgoing => "→",
            RelationDirection::Incoming => "←",
        };
        let label: String = match (dir, kind) {
            (RelationDirection::Outgoing, EntityRelationKind::Calls) => "calls".into(),
            (RelationDirection::Outgoing, EntityRelationKind::Contains) => "contains".into(),
            (RelationDirection::Incoming, EntityRelationKind::Calls) => "called-by".into(),
            (RelationDirection::Incoming, EntityRelationKind::Contains) => "contained-by".into(),
            _ => kind.to_string(),
        };

        // Partition by assertion count.
        let (asserted, blind): (Vec<&&RelatedEntity>, Vec<&&RelatedEntity>) = entries
            .iter()
            .partition(|e| assertion_counts.get(&e.entity.id).copied().unwrap_or(0) > 0);

        const MAX_EXAMPLES: usize = 5;

        if asserted.is_empty() {
            // All blind — compact one-liner.
            let names: Vec<&str> = entries
                .iter()
                .take(MAX_EXAMPLES)
                .map(|e| crate::domain::entity::last_segment(&e.entity.qualified_name))
                .collect();
            let extra = if entries.len() > MAX_EXAMPLES {
                format!(", +{}", entries.len() - MAX_EXAMPLES)
            } else {
                String::new()
            };
            let _ = writeln!(
                out,
                "  {} {} {} ({}{})",
                arrow,
                label,
                entries.len(),
                names.join(", "),
                extra
            );
        } else {
            // Has asserted entities — multi-line with assertion counts.
            // TODO: MAX_ASSERTED should eventually come from user config.
            let max = crate::domain::display::MAX_ASSERTED;
            let _ = writeln!(out, "  {} {} {}:", arrow, label, entries.len());
            for ae in asserted.iter().take(max) {
                let count = assertion_counts.get(&ae.entity.id).copied().unwrap_or(0);
                let _ = writeln!(
                    out,
                    "    {} [{}]  {} active",
                    ae.entity.qualified_name, ae.entity.kind, count
                );
            }
            let remaining = asserted.len().saturating_sub(max);
            if remaining > 0 {
                let _ = writeln!(out, "    + {} more asserted", remaining);
            }
            if !blind.is_empty() {
                let _ = writeln!(out, "    + {} blind", blind.len());
            }
        }
    }

    pub fn query_compact(entity: &Entity, assertions: &[(Assertion, Vec<Evidence>)]) -> String {
        let mut out = String::new();
        let active: Vec<_> = assertions.iter().filter(|(a, _)| a.is_active()).collect();
        let _ = writeln!(
            out,
            "{} [{}] — {} active:",
            entity.qualified_name,
            entity.kind,
            active.len()
        );
        for (assertion, _) in &active {
            let _ = writeln!(
                out,
                "  [{}] {}: {}",
                assertion.kind,
                crate::domain::short_id(&assertion.id),
                assertion.claim
            );
        }
        out
    }

    pub fn cascade_report(
        result: &CascadeReport,
        entity_name: &str,
        remaining_assertions: &[(Assertion, Vec<Evidence>)],
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Retracted {} [{}] on {}",
            crate::domain::short_id(&result.retracted.id),
            result.retracted.kind,
            entity_name
        );
        if let Some(reason) = &result.retracted.retraction_reason {
            let _ = writeln!(out, "  Reason: \"{}\"", reason);
        }

        if !result.affected.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "Cascade: {} assertion(s) affected",
                result.affected.len()
            );
            for affected in &result.affected {
                let reason = match affected.cascade_reason {
                    CascadeReason::MarkedUncertain => "uncertain",
                    CascadeReason::GroundWeakened => "uncertain (ground weakened)",
                };
                let _ = writeln!(
                    out,
                    "  {} [{}] -> {}",
                    crate::domain::short_id(&affected.assertion.id),
                    affected.assertion.kind,
                    reason
                );
                let _ = writeln!(out, "    \"{}\"", affected.assertion.claim);
            }
        }

        if !remaining_assertions.is_empty() {
            let active: Vec<_> = remaining_assertions
                .iter()
                .filter(|(a, _)| a.is_active())
                .collect();
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "{} now has {} active assertions:",
                entity_name,
                active.len()
            );
            let mut has_uncertain = false;
            for (assertion, _) in remaining_assertions {
                let status_tag = if !assertion.is_active() {
                    has_uncertain = true;
                    " [uncertain]"
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "  [{}] {}: {}{}",
                    assertion.kind,
                    crate::domain::short_id(&assertion.id),
                    assertion.claim,
                    status_tag
                );
            }

            if has_uncertain {
                let first_uncertain = remaining_assertions.iter().find(|(a, _)| !a.is_active());
                if let Some((a, _)) = first_uncertain {
                    let _ = writeln!(out);
                    let _ = writeln!(
                        out,
                        "Next: {} is now uncertain. Re-verify it:",
                        crate::domain::short_id(&a.id)
                    );
                    let _ = writeln!(out, "    cog query {} --all", entity_name);
                }
            }
        }

        out
    }

    pub fn impact_report(result: &ImpactCard) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Impact for: {} [{}]",
            result.entity.qualified_name, result.entity.kind
        );

        if let Some(risk) = &result.risk_assessment {
            let _ = writeln!(out);
            let label = if risk.risk_score >= 0.8 {
                "HIGH"
            } else if risk.risk_score >= 0.5 {
                "MEDIUM"
            } else {
                "LOW"
            };
            let _ = writeln!(out, "Risk: {} ({:.2})", label, risk.risk_score);
            let _ = writeln!(
                out,
                "  Dependents: {} entities ({} covered, {} blind)",
                result.downstream_entities.len(),
                result.downstream_entities.len() - result.blind_downstream.unwrap_or(0),
                result.blind_downstream.unwrap_or(0)
            );
            let _ = writeln!(
                out,
                "  Active assertions at stake: {}",
                risk.active_assertions
            );
        }

        if !result.downstream_entities.is_empty() {
            // Partition into covered (has assertions) and blind (no assertions).
            let (covered, blind) = crate::domain::display::partition_by_assertion(
                result.downstream_entities.iter().enumerate().map(|(i, e)| {
                    let count = result
                        .downstream_assertion_counts
                        .get(i)
                        .copied()
                        .unwrap_or(0);
                    (e.clone(), count)
                }),
            );

            // Covered dependents — these have knowledge at stake.
            if !covered.is_empty() {
                let _ = writeln!(out);
                let _ = writeln!(out, "Covered dependents:");
                // TODO: MAX_ASSERTED should eventually come from user config.
                let max = crate::domain::display::MAX_ASSERTED;
                for ae in covered.iter().take(max) {
                    let _ = writeln!(
                        out,
                        "  {} [{}]    ({} assertion{})",
                        ae.entity.qualified_name,
                        ae.entity.kind,
                        ae.active_assertions,
                        if ae.active_assertions == 1 { "" } else { "s" }
                    );
                }
                let remaining = covered.len().saturating_sub(max);
                if remaining > 0 {
                    let _ = writeln!(out, "  + {} more covered", remaining);
                }
            }

            // Blind dependents — no recorded knowledge at risk, collapse to count + samples.
            if !blind.is_empty() {
                let _ = writeln!(out);
                let sample_max = 4usize;
                let sample_names: Vec<&str> = blind
                    .iter()
                    .take(sample_max)
                    .map(|ae| ae.entity.qualified_name.as_str())
                    .collect();
                if blind.len() <= sample_max {
                    let _ = writeln!(
                        out,
                        "Blind dependents ({}): {}",
                        blind.len(),
                        sample_names.join(", ")
                    );
                } else {
                    let remaining = blind.len() - sample_max;
                    let _ = writeln!(
                        out,
                        "Blind dependents: {} entities, e.g. {} ... and {} more",
                        blind.len(),
                        sample_names.join(", "),
                        remaining
                    );
                }
            }
        } else {
            let _ = writeln!(out);
            let _ = writeln!(out, "No dependents found via Calls / Uses edges.");
            let _ = writeln!(out, "For structural hierarchy, use: cog query <entity>");
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
            crate::domain::short_id(&node.assertion.id),
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
        let _ = writeln!(out, "{} entities:", entities.len());
        for (entity, count) in entities {
            let _ = writeln!(
                out,
                "  {} [{}]  {} assertions",
                entity.qualified_name, entity.kind, count
            );
        }
        out
    }

    pub fn assertion_created(
        assertion: &Assertion,
        entity: &Entity,
        existing_assertions: &[(Assertion, Vec<Evidence>)],
        same_kind_count: usize,
    ) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Created {} [{}] on {}",
            crate::domain::short_id(&assertion.id),
            assertion.kind,
            entity.qualified_name
        );
        let _ = writeln!(out, "  \"{}\"", assertion.claim);

        let active: Vec<_> = existing_assertions
            .iter()
            .filter(|(a, _)| a.is_active())
            .collect();
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "{} now has {} active assertions:",
            entity.qualified_name,
            active.len()
        );
        for (i, (a, _)) in existing_assertions.iter().enumerate() {
            let new_tag = if a.id == assertion.id {
                "    [new]"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "  {}. [{}] {}: {}{}",
                i + 1,
                a.kind,
                crate::domain::short_id(&a.id),
                a.claim,
                new_tag
            );
        }

        if same_kind_count > 1 {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "WARNING: {} already has {} {} assertion(s). Ensure this one adds new information rather than duplicating.",
                entity.qualified_name,
                same_kind_count - 1,
                assertion.kind
            );
            // Find an existing assertion of the same kind to suggest retracting
            if let Some((old_a, _)) = existing_assertions
                .iter()
                .find(|(a, _)| a.kind == assertion.kind && a.id != assertion.id)
            {
                let _ = writeln!(
                    out,
                    "  Next: To replace: cog retract {} --reason \"superseded by {}\"",
                    crate::domain::short_id(&old_a.id),
                    crate::domain::short_id(&assertion.id)
                );
            }
        }

        out
    }

    pub fn dependency_report(
        from: &Entity,
        to: &Entity,
        kind: EntityRelationKind,
        related: &[crate::domain::RelatedEntity],
    ) -> String {
        let mut out = format!(
            "dependency recorded\n- from: {}\n- to: {}\n- kind: {}\n",
            from.qualified_name, to.qualified_name, kind
        );
        if !related.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "{} now has {} relation(s):",
                from.qualified_name,
                related.len()
            );
            for rel in related {
                let arrow = match rel.direction {
                    crate::domain::RelationDirection::Outgoing => "->",
                    crate::domain::RelationDirection::Incoming => "<-",
                };
                let _ = writeln!(
                    out,
                    "  {} {:?} {} [{}]",
                    arrow, rel.kind, rel.entity.qualified_name, rel.entity.kind
                );
            }
        }
        out
    }

    pub fn sync_report(report: &SyncReport) -> String {
        let mut out = String::new();

        if report.dry_run {
            let lang_summary = {
                let mut entries: Vec<_> = report.files_by_language.iter().collect();
                entries.sort_by(|a, b| b.1.cmp(a.1));
                entries
                    .iter()
                    .map(|(lang, count)| format!("{lang}: {count}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let _ = writeln!(out, "DRY RUN — no changes written");
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "Scanned {} files ({})",
                report.files_scanned, lang_summary
            );
            let _ = writeln!(out);
            let _ = writeln!(out, "Would sync entities and relations.");
            let _ = writeln!(out, "Next: Apply changes: cog sync");
            return out;
        }

        // ── Non-dry-run output ─────────────────────────────────────
        let lang_summary = {
            let mut entries: Vec<_> = report.files_by_language.iter().collect();
            entries.sort_by(|a, b| b.1.cmp(a.1));
            entries
                .iter()
                .map(|(lang, count)| format!("{lang}: {count}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let _ = writeln!(
            out,
            "Sync: {} files scanned ({})",
            report.files_scanned, lang_summary
        );

        let has_changes = report.entities_created > 0
            || report.entities_removed > 0
            || report.relations_created > 0;

        if has_changes {
            let _ = writeln!(
                out,
                "  +{} entities created, -{} removed, {} relations",
                report.entities_created, report.entities_removed, report.relations_created,
            );
        }

        let kind_order = ["module", "type", "function", "method", "field"];
        for &kind_name in &kind_order {
            if let Some(&count) = report.entity_counts_by_kind.get(kind_name)
                && count > 0
            {
                let _ = writeln!(out, "  {kind_name}: {count}");
            }
        }

        if !report.stale_entities.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "Removed {} stale entities:",
                report.stale_entities.len()
            );
            for name in &report.stale_entities {
                let _ = writeln!(out, "  - {name}");
            }
        }

        if !report.stale_skipped.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "Skipped {} stale entities (have assertions):",
                report.stale_skipped.len()
            );
            for name in &report.stale_skipped {
                let _ = writeln!(out, "  - {name}  (use `cog delete-entity {name}` to force)");
            }
        }

        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "After sync: {} entities, {} assertions",
            report.after_entities, report.after_assertions
        );

        if !has_changes && !report.has_drift {
            let _ = writeln!(out, "Model is up to date — no drift.");
        }

        let _ = writeln!(out);
        let _ = writeln!(out, "Next: cog index | cog impact <entity>");

        out
    }

    pub fn verification_report(report: &VerificationReport) -> String {
        let mut out = String::new();
        if report.success {
            let _ = writeln!(
                out,
                "verify: ok (checked {} entities, {} cleaned)",
                report.checked_count, report.cleaned_count,
            );
        } else {
            let _ = writeln!(out, "verify: found {} issue(s)", report.issues.len());
            for issue in &report.issues {
                let _ = writeln!(
                    out,
                    "- {:?} entity={} assertion={} detail={}",
                    issue.kind,
                    issue.entity_name.as_deref().unwrap_or("-"),
                    issue.assertion_id.as_deref().map(short_id).unwrap_or("-"),
                    issue.detail,
                );
            }
        }
        for line in &report.scan_issues {
            let _ = writeln!(out, "{line}");
        }
        out
    }

    pub fn next_report(report: &NextReport) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "State: {}", report.state);

        if report.active_experiments.is_empty() {
            let _ = writeln!(out, "Experiment: none");
        } else {
            for exp in &report.active_experiments {
                let _ = writeln!(
                    out,
                    "Experiment: {} {} — \"{}\"",
                    exp.status, exp.short_id, exp.description
                );
            }
        }

        let _ = writeln!(
            out,
            "Model: {} entities, {} assertions ({} active, {} retracted)",
            report.model.entities,
            report.model.assertions,
            report.model.active,
            report.model.retracted
        );
        let _ = writeln!(
            out,
            "Coverage: {:.0}% ({}/{})",
            report.coverage_pct, report.covered, report.model.entities
        );

        if !report.suggestions.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(out, "Suggestions:");
            for (i, s) in report.suggestions.iter().enumerate() {
                let _ = writeln!(out, "  {}. [{}] {}", i + 1, s.kind, s.description);
                let _ = writeln!(out, "     Next: {}", s.next_command);
            }
        }

        if let Some(warning) = &report.stagnation_warning {
            let _ = writeln!(out);
            let _ = writeln!(out, "{}", warning);
        }

        let _ = writeln!(out);
        let _ = writeln!(out, "Status: OK");

        out
    }

    pub fn index_summary(coverage: &IndexCoverage) -> String {
        let mut out = String::new();
        let _ = writeln!(
            out,
            "Coverage: {}/{} ({:.0}%)",
            coverage.covered, coverage.total, coverage.pct
        );

        if !coverage.modules.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(
                out,
                "By module (top {} uncovered):",
                coverage.modules.len().min(5)
            );
            for m in coverage.modules.iter().take(5) {
                let uncovered = m.total - m.covered;
                let tag = if uncovered == 0 {
                    "(fully covered)".to_string()
                } else {
                    format!("({} uncovered)", uncovered)
                };
                let _ = writeln!(out, "  {}/    {}/{} {}", m.path, m.covered, m.total, tag);
            }
        }

        if !coverage.top_uncovered.is_empty() {
            let _ = writeln!(out);
            let _ = writeln!(out, "Top uncovered by downstream impact:");
            for u in &coverage.top_uncovered {
                let _ = writeln!(
                    out,
                    "  {} [{}] -- {} assertions, {} dependents",
                    u.entity_name, u.entity_kind, u.assertions, u.dependents
                );
            }
        }

        let _ = writeln!(out);
        let _ = writeln!(out, "Full listing: cog index --verbose");
        let _ = writeln!(out, "Uncovered only: cog index --uncovered");

        out
    }

    /// Render scout suggestions compactly: blind entities collapsed to count + sample.
    pub fn render_scouts(scouts: &[ScoutSuggestion]) -> String {
        let mut out = String::new();
        if scouts.is_empty() {
            return out;
        }
        let _ = writeln!(out, "\nScout before implementing:");

        // Blind entities: count only — individual names are not actionable.
        let blind: Vec<&ScoutSuggestion> = scouts
            .iter()
            .filter(|s| s.action == ScoutAction::Assert)
            .collect();
        if !blind.is_empty() {
            let _ = writeln!(out, "  [assert] {} blind entities in subgraph", blind.len());
        }

        out
    }
}

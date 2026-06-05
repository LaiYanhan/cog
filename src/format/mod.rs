pub mod text;

pub use text::TextRenderer;

use crate::domain::*;

/// Output format for CLI.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

// Free function re-exports for backward compatibility with command modules.
pub fn short_id(id: &str) -> &str {
    TextRenderer::short_id(id)
}
pub fn query_report(
    entity: &Entity,
    assertions: &[(Assertion, Vec<Evidence>)],
    related: &[RelatedEntity],
) -> String {
    TextRenderer::query_report(entity, assertions, related)
}
pub fn cascade_report(result: &CascadeReport) -> String {
    TextRenderer::cascade_report(result)
}
pub fn impact_report(result: &ImpactCard) -> String {
    TextRenderer::impact_report(result)
}
pub fn trace_report(result: &TraceTree) -> String {
    TextRenderer::trace_report(result)
}
pub fn stats_report(stats: &ModelStats) -> String {
    TextRenderer::stats_report(stats)
}
pub fn entity_index_with_counts(entities: &[(Entity, usize)]) -> String {
    TextRenderer::entity_index_with_counts(entities)
}
pub fn diff_summary(summary: &crate::repo::diff::DiffSummary) -> String {
    TextRenderer::diff_summary(summary)
}
pub fn diff_item_detail(index: usize, item: &crate::repo::diff::DiffItem) -> String {
    TextRenderer::diff_item_detail(index, item)
}
pub fn merge_plan(diff: &crate::repo::diff::ModelDiff) -> String {
    TextRenderer::merge_plan(diff)
}
pub fn item_label(item: &crate::repo::diff::DiffItem) -> String {
    TextRenderer::item_label(item)
}
pub fn branch_list_report(branches: &[crate::repo::branch::BranchInfo]) -> String {
    TextRenderer::branch_list_report(branches)
}
pub fn assertion_created(
    assertion: &Assertion,
    entity: &Entity,
    depends_on: Option<&str>,
) -> String {
    TextRenderer::assertion_created(assertion, entity, depends_on)
}
pub fn dependency_recorded(from: &Entity, to: &Entity, kind: EntityRelationKind) -> String {
    TextRenderer::dependency_recorded(from, to, kind)
}

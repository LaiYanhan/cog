pub mod json;
pub mod text;

pub use text::TextRenderer;

use crate::domain::*;

/// Output format for CLI.
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

/// Types that can render themselves as human-readable text.
pub trait Renderable {
    fn render_text(&self) -> String;
}

/// Route a report to text or JSON based on output format.
pub fn emit_report<T: serde::Serialize + Renderable>(report: &T, format: OutputFormat) -> String {
    match format {
        OutputFormat::Text => report.render_text(),
        OutputFormat::Json => json::JsonRender::render(report),
    }
}

// Free function re-exports for backward compatibility with command modules.
pub fn short_id(id: &str) -> &str {
    TextRenderer::short_id(id)
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

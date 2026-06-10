pub mod json;
pub mod text;

pub use text::TextRenderer;

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

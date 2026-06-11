use crate::domain::*;
use crate::format::Renderable;

use super::TextRenderer;

impl Renderable for CascadeReport {
    fn render_text(&self) -> String {
        TextRenderer::cascade_report(self, "", &[])
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
        TextRenderer::query_report(
            &self.entity,
            &self.assertions,
            &self.related,
            &self.related_assertion_counts,
            self.relations_detail,
        )
    }
}

impl Renderable for EntityIndex {
    fn render_text(&self) -> String {
        if self.summary_mode
            && let Some(ref coverage) = self.coverage_summary
        {
            return TextRenderer::index_summary(coverage);
        }
        TextRenderer::entity_index_with_counts(&self.entities)
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

impl Renderable for SyncReport {
    fn render_text(&self) -> String {
        TextRenderer::sync_report(self)
    }
}

impl Renderable for NextReport {
    fn render_text(&self) -> String {
        TextRenderer::next_report(self)
    }
}

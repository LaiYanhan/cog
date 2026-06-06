use serde::Serialize;

/// JSON renderer for report types.
///
/// A thin structural wrapper around `serde_json` that isolates JSON
/// serialization details from the rest of the format module.
/// All report types that derive `Serialize` can be rendered through this type.
pub struct JsonRender;

impl JsonRender {
    /// Render a serializable report as pretty-printed JSON.
    pub fn render<T: Serialize>(report: &T) -> String {
        serde_json::to_string_pretty(report)
            .unwrap_or_else(|e| format!("{{\"error\": \"json serialization failed: {e}\"}}"))
    }
}

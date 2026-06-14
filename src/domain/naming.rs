//! Unified qualified-name handling.
//!
//! All entity names in the model use `::` as their separator (the Rust
//! convention).  User input, however, may arrive in Python-style `.` notation
//! (e.g. `pkg.mod.Class.method`).  This module centralises every operation on
//! qualified names so that separator logic lives in exactly one place:
//!
//! | Function               | Replaces                                    |
//! |------------------------|---------------------------------------------|
//! | [`last_segment`]       | scattered `rsplit("::").next()`             |
//! | [`parent_qname`]       | scattered `rsplit_once("::")`               |
//! | [`ancestors`]          | duplicated `split("::")` hierarchy loops     |
//! | [`normalize`]          | inline `.replace('.', "::")`                |
//! | [`path_to_qualified`]  | `sync_cmd::path_to_qualified` (+ verify copy)|

use std::path::Path;

/// The canonical separator for qualified entity names.
pub const SEP: &str = "::";

/// The last segment of a `::`-separated qualified name.
///
/// `"repo::sqlite::SqliteRepository"` → `"SqliteRepository"`.
/// Returns the input unchanged if no `::` separator is present.
pub fn last_segment(qname: &str) -> &str {
    qname.rsplit(SEP).next().unwrap_or(qname)
}

/// The parent path of a `::`-separated qualified name.
///
/// `"cog::repo::sqlite::SqliteRepository"` → `Some("cog::repo::sqlite")`.
/// Returns `None` when there is only one segment.
pub fn parent_qname(qname: &str) -> Option<&str> {
    qname.rsplit_once(SEP).map(|(p, _)| p)
}

/// All ancestor qualified names, outermost-first.
///
/// `"a::b::c"` → `["a", "a::b"]`.
/// Returns an empty vec for a single-segment name.
///
/// This replaces the repeated `split("::")` + cumulative-build pattern that
/// appeared in both `sync_cmd` and `verify`.
pub fn ancestors(qname: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    for segment in qname.split(SEP) {
        if current.is_empty() {
            current.push_str(segment);
        } else {
            current.push_str(SEP);
            current.push_str(segment);
        }
        // The last segment is the name itself, not an ancestor.
        if current != qname {
            result.push(current.clone());
        }
    }
    result
}

/// Normalise a possibly Python-style qualified name to canonical `::` form.
///
/// `"pkg.mod.fn"` → `"pkg::mod::fn"`.
/// Already-canonical names pass through unchanged.
///
/// This replaces the inline `.replace('.', "::")` that was embedded inside
/// `find_entities_by_suffix`.
pub fn normalize(qname: &str) -> String {
    qname.replace('.', SEP)
}

/// Common source-directory prefixes stripped when deriving entity names from paths.
const SOURCE_PREFIXES: &[&str] = &["src/", "lib/", "pkg/"];

/// Convert a relative filesystem path to a qualified entity name.
///
/// Strips a leading `src/`, `lib/`, or `pkg/` prefix, removes the file
/// extension, then replaces `/` with `::`.
///
/// `"src/repo/sqlite.rs"` → `"repo::sqlite"`.
///
/// Previously lived in `command::sync_cmd` and was re-exported to
/// `command::verify` — now shared from here so command modules no longer
/// depend on each other for a pure utility.
pub fn path_to_qualified(relative: &Path) -> String {
    let s = relative.to_string_lossy();

    let stripped = SOURCE_PREFIXES
        .iter()
        .find_map(|prefix| s.strip_prefix(prefix))
        .unwrap_or(&s);

    let without_ext = stripped
        .rsplit_once('.')
        .map(|(base, _)| base)
        .unwrap_or(stripped);

    without_ext.replace('/', SEP)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── last_segment ──────────────────────────────────────────────────

    #[test]
    fn last_segment_qualified() {
        assert_eq!(
            last_segment("cog::repo::sqlite::SqliteRepository"),
            "SqliteRepository"
        );
    }

    #[test]
    fn last_segment_bare() {
        assert_eq!(last_segment("foo"), "foo");
    }

    // ── parent_qname ──────────────────────────────────────────────────

    #[test]
    fn parent_qname_qualified() {
        assert_eq!(
            parent_qname("cog::repo::sqlite::SqliteRepository"),
            Some("cog::repo::sqlite")
        );
    }

    #[test]
    fn parent_qname_single() {
        assert_eq!(parent_qname("foo"), None);
    }

    #[test]
    fn parent_qname_two_segments() {
        assert_eq!(parent_qname("a::b"), Some("a"));
    }

    // ── ancestors ─────────────────────────────────────────────────────

    #[test]
    fn ancestors_multi() {
        assert_eq!(ancestors("a::b::c"), vec!["a", "a::b"]);
    }

    #[test]
    fn ancestors_single() {
        assert!(ancestors("a").is_empty());
    }

    #[test]
    fn ancestors_two() {
        assert_eq!(ancestors("a::b"), vec!["a"]);
    }

    // ── normalize ─────────────────────────────────────────────────────

    #[test]
    fn normalize_dots() {
        assert_eq!(normalize("pkg.mod.fn"), "pkg::mod::fn");
    }

    #[test]
    fn normalize_already_canonical() {
        assert_eq!(normalize("pkg::mod::fn"), "pkg::mod::fn");
    }

    #[test]
    fn normalize_mixed() {
        assert_eq!(normalize("a.b::c"), "a::b::c");
    }

    // ── path_to_qualified ─────────────────────────────────────────────

    #[test]
    fn path_strips_src_prefix() {
        assert_eq!(
            path_to_qualified(Path::new("src/repo/sqlite.rs")),
            "repo::sqlite"
        );
    }

    #[test]
    fn path_strips_lib_prefix() {
        assert_eq!(path_to_qualified(Path::new("lib/utils.ex")), "utils");
    }

    #[test]
    fn path_no_prefix() {
        assert_eq!(path_to_qualified(Path::new("top/mod.rs")), "top::mod");
    }

    #[test]
    fn path_nested_dirs() {
        assert_eq!(path_to_qualified(Path::new("src/a/b/c.go")), "a::b::c");
    }
}

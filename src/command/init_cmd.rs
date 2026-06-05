use std::collections::HashMap;
use std::fmt::Write;
use std::path::PathBuf;

use anyhow::Result;

use crate::analysis::{Language, ScanConfig, Scanner};
use crate::command::CommandOutput;
use crate::domain::{EntityKind, EntityOrigin, EntityRelationKind};
use crate::repo::Repository;

/// Strips common source-directory prefixes from a relative path, then converts
/// `a/b/c.rs` → `a::b::c` (without extension).
pub(crate) fn path_to_qualified(relative: &std::path::Path) -> String {
    let s = relative.to_string_lossy();

    // Strip common source prefixes: src/, lib/, pkg/
    let stripped = s
        .strip_prefix("src/")
        .or_else(|| s.strip_prefix("lib/"))
        .or_else(|| s.strip_prefix("pkg/"))
        .unwrap_or(&s);

    // Remove file extension
    let without_ext = stripped
        .rsplit_once('.')
        .map(|(base, _)| base)
        .unwrap_or(stripped);

    without_ext.replace('/', "::")
}

/// Increment a counter in a string-keyed map using the EntityKind's Display output.
fn inc_kind(counts: &mut HashMap<String, usize>, kind: EntityKind) {
    *counts.entry(kind.to_string()).or_default() += 1;
}

/// Look up a kind counter by EntityKind.
fn get_kind(counts: &HashMap<String, usize>, kind: EntityKind) -> usize {
    counts.get(kind.to_string().as_str()).copied().unwrap_or(0)
}

pub fn execute(
    repo: &dyn Repository,
    path: &PathBuf,
    dry_run: bool,
    max_depth: Option<usize>,
    languages: Option<Vec<String>>,
) -> Result<CommandOutput> {
    let lang_filters: Option<Vec<Language>> = languages.as_ref().map(|langs| {
        langs
            .iter()
            .filter_map(|l| l.parse().ok())
            .collect::<Vec<_>>()
    });

    let config = ScanConfig {
        root: path.clone(),
        max_depth,
        languages: lang_filters,
    };

    let result = Scanner::scan(&config)?;

    if result.files_scanned == 0 {
        return Ok(CommandOutput::with_exit_code(
            format!("No source files found at {}", path.display()),
            1,
        ));
    }

    // ── Dry-run path: just summarise ──────────────────────────────────
    if dry_run {
        let mut out = String::from("DRY RUN — no changes written\n\n");
        format_summary(
            &mut out,
            &result.definitions,
            &result.files_by_language,
            result.files_scanned,
        )?;
        return Ok(CommandOutput::success(out));
    }

    // ── Build entity hierarchy ────────────────────────────────────────
    let mut dir_entities: HashMap<String, String> = HashMap::new(); // qualified_name → entity_id
    let mut file_entities: HashMap<String, String> = HashMap::new(); // qualified_name → entity_id
    let mut def_entity_ids: HashMap<String, String> = HashMap::new(); // qualified_name → entity_id
    let mut contains_count: usize = 0;
    let mut uses_count: usize = 0;

    // Collect all directory segments using the same prefix-stripping as path_to_qualified.
    let mut all_dirs: Vec<String> = Vec::new();
    for file_scan in &result.file_scans {
        let rel = file_scan.path.strip_prefix(path).unwrap_or(&file_scan.path);
        // Build directory path the same way path_to_qualified does
        let parent_rel = rel.parent().unwrap_or(std::path::Path::new(""));
        let parent_qname = path_to_qualified(parent_rel);
        if parent_qname.is_empty() {
            continue;
        }
        // Split into cumulative segments: "a::b::c" → ["a", "a::b", "a::b::c"]
        let mut current = String::new();
        for segment in parent_qname.split("::") {
            if !current.is_empty() {
                current.push_str("::");
            }
            current.push_str(segment);
            all_dirs.push(current.clone());
        }
    }
    all_dirs.sort();
    all_dirs.dedup();

    // Create directory Module entities + contains hierarchy
    for dir_qname in &all_dirs {
        let entity = repo.upsert_entity(dir_qname, EntityKind::Module, EntityOrigin::Scan)?;
        dir_entities.insert(dir_qname.clone(), entity.id.clone());

        if let Some((parent, _)) = dir_qname.rsplit_once("::")
            && !parent.is_empty()
            && let Some(parent_id) = dir_entities.get(parent)
        {
            repo.add_entity_relation(parent_id, &entity.id, EntityRelationKind::Contains)?;
            contains_count += 1;
        }
    }

    // Create file Module entities + directory → file contains
    for file_scan in &result.file_scans {
        let rel = file_scan.path.strip_prefix(path).unwrap_or(&file_scan.path);
        let file_qname = path_to_qualified(rel);

        let entity = repo.upsert_entity(&file_qname, EntityKind::Module, EntityOrigin::Scan)?;
        file_entities.insert(file_qname.clone(), entity.id.clone());

        // Derive parent from the original relative path, not from the qualified name,
        // so that top-level files (e.g., src/main.rs → qualified "main") still get
        // linked to their directory entity (e.g., "src").
        if let Some(parent_rel) = rel.parent() {
            let parent_qname = path_to_qualified(parent_rel);
            if !parent_qname.is_empty()
                && let Some(parent_id) = dir_entities
                    .get(&parent_qname)
                    .or_else(|| file_entities.get(&parent_qname))
            {
                repo.add_entity_relation(parent_id, &entity.id, EntityRelationKind::Contains)?;
                contains_count += 1;
            }
        }
    }
    let mut kind_counts: HashMap<String, usize> = HashMap::new();
    let mut def_count: usize = 0;

    for def in &result.definitions {
        let entity = repo.upsert_entity(&def.qualified_name, def.kind, EntityOrigin::Scan)?;
        def_entity_ids.insert(def.qualified_name.clone(), entity.id.clone());
        def_count += 1;
        inc_kind(&mut kind_counts, def.kind);

        // Determine parent: explicit parent field, or fall back to containing file
        let parent_qname = def
            .parent
            .as_deref()
            .map(|p| {
                if p.contains("::") {
                    // Already fully qualified
                    p.to_string()
                } else {
                    // Short name — the parent is the enclosing scope (module part)
                    // e.g. auth::login::AuthManager::__init__ → parent is auth::login::AuthManager
                    def.qualified_name
                        .rsplit_once("::")
                        .map(|(module, _)| module.to_string())
                        .unwrap_or_else(|| p.to_string())
                }
            })
            .or_else(|| {
                result
                    .file_scans
                    .iter()
                    .find(|fs| {
                        fs.definitions
                            .iter()
                            .any(|d| d.qualified_name == def.qualified_name)
                    })
                    .and_then(|fs| {
                        let rel = fs.path.strip_prefix(path).unwrap_or(&fs.path);
                        let file_qname = path_to_qualified(rel);
                        file_entities
                            .contains_key(&file_qname)
                            .then_some(file_qname)
                    })
            });

        if let Some(pqname) = &parent_qname
            && let Some(parent_id) = def_entity_ids
                .get(pqname)
                .or_else(|| file_entities.get(pqname))
        {
            repo.add_entity_relation(parent_id, &entity.id, EntityRelationKind::Contains)?;
            contains_count += 1;
        }
    }

    // Build index: module_path → containing file's qualified name (avoids O(imports × files) loop).
    let mut import_file_map: HashMap<String, String> = HashMap::new();
    for file_scan in &result.file_scans {
        let rel = file_scan.path.strip_prefix(path).unwrap_or(&file_scan.path);
        let file_qname = path_to_qualified(rel);
        for imp in &file_scan.imports {
            import_file_map
                .entry(imp.module_path.clone())
                .or_insert_with(|| file_qname.clone());
        }
    }

    // Create uses relations for imports (only if target entity exists)
    for import in &result.imports {
        let file_qname = match import_file_map.get(&import.module_path) {
            Some(fq) => fq.clone(),
            None => continue,
        };
        let from_id = match file_entities.get(&file_qname) {
            Some(id) => id.clone(),
            None => continue,
        };

        // Resolve the import's module_path as a direct entity
        if let Some(target_id) = def_entity_ids.get(&import.module_path) {
            repo.add_entity_relation(&from_id, target_id, EntityRelationKind::Uses)?;
            uses_count += 1;
        }

        // Also try qualified names for individual imported items
        for name in &import.imported_names {
            let qualified = format!("{}::{}", import.module_path, name);
            if let Some(target_id) = def_entity_ids.get(&qualified) {
                repo.add_entity_relation(&from_id, target_id, EntityRelationKind::Uses)?;
                uses_count += 1;
            }
        }
    }

    // Total entities = dirs + files + definitions
    let total_entities = dir_entities.len() + file_entities.len() + def_count;
    let lang_summary = format_language_summary(&result.files_by_language);

    let mut out = String::new();
    writeln!(
        out,
        "Scanned {} files ({})",
        result.files_scanned, lang_summary
    )?;
    writeln!(
        out,
        "Created {} entities, {} contains, {} uses relations",
        total_entities, contains_count, uses_count
    )?;
    writeln!(out)?;

    let module_count =
        get_kind(&kind_counts, EntityKind::Module) + dir_entities.len() + file_entities.len();
    if module_count > 0 {
        writeln!(out, "  Module:    {}", module_count)?;
    }
    for kind in &[
        EntityKind::Type,
        EntityKind::Function,
        EntityKind::Method,
        EntityKind::Field,
    ] {
        let count = get_kind(&kind_counts, *kind);
        if count > 0 {
            let label = kind.to_string();
            let pad = 10 - label.len();
            let pad_str: String = " ".repeat(pad);
            writeln!(out, "  {}:{}{}", label, pad_str, count)?;
        }
    }

    writeln!(out)?;
    writeln!(out, "Next: cog index | cog trace <entity>")?;

    Ok(CommandOutput::success(out))
}

fn format_summary(
    out: &mut String,
    definitions: &[crate::analysis::Definition],
    files_by_language: &HashMap<String, usize>,
    files_scanned: usize,
) -> Result<()> {
    let lang_summary = format_language_summary(files_by_language);
    writeln!(out, "Scanned {} files ({})", files_scanned, lang_summary)?;

    let mut kind_counts: HashMap<String, usize> = HashMap::new();
    for def in definitions {
        inc_kind(&mut kind_counts, def.kind);
    }

    writeln!(out, "Would create {} definitions", definitions.len())?;
    writeln!(out)?;
    for kind in &[EntityKind::Module, EntityKind::Type, EntityKind::Function] {
        let count = get_kind(&kind_counts, *kind);
        if count > 0 {
            let label = kind.to_string();
            let pad = 10 - label.len();
            let pad_str: String = " ".repeat(pad);
            writeln!(out, "  {}:{}{}", label, pad_str, count)?;
        }
    }
    Ok(())
}

fn format_language_summary(files_by_language: &HashMap<String, usize>) -> String {
    if files_by_language.is_empty() {
        return "no languages".to_string();
    }
    let mut entries: Vec<_> = files_by_language.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    entries
        .iter()
        .map(|(lang, count)| format!("{}: {}", lang, count))
        .collect::<Vec<_>>()
        .join(", ")
}

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;

use crate::analysis::{Language, ScanConfig, Scanner};
use crate::command::CommandOutput;
use crate::domain::{EntityKind, EntityOrigin, EntityRelationKind, InitReport, parent_qname};
use crate::format::{self, OutputFormat};
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
    output: OutputFormat,
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

    let result = Scanner::new().scan(&config)?;

    if result.files_scanned == 0 {
        return Ok(CommandOutput::with_exit_code(
            format!("No source files found at {}", path.display()),
            1,
        ));
    }

    // ── Dry-run path: just summarise ──────────────────────────────────
    if dry_run {
        let mut entity_counts: HashMap<String, usize> = HashMap::new();
        for def in &result.definitions {
            *entity_counts.entry(def.kind.to_string()).or_default() += 1;
        }
        let report = InitReport {
            files_scanned: result.files_scanned,
            files_by_language: result.files_by_language.clone(),
            entities_created: 0,
            relations_created: 0,
            entity_counts_by_kind: entity_counts,
            dry_run: true,
        };
        return Ok(CommandOutput::success(format::emit_report(&report, output)));
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

        if let Some(parent) = parent_qname(dir_qname)
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
                    parent_qname(&def.qualified_name)
                        .map(|m| m.to_string())
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

    // ── Compute fan_in/fan_out metrics ───────────────────────────────
    {
        let all_entities = repo.list_entities()?;
        let relations = repo.list_entity_relations()?;

        // Build adjacency: entity_id → (incoming_count, outgoing_count)
        let mut fan_counts: HashMap<&str, (u32, u32)> = HashMap::new();
        for entity in &all_entities {
            fan_counts.entry(&entity.id).or_insert((0, 0));
        }
        for rel in &relations {
            // from → to: from has fan_out, to has fan_in
            if let Some(from) = fan_counts.get_mut(rel.from_entity.as_str()) {
                from.1 += 1;
            }
            if let Some(to) = fan_counts.get_mut(rel.to_entity.as_str()) {
                to.0 += 1;
            }
        }

        for entity in &all_entities {
            if let Some(&(fan_in, fan_out)) = fan_counts.get(entity.id.as_str())
                && (fan_in > 0 || fan_out > 0)
            {
                let mut metrics = entity.metrics.clone();
                metrics.fan_in = Some(fan_in);
                metrics.fan_out = Some(fan_out);
                let _ = repo.update_entity_metrics(&entity.id, &metrics);
            }
        }
    }

    // Build entity kind counts for the report
    let mut entity_counts: HashMap<String, usize> = HashMap::new();
    let module_count =
        get_kind(&kind_counts, EntityKind::Module) + dir_entities.len() + file_entities.len();
    if module_count > 0 {
        entity_counts.insert("module".to_string(), module_count);
    }
    for kind in &[
        EntityKind::Type,
        EntityKind::Function,
        EntityKind::Method,
        EntityKind::Field,
    ] {
        let count = get_kind(&kind_counts, *kind);
        if count > 0 {
            entity_counts.insert(kind.to_string(), count);
        }
    }

    let total_entities = dir_entities.len() + file_entities.len() + def_count;
    let report = InitReport {
        files_scanned: result.files_scanned,
        files_by_language: result.files_by_language.clone(),
        entities_created: total_entities,
        relations_created: contains_count + uses_count,
        entity_counts_by_kind: entity_counts,
        dry_run: false,
    };
    Ok(CommandOutput::success(format::emit_report(&report, output)))
}

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};

use super::session::Experiment;

/// Resolve an experiment ID (short or full UUID) to the actual file path.
///
/// Tries exact match first (`{id}.json`), then scans the experiments
/// directory for files whose stem starts with `id` (prefix match).
/// Returns `Ok(None)` if no file matches.
/// Returns an error if multiple files match the prefix (ambiguous).
fn resolve_path(id: &str, cog_dir: &Path) -> Result<Option<PathBuf>> {
    let dir = cog_dir.join("experiments");
    let direct = dir.join(format!("{id}.json"));
    if direct.exists() {
        return Ok(Some(direct));
    }
    // Short ID resolution: scan for prefix match
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return Ok(None),
    };
    let mut matches: Vec<PathBuf> = entries
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .file_stem()
                .is_some_and(|stem| stem.to_string_lossy().starts_with(id))
        })
        .map(|e| e.path())
        .collect();
    match matches.len() {
        1 => Ok(matches.pop()),
        0 => Ok(None),
        _ => {
            let ids: Vec<String> = matches
                .iter()
                .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                .collect();
            anyhow::bail!(
                "ambiguous experiment ID \"{id}\": {} matching files found\n\
                 IDs: {}\n\
                 Tip: use the full experiment ID from `cog experiment list`",
                ids.len(),
                ids.join(", ")
            )
        }
    }
}

pub fn save(experiment: &Experiment, cog_dir: &Path) -> Result<()> {
    let dir = cog_dir.join("experiments");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", experiment.id));
    let json = serde_json::to_string_pretty(experiment)?;
    std::fs::write(&path, json)?;
    Ok(())
}
pub fn load(id: &str, cog_dir: &Path) -> Result<Experiment> {
    let path = resolve_path(id, cog_dir)?.ok_or_else(|| {
        let short_id = if id.len() >= 8 { &id[..8] } else { id };
        anyhow!(
            "experiment {short_id} not found at {}\n\
             Possible causes:\n\
             1. The experiment was created from a different working directory (different .cog/)\n\
             2. The experiment ID is incorrect\n\
             3. The experiment was already committed or discarded\n\
             Tip: Run `cog experiment list` to see available experiments.",
            cog_dir.display()
        )
    })?;
    let json = std::fs::read_to_string(&path).with_context(|| {
        let short_id = if id.len() >= 8 { &id[..8] } else { id };
        format!("failed to read experiment {short_id} at {}", path.display())
    })?;
    let experiment: Experiment = serde_json::from_str(&json)?;
    Ok(experiment)
}

pub fn list(cog_dir: &Path) -> Result<Vec<String>> {
    let dir = cog_dir.join("experiments");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if let Some(name) = entry.path().file_stem() {
            ids.push(name.to_string_lossy().to_string());
        }
    }
    ids.sort();
    Ok(ids)
}

pub fn remove(id: &str, cog_dir: &Path) -> Result<()> {
    if let Some(path) = resolve_path(id, cog_dir)? {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

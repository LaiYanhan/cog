use std::path::Path;

use anyhow::Result;

use super::session::Experiment;

pub fn save(experiment: &Experiment, cog_dir: &Path) -> Result<()> {
    let dir = cog_dir.join("experiments");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", experiment.id));
    let json = serde_json::to_string_pretty(experiment)?;
    std::fs::write(&path, json)?;
    Ok(())
}

pub fn load(id: &str, cog_dir: &Path) -> Result<Experiment> {
    let path = cog_dir.join("experiments").join(format!("{id}.json"));
    let json = std::fs::read_to_string(&path)?;
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
    let path = cog_dir.join("experiments").join(format!("{id}.json"));
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A single version entry in the registry index.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IndexEntry {
    pub name: String,
    pub vers: String,
    pub deps: Vec<IndexDep>,
    pub cksum: String,
    pub features: BTreeMap<String, Vec<String>>,
    pub yanked: bool,
}

/// A dependency entry within an index record.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct IndexDep {
    pub name: String,
    pub req: String,
    pub features: Vec<String>,
    pub optional: bool,
    pub default_features: bool,
    pub target: Option<String>,
    pub kind: String,
    pub registry: Option<String>,
    pub package: Option<String>,
}

/// Map a crate name to its index file path following the Cargo registry naming convention.
pub fn name_to_path(name: &str) -> PathBuf {
    let lower = name.to_lowercase();
    match lower.len() {
        0 => PathBuf::from(&lower),
        1 => PathBuf::from("1").join(&lower),
        2 => PathBuf::from("2").join(&lower),
        3 => PathBuf::from("3").join(&lower[..1]).join(&lower),
        _ => PathBuf::from(&lower[..2]).join(&lower[2..4]).join(&lower),
    }
}

/// Overwrite an existing version entry in the index file.
///
/// If the version does not exist yet, appends instead.
pub fn upsert_entry(index_dir: &Path, entry: &IndexEntry) -> Result<()> {
    let rel_path = name_to_path(&entry.name);
    let full_path = index_dir.join(&rel_path);

    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string(entry)?;

    if full_path.exists() {
        let existing = std::fs::read_to_string(&full_path)?;
        let mut lines: Vec<String> = Vec::new();
        let mut replaced = false;

        for line in existing.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(existing_entry) = serde_json::from_str::<IndexEntry>(trimmed) {
                if existing_entry.name == entry.name && existing_entry.vers == entry.vers {
                    lines.push(json.clone());
                    replaced = true;
                    continue;
                }
            }
            lines.push(trimmed.to_string());
        }

        if !replaced {
            lines.push(json);
        }

        let mut content = lines.join("\n");
        content.push('\n');
        std::fs::write(&full_path, &content)?;
    } else {
        let mut content = json;
        content.push('\n');
        std::fs::write(&full_path, &content)?;
    }

    Ok(())
}

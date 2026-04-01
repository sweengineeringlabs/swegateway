//! Workspace path management commands
//!
//! Automates renaming and moving crates within the workspace,
//! updating Cargo.toml paths automatically.

use anyhow::{bail, Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item, Value};

/// List all workspace members with their paths
pub fn list() -> Result<()> {
    let root = workspace_root()?;
    let cargo_toml = root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml)?;
    let doc = content.parse::<DocumentMut>()?;

    println!("Workspace members:");
    println!("{:<40} {}", "PATH", "PACKAGE");
    println!("{}", "-".repeat(70));

    if let Some(members) = doc["workspace"]["members"].as_array() {
        for member in members.iter() {
            if let Some(path) = member.as_str() {
                let pkg_name = get_package_name(&root.join(path))?;
                println!("{:<40} {}", path, pkg_name);
            }
        }
    }

    Ok(())
}

/// Rename a directory and update all Cargo.toml references
pub fn rename(from: &str, to: &str, dry_run: bool) -> Result<()> {
    let root = workspace_root()?;
    let from_path = root.join(from);
    let to_path = root.join(to);

    // Validate source exists
    if !from_path.exists() {
        bail!("Source path does not exist: {}", from);
    }

    // Validate target doesn't exist
    if to_path.exists() {
        bail!("Target path already exists: {}", to);
    }

    println!("Renaming: {} -> {}", from, to);

    if dry_run {
        println!("[DRY RUN] Would rename directory");
        println!("[DRY RUN] Would update Cargo.toml paths");
        return Ok(());
    }

    // Create parent directories if needed
    if let Some(parent) = to_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Move the directory
    fs::rename(&from_path, &to_path)
        .with_context(|| format!("Failed to rename {} -> {}", from, to))?;

    // Update root Cargo.toml
    update_cargo_toml_paths(&root, from, to)?;

    println!("Done. Run `cargo check` to verify.");
    Ok(())
}

/// Move a crate to a new location
pub fn mv(source: &str, dest: &str, dry_run: bool) -> Result<()> {
    rename(source, dest, dry_run)
}

/// Sync Cargo.toml paths with actual directory structure
/// Finds and fixes any path mismatches
pub fn sync(dry_run: bool) -> Result<()> {
    let root = workspace_root()?;
    let cargo_toml = root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml)?;
    let mut doc = content.parse::<DocumentMut>()?;

    let mut fixes = Vec::new();

    // Check workspace members
    if let Some(members) = doc["workspace"]["members"].as_array() {
        for (i, member) in members.iter().enumerate() {
            if let Some(path) = member.as_str() {
                let full_path = root.join(path);
                if !full_path.exists() {
                    // Try to find the crate by package name
                    if let Some(new_path) = find_crate_by_name(&root, path)? {
                        fixes.push((i, path.to_string(), new_path));
                    } else {
                        println!("WARNING: Member not found and cannot auto-fix: {}", path);
                    }
                }
            }
        }
    }

    if fixes.is_empty() {
        println!("All paths are in sync.");
        return Ok(());
    }

    println!("Found {} path(s) to fix:", fixes.len());
    for (_, old, new) in &fixes {
        println!("  {} -> {}", old, new);
    }

    if dry_run {
        println!("[DRY RUN] Would update Cargo.toml");
        return Ok(());
    }

    // Apply fixes to members array
    if let Some(members) = doc["workspace"]["members"].as_array_mut() {
        for (i, _, new_path) in &fixes {
            if let Some(item) = members.get_mut(*i) {
                *item = Value::from(new_path.as_str()).into();
            }
        }
    }

    // Apply fixes to workspace.dependencies
    for (_, old, new) in &fixes {
        update_dependency_paths(&mut doc, old, new);
    }

    fs::write(&cargo_toml, doc.to_string())?;
    println!("Updated Cargo.toml");

    Ok(())
}

/// Bulk rename using a pattern (e.g., agents->agent)
pub fn bulk_rename(pattern: &str, dry_run: bool) -> Result<()> {
    let parts: Vec<&str> = pattern.split("->").collect();
    if parts.len() != 2 {
        bail!("Pattern must be in format 'old->new' (e.g., 'agents->agent')");
    }

    let from = parts[0].trim();
    let to = parts[1].trim();

    let root = workspace_root()?;
    let cargo_toml = root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml)?;
    let doc = content.parse::<DocumentMut>()?;

    let mut renames = Vec::new();

    // Find all paths containing the pattern
    if let Some(members) = doc["workspace"]["members"].as_array() {
        for member in members.iter() {
            if let Some(path) = member.as_str() {
                if path.contains(from) {
                    let new_path = path.replace(from, to);
                    renames.push((path.to_string(), new_path));
                }
            }
        }
    }

    if renames.is_empty() {
        println!("No paths match pattern '{}'", from);
        return Ok(());
    }

    println!("Found {} path(s) to rename:", renames.len());
    for (old, new) in &renames {
        println!("  {} -> {}", old, new);
    }

    if dry_run {
        println!("[DRY RUN] Would rename directories and update Cargo.toml");
        return Ok(());
    }

    // Perform renames (in reverse order to handle nested paths)
    let mut renamed_dirs: HashMap<String, String> = HashMap::new();
    for (old, new) in renames.iter().rev() {
        let old_path = root.join(old);
        let new_path = root.join(new);

        // Skip if already renamed by a parent directory rename
        if !old_path.exists() {
            continue;
        }

        // Check if this is a parent of other renames
        let is_parent = renames.iter().any(|(o, _)| o != old && o.starts_with(old));
        if is_parent {
            // This is a parent directory, rename it
            if let Some(parent) = new_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(&old_path, &new_path)?;
            renamed_dirs.insert(old.clone(), new.clone());
        }
    }

    // Update Cargo.toml
    update_cargo_toml_bulk(&root, &renames)?;

    println!("Done. Run `cargo check` to verify.");
    Ok(())
}

// ============================================================================
// Helper functions
// ============================================================================

fn workspace_root() -> Result<PathBuf> {
    let output = std::process::Command::new("cargo")
        .args(["locate-project", "--workspace", "--message-format=plain"])
        .output()?;

    let path = String::from_utf8(output.stdout)?;
    let cargo_toml = PathBuf::from(path.trim());
    Ok(cargo_toml.parent().unwrap().to_path_buf())
}

fn get_package_name(crate_path: &Path) -> Result<String> {
    let cargo_toml = crate_path.join("Cargo.toml");
    if !cargo_toml.exists() {
        return Ok("<not found>".to_string());
    }

    let content = fs::read_to_string(&cargo_toml)?;
    let doc = content.parse::<DocumentMut>()?;

    if let Some(name) = doc["package"]["name"].as_str() {
        Ok(name.to_string())
    } else {
        Ok("<unknown>".to_string())
    }
}

fn update_cargo_toml_paths(root: &Path, from: &str, to: &str) -> Result<()> {
    let cargo_toml = root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml)?;
    let mut doc = content.parse::<DocumentMut>()?;

    // Update workspace.members
    if let Some(members) = doc["workspace"]["members"].as_array_mut() {
        for member in members.iter_mut() {
            if let Some(path) = member.as_str() {
                if path.starts_with(from) || path.contains(&format!("/{}/", from)) || path.contains(&format!("/{}", from)) {
                    let new_path = path.replace(from, to);
                    *member = Value::from(new_path.as_str()).into();
                }
            }
        }
    }

    // Update workspace.dependencies paths
    update_dependency_paths(&mut doc, from, to);

    fs::write(&cargo_toml, doc.to_string())?;
    println!("Updated Cargo.toml");
    Ok(())
}

fn update_cargo_toml_bulk(root: &Path, renames: &[(String, String)]) -> Result<()> {
    let cargo_toml = root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml)?;
    let mut doc = content.parse::<DocumentMut>()?;

    // Update workspace.members
    if let Some(members) = doc["workspace"]["members"].as_array_mut() {
        for member in members.iter_mut() {
            if let Some(path) = member.as_str() {
                for (old, new) in renames {
                    if path == old {
                        *member = Value::from(new.as_str()).into();
                        break;
                    }
                }
            }
        }
    }

    // Update workspace.dependencies paths
    if let Some(deps) = doc["workspace"]["dependencies"].as_table_mut() {
        for (_, value) in deps.iter_mut() {
            if let Some(tbl) = value.as_inline_table_mut() {
                if let Some(path_item) = tbl.get_mut("path") {
                    if let Some(path) = path_item.as_str() {
                        for (old, new) in renames {
                            if path == old {
                                *path_item = Value::from(new.as_str());
                                break;
                            }
                        }
                    }
                }
            } else if let Some(tbl) = value.as_table_mut() {
                if let Some(path_item) = tbl.get_mut("path") {
                    if let Some(path) = path_item.as_str() {
                        for (old, new) in renames {
                            if path == old {
                                *path_item = Item::Value(Value::from(new.as_str()));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    fs::write(&cargo_toml, doc.to_string())?;
    println!("Updated Cargo.toml");
    Ok(())
}

fn update_dependency_paths(doc: &mut DocumentMut, from: &str, to: &str) {
    if let Some(deps) = doc["workspace"]["dependencies"].as_table_mut() {
        for (_, value) in deps.iter_mut() {
            if let Some(tbl) = value.as_inline_table_mut() {
                if let Some(path_item) = tbl.get_mut("path") {
                    if let Some(path) = path_item.as_str() {
                        if path.contains(from) {
                            let new_path = path.replace(from, to);
                            *path_item = Value::from(new_path.as_str());
                        }
                    }
                }
            } else if let Some(tbl) = value.as_table_mut() {
                if let Some(path_item) = tbl.get_mut("path") {
                    if let Some(path) = path_item.as_str() {
                        if path.contains(from) {
                            let new_path = path.replace(from, to);
                            *path_item = Item::Value(Value::from(new_path.as_str()));
                        }
                    }
                }
            }
        }
    }
}

fn find_crate_by_name(root: &Path, old_path: &str) -> Result<Option<String>> {
    // Extract expected package name from old path
    let expected_name = Path::new(old_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    // Search for Cargo.toml files
    for entry in glob::glob(&format!("{}/**/Cargo.toml", root.display()))? {
        if let Ok(cargo_path) = entry {
            if let Ok(content) = fs::read_to_string(&cargo_path) {
                if let Ok(doc) = content.parse::<DocumentMut>() {
                    if let Some(name) = doc["package"]["name"].as_str() {
                        if name == expected_name {
                            let crate_dir = cargo_path.parent().unwrap();
                            let relative = crate_dir.strip_prefix(root)?;
                            return Ok(Some(relative.to_string_lossy().replace('\\', "/")));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

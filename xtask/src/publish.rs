use anyhow::{Context, Result};
use cargo_metadata::{DependencyKind, MetadataCommand, Package};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::process::Command;

use crate::checksum;
use crate::index::{self, IndexDep, IndexEntry};
use crate::registry::{self, RegistryPaths};

pub fn run(packages: Vec<String>, dry_run: bool, registry_path: Option<String>) -> Result<()> {
    let paths = registry::resolve(registry_path.as_deref())?;
    let registry_url = registry::index_url(&paths);

    println!("Registry: {}", paths.root.display());
    println!("Index:    {}", paths.index.display());
    if dry_run {
        println!("Mode:     DRY RUN");
    }
    println!();

    // Get workspace metadata
    let metadata = MetadataCommand::new()
        .exec()
        .context("Failed to get workspace metadata")?;

    let workspace_members: HashSet<_> = metadata.workspace_members.iter().collect();

    // Map package name -> Package for workspace members
    let pkg_map: HashMap<&str, &Package> = metadata
        .packages
        .iter()
        .filter(|p| workspace_members.contains(&p.id))
        .map(|p| (p.name.as_str(), p))
        .collect();

    // Build a set of crate names known to live in the local registry (from index)
    let local_registry_crates = scan_local_registry_index(&paths.index);

    // Determine which crates to publish
    let to_publish: Vec<&Package> = if packages.is_empty() {
        metadata
            .packages
            .iter()
            .filter(|p| workspace_members.contains(&p.id))
            .filter(|p| is_publishable(p))
            .collect()
    } else {
        packages
            .iter()
            .map(|name| {
                pkg_map
                    .get(name.as_str())
                    .copied()
                    .with_context(|| format!("Package '{}' not found in workspace", name))
            })
            .collect::<Result<Vec<_>>>()?
    };

    if to_publish.is_empty() {
        println!("No publishable crates found.");
        return Ok(());
    }

    // Build workspace dependency map for topological sort
    let workspace_deps = build_workspace_deps(&to_publish, &pkg_map);

    let publish_names: Vec<String> = to_publish.iter().map(|p| p.name.clone()).collect();
    let sorted_names = topological_sort(&publish_names, &workspace_deps)?;

    println!("Publish order:");
    for (i, name) in sorted_names.iter().enumerate() {
        let pkg = pkg_map[name.as_str()];
        println!("  {}. {} v{}", i + 1, name, pkg.version);
    }
    println!();

    if dry_run {
        for name in &sorted_names {
            println!("[dry-run] Would publish {}", name);
        }
        println!("\nDone! (dry run)");
        return Ok(());
    }

    // Ensure registry directories exist
    registry::ensure_dirs(&paths)?;

    // ── Phase 1: Pre-register all crates in the index ──────────────────────
    println!(
        "Phase 1: Pre-registering {} crates in index...",
        sorted_names.len()
    );
    for name in &sorted_names {
        let pkg = pkg_map[name.as_str()];
        let stub = build_index_entry(
            pkg,
            PLACEHOLDER_CKSUM,
            &registry_url,
            &pkg_map,
            &local_registry_crates,
        )?;
        index::upsert_entry(&paths.index, &stub)
            .with_context(|| format!("Failed to pre-register {}", name))?;
    }
    git_commit_index(
        &paths.index,
        "pre-register crates for dependency resolution",
    )?;
    println!();

    // ── Phase 2: Package each crate, update index immediately ─────────────
    println!("Phase 2: Packaging crates...");
    for name in &sorted_names {
        let pkg = pkg_map[name.as_str()];
        let entry = package_and_copy(pkg, &paths, &registry_url, &pkg_map, &local_registry_crates)?;
        index::upsert_entry(&paths.index, &entry)
            .with_context(|| format!("Failed to update index for {}", entry.name))?;
        git_commit_index(
            &paths.index,
            &format!("publish {} v{}", entry.name, entry.vers),
        )?;
    }
    println!();

    println!("\nDone!");
    Ok(())
}

const PLACEHOLDER_CKSUM: &str =
    "0000000000000000000000000000000000000000000000000000000000000000";

const CRATES_IO_INDEX: &str = "https://github.com/rust-lang/crates.io-index";

/// Scan the local registry index directory to discover which crate names are already published.
fn scan_local_registry_index(index_dir: &std::path::Path) -> HashSet<String> {
    let mut names = HashSet::new();
    if let Ok(entries) = walk_index_files(index_dir, index_dir) {
        for path in entries {
            // The file name is the crate name
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // Skip git directory and config.json
                if name == "config.json" || path.components().any(|c| c.as_os_str() == ".git") {
                    continue;
                }
                names.insert(name.to_string());
            }
        }
    }
    names
}

fn walk_index_files(
    base: &std::path::Path,
    dir: &std::path::Path,
) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.file_name().map_or(false, |n| n == ".git") {
                continue;
            }
            if path.is_dir() {
                files.extend(walk_index_files(base, &path)?);
            } else if path.is_file() {
                let name = path.file_name().unwrap().to_str().unwrap_or("");
                if name != "config.json" {
                    files.push(path);
                }
            }
        }
    }
    Ok(files)
}

/// Returns true if the package is publishable (not marked `publish = false`).
fn is_publishable(pkg: &Package) -> bool {
    match &pkg.publish {
        Some(registries) => !registries.is_empty(),
        None => true,
    }
}

/// Build a map of crate_name -> [workspace dependency names] (non-dev only).
fn build_workspace_deps(
    to_publish: &[&Package],
    pkg_map: &HashMap<&str, &Package>,
) -> HashMap<String, Vec<String>> {
    let publish_set: HashSet<&str> = to_publish.iter().map(|p| p.name.as_str()).collect();
    let mut result: HashMap<String, Vec<String>> = HashMap::new();

    for pkg in to_publish {
        let mut deps = Vec::new();
        for dep in &pkg.dependencies {
            if dep.kind == DependencyKind::Development {
                continue;
            }
            if publish_set.contains(dep.name.as_str()) && pkg_map.contains_key(dep.name.as_str()) {
                deps.push(dep.name.clone());
            }
        }
        result.insert(pkg.name.clone(), deps);
    }

    result
}

/// Topological sort using Kahn's algorithm.
pub fn topological_sort(
    crates: &[String],
    workspace_deps: &HashMap<String, Vec<String>>,
) -> Result<Vec<String>> {
    let crate_set: HashSet<&str> = crates.iter().map(|s| s.as_str()).collect();

    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for name in crates {
        in_degree.entry(name.as_str()).or_insert(0);

        if let Some(deps) = workspace_deps.get(name.as_str()) {
            for dep in deps {
                if crate_set.contains(dep.as_str()) {
                    *in_degree.entry(name.as_str()).or_insert(0) += 1;
                    dependents
                        .entry(dep.as_str())
                        .or_default()
                        .push(name.as_str());
                }
            }
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();

    let mut initial: Vec<&str> = queue.drain(..).collect();
    initial.sort();
    queue.extend(initial);

    let mut sorted = Vec::new();
    while let Some(name) = queue.pop_front() {
        sorted.push(name.to_string());
        if let Some(deps) = dependents.get(name) {
            let mut next = Vec::new();
            for &dep_name in deps {
                let deg = in_degree.get_mut(dep_name).unwrap();
                *deg -= 1;
                if *deg == 0 {
                    next.push(dep_name);
                }
            }
            next.sort();
            queue.extend(next);
        }
    }

    if sorted.len() != crates.len() {
        anyhow::bail!(
            "Circular dependency detected among workspace crates (sorted {} of {})",
            sorted.len(),
            crates.len()
        );
    }

    Ok(sorted)
}

/// Package a crate, compute its checksum, copy the .crate file, and return
/// the final index entry (with the real checksum).
fn package_and_copy(
    pkg: &Package,
    paths: &RegistryPaths,
    registry_url: &str,
    pkg_map: &HashMap<&str, &Package>,
    local_registry_crates: &HashSet<String>,
) -> Result<IndexEntry> {
    println!("  Packaging {} v{} ...", pkg.name, pkg.version);

    let crate_file = cargo_package(pkg)?;

    let cksum = checksum::sha256_file(&crate_file)?;
    println!("    Checksum: {}...", &cksum[..16]);

    let dest_dir = paths.crates.join(&pkg.name).join(pkg.version.to_string());
    std::fs::create_dir_all(&dest_dir)
        .with_context(|| format!("Failed to create crate dir: {}", dest_dir.display()))?;
    let dest = dest_dir.join("download");
    std::fs::copy(&crate_file, &dest)
        .with_context(|| format!("Failed to copy crate to {}", dest.display()))?;
    println!("    Copied to {}", dest.display());

    build_index_entry(pkg, &cksum, registry_url, pkg_map, local_registry_crates)
}

fn cargo_package(pkg: &Package) -> Result<PathBuf> {
    let output = Command::new("cargo")
        .args(["package", "-p", &pkg.name, "--allow-dirty", "--no-verify"])
        .output()
        .with_context(|| format!("Failed to run `cargo package` for {}", pkg.name))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("cargo package failed for {}:\n{}", pkg.name, stderr);
    }

    let crate_file = PathBuf::from("target/package").join(format!(
        "{}-{}.crate",
        pkg.name, pkg.version
    ));

    if !crate_file.exists() {
        anyhow::bail!(
            "Expected crate file not found: {}",
            crate_file.display()
        );
    }

    Ok(crate_file)
}

fn build_index_entry(
    pkg: &Package,
    cksum: &str,
    registry_url: &str,
    pkg_map: &HashMap<&str, &Package>,
    local_registry_crates: &HashSet<String>,
) -> Result<IndexEntry> {
    let mut deps = Vec::new();

    for dep in &pkg.dependencies {
        if dep.kind == DependencyKind::Development {
            continue;
        }

        let kind_str = match dep.kind {
            DependencyKind::Normal => "normal",
            DependencyKind::Build => "build",
            _ => continue,
        };

        // Determine the correct registry for this dependency:
        // 1. Workspace member being published → local registry
        // 2. Already in local registry index → local registry
        // 3. Has explicit non-crates.io registry → local registry
        // 4. Otherwise → crates.io
        let is_workspace_dep = pkg_map.contains_key(dep.name.as_str());
        let is_in_local_registry = local_registry_crates.contains(&dep.name);
        let has_non_cratesio_registry = dep
            .registry
            .as_ref()
            .map_or(false, |r| !r.contains("crates.io"));

        let registry = if is_workspace_dep || is_in_local_registry || has_non_cratesio_registry {
            Some(registry_url.to_string())
        } else {
            Some(CRATES_IO_INDEX.to_string())
        };

        let (idx_name, idx_package) = match &dep.rename {
            Some(alias) => (alias.clone(), Some(dep.name.clone())),
            None => (dep.name.clone(), None),
        };

        deps.push(IndexDep {
            name: idx_name,
            req: dep.req.to_string(),
            features: dep.features.clone(),
            optional: dep.optional,
            default_features: dep.uses_default_features,
            target: dep.target.as_ref().map(|t| t.to_string()),
            kind: kind_str.to_string(),
            registry,
            package: idx_package,
        });
    }

    let features: BTreeMap<String, Vec<String>> = pkg
        .features
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    Ok(IndexEntry {
        name: pkg.name.clone(),
        vers: pkg.version.to_string(),
        deps,
        cksum: cksum.to_string(),
        features,
        yanked: false,
    })
}

fn git_commit_index(index_dir: &std::path::Path, message: &str) -> Result<()> {
    let output = Command::new("git")
        .current_dir(index_dir)
        .args(["add", "."])
        .output()
        .context("Failed to run git add in index")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git add failed in index:\n{}", stderr);
    }

    let output = Command::new("git")
        .current_dir(index_dir)
        .args(["commit", "-m", message])
        .output()
        .context("Failed to run git commit in index")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if !stderr.contains("nothing to commit") && !stdout.contains("nothing to commit") {
            anyhow::bail!("git commit failed in index:\n{}", stderr);
        }
    }

    println!("Index committed: {}", message);
    Ok(())
}

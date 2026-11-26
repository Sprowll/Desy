//! Project files on disk: default folder, recent list, load/save.

use anyhow::Result;
use desy_core::Document;
use desy_core::text::app_data_dir;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const EXTENSION: &str = "desy";
/// Extension used before the app was renamed from Desyn to Desy.
const LEGACY_EXTENSION: &str = "desyn";

pub fn projects_dir() -> PathBuf {
    let home =
        std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
    home.join("Documents").join("Desy")
}

#[derive(Default, Serialize, Deserialize)]
struct Recent {
    files: Vec<PathBuf>,
}

fn recent_file() -> PathBuf {
    app_data_dir().join("recent.json")
}

fn read_recent() -> Recent {
    std::fs::read_to_string(recent_file()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn remember(path: &Path) {
    let mut r = read_recent();
    r.files.retain(|p| p != path);
    r.files.insert(0, path.to_path_buf());
    r.files.truncate(30);
    let _ = std::fs::create_dir_all(app_data_dir());
    if let Ok(s) = serde_json::to_string_pretty(&r) {
        let _ = std::fs::write(recent_file(), s);
    }
}

pub fn forget(path: &Path) {
    let mut r = read_recent();
    r.files.retain(|p| p != path);
    if let Ok(s) = serde_json::to_string_pretty(&r) {
        let _ = std::fs::write(recent_file(), s);
    }
}

pub struct ProjectEntry {
    pub path: PathBuf,
    pub doc: Document,
    pub modified: std::time::SystemTime,
}

/// Recent projects first, then other projects of the default folder.
pub fn list_projects() -> Vec<ProjectEntry> {
    let mut paths: Vec<PathBuf> = read_recent().files;
    if let Ok(entries) = std::fs::read_dir(projects_dir()) {
        for e in entries.flatten() {
            let p = e.path();
            let ext = p.extension().and_then(|x| x.to_str());
            if (ext == Some(EXTENSION) || ext == Some(LEGACY_EXTENSION)) && !paths.contains(&p) {
                paths.push(p);
            }
        }
    }
    let mut out: Vec<ProjectEntry> = paths
        .into_iter()
        .filter_map(|path| {
            let doc = load(&path).ok()?;
            let modified = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
            Some(ProjectEntry { path, doc, modified })
        })
        .collect();
    out.sort_by_key(|a| std::cmp::Reverse(a.modified));
    out
}

pub fn load(path: &Path) -> Result<Document> {
    Document::from_json(&std::fs::read_to_string(path)?)
}

pub fn save(path: &Path, doc: &Document) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Write atomically so a crash never leaves a half-written project.
    let tmp = path.with_extension("desy.tmp");
    std::fs::write(&tmp, doc.to_json()?)?;
    std::fs::rename(&tmp, path)?;
    remember(path);
    Ok(())
}

/// A free file name for a new project in the default folder.
pub fn new_project_path(name: &str) -> PathBuf {
    let base: String = name.chars().map(|c| if c.is_alphanumeric() || " -_".contains(c) { c } else { '_' }).collect();
    let base = if base.trim().is_empty() { "Sans titre".to_string() } else { base.trim().to_string() };
    let dir = projects_dir();
    let mut p = dir.join(format!("{base}.{EXTENSION}"));
    let mut i = 2;
    while p.exists() {
        p = dir.join(format!("{base} {i}.{EXTENSION}"));
        i += 1;
    }
    p
}

/// One-time move of the data left by the app under its former name (Desyn):
/// app data folder, projects folder and `.desyn` files, recent list.
/// Never overwrites anything that already exists under the new name.
pub fn migrate_legacy() {
    let rename_dir = |new: &Path, old_name: &str| {
        let old = new.with_file_name(old_name);
        if old.is_dir() && !new.exists() {
            let _ = std::fs::rename(&old, new);
        }
    };
    let data = app_data_dir();
    rename_dir(&data, if cfg!(any(target_os = "macos", target_os = "windows")) { "Desyn" } else { "desyn" });
    let dir = projects_dir();
    let old_dir = dir.with_file_name("Desyn");
    rename_dir(&dir, "Desyn");

    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for p in entries.flatten().map(|e| e.path()) {
            if p.extension().and_then(|x| x.to_str()) == Some(LEGACY_EXTENSION) {
                let new = p.with_extension(EXTENSION);
                if !new.exists() && std::fs::rename(&p, &new).is_ok() {
                    moved.push((old_dir.join(p.file_name().unwrap_or_default()), new));
                }
            }
        }
    }
    if !moved.is_empty() {
        let mut r = read_recent();
        for f in &mut r.files {
            if let Some((_, new)) = moved.iter().find(|(old, _)| old == f) {
                *f = new.clone();
            }
        }
        if let Ok(s) = serde_json::to_string_pretty(&r) {
            let _ = std::fs::write(recent_file(), s);
        }
    }
}

//! Enumerate tracked files in a pen's materialised repos.

use std::path::{Path, PathBuf};

use anyhow::Result;

use nave_config::{PathMatcher, ScanConfig};

use crate::storage::{Pen, PenRepo, pen_repo_clone_dir};

#[derive(Debug, Clone)]
pub struct TrackedFile {
    pub owner: String,
    pub repo: String,
    pub relpath: String,
    pub abspath: PathBuf,
}

pub fn tracked_files_in_pen(
    pen_root: &Path,
    pen: &Pen,
    scan: &ScanConfig,
) -> Result<Vec<TrackedFile>> {
    let mut out = Vec::new();
    for r in &pen.repos {
        let dir = pen_repo_clone_dir(pen_root, &pen.name, &r.owner, &r.name);
        if !dir.exists() {
            continue;
        }
        out.extend(tracked_files_in_repo(&dir, r, scan)?);
    }
    Ok(out)
}

pub fn tracked_files_in_repo(
    repo_dir: &Path,
    r: &PenRepo,
    scan: &ScanConfig,
) -> Result<Vec<TrackedFile>> {
    let matcher = PathMatcher::new(&scan.tracked_paths, scan.case_insensitive)?;
    let mut out = Vec::new();
    walk(repo_dir, repo_dir, &matcher, r, &mut out)?;
    Ok(out)
}

fn walk(
    root: &Path,
    dir: &Path,
    matcher: &PathMatcher,
    r: &PenRepo,
    out: &mut Vec<TrackedFile>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        // Skip .git and other obvious noise.
        if name_str == ".git" {
            continue;
        }
        if ft.is_dir() {
            walk(root, &path, matcher, r, out)?;
            continue;
        }
        if !ft.is_file() {
            continue;
        }
        let rel = path.strip_prefix(root).unwrap_or(&path);
        let rel_str = rel
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        if matcher.is_match(&rel_str) {
            out.push(TrackedFile {
                owner: r.owner.clone(),
                repo: r.name.clone(),
                relpath: rel_str,
                abspath: path,
            });
        }
    }
    Ok(())
}

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::process::Command;
use walkdir::WalkDir;

use crate::config::Config;
use crate::db::{Database, ProjectSource};

pub struct Scanner<'a> {
    config: &'a Config,
    db: &'a mut Database,
}

impl<'a> Scanner<'a> {
    pub fn new(config: &'a Config, db: &'a mut Database) -> Self {
        Self { config, db }
    }

    /// Scan all sources and update the database
    pub fn scan_all(&mut self) -> Result<ScanResult> {
        let mut result = ScanResult::default();

        // Scan configured paths
        for path in &self.config.scan_paths.clone() {
            let found = self.scan_directory(path)?;
            result.from_paths += found;
        }

        // Scan via Spotlight
        if self.config.use_spotlight {
            let found = self.scan_spotlight()?;
            result.from_spotlight += found;
        }

        // Prune missing projects
        result.pruned = self.db.prune_missing()?;

        Ok(result)
    }

    /// Scan only configured paths
    pub fn scan_paths_only(&mut self) -> Result<ScanResult> {
        let mut result = ScanResult::default();

        for path in &self.config.scan_paths.clone() {
            let found = self.scan_directory(path)?;
            result.from_paths += found;
        }

        result.pruned = self.db.prune_missing()?;
        Ok(result)
    }

    /// Scan a directory for projects (git repos or folders with files)
    fn scan_directory(&mut self, base_path: &PathBuf) -> Result<usize> {
        if !base_path.exists() {
            return Ok(0);
        }

        let exclude_patterns = &self.config.exclude_patterns;

        // Collect all project paths first
        let mut projects_to_add = Vec::new();
        let mut git_projects = std::collections::HashSet::new();

        for entry in WalkDir::new(base_path)
            .max_depth(self.config.max_depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                // Skip hidden directories (except .git which we're looking for)
                if name.starts_with('.') && name != ".git" {
                    return false;
                }
                // Skip excluded patterns
                !exclude_patterns.iter().any(|p| name.contains(p))
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            // Look for .git directories (high priority - always a project)
            if entry.file_type().is_dir() && entry.file_name() == ".git" {
                if let Some(parent) = entry.path().parent() {
                    git_projects.insert(parent.to_path_buf());
                    projects_to_add.push(parent.to_path_buf());
                }
            }
        }

        // Second pass: find non-git project folders (like blog drafts)
        // Only index "leaf" project folders - folders with files that are not inside git projects
        // and not inside other already-indexed non-git folders
        let mut non_git_projects = Vec::new();

        for entry in WalkDir::new(base_path)
            .max_depth(self.config.max_depth)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !name.starts_with('.')
                    && !exclude_patterns.iter().any(|p| name.contains(p))
            })
        {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };

            if !entry.file_type().is_dir() {
                continue;
            }

            let dir_path = entry.path();

            // Skip if already a git project or inside a git project
            if git_projects.contains(dir_path) {
                continue;
            }
            if git_projects.iter().any(|gp| dir_path.starts_with(gp)) {
                continue;
            }

            // Check if this directory contains files (not just subdirectories)
            if let Ok(contents) = std::fs::read_dir(dir_path) {
                let has_files = contents
                    .filter_map(|e| e.ok())
                    .any(|e| {
                        if let Ok(ft) = e.file_type() {
                            ft.is_file() && !e.file_name().to_string_lossy().starts_with('.')
                        } else {
                            false
                        }
                    });

                if has_files {
                    non_git_projects.push(dir_path.to_path_buf());
                }
            }
        }

        // Filter out parent folders that have child folders with files
        // Keep only the deepest (leaf) project folders
        let filtered_non_git: Vec<_> = non_git_projects
            .iter()
            .filter(|path| {
                // Keep this path only if no OTHER path is its child (descendant)
                !non_git_projects.iter().any(|other| {
                    other != *path && other.starts_with(*path)
                })
            })
            .cloned()
            .collect();

        projects_to_add.extend(filtered_non_git);

        // Batch insert for performance
        self.db.upsert_projects_batch(&projects_to_add, ProjectSource::Scan)
    }

    /// Scan for git repositories using macOS Spotlight (mdfind)
    /// Uses a SINGLE compound query instead of 9 separate queries (9x faster)
    fn scan_spotlight(&mut self) -> Result<usize> {
        let mut seen_paths = std::collections::HashSet::new();
        let mut projects_to_add = Vec::new();

        // Project marker files that Spotlight can find
        let markers = [
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "go.mod",
            "Gemfile",
            "pom.xml",
            "build.gradle",
            "CMakeLists.txt",
            "Makefile",
            // Documentation projects
            "docs.json",      // Mintlify
            "mkdocs.yml",     // MkDocs
            "docusaurus.config.js", // Docusaurus
        ];

        // Build single compound OR query (9x faster than 9 separate queries)
        let query = markers
            .iter()
            .map(|m| format!("kMDItemFSName == '{}'", m))
            .collect::<Vec<_>>()
            .join(" || ");

        for search_path in &self.config.spotlight_paths.clone() {
            if !search_path.exists() {
                continue;
            }

            // Use mdfind with -0 for null-terminated output (safer for paths with newlines)
            let output = Command::new("mdfind")
                .arg("-0")
                .arg("-onlyin")
                .arg(search_path)
                .arg(&query)
                .output()
                .context("Failed to execute mdfind")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                if !stderr.is_empty() {
                    eprintln!("Warning: mdfind issue: {}", stderr.trim());
                }
                continue;
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            // Split by null character for -0 flag, or newlines as fallback
            let lines: Box<dyn Iterator<Item = &str>> = if stdout.contains('\0') {
                Box::new(stdout.split('\0').filter(|s| !s.is_empty()))
            } else {
                Box::new(stdout.lines())
            };

            for line in lines {
                let marker_path = PathBuf::from(line);
                if let Some(project_dir) = marker_path.parent() {
                    // Skip if already seen
                    if seen_paths.contains(project_dir) {
                        continue;
                    }

                    // Only include if it has a .git directory (it's a real project)
                    let git_dir = project_dir.join(".git");
                    if !git_dir.exists() {
                        continue;
                    }

                    // Skip if path matches any exclude pattern
                    let path_str = project_dir.to_string_lossy();
                    if self.config.exclude_patterns.iter().any(|p| path_str.contains(p)) {
                        continue;
                    }

                    seen_paths.insert(project_dir.to_path_buf());
                    projects_to_add.push(project_dir.to_path_buf());
                }
            }
        }

        // Batch insert for performance
        self.db.upsert_projects_batch(&projects_to_add, ProjectSource::Spotlight)
    }
}

#[derive(Debug, Default)]
pub struct ScanResult {
    pub from_paths: usize,
    pub from_spotlight: usize,
    pub pruned: usize,
}

impl ScanResult {
    pub fn total(&self) -> usize {
        self.from_paths + self.from_spotlight
    }
}

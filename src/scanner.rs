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

    /// Scan only via Spotlight
    pub fn scan_spotlight_only(&mut self) -> Result<ScanResult> {
        let mut result = ScanResult::default();
        result.from_spotlight = self.scan_spotlight()?;
        result.pruned = self.db.prune_missing()?;
        Ok(result)
    }

    /// Scan a directory for git repositories
    fn scan_directory(&mut self, base_path: &PathBuf) -> Result<usize> {
        if !base_path.exists() {
            return Ok(0);
        }

        let exclude_patterns = &self.config.exclude_patterns;

        // Collect all project paths first
        let mut projects_to_add = Vec::new();

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

            // Look for .git directories
            if entry.file_type().is_dir() && entry.file_name() == ".git" {
                if let Some(parent) = entry.path().parent() {
                    projects_to_add.push(parent.to_path_buf());
                }
            }
        }

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

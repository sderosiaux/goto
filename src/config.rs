use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    /// Paths to scan for projects (in addition to Spotlight)
    #[serde(default)]
    pub scan_paths: Vec<PathBuf>,

    /// Enable Spotlight integration
    #[serde(default = "default_true")]
    pub use_spotlight: bool,

    /// Paths to search via Spotlight (defaults to home directory)
    #[serde(default = "default_spotlight_paths")]
    pub spotlight_paths: Vec<PathBuf>,

    /// Maximum depth when scanning directories
    #[serde(default = "default_max_depth")]
    pub max_depth: usize,

    /// Command to run after navigating (e.g., "claude")
    #[serde(default)]
    pub post_command: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_max_depth() -> usize {
    5
}

fn default_spotlight_paths() -> Vec<PathBuf> {
    if let Some(home) = dirs::home_dir() {
        vec![home]
    } else {
        vec![]
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            scan_paths: vec![],
            use_spotlight: true,
            spotlight_paths: default_spotlight_paths(),
            max_depth: 5,
            post_command: Some("claude".to_string()),
        }
    }
}

impl Config {
    /// Get the configuration directory path
    pub fn config_dir() -> Result<PathBuf> {
        ProjectDirs::from("dev", "goto", "goto")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .context("Could not determine config directory")
    }

    /// Get the config file path
    pub fn config_path() -> Result<PathBuf> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    /// Get the database file path
    pub fn db_path() -> Result<PathBuf> {
        let data_dir = ProjectDirs::from("dev", "goto", "goto")
            .map(|dirs| dirs.data_dir().to_path_buf())
            .context("Could not determine data directory")?;
        Ok(data_dir.join("cache.db"))
    }

    /// Load config from file, or create default if it doesn't exist
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config from {}", config_path.display()))?;
            toml::from_str(&content)
                .with_context(|| format!("Failed to parse config from {}", config_path.display()))
        } else {
            let config = Self::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Save config to file
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;

        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config to {}", config_path.display()))?;

        Ok(())
    }

    /// Add a path to scan_paths
    pub fn add_path(&mut self, path: PathBuf) -> Result<()> {
        let canonical = path.canonicalize()
            .with_context(|| format!("Path does not exist: {}", path.display()))?;

        if !self.scan_paths.contains(&canonical) {
            self.scan_paths.push(canonical);
            self.save()?;
        }
        Ok(())
    }

    /// Remove a path from scan_paths
    pub fn remove_path(&mut self, path: &PathBuf) -> Result<bool> {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let initial_len = self.scan_paths.len();
        self.scan_paths.retain(|p| p != &canonical && p != path);

        if self.scan_paths.len() != initial_len {
            self.save()?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

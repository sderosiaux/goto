use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "goto")]
#[command(about = "Quickly navigate to recent projects with fuzzy matching")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Search query (shortcut for `goto find <query>`)
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Find and go to a project matching the query
    Find {
        /// Search query (fuzzy matched against project names and paths)
        query: String,

        /// Show all matches instead of just the best one
        #[arg(short, long)]
        all: bool,
    },

    /// Scan and index projects from configured paths and Spotlight
    Scan {
        /// Only scan Spotlight, skip configured paths
        #[arg(long)]
        spotlight_only: bool,

        /// Only scan configured paths, skip Spotlight
        #[arg(long)]
        paths_only: bool,
    },

    /// List all indexed projects
    List {
        /// Sort by: recent, frecency, name
        #[arg(short, long, default_value = "frecency")]
        sort: SortOrder,

        /// Maximum number of projects to show
        #[arg(short, long, default_value = "20")]
        limit: usize,
    },

    /// Add a path to the configuration
    Add {
        /// Path to add to the scan list
        path: PathBuf,
    },

    /// Remove a path from the configuration
    Remove {
        /// Path to remove from the scan list
        path: PathBuf,
    },

    /// Show current configuration
    Config,

    /// Clear the cache and re-scan
    Refresh,
}

#[derive(Clone, Debug, Default)]
pub enum SortOrder {
    Recent,
    #[default]
    Frecency,
    Name,
}

impl std::str::FromStr for SortOrder {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "recent" | "r" => Ok(SortOrder::Recent),
            "frecency" | "f" => Ok(SortOrder::Frecency),
            "name" | "n" => Ok(SortOrder::Name),
            _ => Err(format!("Unknown sort order: {s}. Use: recent, frecency, or name")),
        }
    }
}

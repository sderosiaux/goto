use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "goto")]
#[command(about = "Quickly navigate to projects with fuzzy + semantic search")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,

    /// Search query (fuzzy + semantic search)
    #[arg(value_name = "QUERY", trailing_var_arg = true)]
    pub query: Vec<String>,

    /// Show all matches instead of just the best one
    #[arg(short, long)]
    pub all: bool,

    /// Number of results to show (with -a)
    #[arg(short = 'n', long, default_value = "10")]
    pub limit: usize,

    /// Show debug information
    #[arg(long)]
    pub debug: bool,

    /// Just cd, don't run post command
    #[arg(short = 'c', long)]
    pub cd_only: bool,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show recently accessed projects
    Recent {
        /// Number of recent projects to show
        #[arg(short, long, default_value = "5")]
        limit: usize,
    },

    /// Show project access statistics
    Stats,

    /// Scan directories and index projects for semantic search
    Update {
        /// Re-index all projects (clear existing embeddings first)
        #[arg(short, long)]
        force: bool,
    },

    /// List all indexed projects
    List {
        /// Sort by: recent, frecency, name
        #[arg(short, long, default_value = "frecency")]
        sort: SortOrder,

        /// Maximum number of projects to show (ignored if --all)
        #[arg(short, long, default_value = "20")]
        limit: usize,

        /// Show all projects (no limit)
        #[arg(short, long)]
        all: bool,

        /// Show git branch and dirty status
        #[arg(short, long, default_value = "true")]
        git: bool,
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

    /// Run ranking tests from ~/.config/goto/tests.toml
    Test,
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

mod cli;
mod config;
mod db;
mod embedding;
mod scanner;
mod semantic;

use anyhow::Result;
use chrono::{Duration, Utc};
use clap::Parser;
use std::path::Path;
use std::process::Command;

use cli::{Cli, Commands, SortOrder};
use config::Config;
use db::{Database, Project};
use scanner::Scanner;

fn main() -> Result<()> {
    let cli = Cli::parse();
    embedding::set_debug(cli.debug);
    let config = Config::load()?;
    let mut db = Database::open()?;

    // If a query is provided, search for it
    // Special case: "-" means show recent projects
    if !cli.query.is_empty() {
        let query = cli.query.join(" ");
        if query == "-" {
            return show_recent(5, &config, &db);
        }
        return find_project(&query, cli.all, cli.limit, &config, &db);
    }

    match cli.command {
        Some(Commands::Recent { limit }) => {
            show_recent(limit, &config, &db)
        }
        Some(Commands::Stats) => {
            show_stats(&db)
        }
        Some(Commands::Update { force }) => {
            update_all(force, &config, &mut db)
        }
        Some(Commands::List { sort, limit, all, git }) => {
            let actual_limit = if all { usize::MAX } else { limit };
            list_projects(sort, actual_limit, git, &db)
        }
        Some(Commands::Add { path }) => {
            add_path(path, &mut Config::load()?)
        }
        Some(Commands::Remove { path }) => {
            remove_path(path, &mut Config::load()?)
        }
        Some(Commands::Config) => {
            show_config(&config)
        }
        Some(Commands::Test) => {
            run_tests(&db)
        }
        None => {
            // No command and no query - show help hint
            eprintln!("\x1b[33mUsage:\x1b[0m goto <query> or goto --help for more options");
            std::process::exit(1);
        }
    }
}

/// Get git branch and dirty status for a project
fn get_git_status(path: &Path) -> Option<(String, bool)> {
    // Get current branch
    let branch_output = Command::new("git")
        .args(["-C", &path.to_string_lossy(), "rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;

    if !branch_output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&branch_output.stdout).trim().to_string();

    // Check if dirty (has uncommitted changes)
    let status_output = Command::new("git")
        .args(["-C", &path.to_string_lossy(), "status", "--porcelain"])
        .output()
        .ok()?;

    let is_dirty = !status_output.stdout.is_empty();

    Some((branch, is_dirty))
}

/// Show recently accessed projects
fn show_recent(limit: usize, _config: &Config, db: &Database) -> Result<()> {
    let mut projects = db.get_all_projects()?;

    // Filter to only accessed projects and sort by recency
    projects.retain(|p| p.access_count > 0);
    projects.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));

    if projects.is_empty() {
        eprintln!("\x1b[33m⚠\x1b[0m No recently accessed projects.");
        eprintln!("  Use \x1b[1mgoto <query>\x1b[0m to navigate to a project first.");
        return Ok(());
    }

    eprintln!("\x1b[36mRecent projects:\x1b[0m\n");

    for (i, project) in projects.iter().take(limit).enumerate() {
        let git_info = get_git_status(&project.path)
            .map(|(branch, dirty)| {
                let dirty_marker = if dirty { "*" } else { "" };
                format!(" \x1b[33m{}{}\x1b[0m", branch, dirty_marker)
            })
            .unwrap_or_default();

        eprintln!(
            "  \x1b[33m{}.\x1b[0m \x1b[1m{}\x1b[0m{} \x1b[90m{}\x1b[0m",
            i + 1,
            project.name,
            git_info,
            project.path.display()
        );
    }

    eprintln!("\n\x1b[90mTip: goto <number> to navigate (e.g., goto 1)\x1b[0m");

    // If user provides a number, navigate to that project
    Ok(())
}

/// Show project access statistics
fn show_stats(db: &Database) -> Result<()> {
    let projects = db.get_all_projects()?;

    if projects.is_empty() {
        eprintln!("\x1b[31m✗\x1b[0m No projects indexed yet.");
        return Ok(());
    }

    let total = projects.len();
    let accessed: Vec<&Project> = projects.iter().filter(|p| p.access_count > 0).collect();
    let total_accesses: i64 = projects.iter().map(|p| p.access_count).sum();

    // Projects accessed in last 7 days
    let week_ago = Utc::now() - Duration::days(7);
    let active_this_week: Vec<&Project> = accessed.iter()
        .filter(|p| p.last_accessed > week_ago)
        .copied()
        .collect();

    // Top 5 most accessed
    let mut by_access = projects.clone();
    by_access.sort_by(|a, b| b.access_count.cmp(&a.access_count));

    eprintln!("\x1b[36mProject Statistics\x1b[0m\n");
    eprintln!("  \x1b[90mTotal indexed:\x1b[0m     {}", total);
    eprintln!("  \x1b[90mEver accessed:\x1b[0m     {}", accessed.len());
    eprintln!("  \x1b[90mActive this week:\x1b[0m  {}", active_this_week.len());
    eprintln!("  \x1b[90mTotal navigations:\x1b[0m {}", total_accesses);

    if !by_access.is_empty() && by_access[0].access_count > 0 {
        eprintln!("\n\x1b[36mMost accessed:\x1b[0m\n");
        for project in by_access.iter().take(5).filter(|p| p.access_count > 0) {
            eprintln!(
                "  \x1b[32m{:>3}x\x1b[0m \x1b[1m{}\x1b[0m",
                project.access_count,
                project.name
            );
        }
    }

    if !active_this_week.is_empty() {
        eprintln!("\n\x1b[36mActive this week:\x1b[0m\n");
        for project in active_this_week.iter().take(5) {
            let days_ago = (Utc::now() - project.last_accessed).num_days();
            let when = if days_ago == 0 { "today".to_string() } else { format!("{}d ago", days_ago) };
            eprintln!(
                "  \x1b[90m{:>6}\x1b[0m \x1b[1m{}\x1b[0m",
                when,
                project.name
            );
        }
    }

    Ok(())
}

/// Minimum semantic score to accept a match (below this = no match)
const SEMANTIC_MIN_THRESHOLD: f64 = 55.0;

/// Boost score if project name contains query
const SUBSTRING_BOOST: f32 = 20.0;

/// Stronger boost if project name exactly matches query
const EXACT_NAME_BOOST: f32 = 40.0;

/// Smaller boost if query words found in metadata (README, folders, types)
const METADATA_BOOST: f32 = 10.0;

/// Calculate boosted score based on name and metadata matching
fn calculate_boosted_score(
    project_name: &str,
    query_lower: &str,
    base_score: f32,
    embedded_text: Option<&str>,
) -> f32 {
    let name_lower = project_name.to_lowercase();

    // Check for exact match first (strongest boost)
    if name_lower == query_lower {
        return (base_score + EXACT_NAME_BOOST).min(100.0);
    }

    // Check if name contains the full query
    if name_lower.contains(query_lower) {
        return (base_score + SUBSTRING_BOOST).min(100.0);
    }

    // Check if name contains ALL significant words from the query (3+ chars)
    let query_words: Vec<&str> = query_lower
        .split_whitespace()
        .filter(|w| w.len() >= 3)
        .collect();

    if !query_words.is_empty() {
        let all_words_match = query_words.iter().all(|w| name_lower.contains(*w));
        if all_words_match {
            return (base_score + SUBSTRING_BOOST).min(100.0);
        }

        // Check if ALL query words appear in embedded metadata
        if let Some(text) = embedded_text {
            let text_lower = text.to_lowercase();
            let all_in_metadata = query_words.iter().all(|w| text_lower.contains(*w));
            if all_in_metadata {
                return (base_score + METADATA_BOOST).min(100.0);
            }
        }
    }

    base_score
}

fn find_project(query: &str, show_all: bool, limit: usize, config: &Config, db: &Database) -> Result<()> {
    let projects = db.get_all_projects()?;

    if projects.is_empty() {
        eprintln!("\x1b[31m✗\x1b[0m No projects indexed yet.");
        eprintln!("  Run \x1b[1mgoto scan\x1b[0m to discover projects.");
        std::process::exit(1);
    }

    // If show_all, just display semantic matches
    if show_all {
        return show_all_matches(query, limit, db);
    }

    // Step 1: Check for exact name match (fast path)
    let query_lower = query.to_lowercase();
    if let Some(exact) = projects.iter().find(|p| p.name.to_lowercase() == query_lower) {
        db.mark_accessed(&exact.path)?;
        println!("{}", exact.path.display());
        if let Some(cmd) = &config.post_command {
            eprintln!("__GOTO_POST_CMD__:{}", cmd);
        }
        return Ok(());
    }

    // Step 2: Use semantic search
    let best_project = find_best_match(query, &projects, db)?;

    match best_project {
        Some((project, score, is_semantic)) => {
            // Mark as accessed
            db.mark_accessed(&project.path)?;

            // Output path for the shell function to cd to
            println!("{}", project.path.display());

            // Show match info on stderr (doesn't interfere with path)
            if is_semantic {
                eprintln!(
                    "\x1b[35m◆\x1b[0m \x1b[1m{}\x1b[0m \x1b[90m(semantic: {:.0}%)\x1b[0m",
                    project.name, score
                );
            }

            // Output post command if configured
            if let Some(cmd) = &config.post_command {
                eprintln!("__GOTO_POST_CMD__:{}", cmd);
            }
        }
        None => {
            eprintln!("\x1b[31m✗\x1b[0m No projects matching '\x1b[1m{query}\x1b[0m'");
            eprintln!("  Try a different query or run \x1b[1mgoto list\x1b[0m to see all projects.");
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Find the best match using semantic search with substring boost
fn find_best_match(
    query: &str,
    _projects: &[Project],
    db: &Database,
) -> Result<Option<(Project, f64, bool)>> {
    let (indexed, _) = db.embedding_stats()?;
    if indexed == 0 {
        return Ok(None);
    }

    // Get more results to find matching names
    if let Ok(results) = semantic::semantic_search(db, query, 10) {
        let query_lower = query.to_lowercase();

        // Apply name and metadata-based boost and find best
        let best = results
            .into_iter()
            .map(|(project, score)| {
                let embedded_text = db.get_embedded_text(&project.path).ok().flatten();
                let boosted = calculate_boosted_score(
                    &project.name,
                    &query_lower,
                    score,
                    embedded_text.as_deref(),
                );
                (project, boosted)
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        if let Some((project, score)) = best {
            if score as f64 >= SEMANTIC_MIN_THRESHOLD {
                return Ok(Some((project, score as f64, true)));
            }
        }
    }

    Ok(None)
}

/// Show semantic search results with substring boost
fn show_all_matches(query: &str, limit: usize, db: &Database) -> Result<()> {
    let (indexed, _) = db.embedding_stats()?;
    if indexed == 0 {
        eprintln!("\x1b[31m✗\x1b[0m No projects indexed for semantic search.");
        eprintln!("  Run \x1b[1mgoto update\x1b[0m to index projects.");
        std::process::exit(1);
    }

    // Fetch more than needed to allow for boosting reordering
    let fetch_limit = (limit * 2).max(20);
    if let Ok(results) = semantic::semantic_search(db, query, fetch_limit) {
        let query_lower = query.to_lowercase();

        // Boost scores for name and metadata matches and re-sort
        let mut boosted: Vec<_> = results
            .into_iter()
            .map(|(project, score)| {
                let embedded_text = db.get_embedded_text(&project.path).ok().flatten();
                let boosted_score = calculate_boosted_score(
                    &project.name,
                    &query_lower,
                    score,
                    embedded_text.as_deref(),
                );
                (project, boosted_score)
            })
            .collect();

        boosted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Find duplicate names to show parent dir
        let names: Vec<_> = boosted.iter().take(limit).map(|(p, _)| &p.name).collect();

        for (i, (project, score)) in boosted.iter().take(limit).enumerate() {
            let has_duplicate = names.iter().filter(|n| **n == &project.name).count() > 1;
            let display_name = if has_duplicate {
                // Show full path with ~ for home directory
                let home = dirs::home_dir().unwrap_or_default();
                let path_str = if project.path.starts_with(&home) {
                    format!("~/{}", project.path.strip_prefix(&home).unwrap().display())
                } else {
                    project.path.display().to_string()
                };
                format!("{} \x1b[90m({})\x1b[0m", project.name, path_str)
            } else {
                project.name.clone()
            };

            eprintln!(
                "\x1b[35m{}.\x1b[0m \x1b[1m{}\x1b[0m \x1b[90m({:.0}%)\x1b[0m",
                i + 1,
                display_name,
                score
            );
        }
    }

    Ok(())
}

/// Test case structure
#[derive(Debug, serde::Deserialize)]
struct TestCase {
    query: String,
    expected: Vec<String>,
    #[serde(default = "default_top_n")]
    top_n: usize,
}

fn default_top_n() -> usize { 3 }

#[derive(Debug, serde::Deserialize)]
struct TestFile {
    tests: Vec<TestCase>,
}

/// Run ranking tests from config file
fn run_tests(db: &Database) -> Result<()> {
    let config_dir = directories::ProjectDirs::from("", "", "goto")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".config/goto"));

    let test_file = config_dir.join("tests.toml");

    if !test_file.exists() {
        // Create example test file
        let example = r#"# Ranking tests - run with: goto test
# Each test checks if expected projects appear in top N results

[[tests]]
query = "console"
expected = ["console-plus-web", "console-plus-web-2"]
top_n = 3

[[tests]]
query = "cache en rust"
expected = ["foyer"]
top_n = 3

[[tests]]
query = "kafka"
expected = ["kafka", "apache-kafka-2"]
top_n = 5
"#;
        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(&test_file, example)?;
        eprintln!("\x1b[32m✓\x1b[0m Created example test file: {}", test_file.display());
        eprintln!("  Edit it and run \x1b[1mgoto test\x1b[0m again");
        return Ok(());
    }

    let content = std::fs::read_to_string(&test_file)?;
    let tests: TestFile = toml::from_str(&content)?;

    let (indexed, _) = db.embedding_stats()?;
    if indexed == 0 {
        eprintln!("\x1b[31m✗\x1b[0m No projects indexed. Run \x1b[1mgoto update\x1b[0m first.");
        std::process::exit(1);
    }

    let mut passed = 0;
    let mut failed = 0;

    for test in &tests.tests {
        // Run semantic search with name-based boost
        let results = semantic::semantic_search(db, &test.query, 20)?;
        let query_lower = test.query.to_lowercase();

        let mut boosted: Vec<_> = results
            .into_iter()
            .map(|(project, score)| {
                let embedded_text = db.get_embedded_text(&project.path).ok().flatten();
                let boosted_score = calculate_boosted_score(
                    &project.name,
                    &query_lower,
                    score,
                    embedded_text.as_deref(),
                );
                (project, boosted_score)
            })
            .collect();

        boosted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top_names: Vec<_> = boosted.iter().take(test.top_n).map(|(p, _)| &p.name).collect();

        // Check if any expected result is in top N (exact match)
        let mut found: Vec<&str> = vec![];
        let mut missing: Vec<&str> = vec![];

        for exp in &test.expected {
            if top_names.iter().any(|n| *n == exp) {
                found.push(exp);
            } else {
                missing.push(exp);
            }
        }

        if missing.is_empty() {
            passed += 1;
            eprintln!(
                "\x1b[32m✓\x1b[0m \"{}\" → {} \x1b[90m(found: {})\x1b[0m",
                test.query,
                top_names.first().map(|s| s.as_str()).unwrap_or("?"),
                found.join(", ")
            );
        } else {
            failed += 1;
            eprintln!(
                "\x1b[31m✗\x1b[0m \"{}\" → {} \x1b[90m(missing: {})\x1b[0m",
                test.query,
                top_names.first().map(|s| s.as_str()).unwrap_or("?"),
                missing.join(", ")
            );
            // Show actual top results
            for (i, (p, score)) in boosted.iter().take(test.top_n).enumerate() {
                eprintln!("    {}. {} ({:.0}%)", i + 1, p.name, score);
            }
        }
    }

    eprintln!();
    if failed == 0 {
        eprintln!("\x1b[32m✓ All {} tests passed\x1b[0m", passed);
    } else {
        eprintln!("\x1b[31m✗ {}/{} tests failed\x1b[0m", failed, passed + failed);
        std::process::exit(1);
    }

    Ok(())
}

/// Scan and index all projects
fn update_all(force: bool, config: &Config, db: &mut Database) -> Result<()> {
    // Step 1: Scan for projects
    eprintln!("\x1b[36m⏳\x1b[0m Scanning for projects...");
    let mut scanner = Scanner::new(config, db);
    let result = scanner.scan_all()?;

    eprintln!(
        "\x1b[32m✓\x1b[0m Found \x1b[1m{}\x1b[0m projects ({} from paths, {} from Spotlight)",
        result.total(),
        result.from_paths,
        result.from_spotlight
    );

    if result.pruned > 0 {
        eprintln!("\x1b[33m⚠\x1b[0m Removed {} stale entries", result.pruned);
    }

    // Step 2: Index for semantic search
    if force {
        eprintln!("\x1b[36m⏳\x1b[0m Clearing existing embeddings...");
        db.clear_embeddings()?;
    }

    let count = semantic::index_projects(db)?;

    if count > 0 {
        eprintln!("\x1b[32m✓\x1b[0m Indexed \x1b[1m{}\x1b[0m projects for semantic search", count);
    } else {
        eprintln!("\x1b[32m✓\x1b[0m All projects already indexed");
    }

    Ok(())
}

fn list_projects(sort: SortOrder, limit: usize, show_git: bool, db: &Database) -> Result<()> {
    let mut projects = db.get_all_projects()?;

    match sort {
        SortOrder::Recent => {
            projects.sort_by(|a, b| b.last_accessed.cmp(&a.last_accessed));
        }
        SortOrder::Frecency => {
            projects.sort_by(|a, b| {
                b.frecency_score()
                    .partial_cmp(&a.frecency_score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        SortOrder::Name => {
            projects.sort_by(|a, b| a.name.cmp(&b.name));
        }
    }

    if projects.is_empty() {
        eprintln!("\x1b[31m✗\x1b[0m No projects indexed yet.");
        eprintln!("  Run \x1b[1mgoto scan\x1b[0m to discover projects.");
        return Ok(());
    }

    let total = projects.len();
    eprintln!("\x1b[36mProjects\x1b[0m (showing {}/{}):\n", std::cmp::min(limit, total), total);

    for project in projects.iter().take(limit) {
        let git_info = if show_git {
            get_git_status(&project.path)
                .map(|(branch, dirty)| {
                    let dirty_marker = if dirty { "\x1b[31m*\x1b[0m" } else { "" };
                    format!(" \x1b[33m{}\x1b[0m{}", branch, dirty_marker)
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

        println!(
            "  \x1b[1m{}\x1b[0m{} \x1b[90m{}\x1b[0m",
            project.name,
            git_info,
            project.path.display()
        );
    }

    Ok(())
}

fn add_path(path: std::path::PathBuf, config: &mut Config) -> Result<()> {
    let canonical = path.canonicalize()?;
    config.add_path(canonical.clone())?;
    eprintln!("\x1b[32m✓\x1b[0m Added \x1b[1m{}\x1b[0m to scan paths", canonical.display());

    // Scan the path immediately
    let mut db = Database::open()?;
    let mut scanner = Scanner::new(config, &mut db);
    eprintln!("\x1b[36m⏳\x1b[0m Scanning...");
    let result = scanner.scan_paths_only()?;
    eprintln!("\x1b[32m✓\x1b[0m Found \x1b[1m{}\x1b[0m projects", result.from_paths);

    Ok(())
}

fn remove_path(path: std::path::PathBuf, config: &mut Config) -> Result<()> {
    if config.remove_path(&path)? {
        eprintln!("\x1b[32m✓\x1b[0m Removed \x1b[1m{}\x1b[0m from scan paths", path.display());
    } else {
        eprintln!("\x1b[33m⚠\x1b[0m Path \x1b[1m{}\x1b[0m was not in the scan list", path.display());
    }
    Ok(())
}

fn show_config(config: &Config) -> Result<()> {
    eprintln!("\x1b[36mConfiguration\x1b[0m\n");
    eprintln!("  \x1b[90mConfig file:\x1b[0m {}", Config::config_path()?.display());
    eprintln!("  \x1b[90mDatabase:\x1b[0m    {}", Config::db_path()?.display());
    eprintln!();

    let spotlight_status = if config.use_spotlight { "\x1b[32m✓\x1b[0m" } else { "\x1b[31m✗\x1b[0m" };
    eprintln!("  {} \x1b[1mSpotlight:\x1b[0m {}", spotlight_status, config.use_spotlight);
    eprintln!("  \x1b[90mSpotlight paths:\x1b[0m");
    for path in &config.spotlight_paths {
        eprintln!("    \x1b[90m•\x1b[0m {}", path.display());
    }
    eprintln!();
    eprintln!("  \x1b[1mScan paths:\x1b[0m");
    if config.scan_paths.is_empty() {
        eprintln!("    \x1b[90m(none - use 'goto add <path>' to add paths)\x1b[0m");
    } else {
        for path in &config.scan_paths {
            eprintln!("    \x1b[90m•\x1b[0m {}", path.display());
        }
    }
    eprintln!();
    eprintln!("  \x1b[90mMax depth:\x1b[0m    {}", config.max_depth);
    eprintln!("  \x1b[90mPost command:\x1b[0m {}", config.post_command.as_deref().unwrap_or("\x1b[90m(none)\x1b[0m"));

    Ok(())
}


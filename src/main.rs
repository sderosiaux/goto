mod cli;
mod config;
mod db;
mod matcher;
mod scanner;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Commands, SortOrder};
use config::Config;
use db::{Database, ProjectSource};
use matcher::Matcher;
use scanner::Scanner;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load()?;
    let mut db = Database::open()?;

    // If a query is provided without a subcommand, treat it as "find"
    if let Some(query) = cli.query {
        return find_project(&query, false, &config, &db);
    }

    match cli.command {
        Some(Commands::Find { query, all }) => {
            find_project(&query, all, &config, &db)
        }
        Some(Commands::Scan { spotlight_only, paths_only }) => {
            scan_projects(spotlight_only, paths_only, &config, &mut db)
        }
        Some(Commands::List { sort, limit }) => {
            list_projects(sort, limit, &db)
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
        Some(Commands::Refresh) => {
            refresh(&config, &mut db)
        }
        None => {
            // No command and no query - show help hint
            eprintln!("\x1b[33mUsage:\x1b[0m goto <query> or goto --help for more options");
            std::process::exit(1);
        }
    }
}

fn find_project(query: &str, show_all: bool, config: &Config, db: &Database) -> Result<()> {
    let projects = db.get_all_projects()?;

    if projects.is_empty() {
        eprintln!("\x1b[31m✗\x1b[0m No projects indexed yet.");
        eprintln!("  Run \x1b[1mgoto scan\x1b[0m to discover projects.");
        std::process::exit(1);
    }

    let matcher = Matcher::new();
    let matches = matcher.find_matches(query, &projects);

    if matches.is_empty() {
        eprintln!("\x1b[31m✗\x1b[0m No projects matching '\x1b[1m{query}\x1b[0m'");
        eprintln!("  Try a different query or run \x1b[1mgoto list\x1b[0m to see all projects.");
        std::process::exit(1);
    }

    if show_all {
        // Show all matches for the user to choose
        eprintln!("\x1b[36mMatches for '\x1b[1m{query}\x1b[0m\x1b[36m':\x1b[0m");
        for (i, m) in matches.iter().take(10).enumerate() {
            let score_color = if m.fuzzy_score > 80 { "32" } else { "90" };
            eprintln!(
                "  \x1b[33m{:>2}.\x1b[0m \x1b[1m{}\x1b[0m \x1b[{}m({})\x1b[0m",
                i + 1,
                m.project.path.display(),
                score_color,
                m.fuzzy_score
            );
        }
    } else {
        // Output the best match path - this will be captured by the shell function
        let best = &matches[0];

        // Mark as accessed
        db.mark_accessed(&best.project.path)?;

        // Output path for the shell function to cd to
        println!("{}", best.project.path.display());

        // Output post command if configured (on stderr so it doesn't interfere with path)
        if let Some(cmd) = &config.post_command {
            eprintln!("__GOTO_POST_CMD__:{}", cmd);
        }
    }

    Ok(())
}

fn scan_projects(spotlight_only: bool, paths_only: bool, config: &Config, db: &mut Database) -> Result<()> {
    let mut scanner = Scanner::new(config, db);

    let result = if spotlight_only {
        eprintln!("\x1b[36m⏳\x1b[0m Scanning via Spotlight...");
        scanner.scan_spotlight_only()?
    } else if paths_only {
        eprintln!("\x1b[36m⏳\x1b[0m Scanning configured paths...");
        scanner.scan_paths_only()?
    } else {
        eprintln!("\x1b[36m⏳\x1b[0m Scanning all sources...");
        scanner.scan_all()?
    };

    eprintln!(
        "\x1b[32m✓\x1b[0m Found \x1b[1m{}\x1b[0m projects ({} from paths, {} from Spotlight)",
        result.total(),
        result.from_paths,
        result.from_spotlight
    );

    if result.pruned > 0 {
        eprintln!("\x1b[33m⚠\x1b[0m Removed {} stale entries", result.pruned);
    }

    Ok(())
}

fn list_projects(sort: SortOrder, limit: usize, db: &Database) -> Result<()> {
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
        let (source_badge, source_color) = match project.source {
            ProjectSource::Spotlight => ("S", "35"),  // magenta
            ProjectSource::Manual => ("M", "33"),     // yellow
            ProjectSource::Scan => ("P", "34"),       // blue
        };

        let frecency = project.frecency_score();
        let frecency_color = if frecency > 50.0 { "32" } else { "90" };

        println!(
            "  \x1b[{}m[{}]\x1b[0m \x1b[{}m{:>5.0}\x1b[0m \x1b[1m{}\x1b[0m \x1b[90m{}\x1b[0m",
            source_color,
            source_badge,
            frecency_color,
            frecency,
            project.name,
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

fn refresh(config: &Config, db: &mut Database) -> Result<()> {
    eprintln!("\x1b[36m⏳\x1b[0m Clearing cache...");
    db.clear()?;
    scan_projects(false, false, config, db)
}

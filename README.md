# goto

Fast project navigation with fuzzy matching. Jump to any project directory instantly.

## Why?

As developers, we accumulate dozens (or hundreds) of project directories scattered across our filesystem. Finding and navigating to them is tedious:

```bash
# The old way
cd ~/code/work/team/some-project-i-forgot-the-exact-name
# Wait, was it in ~/projects? Or ~/dev?
```

`goto` solves this by indexing all your git projects and letting you jump to them with fuzzy matching:

```bash
goto docs      # → /Users/you/code/company/documentation
goto api       # → /Users/you/projects/backend-api
goto react     # → /Users/you/dev/react-dashboard
```

## Features

- **Fuzzy matching** - Type partial names, it finds the best match
- **Instant indexing** - Uses macOS Spotlight + directory scanning
- **Recency-aware** - When scores tie, recently accessed projects rank higher
- **Git integration** - Show branch and dirty status with `--git` flag
- **Recent list** - `goto -` shows your last accessed projects
- **Statistics** - `goto stats` shows access patterns and most used projects
- **Auto-launch** - Optionally run a command after navigation (e.g., open your editor)
- **Exclude patterns** - Automatically skips `node_modules`, `vendor`, `.cache`, etc.
- **Fast** - Written in Rust, SQLite-backed, sub-millisecond lookups

## Installation

```bash
# Clone and install
git clone https://github.com/YOUR_USERNAME/goto.git
cd goto
./install.sh
```

This will:
1. Build the Rust binary
2. Install it to `~/.local/bin/`
3. Add the shell function to your `.zshrc`

Then restart your terminal or run `source ~/.zshrc`.

## Usage

### Index your projects

```bash
# Add directories to scan (recursive)
goto add ~/code
goto add ~/projects

# Or just scan with Spotlight (finds all git repos)
goto scan
```

### Navigate

```bash
# Jump to a project
goto myproject

# See all matches
goto find myproject --all

# List indexed projects
goto list
goto list --sort name
goto list --sort recent
```

### Configuration

```bash
# Show current config
goto config
```

Config file: `~/Library/Application Support/dev.goto.goto/config.toml`

```toml
# Directories to scan recursively
scan_paths = ["/Users/you/code", "/Users/you/projects"]

# Use macOS Spotlight for discovery
use_spotlight = true

# Max recursion depth when scanning
max_depth = 5

# Command to run after navigation (optional)
post_command = "code"  # or "claude", "cursor", "vim", etc.
```

## How it works

1. **Indexing**: Scans directories for `.git` folders and/or uses Spotlight to find project markers (`package.json`, `Cargo.toml`, `pyproject.toml`, etc.)

2. **Storage**: Projects are cached in a local SQLite database with access timestamps

3. **Matching**: Uses the [Skim](https://github.com/lotabout/fuzzy-matcher) fuzzy matching algorithm, same as fzf

4. **Ranking**: Primary sort by fuzzy score, secondary by recency (tiebreaker)

5. **Navigation**: Shell function wraps the binary to enable `cd` (subprocesses can't change the parent shell's directory)

## Commands

| Command | Description |
|---------|-------------|
| `goto <query>` | Jump to best matching project |
| `goto find <query> --all` | Show all matches |
| `goto scan` | Re-scan and index projects |
| `goto list` | List all indexed projects |
| `goto add <path>` | Add a directory to scan |
| `goto remove <path>` | Remove a directory from scan |
| `goto config` | Show configuration |
| `goto refresh` | Clear cache and re-scan |

## Requirements

- macOS (uses Spotlight for discovery)
- Rust toolchain (for building)
- zsh (bash support coming)

## License

MIT

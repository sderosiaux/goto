# goto

Fast project navigation CLI with semantic search. Index all your projects, jump to any of them instantly.

## Attitude

**Stable CLI, move fast internally.** The CLI interface and behavior matter for muscle memory (used by a community). Internal refactors are fine, but don't break commands, flags, or output formats without good reason.

## Project Structure

```
goto/
├── src/
│   ├── main.rs        # Entry point, CLI dispatch, scoring/boosting
│   ├── cli.rs         # clap argument definitions
│   ├── config.rs      # TOML config management
│   ├── db.rs          # SQLite + sqlite-vec operations
│   ├── scanner.rs     # Directory scanning, Spotlight integration
│   ├── semantic.rs    # Metadata extraction, embedding text
│   └── embedding.rs   # fastembed model wrapper
├── goto.zsh           # Shell wrapper (handles cd + post-command)
├── install.sh         # Build and install script
├── Cargo.toml
└── README.md
```

## Build & Run

```bash
# Build (debug)
cargo build

# Build (release)
cargo build --release

# Install locally
./install.sh

# Run directly
cargo run -- <args>

# Run tests
cargo test
```

## Architecture

### Data Flow

```
User Query
    │
    ▼
┌─────────────────┐
│  Exact Match?   │──yes──▶ Return immediately
└────────┬────────┘
         │ no
         ▼
┌─────────────────┐     ┌──────────────────┐
│ Embed Query     │────▶│ sqlite-vec L2    │
│ (384-dim)       │     │ distance search  │
└─────────────────┘     └────────┬─────────┘
                                 │
                                 ▼
                        ┌──────────────────┐
                        │ Apply Boosting   │
                        │ +40 exact name   │
                        │ +20 substring    │
                        │ +10 metadata     │
                        └────────┬─────────┘
                                 │
                                 ▼
                        Best match (if score >= 55%)
```

### Key Components

| Component | Purpose |
|-----------|---------|
| `scanner.rs` | Finds projects via directory walk + macOS Spotlight |
| `semantic.rs` | Extracts metadata (README, package files, types) |
| `embedding.rs` | Wraps fastembed for text → vector |
| `db.rs` | SQLite with sqlite-vec for vector similarity |
| `main.rs` | Scoring, boosting, CLI commands |

### Embedding Model

- **Model**: MultilingualE5Small (384 dimensions)
- **Size**: ~80MB (downloaded on first `goto update`)
- **Cache**: `~/Library/Caches/dev.goto.goto/`

### Database

- **Location**: `~/Library/Application Support/dev.goto.goto/cache.db`
- **Tables**: `projects`, `project_metadata`, `project_embeddings` (vec0 virtual table)

### Config

- **Location**: `~/Library/Application Support/dev.goto.goto/config.toml`
- **Key settings**: `scan_paths`, `use_spotlight`, `post_command`, `exclude_patterns`

## Key Files

| Purpose | Location |
|---------|----------|
| CLI args | `src/cli.rs` |
| Scoring constants | `src/main.rs:191-200` |
| Tech stack detection | `src/semantic.rs:385-514` |
| Vector search | `src/db.rs:315-329` |
| Shell integration | `goto.zsh` |

## Patterns

**Follow:**
- Batch database operations (see `upsert_projects_batch`)
- Output path to stdout, info to stderr (shell wrapper depends on this)
- Use `eprintln!` with ANSI colors for user feedback
- Constants at module top for tunable values

**Avoid:**
- Breaking CLI interface or output format
- Blocking operations during search (model is pre-loaded)
- N+1 database queries (batch instead)

## Commands

```bash
goto <query>           # Jump to best match
goto -a <query>        # Show all matches with scores
goto -                 # Show recent projects
goto update            # Scan and index projects
goto update --force    # Re-index all (clear embeddings first)
goto add <path>        # Add path to scan list
goto remove <path>     # Remove path from scan list
goto list              # List all indexed projects
goto stats             # Show access statistics
goto config            # Show current configuration
goto test              # Run ranking tests from ~/.config/goto/tests.toml
```

## Testing

```bash
# Unit tests
cargo test

# Ranking tests (requires indexed projects)
goto test
# Edit tests: ~/.config/goto/tests.toml
```

## Constraints

- NEVER change CLI command names or flag behavior without discussion
- NEVER change stdout output format (shell wrapper parses it)
- NEVER commit model files or database to git
- First `goto update` downloads ~80MB model - this is expected
- macOS only (uses Spotlight via `mdfind`)

## Gotchas

- **First run is slow**: Downloads embedding model (~80MB)
- **Spotlight must be enabled** for full project discovery
- **Shell wrapper required**: The `goto` function in `goto.zsh` handles `cd` - the binary only outputs the path
- **Post-command whitelist**: `goto.zsh` only runs whitelisted commands (claude, code, cursor, vim, nvim, emacs, hx, zed) for security
- **Score threshold**: Matches below 55% similarity are rejected (constant `SEMANTIC_MIN_THRESHOLD`)

## Common Tasks

### Add new CLI command
1. Add variant to `Commands` enum in `src/cli.rs`
2. Add match arm in `main.rs` `match cli.command`
3. Implement handler function in `main.rs`

### Tune search ranking
1. Adjust constants in `src/main.rs:191-200`
2. Add test cases to `~/.config/goto/tests.toml`
3. Run `goto test` to verify

### Add new tech stack detection
1. Add marker to `markers` array in `src/semantic.rs:389-454`
2. Or add extension mapping to `ext_map` in `src/semantic.rs:463-493`

### Add new metadata source
1. Create `read_*_description()` function in `src/semantic.rs`
2. Call it in `extract_metadata()` function
3. Data flows into `to_embedding_text()` automatically

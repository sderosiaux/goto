# goto

Fast project navigation with semantic search. Jump to any project instantly.

> macOS only. Requires Spotlight for full project discovery.

## Why?

We all have dozens of project directories scattered across our filesystem:

```bash
# The old way
cd ~/code/work/team/some-project-i-forgot-the-exact-name
# Wait, was it in ~/projects? Or ~/dev?
```

`goto` indexes all your projects and lets you jump to them instantly:

```bash
goto docs          # → /Users/you/code/documentation
goto api           # → /Users/you/projects/backend-api
goto "cache rust"  # → /Users/you/code/foyer (semantic match!)
```

## What it does

- **Hybrid search** — BM25 keyword search + vector similarity, merged via Reciprocal Rank Fusion
- **Smart ranking** — Projects with matching names get boosted to the top
- **Recent list** — `goto -` shows your last accessed projects
- **Auto-indexing** — Background task keeps the index fresh every 5 minutes
- **Fast** — Exact name matches are instant; search uses a local ML model (no network)

## Installation

**One-liner** (no Rust required):

```bash
curl -fsSL https://raw.githubusercontent.com/sderosiaux/goto/main/install.sh | bash
```

**Via cargo:**

```bash
cargo install goto-cli
```

Then restart your terminal.

## Quick start

```bash
# Tell goto where your projects live
goto add ~/code
goto add ~/projects

# Index everything (downloads ~80MB model on first run)
goto update

# Jump to a project
goto myproject
```

## Usage

```bash
goto <query>           # Jump to best match
goto -a <query>        # Show all matches with scores
goto -a -n 30 <query>  # Show more matches
goto -                 # Show recently visited projects
goto update            # Re-scan and re-index
goto update --force    # Re-embed everything (slower)
goto add ~/code        # Add a directory to scan
goto remove ~/code     # Remove a directory
goto list              # List all indexed projects
goto stats             # Show access statistics
goto config            # Show current configuration
goto test              # Run ranking tests
```

## How it works

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              INDEXING PHASE                                 │
└─────────────────────────────────────────────────────────────────────────────┘

  ~/code/myproject/
         │
         ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                          METADATA EXTRACTION                                │
│                                                                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐    │
│  │ package.json │  │  README.md   │  │ Cargo.toml   │  │  src/*.rs    │    │
│  │ Cargo.toml   │  │  (excerpt)   │  │ package.json │  │  src/*.ts    │    │
│  │ pyproject    │  │              │  │ (tech stack) │  │  (types)     │    │
│  │ (description)│  │              │  │              │  │              │    │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘    │
│         │                 │                 │                 │            │
│         └─────────────────┴─────────────────┴─────────────────┘            │
│                                     │                                       │
│                                     ▼                                       │
│            "myproject | Fast cache library | Rust, async |                  │
│             Technologies: Rust | Type: backend | Structure: cache"          │
└─────────────────────────────────────────────────────────────────────────────┘
         │                                        │
         ▼                                        ▼
┌─────────────────┐                   ┌───────────────────────┐
│  FTS5 index     │                   │  MultilingualE5Small  │
│  (BM25 keyword) │                   │  384-dim embeddings   │
└─────────────────┘                   └───────────────────────┘


┌─────────────────────────────────────────────────────────────────────────────┐
│                               SEARCH PHASE                                  │
└─────────────────────────────────────────────────────────────────────────────┘

     "cache rust"
          │
          ├──────────────────────────────────────────┐
          ▼                                          ▼
  ┌──────────────────┐                    ┌────────────────────┐
  │  Embed query     │                    │  BM25 keyword      │
  │  Vector search   │                    │  FTS5 search       │
  │  (sqlite-vec)    │                    │                    │
  └────────┬─────────┘                    └─────────┬──────────┘
           │                                        │
           └──────────────────┬─────────────────────┘
                              ▼
                   ┌──────────────────────┐
                   │  Reciprocal Rank     │
                   │  Fusion (RRF)        │
                   │  merged scores       │
                   └──────────┬───────────┘
                              │
                              ▼
                   ┌──────────────────────┐
                   │  Apply Boosting      │
                   │  +40 exact name      │
                   │  +20 name match      │
                   │  +10 metadata match  │
                   └──────────┬───────────┘
                              │
                              ▼
                   ┌──────────────────────┐
                   │  1. foyer      (92)  │
                   │  2. redis-cli  (78)  │
                   │  3. cache-lib  (71)  │
                   └──────────────────────┘
```

### Metadata sources

| Source | Data extracted |
|--------|----------------|
| `package.json` / `Cargo.toml` / `pyproject.toml` | Description, keywords |
| `README.md` | First meaningful paragraph (up to 1500 chars) |
| Build files | Tech stack detection (40+ frameworks/languages) |
| Directory structure | Semantic folder names |
| Source files (top 10 by size) | Type/class/interface names |

### Boosting rules

- **+40** — Project name exactly matches query
- **+20** — All query words found in project name
- **+10** — Query words found in indexed metadata

## Requirements

- macOS (uses Spotlight via `mdfind` for project discovery)
- First `goto update` downloads ~80MB embedding model to `~/Library/Caches/dev.goto.goto/`

## License

MIT

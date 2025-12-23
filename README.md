# goto

Fast project navigation with semantic search. Jump to any project instantly.

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

- **Semantic search** - Find projects by concept, not just folder name (powered by local embeddings)
- **Smart ranking** - Projects with matching names get boosted to the top
- **Recent list** - `goto -` shows your last accessed projects
- **Fast** - Exact name matches are instant, semantic search uses local ML model

## Installation

```bash
git clone https://github.com/sderosiaux/goto.git
cd goto && ./install.sh
```

Then restart your terminal.

## Usage

```bash
# Add directories to scan
goto add ~/code
goto add ~/projects

# Scan and index projects (downloads ~80MB model on first run)
goto update

# Jump to projects
goto myproject

# See all matches
goto -a myproject

# See more matches
goto -a -n 30 myproject

# Show recent
goto -

# Run ranking tests
goto test
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
│         ▼                 ▼                 ▼                 ▼            │
│  ┌──────────────────────────────────────────────────────────────────────┐  │
│  │         Combined Text: "myproject | Fast cache library |             │  │
│  │         Rust, async | Technologies: Rust | Type: backend |          │  │
│  │         Structure: cache, storage | Types: CacheManager, LruCache"  │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────┘
                                      │
                                      ▼
                    ┌─────────────────────────────────┐
                    │   MultilingualE5Small Model     │
                    │       (384-dim vectors)         │
                    └─────────────────────────────────┘
                                      │
                                      ▼
                    ┌─────────────────────────────────┐
                    │   SQLite + sqlite-vec           │
                    │   (vector storage & search)     │
                    └─────────────────────────────────┘


┌─────────────────────────────────────────────────────────────────────────────┐
│                               SEARCH PHASE                                  │
└─────────────────────────────────────────────────────────────────────────────┘

     "cache rust"
          │
          ▼
   ┌──────────────┐      ┌─────────────────────┐      ┌─────────────────────┐
   │  Embed Query │  ──▶ │ L2 Distance Search  │  ──▶ │   Apply Boosting    │
   │   (384-dim)  │      │   (sqlite-vec)      │      │                     │
   └──────────────┘      └─────────────────────┘      │  +20 name match     │
                                                      │  +10 metadata match │
                                                      └─────────────────────┘
                                                                 │
                                                                 ▼
                                                      ┌─────────────────────┐
                                                      │   Ranked Results    │
                                                      │                     │
                                                      │  1. foyer      (92) │
                                                      │  2. redis-cli  (78) │
                                                      │  3. cache-lib  (71) │
                                                      └─────────────────────┘
```

### Metadata Sources

| Source | Data Extracted |
|--------|----------------|
| `package.json` / `Cargo.toml` / `pyproject.toml` | Description, keywords |
| `README.md` | First meaningful paragraph (up to 1500 chars) |
| Build files | Tech stack detection (40+ frameworks/languages) |
| Directory structure | Semantic folder names (filtered) |
| Source files (top 10 by size) | Type/class/interface names |

### Boosting Rules

- **+20 points**: All query words found in project name
- **+10 points**: Query words found in embedded metadata text

## License

MIT

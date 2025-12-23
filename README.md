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

1. **Indexing**: Extracts metadata from each project (description, README excerpt, tech stack, keywords)
2. **Embedding**: Uses `MultilingualE5Small` model (384-dim vectors) to embed project descriptions
3. **Search**: Query is embedded and compared via cosine similarity
4. **Boosting**: Projects with names containing the query get +20% score boost

## License

MIT

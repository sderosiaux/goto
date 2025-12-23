# goto

Fast project navigation with fuzzy + semantic search. Jump to any project instantly.

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
goto "hotel app"   # → /Users/you/work/reservo (semantic match!)
```

## What it does

- **Fuzzy matching** - Type partial names, it finds the best match
- **Semantic search** - Find projects by concept, not just folder name
- **Recent list** - `goto -` shows your last accessed projects
- **Fast** - Sub-millisecond lookups

## Installation

```bash
git clone https://github.com/YOUR_USERNAME/goto.git
cd goto && ./install.sh
```

Then restart your terminal.

## Usage

```bash
# Add directories to scan
goto add ~/code
goto add ~/projects

# Scan and index projects
goto update

# Jump to projects
goto myproject

# See all matches
goto myproject -a

# Show recent
goto -
```

## License

MIT

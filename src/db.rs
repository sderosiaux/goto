use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{ffi::sqlite3_auto_extension, params, Connection, OptionalExtension, Transaction};
use sqlite_vec::sqlite3_vec_init;
use std::path::PathBuf;
use zerocopy::AsBytes;

use crate::config::Config;
use crate::embedding::EMBEDDING_DIM;

#[derive(Debug, Clone)]
pub struct Project {
    pub path: PathBuf,
    pub name: String,
    pub last_accessed: DateTime<Utc>,
    pub access_count: i64,
    #[allow(dead_code)]
    pub source: ProjectSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProjectSource {
    Spotlight,
    Manual,
    Scan,
}

impl Project {
    /// Calculate frecency score (frequency + recency)
    /// Higher score = more relevant
    pub fn frecency_score(&self) -> f64 {
        let now = Utc::now();
        let hours_since_access = (now - self.last_accessed).num_hours() as f64;

        // Decay factor: halve the score every 72 hours of inactivity
        let recency_factor = 0.5_f64.powf(hours_since_access / 72.0);

        // Frequency factor: log scale to prevent heavy users from dominating
        let frequency_factor = (self.access_count as f64 + 1.0).ln();

        recency_factor * frequency_factor * 100.0
    }
}

impl std::fmt::Display for ProjectSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectSource::Spotlight => write!(f, "spotlight"),
            ProjectSource::Manual => write!(f, "manual"),
            ProjectSource::Scan => write!(f, "scan"),
        }
    }
}

impl std::str::FromStr for ProjectSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "spotlight" => Ok(ProjectSource::Spotlight),
            "manual" => Ok(ProjectSource::Manual),
            "scan" => Ok(ProjectSource::Scan),
            _ => Err(format!("Unknown source: {s}")),
        }
    }
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open() -> Result<Self> {
        // Initialize sqlite-vec extension (must be done before opening connection)
        unsafe {
            sqlite3_auto_extension(Some(std::mem::transmute::<*const (), unsafe extern "C" fn(*mut rusqlite::ffi::sqlite3, *mut *mut i8, *const rusqlite::ffi::sqlite3_api_routines) -> i32>(sqlite3_vec_init as *const ())));
        }

        let db_path = Config::db_path()?;

        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create data directory: {}", parent.display()))?;
        }

        let conn = Connection::open(&db_path)
            .with_context(|| format!("Failed to open database: {}", db_path.display()))?;

        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            -- Performance optimizations
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA temp_store = MEMORY;
            PRAGMA cache_size = -2000;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS projects (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT UNIQUE NOT NULL,
                name TEXT NOT NULL,
                last_accessed TEXT NOT NULL,
                access_count INTEGER DEFAULT 0,
                last_modified TEXT NOT NULL,
                source TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_projects_name ON projects(name);
            CREATE INDEX IF NOT EXISTS idx_projects_path ON projects(path);
            CREATE INDEX IF NOT EXISTS idx_projects_last_accessed ON projects(last_accessed DESC);
            CREATE INDEX IF NOT EXISTS idx_projects_frecency ON projects(access_count DESC, last_accessed DESC);

            -- Semantic metadata for projects
            CREATE TABLE IF NOT EXISTS project_metadata (
                project_id INTEGER PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,
                description TEXT,
                readme_excerpt TEXT,
                embedded_text TEXT,
                last_indexed TEXT
            );
            "
        )?;

        // vec0 virtual table (vector similarity search)
        self.conn.execute(
            &format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS project_embeddings USING vec0(
                    project_id INTEGER PRIMARY KEY,
                    embedding FLOAT[{}]
                )",
                EMBEDDING_DIM
            ),
            [],
        )?;

        // FTS5 virtual table (keyword search for hybrid retrieval)
        self.conn.execute(
            "CREATE VIRTUAL TABLE IF NOT EXISTS project_fts USING fts5(name, embedded_text)",
            [],
        )?;

        Ok(())
    }

    /// Batch insert/update projects in a single transaction
    pub fn upsert_projects_batch(&mut self, paths: &[PathBuf], source: ProjectSource) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let count = Self::upsert_in_transaction(&tx, paths, source)?;
        tx.commit()?;
        Ok(count)
    }

    fn upsert_in_transaction(tx: &Transaction, paths: &[PathBuf], source: ProjectSource) -> Result<usize> {
        let mut stmt = tx.prepare(
            "INSERT INTO projects (path, name, last_accessed, access_count, last_modified, source)
             VALUES (?1, ?2, ?3, 0, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                 last_modified = ?4,
                 source = CASE WHEN source = 'manual' THEN 'manual' ELSE ?5 END"
        )?;

        let now = Utc::now().to_rfc3339();
        let source_str = source.to_string();
        let mut count = 0;

        for path in paths {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string_lossy().to_string());

            let last_modified = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .map(DateTime::<Utc>::from)
                .unwrap_or_else(|_| Utc::now())
                .to_rfc3339();

            stmt.execute(params![
                path.to_string_lossy().as_ref(),
                name,
                &now,
                &last_modified,
                &source_str,
            ])?;
            count += 1;
        }

        Ok(count)
    }

    /// Mark a project as accessed (increment count and update timestamp)
    pub fn mark_accessed(&self, path: &std::path::Path) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE projects SET last_accessed = ?1, access_count = access_count + 1 WHERE path = ?2",
            params![now, path.to_string_lossy().as_ref()],
        )?;
        Ok(())
    }

    /// Get a project by exact name match (case-insensitive) — avoids loading all projects
    pub fn get_project_by_name(&self, name: &str) -> Result<Option<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, name, last_accessed, access_count, source
             FROM projects WHERE LOWER(name) = LOWER(?1) LIMIT 1",
        )?;
        let result = stmt
            .query_row([name], |row| {
                Ok(Project {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                    name: row.get(1)?,
                    last_accessed: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    access_count: row.get(3)?,
                    source: row.get::<_, String>(4)?.parse().unwrap_or(ProjectSource::Scan),
                })
            })
            .optional()?;
        Ok(result)
    }

    /// Get recently accessed projects — avoids loading all projects into memory
    pub fn get_recent_projects(&self, limit: usize) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, name, last_accessed, access_count, source
             FROM projects
             WHERE access_count > 0
             ORDER BY last_accessed DESC
             LIMIT ?",
        )?;
        let projects = stmt.query_map([limit as i64], |row| {
            Ok(Project {
                path: PathBuf::from(row.get::<_, String>(0)?),
                name: row.get(1)?,
                last_accessed: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                access_count: row.get(3)?,
                source: row.get::<_, String>(4)?.parse().unwrap_or(ProjectSource::Scan),
            })
        })?;
        projects.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get all projects
    pub fn get_all_projects(&self) -> Result<Vec<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, name, last_accessed, access_count, source FROM projects"
        )?;

        let projects = stmt.query_map([], |row| {
            Ok(Project {
                path: PathBuf::from(row.get::<_, String>(0)?),
                name: row.get(1)?,
                last_accessed: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                access_count: row.get(3)?,
                source: row.get::<_, String>(4)?
                    .parse()
                    .unwrap_or(ProjectSource::Scan),
            })
        })?;

        projects.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Remove projects that no longer exist on disk - BATCH DELETE (fixed N+1)
    pub fn prune_missing(&mut self) -> Result<usize> {
        // Get only IDs and paths (lighter than full Project)
        let mut stmt = self.conn.prepare("SELECT id, path FROM projects")?;
        let mut rows = stmt.query([])?;
        let mut entries: Vec<(i64, String)> = Vec::new();
        while let Some(row) = rows.next()? {
            entries.push((row.get(0)?, row.get(1)?));
        }
        drop(rows);
        drop(stmt);

        // Collect IDs of missing projects
        let missing_ids: Vec<i64> = entries
            .into_iter()
            .filter(|(_, path_str)| !PathBuf::from(path_str).exists())
            .map(|(id, _)| id)
            .collect();

        if missing_ids.is_empty() {
            return Ok(0);
        }

        // Single DELETE with IN clause (CASCADE handles project_metadata + project_embeddings)
        let placeholders = missing_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let fts_sql = format!("DELETE FROM project_fts WHERE rowid IN ({})", placeholders);
        self.conn.execute(&fts_sql, rusqlite::params_from_iter(missing_ids.iter()))?;
        let sql = format!("DELETE FROM projects WHERE id IN ({})", placeholders);
        self.conn.execute(&sql, rusqlite::params_from_iter(missing_ids.iter()))?;

        Ok(missing_ids.len())
    }

    // ========== Semantic Search Methods ==========

    /// Store or update project metadata
    pub fn upsert_metadata(
        &self,
        project_id: i64,
        description: Option<&str>,
        readme_excerpt: Option<&str>,
        embedded_text: &str,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT INTO project_metadata (project_id, description, readme_excerpt, embedded_text, last_indexed)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(project_id) DO UPDATE SET
                 description = ?2,
                 readme_excerpt = ?3,
                 embedded_text = ?4,
                 last_indexed = ?5",
            params![project_id, description, readme_excerpt, embedded_text, now],
        )?;
        Ok(())
    }

    /// Batch fetch embedded_text for multiple projects (replaces N+1 single-query calls)
    pub fn get_embedded_texts_batch(&self, paths: &[PathBuf]) -> Result<std::collections::HashMap<PathBuf, String>> {
        if paths.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let placeholders = paths.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(
            "SELECT p.path, pm.embedded_text
             FROM project_metadata pm
             JOIN projects p ON pm.project_id = p.id
             WHERE p.path IN ({}) AND pm.embedded_text IS NOT NULL",
            placeholders
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let path_strs: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
        let results = stmt.query_map(rusqlite::params_from_iter(path_strs.iter()), |row| {
            Ok((PathBuf::from(row.get::<_, String>(0)?), row.get::<_, String>(1)?))
        })?;
        let mut map = std::collections::HashMap::new();
        for result in results {
            let (path, text) = result?;
            map.insert(path, text);
        }
        Ok(map)
    }

    /// Store embedding for a project
    pub fn upsert_embedding(&self, project_id: i64, embedding: &[f32]) -> Result<()> {
        // Delete existing embedding if any
        self.conn.execute(
            "DELETE FROM project_embeddings WHERE project_id = ?",
            [project_id],
        )?;

        // Insert new embedding
        self.conn.execute(
            "INSERT INTO project_embeddings (project_id, embedding) VALUES (?, ?)",
            params![project_id, embedding.as_bytes()],
        )?;
        Ok(())
    }

    /// Find most similar projects to a query embedding
    /// Returns (project_id, distance) pairs sorted by similarity (lower distance = more similar)
    pub fn find_similar(&self, query_embedding: &[f32], limit: usize) -> Result<Vec<(i64, f32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT project_id, distance
             FROM project_embeddings
             WHERE embedding MATCH ?
             ORDER BY distance
             LIMIT ?",
        )?;

        let results = stmt.query_map(params![query_embedding.as_bytes(), limit as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f32>(1)?))
        })?;

        results.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get projects that don't have embeddings yet
    pub fn get_unindexed_projects(&self) -> Result<Vec<(i64, PathBuf, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT p.id, p.path, p.name
             FROM projects p
             LEFT JOIN project_embeddings e ON p.id = e.project_id
             WHERE e.project_id IS NULL",
        )?;

        let results = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                PathBuf::from(row.get::<_, String>(1)?),
                row.get::<_, String>(2)?,
            ))
        })?;

        results.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Get project by ID
    pub fn get_project_by_id(&self, id: i64) -> Result<Option<Project>> {
        let mut stmt = self.conn.prepare(
            "SELECT path, name, last_accessed, access_count, source FROM projects WHERE id = ?",
        )?;

        let result = stmt
            .query_row([id], |row| {
                Ok(Project {
                    path: PathBuf::from(row.get::<_, String>(0)?),
                    name: row.get(1)?,
                    last_accessed: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    access_count: row.get(3)?,
                    source: row.get::<_, String>(4)?
                        .parse()
                        .unwrap_or(ProjectSource::Scan),
                })
            })
            .optional()?;

        Ok(result)
    }

    /// Insert or replace an FTS5 entry for a project
    pub fn fts_upsert(&self, project_id: i64, name: &str, embedded_text: &str) -> Result<()> {
        self.conn.execute("DELETE FROM project_fts WHERE rowid = ?", [project_id])?;
        self.conn.execute(
            "INSERT INTO project_fts(rowid, name, embedded_text) VALUES (?, ?, ?)",
            params![project_id, name, embedded_text],
        )?;
        Ok(())
    }

    /// Keyword search via FTS5 BM25 — returns (project_id, rank) sorted best-first
    pub fn fts_search(&self, query: &str, limit: usize) -> Result<Vec<(i64, f32)>> {
        let fts_query = match prepare_fts_query(query) {
            Some(q) => q,
            None => return Ok(vec![]),
        };
        let mut stmt = self.conn.prepare(
            "SELECT rowid, rank FROM project_fts WHERE project_fts MATCH ? ORDER BY rank LIMIT ?",
        )?;
        let results = stmt.query_map(params![fts_query, limit as i64], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, f32>(1)?))
        })?;
        results.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    /// Clear all embeddings (for re-indexing)
    pub fn clear_embeddings(&self) -> Result<()> {
        self.conn.execute("DELETE FROM project_embeddings", [])?;
        self.conn.execute("DELETE FROM project_metadata", [])?;
        self.conn.execute("DELETE FROM project_fts", [])?;
        Ok(())
    }

    /// Get embedding statistics
    pub fn embedding_stats(&self) -> Result<(usize, usize)> {
        let total: usize = self
            .conn
            .query_row("SELECT COUNT(*) FROM projects", [], |row| row.get(0))?;
        let indexed: usize = self.conn.query_row(
            "SELECT COUNT(*) FROM project_embeddings",
            [],
            |row| row.get(0),
        )?;
        Ok((indexed, total))
    }
}

/// Build a safe FTS5 MATCH query from user input.
/// Each token becomes a prefix match: `"kafka"* "consumer"*`
/// Returns None if no valid tokens remain after cleaning.
fn prepare_fts_query(query: &str) -> Option<String> {
    let tokens: Vec<String> = query
        .split_whitespace()
        .filter_map(|t| {
            let clean: String = t
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
                .collect();
            if clean.len() >= 2 {
                Some(format!("\"{}\"*", clean))
            } else {
                None
            }
        })
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(tokens.join(" "))
    }
}

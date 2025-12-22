use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Transaction};
use std::path::PathBuf;

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct Project {
    pub path: PathBuf,
    pub name: String,
    pub last_accessed: DateTime<Utc>,
    pub access_count: i64,
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
            "
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
                .map(|t| DateTime::<Utc>::from(t))
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
    pub fn mark_accessed(&self, path: &PathBuf) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        self.conn.execute(
            "UPDATE projects SET last_accessed = ?1, access_count = access_count + 1 WHERE path = ?2",
            params![now, path.to_string_lossy().as_ref()],
        )?;
        Ok(())
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

    /// Clear all projects from the database
    pub fn clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM projects", [])?;
        Ok(())
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

        // Batch delete in single transaction
        let tx = self.conn.transaction()?;
        {
            let mut delete_stmt = tx.prepare("DELETE FROM projects WHERE id = ?")?;
            for id in &missing_ids {
                delete_stmt.execute([id])?;
            }
        }
        tx.commit()?;

        Ok(missing_ids.len())
    }

}

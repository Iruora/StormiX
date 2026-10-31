//! Run history: every functional run and load test the app makes is kept
//! in a SQLite file, so runs can be reopened and compared later.
//!
//! [`diff`] compares two runs of the same kind: which steps broke or were
//! fixed, which checks changed, and how latency and throughput moved.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use stormix_engine::{FlowResult, LoadReport};

mod diff;

pub use diff::{diff, CheckChange, LoadDiff, Metric, RunDiff, StepChange, StepDiff, StepLoadDiff};

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("run history: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("run history holds a run this version cannot read: {0}")]
    Json(#[from] serde_json::Error),
    #[error("there is no run {0}")]
    NotFound(u32),
}

/// What a run produced.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "snake_case", tag = "kind", content = "result")]
pub enum RunData {
    Functional(FlowResult),
    Load(LoadReport),
}

/// One line of the history list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct RunSummary {
    pub id: u32,
    /// `functional` or `load`.
    pub kind: String,
    /// The flow's name.
    pub name: String,
    /// The file it came from, when it was saved or imported from one.
    pub source: Option<String>,
    /// Milliseconds since the Unix epoch.
    #[cfg_attr(feature = "specta", specta(type = f64))]
    pub started_at: i64,
    pub duration_ms: f64,
    pub passed: bool,
    /// A few words on the outcome, like `5/6 steps passed` or `p95 12 ms, 340 req/s`.
    pub headline: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "specta", derive(specta::Type))]
#[serde(rename_all = "camelCase")]
pub struct StoredRun {
    pub summary: RunSummary,
    pub data: RunData,
}

impl RunData {
    fn kind(&self) -> &'static str {
        match self {
            Self::Functional(_) => "functional",
            Self::Load(_) => "load",
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Functional(r) => &r.name,
            Self::Load(r) => &r.name,
        }
    }

    fn duration_ms(&self) -> f64 {
        match self {
            Self::Functional(r) => r.duration_ms,
            Self::Load(r) => r.summary.duration_ms,
        }
    }

    fn passed(&self) -> bool {
        match self {
            Self::Functional(r) => r.passed(),
            Self::Load(r) => r.passed(),
        }
    }

    fn headline(&self) -> String {
        match self {
            Self::Functional(r) => {
                let ran = r.steps.iter().filter(|s| !s.skipped).count();
                let (checks_passed, checks) = r.check_counts();
                format!(
                    "{}/{} steps passed, {checks_passed}/{checks} checks",
                    ran - r.failed_steps(),
                    r.steps.len()
                )
            }
            Self::Load(r) => {
                let s = &r.summary;
                format!(
                    "{} requests, {:.0} req/s, p95 {:.1} ms, {:.2}% failed",
                    s.requests,
                    s.rps,
                    s.latency.p95,
                    s.error_rate()
                )
            }
        }
    }
}

pub struct Store {
    db: Connection,
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS runs (
    id          INTEGER PRIMARY KEY,
    kind        TEXT NOT NULL,
    name        TEXT NOT NULL,
    source      TEXT,
    started_at  INTEGER NOT NULL,
    duration_ms REAL NOT NULL,
    passed      INTEGER NOT NULL,
    headline    TEXT NOT NULL,
    data        TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS runs_by_time ON runs (started_at DESC);
";

impl Store {
    /// Opens (or creates) the history at `path`.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        Self::init(Connection::open(path)?)
    }

    /// A history that lives only as long as the value, for tests.
    pub fn in_memory() -> Result<Self, StoreError> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(db: Connection) -> Result<Self, StoreError> {
        db.execute_batch(SCHEMA)?;
        Ok(Self { db })
    }

    /// Keeps a run that just ended and returns its summary.
    pub fn save(&self, data: &RunData, source: Option<&str>) -> Result<RunSummary, StoreError> {
        let ended = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64);
        let started_at = ended - data.duration_ms() as i64;
        self.db.execute(
            "INSERT INTO runs (kind, name, source, started_at, duration_ms, passed, headline, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                data.kind(),
                data.name(),
                source,
                started_at,
                data.duration_ms(),
                data.passed(),
                data.headline(),
                serde_json::to_string(data)?,
            ],
        )?;
        Ok(RunSummary {
            id: self.db.last_insert_rowid() as u32,
            kind: data.kind().into(),
            name: data.name().into(),
            source: source.map(Into::into),
            started_at,
            duration_ms: data.duration_ms(),
            passed: data.passed(),
            headline: data.headline(),
        })
    }

    /// The most recent runs first.
    pub fn list(&self, limit: u32) -> Result<Vec<RunSummary>, StoreError> {
        let mut stmt = self.db.prepare(
            "SELECT id, kind, name, source, started_at, duration_ms, passed, headline
             FROM runs ORDER BY started_at DESC, id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], summary_from_row)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn get(&self, id: u32) -> Result<StoredRun, StoreError> {
        let row = self
            .db
            .query_row(
                "SELECT id, kind, name, source, started_at, duration_ms, passed, headline, data
                 FROM runs WHERE id = ?1",
                [id],
                |row| Ok((summary_from_row(row)?, row.get::<_, String>(8)?)),
            )
            .optional()?;
        let (summary, data) = row.ok_or(StoreError::NotFound(id))?;
        Ok(StoredRun {
            summary,
            data: serde_json::from_str(&data)?,
        })
    }

    pub fn delete(&self, id: u32) -> Result<(), StoreError> {
        self.db.execute("DELETE FROM runs WHERE id = ?1", [id])?;
        Ok(())
    }
}

fn summary_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunSummary> {
    Ok(RunSummary {
        id: row.get(0)?,
        kind: row.get(1)?,
        name: row.get(2)?,
        source: row.get(3)?,
        started_at: row.get(4)?,
        duration_ms: row.get(5)?,
        passed: row.get(6)?,
        headline: row.get(7)?,
    })
}

#[cfg(test)]
mod tests;

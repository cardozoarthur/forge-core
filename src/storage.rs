use crate::graph::Workflow;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

pub struct ForgeStore {
    path: PathBuf,
    connection: Connection,
}

impl ForgeStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create store directory {}", parent.display())
            })?;
        }
        let connection = Connection::open(&path)
            .with_context(|| format!("failed to open SQLite store {}", path.display()))?;
        let store = Self { path, connection };
        store.migrate()?;
        Ok(store)
    }

    pub fn base_dir(&self) -> PathBuf {
        self.path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn migrate(&self) -> Result<()> {
        self.connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS workflows (
                id TEXT PRIMARY KEY,
                goal TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                data_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                path TEXT NOT NULL,
                sha256 TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                workflow_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS executor_policy (
                id TEXT PRIMARY KEY,
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS runtime_policy (
                id TEXT PRIMARY KEY,
                data_json TEXT NOT NULL,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                status TEXT NOT NULL,
                data_json TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
                updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
            );
            "#,
        )?;
        Ok(())
    }

    pub fn save_workflow(&self, workflow: &Workflow) -> Result<()> {
        let data_json = serde_json::to_string(workflow)?;
        self.connection.execute(
            r#"
            INSERT INTO workflows (id, goal, status, created_at, data_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                goal=excluded.goal,
                status=excluded.status,
                data_json=excluded.data_json
            "#,
            params![
                workflow.id,
                workflow.goal,
                workflow.status,
                workflow.created_at.to_rfc3339(),
                data_json
            ],
        )?;
        Ok(())
    }

    pub fn load_workflow(&self, id: &str) -> Result<Workflow> {
        let data_json: Option<String> = self
            .connection
            .query_row(
                "SELECT data_json FROM workflows WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let data_json = data_json.with_context(|| format!("workflow not found: {id}"))?;
        Ok(serde_json::from_str(&data_json)?)
    }

    pub fn record_event(
        &self,
        workflow_id: &str,
        kind: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            "INSERT INTO events (workflow_id, kind, data_json) VALUES (?1, ?2, ?3)",
            params![workflow_id, kind, serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    pub fn save_executor_state(&self, id: &str, data: &serde_json::Value) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO executor_policy (id, data_json, updated_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![id, serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    pub fn load_executor_states(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM executor_policy ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut states = Vec::new();
        for row in rows {
            states.push(serde_json::from_str(&row?)?);
        }
        Ok(states)
    }

    pub fn save_runtime_state(&self, id: &str, data: &serde_json::Value) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO runtime_policy (id, data_json, updated_at)
            VALUES (?1, ?2, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![id, serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    pub fn load_runtime_states(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM runtime_policy ORDER BY id")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut states = Vec::new();
        for row in rows {
            states.push(serde_json::from_str(&row?)?);
        }
        Ok(states)
    }

    pub fn save_run(
        &self,
        id: &str,
        workflow_id: &str,
        status: &str,
        data: &serde_json::Value,
    ) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO runs (id, workflow_id, status, data_json, updated_at)
            VALUES (?1, ?2, ?3, ?4, CURRENT_TIMESTAMP)
            ON CONFLICT(id) DO UPDATE SET
                workflow_id=excluded.workflow_id,
                status=excluded.status,
                data_json=excluded.data_json,
                updated_at=CURRENT_TIMESTAMP
            "#,
            params![id, workflow_id, status, serde_json::to_string(data)?],
        )?;
        Ok(())
    }

    pub fn load_run(&self, id: &str) -> Result<serde_json::Value> {
        let data_json: Option<String> = self
            .connection
            .query_row(
                "SELECT data_json FROM runs WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let data_json = data_json.with_context(|| format!("run not found: {id}"))?;
        Ok(serde_json::from_str(&data_json)?)
    }
}

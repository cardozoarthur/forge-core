use crate::checkpoint::TaskCheckpoint;
use crate::graph::Workflow;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};

pub struct ForgeStore {
    path: PathBuf,
    connection: Connection,
}

pub struct TaskLeaseWrite<'a> {
    pub workflow_id: &'a str,
    pub task_id: &'a str,
    pub lease_id: &'a str,
    pub executor: &'a str,
    pub acquired_at: &'a str,
    pub expires_at: &'a str,
    pub data: &'a serde_json::Value,
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
            CREATE TABLE IF NOT EXISTS task_leases (
                workflow_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                lease_id TEXT NOT NULL,
                executor TEXT NOT NULL,
                acquired_at TEXT NOT NULL,
                expires_at TEXT NOT NULL,
                data_json TEXT NOT NULL,
                PRIMARY KEY (workflow_id, task_id)
            );
            CREATE TABLE IF NOT EXISTS task_checkpoints (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                task_id TEXT NOT NULL,
                executor TEXT NOT NULL,
                state TEXT NOT NULL,
                created_at TEXT NOT NULL,
                data_json TEXT NOT NULL
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

    pub fn load_workflows(&self) -> Result<Vec<Workflow>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM workflows ORDER BY created_at ASC, id ASC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut workflows = Vec::new();
        for row in rows {
            workflows.push(serde_json::from_str(&row?)?);
        }
        Ok(workflows)
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

    pub fn load_runs(&self) -> Result<Vec<serde_json::Value>> {
        let mut statement = self
            .connection
            .prepare("SELECT data_json FROM runs ORDER BY created_at ASC, id ASC")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut runs = Vec::new();
        for row in rows {
            runs.push(serde_json::from_str(&row?)?);
        }
        Ok(runs)
    }

    pub fn try_save_task_lease(&self, lease: TaskLeaseWrite<'_>) -> Result<bool> {
        let changed = self.connection.execute(
            r#"
            INSERT INTO task_leases (
                workflow_id,
                task_id,
                lease_id,
                executor,
                acquired_at,
                expires_at,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(workflow_id, task_id) DO UPDATE SET
                lease_id=excluded.lease_id,
                executor=excluded.executor,
                acquired_at=excluded.acquired_at,
                expires_at=excluded.expires_at,
                data_json=excluded.data_json
            WHERE task_leases.expires_at <= ?8
            "#,
            params![
                lease.workflow_id,
                lease.task_id,
                lease.lease_id,
                lease.executor,
                lease.acquired_at,
                lease.expires_at,
                serde_json::to_string(lease.data)?,
                lease.acquired_at
            ],
        )?;
        Ok(changed == 1)
    }

    pub fn load_task_lease(
        &self,
        workflow_id: &str,
        task_id: &str,
    ) -> Result<Option<serde_json::Value>> {
        let data_json: Option<String> = self
            .connection
            .query_row(
                "SELECT data_json FROM task_leases WHERE workflow_id = ?1 AND task_id = ?2",
                params![workflow_id, task_id],
                |row| row.get(0),
            )
            .optional()?;
        data_json
            .map(|value| serde_json::from_str(&value).map_err(Into::into))
            .transpose()
    }

    pub fn delete_task_lease(
        &self,
        workflow_id: &str,
        task_id: &str,
        lease_id: &str,
    ) -> Result<bool> {
        let changed = self.connection.execute(
            r#"
            DELETE FROM task_leases
            WHERE workflow_id = ?1 AND task_id = ?2 AND lease_id = ?3
            "#,
            params![workflow_id, task_id, lease_id],
        )?;
        Ok(changed == 1)
    }

    pub fn save_task_checkpoint(&self, checkpoint: &TaskCheckpoint) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO task_checkpoints (
                id,
                workflow_id,
                task_id,
                executor,
                state,
                created_at,
                data_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                &checkpoint.checkpoint_id,
                &checkpoint.workflow_id,
                &checkpoint.task_id,
                &checkpoint.executor,
                &checkpoint.state,
                checkpoint.created_at.to_rfc3339(),
                serde_json::to_string(checkpoint)?
            ],
        )?;
        Ok(())
    }

    pub fn load_task_checkpoints(
        &self,
        workflow_id: &str,
        task_id: Option<&str>,
    ) -> Result<Vec<serde_json::Value>> {
        let sql = if task_id.is_some() {
            r#"
            SELECT data_json FROM task_checkpoints
            WHERE workflow_id = ?1 AND task_id = ?2
            ORDER BY created_at ASC, id ASC
            "#
        } else {
            r#"
            SELECT data_json FROM task_checkpoints
            WHERE workflow_id = ?1
            ORDER BY created_at ASC, id ASC
            "#
        };
        let mut statement = self.connection.prepare(sql)?;
        let mut checkpoints = Vec::new();
        if let Some(task_id) = task_id {
            let rows = statement
                .query_map(params![workflow_id, task_id], |row| row.get::<_, String>(0))?;
            for row in rows {
                checkpoints.push(serde_json::from_str(&row?)?);
            }
        } else {
            let rows = statement.query_map(params![workflow_id], |row| row.get::<_, String>(0))?;
            for row in rows {
                checkpoints.push(serde_json::from_str(&row?)?);
            }
        }
        Ok(checkpoints)
    }
}

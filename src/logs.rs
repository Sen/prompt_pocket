use std::{path::Path, str::FromStr};

use chrono::Utc;
use serde::Serialize;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    FromRow, SqlitePool,
};
use thiserror::Error;

use crate::prompts::{PromptMode, ZhToEnTone};

#[derive(Clone)]
pub struct LogStore {
    pool: SqlitePool,
    limit: i64,
}

#[derive(Clone, Debug)]
pub struct CallLogCreateInput {
    pub duration_ms: i64,
    pub status: CallLogStatus,
    pub mode: PromptMode,
    pub model: String,
    pub tone: Option<ZhToEnTone>,
    pub input: String,
    pub output_text: Option<String>,
    pub error: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallLogStatus {
    Success,
    Error,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallLogEntry {
    pub id: i64,
    pub created_at: String,
    pub duration_ms: i64,
    pub status: CallLogStatus,
    pub mode: PromptMode,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tone: Option<ZhToEnTone>,
    pub input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub request_id: Option<String>,
}

#[derive(Debug, Error)]
pub enum LogError {
    #[error("Call log database operation failed. {0}")]
    Database(#[from] sqlx::Error),
    #[error("Could not create database directory. {0}")]
    Directory(#[from] std::io::Error),
}

impl LogStore {
    pub async fn connect(database_path: &str, limit: i64) -> Result<Self, LogError> {
        let pool = create_pool(database_path).await?;
        let store = Self { pool, limit };

        store.init().await?;

        Ok(store)
    }

    pub fn from_pool(pool: SqlitePool, limit: i64) -> Self {
        Self { pool, limit }
    }

    pub fn limit(&self) -> i64 {
        self.limit
    }

    pub async fn init(&self) -> Result<(), LogError> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS call_logs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL,
                duration_ms INTEGER NOT NULL,
                status TEXT NOT NULL,
                mode TEXT NOT NULL,
                model TEXT NOT NULL,
                tone TEXT,
                input TEXT NOT NULL,
                output_text TEXT,
                error TEXT,
                request_id TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE INDEX IF NOT EXISTS call_logs_created_at_id_idx
            ON call_logs (created_at, id)
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn add(&self, input: CallLogCreateInput) -> Result<CallLogEntry, LogError> {
        let row = sqlx::query_as::<_, CallLogRow>(
            r#"
            INSERT INTO call_logs (
                created_at,
                duration_ms,
                status,
                mode,
                model,
                tone,
                input,
                output_text,
                error,
                request_id
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            RETURNING id, created_at, duration_ms, status, mode, model, tone, input, output_text, error, request_id
            "#,
        )
        .bind(Utc::now().to_rfc3339())
        .bind(input.duration_ms)
        .bind(input.status.as_str())
        .bind(input.mode.as_str())
        .bind(input.model)
        .bind(input.tone.map(|tone| tone.as_str().to_string()))
        .bind(input.input)
        .bind(input.output_text)
        .bind(input.error)
        .bind(input.request_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.into_entry())
    }

    pub async fn list(&self) -> Result<Vec<CallLogEntry>, LogError> {
        let rows = sqlx::query_as::<_, CallLogRow>(
            r#"
            SELECT id, created_at, duration_ms, status, mode, model, tone, input, output_text, error, request_id
            FROM call_logs
            ORDER BY created_at DESC, id DESC
            LIMIT ?
            "#,
        )
        .bind(self.limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(CallLogRow::into_entry).collect())
    }
}

impl CallLogStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            CallLogStatus::Success => "success",
            CallLogStatus::Error => "error",
        }
    }
}

#[derive(FromRow)]
struct CallLogRow {
    id: i64,
    created_at: String,
    duration_ms: i64,
    status: String,
    mode: String,
    model: String,
    tone: Option<String>,
    input: String,
    output_text: Option<String>,
    error: Option<String>,
    request_id: Option<String>,
}

impl CallLogRow {
    fn into_entry(self) -> CallLogEntry {
        CallLogEntry {
            id: self.id,
            created_at: self.created_at,
            duration_ms: self.duration_ms,
            status: parse_status(&self.status),
            mode: self.mode.parse().unwrap_or(PromptMode::PolishEn),
            model: self.model,
            tone: self.tone.and_then(|tone| tone.parse().ok()),
            input: self.input,
            output_text: self.output_text,
            error: self.error,
            request_id: self.request_id,
        }
    }
}

async fn create_pool(database_path: &str) -> Result<SqlitePool, LogError> {
    let options = if database_path == ":memory:" {
        SqliteConnectOptions::new()
            .in_memory(true)
            .journal_mode(SqliteJournalMode::Wal)
            .create_if_missing(true)
    } else {
        let path = Path::new(database_path);

        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }

        SqliteConnectOptions::from_str(&format!("sqlite://{}", database_path))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
    };
    let max_connections = if database_path == ":memory:" { 1 } else { 5 };

    Ok(SqlitePoolOptions::new()
        .max_connections(max_connections)
        .connect_with(options)
        .await?)
}

fn parse_status(status: &str) -> CallLogStatus {
    match status {
        "success" => CallLogStatus::Success,
        _ => CallLogStatus::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn persists_entries_and_limits_listing() {
        let store = LogStore::connect(":memory:", 1).await.unwrap();

        store
            .add(CallLogCreateInput {
                duration_ms: 12,
                status: CallLogStatus::Success,
                mode: PromptMode::ZhToEn,
                model: "gpt-5.2".to_string(),
                tone: Some(ZhToEnTone::Casual),
                input: "hello".to_string(),
                output_text: Some("processed:hello".to_string()),
                error: None,
                request_id: Some("req_success".to_string()),
            })
            .await
            .unwrap();
        store
            .add(CallLogCreateInput {
                duration_ms: 8,
                status: CallLogStatus::Error,
                mode: PromptMode::PolishEn,
                model: "gpt-5.2".to_string(),
                tone: None,
                input: "fail".to_string(),
                output_text: None,
                error: Some("mock failure".to_string()),
                request_id: None,
            })
            .await
            .unwrap();

        let limited_logs = store.list().await.unwrap();

        assert_eq!(limited_logs.len(), 1);
        assert_eq!(limited_logs[0].status, CallLogStatus::Error);
        assert_eq!(limited_logs[0].input, "fail");
    }
}

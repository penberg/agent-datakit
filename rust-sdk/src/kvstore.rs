use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use turso::{Builder, Connection};

/// A key-value store backed by SQLite
#[derive(Clone)]
pub struct KvStore {
    conn: Arc<Connection>,
}

impl KvStore {
    /// Create a new KV store
    pub async fn new(db_path: &str) -> Result<Self> {
        let db = Builder::new_local(db_path).build().await?;
        let conn = db.connect()?;
        let kv = Self {
            conn: Arc::new(conn),
        };
        kv.initialize().await?;
        Ok(kv)
    }

    /// Create a KV store from an existing connection
    pub async fn from_connection(conn: Arc<Connection>) -> Result<Self> {
        let kv = Self { conn };
        kv.initialize().await?;
        Ok(kv)
    }

    /// Initialize the database schema
    async fn initialize(&self) -> Result<()> {
        // Enable foreign key constraints
        self.conn.execute("PRAGMA foreign_keys = ON", ()).await?;

        self.conn
            .execute(
                "CREATE TABLE IF NOT EXISTS kv_store (
                    key TEXT PRIMARY KEY,
                    value TEXT NOT NULL,
                    created_at INTEGER DEFAULT (unixepoch()),
                    updated_at INTEGER DEFAULT (unixepoch())
                )",
                (),
            )
            .await?;

        self.conn
            .execute(
                "CREATE INDEX IF NOT EXISTS idx_kv_store_created_at
                ON kv_store(created_at)",
                (),
            )
            .await?;

        Ok(())
    }

    /// Set a key-value pair
    pub async fn set<V: Serialize>(&self, key: &str, value: &V) -> Result<()> {
        let serialized = serde_json::to_string(value)?;
        self.conn
            .execute(
                "INSERT INTO kv_store (key, value, updated_at)
                VALUES (?, ?, unixepoch())
                ON CONFLICT(key) DO UPDATE SET
                    value = excluded.value,
                    updated_at = unixepoch()",
                (key, serialized.as_str()),
            )
            .await?;
        Ok(())
    }

    /// Get a value by key
    pub async fn get<V: for<'de> Deserialize<'de>>(&self, key: &str) -> Result<Option<V>> {
        let mut rows = self
            .conn
            .query("SELECT value FROM kv_store WHERE key = ?", (key,))
            .await?;

        if let Some(row) = rows.next().await? {
            if let Some(value_str) = row.get_value(0).ok().and_then(|v| {
                if let turso::Value::Text(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            }) {
                let value: V = serde_json::from_str(&value_str)?;
                Ok(Some(value))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    /// Delete a key
    pub async fn delete(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM kv_store WHERE key = ?", (key,))
            .await?;
        Ok(())
    }

    /// List all keys
    pub async fn keys(&self) -> Result<Vec<String>> {
        let mut rows = self.conn.query("SELECT key FROM kv_store", ()).await?;
        let mut keys = Vec::new();
        while let Some(row) = rows.next().await? {
            if let Some(key) = row.get_value(0).ok().and_then(|v| {
                if let turso::Value::Text(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            }) {
                keys.push(key);
            }
        }
        Ok(keys)
    }
}

pub mod filesystem;
pub mod kvstore;
pub mod toolcalls;

use anyhow::Result;
use std::sync::Arc;
use turso::{Builder, Connection};

pub use filesystem::{Filesystem, Stats};
pub use kvstore::KvStore;
pub use toolcalls::{ToolCall, ToolCallStats, ToolCallStatus, ToolCalls};

/// The main AgentFS SDK struct
///
/// This provides a unified interface to the filesystem, key-value store,
/// and tool calls tracking backed by a SQLite database.
pub struct AgentFS {
    conn: Arc<Connection>,
    pub kv: KvStore,
    pub fs: Filesystem,
    pub tools: ToolCalls,
}

impl AgentFS {
    /// Create a new AgentFS instance
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file (use ":memory:" for in-memory database)
    pub async fn new(db_path: &str) -> Result<Self> {
        let db = Builder::new_local(db_path).build().await?;
        let conn = db.connect()?;
        let conn = Arc::new(conn);

        let kv = KvStore::from_connection(conn.clone()).await?;
        let fs = Filesystem::from_connection(conn.clone()).await?;
        let tools = ToolCalls::from_connection(conn.clone()).await?;

        Ok(Self {
            conn,
            kv,
            fs,
            tools,
        })
    }

    /// Get the underlying database connection
    pub fn get_connection(&self) -> Arc<Connection> {
        self.conn.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_agentfs_creation() {
        let agentfs = AgentFS::new(":memory:").await.unwrap();
        // Just verify we can get the connection
        let _conn = agentfs.get_connection();
    }

    #[tokio::test]
    async fn test_kv_operations() {
        let agentfs = AgentFS::new(":memory:").await.unwrap();

        // Set a value
        agentfs.kv.set("test_key", &"test_value").await.unwrap();

        // Get the value
        let value: Option<String> = agentfs.kv.get("test_key").await.unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        // Delete the value
        agentfs.kv.delete("test_key").await.unwrap();

        // Verify deletion
        let value: Option<String> = agentfs.kv.get("test_key").await.unwrap();
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn test_filesystem_operations() {
        let agentfs = AgentFS::new(":memory:").await.unwrap();

        // Create a directory
        agentfs.fs.mkdir("/test_dir").await.unwrap();

        // Check directory exists
        let stats = agentfs.fs.stat("/test_dir").await.unwrap();
        assert!(stats.is_some());
        assert!(stats.unwrap().is_directory());

        // Write a file
        let data = b"Hello, AgentFS!";
        agentfs
            .fs
            .write_file("/test_dir/test.txt", data)
            .await
            .unwrap();

        // Read the file
        let read_data = agentfs
            .fs
            .read_file("/test_dir/test.txt")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(read_data, data);

        // List directory
        let entries = agentfs.fs.readdir("/test_dir").await.unwrap().unwrap();
        assert_eq!(entries, vec!["test.txt"]);
    }

    #[tokio::test]
    async fn test_tool_calls() {
        let agentfs = AgentFS::new(":memory:").await.unwrap();

        // Start a tool call
        let id = agentfs
            .tools
            .start("test_tool", Some(serde_json::json!({"param": "value"})))
            .await
            .unwrap();

        // Mark it as successful
        agentfs
            .tools
            .success(id, Some(serde_json::json!({"result": "success"})))
            .await
            .unwrap();

        // Get the tool call
        let call = agentfs.tools.get(id).await.unwrap().unwrap();
        assert_eq!(call.name, "test_tool");
        assert_eq!(call.status, ToolCallStatus::Success);

        // Get stats
        let stats = agentfs.tools.stats_for("test_tool").await.unwrap().unwrap();
        assert_eq!(stats.total_calls, 1);
        assert_eq!(stats.successful, 1);
    }
}

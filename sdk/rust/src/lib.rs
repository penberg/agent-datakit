pub mod filesystem;
pub mod kvstore;
pub mod toolcalls;

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use turso::{Builder, Connection};

pub use filesystem::{Filesystem, Stats};
pub use kvstore::KvStore;
pub use toolcalls::{ToolCall, ToolCallStats, ToolCallStatus, ToolCalls};

/// Configuration options for opening an AgentFS instance
#[derive(Debug, Clone, Default)]
pub struct AgentFSOptions {
    /// Optional unique identifier for the agent.
    /// - If Some(id): Creates persistent storage at `.agentfs/{id}.db`
    /// - If None: Uses ephemeral in-memory database
    pub id: Option<String>,
}

impl AgentFSOptions {
    /// Create options for a persistent agent with the given ID
    pub fn with_id(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
        }
    }

    /// Create options for an ephemeral in-memory agent
    pub fn ephemeral() -> Self {
        Self { id: None }
    }
}

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
    /// Open an AgentFS instance
    ///
    /// # Arguments
    /// * `options` - Configuration options (use Default::default() for ephemeral)
    ///
    /// # Examples
    /// ```no_run
    /// use agentfs_sdk::{AgentFS, AgentFSOptions};
    ///
    /// # async fn example() -> anyhow::Result<()> {
    /// // Persistent storage
    /// let agent = AgentFS::open(AgentFSOptions::with_id("my-agent")).await?;
    ///
    /// // Ephemeral in-memory
    /// let agent = AgentFS::open(AgentFSOptions::ephemeral()).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn open(options: AgentFSOptions) -> Result<Self> {
        // Determine database path based on id
        let db_path = if let Some(id) = options.id {
            // Ensure .agentfs directory exists
            let agentfs_dir = Path::new(".agentfs");
            if !agentfs_dir.exists() {
                std::fs::create_dir_all(agentfs_dir)?;
            }
            format!(".agentfs/{}.db", id)
        } else {
            // No id = ephemeral in-memory database
            ":memory:".to_string()
        };

        let db = Builder::new_local(&db_path).build().await?;
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

    /// Create a new AgentFS instance (deprecated, use `open` instead)
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file (use ":memory:" for in-memory database)
    #[deprecated(
        since = "0.2.0",
        note = "Use AgentFS::open with AgentFSOptions instead"
    )]
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
        let agentfs = AgentFS::open(AgentFSOptions::ephemeral()).await.unwrap();
        // Just verify we can get the connection
        let _conn = agentfs.get_connection();
    }

    #[tokio::test]
    async fn test_agentfs_with_id() {
        let agentfs = AgentFS::open(AgentFSOptions::with_id("test-agent"))
            .await
            .unwrap();
        // Just verify we can get the connection
        let _conn = agentfs.get_connection();

        // Cleanup
        let _ = std::fs::remove_file(".agentfs/test-agent.db");
    }

    #[tokio::test]
    async fn test_kv_operations() {
        let agentfs = AgentFS::open(AgentFSOptions::ephemeral()).await.unwrap();

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
        let agentfs = AgentFS::open(AgentFSOptions::ephemeral()).await.unwrap();

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
        let agentfs = AgentFS::open(AgentFSOptions::ephemeral()).await.unwrap();

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

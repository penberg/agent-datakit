mod cmd;

// Non-Linux placeholder types for MountConfig (needed for CLI parsing)
#[cfg(not(target_os = "linux"))]
mod non_linux {
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum MountType {
        Bind { src: PathBuf },
        Sqlite { src: PathBuf },
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MountConfig {
        pub mount_type: MountType,
        pub dst: PathBuf,
    }

    impl std::str::FromStr for MountConfig {
        type Err = String;

        fn from_str(_s: &str) -> Result<Self, Self::Err> {
            // This will never be called on non-Linux platforms
            Err("Mount configuration is only supported on Linux".to_string())
        }
    }
}

use agentfs_sdk::AgentFS;
use anyhow::{Context, Result as AnyhowResult};
use clap::{Parser, Subcommand};
use cmd::MountConfig;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use turso::{Builder, Value};

#[derive(Parser, Debug)]
#[command(name = "agentfs")]
#[command(about = "A sandbox for agents that intercepts filesystem operations", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initialize a new agent filesystem
    Init {
        /// SQLite file to create (default: agent.db)
        #[arg(default_value = "agent.db")]
        filename: PathBuf,

        /// Overwrite existing file if it exists
        #[arg(long)]
        force: bool,
    },
    /// Filesystem operations
    Fs {
        #[command(subcommand)]
        command: FsCommands,
    },
    Run {
        /// Mount configuration (format: type=bind,src=<host_path>,dst=<sandbox_path>)
        #[arg(long = "mount", value_name = "MOUNT_SPEC")]
        mounts: Vec<MountConfig>,

        /// Enable strace-like output for system calls
        #[arg(long = "strace")]
        strace: bool,

        /// Command to execute
        command: PathBuf,

        /// Arguments for the command
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
enum FsCommands {
    /// List files in the filesystem
    Ls {
        /// Filesystem to use (default: agent.db)
        #[arg(long = "filesystem", default_value = "agent.db")]
        filesystem: PathBuf,

        /// Path to list (default: /)
        #[arg(default_value = "/")]
        path: String,
    },
    /// Display file contents
    Cat {
        /// Filesystem to use (default: agent.db)
        #[arg(long = "filesystem", default_value = "agent.db")]
        filesystem: PathBuf,

        /// Path to the file
        path: String,
    },
}

async fn init_database(db_path: &Path, force: bool) -> AnyhowResult<()> {
    // Check if file already exists
    if db_path.exists() && !force {
        anyhow::bail!(
            "File '{}' already exists. Use --force to overwrite.",
            db_path.display()
        );
    }

    let db_path_str = db_path.to_str().context("Invalid database path")?;

    // Use the SDK to initialize the database - this ensures consistency
    // with how `agentfs run` initializes the database
    AgentFS::new(db_path_str)
        .await
        .context("Failed to initialize database")?;

    eprintln!("Created agent filesystem: {}", db_path.display());

    Ok(())
}

async fn ls_filesystem(db_path: &Path, path: &str) -> AnyhowResult<()> {
    if !db_path.exists() {
        anyhow::bail!("Filesystem '{}' does not exist", db_path.display());
    }

    let db_path_str = db_path.to_str().context("Invalid filesystem path")?;

    let db = Builder::new_local(db_path_str)
        .build()
        .await
        .context("Failed to open filesystem")?;

    let conn = db.connect().context("Failed to connect to filesystem")?;

    const ROOT_INO: i64 = 1;
    const S_IFMT: u32 = 0o170000;
    const S_IFDIR: u32 = 0o040000;

    if path != "/" {
        anyhow::bail!("Only root directory (/) is currently supported");
    }

    let mut queue: VecDeque<(i64, String)> = VecDeque::new();
    queue.push_back((ROOT_INO, String::new()));

    while let Some((parent_ino, prefix)) = queue.pop_front() {
        let query = format!(
            "SELECT d.name, d.ino, i.mode FROM fs_dentry d
             JOIN fs_inode i ON d.ino = i.ino
             WHERE d.parent_ino = {}
             ORDER BY d.name",
            parent_ino
        );

        let mut rows = conn
            .query(&query, ())
            .await
            .context("Failed to query directory entries")?;

        let mut entries = Vec::new();
        while let Some(row) = rows.next().await.context("Failed to fetch row")? {
            let name: String = row
                .get_value(0)
                .ok()
                .and_then(|v| {
                    if let Value::Text(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let ino: i64 = row
                .get_value(1)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .unwrap_or(0);

            let mode: u32 = row
                .get_value(2)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .unwrap_or(0) as u32;

            entries.push((name, ino, mode));
        }

        for (name, ino, mode) in entries {
            let is_dir = mode & S_IFMT == S_IFDIR;
            let type_char = if is_dir { 'd' } else { 'f' };
            let full_path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", prefix, name)
            };

            println!("{} {}", type_char, full_path);

            if is_dir {
                queue.push_back((ino, full_path));
            }
        }
    }

    Ok(())
}

async fn cat_filesystem(db_path: &Path, path: &str) -> AnyhowResult<()> {
    if !db_path.exists() {
        anyhow::bail!("Filesystem '{}' does not exist", db_path.display());
    }

    let db_path_str = db_path.to_str().context("Invalid filesystem path")?;

    let db = Builder::new_local(db_path_str)
        .build()
        .await
        .context("Failed to open filesystem")?;

    let conn = db.connect().context("Failed to connect to filesystem")?;

    const ROOT_INO: i64 = 1;

    let path_components: Vec<&str> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let mut current_ino = ROOT_INO;

    for component in path_components {
        let query = format!(
            "SELECT ino FROM fs_dentry WHERE parent_ino = {} AND name = '{}'",
            current_ino, component
        );

        let mut rows = conn
            .query(&query, ())
            .await
            .context("Failed to query directory entries")?;

        if let Some(row) = rows.next().await.context("Failed to fetch row")? {
            current_ino = row
                .get_value(0)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .ok_or_else(|| anyhow::anyhow!("Invalid inode"))?;
        } else {
            anyhow::bail!("File not found: {}", path);
        }
    }

    let query = format!("SELECT mode FROM fs_inode WHERE ino = {}", current_ino);
    let mut rows = conn
        .query(&query, ())
        .await
        .context("Failed to query inode")?;

    if let Some(row) = rows.next().await.context("Failed to fetch row")? {
        let mode: u32 = row
            .get_value(0)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0) as u32;

        const S_IFMT: u32 = 0o170000;
        const S_IFDIR: u32 = 0o040000;
        const S_IFREG: u32 = 0o100000;

        if mode & S_IFMT == S_IFDIR {
            anyhow::bail!("'{}' is a directory", path);
        } else if mode & S_IFMT != S_IFREG {
            anyhow::bail!("'{}' is not a regular file", path);
        }
    } else {
        anyhow::bail!("File not found: {}", path);
    }

    let query = format!(
        "SELECT data FROM fs_data WHERE ino = {} ORDER BY offset",
        current_ino
    );

    let mut rows = conn
        .query(&query, ())
        .await
        .context("Failed to query file data")?;

    use std::io::Write;
    let stdout = std::io::stdout();
    let mut handle = stdout.lock();

    while let Some(row) = rows.next().await.context("Failed to fetch row")? {
        let data: Vec<u8> = row
            .get_value(0)
            .ok()
            .and_then(|v| {
                if let Value::Blob(b) = v {
                    Some(b.clone())
                } else if let Value::Text(t) = v {
                    Some(t.as_bytes().to_vec())
                } else {
                    None
                }
            })
            .ok_or_else(|| anyhow::anyhow!("Invalid file data"))?;

        handle
            .write_all(&data)
            .context("Failed to write to stdout")?;
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    match args.command {
        Commands::Init { filename, force } => {
            if let Err(e) = init_database(&filename, force).await {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            std::process::exit(0);
        }
        Commands::Fs { command } => match command {
            FsCommands::Ls { filesystem, path } => {
                if let Err(e) = ls_filesystem(&filesystem, &path).await {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
            FsCommands::Cat { filesystem, path } => {
                if let Err(e) = cat_filesystem(&filesystem, &path).await {
                    eprintln!("Error: {}", e);
                    std::process::exit(1);
                }
                std::process::exit(0);
            }
        },
        Commands::Run {
            mounts,
            strace,
            command,
            args,
        } => {
            cmd::handle_run_command(mounts, strace, command, args).await;
        }
    }
}

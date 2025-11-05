use agentfs_sandbox::{
    init_fd_tables, init_mount_table, init_strace, MountConfig, MountTable, PassthroughVfs,
    Sandbox, SqliteVfs,
};
use anyhow::{Context, Result as AnyhowResult};
use clap::{Parser, Subcommand};
use reverie::Error;
use reverie_process::Command;
use reverie_ptrace::TracerBuilder;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
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

    let db = Builder::new_local(db_path_str)
        .build()
        .await
        .context("Failed to build database")?;

    let conn = db.connect().context("Failed to connect to database")?;

    // Create fs_inode table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS fs_inode (
            ino INTEGER PRIMARY KEY AUTOINCREMENT,
            mode INTEGER NOT NULL,
            uid INTEGER NOT NULL DEFAULT 0,
            gid INTEGER NOT NULL DEFAULT 0,
            size INTEGER NOT NULL DEFAULT 0,
            atime INTEGER NOT NULL,
            mtime INTEGER NOT NULL,
            ctime INTEGER NOT NULL
        )",
        (),
    )
    .await
    .context("Failed to create fs_inode table")?;

    // Create fs_dentry table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS fs_dentry (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            parent_ino INTEGER NOT NULL,
            ino INTEGER NOT NULL,
            FOREIGN KEY (ino) REFERENCES fs_inode(ino) ON DELETE CASCADE,
            FOREIGN KEY (parent_ino) REFERENCES fs_inode(ino) ON DELETE CASCADE,
            UNIQUE(parent_ino, name)
        )",
        (),
    )
    .await
    .context("Failed to create fs_dentry table")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_fs_dentry_parent ON fs_dentry(parent_ino, name)",
        (),
    )
    .await
    .context("Failed to create fs_dentry index")?;

    // Create fs_data table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS fs_data (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            ino INTEGER NOT NULL,
            offset INTEGER NOT NULL,
            size INTEGER NOT NULL,
            data BLOB NOT NULL,
            FOREIGN KEY (ino) REFERENCES fs_inode(ino) ON DELETE CASCADE
        )",
        (),
    )
    .await
    .context("Failed to create fs_data table")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_fs_data_ino_offset ON fs_data(ino, offset)",
        (),
    )
    .await
    .context("Failed to create fs_data index")?;

    // Create fs_symlink table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS fs_symlink (
            ino INTEGER PRIMARY KEY,
            target TEXT NOT NULL,
            FOREIGN KEY (ino) REFERENCES fs_inode(ino) ON DELETE CASCADE
        )",
        (),
    )
    .await
    .context("Failed to create fs_symlink table")?;

    // Create kv_store table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS kv_store (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            created_at INTEGER DEFAULT (unixepoch()),
            updated_at INTEGER DEFAULT (unixepoch())
        )",
        (),
    )
    .await
    .context("Failed to create kv_store table")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_kv_store_created_at ON kv_store(created_at)",
        (),
    )
    .await
    .context("Failed to create kv_store index")?;

    // Create tool_calls table
    conn.execute(
        "CREATE TABLE IF NOT EXISTS tool_calls (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            parameters TEXT,
            result TEXT,
            error TEXT,
            status TEXT NOT NULL,
            started_at INTEGER NOT NULL,
            completed_at INTEGER,
            duration_ms INTEGER
        )",
        (),
    )
    .await
    .context("Failed to create tool_calls table")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tool_calls_name ON tool_calls(name)",
        (),
    )
    .await
    .context("Failed to create tool_calls name index")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tool_calls_started_at ON tool_calls(started_at)",
        (),
    )
    .await
    .context("Failed to create tool_calls started_at index")?;

    conn.execute(
        "CREATE INDEX IF NOT EXISTS idx_tool_calls_status ON tool_calls(status)",
        (),
    )
    .await
    .context("Failed to create tool_calls status index")?;

    // Initialize root directory
    const ROOT_INO: i64 = 1;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    // Check if root exists
    let mut rows = conn
        .query("SELECT COUNT(*) FROM fs_inode WHERE ino = ?", (ROOT_INO,))
        .await
        .context("Failed to check root existence")?;

    let root_count: i64 = if let Some(row) = rows.next().await.context("Failed to fetch row")? {
        row.get_value(0)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0)
    } else {
        0
    };

    if root_count == 0 {
        // Create root directory (ino=1, mode=0o040755)
        conn.execute(
            "INSERT INTO fs_inode (ino, mode, uid, gid, size, atime, mtime, ctime)
             VALUES (?, ?, 0, 0, 0, ?, ?, ?)",
            (ROOT_INO, 0o040755u32, now, now, now),
        )
        .await
        .context("Failed to create root inode")?;
    }

    eprintln!("Created agent filesystem: {}", db_path.display());

    Ok(())
}

async fn ls_filesystem(db_path: &Path, path: &str) -> AnyhowResult<()> {
    // Check if file exists
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

    // For now, only support root directory
    if path != "/" {
        anyhow::bail!("Only root directory (/) is currently supported");
    }

    let parent_ino = ROOT_INO;

    // Query directory entries
    let query = format!(
        "SELECT d.name, i.mode FROM fs_dentry d
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

        let mode: u32 = row
            .get_value(1)
            .ok()
            .and_then(|v| v.as_integer().copied())
            .unwrap_or(0) as u32;

        entries.push((name, mode));
    }

    // Print entries
    for (name, mode) in entries {
        // Determine file type
        const S_IFMT: u32 = 0o170000;
        const S_IFDIR: u32 = 0o040000;

        let type_char = if mode & S_IFMT == S_IFDIR { 'd' } else { 'f' };

        println!("{} {}", type_char, name);
    }

    Ok(())
}

async fn cat_filesystem(db_path: &Path, path: &str) -> AnyhowResult<()> {
    // Check if file exists
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

    // Resolve the path to an inode
    let path_components: Vec<&str> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let mut current_ino = ROOT_INO;

    for component in path_components {
        // Query for the next component
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

    // Check if it's a regular file
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

    // Read all data chunks for this file
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
async fn main() -> Result<(), Error> {
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
            mut mounts,
            strace,
            command,
            args,
        } => {
            eprintln!("Welcome to AgentFS!");
            eprintln!();

            let mut mount_table = MountTable::new();

            // If no mounts specified, add default agent.db mount at /agent
            if mounts.is_empty() {
                mounts.push(MountConfig {
                    mount_type: agentfs_sandbox::MountType::Sqlite {
                        src: PathBuf::from("agent.db"),
                    },
                    dst: PathBuf::from("/agent"),
                });
            }

            eprintln!("The following mount points are sandboxed:");
            for mount_config in &mounts {
                match &mount_config.mount_type {
                    agentfs_sandbox::MountType::Bind { src } => {
                        eprintln!(
                            " - {} -> {} (host)",
                            mount_config.dst.display(),
                            src.display()
                        );

                        // Create a PassthroughVfs for this bind mount
                        let vfs =
                            Arc::new(PassthroughVfs::new(src.clone(), mount_config.dst.clone()));
                        mount_table.add_mount(mount_config.dst.clone(), vfs);
                    }
                    agentfs_sandbox::MountType::Sqlite { src } => {
                        eprintln!(
                            " - {} -> {} (sqlite)",
                            mount_config.dst.display(),
                            src.display()
                        );

                        // Create a SqliteVfs for this sqlite mount
                        let vfs = SqliteVfs::new(src, mount_config.dst.clone())
                            .await
                            .expect("Failed to create SQLite VFS");
                        mount_table.add_mount(mount_config.dst.clone(), Arc::new(vfs));
                    }
                }
            }
            eprintln!();

            init_mount_table(mount_table);
            init_fd_tables();
            init_strace(strace);

            let mut cmd = Command::new(command);
            for arg in args {
                cmd.arg(arg);
            }

            let tracer = TracerBuilder::<Sandbox>::new(cmd).spawn().await?;

            let (status, _) = tracer.wait().await?;
            status.raise_or_exit()
        }
    }
}

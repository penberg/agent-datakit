use super::file::FileOps;
use super::{Vfs, VfsError, VfsResult};
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use turso::{Builder, Connection, Value};

/// A SQLite-backed virtual filesystem
///
/// This implements a full POSIX-like filesystem stored in a SQLite database,
/// following the schema specification with inodes, directory entries, and chunked data.
#[derive(Clone)]
pub struct SqliteVfs {
    /// The connection to the SQLite database
    conn: Arc<Connection>,
    /// The virtual path as seen by the sandboxed process
    mount_point: PathBuf,
}

// Constants for file modes (Unix permission bits)
#[allow(dead_code)]
const S_IFMT: u32 = 0o170000; // File type mask
#[allow(dead_code)]
const S_IFREG: u32 = 0o100000; // Regular file
#[allow(dead_code)]
const S_IFDIR: u32 = 0o040000; // Directory
#[allow(dead_code)]
const S_IFLNK: u32 = 0o120000; // Symbolic link

const ROOT_INO: i64 = 1;

impl SqliteVfs {
    /// Create a new SQLite VFS
    ///
    /// # Arguments
    /// * `db_path` - Path to the SQLite database file
    /// * `mount_point` - The virtual path seen by the guest (e.g., "/agent")
    pub async fn new(db_path: impl AsRef<Path>, mount_point: PathBuf) -> VfsResult<Self> {
        let db_path_str = db_path
            .as_ref()
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid database path".to_string()))?;

        let db = Builder::new_local(db_path_str)
            .build()
            .await
            .map_err(|e| VfsError::Other(format!("Failed to build database: {}", e)))?;

        let conn = db
            .connect()
            .map_err(|e| VfsError::Other(format!("Failed to connect to database: {}", e)))?;

        let vfs = Self {
            conn: Arc::new(conn),
            mount_point,
        };

        vfs.initialize_schema().await?;
        Ok(vfs)
    }

    /// Initialize the database schema
    async fn initialize_schema(&self) -> VfsResult<()> {
        let conn = &self.conn;

        // Note: Foreign key enforcement is enabled by default in turso

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
        .map_err(|e| VfsError::Other(format!("Failed to create fs_inode table: {}", e)))?;

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
        .map_err(|e| VfsError::Other(format!("Failed to create fs_dentry table: {}", e)))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_fs_dentry_parent ON fs_dentry(parent_ino, name)",
            (),
        )
        .await
        .map_err(|e| VfsError::Other(format!("Failed to create index: {}", e)))?;

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
        .map_err(|e| VfsError::Other(format!("Failed to create fs_data table: {}", e)))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_fs_data_ino_offset ON fs_data(ino, offset)",
            (),
        )
        .await
        .map_err(|e| VfsError::Other(format!("Failed to create index: {}", e)))?;

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
        .map_err(|e| VfsError::Other(format!("Failed to create fs_symlink table: {}", e)))?;

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
        .map_err(|e| VfsError::Other(format!("Failed to create kv_store table: {}", e)))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_kv_store_created_at ON kv_store(created_at)",
            (),
        )
        .await
        .map_err(|e| VfsError::Other(format!("Failed to create index: {}", e)))?;

        // Create tool_calls table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS tool_calls (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                parameters TEXT,
                result TEXT,
                error TEXT,
                status TEXT NOT NULL CHECK (status IN ('pending', 'success', 'error')),
                started_at INTEGER NOT NULL,
                completed_at INTEGER,
                duration_ms INTEGER
            )",
            (),
        )
        .await
        .map_err(|e| VfsError::Other(format!("Failed to create tool_calls table: {}", e)))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tool_calls_name ON tool_calls(name)",
            (),
        )
        .await
        .map_err(|e| VfsError::Other(format!("Failed to create index: {}", e)))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tool_calls_started_at ON tool_calls(started_at)",
            (),
        )
        .await
        .map_err(|e| VfsError::Other(format!("Failed to create index: {}", e)))?;

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_tool_calls_status ON tool_calls(status)",
            (),
        )
        .await
        .map_err(|e| VfsError::Other(format!("Failed to create index: {}", e)))?;

        // Initialize root directory if it doesn't exist
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Check if root exists - use query to get count
        let mut rows = conn
            .query("SELECT COUNT(*) FROM fs_inode WHERE ino = ?", (ROOT_INO,))
            .await
            .map_err(|e| VfsError::Other(format!("Failed to check root existence: {}", e)))?;

        let root_count: i64 = if let Some(row) = rows.next().await.map_err(|e| VfsError::Other(format!("Failed to fetch row: {}", e)))? {
            row.get_value(0).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0)
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
            .map_err(|e| VfsError::Other(format!("Failed to create root inode: {}", e)))?;
        }

        Ok(())
    }

    /// Get the mount point path
    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }

    /// Helper: resolve a path to an inode number
    async fn resolve_path(&self, path: &Path) -> VfsResult<i64> {
        let path_str = path
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid path".to_string()))?;

        let mount_str = self
            .mount_point
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid mount point".to_string()))?;

        // Remove mount point prefix to get relative path
        let relative = if path_str == mount_str {
            ""
        } else if let Some(rel) = path_str.strip_prefix(&format!("{}/", mount_str)) {
            rel
        } else {
            return Err(VfsError::NotFound);
        };

        // Root directory
        if relative.is_empty() {
            return Ok(ROOT_INO);
        }

        // Walk the path components
        let components: Vec<&str> = relative.split('/').collect();
        let mut current_ino = ROOT_INO;

        for component in components {
            if component.is_empty() {
                continue;
            }

            // WORKAROUND: Limbo has a bug with TEXT column filtering in WHERE clauses
            // (e.g., "WHERE name = 'foo'" doesn't work)
            // Query by parent_ino only and filter manually in application code
            let query = format!("SELECT ino, name FROM fs_dentry WHERE parent_ino = {}", current_ino);
            let mut rows = self.conn.query(&query, ()).await
                .map_err(|e| VfsError::Other(format!("Failed to execute query: {}", e)))?;

            // Manually filter by name
            let mut found_ino: Option<i64> = None;
            while let Some(row) = rows.next().await.map_err(|e| VfsError::Other(format!("Failed to fetch row: {}", e)))? {
                let ino: i64 = row.get_value(0).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0);
                let name: String = row.get_value(1).ok().and_then(|v| {
                    if let turso::Value::Text(s) = v { Some(s.clone()) } else { None }
                }).unwrap_or_default();

                if name == component {
                    found_ino = Some(ino);
                    break;
                }
            }

            // Process the filtered result
            if let Some(ino) = found_ino {
                current_ino = ino;
            } else {
                return Err(VfsError::NotFound);
            }
        }

        Ok(current_ino)
    }

    /// Helper: create a new file at the given path
    async fn create_file(&self, path: &Path, mode: u32) -> VfsResult<i64> {
        // Get parent directory path and file name
        let parent_path = path.parent().ok_or_else(|| VfsError::NotFound)?;
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| VfsError::InvalidInput("Invalid file name".to_string()))?;

        // Resolve parent directory
        let parent_ino = self.resolve_path(parent_path).await?;

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        // Ensure mode includes file type bit (S_IFREG) for regular files
        let file_mode = if mode & S_IFMT == 0 {
            S_IFREG | mode
        } else {
            mode
        };

        // Create the inode
        self.conn
            .execute(
                "INSERT INTO fs_inode (mode, uid, gid, size, atime, mtime, ctime)
                 VALUES (?, 0, 0, 0, ?, ?, ?)",
                (file_mode, now, now, now),
            )
            .await
            .map_err(|e| VfsError::Other(format!("Failed to create inode: {}", e)))?;

        // Get the new inode number
        let mut rows = self
            .conn
            .query("SELECT last_insert_rowid()", ())
            .await
            .map_err(|e| VfsError::Other(format!("Failed to get inode: {}", e)))?;

        let ino: i64 = if let Some(row) = rows.next().await.map_err(|e| VfsError::Other(format!("Failed to fetch row: {}", e)))? {
            row.get_value(0)
                .ok()
                .and_then(|v| v.as_integer().copied())
                .ok_or_else(|| VfsError::Other("Failed to get inode number".to_string()))?
        } else {
            return Err(VfsError::Other("Failed to get inode number".to_string()));
        };

        // Create the directory entry
        self.conn
            .execute(
                "INSERT INTO fs_dentry (name, parent_ino, ino) VALUES (?, ?, ?)",
                (file_name, parent_ino, ino),
            )
            .await
            .map_err(|e| VfsError::Other(format!("Failed to create dentry: {}", e)))?;

        Ok(ino)
    }

    /// Open a file by path, creating it if needed
    ///
    /// This is called from the openat syscall handler to create a SqliteFile
    /// with the correct inode.
    pub async fn open_file(&self, path: &Path, flags: i32, mode: u32) -> VfsResult<super::file::BoxedFileOps> {
        // Try to resolve existing file or directory
        let ino = match self.resolve_path(path).await {
            Ok(ino) => ino,
            Err(VfsError::NotFound) => {
                // Create new file if O_CREAT is set
                if flags & libc::O_CREAT != 0 {
                    self.create_file(path, mode).await?
                } else {
                    return Err(VfsError::NotFound);
                }
            }
            Err(e) => return Err(e),
        };

        // Create the SqliteFile - it handles both files and directories
        Ok(Arc::new(SqliteFile::new(
            Arc::new(self.clone()),
            ino,
            flags,
        )))
    }
}

/// A file handle for a SQLite-backed file
pub struct SqliteFile {
    /// The SQLite VFS instance
    vfs: Arc<SqliteVfs>,
    /// The inode number for this file
    ino: i64,
    /// Current file offset for read/write operations
    offset: Arc<Mutex<i64>>,
    /// File descriptor flags
    flags: Mutex<i32>,
    /// Directory reading position (for getdents)
    dir_pos: Arc<Mutex<usize>>,
}

impl SqliteFile {
    /// Create a new SqliteFile
    pub fn new(vfs: Arc<SqliteVfs>, ino: i64, flags: i32) -> Self {
        Self {
            vfs,
            ino,
            offset: Arc::new(Mutex::new(0)),
            flags: Mutex::new(flags),
            dir_pos: Arc::new(Mutex::new(0)),
        }
    }
}

// Chunk size for data storage (64KB)
const CHUNK_SIZE: usize = 65536;

#[async_trait::async_trait]
impl FileOps for SqliteFile {
    async fn read(&self, buf: &mut [u8]) -> VfsResult<usize> {
            let offset = *self.offset.lock().unwrap();
            let conn = &self.vfs.conn;

            // Read data chunks that overlap with our read range
            let end_offset = offset + buf.len() as i64;

            // WORKAROUND: Limbo parameter binding is broken, use formatted query
            let query = format!(
                "SELECT offset, size, data FROM fs_data WHERE ino = {} AND offset < {} AND offset + size > {} ORDER BY offset",
                self.ino, end_offset, offset
            );

            let mut rows = conn
                .query(&query, ())
                .await
                .map_err(|e| VfsError::Other(format!("Failed to read data: {}", e)))?;

            let mut total_read = 0usize;

            while let Some(row) = rows.next().await.map_err(|e| VfsError::Other(format!("Failed to fetch row: {}", e)))? {
                let chunk_offset: i64 = row
                    .get_value(0)
                    .ok()
                    .and_then(|v| v.as_integer().copied())
                    .ok_or_else(|| VfsError::Other("Invalid chunk offset".to_string()))?;

                let _chunk_size: i64 = row
                    .get_value(1)
                    .ok()
                    .and_then(|v| v.as_integer().copied())
                    .ok_or_else(|| VfsError::Other("Invalid chunk size".to_string()))?;

                let chunk_data: Vec<u8> = row
                    .get_value(2)
                    .ok()
                    .and_then(|v| {
                        if let Value::Blob(b) = v {
                            Some(b.clone())
                        } else if let Value::Text(t) = v {
                            // WORKAROUND: Handle TEXT as well as BLOB for compatibility
                            Some(t.as_bytes().to_vec())
                        } else {
                            None
                        }
                    })
                    .ok_or_else(|| VfsError::Other("Invalid chunk data".to_string()))?;

                // Calculate overlap
                // IMPORTANT: Use actual chunk_data.len() instead of chunk_size from DB
                // because TEXT->bytes conversion may differ in length
                let actual_chunk_size = chunk_data.len() as i64;
                let chunk_start = chunk_offset;
                let chunk_end = chunk_offset + actual_chunk_size;
                let read_start = offset;
                let read_end = offset + buf.len() as i64;

                let overlap_start = std::cmp::max(chunk_start, read_start);
                let overlap_end = std::cmp::min(chunk_end, read_end);

                if overlap_start < overlap_end {
                    let src_offset = (overlap_start - chunk_start) as usize;
                    let dst_offset = (overlap_start - read_start) as usize;
                    let len = (overlap_end - overlap_start) as usize;

                    buf[dst_offset..dst_offset + len]
                        .copy_from_slice(&chunk_data[src_offset..src_offset + len]);

                    total_read = std::cmp::max(total_read, dst_offset + len);
                }
            }

            // Update offset
            *self.offset.lock().unwrap() += total_read as i64;

            Ok(total_read)
    }

    async fn write(&self, buf: &[u8]) -> VfsResult<usize> {
            let offset = *self.offset.lock().unwrap();
            let conn = &self.vfs.conn;

            // Write data in chunks
            let mut written = 0usize;
            while written < buf.len() {
                let chunk_offset = offset + written as i64;
                let chunk_size = std::cmp::min(CHUNK_SIZE, buf.len() - written);
                let chunk_data = &buf[written..written + chunk_size];

                // Delete existing chunk at this offset, then insert new one
                // (turso doesn't support INSERT OR REPLACE)
                conn.execute(
                    "DELETE FROM fs_data WHERE ino = ? AND offset = ?",
                    (self.ino, chunk_offset),
                )
                .await
                .map_err(|e| VfsError::Other(format!("Failed to delete old chunk: {}", e)))?;

                conn.execute(
                    "INSERT INTO fs_data (ino, offset, size, data)
                     VALUES (?, ?, ?, ?)",
                    (self.ino, chunk_offset, chunk_size as i64, chunk_data),
                )
                .await
                .map_err(|e| VfsError::Other(format!("Failed to write data: {}", e)))?;

                written += chunk_size;
            }

            // Update file size and mtime
            let new_size = offset + written as i64;
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;

            conn.execute(
                "UPDATE fs_inode SET size = MAX(size, ?), mtime = ? WHERE ino = ?",
                (new_size, now, self.ino),
            )
            .await
            .map_err(|e| VfsError::Other(format!("Failed to update inode: {}", e)))?;

            // Update offset
            *self.offset.lock().unwrap() += written as i64;

            Ok(written)
    }

    async fn seek(&self, offset: i64, whence: i32) -> VfsResult<i64> {
            let current = *self.offset.lock().unwrap();

            let new_offset = match whence {
                libc::SEEK_SET => offset,
                libc::SEEK_CUR => current + offset,
                libc::SEEK_END => {
                    // Get file size (drop mutex before await)
                    let mut rows = self.vfs.conn
                        .query("SELECT size FROM fs_inode WHERE ino = ?", (self.ino,))
                        .await
                        .map_err(|e| VfsError::Other(format!("Failed to get file size: {}", e)))?;

                    let size: i64 = if let Some(row) = rows.next().await.map_err(|e| VfsError::Other(format!("Failed to fetch row: {}", e)))? {
                        row.get_value(0)
                            .ok()
                            .and_then(|v| v.as_integer().copied())
                            .unwrap_or(0)
                    } else {
                        0
                    };

                    size + offset
                }
                _ => return Err(VfsError::InvalidInput("Invalid whence".to_string())),
            };

            if new_offset < 0 {
                return Err(VfsError::InvalidInput("Negative seek offset".to_string()));
            }

            *self.offset.lock().unwrap() = new_offset;
            Ok(new_offset)
    }

    async fn fstat(&self) -> VfsResult<libc::stat> {
            let mut rows = self.vfs.conn
                .query(
                    "SELECT mode, uid, gid, size, atime, mtime, ctime FROM fs_inode WHERE ino = ?",
                    (self.ino,),
                )
                .await
                .map_err(|e| VfsError::Other(format!("Failed to stat file: {}", e)))?;

            if let Some(row) = rows.next().await.map_err(|e| VfsError::Other(format!("Failed to fetch row: {}", e)))? {
                let mode = row.get_value(0).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) as u32;
                let uid = row.get_value(1).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) as u32;
                let gid = row.get_value(2).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) as u32;
                let size = row.get_value(3).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0);
                let atime = row.get_value(4).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0);
                let mtime = row.get_value(5).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0);
                let ctime = row.get_value(6).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0);

                // Use unsafe to create and initialize stat struct
                let mut stat: libc::stat = unsafe { std::mem::zeroed() };
                stat.st_dev = 0;
                stat.st_ino = self.ino as u64;
                stat.st_nlink = 1;
                stat.st_mode = mode;
                stat.st_uid = uid;
                stat.st_gid = gid;
                stat.st_rdev = 0;
                stat.st_size = size;
                stat.st_blksize = 4096;
                stat.st_blocks = (size + 511) / 512;
                stat.st_atime = atime;
                stat.st_atime_nsec = 0;
                stat.st_mtime = mtime;
                stat.st_mtime_nsec = 0;
                stat.st_ctime = ctime;
                stat.st_ctime_nsec = 0;

                Ok(stat)
        } else {
            Err(VfsError::NotFound)
        }
    }

    fn fsync(&self) -> VfsResult<()> {
        // SQLite handles synchronization automatically
        Ok(())
    }

    fn fdatasync(&self) -> VfsResult<()> {
        // SQLite handles synchronization automatically
        Ok(())
    }

    fn fcntl(&self, cmd: i32, arg: i64) -> VfsResult<i64> {
        match cmd {
            libc::F_GETFL => Ok(*self.flags.lock().unwrap() as i64),
            libc::F_SETFL => {
                *self.flags.lock().unwrap() = arg as i32;
                Ok(0)
            }
            _ => Err(VfsError::Other(format!("Unsupported fcntl command: {}", cmd))),
        }
    }

    fn ioctl(&self, _request: u64, _arg: u64) -> VfsResult<i64> {
        // Most ioctl operations are not supported on virtual files
        Err(VfsError::Other("ioctl not supported on SQLite VFS".to_string()))
    }

    fn as_raw_fd(&self) -> Option<RawFd> {
        // SQLite files don't have a kernel FD
        None
    }

    fn close(&self) -> VfsResult<()> {
        // No cleanup needed - SQLite handles everything
        Ok(())
    }

    fn get_flags(&self) -> i32 {
        *self.flags.lock().unwrap()
    }

    fn set_flags(&self, flags: i32) -> VfsResult<()> {
        *self.flags.lock().unwrap() = flags;
        Ok(())
    }

    async fn getdents(&self) -> VfsResult<Vec<(u64, String, u8)>> {
        // Check directory position
        let start_pos = *self.dir_pos.lock().unwrap();

        // If we've already returned all entries, return empty
        if start_pos > 0 {
            // Check if we need to fetch more entries
            // For now, we return all entries on first call, then empty on subsequent calls
            // This is a simplified implementation - a full implementation would paginate
            return Ok(Vec::new());
        }

        // Query directory entries for this inode (mutex dropped)
        let mut rows = self.vfs.conn
            .query(
                "SELECT d.ino, d.name, i.mode FROM fs_dentry d
                 JOIN fs_inode i ON d.ino = i.ino
                 WHERE d.parent_ino = ?
                 ORDER BY d.name",
                (self.ino,),
            )
            .await
            .map_err(|e| VfsError::Other(format!("Failed to read directory: {}", e)))?;

        let mut entries = Vec::new();

        // Add . and .. entries
        entries.push((self.ino as u64, ".".to_string(), libc::DT_DIR));
        entries.push((self.ino as u64, "..".to_string(), libc::DT_DIR)); // TODO: Get real parent ino

        while let Some(row) = rows.next().await.map_err(|e| VfsError::Other(format!("Failed to fetch row: {}", e)))? {
            let ino = row.get_value(0).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) as u64;
            let name = row.get_value(1).ok().and_then(|v| {
                if let turso::Value::Text(s) = v {
                    Some(s.clone())
                } else {
                    None
                }
            }).unwrap_or_default();
            let mode = row.get_value(2).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) as u32;

            // Determine file type from mode
            let d_type = match mode & S_IFMT {
                S_IFDIR => libc::DT_DIR,
                S_IFREG => libc::DT_REG,
                S_IFLNK => libc::DT_LNK,
                _ => libc::DT_UNKNOWN,
            };

            entries.push((ino, name, d_type));
        }

        // Mark that we've returned entries
        *self.dir_pos.lock().unwrap() = 1;

        Ok(entries)
    }
}

#[async_trait::async_trait]
impl Vfs for SqliteVfs {
    fn translate_path(&self, path: &Path) -> VfsResult<PathBuf> {
        // Check if the path is under our mount point
        let path_str = path
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid path".to_string()))?;

        let mount_str = self
            .mount_point
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid mount point".to_string()))?;

        // Check for exact match or prefix match
        if path_str == mount_str || path_str.starts_with(&format!("{}/", mount_str)) {
            // For SQLite VFS, we return a special marker path that signals
            // this should be handled by the VFS layer, not passed to the kernel
            Ok(PathBuf::from(format!("__sqlite_vfs__{}", path_str)))
        } else {
            Err(VfsError::NotFound)
        }
    }

    fn create_file_ops(&self, _kernel_fd: RawFd, flags: i32) -> super::file::BoxedFileOps {
        // Note: kernel_fd is ignored for SQLite VFS - we don't use kernel FDs
        // This method shouldn't be called for virtual VFS - use open() instead
        Arc::new(SqliteFile::new(
            Arc::new(self.clone()),
            0, // Placeholder - shouldn't be used
            flags,
        ))
    }

    fn is_virtual(&self) -> bool {
        true
    }

    async fn open(&self, path: &Path, flags: i32, mode: u32) -> super::VfsResult<super::file::BoxedFileOps> {
        self.open_file(path, flags, mode).await
    }

    async fn stat(&self, path: &Path) -> super::VfsResult<libc::stat> {
        // Resolve the path to an inode
        let ino = self.resolve_path(path).await?;

        // Query the inode metadata
        let mut rows = self.conn
            .query(
                "SELECT mode, uid, gid, size, atime, mtime, ctime FROM fs_inode WHERE ino = ?",
                (ino,),
            )
            .await
            .map_err(|e| super::VfsError::Other(format!("Failed to stat file: {}", e)))?;

        if let Some(row) = rows.next().await.map_err(|e| super::VfsError::Other(format!("Failed to fetch row: {}", e)))? {
            let mode = row.get_value(0).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) as u32;
            let uid = row.get_value(1).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) as u32;
            let gid = row.get_value(2).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0) as u32;
            let size = row.get_value(3).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0);
            let atime = row.get_value(4).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0);
            let mtime = row.get_value(5).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0);
            let ctime = row.get_value(6).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0);

            // Create stat struct
            let mut stat: libc::stat = unsafe { std::mem::zeroed() };
            stat.st_dev = 0;
            stat.st_ino = ino as u64;
            stat.st_nlink = 1;
            stat.st_mode = mode;
            stat.st_uid = uid;
            stat.st_gid = gid;
            stat.st_rdev = 0;
            stat.st_size = size;
            stat.st_blksize = 4096;
            stat.st_blocks = (size + 511) / 512;
            stat.st_atime = atime;
            stat.st_atime_nsec = 0;
            stat.st_mtime = mtime;
            stat.st_mtime_nsec = 0;
            stat.st_ctime = ctime;
            stat.st_ctime_nsec = 0;

            Ok(stat)
        } else {
            Err(super::VfsError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_initialize_schema() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let vfs = SqliteVfs::new(&db_path, PathBuf::from("/agent"))
            .await
            .unwrap();

        // Verify root directory exists
        let conn = &vfs.conn;
        let mut rows = conn
            .query("SELECT COUNT(*) FROM fs_inode WHERE ino = ?", (ROOT_INO,))
            .await
            .unwrap();

        let root_count: i64 = if let Some(row) = rows.next().await.unwrap() {
            row.get_value(0).ok().and_then(|v| v.as_integer().copied()).unwrap_or(0)
        } else {
            0
        };

        assert_eq!(root_count, 1);
    }

    #[tokio::test]
    async fn test_translate_path_match() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let vfs = SqliteVfs::new(&db_path, PathBuf::from("/agent"))
            .await
            .unwrap();

        let result = vfs.translate_path(Path::new("/agent/test.txt"));
        assert!(result.is_ok());
        assert!(result
            .unwrap()
            .to_string_lossy()
            .contains("__sqlite_vfs__"));
    }

    #[tokio::test]
    async fn test_translate_path_no_match() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let vfs = SqliteVfs::new(&db_path, PathBuf::from("/agent"))
            .await
            .unwrap();

        let result = vfs.translate_path(Path::new("/other/path"));
        assert!(result.is_err());
    }
}

use super::file::{BoxedFileOps, FileOps};
use super::{Vfs, VfsError, VfsResult};
use agentfs_sdk::Filesystem;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// A SQLite-backed virtual filesystem using the AgentFS SDK
///
/// This implements a full POSIX-like filesystem stored in a SQLite database,
/// using the agentfs-sdk Filesystem module.
#[derive(Clone)]
pub struct SqliteVfs {
    /// The filesystem from the SDK
    fs: Arc<Filesystem>,
    /// The virtual path as seen by the sandboxed process
    mount_point: PathBuf,
}

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

        let fs = Filesystem::new(db_path_str)
            .await
            .map_err(|e| VfsError::Other(format!("Failed to create filesystem: {}", e)))?;

        Ok(Self {
            fs: Arc::new(fs),
            mount_point,
        })
    }

    /// Get the mount point path
    pub fn mount_point(&self) -> &Path {
        &self.mount_point
    }

    /// Translate a sandbox path to a relative path for the SDK
    fn translate_to_relative(&self, path: &Path) -> VfsResult<String> {
        let path_str = path
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid path".to_string()))?;

        let mount_str = self
            .mount_point
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid mount point".to_string()))?;

        // Remove mount point prefix to get relative path
        let relative = if path_str == mount_str {
            "/"
        } else if let Some(rel) = path_str.strip_prefix(&format!("{}/", mount_str)) {
            &format!("/{}", rel)
        } else {
            return Err(VfsError::NotFound);
        };

        Ok(relative.to_string())
    }
}

#[async_trait::async_trait]
impl Vfs for SqliteVfs {
    fn translate_path(&self, path: &Path) -> VfsResult<PathBuf> {
        // For virtual VFS, we just validate the path is under our mount point
        let path_str = path
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid path".to_string()))?;

        let mount_str = self
            .mount_point
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid mount point".to_string()))?;

        if path_str.starts_with(mount_str) {
            Ok(path.to_path_buf())
        } else {
            Err(VfsError::NotFound)
        }
    }

    fn create_file_ops(&self, _kernel_fd: RawFd, _flags: i32) -> BoxedFileOps {
        // This should not be called for virtual VFS
        panic!("create_file_ops should not be called for virtual VFS");
    }

    fn is_virtual(&self) -> bool {
        true
    }

    async fn open(&self, path: &Path, _flags: i32, _mode: u32) -> VfsResult<BoxedFileOps> {
        let relative_path = self.translate_to_relative(path)?;

        // Read the file data using the SDK
        let data = self
            .fs
            .read_file(&relative_path)
            .await
            .map_err(|e| VfsError::Other(format!("Failed to read file: {}", e)))?
            .ok_or(VfsError::NotFound)?;

        // Create a file ops for the virtual file
        Ok(Arc::new(SqliteFileOps {
            fs: self.fs.clone(),
            path: relative_path,
            data: Arc::new(Mutex::new(data)),
            offset: Arc::new(Mutex::new(0)),
            flags: Mutex::new(_flags),
            dirty: Arc::new(Mutex::new(false)),
        }))
    }

    async fn stat(&self, path: &Path) -> VfsResult<libc::stat> {
        let relative_path = self.translate_to_relative(path)?;

        let stats = self
            .fs
            .stat(&relative_path)
            .await
            .map_err(|e| VfsError::Other(format!("Failed to stat: {}", e)))?
            .ok_or(VfsError::NotFound)?;

        // Use MaybeUninit to construct libc::stat safely
        let mut stat: std::mem::MaybeUninit<libc::stat> = std::mem::MaybeUninit::zeroed();
        unsafe {
            let stat_ptr = stat.as_mut_ptr();
            (*stat_ptr).st_dev = 0;
            (*stat_ptr).st_ino = stats.ino as u64;
            (*stat_ptr).st_nlink = stats.nlink as u64;
            (*stat_ptr).st_mode = stats.mode;
            (*stat_ptr).st_uid = stats.uid;
            (*stat_ptr).st_gid = stats.gid;
            (*stat_ptr).st_rdev = 0;
            (*stat_ptr).st_size = stats.size;
            (*stat_ptr).st_blksize = 4096;
            (*stat_ptr).st_blocks = (stats.size + 4095) / 4096;
            (*stat_ptr).st_atime = stats.atime;
            (*stat_ptr).st_atime_nsec = 0;
            (*stat_ptr).st_mtime = stats.mtime;
            (*stat_ptr).st_mtime_nsec = 0;
            (*stat_ptr).st_ctime = stats.ctime;
            (*stat_ptr).st_ctime_nsec = 0;
            Ok(stat.assume_init())
        }
    }
}

/// File operations for SQLite VFS files
struct SqliteFileOps {
    fs: Arc<Filesystem>,
    path: String,
    data: Arc<Mutex<Vec<u8>>>,
    offset: Arc<Mutex<i64>>,
    flags: Mutex<i32>,
    dirty: Arc<Mutex<bool>>,
}

#[async_trait::async_trait]
impl FileOps for SqliteFileOps {
    async fn read(&self, buf: &mut [u8]) -> VfsResult<usize> {
        let data = self.data.lock().unwrap();
        let mut offset = self.offset.lock().unwrap();

        let start = *offset as usize;
        if start >= data.len() {
            return Ok(0);
        }

        let end = std::cmp::min(start + buf.len(), data.len());
        let bytes_read = end - start;
        buf[..bytes_read].copy_from_slice(&data[start..end]);
        *offset += bytes_read as i64;

        Ok(bytes_read)
    }

    async fn write(&self, buf: &[u8]) -> VfsResult<usize> {
        let mut data = self.data.lock().unwrap();
        let mut offset = self.offset.lock().unwrap();

        let start = *offset as usize;

        // Extend the buffer if necessary
        if start + buf.len() > data.len() {
            data.resize(start + buf.len(), 0);
        }

        data[start..start + buf.len()].copy_from_slice(buf);
        *offset += buf.len() as i64;

        // Mark as dirty since we modified the data
        *self.dirty.lock().unwrap() = true;

        Ok(buf.len())
    }

    async fn seek(&self, offset: i64, whence: i32) -> VfsResult<i64> {
        let data = self.data.lock().unwrap();
        let mut current_offset = self.offset.lock().unwrap();

        let new_offset = match whence {
            libc::SEEK_SET => offset,
            libc::SEEK_CUR => *current_offset + offset,
            libc::SEEK_END => data.len() as i64 + offset,
            _ => return Err(VfsError::Other("Invalid whence".to_string())),
        };

        if new_offset < 0 {
            return Err(VfsError::Other("Invalid offset".to_string()));
        }

        *current_offset = new_offset;
        Ok(new_offset)
    }

    async fn fstat(&self) -> VfsResult<libc::stat> {
        let data = self.data.lock().unwrap();

        // Use MaybeUninit to construct libc::stat safely
        let mut stat: std::mem::MaybeUninit<libc::stat> = std::mem::MaybeUninit::zeroed();
        unsafe {
            let stat_ptr = stat.as_mut_ptr();
            (*stat_ptr).st_dev = 0;
            (*stat_ptr).st_ino = 0;
            (*stat_ptr).st_nlink = 1;
            (*stat_ptr).st_mode = libc::S_IFREG | 0o644;
            (*stat_ptr).st_uid = 0;
            (*stat_ptr).st_gid = 0;
            (*stat_ptr).st_rdev = 0;
            (*stat_ptr).st_size = data.len() as i64;
            (*stat_ptr).st_blksize = 4096;
            (*stat_ptr).st_blocks = (data.len() as i64 + 4095) / 4096;
            (*stat_ptr).st_atime = 0;
            (*stat_ptr).st_atime_nsec = 0;
            (*stat_ptr).st_mtime = 0;
            (*stat_ptr).st_mtime_nsec = 0;
            (*stat_ptr).st_ctime = 0;
            (*stat_ptr).st_ctime_nsec = 0;
            Ok(stat.assume_init())
        }
    }

    async fn fsync(&self) -> VfsResult<()> {
        // For virtual file, sync means write to database
        let dirty = *self.dirty.lock().unwrap();
        if !dirty {
            return Ok(());
        }

        let data = self.data.lock().unwrap().clone();

        // Write the data to the database
        self.fs
            .write_file(&self.path, &data)
            .await
            .map_err(|e| VfsError::Other(format!("Failed to write file: {}", e)))?;

        // Clear dirty flag after successful write
        *self.dirty.lock().unwrap() = false;

        Ok(())
    }

    async fn fdatasync(&self) -> VfsResult<()> {
        // For virtual file, same as fsync
        self.fsync().await
    }

    fn fcntl(&self, cmd: i32, arg: i64) -> VfsResult<i64> {
        match cmd {
            libc::F_GETFL => Ok(self.get_flags() as i64),
            libc::F_SETFL => {
                self.set_flags(arg as i32)?;
                Ok(0)
            }
            _ => Err(VfsError::Other(format!(
                "Unsupported fcntl command: {}",
                cmd
            ))),
        }
    }

    fn ioctl(&self, _request: u64, _arg: u64) -> VfsResult<i64> {
        // Virtual file doesn't support ioctl
        Err(VfsError::Other("ioctl not supported".to_string()))
    }

    fn as_raw_fd(&self) -> Option<RawFd> {
        // No real kernel FD for virtual files
        None
    }

    async fn close(&self) -> VfsResult<()> {
        // Ensure all data is written to the database before closing
        self.fsync().await
    }

    fn get_flags(&self) -> i32 {
        *self.flags.lock().unwrap()
    }

    fn set_flags(&self, flags: i32) -> VfsResult<()> {
        *self.flags.lock().unwrap() = flags;
        Ok(())
    }
}

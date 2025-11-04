pub mod fdtable;
pub mod file;
pub mod mount;
pub mod passthrough;
pub mod sqlite;

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::result::Result as StdResult;

/// VFS error type
#[derive(Debug)]
pub enum VfsError {
    NotFound,
    PermissionDenied,
    InvalidInput(String),
    IoError(std::io::Error),
    Other(String),
}

impl From<std::io::Error> for VfsError {
    fn from(err: std::io::Error) -> Self {
        VfsError::IoError(err)
    }
}

impl std::fmt::Display for VfsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VfsError::NotFound => write!(f, "Not found"),
            VfsError::PermissionDenied => write!(f, "Permission denied"),
            VfsError::InvalidInput(msg) => write!(f, "Invalid input: {}", msg),
            VfsError::IoError(err) => write!(f, "IO error: {}", err),
            VfsError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for VfsError {}

pub type VfsResult<T> = StdResult<T, VfsError>;

use file::BoxedFileOps;
use std::os::unix::io::RawFd;

/// Virtual file system trait.
///
/// This trait provides a Linux VFS-like interface for implementing
/// different filesystem backends.
#[async_trait]
pub trait Vfs: Send + Sync {
    /// Translate a sandbox path to the actual backend path
    ///
    /// This is the core operation for path-based VFS implementations.
    /// It maps a guest/sandbox path to the real path that should be used.
    fn translate_path(&self, path: &Path) -> VfsResult<PathBuf>;

    /// Open a file and return a FileOps implementation
    ///
    /// This creates the appropriate FileOps implementation for this VFS.
    /// The kernel_fd is the file descriptor returned by the kernel after opening.
    fn create_file_ops(&self, kernel_fd: RawFd, flags: i32) -> BoxedFileOps;

    /// Check if this VFS is purely virtual (no kernel file descriptors)
    ///
    /// Returns true if files are stored entirely in the VFS (like SQLite),
    /// false if they use kernel file descriptors (like passthrough).
    fn is_virtual(&self) -> bool {
        false
    }

    /// Open a file directly in the VFS (for virtual filesystems)
    ///
    /// This is only called for virtual VFS implementations. For passthrough
    /// VFS, the kernel opens the file and create_file_ops is called instead.
    async fn open(&self, _path: &Path, _flags: i32, _mode: u32) -> VfsResult<BoxedFileOps> {
        Err(VfsError::Other("open() not supported by this VFS".to_string()))
    }

    /// Get file status directly from the VFS (for virtual filesystems)
    ///
    /// This is only called for virtual VFS implementations. For passthrough
    /// VFS, the kernel handles stat operations.
    async fn stat(&self, _path: &Path) -> VfsResult<libc::stat> {
        Err(VfsError::Other("stat() not supported by this VFS".to_string()))
    }
}

/// A boxed VFS trait object for dynamic dispatch
pub type BoxedVfs = Box<dyn Vfs>;

use super::VfsResult;
use async_trait::async_trait;
use std::os::unix::io::RawFd;
use std::sync::Arc;

/// File operations trait for VFS implementations.
///
/// This trait provides a VFS-level abstraction over file operations,
/// allowing different implementations (passthrough, bind mount, SQLite VFS, etc.)
/// to handle file I/O differently.
#[async_trait]
pub trait FileOps: Send + Sync {
    /// Read from the file at the current offset
    async fn read(&self, buf: &mut [u8]) -> VfsResult<usize>;

    /// Write to the file at the current offset
    async fn write(&self, buf: &[u8]) -> VfsResult<usize>;

    /// Seek to a position in the file
    async fn seek(&self, offset: i64, whence: i32) -> VfsResult<i64>;

    /// Get file status
    async fn fstat(&self) -> VfsResult<libc::stat>;

    /// Sync file data to storage
    async fn fsync(&self) -> VfsResult<()>;

    /// Sync file data (but not metadata) to storage
    async fn fdatasync(&self) -> VfsResult<()>;

    /// Perform file control operations
    fn fcntl(&self, cmd: i32, arg: i64) -> VfsResult<i64>;

    /// Perform device-specific I/O operations
    fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64>;

    /// Get the underlying kernel file descriptor (if any)
    ///
    /// Returns None for virtualized files that don't have a real kernel FD.
    /// Some operations may need to fall back to the kernel FD.
    fn as_raw_fd(&self) -> Option<RawFd>;

    /// Close the file
    async fn close(&self) -> VfsResult<()>;

    /// Get flags associated with this file descriptor
    fn get_flags(&self) -> i32;

    /// Set flags associated with this file descriptor
    fn set_flags(&self, flags: i32) -> VfsResult<()>;

    /// Read directory entries (for directories only)
    ///
    /// This is used to implement getdents64. Returns a vector of (inode, name, type) tuples.
    /// Returns an error if this is not a directory.
    async fn getdents(&self) -> VfsResult<Vec<(u64, String, u8)>> {
        Err(super::VfsError::Other("Not a directory".to_string()))
    }
}

/// A boxed FileOps trait object for dynamic dispatch
pub type BoxedFileOps = Arc<dyn FileOps>;

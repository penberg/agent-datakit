use super::file::FileOps;
use super::{Vfs, VfsError, VfsResult};
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A passthrough VFS that maps a sandbox path to a host directory
///
/// This is essentially a bind mount implementation - it takes paths
/// under a sandbox prefix and redirects them to a host directory.
#[derive(Debug, Clone)]
pub struct PassthroughVfs {
    /// The real filesystem path on the host
    host_root: PathBuf,
    /// The virtual path as seen by the sandboxed process
    sandbox_root: PathBuf,
}

impl PassthroughVfs {
    /// Create a new passthrough VFS
    ///
    /// # Arguments
    /// * `host_root` - The real directory on the host filesystem
    /// * `sandbox_root` - The virtual path seen by the guest
    pub fn new(host_root: PathBuf, sandbox_root: PathBuf) -> Self {
        Self {
            host_root,
            sandbox_root,
        }
    }

    /// Get the host root path
    pub fn host_root(&self) -> &Path {
        &self.host_root
    }

    /// Get the sandbox root path
    pub fn sandbox_root(&self) -> &Path {
        &self.sandbox_root
    }
}

#[async_trait::async_trait]
impl Vfs for PassthroughVfs {
    fn translate_path(&self, path: &Path) -> VfsResult<PathBuf> {
        // Check if the path is under our sandbox root
        let sandbox_str = self
            .sandbox_root
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid sandbox path".to_string()))?;

        let path_str = path
            .to_str()
            .ok_or_else(|| VfsError::InvalidInput("Invalid path".to_string()))?;

        // Check for exact match or prefix match
        if path_str == sandbox_str || path_str.starts_with(&format!("{}/", sandbox_str)) {
            // Extract the relative part
            let relative = path_str
                .strip_prefix(sandbox_str)
                .unwrap_or("")
                .trim_start_matches('/');

            // Construct the host path
            let host_path = if relative.is_empty() {
                self.host_root.clone()
            } else {
                self.host_root.join(relative)
            };

            Ok(host_path)
        } else {
            Err(VfsError::NotFound)
        }
    }

    fn create_file_ops(&self, kernel_fd: RawFd, flags: i32) -> super::file::BoxedFileOps {
        use std::sync::Arc;
        Arc::new(PassthroughFile::new(kernel_fd, flags))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_path_exact_match() {
        let vfs = PassthroughVfs::new(PathBuf::from("/tmp/agent"), PathBuf::from("/agent"));

        let result = vfs.translate_path(Path::new("/agent")).unwrap();
        assert_eq!(result, PathBuf::from("/tmp/agent"));
    }

    #[test]
    fn test_translate_path_with_subpath() {
        let vfs = PassthroughVfs::new(PathBuf::from("/tmp/agent"), PathBuf::from("/agent"));

        let result = vfs
            .translate_path(Path::new("/agent/subdir/file.txt"))
            .unwrap();
        assert_eq!(result, PathBuf::from("/tmp/agent/subdir/file.txt"));
    }

    #[test]
    fn test_translate_path_no_match() {
        let vfs = PassthroughVfs::new(PathBuf::from("/tmp/agent"), PathBuf::from("/agent"));

        let result = vfs.translate_path(Path::new("/other/path"));
        assert!(result.is_err());
    }
}

/// A file implementation that passes through operations to a kernel file descriptor.
///
/// This is used for normal file operations where we simply forward to the actual
/// kernel FD without any virtualization.
pub struct PassthroughFile {
    /// The kernel file descriptor
    fd: RawFd,
    /// File descriptor flags (O_CLOEXEC, etc.)
    flags: Mutex<i32>,
}

impl PassthroughFile {
    /// Create a new passthrough file from a kernel file descriptor
    pub fn new(fd: RawFd, flags: i32) -> Self {
        Self {
            fd,
            flags: Mutex::new(flags),
        }
    }
}

#[async_trait::async_trait]
impl FileOps for PassthroughFile {
    async fn read(&self, buf: &mut [u8]) -> VfsResult<usize> {
        let result =
            unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if result < 0 {
            Err(VfsError::IoError(std::io::Error::last_os_error()))
        } else {
            Ok(result as usize)
        }
    }

    async fn write(&self, buf: &[u8]) -> VfsResult<usize> {
        let result =
            unsafe { libc::write(self.fd, buf.as_ptr() as *const libc::c_void, buf.len()) };
        if result < 0 {
            Err(VfsError::IoError(std::io::Error::last_os_error()))
        } else {
            Ok(result as usize)
        }
    }

    async fn seek(&self, offset: i64, whence: i32) -> VfsResult<i64> {
        let result = unsafe { libc::lseek(self.fd, offset, whence) };
        if result < 0 {
            Err(VfsError::IoError(std::io::Error::last_os_error()))
        } else {
            Ok(result)
        }
    }

    async fn fstat(&self) -> VfsResult<libc::stat> {
        let mut stat: std::mem::MaybeUninit<libc::stat> = std::mem::MaybeUninit::uninit();
        let result = unsafe { libc::fstat(self.fd, stat.as_mut_ptr()) };
        if result < 0 {
            Err(VfsError::IoError(std::io::Error::last_os_error()))
        } else {
            Ok(unsafe { stat.assume_init() })
        }
    }

    async fn fsync(&self) -> VfsResult<()> {
        let result = unsafe { libc::fsync(self.fd) };
        if result < 0 {
            Err(VfsError::IoError(std::io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }

    async fn fdatasync(&self) -> VfsResult<()> {
        let result = unsafe { libc::fdatasync(self.fd) };
        if result < 0 {
            Err(VfsError::IoError(std::io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }

    fn fcntl(&self, cmd: i32, arg: i64) -> VfsResult<i64> {
        let result = unsafe { libc::fcntl(self.fd, cmd, arg) };
        if result < 0 {
            Err(VfsError::IoError(std::io::Error::last_os_error()))
        } else {
            Ok(result as i64)
        }
    }

    fn ioctl(&self, request: u64, arg: u64) -> VfsResult<i64> {
        let result = unsafe { libc::ioctl(self.fd, request, arg) };
        if result < 0 {
            Err(VfsError::IoError(std::io::Error::last_os_error()))
        } else {
            Ok(result as i64)
        }
    }

    fn as_raw_fd(&self) -> Option<RawFd> {
        Some(self.fd)
    }

    async fn close(&self) -> VfsResult<()> {
        let result = unsafe { libc::close(self.fd) };
        if result < 0 {
            Err(VfsError::IoError(std::io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }

    fn get_flags(&self) -> i32 {
        *self.flags.lock().unwrap()
    }

    fn set_flags(&self, flags: i32) -> VfsResult<()> {
        *self.flags.lock().unwrap() = flags;
        Ok(())
    }
}

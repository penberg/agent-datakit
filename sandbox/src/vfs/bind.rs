use super::{Vfs, VfsError, VfsResult};
use std::path::{Path, PathBuf};

/// A bind mount VFS that maps a sandbox path to a host directory
///
/// This implements a simple bind mount - it translates paths under
/// a sandbox prefix to a host directory. After the refactoring in
/// commit b06115d, bind mounts no longer need FileOps implementations
/// because the syscall handlers directly use kernel FDs via
/// FdEntry::Passthrough.
#[derive(Debug, Clone)]
pub struct BindVfs {
    /// The real filesystem path on the host
    host_root: PathBuf,
    /// The virtual path as seen by the sandboxed process
    sandbox_root: PathBuf,
}

impl BindVfs {
    /// Create a new bind mount VFS
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
impl Vfs for BindVfs {
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

    fn is_virtual(&self) -> bool {
        // Bind mounts are not virtual - they use real kernel file descriptors
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_path_exact_match() {
        let vfs = BindVfs::new(PathBuf::from("/tmp/agent"), PathBuf::from("/agent"));

        let result = vfs.translate_path(Path::new("/agent")).unwrap();
        assert_eq!(result, PathBuf::from("/tmp/agent"));
    }

    #[test]
    fn test_translate_path_with_subpath() {
        let vfs = BindVfs::new(PathBuf::from("/tmp/agent"), PathBuf::from("/agent"));

        let result = vfs
            .translate_path(Path::new("/agent/subdir/file.txt"))
            .unwrap();
        assert_eq!(result, PathBuf::from("/tmp/agent/subdir/file.txt"));
    }

    #[test]
    fn test_translate_path_no_match() {
        let vfs = BindVfs::new(PathBuf::from("/tmp/agent"), PathBuf::from("/agent"));

        let result = vfs.translate_path(Path::new("/other/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_translate_path_partial_match() {
        let vfs = BindVfs::new(PathBuf::from("/tmp/agent"), PathBuf::from("/agent"));

        // /agentfoo should not match /agent
        let result = vfs.translate_path(Path::new("/agentfoo"));
        assert!(result.is_err());
    }

    #[test]
    fn test_is_not_virtual() {
        let vfs = BindVfs::new(PathBuf::from("/tmp/agent"), PathBuf::from("/agent"));
        assert!(!vfs.is_virtual());
    }
}

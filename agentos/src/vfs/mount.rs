use super::Vfs;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Reverse,
    path::{Path, PathBuf},
    sync::Arc,
};

/// A mount point entry in the mount table
#[derive(Clone)]
pub struct MountPoint {
    /// The virtual path as seen by the sandboxed process
    pub sandbox_path: PathBuf,
    /// The VFS implementation for this mount point
    pub vfs: Arc<dyn Vfs>,
}

/// Mount table manages multiple VFS mount points
///
/// This is similar to Linux's VFS mount table - it tracks multiple
/// mounted filesystems and resolves paths to the appropriate VFS
/// implementation using longest-prefix matching.
#[derive(Clone)]
pub struct MountTable {
    mounts: Vec<MountPoint>,
}

impl MountTable {
    /// Create a new empty mount table
    pub fn new() -> Self {
        Self { mounts: Vec::new() }
    }

    /// Add a new mount point
    ///
    /// Mount points are automatically sorted by path depth (longest first)
    /// to ensure longest-prefix matching works correctly.
    pub fn add_mount(&mut self, sandbox_path: PathBuf, vfs: Arc<dyn Vfs>) {
        self.mounts.push(MountPoint { sandbox_path, vfs });
        // Sort by path depth (deepest first) to implement longest-prefix matching
        self.mounts
            .sort_by_key(|m| Reverse(m.sandbox_path.components().count()));
    }

    /// Resolve a path to a VFS and translated path
    ///
    /// This implements longest-prefix matching - if multiple mount points
    /// could match, the one with the longest matching prefix is chosen.
    ///
    /// Returns None if no mount point matches the path.
    pub fn resolve(&self, path: &Path) -> Option<(Arc<dyn Vfs>, PathBuf)> {
        for mount in &self.mounts {
            // Try to translate the path using this mount's VFS
            if let Ok(translated) = mount.vfs.translate_path(path) {
                return Some((mount.vfs.clone(), translated));
            }
        }
        None
    }

    /// Get all mount points
    pub fn mounts(&self) -> &[MountPoint] {
        &self.mounts
    }

    /// Create a FileOps instance for a given path and kernel FD
    ///
    /// This resolves the path to the appropriate VFS and creates the
    /// corresponding FileOps implementation. Returns None if no mount
    /// point matches the path (indicating a regular file outside any mount).
    pub fn create_file_ops(
        &self,
        path: &Path,
        kernel_fd: std::os::unix::io::RawFd,
        flags: i32,
    ) -> Option<super::file::BoxedFileOps> {
        // Try to find a matching VFS for this path
        for mount in &self.mounts {
            if let Ok(_) = mount.vfs.translate_path(path) {
                return Some(mount.vfs.create_file_ops(kernel_fd, flags));
            }
        }
        // No mount point matched - return None to indicate passthrough
        None
    }
}

impl Default for MountTable {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for MountTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MountTable")
            .field("mount_count", &self.mounts.len())
            .finish()
    }
}

/// Type of VFS mount supported by the sandbox.
///
/// This enum defines the different ways to make host resources available
/// to sandboxed processes, similar to mount types in traditional Unix systems.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MountType {
    /// Bind mount that passes through to a host path.
    ///
    /// Similar to Linux bind mounts, this makes a host directory tree
    /// available at a path within the sandbox. The source path is
    /// canonicalized (symlinks are resolved) during parsing.
    Bind {
        /// Source path on the host (canonicalized).
        src: PathBuf,
    },
    /// SQLite-backed virtual filesystem.
    ///
    /// This mount type creates a full POSIX-like filesystem stored in a
    /// SQLite database, providing persistence and atomic operations.
    Sqlite {
        /// Path to the SQLite database file.
        src: PathBuf,
    },
}

/// Configuration for a mount point (used for CLI parsing).
///
/// Mount specifications follow Docker-style syntax with key=value pairs:
/// `type=bind,src=/host/path,dst=/sandbox/path`
///
/// Aliases are supported: `source` for `src`, `target` for `dst`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountConfig {
    /// Type of mount.
    pub mount_type: MountType,
    /// Destination path in the sandbox (must be absolute).
    pub dst: PathBuf,
}

impl std::str::FromStr for MountConfig {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use std::collections::HashMap;

        // Parse key=value pairs separated by commas
        let mut options: HashMap<String, String> = HashMap::new();

        for part in s.split(',') {
            let kv: Vec<&str> = part.splitn(2, '=').collect();
            if kv.len() != 2 {
                return Err(format!(
                    "Invalid mount option '{}'. Expected format: key=value.",
                    part
                ));
            }
            // Check for duplicate keys
            if options
                .insert(kv[0].to_string(), kv[1].to_string())
                .is_some()
            {
                return Err(format!("Duplicate key '{}' in mount specification.", kv[0]));
            }
        }

        // Check for required 'type' field
        let mount_type = options.get("type").ok_or_else(|| {
            "Missing required field 'type'. Example: type=bind,src=/host/path,dst=/sandbox/path."
                .to_string()
        })?;

        match mount_type.as_str() {
            "bind" => {
                // Get src (or source as alias)
                let src_str = options.get("src")
                    .or_else(|| options.get("source"))
                    .ok_or_else(|| {
                        "Bind mount requires 'src' field. Example: type=bind,src=/host/path,dst=/sandbox/path.".to_string()
                    })?;

                // Get dst (or target as alias)
                let dst_str = options.get("dst")
                    .or_else(|| options.get("target"))
                    .ok_or_else(|| {
                        "Bind mount requires 'dst' field. Example: type=bind,src=/host/path,dst=/sandbox/path.".to_string()
                    })?;

                // Validate destination is absolute
                let dst = PathBuf::from(dst_str);
                if !dst.is_absolute() {
                    return Err(format!("Destination path '{}' must be absolute.", dst_str));
                }

                // Canonicalize the source path
                let src = std::fs::canonicalize(src_str).map_err(|e| {
                    format!("Failed to canonicalize source path '{}': {}.", src_str, e)
                })?;

                Ok(MountConfig {
                    mount_type: MountType::Bind { src },
                    dst,
                })
            }
            "sqlite" => {
                // Get src (or source as alias)
                let src_str = options.get("src")
                    .or_else(|| options.get("source"))
                    .ok_or_else(|| {
                        "SQLite mount requires 'src' field. Example: type=sqlite,src=agent.db,dst=/agent.".to_string()
                    })?;

                // Get dst (or target as alias)
                let dst_str = options.get("dst")
                    .or_else(|| options.get("target"))
                    .ok_or_else(|| {
                        "SQLite mount requires 'dst' field. Example: type=sqlite,src=agent.db,dst=/agent.".to_string()
                    })?;

                // Validate destination is absolute
                let dst = PathBuf::from(dst_str);
                if !dst.is_absolute() {
                    return Err(format!("Destination path '{}' must be absolute.", dst_str));
                }

                // For SQLite, we use the path as-is (may be relative or absolute)
                let src = PathBuf::from(src_str);

                Ok(MountConfig {
                    mount_type: MountType::Sqlite { src },
                    dst,
                })
            }
            _ => Err(format!(
                "Unsupported mount type '{}'. Supported types: bind, sqlite.",
                mount_type
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vfs::passthrough::PassthroughVfs;

    #[test]
    fn test_mount_table_longest_prefix() {
        let mut table = MountTable::new();

        // Add two overlapping mount points
        table.add_mount(
            PathBuf::from("/agent"),
            Arc::new(PassthroughVfs::new(
                PathBuf::from("/tmp/agent"),
                PathBuf::from("/agent"),
            )),
        );

        table.add_mount(
            PathBuf::from("/agent/special"),
            Arc::new(PassthroughVfs::new(
                PathBuf::from("/tmp/special"),
                PathBuf::from("/agent/special"),
            )),
        );

        // Path /agent/special/file should match the more specific mount
        let result = table.resolve(Path::new("/agent/special/file"));
        assert!(result.is_some());

        let (_, translated) = result.unwrap();
        assert_eq!(translated, PathBuf::from("/tmp/special/file"));

        // Path /agent/normal should match the less specific mount
        let result = table.resolve(Path::new("/agent/normal"));
        assert!(result.is_some());

        let (_, translated) = result.unwrap();
        assert_eq!(translated, PathBuf::from("/tmp/agent/normal"));
    }

    #[test]
    fn test_mount_table_no_match() {
        let mut table = MountTable::new();

        table.add_mount(
            PathBuf::from("/agent"),
            Arc::new(PassthroughVfs::new(
                PathBuf::from("/tmp/agent"),
                PathBuf::from("/agent"),
            )),
        );

        let result = table.resolve(Path::new("/other/path"));
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_bind_mount() {
        // Use /tmp which should exist on all systems
        let config: Result<MountConfig, _> = "type=bind,src=/tmp,dst=/data".parse();
        assert!(config.is_ok());

        let config = config.unwrap();
        match config.mount_type {
            MountType::Bind { src } => {
                assert_eq!(src, std::fs::canonicalize("/tmp").unwrap());
                assert_eq!(config.dst, PathBuf::from("/data"));
            }
        }
    }

    #[test]
    fn test_parse_bind_mount_with_aliases() {
        // Test using 'source' and 'target' aliases
        let config: Result<MountConfig, _> = "type=bind,source=/tmp,target=/data".parse();
        assert!(config.is_ok());

        let config = config.unwrap();
        match config.mount_type {
            MountType::Bind { src } => {
                assert_eq!(src, std::fs::canonicalize("/tmp").unwrap());
                assert_eq!(config.dst, PathBuf::from("/data"));
            }
        }
    }

    #[test]
    fn test_missing_type() {
        let config: Result<MountConfig, _> = "src=/tmp,dst=/data".parse();
        assert!(config.is_err());
        assert!(config
            .unwrap_err()
            .contains("Missing required field 'type'"));
    }

    #[test]
    fn test_missing_dst() {
        let config: Result<MountConfig, _> = "type=bind,src=/tmp".parse();
        assert!(config.is_err());
        assert!(config.unwrap_err().contains("requires 'dst' field"));
    }

    #[test]
    fn test_bind_missing_src() {
        let config: Result<MountConfig, _> = "type=bind,dst=/data".parse();
        assert!(config.is_err());
        assert!(config.unwrap_err().contains("requires 'src' field"));
    }

    #[test]
    fn test_invalid_type() {
        let config: Result<MountConfig, _> = "type=foobar,dst=/data".parse();
        assert!(config.is_err());
        assert!(config.unwrap_err().contains("Unsupported mount type"));
    }

    #[test]
    fn test_invalid_key_value_format() {
        let config: Result<MountConfig, _> = "type=bind,invalid,dst=/data".parse();
        assert!(config.is_err());
        assert!(config.unwrap_err().contains("Invalid mount option"));
    }

    #[test]
    fn test_duplicate_keys() {
        let config: Result<MountConfig, _> = "type=bind,src=/tmp,src=/var,dst=/data".parse();
        assert!(config.is_err());
        assert!(config.unwrap_err().contains("Duplicate key 'src'"));
    }

    #[test]
    fn test_relative_destination() {
        let config: Result<MountConfig, _> = "type=bind,src=/tmp,dst=relative/path".parse();
        assert!(config.is_err());
        assert!(config.unwrap_err().contains("must be absolute"));
    }

    #[test]
    fn test_nonexistent_source() {
        let config: Result<MountConfig, _> =
            "type=bind,src=/nonexistent-path-12345,dst=/data".parse();
        assert!(config.is_err());
        assert!(config.unwrap_err().contains("Failed to canonicalize"));
    }
}

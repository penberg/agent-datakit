use super::file::BoxedFileOps;
use std::collections::{BinaryHeap, HashMap};
use std::sync::{Arc, Mutex};

/// Standard file descriptor constants
const STDIN_FILENO: i32 = 0;
const STDOUT_FILENO: i32 = 1;
const STDERR_FILENO: i32 = 2;
const FIRST_USER_FD: i32 = 3;

/// Information about a virtualized file descriptor
#[derive(Clone)]
pub struct FdEntry {
    /// The file operations implementation for this FD
    pub file_ops: BoxedFileOps,
    /// Flags associated with this FD (O_CLOEXEC, etc.)
    pub flags: i32,
}

impl FdEntry {
    /// Get the kernel file descriptor if this is a passthrough file
    pub fn kernel_fd(&self) -> Option<i32> {
        self.file_ops.as_raw_fd()
    }
}

/// Inner state of the FD table, protected by a single mutex
struct FdTableInner {
    /// Mapping from virtual FD to kernel FD
    entries: HashMap<i32, FdEntry>,
    /// Next virtual FD to allocate (monotonically increasing)
    next_vfd: i32,
    /// Min-heap of freed FDs available for reuse (stored as negative for min-heap behavior)
    free_fds: BinaryHeap<std::cmp::Reverse<i32>>,
}

/// Per-process file descriptor table that virtualizes file descriptors
///
/// This table maintains a mapping from virtual (process-visible) file descriptors
/// to kernel (actual) file descriptors. It is thread-safe and can be shared across
/// threads within the same process.
///
/// Note: Clone creates a shallow copy that shares the same underlying FD table.
/// For fork/clone syscalls, use `deep_clone()` instead.
#[derive(Clone)]
pub struct FdTable {
    inner: Arc<Mutex<FdTableInner>>,
}

impl FdTable {
    /// Create a new FD table with standard FDs (stdin, stdout, stderr)
    pub fn new() -> Self {
        use super::passthrough::PassthroughFile;

        let mut entries = HashMap::new();

        // Initialize standard file descriptors (0, 1, 2) as passthrough files
        entries.insert(
            STDIN_FILENO,
            FdEntry {
                file_ops: Arc::new(PassthroughFile::new(STDIN_FILENO, 0)),
                flags: 0,
            },
        );
        entries.insert(
            STDOUT_FILENO,
            FdEntry {
                file_ops: Arc::new(PassthroughFile::new(STDOUT_FILENO, 0)),
                flags: 0,
            },
        );
        entries.insert(
            STDERR_FILENO,
            FdEntry {
                file_ops: Arc::new(PassthroughFile::new(STDERR_FILENO, 0)),
                flags: 0,
            },
        );

        Self {
            inner: Arc::new(Mutex::new(FdTableInner {
                entries,
                next_vfd: FIRST_USER_FD,
                free_fds: BinaryHeap::new(),
            })),
        }
    }

    /// Create a deep clone of this FD table (for fork/clone syscalls)
    ///
    /// This creates a completely independent copy of the FD table,
    /// unlike the default Clone which shares the underlying table.
    pub fn deep_clone(&self) -> Self {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        Self {
            inner: Arc::new(Mutex::new(FdTableInner {
                entries: inner.entries.clone(),
                next_vfd: inner.next_vfd,
                free_fds: inner.free_fds.clone(),
            })),
        }
    }

    /// Allocate a new virtual FD for the given file operations
    ///
    /// This uses the lowest available FD number, as required by POSIX.
    pub fn allocate(&self, file_ops: BoxedFileOps, flags: i32) -> i32 {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Try to reuse a freed FD first (POSIX requires lowest available FD)
        let vfd = if let Some(std::cmp::Reverse(free_fd)) = inner.free_fds.pop() {
            free_fd
        } else {
            // No free FDs, allocate a new one
            let vfd = inner.next_vfd;
            if vfd == i32::MAX {
                // FD exhaustion - search for gaps in allocated FDs
                // This is a rare edge case
                (FIRST_USER_FD..i32::MAX)
                    .find(|fd| !inner.entries.contains_key(fd))
                    .expect("File descriptor table exhausted")
            } else {
                inner.next_vfd += 1;
                vfd
            }
        };

        inner.entries.insert(vfd, FdEntry { file_ops, flags });
        vfd
    }

    /// Allocate a new virtual FD at or above the specified minimum
    ///
    /// This is used for fcntl F_DUPFD and F_DUPFD_CLOEXEC commands.
    pub fn allocate_min(&self, min_vfd: i32, file_ops: BoxedFileOps, flags: i32) -> i32 {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Find the lowest available FD >= min_vfd
        let vfd = (min_vfd..i32::MAX)
            .find(|fd| !inner.entries.contains_key(fd))
            .expect("File descriptor table exhausted");

        // Update next_vfd if we allocated beyond it
        if vfd >= inner.next_vfd {
            inner.next_vfd = vfd + 1;
        }

        // Remove from free list if it was there
        inner.free_fds = inner
            .free_fds
            .clone()
            .into_iter()
            .filter(|&std::cmp::Reverse(fd)| fd != vfd)
            .collect();

        inner.entries.insert(vfd, FdEntry { file_ops, flags });
        vfd
    }

    /// Allocate a specific virtual FD (used for dup2)
    ///
    /// Returns the old FdEntry if the VFD was already allocated, which the caller
    /// should close if needed.
    pub fn allocate_at(&self, vfd: i32, file_ops: BoxedFileOps, flags: i32) -> Option<FdEntry> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        // Remove the FD from free list if it's there
        // (This is inefficient but dup2 to freed FDs is rare)
        inner.free_fds = inner
            .free_fds
            .clone()
            .into_iter()
            .filter(|&std::cmp::Reverse(fd)| fd != vfd)
            .collect();

        // Update next_vfd if necessary
        if vfd >= inner.next_vfd {
            inner.next_vfd = vfd + 1;
        }

        // Insert the new entry and return the old one if it existed
        inner.entries.insert(vfd, FdEntry { file_ops, flags })
    }

    /// Translate a virtual FD to a kernel FD
    ///
    /// Returns the kernel FD if this is a passthrough file, or None if it's a
    /// virtualized file or the VFD doesn't exist.
    pub fn translate(&self, vfd: i32) -> Option<i32> {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.entries.get(&vfd).and_then(|entry| entry.kernel_fd())
    }

    /// Get the full entry for a virtual FD
    pub fn get(&self, vfd: i32) -> Option<FdEntry> {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        inner.entries.get(&vfd).cloned()
    }

    /// Deallocate a virtual FD and mark it as available for reuse
    pub fn deallocate(&self, vfd: i32) -> Option<FdEntry> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let entry = inner.entries.remove(&vfd)?;

        // Add to free list for reuse (unless it's a standard FD)
        if vfd >= FIRST_USER_FD {
            inner.free_fds.push(std::cmp::Reverse(vfd));
        }

        Some(entry)
    }

    /// Duplicate a virtual FD (for dup syscall)
    pub fn duplicate(&self, old_vfd: i32) -> Option<i32> {
        let entry = self.get(old_vfd)?;
        // Allocate a new virtual FD pointing to the same file operations
        Some(self.allocate(entry.file_ops.clone(), entry.flags))
    }

    /// Duplicate a virtual FD to a specific new FD (for dup2 syscall)
    ///
    /// Returns the old entry that was at new_vfd if it existed (caller should close it)
    pub fn duplicate_at(&self, old_vfd: i32, new_vfd: i32) -> Option<FdEntry> {
        let entry = self.get(old_vfd)?;
        self.allocate_at(new_vfd, entry.file_ops.clone(), entry.flags)
    }
}

impl Default for FdTable {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for FdTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let inner = self.inner.lock().unwrap();
        f.debug_struct("FdTable")
            .field("entry_count", &inner.entries.len())
            .field("next_vfd", &inner.next_vfd)
            .field("free_fds_count", &inner.free_fds.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_fds() {
        let table = FdTable::new();

        assert_eq!(table.translate(0), Some(0)); // stdin
        assert_eq!(table.translate(1), Some(1)); // stdout
        assert_eq!(table.translate(2), Some(2)); // stderr
    }

    #[test]
    fn test_allocate() {
        use super::super::passthrough::PassthroughFile;

        let table = FdTable::new();

        let vfd1 = table.allocate(Arc::new(PassthroughFile::new(100, 0)), 0);
        assert_eq!(vfd1, 3); // First non-standard FD
        assert_eq!(table.translate(3), Some(100));

        let vfd2 = table.allocate(Arc::new(PassthroughFile::new(101, 0)), 0);
        assert_eq!(vfd2, 4);
        assert_eq!(table.translate(4), Some(101));
    }

    #[test]
    fn test_deallocate() {
        use super::super::passthrough::PassthroughFile;

        let table = FdTable::new();

        let vfd = table.allocate(Arc::new(PassthroughFile::new(100, 0)), 0);
        assert_eq!(table.translate(vfd), Some(100));

        let entry = table.deallocate(vfd);
        assert!(entry.is_some());
        assert_eq!(entry.unwrap().kernel_fd(), Some(100));

        assert_eq!(table.translate(vfd), None);
    }

    #[test]
    fn test_duplicate() {
        use super::super::passthrough::PassthroughFile;

        let table = FdTable::new();

        let vfd1 = table.allocate(Arc::new(PassthroughFile::new(100, 0)), 0);
        let vfd2 = table.duplicate(vfd1).unwrap();

        assert_ne!(vfd1, vfd2);
        assert_eq!(table.translate(vfd1), Some(100));
        assert_eq!(table.translate(vfd2), Some(100));
    }

    #[test]
    fn test_duplicate_at() {
        use super::super::passthrough::PassthroughFile;

        let table = FdTable::new();

        let vfd1 = table.allocate(Arc::new(PassthroughFile::new(100, 0)), 0);
        let result = table.duplicate_at(vfd1, 10);

        // duplicate_at returns the old FdEntry that was at new_vfd (if any)
        // In this case, there was no previous entry at fd 10, so it returns None
        assert!(result.is_none());
        assert_eq!(table.translate(10), Some(100));
    }
}

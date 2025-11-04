use crate::{
    sandbox::Sandbox,
    syscall::translate_path,
    vfs::{fdtable::FdTable, mount::MountTable},
};
use reverie::{
    syscalls::{MemoryAccess, ReadAddr, Syscall},
    Error, Guest,
};

/// The `statx` system call.
///
/// This intercepts `statx` system calls and translates paths according to the mount table
/// and virtualizes the dirfd.
/// Returns `Some(result)` if the syscall was handled and the result should be returned directly,
/// or `None` if the original syscall should be used.
pub async fn handle_statx<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Statx,
    mount_table: &MountTable,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let dirfd = args.dirfd();
    // AT_FDCWD is -100
    let kernel_dirfd = if dirfd == -100 {
        dirfd
    } else {
        fd_table.translate(dirfd).unwrap_or(dirfd)
    };

    if let Some(path_addr) = args.path() {
        // Read the original path from guest memory
        let path: std::path::PathBuf = path_addr.read(&guest.memory())?;

        // Check if this path matches a mount point
        if let Some((vfs, _translated_path)) = mount_table.resolve(&path) {
            // Check if this is a virtual VFS (like SQLite)
            if vfs.is_virtual() {
                // For virtual VFS, statx is not supported - return ENOSYS
                // The caller will fall back to newfstatat
                return Ok(Some(-libc::ENOSYS as i64));
            }
        }

        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Statx::new()
                .with_dirfd(kernel_dirfd)
                .with_path(Some(new_path_addr))
                .with_flags(args.flags())
                .with_mask(args.mask())
                .with_statx(args.statx());

            let result = guest.inject(Syscall::Statx(new_syscall)).await?;
            return Ok(Some(result));
        }
    }
    Ok(None)
}

/// The `newfstatat` system call.
///
/// This intercepts `newfstatat` system calls and translates paths according to the mount table
/// and virtualizes the dirfd.
/// Returns `Some(result)` if the syscall was handled and the result should be returned directly,
/// or `None` if the original syscall should be used.
pub async fn handle_newfstatat<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Newfstatat,
    mount_table: &MountTable,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let dirfd = args.dirfd();
    // AT_FDCWD is -100
    let kernel_dirfd = if dirfd == -100 {
        dirfd
    } else {
        fd_table.translate(dirfd).unwrap_or(dirfd)
    };

    if let Some(path_addr) = args.path() {
        // Read the original path from guest memory
        let path: std::path::PathBuf = path_addr.read(&guest.memory())?;

        // Check if this path matches a mount point
        if let Some((vfs, _translated_path)) = mount_table.resolve(&path) {
            // Check if this is a virtual VFS (like SQLite)
            if vfs.is_virtual() {
                // For virtual VFS, call vfs.stat() directly
                match vfs.stat(&path).await {
                    Ok(stat_buf) => {
                        // Write the stat result to guest memory
                        if let Some(stat_addr) = args.stat() {
                            // Convert stat struct to bytes and write
                            let stat_bytes: &[u8] = unsafe {
                                std::slice::from_raw_parts(
                                    &stat_buf as *const _ as *const u8,
                                    std::mem::size_of::<libc::stat>(),
                                )
                            };
                            guest
                                .memory()
                                .write_exact(stat_addr.0.cast::<u8>(), stat_bytes)?;
                        }
                        return Ok(Some(0)); // Success
                    }
                    Err(e) => {
                        // Map VFS errors to errno
                        let errno = match e {
                            crate::vfs::VfsError::NotFound => -libc::ENOENT as i64,
                            crate::vfs::VfsError::PermissionDenied => -libc::EACCES as i64,
                            _ => -libc::EIO as i64,
                        };
                        return Ok(Some(errno));
                    }
                }
            }
        }

        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Newfstatat::new()
                .with_dirfd(kernel_dirfd)
                .with_path(Some(new_path_addr))
                .with_stat(args.stat())
                .with_flags(args.flags());

            let result = guest.inject(Syscall::Newfstatat(new_syscall)).await?;
            return Ok(Some(result));
        }
    }
    Ok(None)
}

/// The `statfs` system call.
///
/// This intercepts `statfs` system calls and translates paths according to the mount table.
pub async fn handle_statfs<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Statfs,
    mount_table: &MountTable,
) -> Result<Option<Syscall>, Error> {
    if let Some(path_addr) = args.path() {
        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Statfs::new()
                .with_path(Some(new_path_addr))
                .with_buf(args.buf());

            return Ok(Some(Syscall::Statfs(new_syscall)));
        }
    }
    Ok(None)
}

/// The `readlink` system call.
///
/// This intercepts `readlink` system calls and translates paths according to the mount table.
pub async fn handle_readlink<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Readlink,
    mount_table: &MountTable,
) -> Result<Option<Syscall>, Error> {
    if let Some(path_addr) = args.path() {
        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Readlink::new()
                .with_path(Some(new_path_addr))
                .with_buf(args.buf())
                .with_bufsize(args.bufsize());

            return Ok(Some(Syscall::Readlink(new_syscall)));
        }
    }
    Ok(None)
}

/// The `readlinkat` system call.
///
/// This intercepts `readlinkat` system calls and translates paths according to the mount table
/// and virtualizes the dirfd.
pub async fn handle_readlinkat<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Readlinkat,
    mount_table: &MountTable,
    fd_table: &FdTable,
) -> Result<Option<Syscall>, Error> {
    let dirfd = args.dirfd();
    // AT_FDCWD is -100
    let kernel_dirfd = if dirfd == -100 {
        dirfd
    } else {
        fd_table.translate(dirfd).unwrap_or(dirfd)
    };

    if let Some(path_addr) = args.path() {
        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Readlinkat::new()
                .with_dirfd(kernel_dirfd)
                .with_path(Some(new_path_addr))
                .with_buf(args.buf());

            return Ok(Some(Syscall::Readlinkat(new_syscall)));
        }
    }
    Ok(None)
}

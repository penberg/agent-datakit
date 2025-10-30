use crate::{
    sandbox::Sandbox,
    syscall::translate_path,
    vfs::{fdtable::FdTable, mount::MountTable},
};
use reverie::{
    syscalls::{MemoryAccess, ReadAddr, Syscall},
    Error, Guest, Stack,
};
use std::mem::MaybeUninit;

/// The `openat` system call.
///
/// This intercepts `openat` system calls and translates paths according to the mount table,
/// virtualizes the dirfd parameter, and virtualizes the returned file descriptor.
///
/// Returns `Some(result)` if the syscall was handled and the result should be returned directly,
/// or `None` if the original syscall should be used.
pub async fn handle_openat<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Openat,
    mount_table: &MountTable,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    // Virtualize the dirfd: AT_FDCWD is a special value that doesn't need translation
    let dirfd = args.dirfd();
    let kernel_dirfd = if dirfd == libc::AT_FDCWD {
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
                // For virtual VFS, open the file directly without going to the kernel
                let mode = args.mode().map(|m| m.bits()).unwrap_or(0o644);
                match vfs.open(&path, args.flags().bits() as i32, mode).await {
                    Ok(file_ops) => {
                        let virtual_fd = fd_table.allocate(file_ops, args.flags().bits() as i32);
                        return Ok(Some(virtual_fd as i64));
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
            } else {
                // For passthrough VFS, translate the path and call the kernel
                let new_path_addr = translate_path(guest, path_addr, mount_table).await?;

                let new_syscall = reverie::syscalls::Openat::new()
                    .with_dirfd(kernel_dirfd)
                    .with_path(new_path_addr.or(Some(path_addr)))
                    .with_flags(args.flags())
                    .with_mode(args.mode());

                let kernel_fd = guest.inject(Syscall::Openat(new_syscall)).await?;

                if kernel_fd >= 0 {
                    let file_ops = vfs.create_file_ops(kernel_fd as i32, args.flags().bits() as i32);
                    let virtual_fd = fd_table.allocate(file_ops, args.flags().bits() as i32);
                    return Ok(Some(virtual_fd as i64));
                } else {
                    return Ok(Some(kernel_fd));
                }
            }
        } else {
            // No mount point matches - pass through to kernel with original path
            let new_syscall = reverie::syscalls::Openat::new()
                .with_dirfd(kernel_dirfd)
                .with_path(Some(path_addr))
                .with_flags(args.flags())
                .with_mode(args.mode());

            let kernel_fd = guest.inject(Syscall::Openat(new_syscall)).await?;

            if kernel_fd >= 0 {
                use crate::vfs::passthrough::PassthroughFile;
                use std::sync::Arc;
                let file_ops = Arc::new(PassthroughFile::new(kernel_fd as i32, args.flags().bits() as i32));
                let virtual_fd = fd_table.allocate(file_ops, args.flags().bits() as i32);
                return Ok(Some(virtual_fd as i64));
            } else {
                return Ok(Some(kernel_fd));
            }
        }
    }
    Ok(None)
}

/// The `read` system call.
///
/// This intercepts `read` system calls and translates virtual FDs to kernel FDs,
/// or calls FileOps directly for virtual files.
pub async fn handle_read<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Read,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Get the FD entry
    if let Some(entry) = fd_table.get(virtual_fd) {
        // Check if this is a passthrough file with a kernel FD
        if let Some(kernel_fd) = entry.kernel_fd() {
            // Passthrough file - use kernel FD
            let new_syscall = reverie::syscalls::Read::new()
                .with_fd(kernel_fd)
                .with_buf(args.buf())
                .with_len(args.len());

            let result = guest.inject(Syscall::Read(new_syscall)).await?;
            return Ok(Some(result));
        } else {
            // Virtual file - use FileOps directly
            let buf_addr = match args.buf() {
                Some(addr) => addr,
                None => return Ok(Some(-libc::EFAULT as i64)),
            };

            let buf_len = args.len();
            let mut buf = vec![0u8; buf_len];

            match entry.file_ops.read(&mut buf).await {
                Ok(n) => {
                    // Write the data back to guest memory
                    if n > 0 {
                        guest.memory().write_exact(buf_addr, &buf[..n])?;
                    }
                    return Ok(Some(n as i64));
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

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `write` system call.
///
/// This intercepts `write` system calls and translates virtual FDs to kernel FDs,
/// or calls FileOps directly for virtual files.
pub async fn handle_write<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Write,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Get the FD entry
    if let Some(entry) = fd_table.get(virtual_fd) {
        // Check if this is a passthrough file with a kernel FD
        if let Some(kernel_fd) = entry.kernel_fd() {
            // Passthrough file - use kernel FD
            let new_syscall = reverie::syscalls::Write::new()
                .with_fd(kernel_fd)
                .with_buf(args.buf())
                .with_len(args.len());

            let result = guest.inject(Syscall::Write(new_syscall)).await?;
            return Ok(Some(result));
        } else {
            // Virtual file - use FileOps directly
            let buf_addr = match args.buf() {
                Some(addr) => addr,
                None => return Ok(Some(-libc::EFAULT as i64)),
            };

            let buf_len = args.len();
            let mut buf = vec![0u8; buf_len];

            // Read data from guest memory
            guest.memory().read_exact(buf_addr, &mut buf)?;

            match entry.file_ops.write(&buf).await {
                Ok(n) => {
                    return Ok(Some(n as i64));
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

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `close` system call.
///
/// This intercepts `close` system calls, translates virtual FDs to kernel FDs,
/// and cleans up the FD mapping.
pub async fn handle_close<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Close,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Translate and deallocate the virtual FD
    if let Some(entry) = fd_table.deallocate(virtual_fd) {
        if let Some(kernel_fd) = entry.kernel_fd() {
            let new_syscall = reverie::syscalls::Close::new().with_fd(kernel_fd);

            let result = guest.inject(Syscall::Close(new_syscall)).await?;
            return Ok(Some(result));
        } else {
            // Virtualized file - just call close on the FileOps
            entry.file_ops.close().ok();
            return Ok(Some(0)); // Success
        }
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `dup` system call.
///
/// This intercepts `dup` system calls and duplicates both the virtual and kernel FDs.
pub async fn handle_dup<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Dup,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let old_vfd = args.oldfd() as i32;

    // Duplicate using the FdTable's built-in duplicate method
    // This automatically handles both kernel FD duplication and FileOps cloning
    if let Some(new_vfd) = fd_table.duplicate(old_vfd) {
        // Get the kernel FD (if any) to actually duplicate it at the kernel level
        if let Some(kernel_fd) = fd_table.translate(old_vfd) {
            // Duplicate the kernel FD at the kernel level
            let new_syscall = reverie::syscalls::Dup::new().with_oldfd(kernel_fd);
            let result = guest.inject(Syscall::Dup(new_syscall)).await?;

            if result < 0 {
                // Dup failed, clean up the virtual FD we allocated
                fd_table.deallocate(new_vfd);
                return Ok(Some(result));
            }
        }
        return Ok(Some(new_vfd as i64));
    }

    // FD not in table, let the original syscall through
    Ok(None)
}

/// The `dup2` system call.
///
/// This intercepts `dup2` system calls and handles virtual FD duplication.
pub async fn handle_dup2<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Dup2,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let old_vfd = args.oldfd() as i32;
    let new_vfd = args.newfd() as i32;

    // Get the entry for the old virtual FD
    if let Some(old_entry) = fd_table.get(old_vfd) {
        // Get the entry at new_vfd if it exists (we need to close its kernel FD)
        let old_new_entry = fd_table.get(new_vfd);

        // If we have a kernel FD, duplicate it at the kernel level
        if let Some(old_kernel_fd) = old_entry.kernel_fd() {
            // Allocate a new kernel FD - we need to duplicate to a fresh FD first,
            // then close the old one if needed, to avoid race conditions
            let new_kernel_fd = guest
                .inject(Syscall::Dup(
                    reverie::syscalls::Dup::new().with_oldfd(old_kernel_fd),
                ))
                .await?;

            if new_kernel_fd < 0 {
                // Dup failed, return the error
                return Ok(Some(new_kernel_fd));
            }

            // Close the old kernel FD at new_vfd if it existed
            if let Some(old_entry) = old_new_entry {
                if let Some(kernel_fd) = old_entry.kernel_fd() {
                    let _ = guest
                        .inject(Syscall::Close(
                            reverie::syscalls::Close::new().with_fd(kernel_fd),
                        ))
                        .await?;
                } else {
                    // Close the FileOps if it's a virtualized file
                    old_entry.file_ops.close().ok();
                }
            }

            // Create new PassthroughFile for the duplicated kernel FD
            use crate::vfs::passthrough::PassthroughFile;
            use std::sync::Arc;
            let file_ops = Arc::new(PassthroughFile::new(new_kernel_fd as i32, 0));
            let _ = fd_table.allocate_at(new_vfd, file_ops, 0);
        } else {
            // Virtualized file - just clone the FileOps
            if let Some(old_entry) = old_new_entry {
                old_entry.file_ops.close().ok();
            }
            let _ = fd_table.allocate_at(new_vfd, old_entry.file_ops.clone(), 0);
        }

        Ok(Some(new_vfd as i64))
    } else {
        // FD not in table, let the original syscall through
        Ok(None)
    }
}

/// The `dup3` system call.
///
/// This intercepts `dup3` system calls and handles virtual FD duplication with flags.
pub async fn handle_dup3<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Dup3,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let old_vfd = args.oldfd() as i32;
    let new_vfd = args.newfd() as i32;
    let flags = args.flags();

    // Get the entry for the old virtual FD
    if let Some(old_entry) = fd_table.get(old_vfd) {
        // Get the entry at new_vfd if it exists (we need to close its kernel FD)
        let old_new_entry = fd_table.get(new_vfd);

        // If we have a kernel FD, duplicate it at the kernel level
        if let Some(old_kernel_fd) = old_entry.kernel_fd() {
            // Allocate a new kernel FD - we need to duplicate to a fresh FD first,
            // then close the old one if needed, to avoid race conditions
            let new_kernel_fd = guest
                .inject(Syscall::Dup(
                    reverie::syscalls::Dup::new().with_oldfd(old_kernel_fd),
                ))
                .await?;

            if new_kernel_fd < 0 {
                // Dup failed, return the error
                return Ok(Some(new_kernel_fd));
            }

            // Close the old kernel FD at new_vfd if it existed
            if let Some(old_entry) = old_new_entry {
                if let Some(kernel_fd) = old_entry.kernel_fd() {
                    let _ = guest
                        .inject(Syscall::Close(
                            reverie::syscalls::Close::new().with_fd(kernel_fd),
                        ))
                        .await?;
                } else {
                    old_entry.file_ops.close().ok();
                }
            }

            // Note: dup3 flags (O_CLOEXEC) are stored in the FD table and will be
            // applied later when needed (e.g., on exec). The kernel FD itself doesn't
            // need the flag since we're virtualizing the behavior.

            // Create new PassthroughFile for the duplicated kernel FD
            use crate::vfs::passthrough::PassthroughFile;
            use std::sync::Arc;
            let file_ops = Arc::new(PassthroughFile::new(new_kernel_fd as i32, flags.bits() as i32));
            let _ = fd_table.allocate_at(new_vfd, file_ops, flags.bits() as i32);
        } else {
            // Virtualized file - just clone the FileOps
            if let Some(old_entry) = old_new_entry {
                old_entry.file_ops.close().ok();
            }
            let _ = fd_table.allocate_at(new_vfd, old_entry.file_ops.clone(), flags.bits() as i32);
        }

        Ok(Some(new_vfd as i64))
    } else {
        // FD not in table, let the original syscall through
        Ok(None)
    }
}

/// The `ioctl` system call.
///
/// This intercepts `ioctl` system calls and translates virtual FDs to kernel FDs.
/// This is crucial for terminal control operations like job control in shells.
pub async fn handle_ioctl<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Ioctl,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Translate virtual FD to kernel FD
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        // If FDs are identical (common for stdin/stdout/stderr), pass through
        if virtual_fd == kernel_fd {
            return Ok(None);
        }

        // Create a new syscall with the translated kernel FD
        let new_syscall = reverie::syscalls::Ioctl::new()
            .with_fd(kernel_fd)
            .with_request(args.request());

        let result = guest.inject(Syscall::Ioctl(new_syscall)).await?;
        return Ok(Some(result));
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `fcntl` system call.
///
/// This intercepts `fcntl` system calls and handles virtual FD operations.
/// Special handling is needed for F_DUPFD and F_DUPFD_CLOEXEC commands which
/// duplicate file descriptors.
pub async fn handle_fcntl<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Fcntl,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    use reverie::syscalls::FcntlCmd;

    let virtual_fd = args.fd() as i32;

    // Translate virtual FD to kernel FD
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        match args.cmd() {
            FcntlCmd::F_DUPFD(arg) | FcntlCmd::F_DUPFD_CLOEXEC(arg) => {
                // For dup commands, we need to:
                // 1. Execute the syscall with the kernel FD to get a new kernel FD
                // 2. Allocate a new virtual FD for the result

                let is_cloexec = matches!(args.cmd(), FcntlCmd::F_DUPFD_CLOEXEC(_));
                // O_CLOEXEC = 0o2000000 on Linux
                let flags = if is_cloexec { 0o2000000 } else { 0 };

                // Translate the arg if it's a virtual FD (it specifies minimum FD number)
                // For now, we'll use 0 as the minimum for the kernel FD since we're virtualizing
                let kernel_arg = 0;

                let new_cmd = if is_cloexec {
                    FcntlCmd::F_DUPFD_CLOEXEC(kernel_arg)
                } else {
                    FcntlCmd::F_DUPFD(kernel_arg)
                };

                let new_syscall = reverie::syscalls::Fcntl::new()
                    .with_fd(kernel_fd)
                    .with_cmd(new_cmd);

                let new_kernel_fd = guest.inject(Syscall::Fcntl(new_syscall)).await?;

                // If the syscall succeeded, allocate a new virtual FD
                if new_kernel_fd >= 0 {
                    // Create a PassthroughFile for the new kernel FD
                    use crate::vfs::passthrough::PassthroughFile;
                    use std::sync::Arc;
                    let file_ops = Arc::new(PassthroughFile::new(new_kernel_fd as i32, flags));
                    // Allocate virtual FD at or above the requested minimum
                    let new_vfd = fd_table.allocate_min(arg as i32, file_ops, flags);
                    return Ok(Some(new_vfd as i64));
                } else {
                    // Return the error code as-is
                    return Ok(Some(new_kernel_fd));
                }
            }
            _ => {
                // For other fcntl commands, just translate the FD and pass through
                let new_syscall = reverie::syscalls::Fcntl::new()
                    .with_fd(kernel_fd)
                    .with_cmd(args.cmd());

                let result = guest.inject(Syscall::Fcntl(new_syscall)).await?;
                return Ok(Some(result));
            }
        }
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// Helper functions for working with fd_set
mod fdset {
    use super::*;

    /// Check if a file descriptor is set in an fd_set
    #[inline]
    pub fn is_set(fd: i32, set: &libc::fd_set) -> bool {
        unsafe { libc::FD_ISSET(fd, set as *const _ as *mut _) }
    }

    /// Set a file descriptor in an fd_set
    #[inline]
    pub fn set(fd: i32, set: &mut libc::fd_set) {
        unsafe { libc::FD_SET(fd, set) }
    }

    /// Clear an fd_set
    #[inline]
    pub fn zero(set: &mut libc::fd_set) {
        unsafe { libc::FD_ZERO(set) }
    }

    /// Translate an fd_set from virtual FDs to kernel FDs
    pub fn translate_to_kernel(
        virt_set: &libc::fd_set,
        virt_nfds: i32,
        fd_table: &FdTable,
    ) -> (libc::fd_set, i32) {
        let mut kernel_set: libc::fd_set = unsafe { MaybeUninit::zeroed().assume_init() };
        zero(&mut kernel_set);
        let mut max_kernel_fd = 0;

        for vfd in 0..virt_nfds {
            if is_set(vfd, virt_set) {
                if let Some(kfd) = fd_table.translate(vfd) {
                    set(kfd, &mut kernel_set);
                    if kfd > max_kernel_fd {
                        max_kernel_fd = kfd;
                    }
                }
            }
        }

        (kernel_set, max_kernel_fd + 1)
    }

    /// Translate an fd_set from kernel FDs back to virtual FDs
    pub fn translate_to_virtual(
        kernel_set: &libc::fd_set,
        kernel_nfds: i32,
        virt_set: &mut libc::fd_set,
        virt_nfds: i32,
        fd_table: &FdTable,
    ) {
        zero(virt_set);

        // Iterate through virtual FDs and check if their kernel FD is set
        for vfd in 0..virt_nfds {
            if let Some(kfd) = fd_table.translate(vfd) {
                if kfd < kernel_nfds && is_set(kfd, kernel_set) {
                    set(vfd, virt_set);
                }
            }
        }
    }
}

/// The `pselect6` system call.
///
/// This intercepts `pselect6` system calls and translates virtual FDs in the fd_sets
/// to kernel FDs before calling the real syscall, then translates the results back.
pub async fn handle_pselect6<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Pselect6,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virt_nfds = args.nfds();

    // Read the virtual fd_sets from guest memory
    let virt_readfds = if let Some(addr) = args.readfds() {
        Some(guest.memory().read_value(addr)?)
    } else {
        None
    };

    let virt_writefds = if let Some(addr) = args.writefds() {
        Some(guest.memory().read_value(addr)?)
    } else {
        None
    };

    let virt_exceptfds = if let Some(addr) = args.exceptfds() {
        Some(guest.memory().read_value(addr)?)
    } else {
        None
    };

    // Translate fd_sets from virtual to kernel FDs
    let (kernel_readfds, max_read) = if let Some(ref vset) = virt_readfds {
        let (kset, max) = fdset::translate_to_kernel(vset, virt_nfds, fd_table);
        (Some(kset), max)
    } else {
        (None, 0)
    };

    let (kernel_writefds, max_write) = if let Some(ref vset) = virt_writefds {
        let (kset, max) = fdset::translate_to_kernel(vset, virt_nfds, fd_table);
        (Some(kset), max)
    } else {
        (None, 0)
    };

    let (kernel_exceptfds, max_except) = if let Some(ref vset) = virt_exceptfds {
        let (kset, max) = fdset::translate_to_kernel(vset, virt_nfds, fd_table);
        (Some(kset), max)
    } else {
        (None, 0)
    };

    // Calculate the maximum kernel FD + 1
    let kernel_nfds = max_read.max(max_write).max(max_except);

    // If all fd_sets are None or nfds is 0, just pass through
    if kernel_nfds == 0 {
        return Ok(None);
    }

    // Allocate space for kernel fd_sets in guest memory
    let mut stack = guest.stack().await;

    let kernel_readfds_addr = if kernel_readfds.is_some() {
        let addr: reverie::syscalls::AddrMut<libc::fd_set> = stack.reserve();
        Some(addr)
    } else {
        None
    };

    let kernel_writefds_addr = if kernel_writefds.is_some() {
        let addr: reverie::syscalls::AddrMut<libc::fd_set> = stack.reserve();
        Some(addr)
    } else {
        None
    };

    let kernel_exceptfds_addr = if kernel_exceptfds.is_some() {
        let addr: reverie::syscalls::AddrMut<libc::fd_set> = stack.reserve();
        Some(addr)
    } else {
        None
    };

    stack.commit()?;

    // Write kernel fd_sets to guest memory
    if let (Some(kset), Some(addr)) = (kernel_readfds.as_ref(), kernel_readfds_addr) {
        guest.memory().write_value(addr, kset)?;
    }
    if let (Some(kset), Some(addr)) = (kernel_writefds.as_ref(), kernel_writefds_addr) {
        guest.memory().write_value(addr, kset)?;
    }
    if let (Some(kset), Some(addr)) = (kernel_exceptfds.as_ref(), kernel_exceptfds_addr) {
        guest.memory().write_value(addr, kset)?;
    }

    // Create new syscall with translated FDs
    let new_syscall = reverie::syscalls::Pselect6::new()
        .with_nfds(kernel_nfds)
        .with_readfds(kernel_readfds_addr)
        .with_writefds(kernel_writefds_addr)
        .with_exceptfds(kernel_exceptfds_addr)
        .with_timeout(args.timeout())
        .with_sigmask(args.sigmask());

    // Execute the syscall
    let result = guest.inject(Syscall::Pselect6(new_syscall)).await?;

    // If the syscall failed or timed out, return early
    if result <= 0 {
        return Ok(Some(result));
    }

    // Read back the kernel fd_sets and translate to virtual FDs
    if let (Some(addr), Some(_)) = (kernel_readfds_addr, virt_readfds.as_ref()) {
        let kernel_set: libc::fd_set = guest.memory().read_value(addr)?;
        let mut virt_set: libc::fd_set = unsafe { MaybeUninit::zeroed().assume_init() };
        fdset::translate_to_virtual(&kernel_set, kernel_nfds, &mut virt_set, virt_nfds, fd_table);

        // Write back to original guest address
        if let Some(orig_addr) = args.readfds() {
            guest.memory().write_value(orig_addr, &virt_set)?;
        }
    }

    if let (Some(addr), Some(_)) = (kernel_writefds_addr, virt_writefds.as_ref()) {
        let kernel_set: libc::fd_set = guest.memory().read_value(addr)?;
        let mut virt_set: libc::fd_set = unsafe { MaybeUninit::zeroed().assume_init() };
        fdset::translate_to_virtual(&kernel_set, kernel_nfds, &mut virt_set, virt_nfds, fd_table);

        if let Some(orig_addr) = args.writefds() {
            guest.memory().write_value(orig_addr, &virt_set)?;
        }
    }

    if let (Some(addr), Some(_)) = (kernel_exceptfds_addr, virt_exceptfds.as_ref()) {
        let kernel_set: libc::fd_set = guest.memory().read_value(addr)?;
        let mut virt_set: libc::fd_set = unsafe { MaybeUninit::zeroed().assume_init() };
        fdset::translate_to_virtual(&kernel_set, kernel_nfds, &mut virt_set, virt_nfds, fd_table);

        if let Some(orig_addr) = args.exceptfds() {
            guest.memory().write_value(orig_addr, &virt_set)?;
        }
    }

    Ok(Some(result))
}

/// The `poll` system call.
///
/// This intercepts `poll` system calls and translates virtual FDs in the pollfd array
/// to kernel FDs before calling the real syscall, then translates the results back.
pub async fn handle_poll<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Poll,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    use reverie::syscalls::{MemoryAccess, PollFd};

    let nfds = args.nfds();
    if nfds == 0 {
        return Ok(None);
    }

    let fds_addr = match args.fds() {
        Some(addr) => addr,
        None => return Ok(None),
    };

    // Read the pollfd array from guest memory
    let mut pollfds: Vec<PollFd> = Vec::with_capacity(nfds as usize);
    for i in 0..nfds {
        let offset = i as isize * std::mem::size_of::<PollFd>() as isize;
        let pollfd: PollFd = unsafe {
            guest.memory().read_value(fds_addr.offset(offset))?
        };
        pollfds.push(pollfd);
    }

    // Allocate space on stack for kernel pollfd array
    let mut stack = guest.stack().await;
    let kernel_fds_addr: reverie::syscalls::AddrMut<PollFd> = stack.reserve();

    // Reserve space for remaining pollfds
    for _ in 1..nfds {
        let _: reverie::syscalls::AddrMut<PollFd> = stack.reserve();
    }

    stack.commit()?;

    // Write kernel pollfds to guest memory
    for (i, pollfd) in pollfds.iter().enumerate() {
        let kernel_fd = fd_table.translate(pollfd.fd).unwrap_or(pollfd.fd);
        let kernel_pollfd = PollFd {
            fd: kernel_fd,
            events: pollfd.events,
            revents: reverie::syscalls::PollFlags::empty(),
        };

        let offset = i as isize * std::mem::size_of::<PollFd>() as isize;
        unsafe {
            guest.memory().write_value(kernel_fds_addr.offset(offset), &kernel_pollfd)?;
        }
    }

    // Create and inject the syscall with translated FDs
    let new_syscall = reverie::syscalls::Poll::new()
        .with_fds(Some(kernel_fds_addr))
        .with_nfds(nfds)
        .with_timeout(args.timeout());

    let result = guest.inject(Syscall::Poll(new_syscall)).await?;

    // If the syscall failed or timed out, return early
    if result <= 0 {
        return Ok(Some(result));
    }

    // Read back the kernel pollfds and translate to virtual FDs
    for i in 0..nfds {
        let offset = i as isize * std::mem::size_of::<PollFd>() as isize;
        let kernel_pollfd: PollFd = unsafe {
            guest.memory().read_value(kernel_fds_addr.offset(offset))?
        };

        // Write back the revents to the original pollfd array
        let virt_pollfd = PollFd {
            fd: pollfds[i as usize].fd,  // Keep the virtual FD
            events: pollfds[i as usize].events,
            revents: kernel_pollfd.revents,
        };

        unsafe {
            guest.memory().write_value(fds_addr.offset(offset), &virt_pollfd)?;
        }
    }

    Ok(Some(result))
}

/// The `getdents64` system call.
///
/// This intercepts `getdents64` system calls and translates virtual FDs to kernel FDs,
/// or calls FileOps::getdents() for virtual files.
pub async fn handle_getdents64<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Getdents64,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Get the FD entry
    if let Some(entry) = fd_table.get(virtual_fd) {
        // Check if this is a passthrough file with a kernel FD
        if let Some(kernel_fd) = entry.kernel_fd() {
            // Passthrough file - use kernel FD
            let new_syscall = reverie::syscalls::Getdents64::new()
                .with_fd(kernel_fd as u32)
                .with_dirent(args.dirent())
                .with_count(args.count());

            let result = guest.inject(Syscall::Getdents64(new_syscall)).await?;
            return Ok(Some(result));
        } else {
            // Virtual file - use FileOps::getdents()
            match entry.file_ops.getdents().await {
                Ok(entries) => {
                    // Format as linux_dirent64 structures
                    let dirent_addr = match args.dirent() {
                        Some(addr) => addr,
                        None => return Ok(Some(-libc::EFAULT as i64)),
                    };
                    let count = args.count() as usize;

                    let mut buf = Vec::new();
                    let mut offset = 1i64;

                    for (ino, name, d_type) in entries {
                        // Calculate record length (aligned to 8 bytes)
                        let name_len = name.len() + 1; // +1 for null terminator
                        let reclen = ((19 + name_len + 7) / 8) * 8; // 19 = sizeof(ino + off + reclen + type)

                        if buf.len() + reclen > count {
                            break; // Not enough space
                        }

                        // Write linux_dirent64 structure
                        buf.extend_from_slice(&ino.to_ne_bytes());           // d_ino (u64)
                        buf.extend_from_slice(&offset.to_ne_bytes());        // d_off (i64)
                        buf.extend_from_slice(&(reclen as u16).to_ne_bytes()); // d_reclen (u16)
                        buf.push(d_type);                                    // d_type (u8)
                        buf.extend_from_slice(name.as_bytes());              // d_name
                        buf.push(0);                                         // null terminator

                        // Pad to 8-byte alignment
                        while buf.len() % 8 != 0 {
                            buf.push(0);
                        }

                        offset += 1;
                    }

                    // Write to guest memory
                    if !buf.is_empty() {
                        guest.memory().write_exact(dirent_addr.cast::<u8>(), &buf)?;
                    }

                    return Ok(Some(buf.len() as i64));
                }
                Err(_) => {
                    // Not a directory or error
                    return Ok(Some(-libc::ENOTDIR as i64));
                }
            }
        }
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `fstat` system call.
///
/// This intercepts `fstat` system calls and translates virtual FDs to kernel FDs,
/// or calls FileOps::fstat() for virtual files.
pub async fn handle_fstat<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Fstat,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Get the FD entry
    if let Some(entry) = fd_table.get(virtual_fd) {
        // Check if this is a passthrough file with a kernel FD
        if let Some(kernel_fd) = entry.kernel_fd() {
            // Passthrough file - use kernel FD
            let new_syscall = reverie::syscalls::Fstat::new()
                .with_fd(kernel_fd)
                .with_stat(args.stat());

            let result = guest.inject(Syscall::Fstat(new_syscall)).await?;
            return Ok(Some(result));
        } else {
            // Virtual file - use FileOps::fstat()
            match entry.file_ops.fstat().await {
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
                        guest.memory().write_exact(stat_addr.0.cast::<u8>(), stat_bytes)?;
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

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `pread64` system call.
///
/// This intercepts `pread64` system calls and translates virtual FDs to kernel FDs.
pub async fn handle_pread64<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Pread64,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Translate virtual FD to kernel FD
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        let new_syscall = reverie::syscalls::Pread64::new()
            .with_fd(kernel_fd)
            .with_buf(args.buf())
            .with_len(args.len())
            .with_offset(args.offset());

        let result = guest.inject(Syscall::Pread64(new_syscall)).await?;
        return Ok(Some(result));
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `pwrite64` system call.
///
/// This intercepts `pwrite64` system calls and translates virtual FDs to kernel FDs.
pub async fn handle_pwrite64<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Pwrite64,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Translate virtual FD to kernel FD
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        let new_syscall = reverie::syscalls::Pwrite64::new()
            .with_fd(kernel_fd)
            .with_buf(args.buf())
            .with_len(args.len())
            .with_offset(args.offset());

        let result = guest.inject(Syscall::Pwrite64(new_syscall)).await?;
        return Ok(Some(result));
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `lseek` system call.
///
/// This intercepts `lseek` system calls and translates virtual FDs to kernel FDs.
pub async fn handle_lseek<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Lseek,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Translate virtual FD to kernel FD
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        let new_syscall = reverie::syscalls::Lseek::new()
            .with_fd(kernel_fd)
            .with_offset(args.offset())
            .with_whence(args.whence());

        let result = guest.inject(Syscall::Lseek(new_syscall)).await?;
        return Ok(Some(result));
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `mmap` system call.
///
/// This intercepts `mmap` system calls and translates virtual FDs to kernel FDs
/// when mapping files. Anonymous mappings (fd == -1) pass through unchanged.
pub async fn handle_mmap<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Mmap,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // If fd is -1, it's an anonymous mapping - pass through
    if virtual_fd == -1 {
        return Ok(None);
    }

    // Translate virtual FD to kernel FD for file-backed mappings
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        let new_syscall = reverie::syscalls::Mmap::new()
            .with_addr(args.addr())
            .with_len(args.len())
            .with_prot(args.prot())
            .with_flags(args.flags())
            .with_fd(kernel_fd)
            .with_offset(args.offset());

        let result = guest.inject(Syscall::Mmap(new_syscall)).await?;
        return Ok(Some(result));
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `access` system call.
///
/// This intercepts `access` system calls and translates paths according to the mount table.
pub async fn handle_access<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Access,
    mount_table: &MountTable,
) -> Result<Option<Syscall>, Error> {
    if let Some(path_addr) = args.path() {
        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Access::new()
                .with_path(Some(new_path_addr))
                .with_mode(args.mode());

            return Ok(Some(Syscall::Access(new_syscall)));
        }
    }
    Ok(None)
}

/// The `faccessat2` system call.
///
/// This intercepts `faccessat2` system calls, translates paths according to the mount table,
/// and virtualizes the dirfd parameter.
/// Signature: int faccessat2(int dirfd, const char *pathname, int mode, int flags);
pub async fn handle_faccessat2<T: Guest<Sandbox>>(
    guest: &mut T,
    syscall_args: &reverie::syscalls::SyscallArgs,
    mount_table: &MountTable,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    use reverie::syscalls::PathPtr;

    let dirfd = syscall_args.arg0 as i32;
    let pathname_addr: PathPtr = unsafe { std::mem::transmute(syscall_args.arg1) };
    let mode = syscall_args.arg2 as i32;
    let flags = syscall_args.arg3 as i32;

    // Check if dirfd needs virtualization
    let dirfd_needs_translation = dirfd != libc::AT_FDCWD && fd_table.translate(dirfd).is_some();

    // Check if path needs virtualization
    let translated_path_opt = translate_path(guest, pathname_addr, mount_table).await?;
    let path_needs_translation = translated_path_opt.is_some();

    // If nothing needs virtualization, let the original syscall pass through
    if !dirfd_needs_translation && !path_needs_translation {
        return Ok(None);
    }

    // Virtualize the dirfd if needed
    let kernel_dirfd = if dirfd == libc::AT_FDCWD {
        dirfd
    } else {
        fd_table.translate(dirfd).unwrap_or(dirfd)
    };

    let new_path_addr = translated_path_opt.unwrap_or(pathname_addr);
    let new_path_raw: usize = unsafe { std::mem::transmute(new_path_addr) };

    // Build and inject the syscall with virtualized parameters
    let result = guest.inject(Syscall::Other(
        reverie::syscalls::Sysno::faccessat2,
        reverie::syscalls::SyscallArgs {
            arg0: kernel_dirfd as usize,
            arg1: new_path_raw,
            arg2: mode as usize,
            arg3: flags as usize,
            arg4: 0,
            arg5: 0,
        }
    )).await?;

    Ok(Some(result))
}

/// The `rename` system call.
///
/// This intercepts `rename` system calls and translates both paths according to the mount table.
pub async fn handle_rename<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Rename,
    mount_table: &MountTable,
) -> Result<Option<Syscall>, Error> {
    // Only translate if we need to - otherwise pass through unchanged
    let oldpath_needs_translation = args.oldpath().is_some();
    let newpath_needs_translation = args.newpath().is_some();

    if !oldpath_needs_translation && !newpath_needs_translation {
        return Ok(None);
    }

    // Build new syscall with translated paths
    let mut new_syscall = reverie::syscalls::Rename::new();
    let mut modified = false;

    // Translate oldpath
    if let Some(oldpath_addr) = args.oldpath() {
        if let Some(new_path_addr) = translate_path(guest, oldpath_addr, mount_table).await? {
            new_syscall = new_syscall.with_oldpath(Some(new_path_addr));
            modified = true;
        } else {
            new_syscall = new_syscall.with_oldpath(Some(oldpath_addr));
        }
    }

    // Translate newpath
    if let Some(newpath_addr) = args.newpath() {
        if let Some(new_path_addr) = translate_path(guest, newpath_addr, mount_table).await? {
            new_syscall = new_syscall.with_newpath(Some(new_path_addr));
            modified = true;
        } else {
            new_syscall = new_syscall.with_newpath(Some(newpath_addr));
        }
    }

    if modified {
        Ok(Some(Syscall::Rename(new_syscall)))
    } else {
        Ok(None)
    }
}

/// The `unlink` system call.
///
/// This intercepts `unlink` system calls and translates paths according to the mount table.
pub async fn handle_unlink<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Unlink,
    mount_table: &MountTable,
) -> Result<Option<Syscall>, Error> {
    if let Some(path_addr) = args.path() {
        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Unlink::new()
                .with_path(Some(new_path_addr));

            return Ok(Some(Syscall::Unlink(new_syscall)));
        }
    }
    Ok(None)
}

/// The `readv` system call.
///
/// This intercepts `readv` system calls and translates virtual FDs to kernel FDs.
pub async fn handle_readv<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Readv,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Translate virtual FD to kernel FD
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        let new_syscall = reverie::syscalls::Readv::new()
            .with_fd(kernel_fd)
            .with_iov(args.iov());

        let result = guest.inject(Syscall::Readv(new_syscall)).await?;
        return Ok(Some(result));
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `writev` system call.
///
/// This intercepts `writev` system calls and translates virtual FDs to kernel FDs.
pub async fn handle_writev<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Writev,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Translate virtual FD to kernel FD
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        let new_syscall = reverie::syscalls::Writev::new()
            .with_fd(kernel_fd)
            .with_iov(args.iov());

        let result = guest.inject(Syscall::Writev(new_syscall)).await?;
        return Ok(Some(result));
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `pipe2` system call.
///
/// This intercepts `pipe2` system calls and virtualizes the returned file descriptors.
pub async fn handle_pipe2<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Pipe2,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    use reverie::syscalls::MemoryAccess;

    // Execute the syscall to create the pipe
    let result = guest.inject(Syscall::Pipe2(*args)).await?;

    // If successful, virtualize the returned FDs
    if result == 0 {
        // Read the kernel FDs from the pipefd array
        if let Some(pipefd_addr) = args.pipefd() {
            let kernel_fds: [i32; 2] = guest.memory().read_value(pipefd_addr)?;

            // Create PassthroughFile instances for both pipe ends
            use crate::vfs::passthrough::PassthroughFile;
            use std::sync::Arc;
            let read_file_ops = Arc::new(PassthroughFile::new(kernel_fds[0], args.flags().bits() as i32));
            let write_file_ops = Arc::new(PassthroughFile::new(kernel_fds[1], args.flags().bits() as i32));

            // Allocate virtual FDs for both pipe ends
            let virtual_read_fd = fd_table.allocate(read_file_ops, args.flags().bits() as i32);
            let virtual_write_fd = fd_table.allocate(write_file_ops, args.flags().bits() as i32);

            // Write each FD individually as bytes to avoid alignment issues
            let read_bytes = virtual_read_fd.to_ne_bytes();
            let write_bytes = virtual_write_fd.to_ne_bytes();

            guest.memory().write_exact(pipefd_addr.cast(), &read_bytes)?;
            unsafe {
                guest.memory().write_exact(pipefd_addr.cast::<u8>().offset(4), &write_bytes)?;
            }
        }
    }

    Ok(Some(result))
}

/// The `socket` system call.
///
/// This intercepts `socket` system calls and virtualizes the returned file descriptor.
pub async fn handle_socket<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Socket,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    // Execute the syscall to create the socket
    let kernel_fd = guest.inject(Syscall::Socket(*args)).await?;

    // If the syscall succeeded (returned a valid FD), virtualize it
    if kernel_fd >= 0 {
        use crate::vfs::passthrough::PassthroughFile;
        use std::sync::Arc;
        let file_ops = Arc::new(PassthroughFile::new(kernel_fd as i32, 0));
        let virtual_fd = fd_table.allocate(file_ops, 0);
        return Ok(Some(virtual_fd as i64));
    } else {
        // Return the error code as-is
        return Ok(Some(kernel_fd));
    }
}

/// The `sendto` system call.
///
/// This intercepts `sendto` system calls and translates virtual FDs to kernel FDs.
pub async fn handle_sendto<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Sendto,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Translate virtual FD to kernel FD
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        let new_syscall = reverie::syscalls::Sendto::new()
            .with_fd(kernel_fd)
            .with_buf(args.buf())
            .with_flags(args.flags())
            .with_addr(args.addr());

        let result = guest.inject(Syscall::Sendto(new_syscall)).await?;
        return Ok(Some(result));
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `connect` system call.
///
/// This intercepts `connect` system calls and translates virtual FDs to kernel FDs.
pub async fn handle_connect<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Connect,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Translate virtual FD to kernel FD
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        let new_syscall = reverie::syscalls::Connect::new()
            .with_fd(kernel_fd)
            .with_addrlen(args.addrlen());

        let result = guest.inject(Syscall::Connect(new_syscall)).await?;
        return Ok(Some(result));
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

/// The `getpeername` system call.
///
/// This intercepts `getpeername` system calls and translates virtual FDs to kernel FDs.
pub async fn handle_getpeername<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Getpeername,
    fd_table: &FdTable,
) -> Result<Option<i64>, Error> {
    let virtual_fd = args.fd() as i32;

    // Translate virtual FD to kernel FD
    if let Some(kernel_fd) = fd_table.translate(virtual_fd) {
        let new_syscall = reverie::syscalls::Getpeername::new()
            .with_fd(kernel_fd)
            .with_usockaddr(args.usockaddr())
            .with_usockaddr_len(args.usockaddr_len());

        let result = guest.inject(Syscall::Getpeername(new_syscall)).await?;
        return Ok(Some(result));
    }

    // FD not in table, let the original syscall through (will likely fail with EBADF)
    Ok(None)
}

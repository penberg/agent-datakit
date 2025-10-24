use crate::{sandbox::Sandbox, syscall::translate_path, vfs::mount::MountTable};
use reverie::{syscalls::Syscall, Error, Guest};

/// The `llistxattr` system call.
///
/// This intercepts llistxattr syscalls and translates paths according to the mount table.
/// Returns Some(syscall) if the path was translated and should be injected,
/// or None if the original syscall should be used.
pub async fn handle_llistxattr<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Llistxattr,
    mount_table: &MountTable,
) -> Result<Option<Syscall>, Error> {
    if let Some(path_addr) = args.path() {
        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Llistxattr::new()
                .with_path(Some(new_path_addr))
                .with_list(args.list())
                .with_size(args.size());

            return Ok(Some(Syscall::Llistxattr(new_syscall)));
        }
    }
    Ok(None)
}

/// The `lgetxattr` system call.
///
/// This intercepts lgetxattr syscalls and translates paths according to the mount table.
/// Returns Some(syscall) if the path was translated and should be injected,
/// or None if the original syscall should be used.
pub async fn handle_lgetxattr<T: Guest<Sandbox>>(
    guest: &mut T,
    args: &reverie::syscalls::Lgetxattr,
    mount_table: &MountTable,
) -> Result<Option<Syscall>, Error> {
    if let Some(path_addr) = args.path() {
        if let Some(new_path_addr) = translate_path(guest, path_addr, mount_table).await? {
            let new_syscall = reverie::syscalls::Lgetxattr::new()
                .with_path(Some(new_path_addr))
                .with_name(args.name())
                .with_value(args.value())
                .with_size(args.size());

            return Ok(Some(Syscall::Lgetxattr(new_syscall)));
        }
    }
    Ok(None)
}

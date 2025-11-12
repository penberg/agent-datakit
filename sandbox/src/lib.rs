#[cfg(target_os = "linux")]
pub mod sandbox;
#[cfg(target_os = "linux")]
pub mod syscall;
#[cfg(target_os = "linux")]
pub mod vfs;

#[cfg(target_os = "linux")]
pub use sandbox::{init_fd_tables, init_mount_table, init_strace, Sandbox};
#[cfg(target_os = "linux")]
pub use vfs::{
    bind::BindVfs,
    mount::{MountConfig, MountTable, MountType},
    sqlite::SqliteVfs,
    Vfs, VfsError, VfsResult,
};

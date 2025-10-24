use crate::{
    syscall,
    vfs::{fdtable::FdTable, mount::MountTable},
};
use reverie::{syscalls::Syscall, Error, Guest, Tool};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex, OnceLock,
};

/// Global mount table shared across all threads
static MOUNT_TABLE: OnceLock<MountTable> = OnceLock::new();

/// Global FD tables, one per process (keyed by pid)
static FD_TABLES: OnceLock<Mutex<HashMap<i32, FdTable>>> = OnceLock::new();

/// Global flag to enable strace-like output
static STRACE_ENABLED: AtomicBool = AtomicBool::new(false);

/// Initialize the global mount table
///
/// This must be called before spawning the traced process.
pub fn init_mount_table(table: MountTable) {
    MOUNT_TABLE
        .set(table)
        .expect("Mount table already initialized");
}

/// Get a reference to the global mount table
fn get_mount_table() -> &'static MountTable {
    MOUNT_TABLE.get().expect("Mount table not initialized")
}

/// Initialize the global FD tables
///
/// This must be called before spawning the traced process.
pub fn init_fd_tables() {
    FD_TABLES
        .set(Mutex::new(HashMap::new()))
        .expect("FD tables already initialized");
}

/// Initialize strace mode
///
/// This must be called before spawning the traced process.
pub fn init_strace(enabled: bool) {
    STRACE_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Check if strace is enabled
fn is_strace_enabled() -> bool {
    STRACE_ENABLED.load(Ordering::Relaxed)
}

/// Get or create an FD table for a specific process
fn get_fd_table(pid: i32) -> FdTable {
    let tables = FD_TABLES.get().expect("FD tables not initialized");
    let mut tables = tables.lock().unwrap();

    tables.entry(pid).or_insert_with(FdTable::new).clone()
}

/// Insert an FD table for a specific process (used for fork/clone)
pub(crate) fn insert_fd_table(pid: i32, fd_table: FdTable) {
    let tables = FD_TABLES.get().expect("FD tables not initialized");
    let mut tables = tables.lock().unwrap();

    tables.insert(pid, fd_table);
}

/// Format a syscall for strace-like output
fn format_syscall(syscall: &Syscall) -> String {
    // Using the Debug implementation as a starting point
    format!("{:?}", syscall)
}

/// Format a syscall result for strace-like output
fn format_result(value: i64) -> String {
    if value < 0 {
        format!("{}", value)
    } else {
        format!("{}", value)
    }
}

/// The Sandbox tool
///
/// This implements the Reverie Tool trait and intercepts syscalls
/// to provide filesystem virtualization.
#[derive(Default)]
pub struct Sandbox {}

#[reverie::tool]
impl Tool for Sandbox {
    type GlobalState = ();
    type ThreadState = ();

    async fn handle_syscall_event<T: Guest<Self>>(
        &self,
        guest: &mut T,
        syscall: Syscall,
    ) -> Result<i64, Error> {
        let mount_table = get_mount_table();
        let pid = guest.pid().as_raw();
        let fd_table = get_fd_table(pid);

        if is_strace_enabled() {
            eprintln!("[{}] {}", pid, format_syscall(&syscall));
        }

        let result = match syscall::dispatch_syscall(guest, syscall, mount_table, &fd_table).await {
            Ok(syscall::SyscallResult::Value(value)) => {
                if is_strace_enabled() {
                    eprintln!("[{}] = {}", pid, format_result(value));
                }
                Ok(value)
            }
            Ok(syscall::SyscallResult::Syscall(syscall)) => guest.tail_inject(syscall).await,
            Err(e) => {
                if is_strace_enabled() {
                    if let Error::Errno(errno) = &e {
                        eprintln!("[{}] = -1 {}", pid, errno);
                    } else {
                        eprintln!("[{}] = error: {:?}", pid, e);
                    }
                }
                Err(e)
            }
        };

        result
    }
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod run_linux;

use std::path::PathBuf;

// Import MountConfig from the appropriate source
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
pub use agentfs_sandbox::MountConfig;

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
pub use crate::non_linux::MountConfig;

pub async fn handle_run_command(
    mounts: Vec<MountConfig>,
    strace: bool,
    command: PathBuf,
    args: Vec<String>,
) {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        run_linux::run_sandbox(mounts, strace, command, args).await;
    }

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    {
        let _ = (mounts, strace, command, args);
        eprintln!("Sandbox is not available on this platform.");
        eprintln!();
        eprintln!("However, you can still use the other AgentFS commands:");
        eprintln!("  - 'agentfs init' to create a new agent filesystem");
        eprintln!("  - 'agentfs fs ls' to list files");
        eprintln!("  - 'agentfs fs cat' to view file contents");
        std::process::exit(1);
    }
}

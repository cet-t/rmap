//! Windows named pipe IPC (prototype stub).
//! The protocol types are defined; the real pipe wire is deferred to keep the Windows prototype buildable
//! without pulling extra windows features for Storage/Pipes in the core lib at this stage.
//! Daemon tray + watcher + per-app + hook are the primary deliverables for "runnable prototype".

use super::{IpcCommand, IpcResponse};

pub fn start_ipc_server<F>(_on_cmd: F)
where
    F: FnMut(IpcCommand) -> IpcResponse + Send + 'static,
{
    // Stub: no-op server thread for prototype.
    // Real implementation would create \\.\pipe\rmap with proper ACL, accept clients, and dispatch commands.
    // Keeping this stub allows daemon/config to compile and the overall prototype to remain runnable.
}

pub fn send_reload_command() -> anyhow::Result<IpcResponse> {
    // In a full prototype we would open the pipe and send a framed Reload.
    // For now, surface a clear message so users know the wire is not yet active.
    Err(anyhow::anyhow!("IPC pipe not wired in this prototype build (tray Reload and file watch still work)"))
}
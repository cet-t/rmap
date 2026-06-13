//! rmap-config: control app. For prototype: CLI stub that can trigger IPC reload.
//! Usage: rmap-config reload | status | quit

use rmap_core::ipc::{send_reload_command, IpcResponse};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");
    match cmd {
        "reload" => {
            match send_reload_command() {
                Ok(IpcResponse::Ok) => println!("reload sent"),
                Ok(r) => println!("response: {:?}", r),
                Err(e) => eprintln!("IPC error: {e} (is daemon running?)"),
            }
        }
        "status" => {
            println!("status: (IPC status not fully wired in prototype; daemon tray shows state)");
        }
        "quit" => {
            println!("quit: (send IpcCommand::Quit via pipe in full impl)");
        }
        _ => {
            println!("rmap-config (prototype)");
            println!("commands: reload | status | quit");
        }
    }
}

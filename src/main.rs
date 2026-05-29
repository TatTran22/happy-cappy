use clap::Parser;
use happy_cappy::app::{AppCommand, DesktopPetApp};
use happy_cappy::control_socket::{
    bind_control_socket, control_socket_path, send_notify, spawn_listener, BindOutcome,
};
use happy_cappy::notification::{Cli, Command};
use log::warn;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    // Client path: `happy-cappy notify ...` sends one event to the running app and exits.
    let cli = Cli::parse();
    if let Some(Command::Notify(args)) = cli.command {
        let path = control_socket_path()?;
        return match send_notify(&path, &args.to_event()) {
            Ok(()) => Ok(()),
            Err(e) => {
                eprintln!("happy-cappy: could not reach a running pet ({e}). Is the app open?");
                std::process::exit(1);
            }
        };
    }

    // Server/GUI path.
    let event_loop = EventLoop::<AppCommand>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();

    // Control socket: bind BEFORE GUI init so the single-instance check happens first.
    match control_socket_path() {
        Ok(path) => {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match bind_control_socket(&path) {
                BindOutcome::Bound(listener) => spawn_listener(listener, proxy.clone()),
                BindOutcome::AlreadyRunning => {
                    eprintln!("happy-cappy: another instance is already running.");
                    std::process::exit(0);
                }
                BindOutcome::Failed(e) => {
                    warn!("control socket unavailable ({e}); continuing without external triggers");
                }
            }
        }
        Err(e) => warn!("cannot resolve control socket path ({e}); external triggers disabled"),
    }

    let mut app = DesktopPetApp::new(proxy);
    event_loop.run_app(&mut app)?;
    Ok(())
}

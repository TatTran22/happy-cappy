use desktop_pet::app::DesktopPetApp;
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = DesktopPetApp::new();
    event_loop.run_app(&mut app)?;

    Ok(())
}

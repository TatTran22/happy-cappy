use desktop_pet::app::{AppCommand, DesktopPetApp};
use winit::event_loop::{ControlFlow, EventLoop};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let event_loop = EventLoop::<AppCommand>::with_user_event().build()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = DesktopPetApp::new(event_loop.create_proxy());
    event_loop.run_app(&mut app)?;

    Ok(())
}

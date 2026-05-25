use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

#[derive(Debug, Default)]
pub struct DesktopPetApp;

impl DesktopPetApp {
    pub fn new() -> Self {
        Self
    }
}

impl ApplicationHandler for DesktopPetApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

use thiserror::Error;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const WIDTH: u8 = 64;
const HEIGHT: u8 = 32;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Winit event loop error")]
    EventLoopError(#[from] winit::error::EventLoopError),
    #[error("Winit Os Error")]
    OsError(#[from] winit::error::OsError),
}

#[derive(Default)]
struct Chip8 {
    window: Option<Window>,
}

impl ApplicationHandler for Chip8 {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = Some(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Chip8")
                        .with_inner_size(LogicalSize::new(WIDTH, HEIGHT)),
                )
                .unwrap(),
        );
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                println!("Close button pressed, exiting");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                self.window.as_ref().unwrap().request_redraw();
            }
            _ => (),
        }
    }
}

pub fn init() -> Result<(), Error> {
    let mut _mem: [u8; 4096];

    // program counter
    let _pc: u16;
    // index register
    let _idx_reg: u16;

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut chip8 = Chip8::default();
    event_loop.run_app(&mut chip8)?;

    Ok(())
}

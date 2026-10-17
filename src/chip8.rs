use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};
use std::{env, fs};

use thiserror::Error;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::window::{Window, WindowId};

use softbuffer::{Context, Surface};

const CHIP8_WIDTH: u32 = 64;
const CHIP8_HEIGHT: u32 = 32;

const SCALED_WIDTH: u32 = CHIP8_WIDTH * 20;
const SCALED_HEIGHT: u32 = CHIP8_HEIGHT * 20;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Winit event loop error")]
    EventLoop(#[from] winit::error::EventLoopError),
    #[error("Winit Os Error")]
    Os(#[from] winit::error::OsError),
    #[error("Soft buffer error")]
    SoftBuffer(#[from] softbuffer::SoftBufferError),
    #[error("Error reading chip8 file")]
    Io(#[from] std::io::Error),
}

struct Core {
    last_time: Instant,

    ram: [u8; 4096],
    stack: [u16; 16],
    registers: [u8; 16],

    program_counter: u16,
    idx_register: u16,

    delay_timer: u8,
    sound_timer: u8,
}

impl Core {
    fn new(rom: Vec<u8>) -> Self {
        Self {
            last_time: Instant::now(),
            ram: [0; 4096],
            stack: [0; 16],
            registers: [0; 16],
            program_counter: 0,
            idx_register: 0,
            delay_timer: 0,
            sound_timer: 0,
        }
    }
}

struct App {
    window: Option<Rc<Window>>,
    surface: Option<Surface<OwnedDisplayHandle, Rc<Window>>>,

    core: Core,
}

impl App {
    fn new(core: Core) -> Self {
        Self {
            window: None,
            surface: None,
            core,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = Some(Rc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Chip8")
                        .with_inner_size(LogicalSize::new(SCALED_WIDTH, SCALED_HEIGHT))
                        .with_resizable(false),
                )
                .unwrap(),
        ));

        let context = Context::new(event_loop.owned_display_handle()).unwrap();
        self.surface =
            Some(Surface::new(&context, Rc::clone(self.window.as_ref().unwrap())).unwrap());
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::RedrawRequested => {
                let Some(surface) = &mut self.surface else {
                    eprintln!("RedrawRequested ran before resumed or after suspended");
                    return;
                };

                let size = self.window.as_ref().unwrap().inner_size();
                surface
                    .resize(
                        NonZeroU32::new(size.width).unwrap(),
                        NonZeroU32::new(size.height).unwrap(),
                    )
                    .unwrap();

                let mut buffer = surface.buffer_mut().unwrap();
                for i in 0..(buffer.width().get() * buffer.height().get()) {
                    let y = i / buffer.width().get();
                    let x = i % buffer.width().get();

                    let r = x % 255;
                    let g = y % 255;
                    let b = (x * y) % 255;

                    buffer[i as usize] = b | (g << 8) | (r << 16);
                }

                buffer.present().unwrap();
            }
            WindowEvent::CloseRequested => {
                println!("Close button pressed, exiting");
                event_loop.exit();
            }
            _ => (),
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let elapsed = self.core.last_time.elapsed();
        let dt = elapsed.as_secs_f64();
        dbg!(dt);

        self.core.last_time = Instant::now();

        self.window.as_ref().unwrap().request_redraw();
    }
}

pub fn run() -> Result<(), Error> {
    let args: Vec<String> = env::args().collect();
    let rom = fs::read(&args[1])?;

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let core = Core::new(rom);
    let mut app = App::new(core);
    event_loop.run_app(&mut app)?;

    Ok(())
}

use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::{Duration, Instant};

use thiserror::Error;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use softbuffer::{Context, Surface};

const CHIP8_WIDTH: u32 = 64;
const CHIP8_HEIGHT: u32 = 32;
const DISPLAY_SIZE: usize = (CHIP8_WIDTH * CHIP8_HEIGHT) as usize;

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
    timer_60hz: f64,

    ram: [u8; 4096],
    stack: [u16; 16],
    registers: [u8; 16],

    // keypad is left side of keyboard
    // for QWERTY:
    // 1234 - 123C
    // QWER - 456D
    // ASDF - 789E
    // ZXCV - A0BF
    keypad: [bool; 16],

    program_counter: u16,
    idx_register: u16,

    delay_timer: u8,
    sound_timer: u8,

    instruction_timer: f64,
    instructions_per_sec: u32,

    display: [u32; DISPLAY_SIZE],
}

impl Core {
    fn new(rom: Vec<u8>, instructions_per_sec: u32) -> Self {
        let mut ram = [0 as u8; 4096];
        ram[511..511 + rom.len()].copy_from_slice(&rom);

        Self {
            last_time: Instant::now(),
            timer_60hz: 0.0,
            ram: ram,
            stack: [0; 16],
            registers: [0; 16],
            keypad: [false; 16],
            program_counter: 511,
            idx_register: 0,
            delay_timer: 0,
            sound_timer: 0,
            instruction_timer: 0.0,
            instructions_per_sec: instructions_per_sec,
            display: [0; DISPLAY_SIZE],
        }
    }

    fn process_instruction(&mut self) {
        // read instruction
        // instructions are two bytes
        let pc = self.program_counter as usize;
        let instr_bytes = u16::from_be_bytes([self.ram[pc], self.ram[pc + 1]]);

        self.program_counter += 2;

        // decode instruction
        // first hex digit (4 bytes) are category of instruction
        let nibble1 = 0b1111_0000_0000_0000;
        let nibble2 = nibble1 << 4;
        let nibble3 = nibble2 << 4;
        let nibble4 = nibble3 << 4;

        let instr = (
            instr_bytes & nibble1,
            instr_bytes & nibble2,
            instr_bytes & nibble3,
            instr_bytes & nibble4,
        );

        match instr {
            // CLEAR DISPLAY: 00E0
            (0, 0, 0xE, 0) => self.display = [0; DISPLAY_SIZE],

            _ => panic!("Invalid nibble 1 in instruction"),
        };

        // run instruction
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

    fn keypad_mapping(code: KeyCode) -> Option<usize> {
        match code {
            // row 1
            KeyCode::Digit1 => Some(1),
            KeyCode::Digit2 => Some(2),
            KeyCode::Digit3 => Some(3),
            KeyCode::Digit4 => Some(12),
            // row 2
            KeyCode::KeyQ => Some(4),
            KeyCode::KeyW => Some(5),
            KeyCode::KeyE => Some(6),
            KeyCode::KeyR => Some(13),
            // row 3
            KeyCode::KeyA => Some(7),
            KeyCode::KeyS => Some(8),
            KeyCode::KeyD => Some(9),
            KeyCode::KeyF => Some(14),
            // row 4
            KeyCode::KeyZ => Some(10),
            KeyCode::KeyX => Some(0),
            KeyCode::KeyC => Some(11),
            KeyCode::KeyV => Some(15),

            _ => None,
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
                    buffer[i as usize] = self.core.display[i as usize];
                }

                buffer.present().unwrap();
            }
            WindowEvent::CloseRequested => {
                println!("Close button pressed, exiting");
                event_loop.exit();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state.is_pressed();
                let idx = match event.physical_key {
                    PhysicalKey::Code(code) => {
                        if pressed && code == KeyCode::Escape {
                            event_loop.exit();
                        }

                        App::keypad_mapping(code)
                    }
                    PhysicalKey::Unidentified(_) => {
                        eprintln!("Unidentified physical key code");

                        None
                    }
                };

                if let Some(i) = idx {
                    self.core.keypad[i] = pressed;
                }
            }
            WindowEvent::Focused(false) => self.core.keypad = [false; 16],

            _ => (),
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let elapsed = self.core.last_time.elapsed();
        let dt = elapsed.as_secs_f64();
        self.core.timer_60hz += dt;
        self.core.instruction_timer += dt;

        self.core.last_time = Instant::now();

        self.window.as_ref().unwrap().request_redraw();

        // 60hz = 1/60 = ~0.01667
        if self.core.timer_60hz >= 0.01667 {
            self.core.timer_60hz = 0.0;

            if self.core.delay_timer > 0 {
                self.core.delay_timer -= 1;
            }
            if self.core.sound_timer > 0 {
                self.core.sound_timer -= 1;
            }
        }

        if self.core.instruction_timer >= (1.0 / self.core.instructions_per_sec as f64) {
            self.core.instruction_timer = 0.0;

            self.core.process_instruction();
        }
    }
}

pub fn run(rom: Vec<u8>, instructions_per_sec: u32) -> Result<(), Error> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let core = Core::new(rom, instructions_per_sec);
    let mut app = App::new(core);
    event_loop.run_app(&mut app)?;

    Ok(())
}

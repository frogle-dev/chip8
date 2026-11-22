use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use thiserror::Error;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, OwnedDisplayHandle};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use softbuffer::{Context, Surface};

const CHIP8_WIDTH: usize = 64;
const CHIP8_HEIGHT: usize = 32;

const SCALE: usize = 20;
const SCALED_WIDTH: usize = CHIP8_WIDTH * SCALE;
const SCALED_HEIGHT: usize = CHIP8_HEIGHT * SCALE;

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

    display: [u64; CHIP8_HEIGHT as usize],
    // display: [bool; DISPLAY_SIZE],
}

impl Core {
    fn new(rom: Vec<u8>, instructions_per_sec: u32) -> Self {
        const FONT: [u8; 80] = [
            0xF0, 0x90, 0x90, 0x90, 0xF0, // 0
            0x20, 0x60, 0x20, 0x20, 0x70, // 1
            0xF0, 0x10, 0xF0, 0x80, 0xF0, // 2
            0xF0, 0x10, 0xF0, 0x10, 0xF0, // 3
            0x90, 0x90, 0xF0, 0x10, 0x10, // 4
            0xF0, 0x80, 0xF0, 0x10, 0xF0, // 5
            0xF0, 0x80, 0xF0, 0x90, 0xF0, // 6
            0xF0, 0x10, 0x20, 0x40, 0x40, // 7
            0xF0, 0x90, 0xF0, 0x90, 0xF0, // 8
            0xF0, 0x90, 0xF0, 0x10, 0xF0, // 9
            0xF0, 0x90, 0xF0, 0x90, 0x90, // A
            0xE0, 0x90, 0xE0, 0x90, 0xE0, // B
            0xF0, 0x80, 0x80, 0x80, 0xF0, // C
            0xE0, 0x90, 0x90, 0x90, 0xE0, // D
            0xF0, 0x80, 0xF0, 0x80, 0xF0, // E
            0xF0, 0x80, 0xF0, 0x80, 0x80, // F
        ];

        let mut ram = [0 as u8; 4096];
        ram[..FONT.len()].copy_from_slice(&FONT);

        ram[0x200..0x200 + rom.len()].copy_from_slice(&rom);

        Self {
            last_time: Instant::now(),
            timer_60hz: 0.0,
            ram: ram,
            stack: [0; 16],
            registers: [0; 16],
            keypad: [false; 16],
            program_counter: 0x200,
            idx_register: 0,
            delay_timer: 0,
            sound_timer: 0,
            instruction_timer: 0.0,
            instructions_per_sec: instructions_per_sec,
            display: [0; CHIP8_HEIGHT as usize],
        }
    }

    fn process_instruction(&mut self) {
        // read instruction
        // instructions are two bytes
        let pc = self.program_counter as usize;
        let instr_bytes = u16::from_be_bytes([self.ram[pc], self.ram[pc + 1]]);

        self.program_counter += 2;

        // decode instruction
        let instr = (
            (instr_bytes >> 12) as u8 & 0xF,
            (instr_bytes >> 8) as u8 & 0xF,
            (instr_bytes >> 4) as u8 & 0xF,
            instr_bytes as u8 & 0xF,
        );

        match instr {
            // CLEAR DISPLAY: 00E0
            (0, 0, 0xE, 0) => {
                self.display = [0; CHIP8_HEIGHT as usize];
            }
            // JUMP: 1NNN
            (1, n1, n2, n3) => {
                let addr = (n1 as u16) << 8 | (n2 as u16) << 4 | n3 as u16;

                self.program_counter = addr;
            }
            // SET: 6XNN
            (6, reg, n1, n2) => {
                let val = ((n1 as u16) << 4 | n2 as u16) as u8;

                self.registers[reg as usize] = val;
            }
            // ADD: 7XNN
            (7, reg_idx, n1, n2) => {
                let val = n1 << 4 | n2;

                self.registers[reg_idx as usize] =
                    match self.registers[reg_idx as usize].checked_add(val) {
                        Some(res) => res,
                        None => panic!("Error: add instruction integer overflow"),
                    };
            }
            // SET INDEX: ANNN
            (0xA, n1, n2, n3) => {
                let val = (n1 as u16) << 8 | (n2 as u16) << 4 | n3 as u16;

                self.idx_register = val;
            }
            // DISPLAY: DXYN
            (0xD, reg_idx1, reg_idx2, rows) => {
                // coordinates will wrap
                let x_pos = self.registers[reg_idx1 as usize] as usize % CHIP8_WIDTH;
                let y_pos = self.registers[reg_idx2 as usize] as usize % CHIP8_HEIGHT;

                self.registers[0xF] = 0;

                for y in 0..rows as usize {
                    let row = y_pos + y;

                    if row >= CHIP8_HEIGHT {
                        break;
                    }

                    // sprites are 8 pixels wide
                    let row_idx = self.idx_register as usize + y;
                    // reverse bits needed since leftmost digit is highest bit
                    let sprite_row = self.ram[row_idx].reverse_bits();
                    let sprite_pixels = (sprite_row as u64) << x_pos;

                    self.display[row] ^= sprite_pixels;
                }
            }
            _ => println!("Invalid nibble 1 in instruction"),
        };
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
                        .with_inner_size(LogicalSize::new(
                            SCALED_WIDTH as u32,
                            SCALED_HEIGHT as u32,
                        ))
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
                for (y, pixel_row) in self.core.display.iter().enumerate() {
                    for x in 0..64_usize {
                        let bit = (pixel_row >> x) & 1;
                        let color = (bit as u32) * u32::MAX;

                        for s in 0..SCALE {
                            let scaled_x = x * SCALE;
                            let scaled_y = y * SCALE;

                            let idx = (scaled_y + s) * SCALED_WIDTH + scaled_x;
                            let row = &mut buffer[idx..idx + SCALE];
                            row.fill(color);
                        }
                    }
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

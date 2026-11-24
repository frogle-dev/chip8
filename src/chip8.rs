use std::num::NonZeroU32;
use std::rc::Rc;
use std::time::Instant;

use rand::RngExt;

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

fn instr_nn(n1: u8, n2: u8) -> u8 {
    n1 << 4 | n2 as u8
}

fn instr_nnn(n1: u8, n2: u8, n3: u8) -> u16 {
    (n1 as u16) << 8 | (n2 as u16) << 4 | n3 as u16
}

struct CallStack {
    arr: [u16; 16],
    last_idx: usize,
}

impl CallStack {
    fn new() -> Self {
        Self {
            arr: [0; 16],
            last_idx: 0,
        }
    }

    /// # Panics
    fn push(&mut self, val: u16) {
        if self.last_idx >= self.arr.len() {
            panic!("Push overflows CallStack size");
        }

        self.arr[self.last_idx] = val;

        self.last_idx += 1;
    }

    /// # Panics
    fn pop(&mut self) -> u16 {
        if self.last_idx <= 0 {
            panic!("Pop on empty CallStack");
        }

        self.last_idx -= 1;

        self.arr[self.last_idx]
    }
}

struct Core {
    last_time: Instant,
    timer_60hz: f64,

    ram: [u8; 4096],
    call_stack: CallStack,

    // register 0xF is the flag register, holds things like carries and borrows and collisions
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
            call_stack: CallStack::new(),
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

        // instructions are two bytes
        self.program_counter += 2;

        // decode instruction
        let instr = (
            (instr_bytes >> 12) as u8 & 0xF,
            (instr_bytes >> 8) as u8 & 0xF,
            (instr_bytes >> 4) as u8 & 0xF,
            instr_bytes as u8 & 0xF,
        );

        match instr {
            // JUMP: jump NNN: 1NNN
            (1, n1, n2, n3) => {
                let addr = instr_nnn(n1, n2, n3);

                self.program_counter = addr;
            }
            // OFFSET JUMP: jump NNN + V0: BNNN
            (0xB, n1, n2, n3) => {
                let addr = instr_nnn(n1, n2, n3);

                self.program_counter = addr + self.registers[0] as u16;
            }
            // SET: 6XNN
            (6, reg, n1, n2) => {
                let val = instr_nn(n1, n2);

                self.registers[reg as usize] = val;
            }
            // ADD: 7XNN
            (7, reg_idx, n1, n2) => {
                let val = instr_nn(n1, n2);

                let (res, _) = self.registers[reg_idx as usize].overflowing_add(val);
                self.registers[reg_idx as usize] = res;
            }
            // SET INDEX: ANNN
            (0xA, n1, n2, n3) => {
                let val = instr_nnn(n1, n2, n3);

                self.idx_register = val;
            }
            // ADD TO INDEX: FX1E
            (0xF, x, 1, 0xE) => {
                self.idx_register += self.registers[x as usize] as u16;
            }
            // RANDOM: CXNN
            (0xC, x1, n1, n2) => {
                let mut rng = rand::rng();
                self.registers[x1 as usize] = rng.random::<u8>() & instr_nn(n1, n2);
            }
            // FONT CHARACTER: idx reg = addr of hex character in vx FX29
            (0xF, x, 2, 9) => {
                self.idx_register = self.registers[x as usize] as u16 * 5;
            }
            // BINARY CODED DECIMAL CONVERSION: each digit of the number in vx will be stored
            // consecutively in memory at idx reg: FX33
            (0xF, x, 3, 3) => {
                let num = self.registers[x as usize];

                let hundreds = num / 100;
                let tens = (num / 10) % 10;
                let ones = num % 10;

                self.ram[self.idx_register as usize + 0] = hundreds;
                self.ram[self.idx_register as usize + 1] = tens;
                self.ram[self.idx_register as usize + 2] = ones;
            }
            // STORE MEMORY: V0..=VX will be stored from I..=(I + X) in ram
            (0xF, x, 5, 5) => {
                let i = self.idx_register as usize;
                self.ram[i..=i + x as usize].copy_from_slice(&self.registers[..=x as usize]);
            }
            // LOAD MEMORY: data at addr I..=(I + X) will be loaded into V0..=VX
            (0xF, x, 6, 5) => {
                let i = self.idx_register as usize;
                self.registers[..=x as usize].copy_from_slice(&self.ram[i..=i + x as usize]);
            }

            // --------- SUBROUTINES ---------

            // RETURN FROM SUBROUTINE: 00EE
            (0, 0, 0xE, 0xE) => {
                self.program_counter = self.call_stack.pop();
            }
            // CALL SUBROUTINE: 2NNN
            (2, n1, n2, n3) => {
                let addr = instr_nnn(n1, n2, n3);

                self.call_stack.push(self.program_counter);

                self.program_counter = addr;
            }

            // --------- CONDITIONALS ---------

            // SKIP IF EQUAL: 3XNN
            (3, x, n1, n2) => {
                if self.registers[x as usize] == instr_nn(n1, n2) {
                    self.program_counter += 2;
                }
            }
            // SKIP IF NOT  EQUAL: 4XNN
            (4, x, n1, n2) => {
                if self.registers[x as usize] != instr_nn(n1, n2) {
                    self.program_counter += 2;
                }
            }
            // REG SKIP IF EQUAL: 5XY0
            (5, x, y, 0) => {
                if self.registers[x as usize] == self.registers[y as usize] {
                    self.program_counter += 2;
                }
            }
            // REG SKIP IF NOT EQUAL 9XY0
            (9, x, y, 0) => {
                if self.registers[x as usize] != self.registers[y as usize] {
                    self.program_counter += 2;
                }
            }

            // --------- INPUT ---------

            // SKIP IF KEY VX PRESSED: EX9E
            (0xE, x, 9, 0xE) => {
                if self.keypad[self.registers[x as usize] as usize] {
                    self.program_counter += 2;
                }
            }
            // SKIP IF KEY VX NOT PRESSED: EXA1
            (0xE, x, 0xA, 1) => {
                if !self.keypad[self.registers[x as usize] as usize] {
                    self.program_counter += 2;
                }
            }
            // WAIT FOR KEY: FX0A
            (0xF, x, 0, 0xA) => {
                if let Some(i) = self.keypad.iter().position(|&x| x) {
                    self.registers[x as usize] = i as u8;
                } else {
                    self.program_counter -= 2;
                }
            }

            // --------- TIMERS ---------

            //
            (0xF, x, 0, 7) => {
                self.registers[x as usize] = self.delay_timer;
            }
            (0xF, x, 1, 5) => {
                self.delay_timer = self.registers[x as usize];
            }
            (0xF, x, 1, 8) => {
                self.sound_timer = self.registers[x as usize];
            }

            // --------- LOGIC AND MATH ---------

            // SET: VX = VY: 8XY0
            (8, x, y, 0) => {
                self.registers[x as usize] = self.registers[y as usize];
            }
            // BITWISE OR: VX |= VY: 8XY1
            (8, x, y, 1) => {
                self.registers[x as usize] |= self.registers[y as usize];
            }
            // BITWISE AND: VX &= VY: 8XY2
            (8, x, y, 2) => {
                self.registers[x as usize] &= self.registers[y as usize];
            }
            // BITWISE XOR: VX ^= VY: 8XY3
            (8, x, y, 3) => {
                self.registers[x as usize] ^= self.registers[y as usize];
            }
            // ADD: VX += VY: 8XY4
            (8, x, y, 4) => {
                let (res, overflows) =
                    self.registers[x as usize].overflowing_add(self.registers[y as usize]);
                self.registers[x as usize] = res;

                self.registers[0xF] = if overflows { 1 } else { 0 };
            }
            // SUBTRACT: VX -= VY: 8XY5
            (8, x, y, 5) => {
                let (res, overflows) =
                    self.registers[x as usize].overflowing_sub(self.registers[y as usize]);
                self.registers[x as usize] = res;

                self.registers[0xF] = if !overflows { 1 } else { 0 };
            }
            // SUBTRACT: VX -= VY: 8XY7
            (8, x, y, 7) => {
                let (res, overflows) =
                    self.registers[y as usize].overflowing_sub(self.registers[x as usize]);
                self.registers[x as usize] = res;

                self.registers[0xF] = if !overflows { 1 } else { 0 };
            }
            // BITWISE RIGHT SHIFT: VX >>= VY: 8XY6
            (8, x, y, 6) => {
                let vy = self.registers[y as usize];

                // VF is the bit shifted out
                self.registers[x as usize] = vy >> 1;
                self.registers[0xF] = vy & 1;
            }
            // BITWISE LEFT SHIFT: VX <<= VY: 8XYE
            (8, x, y, 0xE) => {
                let vy = self.registers[y as usize];

                // VF is the bit shifted out
                self.registers[x as usize] = vy << 1;
                self.registers[0xF] = vy >> (u8::BITS - 1);
            }

            // --------- DISPLAY ---------

            // CLEAR DISPLAY: 00E0
            (0, 0, 0xE, 0) => {
                self.display = [0; CHIP8_HEIGHT as usize];
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

                    // ch8 collision detection, if xor turns any pixels off VF register is 1
                    if (self.display[row] & sprite_pixels) != 0 {
                        self.registers[0xF] = 1;
                    }

                    self.display[row] ^= sprite_pixels;
                }
            }
            (a, b, c, d) => panic!("Invalid instruction: {:X}{:X}{:X}{:X}", a, b, c, d),
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

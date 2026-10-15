mod chip8;

fn main() {
    if let Err(err) = chip8::init() {
        eprintln!("Error intializing Chip8: {}", err);
    }
}

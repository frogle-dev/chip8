mod chip8;

fn main() {
    if let Err(err) = chip8::run() {
        eprintln!("Error intializing Chip8: {}", err);
    }
}

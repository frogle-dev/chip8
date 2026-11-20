mod chip8;

use std::{env, fs};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        panic!("Error: No ch8 program sent in");
    }

    let rom = match fs::read(&args[1]) {
        Ok(buf) => buf,
        Err(err) => panic!("Error: reading ch8 file command line argument: {}", err),
    };

    if let Err(err) = chip8::run(rom) {
        eprintln!("Error: intializing Chip8: {}", err);
    }
}

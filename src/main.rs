mod chip8;

use std::{env, fs};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        panic!("Error: 3 arguments not sent in, needs ch8 program and instructions per second");
    }

    let rom = match fs::read(&args[1]) {
        Ok(buf) => buf,
        Err(err) => panic!(
            "Error: 1st argument: reading ch8 file command line argument: {}",
            err
        ),
    };

    let instr_per_sec = match args[2].parse::<u32>() {
        Ok(num) => num,
        Err(err) => panic!(
            "Error: 2nd argument: parsing instructions per second: {}",
            err
        ),
    };

    if let Err(err) = chip8::run(rom, instr_per_sec) {
        eprintln!("Error: intializing Chip8: {}", err);
    }
}

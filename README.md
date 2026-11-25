### Keypad:

keypad is left side of keyboard

this emulator uses scancodes, so the same positions will work for any keyboard type

as an example on QWERTY the keys would be:

1234 - 123C

QWER - 456D

ASDF - 789E

ZXCV - A0BF

### Build instructions:

git clone this repo and then in the root directory of it run:

```cargo build -r```

the binary will be:

```target/release/chip8```

### Usage:

```chip8 [ch8 rom] [instructions per second]```

ex: (you can find ch8 roms online, there is a Pong.ch8 in the root of this repo)

```chip8 Pong.ch8 700```

the Pong ch8 rom in this repo uses

1 and 4 HEX keys (1 and Q keys on qwerty this emulator) for the left paddle

C and D hex keys (4 and R keys on qwerty) for the right paddle 

<img width="1275" height="637" alt="image" src="https://github.com/user-attachments/assets/12e3cc30-2d17-4545-9357-a746506cefc4" />


The Pong.ch8 and Tetris.ch8 roms are from https://github.com/netpro2k/Chip8/blob/master/games/Pong.ch8

test_opcode.ch8 is from https://github.com/corax89/chip8-test-rom

IBM_Logo.ch8 is from https://github.com/loktar00/chip8/blob/master/roms/IBM%20Logo.ch8

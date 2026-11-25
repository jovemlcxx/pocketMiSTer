//! nestest.nes em modo automático ($C000), instrução por instrução contra o nestest.log
//! (PC, A, X, Y, P, SP e ciclos), como o `nestest_check` em C.

use nes_core::{Bus, Cpu};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/roms");

/// NROM-128: 2 KB de RAM espelhados e 16 KB de PRG espelhados em $8000-$FFFF. O resto devolve
/// o último valor do barramento.
struct Nrom {
    ram: [u8; 0x800],
    prg: Vec<u8>,
    open_bus: u8,
}

impl Nrom {
    fn tick(cpu: &mut Cpu) {
        cpu.cycles += 1;
        cpu.end_cycle(false);
    }
}

impl Bus for Nrom {
    fn read(&mut self, cpu: &mut Cpu, addr: u16) -> u8 {
        let v = match addr {
            0x0000..=0x1FFF => self.ram[usize::from(addr) & 0x7FF],
            0x8000..=0xFFFF => self.prg[usize::from(addr) & (self.prg.len() - 1)],
            _ => self.open_bus,
        };
        self.open_bus = v;
        Self::tick(cpu);
        v
    }
    fn write(&mut self, cpu: &mut Cpu, addr: u16, v: u8) {
        self.open_bus = v;
        if addr < 0x2000 {
            self.ram[usize::from(addr) & 0x7FF] = v;
        }
        Self::tick(cpu);
    }
}

fn hex(line: &str, key: &str) -> u32 {
    let at = line.find(key).expect("campo no log") + key.len();
    let s = &line[at..];
    let end = s
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(s.len());
    if key == "CYC:" {
        s[..end].parse().expect("ciclos")
    } else {
        u32::from_str_radix(&s[..end], 16).expect("hex")
    }
}

#[test]
fn nestest_matches_log() {
    let rom = std::fs::read(format!("{DIR}/nestest.nes")).expect("nestest.nes");
    let log = std::fs::read_to_string(format!("{DIR}/nestest.log")).expect("nestest.log");
    assert_eq!(&rom[..4], b"NES\x1A");
    let prg = rom[16..16 + usize::from(rom[4]) * 0x4000].to_vec();
    let mut bus = Nrom {
        ram: [0; 0x800],
        prg,
        open_bus: 0,
    };
    let mut cpu = Cpu::default();
    cpu.reset(&mut bus);
    cpu.pc = 0xC000;

    let mut n = 0;
    for line in log.lines() {
        let pc = u32::from_str_radix(&line[..4], 16).expect("pc");
        let want = (
            pc,
            hex(line, "A:"),
            hex(line, "X:"),
            hex(line, "Y:"),
            hex(line, "P:"),
            hex(line, "SP:"),
            hex(line, "CYC:"),
        );
        let got = (
            u32::from(cpu.pc),
            u32::from(cpu.a),
            u32::from(cpu.x),
            u32::from(cpu.y),
            u32::from(cpu.get_p(false)),
            u32::from(cpu.s),
            cpu.cycles,
        );
        assert_eq!(got, want, "diferença na instrução {}: {line}", n + 1);
        cpu.step(&mut bus);
        n += 1;
    }
    assert_eq!(n, 8991);
    // $02/$03 = 0: nenhum teste do nestest falhou
    assert_eq!((bus.ram[2], bus.ram[3]), (0, 0));
}

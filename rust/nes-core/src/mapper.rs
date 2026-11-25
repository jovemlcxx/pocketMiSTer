//! Mappers 0 (NROM), 1 (MMC1), 2 (UxROM), 3 (CNROM), 4 (MMC3), 24/26 (VRC6, sem som) e 69
//! (Sunsoft FME-7, sem som). Referências: NESdev Wiki e os mappers do Mesen.
//! Porte de `firmware/src/nes/mapper.c`. Diferenças de forma, sem mudar o comportamento:
//! - as páginas do CPU de $6000-$FFFF ficam aqui como deslocamentos ([`Mapper::prg`],
//!   [`Mapper::wram`]) em vez de ponteiros na tabela do sistema;
//! - a linha de IRQ do CPU é sempre igual a [`Mapper::irq_flag`] no C (toda mudança de uma
//!   acompanha a outra), então o sistema copia a flag depois de cada chamada.

use crate::Ppu;

/// O que aparece numa página de 2 KB de $6000-$7FFF.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wram {
    /// PRG-RAM, a partir deste deslocamento (leitura e escrita)
    Ram(u16),
    /// PRG-ROM, a partir deste deslocamento (só leitura)
    Rom(u32),
    /// nada: o barramento aberto responde
    Open,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mirror {
    Vertical,
    Horizontal,
    SingleA,
    SingleB,
}

#[derive(Clone, Debug)]
pub struct Mapper {
    pub id: u8,
    prg_size: u32,
    chr_ram_size: u32,
    vertical_mirroring: bool,
    four_screen: bool,
    /// deslocamento na PRG-ROM de cada página de 2 KB de $8000-$FFFF
    pub prg: [u32; 16],
    /// páginas de 2 KB de $6000-$7FFF
    pub wram: [Wram; 4],

    /// registradores de banco (uso depende do mapper)
    pub reg: [u8; 16],
    /// MMC3: $8000; FME-7: comando
    pub select: u8,
    /// MMC1: registrador serial
    pub shift: u8,
    pub shift_n: u8,
    /// MMC1: escritas em ciclos seguidos são ignoradas
    pub last_write_cycle: u32,
    /// MMC1: $8000-$9FFF
    pub control: u8,
    pub irq_latch: u8,
    pub irq_counter: u8,
    pub irq_reload: bool,
    pub irq_enabled: bool,
    pub irq_flag: bool,
    /// FME-7
    pub irq_counter16: u16,
    pub irq_count_enabled: bool,
    /// VRC: modo linha = a cada 341/3 ciclos de CPU
    pub vrc_prescaler: i16,
    pub vrc_cycle_mode: bool,
    pub vrc_enable_after_ack: bool,
    /// true só para VRC6 (24/26) e FME-7 (69): chamar [`Mapper::cpu_cycle`] a cada ciclo
    pub has_cpu_cycle: bool,
}

impl Mapper {
    pub fn supported(id: u8) -> bool {
        matches!(id, 0 | 1 | 2 | 3 | 4 | 24 | 26 | 69)
    }

    /// Mapper ligado (o `mapper_init` do C). A PPU recebe as páginas de CHR e de nomes e o
    /// gancho de A12 (MMC3). `chr_ram_size` só vale quando a PPU não tem CHR-ROM.
    pub fn new(
        id: u8,
        prg_size: u32,
        chr_ram_size: u32,
        vertical_mirroring: bool,
        four_screen: bool,
        ppu: &mut Ppu,
    ) -> Self {
        let mut m = Mapper {
            id,
            prg_size,
            chr_ram_size,
            vertical_mirroring,
            four_screen,
            prg: [0; 16],
            wram: [
                Wram::Ram(0),
                Wram::Ram(0x800),
                Wram::Ram(0x1000),
                Wram::Ram(0x1800),
            ],
            reg: [0; 16],
            select: 0,
            shift: 0,
            shift_n: 0,
            last_write_cycle: 0,
            control: 0,
            irq_latch: 0,
            irq_counter: 0,
            irq_reload: false,
            irq_enabled: false,
            irq_flag: false,
            irq_counter16: 0,
            irq_count_enabled: false,
            vrc_prescaler: 0,
            vrc_cycle_mode: false,
            vrc_enable_after_ack: false,
            has_cpu_cycle: matches!(id, 24 | 26 | 69),
        };
        ppu.chr_writable = ppu.chr_rom.is_empty();
        ppu.a12_hook = false;
        m.mirroring(
            ppu,
            if m.vertical_mirroring {
                Mirror::Vertical
            } else {
                Mirror::Horizontal
            },
        );
        m.prg_16k(0, 0);
        m.prg_16k(1, -1);
        m.chr_8k(ppu, 0);
        match id {
            1 => {
                m.control = 0x0C;
                m.mmc1_update(ppu);
            }
            4 => {
                m.reg[6] = 0;
                m.reg[7] = 1;
                m.mmc3_update(ppu);
                ppu.a12_hook = true;
            }
            69 => m.prg_8k(3, -1),
            _ => {}
        }
        m
    }

    // ---- utilitários de banco ----
    /// slot 0-3 = $8000/$A000/$C000/$E000
    fn prg_8k(&mut self, slot: usize, bank: i32) {
        let count = (self.prg_size / 0x2000) as i32;
        let mut bank = bank;
        if bank < 0 {
            bank += count;
        }
        let base = (bank % count) as u32 * 0x2000;
        for i in 0..4 {
            self.prg[slot * 4 + i] = base + i as u32 * 0x800;
        }
    }
    /// slot 0-1 = $8000/$C000
    fn prg_16k(&mut self, slot: usize, bank: i32) {
        let count = (self.prg_size / 0x4000) as i32;
        let mut bank = bank;
        if bank < 0 {
            bank += count;
        }
        bank %= count;
        self.prg_8k(slot * 2, bank * 2);
        self.prg_8k(slot * 2 + 1, bank * 2 + 1);
    }
    fn prg_32k(&mut self, bank: i32) {
        self.prg_16k(0, bank * 2);
        self.prg_16k(1, bank * 2 + 1);
    }

    fn chr_1k(&self, ppu: &mut Ppu, slot: usize, bank: i32) {
        let size = if ppu.chr_rom.is_empty() {
            self.chr_ram_size
        } else {
            ppu.chr_rom.len() as u32
        };
        let count = (size / 0x400) as i32;
        ppu.set_chr(slot, (bank % count) as u32 * 0x400);
    }
    fn chr_2k(&self, ppu: &mut Ppu, slot: usize, bank: i32) {
        self.chr_1k(ppu, slot, bank * 2);
        self.chr_1k(ppu, slot + 1, bank * 2 + 1);
    }
    fn chr_4k(&self, ppu: &mut Ppu, slot: usize, bank: i32) {
        for i in 0..4 {
            self.chr_1k(ppu, slot + i, bank * 4 + i as i32);
        }
    }
    fn chr_8k(&self, ppu: &mut Ppu, bank: i32) {
        for i in 0..8 {
            self.chr_1k(ppu, i, bank * 8 + i as i32);
        }
    }

    fn mirroring(&self, ppu: &mut Ppu, mode: Mirror) {
        ppu.nt = match mode {
            Mirror::Vertical => [0, 0x400, 0, 0x400],
            Mirror::Horizontal => [0, 0, 0x400, 0x400],
            Mirror::SingleA => [0; 4],
            Mirror::SingleB => [0x400; 4],
        };
    }

    // ---- MMC1 ----
    fn mmc1_update(&mut self, ppu: &mut Ppu) {
        const MIR: [Mirror; 4] = [
            Mirror::SingleA,
            Mirror::SingleB,
            Mirror::Vertical,
            Mirror::Horizontal,
        ];
        self.mirroring(ppu, MIR[usize::from(self.control & 3)]);
        let prg = i32::from(self.reg[2] & 0x0F);
        match (self.control >> 2) & 3 {
            0 | 1 => self.prg_32k(prg >> 1),
            2 => {
                self.prg_16k(0, 0);
                self.prg_16k(1, prg);
            }
            _ => {
                self.prg_16k(0, prg);
                self.prg_16k(1, -1);
            }
        }
        if self.control & 0x10 != 0 {
            self.chr_4k(ppu, 0, i32::from(self.reg[0]));
            self.chr_4k(ppu, 4, i32::from(self.reg[1]));
        } else {
            self.chr_8k(ppu, i32::from(self.reg[0] >> 1));
        }
    }

    fn mmc1_write(&mut self, ppu: &mut Ppu, addr: u16, v: u8, cycles: u32) {
        let consecutive = cycles.wrapping_sub(self.last_write_cycle) < 2; // escritas duplas de RMW: só a primeira vale
        self.last_write_cycle = cycles;
        if v & 0x80 != 0 {
            self.shift = 0;
            self.shift_n = 0;
            self.control |= 0x0C;
            self.mmc1_update(ppu);
            return;
        }
        if consecutive {
            return;
        }
        self.shift |= (v & 1) << self.shift_n;
        self.shift_n += 1;
        if self.shift_n < 5 {
            return;
        }
        match (addr >> 13) & 3 {
            0 => self.control = self.shift,
            r => self.reg[usize::from(r) - 1] = self.shift,
        }
        self.shift = 0;
        self.shift_n = 0;
        self.mmc1_update(ppu);
    }

    // ---- MMC3 ----
    fn mmc3_update(&mut self, ppu: &mut Ppu) {
        if self.select & 0x40 != 0 {
            self.prg_8k(0, -2);
            self.prg_8k(2, i32::from(self.reg[6]));
        } else {
            self.prg_8k(0, i32::from(self.reg[6]));
            self.prg_8k(2, -2);
        }
        self.prg_8k(1, i32::from(self.reg[7]));
        self.prg_8k(3, -1);
        let inv = if self.select & 0x80 != 0 { 4 } else { 0 }; // inversão de A12: troca as metades da CHR
        self.chr_2k(ppu, inv, i32::from(self.reg[0] >> 1));
        self.chr_2k(ppu, 2 ^ inv, i32::from(self.reg[1] >> 1));
        for i in 0..4 {
            self.chr_1k(ppu, (4 + i) ^ inv, i32::from(self.reg[2 + i]));
        }
    }

    fn mmc3_write(&mut self, ppu: &mut Ppu, addr: u16, v: u8) {
        match addr & 0xE001 {
            0x8000 => {
                self.select = v;
                self.mmc3_update(ppu);
            }
            0x8001 => {
                self.reg[usize::from(self.select & 7)] = v;
                self.mmc3_update(ppu);
            }
            0xA000 => {
                if !self.four_screen {
                    self.mirroring(
                        ppu,
                        if v & 1 != 0 {
                            Mirror::Horizontal
                        } else {
                            Mirror::Vertical
                        },
                    );
                }
            }
            0xC000 => self.irq_latch = v,
            0xC001 => {
                self.irq_counter = 0;
                self.irq_reload = true;
            }
            0xE000 => {
                self.irq_enabled = false;
                self.irq_flag = false;
            }
            0xE001 => self.irq_enabled = true,
            _ => {} // $A001: proteção da PRG-RAM, ignorada
        }
    }

    /// Borda de subida de A12 na PPU (filtrada): o MMC3 conta uma linha.
    pub fn a12_rise(&mut self) {
        if self.irq_counter == 0 || self.irq_reload {
            self.irq_counter = self.irq_latch;
            self.irq_reload = false;
        } else {
            self.irq_counter -= 1;
        }
        if self.irq_counter == 0 && self.irq_enabled {
            self.irq_flag = true;
        }
    }

    // ---- Sunsoft FME-7 ----
    fn fme7_map_6000(&mut self) {
        let r = self.reg[8];
        let count = self.prg_size / 0x2000;
        for (i, page) in self.wram.iter_mut().enumerate() {
            *page = if r & 0x40 != 0 {
                if r & 0x80 != 0 {
                    Wram::Ram(i as u16 * 0x800)
                } else {
                    Wram::Open
                }
            } else {
                Wram::Rom(u32::from(r & 0x3F) % count * 0x2000 + i as u32 * 0x800)
            };
        }
    }

    fn fme7_write(&mut self, ppu: &mut Ppu, addr: u16, v: u8) {
        if addr < 0x8000 {
            return; // $6000 em modo ROM: escrita sem efeito
        }
        if addr < 0xA000 {
            self.select = v & 0x0F;
            return;
        }
        if addr >= 0xC000 {
            return; // $C000-$FFFF: som 5B, ainda não implementado
        }
        match self.select {
            c @ 0..8 => self.chr_1k(ppu, usize::from(c), i32::from(v)),
            8 => {
                self.reg[8] = v;
                self.fme7_map_6000();
            }
            c @ 9..12 => self.prg_8k(usize::from(c - 9), i32::from(v & 0x3F)),
            12 => {
                const MIR: [Mirror; 4] = [
                    Mirror::Vertical,
                    Mirror::Horizontal,
                    Mirror::SingleA,
                    Mirror::SingleB,
                ];
                self.mirroring(ppu, MIR[usize::from(v & 3)]);
            }
            13 => {
                self.irq_enabled = v & 1 != 0;
                self.irq_count_enabled = v & 0x80 != 0;
                self.irq_flag = false;
            }
            14 => self.irq_counter16 = (self.irq_counter16 & 0xFF00) | u16::from(v),
            _ => self.irq_counter16 = (self.irq_counter16 & 0x00FF) | (u16::from(v) << 8),
        }
    }

    // ---- Konami VRC6 (24 = VRC6a, 26 = VRC6b com A0/A1 trocados) ----
    fn vrc6_write(&mut self, ppu: &mut Ppu, addr: u16, v: u8) {
        if addr < 0x8000 {
            return;
        }
        let addr = if self.id == 26 {
            (addr & 0xFFFC) | ((addr & 1) << 1) | ((addr & 2) >> 1)
        } else {
            addr
        };
        match addr & 0xF003 {
            0x8000..=0x8003 => self.prg_16k(0, i32::from(v & 0x0F)),
            0xC000..=0xC003 => self.prg_8k(2, i32::from(v & 0x1F)),
            0xB003 => {
                // só o modo de CHR 0 (bancos de 1 KB), o que os jogos da Konami usam
                const MIR: [Mirror; 4] = [
                    Mirror::Vertical,
                    Mirror::Horizontal,
                    Mirror::SingleA,
                    Mirror::SingleB,
                ];
                self.mirroring(ppu, MIR[usize::from((v >> 2) & 3)]);
            }
            0xD000..=0xD003 => self.chr_1k(ppu, usize::from(addr & 3), i32::from(v)),
            0xE000..=0xE003 => self.chr_1k(ppu, 4 + usize::from(addr & 3), i32::from(v)),
            0xF000 => self.irq_latch = v,
            0xF001 => {
                self.vrc_enable_after_ack = v & 1 != 0;
                self.irq_enabled = v & 2 != 0;
                self.vrc_cycle_mode = v & 4 != 0;
                if self.irq_enabled {
                    self.irq_counter = self.irq_latch;
                    self.vrc_prescaler = 341;
                }
                self.irq_flag = false;
            }
            0xF002 => {
                self.irq_enabled = self.vrc_enable_after_ack;
                self.irq_flag = false;
            }
            _ => {} // $9000-$B002: som do VRC6, ainda não implementado
        }
    }

    /// `k` clocks do contador do VRC: ao passar de $FF recarrega o latch e liga a IRQ.
    fn vrc_irq_clocks(&mut self, k: u32) {
        let s = 0x100 - u32::from(self.irq_counter); // clocks até o que recarrega
        if k >= s {
            self.irq_flag = true;
            let period = 0x100 - u32::from(self.irq_latch);
            self.irq_counter = self.irq_latch + ((k - s) % period) as u8;
        } else {
            self.irq_counter += k as u8;
        }
    }

    /// Escrita do CPU em $6000-$FFFF numa página sem RAM. `cycles` é o contador de ciclos do CPU
    /// (o MMC1 ignora a 2ª escrita de um read-modify-write).
    pub fn write(&mut self, ppu: &mut Ppu, addr: u16, v: u8, cycles: u32) {
        match self.id {
            1 if addr >= 0x8000 => self.mmc1_write(ppu, addr, v, cycles),
            2 if addr >= 0x8000 => self.prg_16k(0, i32::from(v & 0x0F)),
            3 if addr >= 0x8000 => self.chr_8k(ppu, i32::from(v & 3)),
            4 if addr >= 0x8000 => self.mmc3_write(ppu, addr, v),
            24 | 26 => self.vrc6_write(ppu, addr, v),
            69 => self.fme7_write(ppu, addr, v),
            _ => {}
        }
    }

    /// Um ciclo de CPU (só quando `has_cpu_cycle`).
    pub fn cpu_cycle(&mut self) {
        self.cpu_cycles(1);
    }

    /// `n` ciclos de CPU de uma vez: o mesmo que `cpu_cycle` chamado `n` vezes. O sistema só
    /// chama isto quando a IRQ prevista chega ou antes de uma escrita no mapper.
    pub fn cpu_cycles(&mut self, n: u32) {
        if n == 0 {
            return;
        }
        if self.id == 69 {
            if !self.irq_count_enabled {
                return;
            }
            // passa por 0 (e vira $FFFF) no tick em que o contador valia 0
            if n > u32::from(self.irq_counter16) && self.irq_enabled {
                self.irq_flag = true;
            }
            self.irq_counter16 = self.irq_counter16.wrapping_sub(n as u16);
        } else if self.id == 24 || self.id == 26 {
            if !self.irq_enabled {
                return;
            }
            let k = if self.vrc_cycle_mode {
                n
            } else {
                // modo linha: o prescaler desce 3 por ciclo e soma 341 (um clock) ao chegar a <= 0
                let p = i64::from(self.vrc_prescaler);
                let d = 3 * i64::from(n);
                if d >= p {
                    let k = (d - p) / 341 + 1;
                    self.vrc_prescaler = (p - d + 341 * k) as i16;
                    k as u32
                } else {
                    self.vrc_prescaler = (p - d) as i16;
                    0
                }
            };
            if k > 0 {
                self.vrc_irq_clocks(k);
            }
        }
    }

    /// Em quantos ciclos (contando o próximo como 1) a IRQ do contador liga, se ligar.
    pub fn next_irq_in(&self) -> Option<u32> {
        if self.irq_flag {
            return None;
        }
        match self.id {
            69 if self.irq_count_enabled && self.irq_enabled => {
                Some(u32::from(self.irq_counter16) + 1)
            }
            24 | 26 if self.irq_enabled => {
                let s = 0x100 - u32::from(self.irq_counter);
                if self.vrc_cycle_mode {
                    Some(s)
                } else {
                    let need = i64::from(self.vrc_prescaler) + 341 * (i64::from(s) - 1);
                    Some(((need + 2) / 3).max(1) as u32)
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Mapper;
    use crate::Ppu;

    /// `cpu_cycles(n)` tem que dar o mesmo que `n` vezes `cpu_cycle`, e `next_irq_in` tem que
    /// acertar o ciclo em que a IRQ liga, para VRC6 e FME-7 em estados variados.
    #[test]
    fn irq_batch_matches_per_cycle() {
        let mut x = 12345u32;
        let mut rnd = || {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            x >> 16
        };
        let mut ppu = Ppu::new(&[]);
        for id in [24u8, 69] {
            for _ in 0..2000 {
                let mut a = Mapper::new(id, 0x20000, 0x2000, false, false, &mut ppu);
                a.irq_latch = rnd() as u8;
                a.irq_counter = rnd() as u8;
                a.irq_enabled = rnd() % 4 != 0;
                a.irq_count_enabled = rnd() % 4 != 0;
                a.vrc_cycle_mode = rnd() % 3 == 0;
                a.vrc_prescaler = (rnd() % 342) as i16;
                a.irq_counter16 = (rnd() % 3000) as u16;
                let mut b = a.clone();
                let n = rnd() % 5000;
                let predicted = a.next_irq_in();
                let mut first = None;
                for i in 1..=n {
                    b.cpu_cycle();
                    if b.irq_flag && first.is_none() {
                        first = Some(i);
                    }
                }
                a.cpu_cycles(n);
                assert_eq!(
                    (a.irq_flag, a.irq_counter, a.vrc_prescaler, a.irq_counter16),
                    (b.irq_flag, b.irq_counter, b.vrc_prescaler, b.irq_counter16),
                    "mapper {id}, {n} ciclos"
                );
                if let Some(f) = first {
                    assert_eq!(predicted, Some(f), "mapper {id}: previsão da IRQ");
                }
            }
        }
    }
}

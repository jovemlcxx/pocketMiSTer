//! Sistema NES: barramento do CPU, RAM, PPU, controles, DMA de sprites e cartucho.
//! Porte de `firmware/src/nes/nes.c`.
//!
//! Relógio-mestre (modelo do Mesen): cada ciclo de CPU tem 12 ticks. A leitura acontece depois
//! de 5 ticks e a escrita depois de 7; a PPU anda até ali antes do acesso e até o fim do ciclo
//! depois dele. No fim do ciclo o CPU amostra a linha NMI.
//!
//! `REFERENCIA = true` é o `PPU_REFERENCE` do C: a PPU anda ponto a ponto até o fim de cada
//! ciclo, sem previsão de eventos. O normal é a sincronização preguiçosa: a PPU só é posta em
//! dia quando o CPU mexe nela, num evento previsto (vblank, A12 do MMC3) ou numa escrita no
//! mapper.

use crate::apu::{Apu, Dma};
use crate::mapper::{Mapper, Wram};
use crate::{Bus, Cpu, Ppu};

const PPU_DIV: u32 = 4;
const PPU_OFFSET: u32 = 1;

/// bits do controle, na ordem em que o NES lê
pub mod pad {
    pub const A: u8 = 1;
    pub const B: u8 = 2;
    pub const SELECT: u8 = 4;
    pub const START: u8 = 8;
    pub const UP: u8 = 16;
    pub const DOWN: u8 = 32;
    pub const LEFT: u8 = 64;
    pub const RIGHT: u8 = 128;
}

/// Por que uma imagem iNES não carregou (os códigos do C entre parênteses).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// não é iNES (-1)
    NotInes,
    /// arquivo menor que o cabeçalho diz, ou sem PRG (-2)
    Truncated,
    /// mapper não suportado (-3)
    Mapper(u8),
}

impl LoadError {
    pub fn code(self) -> i32 {
        match self {
            LoadError::NotInes => -1,
            LoadError::Truncated => -2,
            LoadError::Mapper(_) => -3,
        }
    }
}

/// Tudo o que o CPU enxerga. `repr(C)` com os campos de todo ciclo primeiro: deslocamentos
/// pequenos cabem no imediato das instruções de carga do M33.
#[repr(C)]
pub struct Board<'a, const REFERENCIA: bool = false> {
    /// relógio-mestre NTSC: 12 ticks por ciclo de CPU, 4 por ponto da PPU
    pub master: u32,
    /// tempo-mestre do próximo evento da PPU (vblank, A12)
    pub sync_at: u32,
    pub ppu_master: u32,
    /// ciclo a partir do qual a APU precisa rodar (ver `Apu::next_exec`)
    apu_next: u32,
    /// ciclo em que a IRQ prevista do contador do mapper (VRC6, FME-7) liga
    map_next: u32,
    /// até que ciclo os contadores do mapper andaram
    map_prev: u32,
    pub open_bus: u8,
    pub has_cpu_cycle: bool,
    map_event: bool,
    /// estado atual dos botões, preenchido pela plataforma
    pub pad: [u8; 2],
    pub pad_shift: [u8; 2],
    pub pad_strobe: bool,
    /// as 16 páginas de 2 KB de $8000-$FFFF, como a tabela de ponteiros do C: o tamanho fixo
    /// dispensa a checagem de limite na leitura
    prg_page: [&'a [u8; 0x800]; 16],
    pub ram: [u8; 0x800],
    /// na placa: na PSRAM
    pub prg: &'a [u8],
    pub map: Mapper,
    /// DMA de sprites e de amostras do DMC (o CPU para na próxima leitura)
    pub dma: Dma,
    pub apu: Apu,
    pub ppu: Ppu<'a>,
    /// $6000-$7FFF
    pub prg_ram: [u8; 0x2000],
}

/// `repr(C)` com o CPU primeiro, como no C: registradores e flags a deslocamento pequeno.
#[repr(C)]
pub struct Nes<'a, const REFERENCIA: bool = false> {
    pub cpu: Cpu,
    pub bus: Board<'a, REFERENCIA>,
    /// o cartucho tem bateria: `bus.prg_ram` ($6000-$7FFF) é o save, a plataforma carrega e grava
    pub battery: bool,
}

impl<'a, const R: bool> Board<'a, R> {
    /// Refaz a tabela de páginas depois que o mapper trocou bancos.
    fn map_prg(&mut self) {
        let prg = self.prg;
        for (page, &o) in self.prg_page.iter_mut().zip(&self.map.prg) {
            let o = o as usize;
            // INFALLIBLE: o mapper só gera deslocamentos de bancos que existem (banco % total)
            *page = prg[o..o + 0x800]
                .try_into()
                .expect("banco de PRG fora da ROM");
        }
    }

    /// O MMC3 conta as subidas de A12 que a PPU viu; a linha de IRQ acompanha a flag do mapper.
    #[inline(always)]
    fn drain_a12(&mut self) {
        for _ in 0..self.ppu.take_a12_rises() {
            self.map.a12_rise();
        }
    }

    #[inline(always)]
    fn ppu_run_to(&mut self, t: u32) {
        while self.ppu_master + PPU_DIV <= t {
            self.ppu.step();
            self.ppu_master += PPU_DIV;
        }
        self.drain_a12();
    }

    fn ppu_catch_up(&mut self) {
        let t = self.master.wrapping_sub(PPU_OFFSET);
        if R {
            self.ppu_run_to(t);
        } else {
            if self.ppu_master + PPU_DIV <= t {
                let n = (t - self.ppu_master) >> 2;
                self.ppu.run(n);
                self.ppu_master += n * PPU_DIV;
            }
            // também as subidas de $2006/$2007, que vêm antes desta sincronização
            self.drain_a12();
        }
    }

    fn ppu_sync(&mut self) {
        self.ppu_catch_up();
        if !R {
            // o k-ésimo ponto futuro roda quando master - 1 >= ppu_master + 4k
            let k = self.ppu.dots_to_event(262 * 341);
            self.sync_at = self.ppu_master.wrapping_add(PPU_DIV * k + PPU_OFFSET);
        }
    }

    #[inline(never)]
    fn cycle_slow(&mut self, c: u32) {
        if self.master >= self.sync_at {
            self.ppu_sync();
        }
        if self.map_due(c) {
            self.map_catch_up(c);
        }
    }

    #[inline(always)]
    fn map_due(&self, c: u32) -> bool {
        self.map_event && (c.wrapping_sub(self.map_next) as i32) >= 0
    }

    /// Anda os contadores de IRQ do mapper até o fim do ciclo `c` e prevê a próxima IRQ.
    fn map_catch_up(&mut self, c: u32) {
        if !self.has_cpu_cycle {
            return;
        }
        self.map.cpu_cycles(c.wrapping_sub(self.map_prev));
        self.map_prev = c;
        match self.map.next_irq_in() {
            Some(n) => {
                self.map_event = true;
                self.map_next = c.wrapping_add(n);
            }
            None => self.map_event = false,
        }
    }

    /// A linha de IRQ do CPU junta a do mapper (ver `mapper.rs`) e as da APU. Elas só mudam
    /// nos caminhos lentos (sincronização da PPU, mapper, APU, DMA), que chamam isto antes do
    /// fim do ciclo.
    #[inline(always)]
    fn irq_to(&self, cpu: &mut Cpu) {
        cpu.irq_line = self.map.irq_flag || self.apu.irq();
    }

    /// Começo de um ciclo de CPU: a APU roda aqui quando precisa (o `Exec` do Mesen vem no
    /// começo do ciclo, antes do acesso).
    #[inline(always)]
    fn start_cycle(&mut self, cpu: &mut Cpu) {
        let c = cpu.cycles.wrapping_add(1);
        if (c.wrapping_sub(self.apu_next) as i32) >= 0 {
            self.apu_exec(cpu, c);
        }
    }

    #[inline(never)]
    fn apu_exec(&mut self, cpu: &mut Cpu, c: u32) {
        self.apu.exec(c, &mut self.dma);
        self.apu_after(cpu, c);
    }

    /// Depois de mexer na APU: quando ela precisa rodar de novo e a linha de IRQ.
    #[inline(always)]
    fn apu_after(&mut self, cpu: &mut Cpu, c: u32) {
        self.apu_next = self.apu.next_exec(c);
        self.irq_to(cpu);
    }

    #[inline(always)]
    fn cycle_end(&mut self, cpu: &mut Cpu) {
        cpu.cycles = cpu.cycles.wrapping_add(1);
        self.master = self.master.wrapping_add(12);
        if R {
            // referência: o contador do mapper anda ciclo a ciclo
            self.ppu_run_to(self.master.wrapping_sub(PPU_OFFSET));
            self.map_catch_up(cpu.cycles);
            self.irq_to(cpu);
        } else if self.master >= self.sync_at || self.map_due(cpu.cycles) {
            self.cycle_slow(cpu.cycles);
            self.irq_to(cpu);
        }
        cpu.end_cycle(self.ppu.nmi_out);
    }

    /// A PPU posta em dia até `ticks` depois do começo do ciclo (5 = leitura, 7 = escrita).
    #[inline(always)]
    fn catch_up_at(&mut self, ticks: u32) {
        self.master = self.master.wrapping_add(ticks);
        self.ppu_catch_up();
        self.master = self.master.wrapping_sub(ticks);
    }

    fn io_read(&mut self, cpu: &mut Cpu, addr: u16) -> u8 {
        if addr < 0x4000 {
            self.catch_up_at(5);
            let v = self.ppu.read_reg(addr);
            if addr & 7 == 7 {
                self.ppu_sync();
            }
            return v;
        }
        if addr == 0x4016 || addr == 0x4017 {
            let n = usize::from(addr & 1);
            let bit = if self.pad_strobe {
                self.pad[n] & 1
            } else {
                self.pad_shift[n] & 1
            };
            if !self.pad_strobe {
                self.pad_shift[n] = (self.pad_shift[n] >> 1) | 0x80;
            }
            return (self.open_bus & 0xE0) | bit;
        }
        if addr == 0x4015 {
            let c = cpu.cycles.wrapping_add(1);
            let v = self.apu.read_status(c, self.open_bus, &mut self.dma);
            self.apu_after(cpu, c);
            return v;
        }
        self.open_bus // registradores só de escrita e área de expansão
    }

    fn io_write(&mut self, cpu: &mut Cpu, addr: u16, v: u8) {
        if addr < 0x4000 {
            self.catch_up_at(7);
            self.ppu.write_reg(addr, v);
            self.ppu_sync();
        } else if addr == 0x4014 {
            // DMA de sprites: o CPU para na próxima leitura
            self.dma.sprite = true;
            self.dma.sprite_page = v;
            self.dma.need_halt = true;
        } else if matches!(addr, 0x4000..=0x4013 | 0x4015 | 0x4017) {
            let c = cpu.cycles.wrapping_add(1);
            self.apu.write(c, addr, v, &mut self.dma);
            self.apu_after(cpu, c);
        } else if addr == 0x4016 {
            self.pad_strobe = v & 1 != 0;
            if self.pad_strobe {
                self.pad_shift = self.pad;
            }
        } else if addr >= 0x6000 {
            // troca de banco/espelhamento muda o que a PPU desenha a partir de agora
            self.master = self.master.wrapping_add(7);
            self.ppu_sync();
            self.master = self.master.wrapping_sub(7);
            // os contadores andam até o ciclo anterior; o deste ciclo vem no fim dele
            self.map_catch_up(cpu.cycles);
            self.map.write(&mut self.ppu, addr, v, cpu.cycles);
            self.map_catch_up(cpu.cycles);
            self.map_prg();
            if addr >= 0x9000 {
                let c = cpu.cycles.wrapping_add(1);
                self.apu.expansion_write(c, addr, v, &mut self.dma);
            }
        }
    }

    /// $2000-$7FFF: registradores e a área de $6000 (fora da linha: o caminho quente é RAM e PRG).
    #[inline(never)]
    fn read_slow(&mut self, cpu: &mut Cpu, addr: u16) -> u8 {
        let v = if addr >= 0x6000 {
            let off = usize::from(addr & 0x7FF);
            match self.map.wram[usize::from(addr >> 11) - 12] {
                Wram::Ram(o) => self.prg_ram[usize::from(o) + off],
                Wram::Rom(o) => self.prg[o as usize + off],
                Wram::Open => self.io_read(cpu, addr),
            }
        } else {
            self.io_read(cpu, addr)
        };
        self.irq_to(cpu);
        v
    }

    #[inline(always)]
    fn raw_read(&mut self, cpu: &mut Cpu, addr: u16) -> u8 {
        let v = if addr >= 0x8000 {
            self.prg_page[usize::from(addr >> 11) & 15][usize::from(addr & 0x7FF)]
        } else if addr < 0x2000 {
            self.ram[usize::from(addr & 0x7FF)]
        } else {
            self.read_slow(cpu, addr)
        };
        self.open_bus = v;
        v
    }

    #[inline(never)]
    fn write_slow(&mut self, cpu: &mut Cpu, addr: u16, v: u8) {
        if (0x6000..0x8000).contains(&addr)
            && let Wram::Ram(o) = self.map.wram[usize::from(addr >> 11) - 12]
        {
            self.prg_ram[usize::from(o) + usize::from(addr & 0x7FF)] = v;
        } else {
            self.io_write(cpu, addr, v);
            self.irq_to(cpu);
        }
    }

    /// Ciclo de DMA: os pedidos de parada e de leitura fantasma do DMC são consumidos aqui (o
    /// `processCycle` do Mesen).
    fn dma_cycle_start(&mut self, cpu: &mut Cpu) {
        let d = &mut self.dma;
        if d.abort_dmc {
            d.dmc_running = false;
            d.abort_dmc = false;
            d.need_dummy_read = false;
            d.need_halt = false;
        } else if d.need_halt {
            d.need_halt = false;
        } else if d.need_dummy_read {
            d.need_dummy_read = false;
        }
        self.start_cycle(cpu);
    }

    /// Leitura do DMA. Se o CPU parou lendo $4000-$401F, o 2A03 também lê os registradores
    /// internos ($4015-$4017) junto com o endereço do DMA (bug do chip, ver o Mesen).
    fn dma_read(&mut self, cpu: &mut Cpu, addr: u16, prev: &mut u16, internal: bool) -> u8 {
        if !internal {
            *prev = addr;
            return if (0x4000..=0x401F).contains(&addr) {
                self.open_bus
            } else {
                self.raw_read(cpu, addr)
            };
        }
        let ia = 0x4000 | (addr & 0x1F);
        let same = ia == addr;
        let v = match ia {
            0x4015 => {
                let v = self.raw_read(cpu, 0x4015);
                if !same {
                    self.raw_read(cpu, addr);
                }
                v
            }
            0x4016 | 0x4017 => {
                // o mesmo registrador de controle duas vezes seguidas: o controle não vê a 2ª
                let mut v = if *prev == ia {
                    self.open_bus
                } else {
                    self.raw_read(cpu, ia)
                };
                if !same {
                    // conflito no barramento: bits abertos vêm de fora, o resto é E dos dois
                    const OB: u8 = 0xE0;
                    let ext = self.raw_read(cpu, addr);
                    v = (ext & OB) | ((v & !OB) & (ext & !OB));
                }
                v
            }
            _ => self.raw_read(cpu, addr),
        };
        *prev = ia;
        v
    }

    /// O CPU parou para DMA na leitura de `addr` (o `ProcessPendingDma` do Mesen): ciclo de
    /// parada, ciclos de alinhamento e as leituras do DMC e/ou os 256 pares do DMA de sprites.
    #[inline(never)]
    fn process_pending_dma(&mut self, cpu: &mut Cpu, addr: u16) {
        let mut prev = addr;
        let internal = addr & 0xFFE0 == 0x4000;
        let pad_addr = addr == 0x4016 || addr == 0x4017;
        // o DMC lendo o mesmo endereço esconde a leitura dos controles (/OE fica ativo)
        let skip_first = internal
            && self.dma.dmc_running
            && pad_addr
            && (self.apu.dmc_addr() & 0x1F) == (addr & 0x1F);
        // no NES (não no Famicom) só a 1ª leitura fantasma de $4016/$4017 tem efeito
        let skip_dummy = pad_addr;
        self.dma.need_halt = false;

        self.start_cycle(cpu);
        if !(self.dma.abort_dmc && pad_addr) && !skip_first {
            self.raw_read(cpu, addr);
        }
        self.cycle_end(cpu);
        if self.dma.abort_dmc {
            self.dma.dmc_running = false;
            self.dma.abort_dmc = false;
            if !self.dma.sprite {
                self.dma.need_dummy_read = false;
                return;
            }
        }

        let (mut count, mut spr, mut value) = (0u16, 0u8, 0u8);
        while self.dma.dmc_running || self.dma.sprite {
            if cpu.cycles & 1 == 0 {
                if self.dma.dmc_running && !self.dma.need_halt && !self.dma.need_dummy_read {
                    // DMC pronto para ler (parada e leitura fantasma já feitas)
                    self.dma_cycle_start(cpu);
                    value = self.dma_read(cpu, self.apu.dmc_addr(), &mut prev, internal);
                    self.cycle_end(cpu);
                    self.dma.dmc_running = false;
                    self.dma.abort_dmc = false;
                    self.apu.dmc_set_buffer(value, &mut self.dma);
                    let c = cpu.cycles;
                    self.apu_after(cpu, c);
                } else if self.dma.sprite {
                    self.dma_cycle_start(cpu);
                    let a = u16::from(self.dma.sprite_page) << 8 | u16::from(spr);
                    value = self.dma_read(cpu, a, &mut prev, internal);
                    self.cycle_end(cpu);
                    spr = spr.wrapping_add(1);
                    count += 1;
                } else {
                    // DMC esperando a parada/leitura fantasma e sem DMA de sprite: ciclo parado
                    self.dma_cycle_start(cpu);
                    if !skip_dummy {
                        self.raw_read(cpu, addr);
                    }
                    self.cycle_end(cpu);
                }
            } else if self.dma.sprite && count & 1 != 0 {
                // escrita do DMA de sprites (depois de uma leitura)
                self.dma_cycle_start(cpu);
                self.open_bus = value;
                self.write_slow(cpu, 0x2004, value);
                self.cycle_end(cpu);
                count += 1;
                if count == 0x200 {
                    self.dma.sprite = false;
                }
            } else {
                // alinhamento até o ciclo de leitura
                self.dma_cycle_start(cpu);
                if !skip_dummy {
                    self.raw_read(cpu, addr);
                }
                self.cycle_end(cpu);
            }
        }
    }
}

impl<const R: bool> Bus for Board<'_, R> {
    #[inline(always)]
    fn read(&mut self, cpu: &mut Cpu, addr: u16) -> u8 {
        if self.dma.need_halt {
            self.process_pending_dma(cpu, addr);
        }
        self.start_cycle(cpu);
        let v = self.raw_read(cpu, addr);
        self.cycle_end(cpu);
        v
    }

    #[inline(always)]
    fn write(&mut self, cpu: &mut Cpu, addr: u16, v: u8) {
        self.start_cycle(cpu);
        self.open_bus = v;
        if addr < 0x2000 {
            self.ram[usize::from(addr & 0x7FF)] = v;
        } else {
            self.write_slow(cpu, addr, v);
        }
        self.cycle_end(cpu);
    }
}

impl<'a, const R: bool> Nes<'a, R> {
    /// Carrega uma imagem iNES (só lê o buffer, não copia a PRG nem a CHR) e liga o console.
    pub fn new(rom: &'a [u8]) -> Result<Self, LoadError> {
        if rom.len() < 16 || &rom[..4] != b"NES\x1A" {
            return Err(LoadError::NotInes);
        }
        let prg = usize::from(rom[4]) * 0x4000;
        let chr = usize::from(rom[5]) * 0x2000;
        let off = 16 + if rom[6] & 0x04 != 0 { 512 } else { 0 };
        if off + prg + chr > rom.len() || prg == 0 {
            return Err(LoadError::Truncated);
        }
        let id = (rom[6] >> 4) | (rom[7] & 0xF0);
        if !Mapper::supported(id) {
            return Err(LoadError::Mapper(id));
        }
        let mut chr_ram_size = 0x2000u32;
        if rom[7] & 0x0C == 0x08 && rom[11] & 0x0F != 0 {
            // NES 2.0: tamanho da CHR-RAM = 64 << n
            chr_ram_size = (64u32 << (rom[11] & 0x0F)).min(0x8000);
        }
        let vertical = rom[6] & 1 != 0;
        let battery = rom[6] & 2 != 0;
        let four_screen = rom[6] & 8 != 0; // tratado como vertical por enquanto (precisa de 4 KB de VRAM)

        let mut ppu = Ppu::new(&rom[off + prg..off + prg + chr]);
        ppu.power();
        let map = Mapper::new(
            id,
            prg as u32,
            chr_ram_size,
            vertical,
            four_screen,
            &mut ppu,
        );
        let prg = &rom[off..off + prg];
        // INFALLIBLE: a PRG tem pelo menos 16 KB (conferido acima)
        let first: &[u8; 0x800] = prg[..0x800].try_into().expect("PRG vazia");
        let mut nes = Nes {
            cpu: Cpu::default(),
            bus: Board {
                master: 0,
                sync_at: 0,
                ppu_master: 0,
                apu_next: 0,
                map_next: 0,
                map_prev: 0,
                open_bus: 0,
                has_cpu_cycle: map.has_cpu_cycle,
                map_event: false,
                pad: [0; 2],
                pad_shift: [0; 2],
                pad_strobe: false,
                prg_page: [first; 16],
                ram: [0; 0x800],
                prg,
                map,
                dma: Dma::default(),
                apu: Apu::new(0),
                ppu,
                prg_ram: [0; 0x2000],
            },
            battery,
        };
        nes.bus.map_prg();
        nes.bus.apu.set_mapper(id);
        nes.start();
        Ok(nes)
    }

    /// Partida do CPU depois de ligar ou do reset: como no Mesen, a APU roda um ciclo antes dos
    /// 7 ciclos de partida do CPU.
    fn start(&mut self) {
        let b = &mut self.bus;
        b.apu.exec(0, &mut b.dma);
        b.apu_next = b.apu.next_exec(0);
        self.cpu.reset(&mut self.bus);
    }

    /// Botão reset (não é desligar): CPU, APU e os registradores da PPU que o reset zera; a RAM,
    /// a VRAM e o cartucho continuam.
    pub fn reset(&mut self) {
        let (a, x, y, s, p) = (
            self.cpu.a,
            self.cpu.x,
            self.cpu.y,
            self.cpu.s,
            self.cpu.get_p(false),
        );
        let b = &mut self.bus;
        // o contador de ciclos recomeça do 0: os contadores do mapper ficam em dia antes
        b.map_catch_up(self.cpu.cycles);
        b.map_prev = 0;
        b.map_catch_up(0);
        b.dma = Dma::default();
        b.apu.reset(true, 0);
        b.ppu.ctrl = 0;
        b.ppu.mask = 0;
        b.ppu.w = false;
        b.ppu.read_buf = 0;
        b.ppu.nmi_out = false;
        self.start();
        // o reset do 6502 não zera os registradores: só desce a pilha 3 e liga o I
        self.cpu.a = a;
        self.cpu.x = x;
        self.cpu.y = y;
        self.cpu.s = s.wrapping_sub(3);
        self.cpu.set_p(p | 0x04);
    }

    /// Amostras de áudio (16 bits, mono, `apu::SAMPLE_RATE`) desde a última chamada.
    pub fn take_samples(&mut self, out: &mut [i16]) -> usize {
        self.bus.apu.take_samples(out)
    }

    /// Uma instrução.
    pub fn step(&mut self) {
        self.cpu.step(&mut self.bus);
    }

    /// Até a PPU terminar um quadro.
    pub fn run_frame(&mut self) {
        self.bus.ppu.frame_done = false;
        while !self.bus.ppu.frame_done {
            self.cpu.step(&mut self.bus);
        }
        // a APU em dia até o fim do quadro, para o áudio sair completo
        let c = self.cpu.cycles;
        let b = &mut self.bus;
        b.apu.catch_up(c, &mut b.dma);
        b.apu_after(&mut self.cpu, c);
        let b = &mut self.bus;
        let base = b.ppu_master;
        b.master = b.master.wrapping_sub(base);
        b.ppu_master = 0;
        b.sync_at = b.sync_at.wrapping_sub(base);
    }
}

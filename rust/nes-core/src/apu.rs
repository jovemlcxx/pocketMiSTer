//! APU 2A03 (NTSC): 2 pulsos, triângulo, ruído, DMC e contador de quadros, com IRQ e a saída
//! mixada em amostras de 16 bits. Baseada na APU do Mesen (GPL-3.0, mesma licença): mesma
//! temporização, inclusive a do DMA de amostras do DMC (ver `nes.rs`).
//!
//! O Mesen chama a APU em todo ciclo de CPU e ela decide se precisa rodar (`NeedToRun`). Aqui o
//! sistema só chama [`Apu::exec`] no ciclo que [`Apu::next_exec`] indica: nos ciclos pulados o
//! `NeedToRun` do Mesen daria falso. Enquanto há algo que precisa de todo ciclo (escrita
//! pendente em $4017, DMC tocando) o próximo é sempre o ciclo seguinte, como lá.
//!
//! Os tempos são ciclos de CPU absolutos (u32, com volta): o ciclo `c` do sistema é o mesmo `c`
//! aqui.

/// Taxa das amostras de áudio.
pub const SAMPLE_RATE: u32 = 48_000;
const CPU_HZ: u64 = 1_789_773;
/// cabe um quadro com folga (800 amostras a 48 kHz)
const SAMPLE_BUF: usize = 2048;

const LENGTH_TABLE: [u8; 32] = [
    10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14, 12, 16, 24, 18, 48, 20, 96, 22,
    192, 24, 72, 26, 16, 28, 32, 30,
];
const DUTY: [[u8; 8]; 4] = [
    [0, 0, 0, 0, 0, 0, 0, 1],
    [0, 0, 0, 0, 0, 0, 1, 1],
    [0, 0, 0, 0, 1, 1, 1, 1],
    [1, 1, 1, 1, 1, 1, 0, 0],
];
const TRIANGLE: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
    13, 14, 15,
];
const NOISE_PERIOD: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068,
];
const DMC_PERIOD: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54,
];
/// ciclos de cada passo do contador de quadros (modo 4 e 5 passos)
const STEP_CYCLES: [[u32; 6]; 2] = [
    [7457, 14913, 22371, 29828, 29829, 29830],
    [7457, 14913, 22371, 29829, 37281, 37282],
];
/// 0 = nada, 1 = quarto de quadro, 2 = meio quadro
const STEP_TYPE: [[u8; 6]; 2] = [[1, 2, 1, 0, 2, 0], [1, 2, 1, 0, 2, 0]];

/// Mixer não linear do NES (NESdev Wiki), em escala de 16 bits. Calculado na compilação: na
/// emulação não há ponto flutuante (ADR-002).
const PULSE_MIX: [u16; 31] = {
    let mut t = [0u16; 31];
    let mut i = 1;
    while i < 31 {
        t[i] = (95.52 / (8128.0 / i as f64 + 100.0) * 32767.0) as u16;
        i += 1;
    }
    t
};
const TND_MIX: [u16; 203] = {
    let mut t = [0u16; 203];
    let mut i = 1;
    while i < 203 {
        t[i] = (163.67 / (24329.0 / i as f64 + 100.0) * 32767.0) as u16;
        i += 1;
    }
    t
};

/// Cabeçalho WAV (PCM 16 bits mono, `SAMPLE_RATE`) para `n` amostras.
pub fn wav_header(n: u32) -> [u8; 44] {
    let data = n * 2;
    let mut h = [0u8; 44];
    let mut put = |at: usize, b: &[u8]| h[at..at + b.len()].copy_from_slice(b);
    put(0, b"RIFF");
    put(4, &(36 + data).to_le_bytes());
    put(8, b"WAVEfmt ");
    put(16, &16u32.to_le_bytes());
    put(20, &1u16.to_le_bytes()); // PCM
    put(22, &1u16.to_le_bytes()); // mono
    put(24, &SAMPLE_RATE.to_le_bytes());
    put(28, &(SAMPLE_RATE * 2).to_le_bytes());
    put(32, &2u16.to_le_bytes());
    put(34, &16u16.to_le_bytes());
    put(36, b"data");
    put(40, &data.to_le_bytes());
    h
}

// ---- som de expansão do cartucho (como o Vrc6Audio e o Sunsoft5bAudio do Mesen) ----

/// Pulso do VRC6: 16 passos, `duty` + 1 deles ligados (ou sempre ligado no modo digital).
#[derive(Default, Clone, Copy, Debug)]
struct Vrc6Pulse {
    volume: u8,
    duty: u8,
    ignore_duty: bool,
    period: u16,
    enabled: bool,
    timer: u16,
    step: u8,
}

impl Vrc6Pulse {
    fn write(&mut self, reg: u16, v: u8) {
        match reg {
            0 => {
                self.volume = v & 0x0F;
                self.duty = (v & 0x70) >> 4;
                self.ignore_duty = v & 0x80 != 0;
            }
            1 => self.period = (self.period & 0x0F00) | u16::from(v),
            _ => {
                self.period = (self.period & 0xFF) | (u16::from(v & 0x0F) << 8);
                self.enabled = v & 0x80 != 0;
                if !self.enabled {
                    self.step = 0;
                }
            }
        }
    }
    fn run(&mut self, mut n: u32, shift: u8) {
        if !self.enabled {
            return;
        }
        while n > 0 {
            let t = u32::from(self.timer.max(1));
            if n < t {
                self.timer = (t - n) as u16;
                return;
            }
            n -= t;
            self.step = (self.step + 1) & 0x0F;
            self.timer = (self.period >> shift) + 1;
        }
    }
    fn out(&self) -> u8 {
        if !self.enabled {
            0
        } else if self.ignore_duty || self.step <= self.duty {
            self.volume
        } else {
            0
        }
    }
}

/// Serra do VRC6: acumulador que soma a taxa a cada 2 passos e zera no 14º.
#[derive(Default, Clone, Copy, Debug)]
struct Vrc6Saw {
    rate: u8,
    period: u16,
    enabled: bool,
    timer: u16,
    step: u8,
    acc: u8,
}

impl Vrc6Saw {
    fn write(&mut self, reg: u16, v: u8) {
        match reg {
            0 => self.rate = v & 0x3F,
            1 => self.period = (self.period & 0x0F00) | u16::from(v),
            _ => {
                self.period = (self.period & 0xFF) | (u16::from(v & 0x0F) << 8);
                self.enabled = v & 0x80 != 0;
                if !self.enabled {
                    self.acc = 0;
                    self.step = 0;
                }
            }
        }
    }
    fn run(&mut self, mut n: u32, shift: u8) {
        if !self.enabled {
            return;
        }
        while n > 0 {
            let t = u32::from(self.timer.max(1));
            if n < t {
                self.timer = (t - n) as u16;
                return;
            }
            n -= t;
            self.timer = (self.period >> shift) + 1;
            self.step = (self.step + 1) % 14;
            if self.step == 0 {
                self.acc = 0;
            } else if self.step & 1 == 0 {
                self.acc = self.acc.wrapping_add(self.rate);
            }
        }
    }
    fn out(&self) -> u8 {
        if self.enabled { self.acc >> 3 } else { 0 }
    }
}

/// Volume do 5B: 3 dB por passo (16 passos), em escala de 0 a 1000.
const S5B_VOLUME: [u16; 16] = {
    let mut t = [0u16; 16];
    let mut i = 1;
    let mut v = 1000.0;
    while i < 16 {
        t[16 - i] = v as u16;
        v /= 1.412_537_5; // 10^(3/20)
        i += 1;
    }
    t
};

#[derive(Clone, Copy, Debug)]
enum Expansion {
    None,
    /// `swap`: VRC6b (mapper 26), com A0 e A1 trocados
    Vrc6 {
        swap: bool,
        p: [Vrc6Pulse; 2],
        saw: Vrc6Saw,
        halt: bool,
        shift: u8,
    },
    /// Sunsoft 5B: só os 3 tons e o volume (sem envelope nem ruído, que os jogos quase não usam)
    S5b {
        select: u8,
        reg: [u8; 16],
        timer: [u16; 3],
        step: [u8; 3],
        half: bool,
    },
}

impl Expansion {
    fn run(&mut self, n: u32) {
        match self {
            Expansion::None => {}
            Expansion::Vrc6 {
                p,
                saw,
                halt,
                shift,
                ..
            } => {
                if !*halt {
                    p[0].run(n, *shift);
                    p[1].run(n, *shift);
                    saw.run(n, *shift);
                }
            }
            Expansion::S5b {
                reg,
                timer,
                step,
                half,
                ..
            } => {
                // os tons andam a cada 2 ciclos de CPU
                let ticks = (n + u32::from(*half)) / 2;
                *half = (n + u32::from(*half)) % 2 == 1;
                for ch in 0..3 {
                    let period =
                        (u16::from(reg[ch * 2]) | (u16::from(reg[ch * 2 + 1] & 0x0F) << 8)).max(1);
                    let mut k = ticks;
                    while k > 0 {
                        let t = u32::from(timer[ch].max(1));
                        if k < t {
                            timer[ch] = (t - k) as u16;
                            break;
                        }
                        k -= t;
                        timer[ch] = period;
                        step[ch] = (step[ch] + 1) & 0x0F;
                    }
                }
            }
        }
    }

    /// Saída na mesma escala do mixer da APU (16 bits).
    fn out(&self) -> u32 {
        match self {
            Expansion::None => 0,
            Expansion::Vrc6 { p, saw, .. } => {
                (u32::from(p[0].out()) + u32::from(p[1].out()) + u32::from(saw.out())) * 300
            }
            Expansion::S5b { reg, step, .. } => {
                let mut s = 0;
                for ch in 0..3 {
                    let tone_on = reg[7] & (1 << ch) == 0;
                    if tone_on && step[ch] < 8 {
                        s += u32::from(S5B_VOLUME[usize::from(reg[8 + ch] & 0x0F)]) * 4;
                    }
                }
                s
            }
        }
    }

    fn write(&mut self, addr: u16, v: u8) {
        match self {
            Expansion::None => {}
            Expansion::Vrc6 {
                swap,
                p,
                saw,
                halt,
                shift,
            } => {
                let a = if *swap {
                    (addr & 0xFFFC) | ((addr & 1) << 1) | ((addr & 2) >> 1)
                } else {
                    addr
                };
                match a & 0xF003 {
                    0x9003 => {
                        *halt = v & 0x01 != 0;
                        *shift = if v & 0x04 != 0 {
                            8
                        } else if v & 0x02 != 0 {
                            4
                        } else {
                            0
                        };
                    }
                    0x9000..=0x9002 => p[0].write(a & 3, v),
                    0xA000..=0xA002 => p[1].write(a & 3, v),
                    0xB000..=0xB002 => saw.write(a & 3, v),
                    _ => {}
                }
            }
            Expansion::S5b { select, reg, .. } => match addr & 0xE000 {
                0xC000 => *select = v & 0x0F,
                0xE000 => reg[usize::from(*select)] = v,
                _ => {}
            },
        }
    }
}

/// Estado do DMA da CPU que o DMC pede e cancela (o `NesCpu` do Mesen). Fica no sistema.
#[derive(Default, Clone, Debug)]
pub struct Dma {
    pub need_halt: bool,
    pub need_dummy_read: bool,
    pub dmc_running: bool,
    pub abort_dmc: bool,
    pub sprite: bool,
    pub sprite_page: u8,
}

impl Dma {
    fn start_dmc(&mut self) {
        self.dmc_running = true;
        self.need_dummy_read = true;
        self.need_halt = true;
    }
    fn stop_dmc(&mut self) {
        if self.dmc_running {
            if self.need_halt {
                // interrompido antes do ciclo de parada: cancela inteiro
                self.dmc_running = false;
                self.need_dummy_read = false;
                self.need_halt = false;
            } else {
                // só dá para abortar no primeiro ciclo do DMA
                self.abort_dmc = true;
            }
        }
    }
}

/// Divisor que dispara a cada `period + 1` ciclos (o `ApuTimer` do Mesen).
#[derive(Default, Clone, Copy, Debug)]
struct Timer {
    timer: u16,
    period: u16,
    prev: u32,
}

impl Timer {
    #[inline(always)]
    fn run(&mut self, target: u32) -> bool {
        let n = target.wrapping_sub(self.prev) as i32;
        if n > i32::from(self.timer) {
            self.prev = self.prev.wrapping_add(u32::from(self.timer) + 1);
            self.timer = self.period;
            true
        } else {
            self.timer -= n.max(0) as u16;
            self.prev = target;
            false
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct Length {
    enabled: bool,
    halt: bool,
    new_halt: bool,
    counter: u8,
    reload: u8,
    previous: u8,
}

impl Length {
    fn load(&mut self, v: u8) -> bool {
        if self.enabled {
            self.reload = LENGTH_TABLE[usize::from(v & 0x1F)];
            self.previous = self.counter;
        }
        self.enabled
    }
    fn reload_counter(&mut self) {
        if self.reload != 0 {
            if self.counter == self.previous {
                self.counter = self.reload;
            }
            self.reload = 0;
        }
        self.halt = self.new_halt;
    }
    fn tick(&mut self) {
        if self.counter > 0 && !self.halt {
            self.counter -= 1;
        }
    }
    fn set_enabled(&mut self, e: bool) {
        if !e {
            self.counter = 0;
        }
        self.enabled = e;
    }
    fn reset(&mut self, soft: bool, triangle: bool) {
        self.enabled = false;
        if !soft || !triangle {
            // no reset o triângulo não perde a contagem
            self.halt = false;
            self.counter = 0;
            self.new_halt = false;
            self.reload = 0;
            self.previous = 0;
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct Envelope {
    constant: bool,
    volume: u8,
    start: bool,
    divider: i8,
    counter: u8,
}

impl Envelope {
    fn init(&mut self, v: u8) {
        self.constant = v & 0x10 != 0;
        self.volume = v & 0x0F;
    }
    fn tick(&mut self, looping: bool) {
        if self.start {
            self.start = false;
            self.counter = 15;
            self.divider = self.volume as i8;
        } else {
            self.divider -= 1;
            if self.divider < 0 {
                self.divider = self.volume as i8;
                if self.counter > 0 {
                    self.counter -= 1;
                } else if looping {
                    self.counter = 15;
                }
            }
        }
    }
    fn output(&self, len: &Length) -> u8 {
        if len.counter == 0 {
            0
        } else if self.constant {
            self.volume
        } else {
            self.counter
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct Pulse {
    first: bool,
    timer: Timer,
    len: Length,
    env: Envelope,
    duty: u8,
    pos: u8,
    period: u16,
    sweep_enabled: bool,
    sweep_negate: bool,
    sweep_period: u8,
    sweep_shift: u8,
    sweep_divider: u8,
    sweep_reload: bool,
    target: u32,
    out: u8,
}

impl Pulse {
    fn muted(&self) -> bool {
        self.period < 8 || (!self.sweep_negate && self.target > 0x7FF)
    }
    fn update_target(&mut self) {
        let shift = u32::from(self.period) >> self.sweep_shift;
        self.target = if self.sweep_negate {
            let t = u32::from(self.period).wrapping_sub(shift);
            if self.first { t.wrapping_sub(1) } else { t }
        } else {
            u32::from(self.period) + shift
        };
    }
    fn set_period(&mut self, p: u16) {
        self.period = p;
        self.timer.period = p * 2 + 1;
        self.update_target();
    }
    fn update_output(&mut self) {
        self.out = if self.muted() {
            0
        } else {
            DUTY[usize::from(self.duty)][usize::from(self.pos)] * self.env.output(&self.len)
        };
    }
    fn run(&mut self, target: u32) {
        while self.timer.run(target) {
            self.pos = (self.pos.wrapping_sub(1)) & 7;
            self.update_output();
        }
    }
    fn tick_sweep(&mut self) {
        self.sweep_divider = self.sweep_divider.wrapping_sub(1);
        if self.sweep_divider == 0 {
            if self.sweep_shift > 0
                && self.sweep_enabled
                && self.period >= 8
                && self.target <= 0x7FF
            {
                self.set_period(self.target as u16);
            }
            self.sweep_divider = self.sweep_period;
        }
        if self.sweep_reload {
            self.sweep_divider = self.sweep_period;
            self.sweep_reload = false;
        }
    }
    /// devolve true se a escrita mexeu na contagem de duração (a APU tem que rodar no próximo ciclo)
    fn write(&mut self, reg: u16, v: u8) -> bool {
        match reg & 3 {
            0 => {
                self.len.new_halt = v & 0x20 != 0;
                self.env.init(v);
                self.duty = (v & 0xC0) >> 6;
                self.update_output();
                true
            }
            1 => {
                self.sweep_enabled = v & 0x80 != 0;
                self.sweep_negate = v & 0x08 != 0;
                self.sweep_period = ((v & 0x70) >> 4) + 1;
                self.sweep_shift = v & 0x07;
                self.update_target();
                self.sweep_reload = true;
                false
            }
            2 => {
                self.set_period((self.period & 0x0700) | u16::from(v));
                false
            }
            _ => {
                let need = self.len.load(v >> 3);
                self.set_period((self.period & 0xFF) | (u16::from(v & 7) << 8));
                self.pos = 0;
                self.env.start = true;
                need
            }
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct Triangle {
    timer: Timer,
    len: Length,
    period: u16,
    linear: u8,
    linear_reload: u8,
    linear_control: bool,
    linear_reload_flag: bool,
    pos: u8,
    out: u8,
}

impl Triangle {
    fn run(&mut self, target: u32) {
        while self.timer.run(target) {
            if self.len.counter > 0 && self.linear > 0 {
                self.pos = (self.pos + 1) & 0x1F;
                self.out = TRIANGLE[usize::from(self.pos)];
            }
        }
    }
    fn tick_linear(&mut self) {
        if self.linear_reload_flag {
            self.linear = self.linear_reload;
        } else if self.linear > 0 {
            self.linear -= 1;
        }
        if !self.linear_control {
            self.linear_reload_flag = false;
        }
    }
    fn write(&mut self, reg: u16, v: u8) -> bool {
        match reg & 3 {
            0 => {
                self.linear_control = v & 0x80 != 0;
                self.linear_reload = v & 0x7F;
                self.len.new_halt = self.linear_control;
                true
            }
            2 => {
                self.period = (self.period & 0x0700) | u16::from(v);
                self.timer.period = self.period;
                false
            }
            3 => {
                let need = self.len.load(v >> 3);
                self.period = (self.period & 0xFF) | (u16::from(v & 7) << 8);
                self.timer.period = self.period;
                self.linear_reload_flag = true;
                need
            }
            _ => false,
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct Noise {
    timer: Timer,
    len: Length,
    env: Envelope,
    shift: u16,
    mode: bool,
    out: u8,
}

impl Noise {
    fn update_output(&mut self) {
        self.out = if self.shift & 1 != 0 {
            0
        } else {
            self.env.output(&self.len)
        };
    }
    fn run(&mut self, target: u32) {
        while self.timer.run(target) {
            let other = if self.mode { 6 } else { 1 };
            let feedback = (self.shift ^ (self.shift >> other)) & 1;
            self.shift = (self.shift >> 1) | (feedback << 14);
            self.update_output();
        }
    }
    fn write(&mut self, reg: u16, v: u8) -> bool {
        match reg & 3 {
            0 => {
                self.len.new_halt = v & 0x20 != 0;
                self.env.init(v);
                self.update_output();
                true
            }
            2 => {
                self.timer.period = NOISE_PERIOD[usize::from(v & 0x0F)] - 1;
                self.mode = v & 0x80 != 0;
                false
            }
            3 => {
                let need = self.len.load(v >> 3);
                self.env.start = true;
                need
            }
            _ => false,
        }
    }
}

#[derive(Default, Clone, Copy, Debug)]
struct Dmc {
    timer: Timer,
    sample_addr: u16,
    sample_len: u16,
    out: u8,
    irq_enabled: bool,
    looping: bool,
    addr: u16,
    bytes: u16,
    buffer: u8,
    buffer_empty: bool,
    shift: u8,
    bits: u8,
    silence: bool,
    need_to_run: bool,
    start_delay: u8,
    disable_delay: u8,
}

impl Dmc {
    fn reset(&mut self, soft: bool, now: u32) {
        if !soft {
            // no reset o endereço e o tamanho da amostra continuam
            self.sample_addr = 0xC000;
            self.sample_len = 1;
        }
        self.timer = Timer {
            timer: 0,
            period: 0,
            prev: now,
        };
        self.out = 0;
        self.irq_enabled = false;
        self.looping = false;
        self.addr = 0;
        self.bytes = 0;
        self.buffer = 0;
        self.buffer_empty = true;
        self.shift = 0;
        self.bits = 8;
        self.silence = true;
        self.need_to_run = false;
        self.start_delay = 0;
        self.disable_delay = 0;
        self.timer.period = DMC_PERIOD[0] - 1;
        // o DMC não dispara no primeiro ciclo (ajuda os testes de DMA de sprite e DMC)
        self.timer.timer = self.timer.period;
    }
    fn init_sample(&mut self) {
        self.addr = self.sample_addr;
        self.bytes = self.sample_len;
        self.need_to_run |= self.bytes > 0;
    }
    fn start_transfer(&mut self, dma: &mut Dma) {
        if self.buffer_empty && self.bytes > 0 {
            dma.start_dmc();
        }
    }
    fn irq_pending(&self, cycles: u32) -> bool {
        self.irq_enabled
            && self.bytes > 0
            && cycles
                >= (u32::from(self.bits) + (u32::from(self.bytes) - 1) * 8)
                    * u32::from(self.timer.period)
    }
    fn run(&mut self, target: u32, dma: &mut Dma) {
        while self.timer.run(target) {
            if !self.silence {
                if self.shift & 1 != 0 {
                    if self.out <= 125 {
                        self.out += 2;
                    }
                } else if self.out >= 2 {
                    self.out -= 2;
                }
                self.shift >>= 1;
            }
            self.bits -= 1;
            if self.bits == 0 {
                self.bits = 8;
                if self.buffer_empty {
                    self.silence = true;
                } else {
                    self.silence = false;
                    self.shift = self.buffer;
                    self.buffer_empty = true;
                    self.need_to_run = true;
                    if self.start_delay == 0 {
                        // ligado agora por $4015: o DMA vem depois (process_clock)
                        self.start_transfer(dma);
                    }
                }
            }
        }
    }
    fn process_clock(&mut self, dma: &mut Dma) {
        if self.disable_delay > 0 {
            self.disable_delay -= 1;
            if self.disable_delay == 0 {
                self.bytes = 0;
                // aborta a transferência que ainda não começou de fato
                dma.stop_dmc();
            }
        }
        if self.start_delay > 0 {
            self.start_delay -= 1;
            if self.start_delay == 0 {
                self.start_transfer(dma);
            }
        }
        self.need_to_run = self.disable_delay > 0 || self.start_delay > 0 || self.bytes > 0;
    }
}

#[derive(Clone, Debug)]
pub struct Apu {
    pulse: [Pulse; 2],
    tri: Triangle,
    noise: Noise,
    dmc: Dmc,
    exp: Expansion,
    /// IRQ do contador de quadros e do DMC (fontes separadas da do mapper)
    pub frame_irq: bool,
    pub dmc_irq: bool,
    need_to_run: bool,
    /// último ciclo até onde a APU rodou (`_previousCycle` do Mesen)
    prev: u32,
    // contador de quadros
    fc_mode: usize,
    fc_step: usize,
    fc_prev: u32,
    fc_new: Option<u8>,
    fc_delay: u8,
    fc_inhibit: bool,
    fc_block: u8,
    /// até onde o som de expansão andou
    exp_prev: u32,
    // áudio
    sample_n: u64,
    sample_base: u32,
    next_sample: u32,
    samples: [i16; SAMPLE_BUF],
    nsamples: usize,
}

impl Apu {
    /// APU ligada. `now` é o ciclo da primeira chamada de `exec` que virá.
    pub fn new(now: u32) -> Self {
        let mut a = Apu {
            pulse: [
                Pulse {
                    first: true,
                    ..Pulse::default()
                },
                Pulse::default(),
            ],
            tri: Triangle::default(),
            noise: Noise::default(),
            dmc: Dmc::default(),
            exp: Expansion::None,
            frame_irq: false,
            dmc_irq: false,
            need_to_run: false,
            prev: 0,
            fc_mode: 0,
            fc_step: 0,
            fc_prev: 0,
            fc_new: None,
            fc_delay: 0,
            fc_inhibit: false,
            fc_block: 0,
            exp_prev: 0,
            sample_n: 0,
            sample_base: 0,
            next_sample: 0,
            samples: [0; SAMPLE_BUF],
            nsamples: 0,
        };
        a.reset(false, now);
        a
    }

    /// Liga ou reinicia (`soft` = botão reset). `now` é o ciclo da próxima chamada de `exec`.
    pub fn reset(&mut self, soft: bool, now: u32) {
        let prev = now.wrapping_sub(1);
        self.prev = prev;
        for (i, p) in self.pulse.iter_mut().enumerate() {
            let len = p.len;
            *p = Pulse {
                first: i == 0,
                len,
                ..Pulse::default()
            };
            p.len.reset(soft, false);
            p.timer.prev = prev;
        }
        let len = self.tri.len;
        self.tri = Triangle {
            len,
            ..Triangle::default()
        };
        self.tri.len.reset(soft, true);
        self.tri.timer.prev = prev;
        let len = self.noise.len;
        self.noise = Noise {
            len,
            shift: 1,
            ..Noise::default()
        };
        self.noise.len.reset(soft, false);
        self.noise.timer.prev = prev;
        self.dmc.reset(soft, prev);
        // contador de quadros: "depois do reset a APU age como se $4017 tivesse recebido $00
        // de 9 a 12 ciclos antes da primeira instrução"; o modo continua no reset
        self.fc_prev = 0;
        if !soft {
            self.fc_mode = 0;
        }
        self.fc_step = 0;
        self.fc_new = Some(if self.fc_mode == 1 { 0x80 } else { 0 });
        self.fc_delay = 3;
        self.fc_inhibit = false;
        self.fc_block = 0;
        self.frame_irq = false;
        self.dmc_irq = false;
        self.need_to_run = false;
        self.exp_prev = prev;
        self.sample_base = now;
        self.sample_n = 0;
        self.next_sample = now;
    }

    /// Liga o som de expansão do cartucho: VRC6 (mappers 24 e 26) ou Sunsoft 5B (69).
    pub fn set_mapper(&mut self, id: u8) {
        self.exp = match id {
            24 | 26 => Expansion::Vrc6 {
                swap: id == 26,
                p: [Vrc6Pulse::default(); 2],
                saw: Vrc6Saw::default(),
                halt: false,
                shift: 0,
            },
            69 => Expansion::S5b {
                select: 0,
                reg: [0; 16],
                timer: [1; 3],
                step: [0; 3],
                half: false,
            },
            _ => Expansion::None,
        };
    }

    /// Escrita do CPU nos registradores de som do cartucho ($9000-$FFFF), no ciclo `c`.
    pub fn expansion_write(&mut self, c: u32, addr: u16, v: u8, dma: &mut Dma) {
        if !matches!(self.exp, Expansion::None) {
            self.run(c, dma);
            self.exp.write(addr, v);
        }
    }

    /// A linha de IRQ da APU.
    #[inline(always)]
    pub fn irq(&self) -> bool {
        self.frame_irq || self.dmc_irq
    }

    /// Começo do ciclo `c` (o `Exec` do Mesen). Só precisa ser chamado em `c >= next_exec()`.
    pub fn exec(&mut self, c: u32, dma: &mut Dma) {
        let dmc_need = if self.dmc.need_to_run {
            self.dmc.process_clock(dma);
            self.dmc.need_to_run
        } else {
            false
        };
        let run = if dmc_need || self.need_to_run {
            self.need_to_run = false;
            true
        } else {
            let n = c.wrapping_sub(self.prev);
            self.fc_need_to_run(n) || self.dmc.irq_pending(n)
        };
        if run {
            self.run(c, dma);
        }
    }

    /// Primeiro ciclo a partir do qual `exec` tem efeito (antes dele o `NeedToRun` do Mesen é falso).
    pub fn next_exec(&self, c: u32) -> u32 {
        let d = &self.dmc;
        if self.need_to_run || self.fc_new.is_some() || self.fc_block > 0 {
            return c.wrapping_add(1);
        }
        // O Mesen roda todo ciclo enquanto o DMC toca, só para o pedido de DMA sair no ciclo
        // certo. Esse pedido só nasce quando o divisor do DMC dispara: basta rodar nesse ciclo.
        // Com atraso de liga/desliga pendente (ou fim de amostra), aí sim ciclo a ciclo.
        let dmc_at = if !d.need_to_run {
            None
        } else if d.start_delay > 0 || d.disable_delay > 0 || d.bytes == 0 {
            return c.wrapping_add(1);
        } else {
            Some(d.timer.prev.wrapping_add(u32::from(d.timer.timer) + 1))
        };
        let step = STEP_CYCLES[self.fc_mode][self.fc_step];
        let mut n = (step - 1).saturating_sub(self.fc_prev);
        if self.dmc.irq_enabled && self.dmc.bytes > 0 {
            let empty = (u32::from(self.dmc.bits) + (u32::from(self.dmc.bytes) - 1) * 8)
                * u32::from(self.dmc.timer.period);
            n = n.min(empty);
        }
        // ciclo t com t - prev >= n; nunca antes do próximo
        let mut t = self.prev.wrapping_add(n);
        if let Some(at) = dmc_at
            && (at.wrapping_sub(t) as i32) < 0
        {
            t = at;
        }
        if (t.wrapping_sub(c) as i32) < 1 {
            c.wrapping_add(1)
        } else {
            t
        }
    }

    fn fc_need_to_run(&self, n: u32) -> bool {
        self.fc_new.is_some()
            || self.fc_block > 0
            || self.fc_prev + n >= STEP_CYCLES[self.fc_mode][self.fc_step] - 1
    }

    fn frame_tick(&mut self, kind: u8) {
        // quarto de quadro: envelopes e contador linear
        for p in &mut self.pulse {
            p.env.tick(p.len.halt);
        }
        self.tri.tick_linear();
        self.noise.env.tick(self.noise.len.halt);
        if kind == 2 {
            // meio quadro: duração e sweep
            for p in &mut self.pulse {
                p.len.tick();
            }
            self.tri.len.tick();
            self.noise.len.tick();
            for p in &mut self.pulse {
                p.tick_sweep();
            }
        }
    }

    /// O `ApuFrameCounter::Run` do Mesen: anda até o próximo passo (ou tudo) e devolve quantos
    /// ciclos andou.
    fn fc_run(&mut self, to_run: &mut i32) -> u32 {
        let step = STEP_CYCLES[self.fc_mode][self.fc_step];
        let ran;
        if self.fc_prev as i64 + i64::from(*to_run) >= i64::from(step) {
            if !self.fc_inhibit && self.fc_mode == 0 && self.fc_step >= 3 {
                // IRQ nos 3 últimos ciclos do modo de 4 passos
                self.frame_irq = true;
            }
            let kind = STEP_TYPE[self.fc_mode][self.fc_step];
            if kind != 0 && self.fc_block == 0 {
                self.frame_tick(kind);
                // escrita em $4017 não gera outro clock neste ciclo nem no seguinte
                self.fc_block = 2;
            }
            ran = step.saturating_sub(self.fc_prev);
            *to_run -= ran as i32;
            self.fc_step += 1;
            if self.fc_step == 6 {
                self.fc_step = 0;
                self.fc_prev = 0;
            } else {
                self.fc_prev += ran;
            }
        } else {
            ran = *to_run as u32;
            *to_run = 0;
            self.fc_prev += ran;
        }
        if let Some(v) = self.fc_new {
            self.fc_delay -= 1;
            if self.fc_delay == 0 {
                self.fc_mode = usize::from(v & 0x80 != 0);
                self.fc_step = 0;
                self.fc_prev = 0;
                self.fc_new = None;
                if self.fc_mode == 1 && self.fc_block == 0 {
                    // modo de 5 passos: clock imediato de quarto e meio quadro
                    self.frame_tick(2);
                    self.fc_block = 2;
                }
            }
        }
        if self.fc_block > 0 {
            self.fc_block -= 1;
        }
        ran
    }

    /// Roda tudo até o ciclo `c` (o `Run` do Mesen), gerando as amostras de áudio no caminho.
    fn run(&mut self, c: u32, dma: &mut Dma) {
        let mut to_run = c.wrapping_sub(self.prev) as i32;
        while to_run > 0 {
            let ran = self.fc_run(&mut to_run);
            let target = self.prev.wrapping_add(ran);
            // recargas de duração depois do contador de quadros (len_reload_timing)
            for p in &mut self.pulse {
                p.len.reload_counter();
            }
            self.noise.len.reload_counter();
            self.tri.len.reload_counter();
            // os canais andam em pedaços até cada instante de amostra
            while (target.wrapping_sub(self.next_sample) as i32) >= 0 {
                let s = self.next_sample;
                self.run_channels(s, dma);
                self.push_sample();
            }
            self.run_channels(target, dma);
            self.prev = target;
        }
    }

    fn run_channels(&mut self, t: u32, dma: &mut Dma) {
        self.exp.run(t.wrapping_sub(self.exp_prev));
        self.exp_prev = t;
        self.pulse[0].run(t);
        self.pulse[1].run(t);
        self.noise.run(t);
        self.tri.run(t);
        self.dmc.run(t, dma);
    }

    fn push_sample(&mut self) {
        let p = PULSE_MIX[usize::from(self.pulse[0].out + self.pulse[1].out)];
        let tnd = TND_MIX[usize::from(3 * self.tri.out + 2 * self.noise.out + self.dmc.out)];
        if self.nsamples < SAMPLE_BUF {
            self.samples[self.nsamples] =
                (u32::from(p) + u32::from(tnd) + self.exp.out()).min(32767) as i16;
            self.nsamples += 1;
        }
        self.sample_n += 1;
        self.next_sample = self
            .sample_base
            .wrapping_add((self.sample_n * CPU_HZ).div_ceil(u64::from(SAMPLE_RATE)) as u32);
    }

    /// Amostras geradas desde a última chamada (o sistema roda a APU até o fim do quadro antes).
    pub fn take_samples(&mut self, out: &mut [i16]) -> usize {
        let n = self.nsamples.min(out.len());
        out[..n].copy_from_slice(&self.samples[..n]);
        self.nsamples = 0;
        n
    }

    /// Põe a APU em dia até o ciclo `c` (fim de quadro, para o áudio).
    pub fn catch_up(&mut self, c: u32, dma: &mut Dma) {
        self.run(c, dma);
    }

    // ---- registradores ----

    /// Escrita em $4000-$4013, $4015, $4017 no ciclo `c`.
    pub fn write(&mut self, c: u32, addr: u16, v: u8, dma: &mut Dma) {
        self.run(c, dma);
        let need = match addr {
            0x4000..=0x4003 => self.pulse[0].write(addr, v),
            0x4004..=0x4007 => self.pulse[1].write(addr, v),
            0x4008..=0x400B => self.tri.write(addr, v),
            0x400C..=0x400F => self.noise.write(addr, v),
            0x4010 => {
                self.dmc.irq_enabled = v & 0x80 != 0;
                self.dmc.looping = v & 0x40 != 0;
                self.dmc.timer.period = DMC_PERIOD[usize::from(v & 0x0F)] - 1;
                if !self.dmc.irq_enabled {
                    self.dmc_irq = false;
                }
                false
            }
            0x4011 => {
                self.dmc.out = v & 0x7F;
                false
            }
            0x4012 => {
                self.dmc.sample_addr = 0xC000 | (u16::from(v) << 6);
                false
            }
            0x4013 => {
                self.dmc.sample_len = (u16::from(v) << 4) | 1;
                false
            }
            0x4015 => {
                // limpa a IRQ do DMC antes de ligar o canal (que pode gerar outra)
                self.dmc_irq = false;
                self.pulse[0].len.set_enabled(v & 0x01 != 0);
                self.pulse[1].len.set_enabled(v & 0x02 != 0);
                self.tri.len.set_enabled(v & 0x04 != 0);
                self.noise.len.set_enabled(v & 0x08 != 0);
                self.dmc_set_enabled(v & 0x10 != 0, c);
                false
            }
            0x4017 => {
                self.fc_new = Some(v);
                // "entre ciclos de APU o efeito vem 4 ciclos depois; num ciclo de APU, 3"
                self.fc_delay = if c & 1 != 0 { 4 } else { 3 };
                self.fc_inhibit = v & 0x40 != 0;
                if self.fc_inhibit {
                    self.frame_irq = false;
                }
                false
            }
            _ => false,
        };
        if need {
            self.need_to_run = true;
        }
        for p in &mut self.pulse {
            p.update_output();
        }
        self.noise.update_output();
    }

    fn dmc_set_enabled(&mut self, on: bool, c: u32) {
        if !on {
            if self.dmc.disable_delay == 0 {
                // desligar leva 1 ciclo de APU; um DMA que comece nesse meio é cancelado
                self.dmc.disable_delay = if c & 1 == 0 { 2 } else { 3 };
            }
            self.dmc.need_to_run = true;
        } else if self.dmc.bytes == 0 {
            self.dmc.init_sample();
            self.dmc.start_delay = if c & 1 == 0 { 2 } else { 3 };
            self.dmc.need_to_run = true;
        }
    }

    /// Leitura de $4015 no ciclo `c`. `open_bus` dá o bit 5.
    pub fn read_status(&mut self, c: u32, open_bus: u8, dma: &mut Dma) -> u8 {
        self.run(c, dma);
        let s = u8::from(self.pulse[0].len.counter > 0)
            | u8::from(self.pulse[1].len.counter > 0) << 1
            | u8::from(self.tri.len.counter > 0) << 2
            | u8::from(self.noise.len.counter > 0) << 3
            | u8::from(self.dmc.bytes > 0) << 4
            | u8::from(self.frame_irq) << 6
            | u8::from(self.dmc_irq) << 7
            | (open_bus & 0x20);
        self.frame_irq = false;
        s
    }

    /// Endereço da próxima leitura de amostra do DMC.
    pub fn dmc_addr(&self) -> u16 {
        self.dmc.addr
    }

    /// Byte de amostra que o DMA trouxe.
    pub fn dmc_set_buffer(&mut self, v: u8, dma: &mut Dma) {
        let d = &mut self.dmc;
        if d.bytes > 0 {
            d.buffer = v;
            d.buffer_empty = false;
            // passou de $FFFF volta para $8000
            d.addr = d.addr.wrapping_add(1);
            if d.addr == 0 {
                d.addr = 0x8000;
            }
            d.bytes -= 1;
            if d.bytes == 0 {
                if d.looping {
                    d.init_sample();
                } else if d.irq_enabled {
                    self.dmc_irq = true;
                }
            }
        }
        if d.sample_len == 1 && !d.looping && d.bits == 1 && d.timer.timer < 2 {
            // DMA que termina no ciclo de APU antes do contador de bits zerar: outro DMA é
            // pedido e abortado logo depois (1 ciclo de CPU parado)
            d.shift = d.buffer;
            d.buffer_empty = false;
            d.init_sample();
            d.disable_delay = 3;
        }
        let _ = dma;
    }
}

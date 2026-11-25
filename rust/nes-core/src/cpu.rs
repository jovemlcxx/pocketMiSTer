//! CPU 6502 do 2A03. Cada acesso ao barramento é um ciclo, inclusive as leituras e escritas
//! fantasmas, na mesma ordem do hardware (tabela de ciclos do 64doc e o NesCpu do Mesen).
//! Porte de `firmware/src/nes/cpu6502_impl.h`, acesso por acesso.

/// O que o CPU enxerga. Cada chamada é um ciclo: o barramento faz o acesso, conta o ciclo
/// (`cpu.cycles`) e no fim dele chama [`Cpu::end_cycle`] com o nível da linha NMI.
pub trait Bus {
    fn read(&mut self, cpu: &mut Cpu, addr: u16) -> u8;
    fn write(&mut self, cpu: &mut Cpu, addr: u16, v: u8);
}

/// Registradores e estado das interrupções. As flags ficam separadas: mais barato que montar P
/// em toda instrução. Z vale `z_res == 0` e N é o bit 7 de `n_res`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cpu {
    pub pc: u16,
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub s: u8,
    /// carry, sempre 0 ou 1 (entra direto nas contas)
    pub c: u8,
    pub z_res: u8,
    pub n_res: u8,
    pub v: bool,
    pub i: bool,
    pub d: bool,
    /// opcode KIL/JAM: o CPU parou
    pub jammed: bool,
    /// nível: algum dispositivo pedindo IRQ
    pub irq_line: bool,
    /// nível da linha NMI no último ciclo (para achar a borda)
    pub nmi_level: bool,
    pub need_nmi: bool,
    pub prev_need_nmi: bool,
    pub run_irq: bool,
    pub prev_run_irq: bool,
    pub cycles: u32,
}

impl Cpu {
    /// Fim de ciclo: amostra as linhas de interrupção. `nmi` é o nível atual da linha NMI.
    #[inline(always)]
    pub fn end_cycle(&mut self, nmi: bool) {
        self.prev_need_nmi = self.need_nmi;
        if nmi && !self.nmi_level {
            self.need_nmi = true;
        }
        self.nmi_level = nmi;
        self.prev_run_irq = self.run_irq;
        self.run_irq = self.irq_line && !self.i;
    }

    pub fn get_p(&self, brk: bool) -> u8 {
        (self.n_res & 0x80)
            | (u8::from(self.v) << 6)
            | 0x20
            | (u8::from(brk) << 4)
            | (u8::from(self.d) << 3)
            | (u8::from(self.i) << 2)
            | (u8::from(self.z_res == 0) << 1)
            | self.c
    }

    pub fn set_p(&mut self, p: u8) {
        self.n_res = p & 0x80;
        self.v = p & 0x40 != 0;
        self.d = p & 0x08 != 0;
        self.i = p & 0x04 != 0;
        self.z_res = u8::from(p & 0x02 == 0);
        self.c = p & 1;
    }

    /// Reset: 5 ciclos antes do vetor, depois o vetor em $FFFC.
    pub fn reset<B: Bus>(&mut self, b: &mut B) {
        *self = Cpu {
            s: 0xFD,
            ..Cpu::default()
        };
        self.set_p(0x24);
        for _ in 0..5 {
            self.rd(b, 0x00FF);
        }
        let lo = u16::from(self.rd(b, 0xFFFC));
        self.pc = lo | u16::from(self.rd(b, 0xFFFD)) << 8;
    }

    // ---- barramento ----
    #[inline(always)]
    fn rd<B: Bus>(&mut self, b: &mut B, a: u16) -> u8 {
        b.read(self, a)
    }
    #[inline(always)]
    fn wr<B: Bus>(&mut self, b: &mut B, a: u16, v: u8) {
        b.write(self, a, v)
    }
    #[inline(always)]
    fn fetch<B: Bus>(&mut self, b: &mut B) -> u8 {
        let pc = self.pc;
        self.pc = pc.wrapping_add(1);
        self.rd(b, pc)
    }
    #[inline(always)]
    fn push<B: Bus>(&mut self, b: &mut B, v: u8) {
        self.wr(b, 0x100 | u16::from(self.s), v);
        self.s = self.s.wrapping_sub(1);
    }
    #[inline(always)]
    fn pull<B: Bus>(&mut self, b: &mut B) -> u8 {
        self.s = self.s.wrapping_add(1);
        self.rd(b, 0x100 | u16::from(self.s))
    }
    #[inline(always)]
    fn dummy_pc<B: Bus>(&mut self, b: &mut B) {
        self.rd(b, self.pc);
    }
    #[inline(always)]
    fn dummy_stack<B: Bus>(&mut self, b: &mut B) {
        self.rd(b, 0x100 | u16::from(self.s));
    }
    #[inline(always)]
    fn nz(&mut self, v: u8) {
        self.z_res = v;
        self.n_res = v;
    }

    // ---- modos de endereçamento: devolvem o endereço efetivo, já com os acessos fantasmas ----
    #[inline(always)]
    fn am_imm(&mut self) -> u16 {
        let pc = self.pc;
        self.pc = pc.wrapping_add(1);
        pc
    }
    #[inline(always)]
    fn am_zp<B: Bus>(&mut self, b: &mut B) -> u16 {
        u16::from(self.fetch(b))
    }
    #[inline(always)]
    fn am_zpi<B: Bus>(&mut self, b: &mut B, i: u8) -> u16 {
        let z = self.fetch(b);
        self.rd(b, u16::from(z)); // lê o endereço sem índice
        u16::from(z.wrapping_add(i))
    }
    #[inline(always)]
    fn am_abs<B: Bus>(&mut self, b: &mut B) -> u16 {
        let lo = u16::from(self.fetch(b));
        lo | u16::from(self.fetch(b)) << 8
    }
    /// Indexado: leitura só paga o ciclo extra se cruzar página; escrita e RMW sempre pagam.
    #[inline(always)]
    fn am_absi<B: Bus>(&mut self, b: &mut B, i: u8, always: bool) -> u16 {
        let base = self.am_abs(b);
        let ea = base.wrapping_add(u16::from(i));
        if always || (base ^ ea) & 0xFF00 != 0 {
            self.rd(b, (base & 0xFF00) | (ea & 0x00FF));
        }
        ea
    }
    #[inline(always)]
    fn am_indx<B: Bus>(&mut self, b: &mut B) -> u16 {
        let mut z = self.fetch(b);
        self.rd(b, u16::from(z));
        z = z.wrapping_add(self.x);
        let lo = u16::from(self.rd(b, u16::from(z)));
        lo | u16::from(self.rd(b, u16::from(z.wrapping_add(1)))) << 8
    }
    #[inline(always)]
    fn zp_ptr<B: Bus>(&mut self, b: &mut B) -> u16 {
        let z = self.fetch(b);
        let lo = u16::from(self.rd(b, u16::from(z)));
        lo | u16::from(self.rd(b, u16::from(z.wrapping_add(1)))) << 8
    }
    #[inline(always)]
    fn am_indy<B: Bus>(&mut self, b: &mut B, always: bool) -> u16 {
        let base = self.zp_ptr(b);
        let ea = base.wrapping_add(u16::from(self.y));
        if always || (base ^ ea) & 0xFF00 != 0 {
            self.rd(b, (base & 0xFF00) | (ea & 0x00FF));
        }
        ea
    }

    // ---- operações ----
    #[inline(always)]
    fn adc(&mut self, m: u8) {
        let r = u16::from(self.a) + u16::from(m) + u16::from(self.c);
        self.v = !(self.a ^ m) & (self.a ^ r as u8) & 0x80 != 0;
        self.c = u8::from(r > 0xFF);
        self.a = r as u8;
        self.nz(self.a);
    }
    #[inline(always)]
    fn cmp(&mut self, reg: u8, m: u8) {
        self.c = u8::from(reg >= m);
        self.nz(reg.wrapping_sub(m));
    }
    #[inline(always)]
    fn asl(&mut self, v: u8) -> u8 {
        self.c = v >> 7;
        self.nz(v << 1);
        v << 1
    }
    #[inline(always)]
    fn lsr(&mut self, v: u8) -> u8 {
        self.c = v & 1;
        self.nz(v >> 1);
        v >> 1
    }
    #[inline(always)]
    fn rol(&mut self, v: u8) -> u8 {
        let r = v << 1 | self.c;
        self.c = v >> 7;
        self.nz(r);
        r
    }
    #[inline(always)]
    fn ror(&mut self, v: u8) -> u8 {
        let r = v >> 1 | self.c << 7;
        self.c = v & 1;
        self.nz(r);
        r
    }
    #[inline(always)]
    fn bit(&mut self, m: u8) {
        self.z_res = self.a & m;
        self.n_res = m;
        self.v = m & 0x40 != 0;
    }

    #[inline(always)]
    fn branch<B: Bus>(&mut self, b: &mut B, take: bool) {
        let off = self.fetch(b) as i8;
        if !take {
            return;
        }
        self.dummy_pc(b);
        let t = self.pc.wrapping_add(off as u16);
        if (t ^ self.pc) & 0xFF00 != 0 {
            self.rd(b, (self.pc & 0xFF00) | (t & 0x00FF));
        }
        self.pc = t;
    }

    /// SHA/SHX/SHY/TAS: guardam valor & (alto+1); se cruzou página o byte alto vira o valor.
    #[inline(always)]
    fn sh<B: Bus>(&mut self, b: &mut B, base: u16, idx: u8, val: u8) {
        let mut ea = base.wrapping_add(u16::from(idx));
        self.rd(b, (base & 0xFF00) | (ea & 0x00FF));
        let v = val & ((base >> 8) as u8).wrapping_add(1);
        if (base ^ ea) & 0xFF00 != 0 {
            ea = u16::from(v) << 8 | (ea & 0xFF);
        }
        self.wr(b, ea, v);
    }

    /// IRQ, NMI ou BRK. Um NMI que chega até o empilhamento de P "sequestra" o vetor (inclusive
    /// do BRK).
    fn interrupt<B: Bus>(&mut self, b: &mut B, brk: bool) {
        if !brk {
            // BRK já leu o opcode e o byte de preenchimento
            self.dummy_pc(b);
            self.dummy_pc(b);
        }
        self.push(b, (self.pc >> 8) as u8);
        self.push(b, self.pc as u8);
        let mut vector = 0xFFFE;
        if self.need_nmi {
            self.need_nmi = false;
            vector = 0xFFFA;
        }
        self.push(b, self.get_p(brk));
        self.i = true;
        let lo = u16::from(self.rd(b, vector));
        self.pc = lo | u16::from(self.rd(b, vector + 1)) << 8;
    }

    /// Executa uma instrução (ou atende uma interrupção pendente).
    #[inline(always)]
    pub fn step<B: Bus>(&mut self, b: &mut B) {
        let s = self;
        if s.jammed {
            s.dummy_pc(b);
            return;
        }
        // decisão com o que foi visto no penúltimo ciclo da instrução anterior
        if s.prev_need_nmi || s.prev_run_irq {
            s.interrupt(b, false);
            return;
        }

        // leitura: endereço efetivo, um ciclo de leitura e o corpo com o valor lido
        macro_rules! rd_op {
            ($ea:expr, |$m:ident| $body:expr) => {{
                let ea = $ea;
                let $m = s.rd(b, ea);
                $body;
            }};
        }
        // read-modify-write: lê, escreve o valor antigo, escreve o novo (que o corpo devolve)
        macro_rules! rmw {
            ($ea:expr, |$m:ident| $body:expr) => {{
                let ea = $ea;
                let $m = s.rd(b, ea);
                s.wr(b, ea, $m);
                let r: u8 = $body;
                s.wr(b, ea, r);
            }};
        }
        macro_rules! implied {
            ($body:expr) => {{
                s.dummy_pc(b);
                $body;
            }};
        }

        let op = s.fetch(b);
        match op {
            // ---------- carga e armazenamento ----------
            0xA9 => rd_op!(s.am_imm(), |m| {
                s.a = m;
                s.nz(m)
            }),
            0xA5 => rd_op!(s.am_zp(b), |m| {
                s.a = m;
                s.nz(m)
            }),
            0xB5 => rd_op!(s.am_zpi(b, s.x), |m| {
                s.a = m;
                s.nz(m)
            }),
            0xAD => rd_op!(s.am_abs(b), |m| {
                s.a = m;
                s.nz(m)
            }),
            0xBD => rd_op!(s.am_absi(b, s.x, false), |m| {
                s.a = m;
                s.nz(m)
            }),
            0xB9 => rd_op!(s.am_absi(b, s.y, false), |m| {
                s.a = m;
                s.nz(m)
            }),
            0xA1 => rd_op!(s.am_indx(b), |m| {
                s.a = m;
                s.nz(m)
            }),
            0xB1 => rd_op!(s.am_indy(b, false), |m| {
                s.a = m;
                s.nz(m)
            }),
            0xA2 => rd_op!(s.am_imm(), |m| {
                s.x = m;
                s.nz(m)
            }),
            0xA6 => rd_op!(s.am_zp(b), |m| {
                s.x = m;
                s.nz(m)
            }),
            0xB6 => rd_op!(s.am_zpi(b, s.y), |m| {
                s.x = m;
                s.nz(m)
            }),
            0xAE => rd_op!(s.am_abs(b), |m| {
                s.x = m;
                s.nz(m)
            }),
            0xBE => rd_op!(s.am_absi(b, s.y, false), |m| {
                s.x = m;
                s.nz(m)
            }),
            0xA0 => rd_op!(s.am_imm(), |m| {
                s.y = m;
                s.nz(m)
            }),
            0xA4 => rd_op!(s.am_zp(b), |m| {
                s.y = m;
                s.nz(m)
            }),
            0xB4 => rd_op!(s.am_zpi(b, s.x), |m| {
                s.y = m;
                s.nz(m)
            }),
            0xAC => rd_op!(s.am_abs(b), |m| {
                s.y = m;
                s.nz(m)
            }),
            0xBC => rd_op!(s.am_absi(b, s.x, false), |m| {
                s.y = m;
                s.nz(m)
            }),
            0x85 => {
                let ea = s.am_zp(b);
                s.wr(b, ea, s.a)
            }
            0x95 => {
                let ea = s.am_zpi(b, s.x);
                s.wr(b, ea, s.a)
            }
            0x8D => {
                let ea = s.am_abs(b);
                s.wr(b, ea, s.a)
            }
            0x9D => {
                let ea = s.am_absi(b, s.x, true);
                s.wr(b, ea, s.a)
            }
            0x99 => {
                let ea = s.am_absi(b, s.y, true);
                s.wr(b, ea, s.a)
            }
            0x81 => {
                let ea = s.am_indx(b);
                s.wr(b, ea, s.a)
            }
            0x91 => {
                let ea = s.am_indy(b, true);
                s.wr(b, ea, s.a)
            }
            0x86 => {
                let ea = s.am_zp(b);
                s.wr(b, ea, s.x)
            }
            0x96 => {
                let ea = s.am_zpi(b, s.y);
                s.wr(b, ea, s.x)
            }
            0x8E => {
                let ea = s.am_abs(b);
                s.wr(b, ea, s.x)
            }
            0x84 => {
                let ea = s.am_zp(b);
                s.wr(b, ea, s.y)
            }
            0x94 => {
                let ea = s.am_zpi(b, s.x);
                s.wr(b, ea, s.y)
            }
            0x8C => {
                let ea = s.am_abs(b);
                s.wr(b, ea, s.y)
            }

            // ---------- transferências ----------
            0xAA => implied!({
                s.x = s.a;
                s.nz(s.x)
            }),
            0xA8 => implied!({
                s.y = s.a;
                s.nz(s.y)
            }),
            0x8A => implied!({
                s.a = s.x;
                s.nz(s.a)
            }),
            0x98 => implied!({
                s.a = s.y;
                s.nz(s.a)
            }),
            0xBA => implied!({
                s.x = s.s;
                s.nz(s.x)
            }),
            0x9A => implied!(s.s = s.x),

            // ---------- pilha ----------
            0x48 => {
                s.dummy_pc(b);
                s.push(b, s.a)
            }
            0x08 => {
                s.dummy_pc(b);
                s.push(b, s.get_p(true))
            }
            0x68 => {
                s.dummy_pc(b);
                s.dummy_stack(b);
                s.a = s.pull(b);
                s.nz(s.a)
            }
            0x28 => {
                s.dummy_pc(b);
                s.dummy_stack(b);
                let p = s.pull(b);
                s.set_p(p)
            }

            // ---------- lógica e aritmética ----------
            0x09 => rd_op!(s.am_imm(), |m| {
                s.a |= m;
                s.nz(s.a)
            }),
            0x05 => rd_op!(s.am_zp(b), |m| {
                s.a |= m;
                s.nz(s.a)
            }),
            0x15 => rd_op!(s.am_zpi(b, s.x), |m| {
                s.a |= m;
                s.nz(s.a)
            }),
            0x0D => rd_op!(s.am_abs(b), |m| {
                s.a |= m;
                s.nz(s.a)
            }),
            0x1D => rd_op!(s.am_absi(b, s.x, false), |m| {
                s.a |= m;
                s.nz(s.a)
            }),
            0x19 => rd_op!(s.am_absi(b, s.y, false), |m| {
                s.a |= m;
                s.nz(s.a)
            }),
            0x01 => rd_op!(s.am_indx(b), |m| {
                s.a |= m;
                s.nz(s.a)
            }),
            0x11 => rd_op!(s.am_indy(b, false), |m| {
                s.a |= m;
                s.nz(s.a)
            }),
            0x29 => rd_op!(s.am_imm(), |m| {
                s.a &= m;
                s.nz(s.a)
            }),
            0x25 => rd_op!(s.am_zp(b), |m| {
                s.a &= m;
                s.nz(s.a)
            }),
            0x35 => rd_op!(s.am_zpi(b, s.x), |m| {
                s.a &= m;
                s.nz(s.a)
            }),
            0x2D => rd_op!(s.am_abs(b), |m| {
                s.a &= m;
                s.nz(s.a)
            }),
            0x3D => rd_op!(s.am_absi(b, s.x, false), |m| {
                s.a &= m;
                s.nz(s.a)
            }),
            0x39 => rd_op!(s.am_absi(b, s.y, false), |m| {
                s.a &= m;
                s.nz(s.a)
            }),
            0x21 => rd_op!(s.am_indx(b), |m| {
                s.a &= m;
                s.nz(s.a)
            }),
            0x31 => rd_op!(s.am_indy(b, false), |m| {
                s.a &= m;
                s.nz(s.a)
            }),
            0x49 => rd_op!(s.am_imm(), |m| {
                s.a ^= m;
                s.nz(s.a)
            }),
            0x45 => rd_op!(s.am_zp(b), |m| {
                s.a ^= m;
                s.nz(s.a)
            }),
            0x55 => rd_op!(s.am_zpi(b, s.x), |m| {
                s.a ^= m;
                s.nz(s.a)
            }),
            0x4D => rd_op!(s.am_abs(b), |m| {
                s.a ^= m;
                s.nz(s.a)
            }),
            0x5D => rd_op!(s.am_absi(b, s.x, false), |m| {
                s.a ^= m;
                s.nz(s.a)
            }),
            0x59 => rd_op!(s.am_absi(b, s.y, false), |m| {
                s.a ^= m;
                s.nz(s.a)
            }),
            0x41 => rd_op!(s.am_indx(b), |m| {
                s.a ^= m;
                s.nz(s.a)
            }),
            0x51 => rd_op!(s.am_indy(b, false), |m| {
                s.a ^= m;
                s.nz(s.a)
            }),
            0x69 => rd_op!(s.am_imm(), |m| s.adc(m)),
            0x65 => rd_op!(s.am_zp(b), |m| s.adc(m)),
            0x75 => rd_op!(s.am_zpi(b, s.x), |m| s.adc(m)),
            0x6D => rd_op!(s.am_abs(b), |m| s.adc(m)),
            0x7D => rd_op!(s.am_absi(b, s.x, false), |m| s.adc(m)),
            0x79 => rd_op!(s.am_absi(b, s.y, false), |m| s.adc(m)),
            0x61 => rd_op!(s.am_indx(b), |m| s.adc(m)),
            0x71 => rd_op!(s.am_indy(b, false), |m| s.adc(m)),
            0xC9 => rd_op!(s.am_imm(), |m| s.cmp(s.a, m)),
            0xC5 => rd_op!(s.am_zp(b), |m| s.cmp(s.a, m)),
            0xD5 => rd_op!(s.am_zpi(b, s.x), |m| s.cmp(s.a, m)),
            0xCD => rd_op!(s.am_abs(b), |m| s.cmp(s.a, m)),
            0xDD => rd_op!(s.am_absi(b, s.x, false), |m| s.cmp(s.a, m)),
            0xD9 => rd_op!(s.am_absi(b, s.y, false), |m| s.cmp(s.a, m)),
            0xC1 => rd_op!(s.am_indx(b), |m| s.cmp(s.a, m)),
            0xD1 => rd_op!(s.am_indy(b, false), |m| s.cmp(s.a, m)),
            // SBC = ADC do complemento; $EB é o SBC ilegal
            0xE9 | 0xEB => rd_op!(s.am_imm(), |m| s.adc(!m)),
            0xE5 => rd_op!(s.am_zp(b), |m| s.adc(!m)),
            0xF5 => rd_op!(s.am_zpi(b, s.x), |m| s.adc(!m)),
            0xED => rd_op!(s.am_abs(b), |m| s.adc(!m)),
            0xFD => rd_op!(s.am_absi(b, s.x, false), |m| s.adc(!m)),
            0xF9 => rd_op!(s.am_absi(b, s.y, false), |m| s.adc(!m)),
            0xE1 => rd_op!(s.am_indx(b), |m| s.adc(!m)),
            0xF1 => rd_op!(s.am_indy(b, false), |m| s.adc(!m)),
            0xE0 => rd_op!(s.am_imm(), |m| s.cmp(s.x, m)),
            0xE4 => rd_op!(s.am_zp(b), |m| s.cmp(s.x, m)),
            0xEC => rd_op!(s.am_abs(b), |m| s.cmp(s.x, m)),
            0xC0 => rd_op!(s.am_imm(), |m| s.cmp(s.y, m)),
            0xC4 => rd_op!(s.am_zp(b), |m| s.cmp(s.y, m)),
            0xCC => rd_op!(s.am_abs(b), |m| s.cmp(s.y, m)),
            0x24 => rd_op!(s.am_zp(b), |m| s.bit(m)),
            0x2C => rd_op!(s.am_abs(b), |m| s.bit(m)),

            // ---------- deslocamentos, INC/DEC (read-modify-write) ----------
            0x06 => rmw!(s.am_zp(b), |m| s.asl(m)),
            0x16 => rmw!(s.am_zpi(b, s.x), |m| s.asl(m)),
            0x0E => rmw!(s.am_abs(b), |m| s.asl(m)),
            0x1E => rmw!(s.am_absi(b, s.x, true), |m| s.asl(m)),
            0x26 => rmw!(s.am_zp(b), |m| s.rol(m)),
            0x36 => rmw!(s.am_zpi(b, s.x), |m| s.rol(m)),
            0x2E => rmw!(s.am_abs(b), |m| s.rol(m)),
            0x3E => rmw!(s.am_absi(b, s.x, true), |m| s.rol(m)),
            0x46 => rmw!(s.am_zp(b), |m| s.lsr(m)),
            0x56 => rmw!(s.am_zpi(b, s.x), |m| s.lsr(m)),
            0x4E => rmw!(s.am_abs(b), |m| s.lsr(m)),
            0x5E => rmw!(s.am_absi(b, s.x, true), |m| s.lsr(m)),
            0x66 => rmw!(s.am_zp(b), |m| s.ror(m)),
            0x76 => rmw!(s.am_zpi(b, s.x), |m| s.ror(m)),
            0x6E => rmw!(s.am_abs(b), |m| s.ror(m)),
            0x7E => rmw!(s.am_absi(b, s.x, true), |m| s.ror(m)),
            0xC6 => rmw!(s.am_zp(b), |m| {
                let r = m.wrapping_sub(1);
                s.nz(r);
                r
            }),
            0xD6 => rmw!(s.am_zpi(b, s.x), |m| {
                let r = m.wrapping_sub(1);
                s.nz(r);
                r
            }),
            0xCE => rmw!(s.am_abs(b), |m| {
                let r = m.wrapping_sub(1);
                s.nz(r);
                r
            }),
            0xDE => rmw!(s.am_absi(b, s.x, true), |m| {
                let r = m.wrapping_sub(1);
                s.nz(r);
                r
            }),
            0xE6 => rmw!(s.am_zp(b), |m| {
                let r = m.wrapping_add(1);
                s.nz(r);
                r
            }),
            0xF6 => rmw!(s.am_zpi(b, s.x), |m| {
                let r = m.wrapping_add(1);
                s.nz(r);
                r
            }),
            0xEE => rmw!(s.am_abs(b), |m| {
                let r = m.wrapping_add(1);
                s.nz(r);
                r
            }),
            0xFE => rmw!(s.am_absi(b, s.x, true), |m| {
                let r = m.wrapping_add(1);
                s.nz(r);
                r
            }),
            0x0A => implied!(s.a = s.asl(s.a)),
            0x2A => implied!(s.a = s.rol(s.a)),
            0x4A => implied!(s.a = s.lsr(s.a)),
            0x6A => implied!(s.a = s.ror(s.a)),
            0xE8 => implied!({
                s.x = s.x.wrapping_add(1);
                s.nz(s.x)
            }),
            0xC8 => implied!({
                s.y = s.y.wrapping_add(1);
                s.nz(s.y)
            }),
            0xCA => implied!({
                s.x = s.x.wrapping_sub(1);
                s.nz(s.x)
            }),
            0x88 => implied!({
                s.y = s.y.wrapping_sub(1);
                s.nz(s.y)
            }),

            // ---------- flags ----------
            0x18 => implied!(s.c = 0),
            0x38 => implied!(s.c = 1),
            0x58 => implied!(s.i = false),
            0x78 => implied!(s.i = true),
            0xB8 => implied!(s.v = false),
            0xD8 => implied!(s.d = false),
            0xF8 => implied!(s.d = true),

            // ---------- desvios e saltos ----------
            0x10 => s.branch(b, s.n_res & 0x80 == 0),
            0x30 => s.branch(b, s.n_res & 0x80 != 0),
            0x50 => s.branch(b, !s.v),
            0x70 => s.branch(b, s.v),
            0x90 => s.branch(b, s.c == 0),
            0xB0 => s.branch(b, s.c != 0),
            0xD0 => s.branch(b, s.z_res != 0),
            0xF0 => s.branch(b, s.z_res == 0),
            0x4C => s.pc = s.am_abs(b),
            0x6C => {
                let p = s.am_abs(b);
                let lo = u16::from(s.rd(b, p));
                // bug da página: o byte alto vem do começo da mesma página
                s.pc = lo | u16::from(s.rd(b, (p & 0xFF00) | (p.wrapping_add(1) & 0xFF))) << 8;
            }
            0x20 => {
                let lo = u16::from(s.fetch(b));
                s.dummy_stack(b);
                s.push(b, (s.pc >> 8) as u8);
                s.push(b, s.pc as u8);
                s.pc = lo | u16::from(s.rd(b, s.pc)) << 8;
            }
            0x60 => {
                s.dummy_pc(b);
                s.dummy_stack(b);
                let lo = u16::from(s.pull(b));
                s.pc = lo | u16::from(s.pull(b)) << 8;
                s.fetch(b);
            }
            0x40 => {
                s.dummy_pc(b);
                s.dummy_stack(b);
                let p = s.pull(b);
                s.set_p(p);
                let lo = u16::from(s.pull(b));
                s.pc = lo | u16::from(s.pull(b)) << 8;
            }
            0x00 => {
                s.fetch(b);
                s.interrupt(b, true);
            }

            // ---------- NOPs (oficial e ilegais) ----------
            0xEA | 0x1A | 0x3A | 0x5A | 0x7A | 0xDA | 0xFA => s.dummy_pc(b),
            0x80 | 0x82 | 0x89 | 0xC2 | 0xE2 => {
                let ea = s.am_imm();
                s.rd(b, ea);
            }
            0x04 | 0x44 | 0x64 => {
                let ea = s.am_zp(b);
                s.rd(b, ea);
            }
            0x14 | 0x34 | 0x54 | 0x74 | 0xD4 | 0xF4 => {
                let ea = s.am_zpi(b, s.x);
                s.rd(b, ea);
            }
            0x0C => {
                let ea = s.am_abs(b);
                s.rd(b, ea);
            }
            0x1C | 0x3C | 0x5C | 0x7C | 0xDC | 0xFC => {
                let ea = s.am_absi(b, s.x, false);
                s.rd(b, ea);
            }

            // ---------- ilegais estáveis ----------
            // SLO
            0x07 => rmw!(s.am_zp(b), |m| {
                let r = s.asl(m);
                s.a |= r;
                s.nz(s.a);
                r
            }),
            0x17 => rmw!(s.am_zpi(b, s.x), |m| {
                let r = s.asl(m);
                s.a |= r;
                s.nz(s.a);
                r
            }),
            0x0F => rmw!(s.am_abs(b), |m| {
                let r = s.asl(m);
                s.a |= r;
                s.nz(s.a);
                r
            }),
            0x1F => rmw!(s.am_absi(b, s.x, true), |m| {
                let r = s.asl(m);
                s.a |= r;
                s.nz(s.a);
                r
            }),
            0x1B => rmw!(s.am_absi(b, s.y, true), |m| {
                let r = s.asl(m);
                s.a |= r;
                s.nz(s.a);
                r
            }),
            0x03 => rmw!(s.am_indx(b), |m| {
                let r = s.asl(m);
                s.a |= r;
                s.nz(s.a);
                r
            }),
            0x13 => rmw!(s.am_indy(b, true), |m| {
                let r = s.asl(m);
                s.a |= r;
                s.nz(s.a);
                r
            }),
            // RLA
            0x27 => rmw!(s.am_zp(b), |m| {
                let r = s.rol(m);
                s.a &= r;
                s.nz(s.a);
                r
            }),
            0x37 => rmw!(s.am_zpi(b, s.x), |m| {
                let r = s.rol(m);
                s.a &= r;
                s.nz(s.a);
                r
            }),
            0x2F => rmw!(s.am_abs(b), |m| {
                let r = s.rol(m);
                s.a &= r;
                s.nz(s.a);
                r
            }),
            0x3F => rmw!(s.am_absi(b, s.x, true), |m| {
                let r = s.rol(m);
                s.a &= r;
                s.nz(s.a);
                r
            }),
            0x3B => rmw!(s.am_absi(b, s.y, true), |m| {
                let r = s.rol(m);
                s.a &= r;
                s.nz(s.a);
                r
            }),
            0x23 => rmw!(s.am_indx(b), |m| {
                let r = s.rol(m);
                s.a &= r;
                s.nz(s.a);
                r
            }),
            0x33 => rmw!(s.am_indy(b, true), |m| {
                let r = s.rol(m);
                s.a &= r;
                s.nz(s.a);
                r
            }),
            // SRE
            0x47 => rmw!(s.am_zp(b), |m| {
                let r = s.lsr(m);
                s.a ^= r;
                s.nz(s.a);
                r
            }),
            0x57 => rmw!(s.am_zpi(b, s.x), |m| {
                let r = s.lsr(m);
                s.a ^= r;
                s.nz(s.a);
                r
            }),
            0x4F => rmw!(s.am_abs(b), |m| {
                let r = s.lsr(m);
                s.a ^= r;
                s.nz(s.a);
                r
            }),
            0x5F => rmw!(s.am_absi(b, s.x, true), |m| {
                let r = s.lsr(m);
                s.a ^= r;
                s.nz(s.a);
                r
            }),
            0x5B => rmw!(s.am_absi(b, s.y, true), |m| {
                let r = s.lsr(m);
                s.a ^= r;
                s.nz(s.a);
                r
            }),
            0x43 => rmw!(s.am_indx(b), |m| {
                let r = s.lsr(m);
                s.a ^= r;
                s.nz(s.a);
                r
            }),
            0x53 => rmw!(s.am_indy(b, true), |m| {
                let r = s.lsr(m);
                s.a ^= r;
                s.nz(s.a);
                r
            }),
            // RRA
            0x67 => rmw!(s.am_zp(b), |m| {
                let r = s.ror(m);
                s.adc(r);
                r
            }),
            0x77 => rmw!(s.am_zpi(b, s.x), |m| {
                let r = s.ror(m);
                s.adc(r);
                r
            }),
            0x6F => rmw!(s.am_abs(b), |m| {
                let r = s.ror(m);
                s.adc(r);
                r
            }),
            0x7F => rmw!(s.am_absi(b, s.x, true), |m| {
                let r = s.ror(m);
                s.adc(r);
                r
            }),
            0x7B => rmw!(s.am_absi(b, s.y, true), |m| {
                let r = s.ror(m);
                s.adc(r);
                r
            }),
            0x63 => rmw!(s.am_indx(b), |m| {
                let r = s.ror(m);
                s.adc(r);
                r
            }),
            0x73 => rmw!(s.am_indy(b, true), |m| {
                let r = s.ror(m);
                s.adc(r);
                r
            }),
            // DCP
            0xC7 => rmw!(s.am_zp(b), |m| {
                let r = m.wrapping_sub(1);
                s.cmp(s.a, r);
                r
            }),
            0xD7 => rmw!(s.am_zpi(b, s.x), |m| {
                let r = m.wrapping_sub(1);
                s.cmp(s.a, r);
                r
            }),
            0xCF => rmw!(s.am_abs(b), |m| {
                let r = m.wrapping_sub(1);
                s.cmp(s.a, r);
                r
            }),
            0xDF => rmw!(s.am_absi(b, s.x, true), |m| {
                let r = m.wrapping_sub(1);
                s.cmp(s.a, r);
                r
            }),
            0xDB => rmw!(s.am_absi(b, s.y, true), |m| {
                let r = m.wrapping_sub(1);
                s.cmp(s.a, r);
                r
            }),
            0xC3 => rmw!(s.am_indx(b), |m| {
                let r = m.wrapping_sub(1);
                s.cmp(s.a, r);
                r
            }),
            0xD3 => rmw!(s.am_indy(b, true), |m| {
                let r = m.wrapping_sub(1);
                s.cmp(s.a, r);
                r
            }),
            // ISB
            0xE7 => rmw!(s.am_zp(b), |m| {
                let r = m.wrapping_add(1);
                s.adc(!r);
                r
            }),
            0xF7 => rmw!(s.am_zpi(b, s.x), |m| {
                let r = m.wrapping_add(1);
                s.adc(!r);
                r
            }),
            0xEF => rmw!(s.am_abs(b), |m| {
                let r = m.wrapping_add(1);
                s.adc(!r);
                r
            }),
            0xFF => rmw!(s.am_absi(b, s.x, true), |m| {
                let r = m.wrapping_add(1);
                s.adc(!r);
                r
            }),
            0xFB => rmw!(s.am_absi(b, s.y, true), |m| {
                let r = m.wrapping_add(1);
                s.adc(!r);
                r
            }),
            0xE3 => rmw!(s.am_indx(b), |m| {
                let r = m.wrapping_add(1);
                s.adc(!r);
                r
            }),
            0xF3 => rmw!(s.am_indy(b, true), |m| {
                let r = m.wrapping_add(1);
                s.adc(!r);
                r
            }),
            // LAX
            0xA7 => rd_op!(s.am_zp(b), |m| {
                s.a = m;
                s.x = m;
                s.nz(m)
            }),
            0xB7 => rd_op!(s.am_zpi(b, s.y), |m| {
                s.a = m;
                s.x = m;
                s.nz(m)
            }),
            0xAF => rd_op!(s.am_abs(b), |m| {
                s.a = m;
                s.x = m;
                s.nz(m)
            }),
            0xBF => rd_op!(s.am_absi(b, s.y, false), |m| {
                s.a = m;
                s.x = m;
                s.nz(m)
            }),
            0xA3 => rd_op!(s.am_indx(b), |m| {
                s.a = m;
                s.x = m;
                s.nz(m)
            }),
            0xB3 => rd_op!(s.am_indy(b, false), |m| {
                s.a = m;
                s.x = m;
                s.nz(m)
            }),
            // SAX
            0x87 => {
                let ea = s.am_zp(b);
                s.wr(b, ea, s.a & s.x)
            }
            0x97 => {
                let ea = s.am_zpi(b, s.y);
                s.wr(b, ea, s.a & s.x)
            }
            0x8F => {
                let ea = s.am_abs(b);
                s.wr(b, ea, s.a & s.x)
            }
            0x83 => {
                let ea = s.am_indx(b);
                s.wr(b, ea, s.a & s.x)
            }
            // ANC
            0x0B | 0x2B => rd_op!(s.am_imm(), |m| {
                s.a &= m;
                s.nz(s.a);
                s.c = s.a >> 7
            }),
            // ALR
            0x4B => rd_op!(s.am_imm(), |m| {
                s.a &= m;
                s.a = s.lsr(s.a)
            }),
            // ARR
            0x6B => rd_op!(s.am_imm(), |m| {
                s.a = ((s.a & m) >> 1) | (s.c << 7);
                s.nz(s.a);
                s.c = (s.a >> 6) & 1;
                s.v = ((s.a >> 6) ^ (s.a >> 5)) & 1 != 0;
            }),
            // AXS/SBX
            0xCB => rd_op!(s.am_imm(), |m| {
                let ax = s.a & s.x;
                s.c = u8::from(ax >= m);
                s.x = ax.wrapping_sub(m);
                s.nz(s.x);
            }),
            // LAS
            0xBB => rd_op!(s.am_absi(b, s.y, false), |m| {
                let v = m & s.s;
                s.a = v;
                s.x = v;
                s.s = v;
                s.nz(v);
            }),

            // ---------- ilegais instáveis (comportamento mais comum no 2A03) ----------
            // XAA/ANE
            0x8B => rd_op!(s.am_imm(), |m| {
                s.a = (s.a | 0xEE) & s.x & m;
                s.nz(s.a)
            }),
            // LXA: no 2A03 se comporta como LDA+TAX
            0xAB => rd_op!(s.am_imm(), |m| {
                s.a = m;
                s.x = m;
                s.nz(m)
            }),
            // SHA abs,Y
            0x9F => {
                let base = s.am_abs(b);
                s.sh(b, base, s.y, s.a & s.x)
            }
            // SHA (ind),Y
            0x93 => {
                let base = s.zp_ptr(b);
                s.sh(b, base, s.y, s.a & s.x)
            }
            // SHX
            0x9E => {
                let base = s.am_abs(b);
                s.sh(b, base, s.y, s.x)
            }
            // SHY
            0x9C => {
                let base = s.am_abs(b);
                s.sh(b, base, s.x, s.y)
            }
            // TAS
            0x9B => {
                s.s = s.a & s.x;
                let base = s.am_abs(b);
                s.sh(b, base, s.y, s.s)
            }

            // ---------- KIL/JAM ----------
            0x02 | 0x12 | 0x22 | 0x32 | 0x42 | 0x52 | 0x62 | 0x72 | 0x92 | 0xB2 | 0xD2 | 0xF2 => {
                s.jammed = true
            }
        }
    }
}

//! PPU 2C02 (NTSC), por ponto: 341 pontos x 262 linhas, 3 pontos por ciclo de CPU.
//! Referências: NESdev Wiki (PPU rendering, PPU scrolling) e o NesPpu do Mesen.
//! Porte de `firmware/src/nes/ppu.c`: [`Ppu::step`] é a referência ponto a ponto e [`Ppu::run`]
//! o caminho rápido, que tem que dar exatamente o mesmo resultado.
//!
//! Diferenças de forma em relação ao C, sem mudar o comportamento:
//! - as páginas de CHR e de tabela de nomes são deslocamentos (em `chr_rom`/`chr_ram` e em
//!   `vram`) em vez de ponteiros;
//! - a subida de A12 que o MMC3 conta não chama o mapper: soma em `a12_rises`, que o sistema
//!   consome logo depois de andar com a PPU ([`Ppu::take_a12_rises`]). O mapper só muda de estado
//!   em escrita do CPU, e o sistema sincroniza a PPU antes de cada uma, então a ordem é a mesma.

/// paleta 2C02 comum aos emuladores (RGB), para mostrar o `fb`
#[rustfmt::skip]
pub const PALETTE: [[u8; 3]; 64] = [
    [84,84,84],[0,30,116],[8,16,144],[48,0,136],[68,0,100],[92,0,48],[84,4,0],[60,24,0],
    [32,42,0],[8,58,0],[0,64,0],[0,60,0],[0,50,60],[0,0,0],[0,0,0],[0,0,0],
    [152,150,152],[8,76,196],[48,50,236],[92,30,228],[136,20,176],[160,20,100],[152,34,32],[120,60,0],
    [84,90,0],[40,114,0],[8,124,0],[0,118,40],[0,102,120],[0,0,0],[0,0,0],[0,0,0],
    [236,238,236],[76,154,236],[120,124,236],[176,98,236],[228,84,236],[236,88,180],[236,106,100],[212,136,32],
    [160,170,0],[116,196,0],[76,208,32],[56,204,108],[56,180,204],[60,60,60],[0,0,0],[0,0,0],
    [236,238,236],[168,204,236],[188,188,236],[212,178,236],[236,174,236],[236,174,212],[236,180,176],[228,196,144],
    [204,210,120],[180,222,120],[168,226,144],[152,226,180],[160,214,228],[160,162,160],[0,0,0],[0,0,0],
];

pub const W: usize = 256;
pub const H: usize = 240;
const PRE_LINE: i32 = 261;
const LINE_DOTS: i32 = 341;
const FRAME_DOTS: i32 = 262 * LINE_DOTS;

/// bit b de i vai para o bit 2b: junta dois planos de padrão em pares de bits
const SPREAD: [u16; 256] = {
    let mut t = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut b = 0;
        while b < 8 {
            if i & (1 << b) != 0 {
                t[i] |= 1 << (b * 2);
            }
            b += 1;
        }
        i += 1;
    }
    t
};

const CHUNK_START: [i32; 42] = [
    1, 9, 17, 25, 33, 41, 49, 57, 65, 73, 81, 89, 97, 105, 113, 121, 129, 137, 145, 153, 161, 169,
    177, 185, 193, 201, 209, 217, 225, 233, 241, 249, 257, 265, 273, 281, 289, 297, 305, 313, 321,
    329,
];

#[inline(always)]
fn pal_index(a: u16) -> usize {
    let i = a & 0x1F;
    // $3F10/14/18/1C espelham $3F00/04/08/0C
    usize::from(if i & 0x13 == 0x10 { i & !0x10 } else { i })
}

/// página vazia para `chr_page` quando o cartucho usa CHR-RAM
static NO_CHR: [u8; 0x400] = [0; 0x400];

#[inline(always)]
fn reverse8(b: u8) -> u8 {
    b.reverse_bits()
}

#[repr(C)]
pub struct Ppu<'a> {
    // registradores
    pub ctrl: u8,
    pub mask: u8,
    pub status: u8,
    pub oam_addr: u8,
    /// endereço de VRAM atual e temporário ("loopy")
    pub v: u16,
    pub t: u16,
    pub fine_x: u8,
    /// trava de 1ª/2ª escrita em $2005/$2006
    pub w: bool,
    /// buffer de leitura de $2007
    pub read_buf: u8,
    /// barramento de dados interno (open bus da PPU)
    pub io_db: u8,
    /// quadro em que cada bit de `io_db` foi renovado (o bit volta a 0 depois de ~600 ms)
    pub db_stamp: [u32; 8],

    // memória: padrões e nomes em páginas de 1 KB, mapeadas pelo mapper
    /// deslocamento de cada página de 1 KB de $0000-$1FFF (em `chr_ram` se `chr_writable`);
    /// mudar só por [`Ppu::set_chr`]
    pub chr: [u32; 8],
    /// as páginas de CHR-ROM, como a tabela de ponteiros do C (tamanho fixo: sem checagem de
    /// limite na busca de padrões)
    chr_page: [&'a [u8; 0x400]; 8],
    pub chr_writable: bool,
    /// deslocamento em `vram` de cada tabela de nomes
    pub nt: [u16; 4],
    /// o cartucho conta subidas de A12 (MMC3)
    pub a12_hook: bool,
    pub a12_rises: u32,
    pub a12: bool,
    /// contador de pontos, para o filtro do A12
    pub dots: u32,
    pub a12_low_since: u32,
    /// `dots` absoluto do próximo evento previsto
    pub next_event_dot: u32,
    pub event_valid: bool,
    pub palette: [u8; 32],

    // temporização
    /// 0-239 visível, 240 pós, 241-260 vblank, 261 pré-render
    pub scanline: i16,
    /// 0-340
    pub dot: u16,
    pub odd_frame: bool,
    pub frame: u32,
    /// nível da linha NMI (ativa quando vblank e NMI habilitado)
    pub nmi_out: bool,
    /// $2002 lido 1 ponto antes do vblank: o flag não sobe neste quadro
    pub prevent_vbl: bool,
    /// fundo ou sprites ligados, com o atraso de 1 ponto do chip real
    pub rendering: bool,
    pub frame_done: bool,

    // pipeline de fundo
    pub bg_lo: u16,
    pub bg_hi: u16,
    pub at_lo: u16,
    pub at_hi: u16,
    pub nt_byte: u8,
    pub at_byte: u8,
    pub pat_lo: u8,
    pub pat_hi: u8,

    // sprites da linha atual
    pub spr_count: u8,
    pub spr_x: [u8; 8],
    pub spr_attr: [u8; 8],
    pub spr_lo: [u8; 8],
    pub spr_hi: [u8; 8],
    /// endereço do padrão de cada sprite (buscado em 257-320)
    pub spr_addr: [u16; 8],
    pub spr0_in_line: bool,
    pub spr_line_ok: bool,
    /// bit n = algum sprite opaco nos pixels 8n..8n+7
    pub spr_chunks: u32,

    // vetores grandes no fim: os campos de todo ponto ficam a deslocamento pequeno
    pub chr_rom: &'a [u8],
    pub chr_ram: [u8; 0x8000],
    pub vram: [u8; 0x800],
    pub oam: [u8; 256],
    /// sprites da linha por pixel: cor 0-1, paleta 2-3, atrás 5, sprite 0 bit 6
    pub spr_line: [u8; 256],
    /// índice de cor 0-63 por pixel
    pub fb: [[u8; W]; H],
}

impl<'a> Ppu<'a> {
    /// PPU zerada (como o `static` do C) e ligada.
    pub fn new(chr_rom: &'a [u8]) -> Self {
        Ppu {
            ctrl: 0,
            mask: 0,
            status: 0,
            oam_addr: 0,
            v: 0,
            t: 0,
            fine_x: 0,
            w: false,
            read_buf: 0,
            io_db: 0,
            db_stamp: [0; 8],
            chr_rom,
            chr_ram: [0; 0x8000],
            chr: [0; 8],
            chr_page: [&NO_CHR; 8],
            chr_writable: false,
            nt: [0; 4],
            a12_hook: false,
            a12_rises: 0,
            a12: false,
            dots: 0,
            a12_low_since: 0,
            next_event_dot: 0,
            event_valid: false,
            vram: [0; 0x800],
            palette: [0; 32],
            oam: [0; 256],
            scanline: 0,
            dot: 0,
            odd_frame: false,
            frame: 0,
            nmi_out: false,
            prevent_vbl: false,
            rendering: false,
            frame_done: false,
            bg_lo: 0,
            bg_hi: 0,
            at_lo: 0,
            at_hi: 0,
            nt_byte: 0,
            at_byte: 0,
            pat_lo: 0,
            pat_hi: 0,
            spr_count: 0,
            spr_x: [0; 8],
            spr_attr: [0; 8],
            spr_lo: [0; 8],
            spr_hi: [0; 8],
            spr_addr: [0; 8],
            spr0_in_line: false,
            spr_line: [0; 256],
            spr_line_ok: false,
            spr_chunks: 0,
            fb: [[0; W]; H],
        }
    }

    pub fn power(&mut self) {
        self.ctrl = 0;
        self.mask = 0;
        self.status = 0;
        self.oam_addr = 0;
        self.v = 0;
        self.t = 0;
        self.fine_x = 0;
        self.w = false;
        self.read_buf = 0;
        self.io_db = 0;
        self.scanline = 0;
        self.dot = 0;
        self.odd_frame = false;
        self.frame = 0;
        self.nmi_out = false;
        self.frame_done = false;
        self.spr_line_ok = false;
        self.event_valid = false;
    }

    /// Mapeia a página de 1 KB `slot` ($0000-$1FFF) no deslocamento `off` da CHR-ROM, ou da
    /// CHR-RAM se `chr_writable`.
    pub fn set_chr(&mut self, slot: usize, off: u32) {
        self.chr[slot] = off;
        if !self.chr_writable {
            let o = off as usize;
            // INFALLIBLE: o mapper só gera deslocamentos de bancos que existem (banco % total)
            self.chr_page[slot] = self.chr_rom[o..o + 0x400]
                .try_into()
                .expect("banco de CHR fora da ROM");
        }
    }

    /// Subidas de A12 desde a última chamada (o MMC3 conta uma linha por subida).
    pub fn take_a12_rises(&mut self) -> u32 {
        core::mem::take(&mut self.a12_rises)
    }

    #[inline(always)]
    fn render_on(&self) -> bool {
        self.mask & 0x18 != 0
    }
    #[inline(always)]
    fn update_nmi(&mut self) {
        self.nmi_out = self.status & 0x80 != 0 && self.ctrl & 0x80 != 0;
    }

    // ---- memória da PPU ----
    #[inline(always)]
    fn chr_at(&self, a: u16) -> u8 {
        let page = usize::from(a >> 10) & 7;
        if self.chr_writable {
            // o deslocamento é sempre menor que a CHR-RAM (32 KB): a máscara só tira a checagem
            self.chr_ram[(self.chr[page] as usize + usize::from(a & 0x3FF)) & 0x7FFF]
        } else {
            self.chr_page[page][usize::from(a & 0x3FF)]
        }
    }
    #[inline(always)]
    fn nt_at(&self, a: u16) -> u8 {
        // as tabelas ficam em 0 ou 0x400: a máscara só tira a checagem de limite
        self.vram[(usize::from(self.nt[usize::from(a >> 10) & 3]) + usize::from(a & 0x3FF)) & 0x7FF]
    }

    /// Acompanha A12 em cada acesso. Uma subida conta para o mapper se A12 ficou baixo por um
    /// tempo (o MMC3 real filtra pulsos curtos, como os das buscas de fundo e sprite na mesma
    /// linha).
    #[inline(always)]
    fn track_a12(&mut self, a: u16) {
        let hi = a & 0x1000 != 0;
        if hi && !self.a12 && self.dots.wrapping_sub(self.a12_low_since) > 10 && self.a12_hook {
            self.a12_rises += 1;
        }
        if !hi && self.a12 {
            self.a12_low_since = self.dots;
        }
        self.a12 = hi;
    }

    #[inline(always)]
    fn mem_read(&mut self, a: u16) -> u8 {
        let a = a & 0x3FFF;
        self.track_a12(a);
        if a < 0x2000 {
            self.chr_at(a)
        } else if a < 0x3F00 {
            self.nt_at(a)
        } else {
            self.palette[pal_index(a)]
        }
    }

    fn mem_write(&mut self, a: u16, v: u8) {
        let a = a & 0x3FFF;
        self.track_a12(a);
        if a < 0x2000 {
            if self.chr_writable {
                let off = self.chr[usize::from(a >> 10)] as usize + usize::from(a & 0x3FF);
                self.chr_ram[off & 0x7FFF] = v;
            }
        } else if a < 0x3F00 {
            let off = usize::from(self.nt[usize::from(a >> 10) & 3]) + usize::from(a & 0x3FF);
            self.vram[off] = v;
        } else {
            self.palette[pal_index(a)] = v & 0x3F;
        }
    }

    // ---- incrementos de rolagem ("loopy") ----
    #[inline(always)]
    fn inc_x(&mut self) {
        if self.v & 0x001F == 31 {
            self.v &= !0x001F;
            self.v ^= 0x0400;
        } else {
            self.v = self.v.wrapping_add(1);
        }
    }
    fn inc_y(&mut self) {
        if self.v & 0x7000 != 0x7000 {
            self.v += 0x1000;
            return;
        }
        self.v &= !0x7000;
        let mut y = (self.v & 0x03E0) >> 5;
        if y == 29 {
            y = 0;
            self.v ^= 0x0800;
        } else if y == 31 {
            y = 0;
        } else {
            y += 1;
        }
        self.v = (self.v & !0x03E0) | (y << 5);
    }
    #[inline(always)]
    fn copy_x(&mut self) {
        self.v = (self.v & !0x041F) | (self.t & 0x041F);
    }
    #[inline(always)]
    fn copy_y(&mut self) {
        self.v = (self.v & !0x7BE0) | (self.t & 0x7BE0);
    }

    #[inline(always)]
    fn load_shifters(&mut self) {
        self.bg_lo = (self.bg_lo & 0xFF00) | u16::from(self.pat_lo);
        self.bg_hi = (self.bg_hi & 0xFF00) | u16::from(self.pat_hi);
        self.at_lo = (self.at_lo & 0xFF00) | if self.at_byte & 1 != 0 { 0xFF } else { 0 };
        self.at_hi = (self.at_hi & 0xFF00) | if self.at_byte & 2 != 0 { 0xFF } else { 0 };
    }
    #[inline(always)]
    fn shift_bg(&mut self) {
        self.bg_lo <<= 1;
        self.bg_hi <<= 1;
        self.at_lo <<= 1;
        self.at_hi <<= 1;
    }
    #[inline(always)]
    fn shift_bg7(&mut self) {
        self.bg_lo <<= 7;
        self.bg_hi <<= 7;
        self.at_lo <<= 7;
        self.at_hi <<= 7;
    }

    #[inline(always)]
    fn at_addr(&self) -> u16 {
        0x23C0 | (self.v & 0x0C00) | ((self.v >> 4) & 0x38) | ((self.v >> 2) & 0x07)
    }
    #[inline(always)]
    fn at_bits(&self, raw: u8) -> u8 {
        let mut a = raw;
        if self.v & 0x40 != 0 {
            a >>= 4; // bit 1 do coarse Y
        }
        if self.v & 0x02 != 0 {
            a >>= 2; // bit 1 do coarse X
        }
        a & 3
    }
    #[inline(always)]
    fn bg_pat_addr(&self) -> u16 {
        (u16::from(self.ctrl & 0x10) << 8) | (u16::from(self.nt_byte) << 4) | (self.v >> 12)
    }

    /// Avaliação de sprites no ponto 257 da linha `line`: escolhe os da linha seguinte e busca
    /// os padrões.
    fn eval_sprites(&mut self, line: i32) {
        let h: u32 = if self.ctrl & 0x20 != 0 { 16 } else { 8 };
        self.spr_count = 0;
        self.spr0_in_line = false;
        self.spr_line_ok = false;
        // 1ª passada só lê a OAM: os até 9 sprites que cobrem a linha (o 9º só liga o overflow)
        let mut hits = [0u8; 9];
        let mut nh = 0;
        for (i, e) in self.oam.as_chunks::<4>().0.iter().enumerate() {
            if (line.wrapping_sub(i32::from(e[0])) as u32) < h {
                hits[nh] = i as u8;
                nh += 1;
                if nh == hits.len() {
                    break;
                }
            }
        }
        if nh == hits.len() {
            self.status |= 0x20;
            nh = 8;
        }
        for &i in &hits[..nh] {
            let o = usize::from(i) * 4;
            let mut row = line.wrapping_sub(i32::from(self.oam[o])) as u32;
            let tile = u16::from(self.oam[o + 1]);
            let attr = self.oam[o + 2];
            if attr & 0x80 != 0 {
                row = h - 1 - row; // espelho vertical
            }
            let n = usize::from(self.spr_count);
            self.spr_count += 1;
            let row = row as u16;
            self.spr_addr[n] = if h == 16 {
                ((tile & 1) << 12) | ((tile & 0xFE) << 4) | ((row & 8) << 1) | (row & 7)
            } else {
                (u16::from(self.ctrl & 0x08) << 9) | (tile << 4) | row
            };
            self.spr_x[n] = self.oam[o + 3];
            self.spr_attr[n] = attr;
            if i == 0 {
                self.spr0_in_line = true;
            }
        }
        // o chip sempre faz 8 buscas: as vagas leem o tile $FF (é o que move A12 para o MMC3)
        let empty = if h == 16 {
            0x1000 | (0xFE << 4)
        } else {
            (u16::from(self.ctrl & 0x08) << 9) | (0xFF << 4)
        };
        for n in usize::from(self.spr_count)..8 {
            self.spr_addr[n] = empty;
        }
    }

    /// Busca de sprite nos pontos 257-320: 8 pontos por sprite, padrão baixo no 5º e alto no
    /// 7º. Os pontos 1º e 3º leem a tabela de nomes (lixo), o que mantém A12 baixo entre as
    /// buscas.
    #[inline(always)]
    fn fetch_sprite(&mut self, d: i32) {
        let n = ((d - 257) >> 3) as usize;
        let ph = (d - 257) & 7;
        if ph == 0 || ph == 2 {
            self.mem_read(0x2000 | (self.v & 0x0FFF));
            return;
        }
        if ph != 4 && ph != 6 {
            return;
        }
        let mut b = self.mem_read(self.spr_addr[n] + if ph == 6 { 8 } else { 0 });
        if n < usize::from(self.spr_count) && self.spr_attr[n] & 0x40 != 0 {
            b = reverse8(b); // espelho horizontal
        }
        if ph == 4 {
            self.spr_lo[n] = b;
        } else {
            self.spr_hi[n] = b;
        }
        self.spr_line_ok = false;
    }

    /// Monta os sprites da linha pixel a pixel: o de menor índice com cor não transparente
    /// ganha.
    fn build_spr_line(&mut self) {
        // fora dos blocos marcados em spr_chunks a linha já é zero: só esses precisam ser limpos
        let mut old = self.spr_chunks;
        while old != 0 {
            let c = old.trailing_zeros() as usize * 8;
            self.spr_line[c..c + 8].fill(0);
            old &= old - 1;
        }
        self.spr_chunks = 0;
        for i in (0..usize::from(self.spr_count)).rev() {
            let (lo, hi) = (self.spr_lo[i], self.spr_hi[i]);
            if lo == 0 && hi == 0 {
                continue;
            }
            let attr = self.spr_attr[i];
            let meta = ((attr & 3) << 2)
                | (attr & 0x20)
                | if i == 0 && self.spr0_in_line { 0x40 } else { 0 };
            for b in 0..8 {
                let x = usize::from(self.spr_x[i]) + b;
                if x > 255 {
                    break;
                }
                let c = (((hi >> (7 - b)) & 1) << 1) | ((lo >> (7 - b)) & 1);
                if c != 0 {
                    self.spr_line[x] = meta | c;
                    self.spr_chunks |= 1 << (x >> 3);
                }
            }
        }
        self.spr_line_ok = true;
    }

    fn render_pixel(&mut self) {
        let x = usize::from(self.dot) - 1;
        let (mut bg, mut bg_pal, mut spr, mut spr_pal) = (0u8, 0u8, 0u8, 0u8);
        let (mut spr_behind, mut spr_zero) = (false, false);

        if self.mask & 0x08 != 0 && (x >= 8 || self.mask & 0x02 != 0) {
            let bit = 0x8000u16 >> self.fine_x;
            bg = (if self.bg_hi & bit != 0 { 2 } else { 0 }) | u8::from(self.bg_lo & bit != 0);
            bg_pal = (if self.at_hi & bit != 0 { 2 } else { 0 }) | u8::from(self.at_lo & bit != 0);
        }
        if self.mask & 0x10 != 0 && (x >= 8 || self.mask & 0x04 != 0) {
            if !self.spr_line_ok {
                self.build_spr_line();
            }
            let s = self.spr_line[x];
            spr = s & 3;
            spr_pal = ((s >> 2) & 3) + 4;
            spr_behind = s & 0x20 != 0;
            spr_zero = s & 0x40 != 0;
        }
        if spr_zero && bg != 0 && spr != 0 && x != 255 {
            self.status |= 0x40;
        }

        let idx = if bg == 0 && spr == 0 {
            0
        } else if bg == 0 || (spr != 0 && !spr_behind) {
            (spr_pal << 2) | spr
        } else {
            (bg_pal << 2) | bg
        };
        let mut color = self.palette[pal_index(u16::from(idx))];
        if self.mask & 0x01 != 0 {
            color &= 0x30; // tons de cinza
        }
        self.fb[self.scanline as usize][x] = color;
    }

    /// Cor de fundo com a renderização desligada: a paleta apontada por v, se v estiver nela.
    #[inline(always)]
    fn blank_color(&self) -> u8 {
        let c = if self.v & 0x3F00 == 0x3F00 {
            self.palette[pal_index(self.v)]
        } else {
            self.palette[0]
        };
        if self.mask & 0x01 != 0 { c & 0x30 } else { c }
    }

    /// Avança um ponto (referência).
    pub fn step(&mut self) {
        let mut sl = i32::from(self.scanline);
        let mut d = i32::from(self.dot);
        let rend = self.rendering;
        self.dots = self.dots.wrapping_add(1);

        if sl < 240 || sl == PRE_LINE {
            if sl == PRE_LINE && d == 1 {
                self.status &= !0xE0; // limpa vblank, sprite 0 e overflow
                self.prevent_vbl = false;
            }

            if rend {
                // desloca antes de desenhar: o pixel x usa o registrador já deslocado x vezes
                if (2..=257).contains(&d) || (322..=337).contains(&d) {
                    self.shift_bg();
                }
                if sl < 240 && (1..=256).contains(&d) {
                    self.render_pixel();
                }

                if (1..=256).contains(&d) || (321..=336).contains(&d) {
                    match (d - 1) & 7 {
                        0 => {
                            if d != 1 && d != 321 {
                                self.load_shifters();
                            }
                            self.nt_byte = self.mem_read(0x2000 | (self.v & 0x0FFF));
                        }
                        2 => {
                            let a = self.mem_read(self.at_addr());
                            self.at_byte = self.at_bits(a);
                        }
                        4 => self.pat_lo = self.mem_read(self.bg_pat_addr()),
                        6 => self.pat_hi = self.mem_read(self.bg_pat_addr() | 8),
                        7 => self.inc_x(),
                        _ => {}
                    }
                }
                if d == 256 {
                    self.inc_y();
                }
                if d == 257 {
                    self.load_shifters();
                    self.copy_x();
                    self.eval_sprites(if sl < 240 { sl } else { -1000 }); // pré-render: só as buscas vazias
                }
                if d == 337 {
                    self.load_shifters(); // 2º tile da próxima linha (o 1º carrega no 329, pelo case 0)
                }
                if (257..=320).contains(&d) {
                    self.oam_addr = 0;
                    self.fetch_sprite(d);
                }
                if sl == PRE_LINE && (280..=304).contains(&d) {
                    self.copy_y();
                }
            } else if sl < 240 && (1..=256).contains(&d) {
                self.fb[sl as usize][(d - 1) as usize] = self.blank_color();
            }
        } else if sl == 241 && d == 1 {
            if !self.prevent_vbl {
                self.status |= 0x80;
            }
            self.frame_done = true;
        }
        self.update_nmi();
        self.rendering = self.render_on(); // uma escrita em $2001 só vale a partir do próximo ponto

        // avança; em quadro ímpar com renderização, a linha pré-render perde o último ponto
        if sl == PRE_LINE && d == 339 && rend && self.odd_frame {
            d = 340;
        }
        d += 1;
        if d > 340 {
            d = 0;
            sl += 1;
            if sl > PRE_LINE {
                sl = 0;
                self.odd_frame = !self.odd_frame;
                self.frame = self.frame.wrapping_add(1);
            }
        }
        self.dot = d as u16;
        self.scanline = sl as i16;
    }

    /// $2000-$2007
    pub fn read_reg(&mut self, addr: u16) -> u8 {
        match addr & 7 {
            2 => {
                self.db_apply(0x1F, self.status & 0xE0);
                self.status &= 0x7F;
                self.w = false;
                self.nmi_out = false;
                if self.scanline == 241 && self.dot == 1 {
                    self.prevent_vbl = true; // o próximo ponto é o do vblank
                }
            }
            4 => {
                let mut v = self.oam[usize::from(self.oam_addr)];
                if self.oam_addr & 3 == 2 {
                    v &= 0xE3; // bits não implementados do atributo
                }
                self.db_apply(0x00, v);
            }
            7 => {
                let a = self.v & 0x3FFF;
                if a >= 0x3F00 {
                    // paleta: só 6 bits; os 2 de cima são o barramento interno
                    let pal = self.mem_read(a);
                    self.db_apply(0xC0, pal);
                    self.read_buf = self.mem_read(a - 0x1000); // buffer recebe a tabela de nomes por baixo
                } else {
                    self.db_apply(0x00, self.read_buf);
                    self.read_buf = self.mem_read(a);
                }
                self.inc_v();
            }
            _ => {
                // registradores só de escrita: devolve o barramento interno (que decai)
                self.db_apply(0xFF, 0);
            }
        }
        self.io_db
    }

    /// Barramento interno da PPU (como o Mesen): os bits fora de `keep` recebem `value` e são
    /// renovados; os de `keep` continuam, mas voltam a 0 se ficaram mais de 30 quadros sem
    /// renovação.
    fn db_apply(&mut self, keep: u8, value: u8) {
        let mut db = self.io_db;
        for (i, stamp) in self.db_stamp.iter_mut().enumerate() {
            let bit = 1u8 << i;
            if keep & bit == 0 {
                db = (db & !bit) | (value & bit);
                *stamp = self.frame;
            } else if self.frame.wrapping_sub(*stamp) > 30 {
                db &= !bit;
            }
        }
        self.io_db = db;
    }

    /// $2007 avança v; o endereço incrementado fica no barramento.
    fn inc_v(&mut self) {
        self.v = self
            .v
            .wrapping_add(if self.ctrl & 0x04 != 0 { 32 } else { 1 })
            & 0x7FFF;
        self.track_a12(self.v);
        self.event_valid = false;
    }

    pub fn write_reg(&mut self, addr: u16, v: u8) {
        self.io_db = v;
        self.db_stamp = [self.frame; 8];
        match addr & 7 {
            0 => {
                self.ctrl = v;
                self.t = (self.t & !0x0C00) | (u16::from(v & 3) << 10);
                self.update_nmi();
                self.event_valid = false;
            }
            1 => {
                self.mask = v;
                self.event_valid = false;
            }
            3 => self.oam_addr = v,
            4 => {
                self.oam[usize::from(self.oam_addr)] = v;
                self.oam_addr = self.oam_addr.wrapping_add(1);
                self.event_valid = false;
            }
            5 => {
                if !self.w {
                    self.t = (self.t & !0x001F) | u16::from(v >> 3);
                    self.fine_x = v & 7;
                } else {
                    self.t =
                        (self.t & !0x73E0) | (u16::from(v & 7) << 12) | (u16::from(v & 0xF8) << 2);
                }
                self.w = !self.w;
            }
            6 => {
                if !self.w {
                    self.t = (self.t & 0x00FF) | (u16::from(v & 0x3F) << 8);
                } else {
                    self.t = (self.t & 0xFF00) | u16::from(v);
                    self.v = self.t;
                    self.track_a12(self.v); // v vai para o barramento
                }
                self.w = !self.w;
                self.event_valid = false;
            }
            7 => {
                self.mem_write(self.v, v);
                self.inc_v();
            }
            _ => {}
        }
    }

    // =====================================================================================
    // Execução rápida. O resultado tem que ser idêntico ao de `step` ponto a ponto.
    // =====================================================================================

    /// Quantos pontos executar para que o ponto (sl, d) seja o último (1 = o próximo).
    fn dots_until(&self, sl: i32, d: i32) -> u32 {
        let cur = i32::from(self.scanline) * LINE_DOTS + i32::from(self.dot);
        let mut t = sl * LINE_DOTS + d;
        if t < cur {
            t += FRAME_DOTS;
        }
        let mut k = (t - cur + 1) as u32;
        let skip = PRE_LINE * LINE_DOTS + 340; // ponto pulado em quadro ímpar com renderização
        if cur <= skip && skip < t && self.render_on() && self.odd_frame {
            k -= 1;
        }
        k
    }

    /// Bit A12 dos endereços de padrão dos 8 sprites que a avaliação do ponto 257 vai escolher.
    fn predict_spr_a12(&self, line: i32) -> u8 {
        if self.ctrl & 0x20 == 0 {
            return if self.ctrl & 0x08 != 0 { 0xFF } else { 0x00 };
        }
        let mut bits = 0u8;
        let mut n = 0;
        for e in self.oam.as_chunks::<4>().0.iter() {
            if !(0..16).contains(&(line - i32::from(e[0]))) {
                continue;
            }
            if e[1] & 1 != 0 {
                bits |= 1 << n;
            }
            n += 1;
            if n >= 8 {
                break;
            }
        }
        while n < 8 {
            bits |= 1 << n; // buscas vazias: tile $FF
            n += 1;
        }
        bits
    }

    fn cur_spr_a12(&self) -> u8 {
        let mut bits = 0u8;
        for (n, a) in self.spr_addr.iter().enumerate() {
            if a & 0x1000 != 0 {
                bits |= 1 << n;
            }
        }
        bits
    }

    /// Próxima subida de A12 que o mapper vai contar, em pontos a partir de agora (0 = nenhuma
    /// até `limit`). Percorre só as buscas (4 por bloco de 8 pontos), simulando o filtro de
    /// `track_a12`.
    fn next_a12_event(&self, limit: u32) -> u32 {
        if !self.a12_hook || !self.render_on() {
            return 0;
        }
        let bg_hi = self.ctrl & 0x10 != 0;
        let scanline = i32::from(self.scanline);
        let dot = i32::from(self.dot);

        // caminho analítico para BG em $0000 e sprites 8x8: subida única por linha no ponto 261
        if !bg_hi && self.ctrl & 0x20 == 0 {
            if self.ctrl & 0x08 == 0 {
                return 0;
            }
            let next_sl = if dot <= 261 && (scanline < 240 || scanline == PRE_LINE) {
                scanline
            } else if scanline < 239 {
                scanline + 1
            } else if scanline < PRE_LINE {
                PRE_LINE
            } else {
                0
            };
            let k = self.dots_until(next_sl, 261);
            return if k <= limit { k } else { 0 };
        }

        let mut a12 = self.a12;
        let mut low_since = self.a12_low_since;
        for step in 0..263 {
            let sl = (scanline + step) % 262;
            if (240..PRE_LINE).contains(&sl) {
                continue;
            }
            let line_base = if step == 0 { 0 } else { self.dots_until(sl, 1) };
            if step > 0 && line_base > limit {
                return 0;
            }
            let eval_line = if sl < 240 { sl } else { -1000 };

            if step == 0 && !bg_hi && dot > 257 && self.cur_spr_a12() == 0xFF {
                a12 = false;
                low_since = if dot < 321 {
                    self.dots.wrapping_add((321 - dot + 1) as u32)
                } else {
                    self.a12_low_since
                };
                continue;
            }

            // BG em $0000 e A12 baixo há > 10 pontos: o primeiro sprite com A12=1 dispara a subida
            if !bg_hi
                && !a12
                && (step > 0 || (dot <= 257 && self.dots.wrapping_sub(low_since) > 10))
            {
                let spr_bits = self.predict_spr_a12(eval_line);
                if spr_bits != 0 {
                    let n = spr_bits.trailing_zeros() as i32;
                    let rise_dot = 257 + (n << 3) + 4;
                    let k = if step == 0 {
                        (rise_dot - dot + 1) as u32
                    } else {
                        line_base + (rise_dot - 1) as u32
                    };
                    return if k <= limit { k } else { 0 };
                }
            }

            let mut spr_bits = 0u8;
            let mut spr_known = false;
            for (c, &d0) in CHUNK_START.iter().enumerate() {
                if !bg_hi && !a12 && c < 32 {
                    continue;
                }
                if step == 0 && d0 + 6 < dot {
                    continue; // bloco já passou
                }
                let hi_fetch = if (257..321).contains(&d0) {
                    if !spr_known {
                        spr_known = true;
                        spr_bits = if step == 0 && dot > 257 {
                            self.cur_spr_a12()
                        } else {
                            self.predict_spr_a12(eval_line)
                        };
                    }
                    spr_bits & (1 << ((d0 - 257) >> 3)) != 0
                } else {
                    bg_hi
                };
                if !hi_fetch && !a12 {
                    continue; // bloco todo em A12 baixo, que já está baixo
                }
                for ph in [0, 2, 4, 6] {
                    let d = d0 + ph;
                    if step == 0 && d < dot {
                        continue;
                    }
                    let k = if step == 0 {
                        (d - dot + 1) as u32
                    } else {
                        line_base + (d - 1) as u32
                    };
                    if k > limit {
                        return 0;
                    }
                    let dots = self.dots.wrapping_add(k);
                    let hi = ph >= 4 && hi_fetch;
                    if hi && !a12 && dots.wrapping_sub(low_since) > 10 {
                        return k;
                    }
                    if !hi && a12 {
                        low_since = dots;
                    }
                    a12 = hi;
                }
            }
        }
        0
    }

    /// Pontos até o próximo sinal externo mudar (vblank, A12), no máximo `limit`.
    pub fn dots_to_event(&mut self, limit: u32) -> u32 {
        if self.event_valid {
            let rem = self.next_event_dot.wrapping_sub(self.dots) as i32;
            if rem > 0 {
                return (rem as u32).min(limit);
            }
        }
        let k = self
            .dots_until(241, 1)
            .min(self.dots_until(PRE_LINE, 1))
            .min(limit); // vblank, fim do vblank
        let a = self.next_a12_event(k);
        let res = if a != 0 { a } else { k };
        self.next_event_dot = self.dots.wrapping_add(res);
        self.event_valid = true;
        res
    }

    /// Começo de bloco de busca: a leitura da tabela de nomes baixa A12 no 1º ponto.
    #[inline(always)]
    fn a12_low_at(&mut self, dots0: u32) {
        if self.a12 {
            self.dots = dots0.wrapping_add(1);
            self.a12_low_since = self.dots;
            self.a12 = false;
        }
    }

    /// Busca de um tile de fundo (NT, AT e os dois planos), como nos pontos 1-8 do bloco.
    #[inline(always)]
    fn fast_bg_tile(&mut self, dots0: u32) {
        self.a12_low_at(dots0);
        self.nt_byte = self.nt_at(0x2000 | (self.v & 0x0FFF));
        let raw = self.nt_at(self.at_addr());
        self.at_byte = self.at_bits(raw);
        let pat_addr = self.bg_pat_addr();
        if pat_addr & 0x1000 != 0 {
            self.dots = dots0.wrapping_add(5);
            self.track_a12(pat_addr);
        }
        self.pat_lo = self.chr_at(pat_addr);
        self.pat_hi = self.chr_at(pat_addr + 8);
        self.dots = dots0.wrapping_add(8);
        self.inc_x();
    }

    /// Executa `count` blocos de 8 pontos de linha visível com renderização estável.
    fn fast_chunks(&mut self, count: u32) {
        let mask = self.mask;
        let grey = if mask & 0x01 != 0 { 0x30 } else { 0x3F };
        let spr_on = mask & 0x10 != 0;
        let bg_on_base = mask & 0x08 != 0;
        let fx = u32::from(self.fine_x);
        let sl = self.scanline as usize;
        let mut d = i32::from(self.dot);
        // os registradores de deslocamento ficam em variáveis durante o laço (nada mais os lê
        // aqui dentro) e voltam para a PPU no fim
        let (mut bg_lo, mut bg_hi, mut at_lo, mut at_hi) =
            (self.bg_lo, self.bg_hi, self.at_lo, self.at_hi);

        for _ in 0..count {
            let x0 = (d - 1) as usize;
            let spr_here =
                spr_on && (x0 >= 8 || mask & 0x04 != 0) && (self.spr_chunks >> (x0 >> 3)) & 1 != 0;
            let bg_on = bg_on_base && (x0 >= 8 || mask & 0x02 != 0);
            let dots0 = self.dots;

            let (full_lo, full_hi, full_alo, full_ahi) = if d != 1 {
                (
                    ((bg_lo << 1) & 0xFF00) | u16::from(self.pat_lo),
                    ((bg_hi << 1) & 0xFF00) | u16::from(self.pat_hi),
                    ((at_lo << 1) & 0xFF00) | if self.at_byte & 1 != 0 { 0xFF } else { 0 },
                    ((at_hi << 1) & 0xFF00) | if self.at_byte & 2 != 0 { 0xFF } else { 0 },
                )
            } else {
                (bg_lo, bg_hi, at_lo, at_hi)
            };
            let cur = |r: u16| ((u32::from(r) << fx) >> 8) as u8;

            let (mut pat, mut att) = (0u16, 0u16);
            let mut bg_zero = !bg_on;
            if bg_on {
                pat = (SPREAD[usize::from(cur(full_hi))] << 1) | SPREAD[usize::from(cur(full_lo))];
                if pat == 0 {
                    bg_zero = true;
                } else {
                    att = (SPREAD[usize::from(cur(full_ahi))] << 1)
                        | SPREAD[usize::from(cur(full_alo))];
                }
            }

            // leituras diretas: NT e AT em $2000-$2FFF (A12 sempre 0); padrões em CHR
            self.fast_bg_tile(dots0);
            if d + 7 == 256 {
                self.inc_y();
            }

            bg_lo = full_lo << 7;
            bg_hi = full_hi << 7;
            at_lo = full_alo << 7;
            at_hi = full_ahi << 7;
            // índice de cor de cada pixel do fundo: 2 bits do padrão e, onde o padrão não é 0,
            // 2 bits do atributo (a máscara faz o "pcol ? ... : 0" para os 8 de uma vez)
            let att = att & (((pat | (pat >> 1)) & 0x5555) * 3);
            let bg_idx =
                |k: usize| (((att >> (14 - 2 * k)) & 3) << 2 | ((pat >> (14 - 2 * k)) & 3)) as u8;
            let pal = &self.palette;
            let mut px = [0u8; 8];
            if !spr_here {
                if bg_zero {
                    px = [pal[0] & grey; 8];
                } else {
                    for (k, o) in px.iter_mut().enumerate() {
                        *o = pal[usize::from(bg_idx(k))] & grey;
                    }
                }
            } else {
                let spl: &[u8; 8] = self.spr_line[x0..x0 + 8].try_into().unwrap_or(&[0; 8]);
                let mut hit = false;
                for (k, (o, &sp)) in px.iter_mut().zip(spl).enumerate() {
                    let b = bg_idx(k);
                    let mut idx = b;
                    if sp & 3 != 0 {
                        if sp & 0x40 != 0 && b != 0 && x0 + k != 255 {
                            hit = true; // sprite 0
                        }
                        idx = if b != 0 && sp & 0x20 != 0 {
                            b
                        } else {
                            0x10 | (sp & 0x0F)
                        };
                    }
                    *o = pal[usize::from(idx)] & grey; // idx com cor nunca cai nos espelhos $3F10/14/18/1C
                }
                if hit {
                    self.status |= 0x40;
                }
            }
            self.fb[sl][x0..x0 + 8].copy_from_slice(&px);
            d += 8;
        }
        (self.bg_lo, self.bg_hi, self.at_lo, self.at_hi) = (bg_lo, bg_hi, at_lo, at_hi);
        self.dot = d as u16;
    }

    #[inline(always)]
    fn spr_pattern(&self, n: usize, a: u16) -> u8 {
        let b = self.chr_at(a & 0x1FFF);
        if n < usize::from(self.spr_count) && self.spr_attr[n] & 0x40 != 0 {
            reverse8(b)
        } else {
            b
        }
    }

    /// Executa a busca de sprite n (8 pontos).
    #[inline(always)]
    fn fast_spr_fetch(&mut self, n: usize) {
        let dots0 = self.dots;
        self.oam_addr = 0;
        self.a12_low_at(dots0);
        let a_lo = self.spr_addr[n];
        if a_lo & 0x1000 != 0 {
            self.dots = dots0.wrapping_add(5);
            self.track_a12(a_lo);
        }
        self.spr_lo[n] = self.spr_pattern(n, a_lo);
        self.spr_hi[n] = self.spr_pattern(n, a_lo + 8);
        self.spr_line_ok = false;
        self.dots = dots0.wrapping_add(8);
        self.dot += 8;
    }

    /// Ponto 257: deslocamento final, loopy e avaliação de sprites.
    fn spr_start(&mut self) {
        self.shift_bg();
        self.load_shifters();
        self.copy_x();
        self.eval_sprites(i32::from(self.scanline));
    }

    /// Ponto 257 e a busca do sprite 0.
    fn fast_spr_start(&mut self) {
        self.spr_start();
        self.fast_spr_fetch(0);
    }

    /// Pontos 257-261: deslocamento, loopy, avaliação, buscas lixo e padrão baixo do sprite 0.
    fn fast_spr_start_to_261(&mut self) {
        let dots0 = self.dots;
        self.spr_start();
        self.oam_addr = 0;
        self.a12_low_at(dots0);
        let a_lo = self.spr_addr[0];
        self.dots = dots0.wrapping_add(5);
        self.track_a12(a_lo);
        self.spr_lo[0] = self.spr_pattern(0, a_lo);
        self.spr_line_ok = false;
        self.dot = 262;
    }

    /// Pontos 262-264: ponto 262 e busca do padrão alto do sprite 0 (pontos 263-264).
    fn fast_spr_start_from_262(&mut self) {
        let dots0 = self.dots;
        self.oam_addr = 0;
        let a_hi = self.spr_addr[0] + 8;
        self.dots = dots0.wrapping_add(2);
        self.track_a12(a_hi);
        self.spr_hi[0] = self.spr_pattern(0, a_hi);
        self.spr_line_ok = false;
        self.dots = dots0.wrapping_add(3);
        self.dot = 265;
    }

    /// Busca do 1º tile de fundo da próxima linha (pontos 321-328).
    #[inline(always)]
    fn fast_bg_fetch_1(&mut self) {
        self.fast_bg_tile(self.dots);
        self.shift_bg7();
        self.dot = 329;
    }

    /// Busca do 2º tile de fundo da próxima linha (pontos 329-336).
    #[inline(always)]
    fn fast_bg_fetch_2(&mut self) {
        let dots0 = self.dots;
        self.shift_bg();
        self.load_shifters();
        self.fast_bg_tile(dots0);
        self.shift_bg7();
        self.dot = 337;
    }

    /// Fim da linha e ponto 0 da próxima (pontos 337-340 e 0, 5 pontos).
    #[inline(always)]
    fn fast_line_tail(&mut self) {
        self.shift_bg();
        self.load_shifters();
        self.dots = self.dots.wrapping_add(5);
        self.dot = 1;
        self.scanline += 1;
        self.update_nmi();
        self.rendering = self.render_on();
    }

    /// A partir de 265 até o fim da linha e o ponto 0 (77 pontos).
    #[inline(always)]
    fn fast_line_from_265(&mut self) {
        for k in 1..8 {
            self.fast_spr_fetch(k);
        }
        self.fast_bg_fetch_1();
        self.fast_bg_fetch_2();
        self.fast_line_tail();
    }

    /// Avança `n` pontos, com atalhos onde nada externo acontece.
    pub fn run(&mut self, mut n: u32) {
        while n != 0 {
            let mut sl = i32::from(self.scanline);
            let mut d = i32::from(self.dot);
            let rend = self.render_on();
            if self.rendering != rend {
                // $2001 acabou de mudar
                self.step();
                n -= 1;
                continue;
            }
            if rend && sl < 240 {
                if (1..=249).contains(&d) && (d - 1) & 7 == 0 && n >= 8 {
                    if self.mask & 0x10 != 0 && !self.spr_line_ok {
                        self.build_spr_line();
                    }
                    let avail = ((257 - d) as u32 >> 3).min(n >> 3);
                    self.fast_chunks(avail);
                    n -= avail * 8;
                    continue;
                }
                let took = match d {
                    257 if n >= 85 => {
                        self.fast_spr_start();
                        self.fast_line_from_265();
                        85
                    }
                    265 if n >= 77 => {
                        self.fast_line_from_265();
                        77
                    }
                    257 if n >= 8 => {
                        self.fast_spr_start();
                        8
                    }
                    257 if n >= 5 => {
                        self.fast_spr_start_to_261();
                        5
                    }
                    262 if n >= 3 => {
                        self.fast_spr_start_from_262();
                        3
                    }
                    265..=313 if (d - 257) & 7 == 0 && n >= 8 => {
                        self.fast_spr_fetch(((d - 257) >> 3) as usize);
                        8
                    }
                    321 if n >= 8 => {
                        self.fast_bg_fetch_1();
                        8
                    }
                    329 if n >= 8 => {
                        self.fast_bg_fetch_2();
                        8
                    }
                    337 if n >= 5 => {
                        self.fast_line_tail();
                        5
                    }
                    _ => 0,
                };
                if took != 0 {
                    n -= took;
                    continue;
                }
            }
            let vblank = (240..PRE_LINE).contains(&sl) && !(sl == 241 && d <= 1); // nada acontece
            let blank_line = !rend && sl < 240; // linha visível sem renderização
            if vblank || blank_line {
                let mut k = ((341 - d) as u32).min(n);
                if blank_line && (1..=256).contains(&d) {
                    let end = (d as u32 + k - 1).min(256);
                    k = end - d as u32 + 1;
                    let c = self.blank_color();
                    self.fb[sl as usize][(d - 1) as usize..end as usize].fill(c);
                } else if blank_line && d == 0 {
                    k = 1;
                }
                self.dots = self.dots.wrapping_add(k);
                d += k as i32;
                if d > 340 {
                    d = 0;
                    sl += 1;
                }
                self.dot = d as u16;
                self.scanline = sl as i16;
                self.update_nmi();
                n -= k;
                continue;
            }
            self.step();
            n -= 1;
        }
    }
}

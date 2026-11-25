//! Fundo atrás do menu (background), já escurecido como no tema (60%). Os fundos ficam no cartão,
//! em `pocketmister/backgrounds/*.fnd`; o firmware só tem embutido o liso, para quando a pasta estiver
//! vazia. De onde vêm os arquivos:
//! - os fundos gerados aqui ([`Estilo`]: paisagem, noite estrelada, synthwave, xadrez, barras de
//!   teste, liso), todos nossos;
//! - arquivos `.fnd` do cartão (`pocketmister/backgrounds/*.fnd`), feitos pelo `cargo xtask background` a partir de
//!   qualquer imagem: `PNFUNDO1`, 160 cores RGB e 320x240 índices (0-159). Sem decodificar PNG na
//!   placa: é só copiar.

use crate::canvas::{Canvas, HEIGHT, WIDTH};
use crate::theme::{BACKGROUND_BASE, BACKGROUND_COLORS};

pub const MAGIC: &[u8; 8] = b"PNFUNDO1";
pub const FILE_SIZE: usize = 8 + BACKGROUND_COLORS * 3 + WIDTH * HEIGHT;

/// O escurecimento do tema: 60% (o ES-DE desenha o fundo com opacidade 0.6 sobre preto).
const fn darken(c: u8) -> u8 {
    (c as u16 * 0x99 / 0xFF) as u8
}

pub struct Background {
    pub px: [[u8; WIDTH]; HEIGHT],
    pub palette: [[u8; 3]; BACKGROUND_COLORS],
}

impl Background {
    /// Lê um `.fnd`; None se o arquivo não tem o formato certo.
    pub fn from_file(data: &[u8]) -> Option<Self> {
        if data.len() != FILE_SIZE || &data[..8] != MAGIC {
            return None;
        }
        let mut f = Background {
            px: [[0; WIDTH]; HEIGHT],
            palette: [[0; 3]; BACKGROUND_COLORS],
        };
        for (c, rgb) in f.palette.iter_mut().zip(data[8..].as_chunks::<3>().0) {
            *c = *rgb;
        }
        let pix = &data[8 + BACKGROUND_COLORS * 3..];
        for (row, src) in f.px.iter_mut().zip(pix.as_chunks::<WIDTH>().0) {
            for (d, &s) in row.iter_mut().zip(src) {
                *d = BACKGROUND_BASE + s.min(BACKGROUND_COLORS as u8 - 1);
            }
        }
        Some(f)
    }

    /// A paisagem padrão.
    pub fn landscape() -> Self {
        let mut f = Background {
            px: [[0; WIDTH]; HEIGHT],
            palette: [[0; 3]; BACKGROUND_COLORS],
        };
        // céu: 96 tons do roxo da noite ao laranja do horizonte
        const SKY: usize = 96;
        let top = [0x24, 0x18, 0x48];
        let horizon = [0xF0, 0x8A, 0x6A];
        for (i, c) in f.palette.iter_mut().take(SKY).enumerate() {
            let t = i as u32;
            let mix = |a: u8, b: u8| {
                ((a as u32 * (SKY as u32 - 1 - t) + b as u32 * t) / (SKY as u32 - 1)) as u8
            };
            *c = [
                darken(mix(top[0], horizon[0])),
                darken(mix(top[1], horizon[1])),
                darken(mix(top[2], horizon[2])),
            ];
        }
        // montanhas: 3 camadas, do fundo (mais clara) para a frente (mais escura), 2 tons cada
        let layers: [([u8; 3], [u8; 3], usize, u32); 3] = [
            ([0x9A, 0x5E, 0x8C], [0x86, 0x50, 0x7C], 120, 0x1234),
            ([0x5E, 0x34, 0x6A], [0x4E, 0x2A, 0x5A], 150, 0xBEEF),
            ([0x2E, 0x18, 0x3C], [0x24, 0x12, 0x30], 182, 0x7777),
        ];
        for (k, (lighter, darker, _, _)) in layers.iter().enumerate() {
            f.palette[SKY + k * 2] = lighter.map(darken);
            f.palette[SKY + k * 2 + 1] = darker.map(darken);
        }
        // estrelas
        f.palette[SKY + 6] = [0xFF, 0xF4, 0xD8].map(darken);

        for (y, row) in f.px.iter_mut().enumerate() {
            // degradê em faixas de 2 linhas, como pixel art
            let shade = ((y / 2) * SKY / (HEIGHT / 2)).min(SKY - 1);
            row.fill(BACKGROUND_BASE + shade as u8);
        }
        let mut seed = 0x9E37_79B9u32;
        let mut rnd = || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        };
        for _ in 0..70 {
            let (x, y) = (rnd() as usize % WIDTH, rnd() as usize % 90);
            f.px[y][x] = BACKGROUND_BASE + SKY as u8 + 6;
        }
        for (k, (_, _, base, seed)) in layers.iter().enumerate() {
            let mut s = *seed;
            let mut h: i32 = 0;
            let mut target: i32 = 0;
            for x in 0..WIDTH {
                // alvo novo a cada 24 colunas; a altura anda até ele de 1 em 1 (encostas em degrau)
                if x % 24 == 0 {
                    s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
                    target = ((s >> 16) % 46) as i32 - 10;
                }
                h += (target - h).signum();
                let top = (*base as i32 - h).clamp(0, HEIGHT as i32 - 1) as usize;
                for (y, row) in f.px.iter_mut().enumerate().skip(top) {
                    // lado iluminado: as colunas à esquerda do pico de cada degrau
                    let shade = if (x / 6 + y / 6) % 3 == 0 { 1 } else { 0 };
                    row[x] = BACKGROUND_BASE + (SKY + k * 2 + shade) as u8;
                }
            }
        }
        f
    }
}

/// Os fundos gerados. `id` é o que vai no `config.ini` (`fundo = paisagem`); um nome terminado em
/// `.fnd` é um arquivo de `pocketmister/backgrounds/`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
    Landscape,
    Starry,
    Synthwave,
    Checkers,
    TestBars,
    Plain,
}

impl Style {
    pub const ALL: [Style; 6] = [
        Style::Landscape,
        Style::Starry,
        Style::Synthwave,
        Style::Checkers,
        Style::TestBars,
        Style::Plain,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Style::Landscape => "landscape",
            Style::Starry => "starry",
            Style::Synthwave => "synthwave",
            Style::Checkers => "checkers",
            Style::TestBars => "testbars",
            Style::Plain => "plain",
        }
    }

    /// (português, inglês)
    pub fn name(self) -> (&'static str, &'static str) {
        match self {
            Style::Landscape => ("Paisagem", "Landscape"),
            Style::Starry => ("Noite estrelada", "Starry night"),
            Style::Synthwave => ("Synthwave", "Synthwave"),
            Style::Checkers => ("Xadrez", "Checkers"),
            Style::TestBars => ("Barras de teste", "Test bars"),
            Style::Plain => ("Liso", "Plain"),
        }
    }

    pub fn from_config(v: &str) -> Option<Self> {
        Style::ALL
            .iter()
            .copied()
            .find(|e| e.id().eq_ignore_ascii_case(v.trim()))
    }
}

/// Gerador xorshift: os fundos saem sempre iguais.
struct Rnd(u32);
impl Rnd {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }
}

impl Background {
    fn empty() -> Self {
        Background {
            px: [[BACKGROUND_BASE; WIDTH]; HEIGHT],
            palette: [[0; 3]; BACKGROUND_COLORS],
        }
    }

    /// Cor `i` do fundo (já escurecida como o tema); devolve o índice no framebuffer.
    fn color(&mut self, i: usize, c: [u8; 3]) -> u8 {
        self.palette[i] = c.map(darken);
        BACKGROUND_BASE + i as u8
    }

    /// Degradê vertical de `n` tons entre `a` e `b`, a partir da cor `i0`.
    fn gradient(&mut self, i0: usize, n: usize, a: [u8; 3], b: [u8; 3]) {
        for k in 0..n {
            let t = (k * 255 / (n - 1).max(1)) as u16;
            self.color(i0 + k, crate::theme::blend(b, a, t));
        }
    }

    pub fn generate(e: Style) -> Self {
        match e {
            Style::Landscape => Background::landscape(),
            Style::Starry => Background::starry(),
            Style::Synthwave => Background::synthwave(),
            Style::Checkers => Background::checkers(),
            Style::TestBars => Background::test_bars(),
            Style::Plain => Background::plain(),
        }
    }

    fn starry() -> Self {
        let mut f = Background::empty();
        const N: usize = 64;
        f.gradient(0, N, [0x05, 0x08, 0x1C], [0x1E, 0x2A, 0x5A]);
        for (y, l) in f.px.iter_mut().enumerate() {
            l.fill(BACKGROUND_BASE + ((y / 2) * N / (HEIGHT / 2)).min(N - 1) as u8);
        }
        let glow = [
            f.color(N, [0x70, 0x78, 0x98]),
            f.color(N + 1, [0xB8, 0xC0, 0xE0]),
            f.color(N + 2, [0xFF, 0xFF, 0xFF]),
        ];
        let mut r = Rnd(0xC0FF_EE11);
        for _ in 0..260 {
            let (x, y) = (r.next() as usize % WIDTH, r.next() as usize % HEIGHT);
            let b = glow[(r.next() % 7).min(2) as usize];
            f.px[y][x] = b;
            if b == glow[2] && x + 1 < WIDTH && y + 1 < HEIGHT {
                // as mais fortes ganham uma cruz
                f.px[y][x + 1] = glow[1];
                f.px[y + 1][x] = glow[1];
            }
        }
        // lua cheia com duas crateras
        let moon = f.color(N + 3, [0xF4, 0xEE, 0xD0]);
        let crater = f.color(N + 4, [0xC8, 0xC0, 0xA0]);
        let (cx, cy, radius) = (250i32, 52i32, 18i32);
        for y in cy - radius..=cy + radius {
            for x in cx - radius..=cx + radius {
                let d = (x - cx) * (x - cx) + (y - cy) * (y - cy);
                if d <= radius * radius {
                    let in_crater = (x - cx + 6).pow(2) + (y - cy + 4).pow(2) < 20
                        || (x - cx - 5).pow(2) + (y - cy - 6).pow(2) < 12;
                    f.px[y as usize][x as usize] = if in_crater { crater } else { moon };
                }
            }
        }
        f
    }

    fn synthwave() -> Self {
        let mut f = Background::empty();
        const SKY: usize = 80;
        const HORIZON: usize = 150;
        f.gradient(0, SKY, [0x1A, 0x06, 0x3A], [0xF0, 0x4A, 0x9A]);
        for (y, l) in f.px.iter_mut().enumerate().take(HORIZON) {
            l.fill(BACKGROUND_BASE + (y * SKY / HORIZON).min(SKY - 1) as u8);
        }
        // sol com faixas, do amarelo ao laranja
        const SUN: usize = 24;
        f.gradient(SKY, SUN, [0xFF, 0xE8, 0x6A], [0xFF, 0x5A, 0x7A]);
        let (cx, cy, radius) = (160i32, 118i32, 46i32);
        for y in cy - radius..HORIZON as i32 {
            let band = y > cy && (y - cy) % 8 < ((y - cy) / 8).min(5);
            if band {
                continue;
            }
            for x in cx - radius..=cx + radius {
                if (x - cx).pow(2) + (y - cy).pow(2) <= radius * radius {
                    let shade =
                        ((y - (cy - radius)) as usize * SUN / (2 * radius as usize)).min(SUN - 1);
                    f.px[y as usize][x as usize] = BACKGROUND_BASE + (SKY + shade) as u8;
                }
            }
        }
        // chão escuro com a grade em perspectiva
        let ground = f.color(SKY + SUN, [0x10, 0x02, 0x1E]);
        let grid = f.color(SKY + SUN + 1, [0xFF, 0x3A, 0xC8]);
        for l in f.px.iter_mut().skip(HORIZON) {
            l.fill(ground);
        }
        // horizontais cada vez mais espaçadas
        // em décimos de pixel: o espaço cresce 35% a cada linha
        let (mut y, mut spacing) = (HORIZON * 10, 20);
        while y / 10 < HEIGHT {
            f.px[y / 10].fill(grid);
            y += spacing;
            spacing = spacing * 135 / 100;
        }
        // verticais saindo do ponto de fuga
        for k in -12i32..=12 {
            for (y, row) in f.px.iter_mut().enumerate().skip(HORIZON) {
                let x = 160 + k * 6 * (y - HORIZON + 4) as i32 / 4;
                if (0..WIDTH as i32).contains(&x) {
                    row[x as usize] = grid;
                }
            }
        }
        f
    }

    fn checkers() -> Self {
        let mut f = Background::empty();
        let a = f.color(0, [0x2E, 0x30, 0x3A]);
        let b = f.color(1, [0x24, 0x26, 0x2E]);
        for (y, l) in f.px.iter_mut().enumerate() {
            for (x, p) in l.iter_mut().enumerate() {
                *p = if (x / 16 + y / 16) % 2 == 0 { a } else { b };
            }
        }
        f
    }

    fn test_bars() -> Self {
        // barras de cor no padrão das de teste de TV, com uma faixa de cinzas embaixo
        let mut f = Background::empty();
        let colors = [
            [0xC0, 0xC0, 0xC0],
            [0xC0, 0xC0, 0x00],
            [0x00, 0xC0, 0xC0],
            [0x00, 0xC0, 0x00],
            [0xC0, 0x00, 0xC0],
            [0xC0, 0x00, 0x00],
            [0x00, 0x00, 0xC0],
        ];
        let idx: [u8; 7] = core::array::from_fn(|i| f.color(i, colors[i]));
        let grays: [u8; 8] = core::array::from_fn(|i| {
            let v = (i * 255 / 7) as u8;
            f.color(7 + i, [v, v, v])
        });
        for (y, l) in f.px.iter_mut().enumerate() {
            for (x, p) in l.iter_mut().enumerate() {
                *p = if y < HEIGHT * 2 / 3 {
                    idx[x * 7 / WIDTH]
                } else {
                    grays[x * 8 / WIDTH]
                };
            }
        }
        f
    }

    fn plain() -> Self {
        let mut f = Background::empty();
        let c = f.color(0, [0x30, 0x30, 0x36]);
        for l in f.px.iter_mut() {
            l.fill(c);
        }
        f
    }

    /// Grava no formato `.fnd` (o inverso de [`Background::from_file`]).
    pub fn write(&self, out: &mut [u8; FILE_SIZE]) {
        out[..8].copy_from_slice(MAGIC);
        for (o, c) in out[8..8 + BACKGROUND_COLORS * 3]
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .zip(&self.palette)
        {
            *o = *c;
        }
        for (o, &p) in out[8 + BACKGROUND_COLORS * 3..]
            .iter_mut()
            .zip(self.px.as_flattened())
        {
            *o = p - BACKGROUND_BASE;
        }
    }

    /// Põe o fundo na tela (pixels e as 160 cores dele na paleta).
    pub fn draw(&self, t: &mut Canvas) {
        t.px = self.px;
        let b = usize::from(BACKGROUND_BASE);
        t.palette[b..b + BACKGROUND_COLORS].copy_from_slice(&self.palette);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// gravar em `.fnd` e ler de volta dá o mesmo fundo, para todos os estilos
    #[test]
    fn fnd_round_trip() {
        let mut buf = [0u8; FILE_SIZE];
        for s in Style::ALL {
            let a = Background::generate(s);
            a.write(&mut buf);
            let b = Background::from_file(&buf).expect("formato válido");
            assert!(a.px == b.px && a.palette == b.palette, "{}", s.id());
        }
        assert!(Background::from_file(&buf[..100]).is_none());
    }
}

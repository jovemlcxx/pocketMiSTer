//! Framebuffer do frontend (320x240, um índice da paleta de 256 cores por pixel) e as primitivas
//! de desenho: retângulo, texto na fonte 8x8 (também na vertical), escurecer e a imagem do NES.

use crate::theme;
use font8x8::legacy::BASIC_LEGACY;

pub const WIDTH: usize = 320;
pub const HEIGHT: usize = 240;
/// colunas de texto na fonte 8x8
pub const COLS: usize = WIDTH / 8;

pub struct Canvas {
    pub px: [[u8; WIDTH]; HEIGHT],
    /// cor RGB de cada índice (0-63 NES, 64-95 tema, 96-255 papel de parede)
    pub palette: [[u8; 3]; 256],
}

impl Default for Canvas {
    fn default() -> Self {
        Canvas {
            px: [[theme::BLACK; WIDTH]; HEIGHT],
            palette: theme::default_palette(),
        }
    }
}

/// Glifo 8x8 de um caractere. Acentos do português viram a letra sem acento (a fonte é ASCII);
/// o resto fora do ASCII vira '?'.
fn glyph(c: char) -> &'static [u8; 8] {
    let base = match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
        'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'A',
        'é' | 'ê' | 'è' => 'e',
        'É' | 'Ê' | 'È' => 'E',
        'í' | 'ì' | 'î' => 'i',
        'Í' | 'Ì' | 'Î' => 'I',
        'ó' | 'ô' | 'õ' | 'ò' | 'ö' => 'o',
        'Ó' | 'Ô' | 'Õ' | 'Ò' | 'Ö' => 'O',
        'ú' | 'ù' | 'û' | 'ü' => 'u',
        'Ú' | 'Ù' | 'Û' | 'Ü' => 'U',
        'ç' => 'c',
        'Ç' => 'C',
        c if c.is_ascii() => c,
        _ => '?',
    };
    &BASIC_LEGACY[base as usize]
}

/// Decide o recorte de overscan da imagem do NES, como a TV de tubo fazia: quando o jogo apaga a
/// coluna esquerda (os 8 pixels de cada linha numa cor só) por 30 quadros seguidos, a imagem perde
/// 8 pixels de cada lado (fica centrada). A decisão fica até o jogo fechar, para não piscar.
#[derive(Clone, Copy, Debug, Default)]
pub struct Overscan {
    pub crop: bool,
    streak: u8,
}

impl Overscan {
    pub fn update(&mut self, fb: &[[u8; 256]; 240]) {
        if self.crop {
            return;
        }
        let blank = fb.iter().all(|l| l[1..8].iter().all(|&c| c == l[0]));
        self.streak = if blank {
            self.streak.saturating_add(1)
        } else {
            0
        };
        self.crop = self.streak >= 30;
    }
}

/// Botões do controle desenhados na legenda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    A,
    B,
    X,
    Y,
    L,
    R,
    Select,
    Start,
    /// direcional (cruz)
    Dpad,
}

/// Altura dos botões desenhados (o texto da legenda fica alinhado no meio).
pub const BUTTON_SIZE: usize = 11;

/// Fonte 3x5 para escrever dentro das pílulas de Select e Start (só as letras usadas).
fn mini_glyph(c: char) -> [u8; 5] {
    match c {
        'A' => [0b010, 0b101, 0b111, 0b101, 0b101],
        'E' => [0b111, 0b100, 0b110, 0b100, 0b111],
        'L' => [0b100, 0b100, 0b100, 0b100, 0b111],
        'S' => [0b011, 0b100, 0b010, 0b001, 0b110],
        'T' => [0b111, 0b010, 0b010, 0b010, 0b010],
        _ => [0; 5],
    }
}

/// Largura em pixels de um botão desenhado.
pub fn button_width(b: Button) -> usize {
    match b {
        Button::A | Button::B | Button::X | Button::Y => BUTTON_SIZE,
        Button::L | Button::R => 15,
        Button::Select | Button::Start => 15,
        Button::Dpad => BUTTON_SIZE,
    }
}

impl Canvas {
    pub fn clear(&mut self, color: u8) {
        for l in self.px.iter_mut() {
            l.fill(color);
        }
    }

    /// Retângulo cheio, cortado nas bordas da tela.
    pub fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: u8) {
        for l in self.px.iter_mut().skip(y).take(h) {
            for p in l.iter_mut().skip(x).take(w) {
                *p = color;
            }
        }
    }

    /// Texto a partir do pixel (x, y), sem fundo; para na borda direita. Devolve a largura em pixels.
    pub fn text(&mut self, x: usize, y: usize, s: &str, color: u8) -> usize {
        let mut cx = x;
        for c in s.chars() {
            if cx + 8 > WIDTH {
                break;
            }
            for (dy, bits) in glyph(c).iter().enumerate() {
                if let Some(row) = self.px.get_mut(y + dy) {
                    for dx in 0..8 {
                        if bits & (1 << dx) != 0 {
                            row[cx + dx] = color;
                        }
                    }
                }
            }
            cx += 8;
        }
        cx - x
    }

    /// Texto na vertical, de baixo para cima (girado 270 graus), começando em (x, y_base) e subindo.
    pub fn text_vertical(&mut self, x: usize, y_base: usize, s: &str, color: u8) {
        let mut base = y_base;
        for c in s.chars() {
            if base < 8 {
                break;
            }
            for (dy, bits) in glyph(c).iter().enumerate() {
                for dx in 0..8 {
                    if bits & (1 << dx) != 0
                        && let Some(row) = self.px.get_mut(base - dx)
                        && let Some(p) = row.get_mut(x + dy)
                    {
                        *p = color;
                    }
                }
            }
            base -= 8;
        }
    }

    /// Texto alinhado à direita terminando em `x_fim`.
    pub fn text_right(&mut self, x_end: usize, y: usize, s: &str, color: u8) {
        let w = s.chars().count() * 8;
        self.text(x_end.saturating_sub(w), y, s, color);
    }

    /// Texto que cabe em `max` caracteres: corta e põe "..." no fim.
    pub fn text_clipped(&mut self, x: usize, y: usize, s: &str, max: usize, color: u8) {
        if s.chars().count() <= max {
            self.text(x, y, s, color);
        } else {
            let end = s
                .char_indices()
                .nth(max.saturating_sub(3))
                .map_or(s.len(), |(i, _)| i);
            let w = self.text(x, y, &s[..end], color);
            self.text(x + w, y, "...", color);
        }
    }

    /// Desenha um botão do controle com o canto de cima à esquerda em (x, y): `cor` no corpo e
    /// `letra` na marcação. Devolve a largura.
    pub fn button(&mut self, x: usize, y: usize, b: Button, color: u8, letter: u8) -> usize {
        let w = button_width(b);
        match b {
            Button::A | Button::B | Button::X | Button::Y => {
                // círculo de 11 pixels
                for dy in 0..BUTTON_SIZE {
                    for dx in 0..BUTTON_SIZE {
                        let (cx, cy) = (dx as i32 * 2 - 10, dy as i32 * 2 - 10);
                        if cx * cx + cy * cy <= 121 {
                            self.pixel(x + dx, y + dy, color);
                        }
                    }
                }
                let c = match b {
                    Button::A => "A",
                    Button::B => "B",
                    Button::X => "X",
                    _ => "Y",
                };
                self.text(x + 2, y + 2, c, letter);
            }
            Button::L | Button::R => {
                // botão de ombro: retângulo com os cantos de cima arredondados
                let h = 9;
                let top = y + 1;
                self.rect(x + 2, top, w - 4, 1, color);
                self.rect(x + 1, top + 1, w - 2, 1, color);
                self.rect(x, top + 2, w, h - 2, color);
                self.text(
                    x + 4,
                    top + 1,
                    if b == Button::L { "L" } else { "R" },
                    letter,
                );
            }
            Button::Dpad => {
                // cruz de 11 pixels com o miolo marcado
                self.rect(x, y + 4, BUTTON_SIZE, 3, color);
                self.rect(x + 4, y, 3, BUTTON_SIZE, color);
                self.pixel(x + 5, y + 5, letter);
            }
            Button::Select | Button::Start => {
                // pílula com "SEL"/"STA" na fonte 3x5
                let top = y + 2;
                self.rect(x + 1, top, w - 2, 7, color);
                self.rect(x, top + 1, w, 5, color);
                let txt = if b == Button::Select { "SEL" } else { "STA" };
                for (i, c) in txt.chars().enumerate() {
                    for (dy, bits) in mini_glyph(c).iter().enumerate() {
                        for dx in 0..3 {
                            if bits & (0b100 >> dx) != 0 {
                                self.pixel(x + 2 + i * 4 + dx, top + 1 + dy, letter);
                            }
                        }
                    }
                }
            }
        }
        w
    }

    fn pixel(&mut self, x: usize, y: usize, color: u8) {
        if let Some(p) = self.px.get_mut(y).and_then(|l| l.get_mut(x)) {
            *p = color;
        }
    }

    /// Escurece uma região em xadrez (o fundo continua visível atrás do menu).
    pub fn darken(&mut self, x: usize, y: usize, w: usize, h: usize) {
        for (j, l) in self.px.iter_mut().enumerate().skip(y).take(h) {
            for (i, p) in l.iter_mut().enumerate().skip(x).take(w) {
                if (i + j) % 2 == 0 {
                    *p = theme::BLACK;
                }
            }
        }
    }

    /// A imagem do NES (256x240, índices 0-63) no meio da tela, preto em volta. Com `recortar`
    /// (ver [`Overscan`]) somem os 8 pixels de cada lado.
    pub fn nes_image(&mut self, fb: &[[u8; 256]; 240], crop: bool) {
        let cut = if crop { 8 } else { 0 };
        let x0 = (WIDTH - 256) / 2;
        for (l, src) in self.px.iter_mut().zip(fb.iter()) {
            l.fill(theme::BLACK);
            for (d, &s) in l[x0 + cut..x0 + 256 - cut]
                .iter_mut()
                .zip(&src[cut..256 - cut])
            {
                *d = s & 0x3F;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overscan_only_with_blank_column() {
        let mut fb = [[0x21u8; 256]; 240];
        let mut o = Overscan::default();
        for _ in 0..29 {
            o.update(&fb);
        }
        assert!(!o.crop);
        o.update(&fb);
        assert!(o.crop);
        // jogo que desenha na coluna esquerda (o HUD do Castlevania III) não é recortado
        fb[100][3] = 0x0F;
        let mut o = Overscan::default();
        for _ in 0..100 {
            o.update(&fb);
        }
        assert!(!o.crop);
    }

    #[test]
    fn vertical_text_goes_up() {
        let mut t = Canvas::default();
        t.text_vertical(10, 100, "I", theme::BAR);
        // o "I" deitado ocupa colunas 10-17 e linhas 93-100
        let pintados = (0..HEIGHT)
            .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
            .filter(|&(x, y)| t.px[y][x] == theme::BAR);
        for (x, y) in pintados {
            assert!(
                (10..18).contains(&x) && (93..=100).contains(&y),
                "({x},{y})"
            );
        }
    }
}

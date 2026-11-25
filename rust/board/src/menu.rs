//! Menu de jogos: a lista do pacote gravado na flash, desenhada com fonte 8x8 numa imagem do
//! mesmo formato da do NES (32 colunas x 30 linhas de texto). Cima/baixo escolhe, A ou Start
//! carrega. Por enquanto simples; o frontend no estilo do MiSTer é o item 12 do mapa.

use font8x8::legacy::BASIC_LEGACY;
use nes_core::ppu::{H, W};

/// cores (índices da paleta do NES)
const BG: u8 = 0x0F;
const TEXT: u8 = 0x30;
const TITLE: u8 = 0x2C;
const BAR: u8 = 0x12;
const DIM: u8 = 0x00;

pub const ROWS: usize = H / 8;
pub const COLS: usize = W / 8;
/// linhas de texto para a lista (fora título e rodapé)
pub const LIST_ROWS: usize = ROWS - 6;

pub fn clear(fb: &mut [[u8; W]; H]) {
    for row in fb.iter_mut() {
        row.fill(BG);
    }
}

/// Escreve `s` a partir da coluna `col` da linha de texto `row` (fora do ASCII vira '?').
pub fn text(fb: &mut [[u8; W]; H], col: usize, row: usize, s: &str, fg: u8, bg: u8) {
    for (i, ch) in s.chars().enumerate() {
        let c = col + i;
        if c >= COLS || row >= ROWS {
            break;
        }
        let glyph = BASIC_LEGACY[if ch.is_ascii() {
            ch as usize
        } else {
            b'?' as usize
        }];
        for (y, bits) in glyph.iter().enumerate() {
            for x in 0..8 {
                fb[row * 8 + y][c * 8 + x] = if bits & (1 << x) != 0 { fg } else { bg };
            }
        }
    }
}

/// Desenha a lista com o item `sel` marcado; `top` é o primeiro item visível.
pub fn draw(fb: &mut [[u8; W]; H], names: &[&str], sel: usize, top: usize) {
    clear(fb);
    text(fb, 1, 1, "pocketMiSTer", TITLE, BG);
    text(fb, 1, 2, "escolha um jogo", DIM, BG);
    for (k, name) in names.iter().enumerate().skip(top).take(LIST_ROWS) {
        let row = 4 + k - top;
        let bg = if k == sel { BAR } else { BG };
        if k == sel {
            for line in &mut fb[row * 8..row * 8 + 8] {
                line.fill(BAR);
            }
        }
        let shown: &str = if name.len() > COLS - 2 {
            &name[..name.floor_char_boundary(COLS - 2)]
        } else {
            name
        };
        text(fb, 1, row, shown, TEXT, bg);
    }
    text(fb, 1, ROWS - 1, "A/START: jogar  SEL+START: menu", DIM, BG);
}

//! Tema visual: o layout do "MiSTer Menu" do ES-DE (anthonycaccese/mister-menu-es-de) na versão
//! 640x480 do jovemlcxx/es-theme-misterfpga-fcamod, com esquemas de cores à escolha. Dos temas só
//! vêm o layout e as cores do esquema "MiSTer" (fontes e imagens deles não são usadas; os temas são
//! CC BY-NC-SA).
//!
//! O framebuffer guarda índices numa paleta de 256 cores:
//! - 0-63: a paleta do NES (o jogo atrás do menu do jogo);
//! - 64-95: as cores do tema, por papel (abaixo); o esquema escolhido preenche essas posições;
//! - 96-255: o papel de parede (160 cores, ver `fundo`).

/// primeiro índice das cores do tema e do papel de parede
pub const BASE: u8 = 64;
pub const BACKGROUND_BASE: u8 = 96;
pub const BACKGROUND_COLORS: usize = 160;

/// cabeçalho, coluna lateral, barra de seleção, corpo dos botões da legenda
pub const BAR: u8 = BASE;
/// texto sobre a barra (título, nome da tela, item marcado, letra dos botões)
pub const BAR_TEXT: u8 = BASE + 1;
/// painel da lista e faixa da legenda
pub const PANEL: u8 = BASE + 2;
/// texto da lista e da legenda
pub const TEXT: u8 = BASE + 3;
/// texto secundário no painel (tamanho, contagem, "em breve")
pub const TEXT_DIM: u8 = BASE + 4;
/// texto secundário sobre a barra
pub const BAR_TEXT_DIM: u8 = BASE + 5;
/// aviso (jogo que não abre)
pub const WARN: u8 = BASE + 6;
pub const BLACK: u8 = BASE + 7;

/// Um esquema de cores: quatro cores de base e a do aviso; as suaves saem da mistura delas.
#[derive(Clone, Copy, Debug)]
pub struct Scheme {
    pub id: &'static str,
    pub name_pt: &'static str,
    pub name_en: &'static str,
    pub bar: [u8; 3],
    pub bar_text: [u8; 3],
    pub panel: [u8; 3],
    pub text: [u8; 3],
    pub warning: [u8; 3],
}

const fn rgb(v: u32) -> [u8; 3] {
    [(v >> 16) as u8, (v >> 8) as u8, v as u8]
}

const fn scheme_def(id: &'static str, pt: &'static str, en: &'static str, c: [u32; 5]) -> Scheme {
    Scheme {
        id,
        name_pt: pt,
        name_en: en,
        bar: rgb(c[0]),
        bar_text: rgb(c[1]),
        panel: rgb(c[2]),
        text: rgb(c[3]),
        warning: rgb(c[4]),
    }
}

/// Os esquemas; o primeiro é o padrão. `id` é o que vai no `config.ini` (`cores = mister`).
/// Cores: barra, texto da barra, painel, texto, aviso.
pub const SCHEMES: [Scheme; 9] = [
    scheme_def(
        "mister",
        "MiSTer",
        "MiSTer",
        [0xFFDFDF, 0x1E0000, 0x220505, 0xFFDFDF, 0xFF7A6E],
    ),
    scheme_def(
        "light",
        "Claro",
        "Light",
        [0x2A0A0A, 0xFFE8E8, 0xF6EEEE, 0x2A0A0A, 0xC0281E],
    ),
    scheme_def(
        "famicom",
        "Famicom",
        "Famicom",
        [0xE8DFC6, 0xA81E22, 0x2C2A28, 0xE8DFC6, 0xFF8A70],
    ),
    scheme_def(
        "super",
        "Super",
        "Super",
        [0xCFCBEA, 0x3B2C85, 0x1B1640, 0xDCD8F4, 0xFF8A80],
    ),
    scheme_def(
        "gameboy",
        "Game Boy",
        "Game Boy",
        [0x9BBC0F, 0x0F380F, 0x0F380F, 0x9BBC0F, 0xE0F070],
    ),
    scheme_def(
        "amber",
        "Ambar",
        "Amber",
        [0xFFB000, 0x1A1000, 0x140C00, 0xFFB000, 0xFF6040],
    ),
    scheme_def(
        "phosphor",
        "Fosforo",
        "Phosphor",
        [0x4DFF88, 0x002A10, 0x001A0A, 0x4DFF88, 0xFFD040],
    ),
    scheme_def(
        "ocean",
        "Oceano",
        "Ocean",
        [0xBFE6F2, 0x063247, 0x07263A, 0xBFE6F2, 0xFF9A7A],
    ),
    // preto e branco: sem cor nenhuma; o aviso é cinza (a lista ainda marca com "!")
    scheme_def(
        "mono",
        "Preto e branco",
        "Black & white",
        [0xFFFFFF, 0x000000, 0x000000, 0xFFFFFF, 0xAAAAAA],
    ),
];

/// Esquema pelo `id` do `config.ini` (sem diferença de maiúsculas); o padrão se não achar.
pub fn scheme(id: &str) -> &'static Scheme {
    SCHEMES
        .iter()
        .find(|e| e.id.eq_ignore_ascii_case(id.trim()))
        .unwrap_or(&SCHEMES[0])
}

const fn channel(x: u8, y: u8, alfa_x: u16) -> u8 {
    ((x as u16 * alfa_x + y as u16 * (255 - alfa_x)) / 255) as u8
}

/// `a` com opacidade `alfa_a` (0-255) sobre `b`
pub const fn blend(a: [u8; 3], b: [u8; 3], alfa_a: u16) -> [u8; 3] {
    [
        channel(a[0], b[0], alfa_a),
        channel(a[1], b[1], alfa_a),
        channel(a[2], b[2], alfa_a),
    ]
}

/// Põe as cores de um esquema nas posições do tema da paleta.
pub fn apply(palette: &mut [[u8; 3]; 256], e: &Scheme) {
    let colors = [
        e.bar,
        e.bar_text,
        e.panel,
        e.text,
        blend(e.text, e.panel, 0x99),
        blend(e.bar_text, e.bar, 0x99),
        e.warning,
        [0, 0, 0],
    ];
    let b = usize::from(BASE);
    palette[b..b + colors.len()].copy_from_slice(&colors);
}

/// Contraste (WCAG) entre duas cores, x100: 450 = 4,5:1. Luminância com gama aproximada por
/// quadrado, em inteiro: basta para conferir os esquemas.
pub fn contrast_x100(a: [u8; 3], b: [u8; 3]) -> u32 {
    let lum = |c: [u8; 3]| -> u64 {
        let l = |v: u8| u64::from(v) * u64::from(v);
        2126 * l(c[0]) + 7152 * l(c[1]) + 722 * l(c[2])
    };
    let (la, lb) = (lum(a), lum(b));
    let (lighter, darker) = if la > lb { (la, lb) } else { (lb, la) };
    // (L1 + 0,05) / (L2 + 0,05), com L de 0 a 65025 * 10000
    let extra = 65025 * 10000 / 20;
    ((lighter + extra) * 100 / (darker + extra)) as u32
}

/// a mesma paleta 2C02 do `nes-core`, repetida aqui para o frontend não depender do núcleo
#[rustfmt::skip]
pub const NES_PALETTE: [[u8; 3]; 64] = [
    [84,84,84],[0,30,116],[8,16,144],[48,0,136],[68,0,100],[92,0,48],[84,4,0],[60,24,0],
    [32,42,0],[8,58,0],[0,64,0],[0,60,0],[0,50,60],[0,0,0],[0,0,0],[0,0,0],
    [152,150,152],[8,76,196],[48,50,236],[92,30,228],[136,20,176],[160,20,100],[152,34,32],[120,60,0],
    [84,90,0],[40,114,0],[8,124,0],[0,118,40],[0,102,120],[0,0,0],[0,0,0],[0,0,0],
    [236,238,236],[76,154,236],[120,124,236],[176,98,236],[228,84,236],[236,88,180],[236,106,100],[212,136,32],
    [160,170,0],[116,196,0],[76,208,32],[56,204,108],[56,180,204],[60,60,60],[0,0,0],[0,0,0],
    [236,238,236],[168,204,236],[188,188,236],[212,178,236],[236,174,236],[236,174,212],[236,180,176],[228,196,144],
    [204,210,120],[180,222,120],[168,226,144],[152,226,180],[160,214,228],[160,162,160],[0,0,0],[0,0,0],
];

/// Paleta inicial: NES, esquema padrão, e o papel de parede ainda preto.
pub fn default_palette() -> [[u8; 3]; 256] {
    let mut p = [[0u8; 3]; 256];
    p[..64].copy_from_slice(&NES_PALETTE);
    apply(&mut p, &SCHEMES[0]);
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    /// todo esquema lê bem: texto da lista e texto da barra com pelo menos 4,5:1
    #[test]
    fn schemes_readable() {
        for e in &SCHEMES {
            let list = contrast_x100(e.text, e.panel);
            let bar = contrast_x100(e.bar_text, e.bar);
            assert!(list >= 450, "{}: lista {}", e.id, list);
            assert!(bar >= 450, "{}: barra {}", e.id, bar);
        }
    }

    #[test]
    fn scheme_from_config() {
        assert_eq!(scheme("Game Boy").id, "mister"); // id errado cai no padrão
        assert_eq!(scheme(" gameboy ").id, "gameboy");
        assert_eq!(scheme("").id, "mister");
    }
}

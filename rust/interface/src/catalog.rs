//! Catálogo de jogos do cartão SD (`docs/frontend-and-sd.md`, seção 2.3): que arquivos contam como
//! jogo de cada sistema, o nome que aparece na tela, a ordem da lista e a navegação. Sem alocação:
//! quem lista o cartão guarda os nomes e passa fatias.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum System {
    Nes,
    Snes,
    Pico8,
}

impl System {
    pub const ALL: [System; 3] = [System::Nes, System::Snes, System::Pico8];

    /// pasta na raiz do cartão
    pub fn folder(self) -> &'static str {
        match self {
            System::Nes => "nes",
            System::Snes => "snes",
            System::Pico8 => "pico8",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            System::Nes => "Nintendo (NES)",
            System::Snes => "Super Nintendo",
            System::Pico8 => "PICO-8",
        }
    }

    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            System::Nes => &[".nes", ".fds"],
            System::Snes => &[".sfc", ".smc"],
            System::Pico8 => &[".p8.png", ".p8"],
        }
    }

    /// já existe núcleo para jogar (os outros aparecem como "em breve")
    pub fn has_core(self) -> bool {
        self == System::Nes
    }
}

/// Subpastas de cada sistema que são do firmware e não aparecem na lista.
pub const RESERVED: [&str; 3] = ["bios", "saves", "states"];

pub fn is_reserved(name: &str) -> bool {
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(name))
}

fn ends_with_ci(name: &str, ext: &str) -> bool {
    name.len() > ext.len()
        && name.is_char_boundary(name.len() - ext.len())
        && name[name.len() - ext.len()..].eq_ignore_ascii_case(ext)
}

/// Extensão do sistema que o arquivo tem, se tiver (a mais longa primeiro: `.p8.png` antes de `.p8`).
fn extension(system: System, name: &str) -> Option<&'static str> {
    system
        .extensions()
        .iter()
        .copied()
        .filter(|e| ends_with_ci(name, e))
        .max_by_key(|e| e.len())
}

pub fn accepts(system: System, name: &str) -> bool {
    !name.starts_with('.') && extension(system, name).is_some()
}

/// Nome que aparece na tela: sem a extensão e, se `sem_etiquetas`, sem as etiquetas do padrão
/// No-Intro entre parênteses ou colchetes ("Kirby's Adventure (USA)" -> "Kirby's Adventure").
pub fn display_name(system: System, file: &str, strip_tags: bool) -> &str {
    let base = match extension(system, file) {
        Some(e) => &file[..file.len() - e.len()],
        None => file,
    };
    let cut = [base.find(" ("), base.find(" [")]
        .into_iter()
        .flatten()
        .min();
    match cut {
        Some(i) if strip_tags && i > 0 => base[..i].trim_end(),
        _ => base,
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Warning {
    /// o núcleo ainda não tem este mapper
    Mapper(u16),
    /// falta um arquivo em `<sistema>/bios`
    Bios(&'static str),
    /// arquivo que não é uma ROM válida
    Invalid,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Folder,
    Game { size: u32, warning: Option<Warning> },
}

#[derive(Clone, Copy, Debug)]
pub struct Item<'a> {
    /// nome do arquivo ou da pasta, como está no cartão
    pub name: &'a str,
    pub tipo: Kind,
}

/// Letra sem acento, minúscula.
fn strip_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => 'a',
        'é' | 'ê' | 'è' | 'É' | 'Ê' | 'È' => 'e',
        'í' | 'ì' | 'î' | 'Í' | 'Ì' | 'Î' => 'i',
        'ó' | 'ô' | 'õ' | 'ò' | 'ö' | 'Ó' | 'Ô' | 'Õ' | 'Ò' | 'Ö' => 'o',
        'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' | 'Ü' => 'u',
        'ç' | 'Ç' => 'c',
        c => c.to_ascii_lowercase(),
    }
}

/// Chave de ordem: sem artigo no começo ("The Legend of Zelda" fica em L), sem acento, minúscula.
fn sort_key(name: &str) -> impl Iterator<Item = char> + '_ {
    let mut s = name;
    for art in ["the ", "a ", "an ", "o ", "os ", "as "] {
        if s.len() > art.len()
            && s.is_char_boundary(art.len())
            && s[..art.len()].eq_ignore_ascii_case(art)
        {
            s = &s[art.len()..];
            break;
        }
    }
    s.chars().map(strip_accent)
}

/// Letra que o L/R usa para pular (maiúscula; '#' para números e símbolos).
pub fn initial(name: &str) -> char {
    match sort_key(name).next() {
        Some(c) if c.is_ascii_alphabetic() => c.to_ascii_uppercase(),
        _ => '#',
    }
}

/// Pastas primeiro, depois jogos; cada grupo em ordem alfabética pela [`chave`] do nome exibido.
pub fn sort_items(system: System, items: &mut [Item]) {
    items.sort_unstable_by(|a, b| {
        let pa = matches!(a.tipo, Kind::Folder);
        let pb = matches!(b.tipo, Kind::Folder);
        pb.cmp(&pa).then_with(|| {
            sort_key(display_name(system, a.name, false))
                .cmp(sort_key(display_name(system, b.name, false)))
                .then_with(|| a.name.cmp(b.name))
        })
    });
}

/// Tamanho curto para a coluna da direita: "40K", "384K", "1.5M".
pub fn short_size(bytes: u32, buf: &mut [u8; 8]) -> &str {
    let k = bytes.div_ceil(1024);
    let mut w = Writer { buf, n: 0 };
    if k < 1024 {
        w.number(k);
        w.byte(b'K');
    } else {
        let decimos = (k * 10).div_ceil(1024);
        w.number(decimos / 10);
        if !decimos.is_multiple_of(10) {
            w.byte(b'.');
            w.number(decimos % 10);
        }
        w.byte(b'M');
    }
    let n = w.n;
    core::str::from_utf8(&buf[..n]).unwrap_or("?")
}

struct Writer<'b> {
    buf: &'b mut [u8; 8],
    n: usize,
}

impl Writer<'_> {
    fn byte(&mut self, b: u8) {
        if self.n < self.buf.len() {
            self.buf[self.n] = b;
            self.n += 1;
        }
    }
    fn number(&mut self, v: u32) {
        if v >= 10 {
            self.number(v / 10);
        }
        self.byte(b'0' + (v % 10) as u8);
    }
}

/// Posição numa lista com rolagem: item marcado e primeiro item visível.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListPos {
    pub sel: usize,
    pub top: usize,
}

impl ListPos {
    /// Anda `delta` itens (dá a volta nas pontas, como o menu do MiSTer) e acerta a rolagem.
    pub fn step(&mut self, delta: isize, total: usize, visible: usize) {
        if total == 0 {
            return;
        }
        self.sel = (self.sel as isize + delta).rem_euclid(total as isize) as usize;
        self.scroll(total, visible);
    }

    /// Página para cima/baixo, sem dar a volta.
    pub fn page(&mut self, dir: isize, total: usize, visible: usize) {
        if total == 0 {
            return;
        }
        let target = self.sel as isize + dir * visible as isize;
        self.sel = target.clamp(0, total as isize - 1) as usize;
        self.scroll(total, visible);
    }

    /// Pula para o primeiro item da letra seguinte (dir = 1) ou anterior (dir = -1).
    pub fn jump_letter(&mut self, items: &[Item], dir: isize, visible: usize) {
        let total = items.len();
        if total == 0 {
            return;
        }
        let current = initial(items[self.sel].name);
        let mut i = self.sel;
        for _ in 0..total {
            i = (i as isize + dir).rem_euclid(total as isize) as usize;
            let c = initial(items[i].name);
            if c != current {
                // voltando, para no primeiro item daquela letra
                if dir < 0 {
                    while i > 0 && initial(items[i - 1].name) == c {
                        i -= 1;
                    }
                }
                self.sel = i;
                break;
            }
        }
        self.scroll(total, visible);
    }

    fn scroll(&mut self, total: usize, visible: usize) {
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + visible {
            self.top = self.sel + 1 - visible;
        }
        self.top = self.top.min(total.saturating_sub(visible));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(name: &str) -> Item<'_> {
        Item {
            name,
            tipo: Kind::Game {
                size: 0,
                warning: None,
            },
        }
    }

    #[test]
    fn filters_by_extension() {
        assert!(accepts(System::Nes, "Kirby.NES"));
        assert!(accepts(System::Nes, "Zelda.fds"));
        assert!(!accepts(System::Nes, "leia-me.txt"));
        assert!(!accepts(System::Nes, "._Kirby.nes")); // lixo do macOS
        assert!(accepts(System::Pico8, "celeste.p8.png"));
        assert!(!accepts(System::Pico8, "foto.png"));
        assert!(is_reserved("BIOS") && is_reserved("saves") && !is_reserved("Plataforma"));
    }

    #[test]
    fn display_names() {
        assert_eq!(
            display_name(System::Nes, "Kirby's Adventure (USA).nes", false),
            "Kirby's Adventure (USA)"
        );
        assert_eq!(
            display_name(System::Nes, "Kirby's Adventure (USA).nes", true),
            "Kirby's Adventure"
        );
        assert_eq!(
            display_name(System::Nes, "Micro Mages.nes", true),
            "Micro Mages"
        );
        assert_eq!(
            display_name(System::Pico8, "celeste.p8.png", true),
            "celeste"
        );
    }

    #[test]
    fn sort_folders_articles_accents() {
        let mut v = [
            game("Zelda.nes"),
            game("The Adventures of Bayou Billy.nes"),
            Item {
                name: "Homebrew",
                tipo: Kind::Folder,
            },
            game("Ácaro.nes"),
            game("batman.nes"),
        ];
        sort_items(System::Nes, &mut v);
        let names: [&str; 5] = core::array::from_fn(|i| v[i].name);
        // "acaro" < "adventures" (sem artigo e sem acento) < "batman" < "zelda"
        assert_eq!(
            names,
            [
                "Homebrew",
                "Ácaro.nes",
                "The Adventures of Bayou Billy.nes",
                "batman.nes",
                "Zelda.nes"
            ]
        );
    }

    #[test]
    fn sizes() {
        let mut b = [0; 8];
        assert_eq!(short_size(40976, &mut b), "41K");
        assert_eq!(short_size(393_232, &mut b), "385K");
        assert_eq!(short_size(786_448, &mut b), "769K");
        assert_eq!(short_size(3 << 20, &mut b), "3M");
        assert_eq!(short_size(1536 * 1024, &mut b), "1.5M");
    }

    #[test]
    fn navigation() {
        let mut l = ListPos::default();
        l.step(-1, 50, 10); // volta para o último
        assert_eq!((l.sel, l.top), (49, 40));
        l.step(1, 50, 10);
        assert_eq!((l.sel, l.top), (0, 0));
        l.page(1, 50, 10);
        assert_eq!(l.sel, 10);
        let items = [
            game("Alpha"),
            game("Atari"),
            game("Batman"),
            game("Bomb"),
            game("Contra"),
        ];
        let mut l = ListPos::default();
        l.jump_letter(&items, 1, 10);
        assert_eq!(l.sel, 2);
        l.jump_letter(&items, 1, 10);
        assert_eq!(l.sel, 4);
        l.jump_letter(&items, -1, 10);
        assert_eq!(l.sel, 2); // primeiro do B
    }
}

//! Gera as duas figuras do manual pocketMiSTer na Waveshare RP2350-PiZero (porte de
//! `hardware/gerar_esquema.py`):
//!
//! - pocketmister-wiring.svg: header de 40 pinos completo + dois conectores SNES (vistos por trás);
//! - pocketmister-board.svg: foto da placa (anúncio Waveshare) com cada parte apontada.
//!
//! Pinos do pico-infonesPlus (pico_shared/BoardConfigs.cmake, HW_CONFIG 7): controle 1 = clock
//! GPIO5, data GPIO6, latch GPIO9; controle 2 = clock GPIO10, latch GPIO11, data GPIO12. O header
//! segue o padrão Raspberry Pi (cores conferidas na foto da placa: 5V nos pinos 2/4, 3V3 nos 1/17,
//! ID nos 27/28).
//!
//! Os números saem como no Python (inteiro sem ".0", fração com o repr mais curto), então o SVG
//! de ligação é byte a byte o mesmo de antes.

use crate::roms::repo;
use std::fmt;
use std::path::Path;

const SANS: &str = "font-family=\"'IBM Plex Sans','Segoe UI',sans-serif\"";
const NARROW: &str = "font-family=\"'Barlow Semi Condensed','Arial Narrow',sans-serif\"";
const INK: &str = "#1b2632";
const MUTED: &str = "#4d5b69";
const RED: &str = "#e03131";
const BLACK: &str = "#20262d";
const BLUE: &str = "#1c7ed6";
const TEAL: &str = "#0c8599";
const YELLOW: &str = "#c99700";

/// Número como o Python o escreve: `int` sem casas, `float` com o repr mais curto ("160.0").
#[derive(Clone, Copy)]
enum N {
    I(i32),
    F(f64),
}
impl fmt::Display for N {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            N::I(v) => write!(f, "{v}"),
            N::F(v) => write!(f, "{v:?}"),
        }
    }
}
impl From<i32> for N {
    fn from(v: i32) -> N {
        N::I(v)
    }
}
impl From<f64> for N {
    fn from(v: f64) -> N {
        N::F(v)
    }
}

struct Svg {
    o: Vec<String>,
}

impl Svg {
    fn new(w: i32, h: i32, title: &str, fonts: &str) -> Svg {
        Svg {
            o: vec![
                format!(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" role=\"img\"><title>{title}</title>"
                ),
                fonts.to_string(),
                "<defs><pattern id=\"grid\" width=\"21.6\" height=\"21.6\" patternUnits=\"userSpaceOnUse\"><path d=\"M21.6 0H0V21.6\" fill=\"none\" stroke=\"#d5dce3\" stroke-width=\"0.6\"/></pattern></defs>".into(),
                format!("<rect width=\"{w}\" height=\"{h}\" fill=\"#e6ebf0\"/><rect width=\"{w}\" height=\"{h}\" fill=\"url(#grid)\"/>"),
            ],
        }
    }

    fn add(&mut self, s: String) {
        self.o.push(s);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "mesma assinatura do Svg.text do Python"
    )]
    fn text_full(
        &mut self,
        x: impl Into<N>,
        y: impl Into<N>,
        t: &str,
        size: impl Into<N>,
        weight: i32,
        fill: &str,
        anchor: &str,
        fam: &str,
        extra: &str,
    ) {
        let (x, y, size) = (x.into(), y.into(), size.into());
        self.add(format!(
            "<text x=\"{x}\" y=\"{y}\" font-size=\"{size}\" {fam} font-weight=\"{weight}\" fill=\"{fill}\" text-anchor=\"{anchor}\"{extra}>{t}</text>"
        ));
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "mesma assinatura do Svg.text do Python"
    )]
    fn text(
        &mut self,
        x: impl Into<N>,
        y: impl Into<N>,
        t: &str,
        size: impl Into<N>,
        weight: i32,
        fill: &str,
        anchor: &str,
    ) {
        self.text_full(x, y, t, size, weight, fill, anchor, SANS, "");
    }

    fn wire(&mut self, pts: &[(i32, i32)], color: &str) {
        let d = format!(
            "M{}",
            pts.iter()
                .map(|(x, y)| format!("{x},{y}"))
                .collect::<Vec<_>>()
                .join(" L")
        );
        for (sw, c) in [("8.5", "#ffffff"), ("4.6", color)] {
            self.add(format!(
                "<path d=\"{d}\" fill=\"none\" stroke=\"{c}\" stroke-width=\"{sw}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"
            ));
        }
    }

    fn dot(&mut self, (x, y): (i32, i32), color: &str) {
        self.add(format!("<circle cx=\"{x}\" cy=\"{y}\" r=\"4.6\" fill=\"{color}\" stroke=\"#ffffff\" stroke-width=\"1.5\"/>"));
    }

    fn pill(&mut self, x: i32, y: i32, t: &str, color: &str) {
        let w = 7.2 * t.chars().count() as f64 + 16.0;
        let x0 = f64::from(x) - w / 2.0;
        self.add(format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"20\" rx=\"10\" fill=\"{color}\" stroke=\"#ffffff\" stroke-width=\"1.5\"/>",
            N::F(x0),
            y - 11,
            N::F(w)
        ));
        self.text(
            x0 + w / 2.0,
            f64::from(y) + 3.5,
            t,
            11,
            600,
            "#ffffff",
            "middle",
        );
    }

    fn save(mut self, path: &Path) -> Result<(), String> {
        self.o.push("</svg>".into());
        std::fs::write(path, self.o.join("\n")).map_err(|e| format!("{}: {e}", path.display()))
    }
}

const HEADER: [&str; 41] = [
    "", "3V3", "5V", "GPIO2", "5V", "GPIO3", "GND", "GPIO4", "GPIO14", "GND", "GPIO15", "GPIO17",
    "GPIO18", "GPIO27", "GND", "GPIO22", "GPIO23", "3V3", "GPIO24", "GPIO10", "GND", "GPIO9",
    "GPIO25", "GPIO11", "GPIO8", "GND", "GPIO7", "GPIO0", "GPIO1", "GPIO5", "GND", "GPIO6",
    "GPIO12", "GPIO13", "GND", "GPIO19", "GPIO16", "GPIO26", "GPIO20", "GND", "GPIO21",
];

const X0: i32 = 200;
const PITCH: i32 = 46;
const YE: i32 = 210; // pinos pares na borda (em cima), ímpares por dentro
const YO: i32 = 246;
fn px(n: i32) -> i32 {
    X0 + ((n - 1) / 2) * PITCH
}
fn py(n: i32) -> i32 {
    if n % 2 == 0 { YE } else { YO }
}

/// Conector SNES fêmea visto por trás: arredondado à esquerda (pino 7), reto à direita (pino 1).
/// Devolve a posição de cada pino (índice = número do pino).
fn connector(s: &mut Svg, x0: i32, y0: i32, title: &str, sub: &str) -> [(i32, i32); 8] {
    s.add(format!(
        "<path d=\"M{},{y0} L{},{y0} L{},{} L{},{} A32,32 0 0 1 {},{y0} Z\" fill=\"#2b2d31\"/>",
        x0 + 32,
        x0 + 320,
        x0 + 320,
        y0 + 64,
        x0 + 32,
        y0 + 64,
        x0 + 32
    ));
    let pos = [
        (7, 40),
        (6, 75),
        (5, 110),
        (4, 170),
        (3, 205),
        (2, 240),
        (1, 275),
    ];
    let cy = y0 + 32;
    let mut at = [(0, 0); 8];
    for (n, dx) in pos {
        s.add(format!(
            "<circle cx=\"{}\" cy=\"{cy}\" r=\"8\" fill=\"#d9b45f\" stroke=\"#b08a3a\"/><circle cx=\"{}\" cy=\"{cy}\" r=\"3\" fill=\"#6b5a32\"/>",
            x0 + dx,
            x0 + dx
        ));
        if n == 1 || n == 7 {
            s.text(x0 + dx, y0 - 8, &n.to_string(), 11, 600, INK, "middle");
        } else {
            // 2-4 recebem fio por cima: número embaixo
            s.text(x0 + dx, y0 + 84, &n.to_string(), 11, 600, INK, "middle");
        }
        at[n as usize] = (x0 + dx, cy);
    }
    for (n, lab, col) in [(2, "clock", BLUE), (3, "latch", TEAL), (4, "data", YELLOW)] {
        s.text(at[n].0, y0 + 100, lab, 10.5, 600, col, "middle");
    }
    s.text(x0 + 160, y0 + 182, title, 13, 600, INK, "middle");
    s.text(x0 + 160, y0 + 199, sub, 10.5, 400, MUTED, "middle");
    at
}

fn wiring(dir: &Path, fonts: &str) -> Result<(), String> {
    let mut s = Svg::new(
        1240,
        870,
        "Ligação de dois controles SNES na Waveshare RP2350-PiZero",
        fonts,
    );
    let used = |n: i32| match n {
        2 | 4 => Some(RED),
        6 | 34 => Some(BLACK),
        19 | 29 => Some(BLUE),
        23 | 21 => Some(TEAL),
        32 | 31 => Some(YELLOW),
        _ => None,
    };
    s.text(
        X0 - 30,
        40,
        "Header de 40 pinos · visto de cima, com o micro SD à esquerda",
        14,
        600,
        INK,
        "start",
    );
    s.text(
        X0 - 30,
        58,
        "cores como na placa: vermelho 5V, amarelo 3V3, preto GND, azul ID (GPIO0/1), verde os outros GPIOs",
        11,
        400,
        MUTED,
        "start",
    );
    s.add(format!(
        "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"5\" fill=\"#1f5aa6\"/>",
        X0 - 22,
        YE - 18,
        19 * PITCH + 44,
        YO - YE + 36
    ));
    for n in 1..=40 {
        let name = HEADER[n as usize];
        let c = match name {
            "5V" => "#e5484d",
            "3V3" => "#e8c440",
            "GND" => "#2b2d31",
            _ if n == 27 || n == 28 => "#3b82c4",
            _ => "#2f9e44",
        };
        let ring = match used(n) {
            Some(u) => format!(" stroke=\"{u}\" stroke-width=\"4\""),
            None => " stroke=\"#ffffff\" stroke-width=\"1\"".into(),
        };
        s.add(format!(
            "<rect x=\"{}\" y=\"{}\" width=\"22\" height=\"22\" rx=\"3\" fill=\"{c}\"{ring}/>",
            px(n) - 11,
            py(n) - 11
        ));
        s.add(format!(
            "<rect x=\"{}\" y=\"{}\" width=\"8\" height=\"8\" fill=\"#d9b45f\"/>",
            px(n) - 4,
            py(n) - 4
        ));
        let lab = format!("{n} · {name}");
        let (w, col) = if used(n).is_some() {
            (600, INK)
        } else {
            (400, MUTED)
        };
        let lx = px(n) + if used(n).is_some() { 19 } else { 4 }; // rótulo dos pinos usados fica ao lado do fio
        if n % 2 == 0 {
            let rot = format!(" transform=\"rotate(-90 {lx} {})\"", YE - 24);
            s.text_full(lx, YE - 24, &lab, 10.5, w, col, "start", NARROW, &rot);
        } else {
            let rot = format!(" transform=\"rotate(-90 {lx} {})\"", YO + 24);
            s.text_full(lx, YO + 24, &lab, 10.5, w, col, "end", NARROW, &rot);
        }
    }
    s.text(X0 - 30, YO + 4, "1", 11, 600, INK, "end");
    s.text(X0 - 30, YE + 4, "2", 11, 600, INK, "end");

    let c2 = connector(
        &mut s,
        230,
        600,
        "Controle 2 · conector visto por trás",
        "lado reto = pino 1 · lado arredondado = pino 7",
    );
    let c1 = connector(
        &mut s,
        760,
        600,
        "Controle 1 · conector visto por trás",
        "lado reto = pino 1 · lado arredondado = pino 7",
    );

    // sinais: (pino do header, conector, pino do conector, cor, faixa)
    let odd = [
        (29, &c1, 2, BLUE, 360),
        (31, &c1, 4, YELLOW, 385),
        (21, &c1, 3, TEAL, 410),
        (19, &c2, 2, BLUE, 435),
        (23, &c2, 3, TEAL, 460),
    ];
    for &(hp, c, cp, col, lane) in &odd {
        let (x1, y1) = c[cp];
        s.wire(&[(px(hp), YO), (px(hp), lane), (x1, lane), (x1, y1)], col);
    }
    let (x1, y1) = c2[4];
    s.wire(
        &[
            (px(32), YE),
            (px(32), 70),
            (150, 70),
            (150, 560),
            (x1, 560),
            (x1, y1),
        ],
        YELLOW,
    );
    for &(hp, c, cp, col, _) in odd.iter().chain([(32, &c2, 4, YELLOW, 0)].iter()) {
        s.dot((px(hp), py(hp)), col);
        s.dot(c[cp], col);
    }

    // energia: cotoco em cada conector com a etiqueta do pino de origem
    for (c, v5, gnd) in [(&c2, "pino 2", "pino 6"), (&c1, "pino 4", "pino 34")] {
        let ((xa, ya), (xg, yg)) = (c[1], c[7]);
        s.wire(&[(xa, ya), (xa, ya + 92)], RED);
        s.dot((xa, ya), RED);
        s.wire(&[(xg, yg), (xg, yg + 92)], BLACK);
        s.dot((xg, yg), BLACK);
        s.pill(xa, ya + 108, &format!("5V · {v5}"), RED);
        s.pill(xg, yg + 108, &format!("GND · {gnd}"), BLACK);
    }
    for c in [&c2, &c1] {
        s.text(
            f64::from(c[5].0 + c[6].0) / 2.0,
            c[6].1 + 68,
            "5 e 6 livres",
            10.5,
            400,
            MUTED,
            "middle",
        );
    }
    s.text(1220, 858, "esquema fora de escala", 11, 400, MUTED, "end");
    s.save(&dir.join("pocketmister-wiring.svg"))
}

fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (u32::from(c[0]) << 16)
            | (u32::from(*c.get(1).unwrap_or(&0)) << 8)
            | u32::from(*c.get(2).unwrap_or(&0));
        for (i, sh) in [18, 12, 6, 0].into_iter().enumerate() {
            out.push(if i <= c.len() {
                char::from(T[(n >> sh) as usize & 63])
            } else {
                '='
            });
        }
    }
    out
}

fn board_figure(dir: &Path, fonts: &str) -> Result<(), String> {
    let photo = repo().join("referencias/fotos/rp2350-pizero-frente.jpg");
    // o arquivo .jpg do anúncio é WebP por dentro: o formato vem do conteúdo, como no PIL
    let err = |e: &dyn std::fmt::Display| format!("{}: {e}", photo.display());
    let im = image::ImageReader::open(&photo)
        .map_err(|e| err(&e))?
        .with_guessed_format()
        .map_err(|e| err(&e))?
        .decode()
        .map_err(|e| err(&e))?
        .to_rgb8();
    let im = image::imageops::crop_imm(&im, 110, 490, 1220, 590).to_image(); // só a placa
    let mut jpg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpg, 86)
        .encode_image(&im)
        .map_err(|e| e.to_string())?;
    const OX: i32 = 200;
    const OY: i32 = 150; // onde a foto entra no SVG
    let pt = |x: i32, y: i32| (OX + x - 110, OY + y - 490);
    let mut p = Svg::new(
        1620,
        900,
        "Waveshare RP2350-PiZero com as partes usadas pelo pocketMiSTer",
        fonts,
    );
    p.add(format!(
        "<image x=\"{OX}\" y=\"{OY}\" width=\"{}\" height=\"{}\" href=\"data:image/jpeg;base64,{}\"/>",
        im.width(),
        im.height(),
        base64(&jpg)
    ));
    p.text(
        OX,
        50,
        "Waveshare RP2350-PiZero · vista de cima (foto do anúncio)",
        14,
        600,
        INK,
        "start",
    );
    p.text(
        OX,
        68,
        "confira na sua placa: a marca do chip de flash pode ser outra",
        11,
        400,
        MUTED,
        "start",
    );

    let callout = |p: &mut Svg,
                   (px_, py_): (i32, i32),
                   (lx, ly): (i32, i32),
                   title: &str,
                   sub: &str,
                   color: &str,
                   anchor: &str| {
        let (x, y) = pt(px_, py_);
        p.add(format!("<line x1=\"{x}\" y1=\"{y}\" x2=\"{lx}\" y2=\"{ly}\" stroke=\"{color}\" stroke-width=\"2\"/>"));
        p.add(format!(
            "<circle cx=\"{x}\" cy=\"{y}\" r=\"6\" fill=\"none\" stroke=\"{color}\" stroke-width=\"2.5\"/><circle cx=\"{lx}\" cy=\"{ly}\" r=\"2.5\" fill=\"{color}\"/>"
        ));
        let dx = if anchor == "start" { 8 } else { -8 };
        p.text(
            lx + dx,
            if sub.is_empty() { ly + 4 } else { ly - 4 },
            title,
            12.5,
            600,
            color,
            anchor,
        );
        if !sub.is_empty() {
            p.text(lx + dx, ly + 12, sub, 10.5, 400, MUTED, anchor);
        }
    };
    // PSRAM em destaque: retângulo sobre as ilhas
    let ((x, y), (x2, y2)) = (pt(985, 690), pt(1105, 800));
    p.add(format!(
        "<rect x=\"{x}\" y=\"{y}\" width=\"{}\" height=\"{}\" rx=\"4\" fill=\"none\" stroke=\"{RED}\" stroke-width=\"3.5\"/>",
        x2 - x,
        y2 - y
    ));
    callout(
        &mut p,
        (1072, 775),
        (1420, 110),
        "PSRAM: ilhas vazias de 8 pinos",
        "bolinha do pino 1 embaixo à direita",
        RED,
        "end",
    );
    callout(
        &mut p,
        (880, 740),
        (1010, 110),
        "Flash 16 MB",
        "veja a marca: Winbond ou Puya",
        INK,
        "end",
    );
    callout(
        &mut p,
        (282, 578),
        (280, 110),
        "Pino 1 do header",
        "fileira de dentro, ponta do micro SD",
        INK,
        "start",
    );
    callout(
        &mut p,
        (880, 665),
        (650, 110),
        "RUN",
        "reinicia",
        INK,
        "start",
    );
    callout(
        &mut p,
        (880, 835),
        (830, 860),
        "BOOT",
        "segure + RUN para gravar",
        INK,
        "start",
    );
    callout(&mut p, (630, 740), (650, 860), "RP2350B", "", INK, "start");
    callout(
        &mut p,
        (250, 730),
        (185, 520),
        "micro SD",
        "jogos .nes / .fds",
        INK,
        "end",
    );
    callout(
        &mut p,
        (450, 850),
        (420, 860),
        "CLK GND DIO",
        "depuração, não usar",
        INK,
        "start",
    );
    callout(
        &mut p,
        (355, 990),
        (200, 860),
        "mini HDMI",
        "adaptador C8K-01",
        INK,
        "start",
    );
    callout(
        &mut p,
        (840, 1000),
        (1050, 860),
        "PIO-USB",
        "controle USB (opcional)",
        INK,
        "start",
    );
    callout(
        &mut p,
        (1105, 1000),
        (1270, 860),
        "USB",
        "fonte 5V e gravação",
        INK,
        "start",
    );
    callout(
        &mut p,
        (1240, 700),
        (1600, 520),
        "BAT",
        "bateria, não usar",
        INK,
        "end",
    );
    p.save(&dir.join("pocketmister-board.svg"))
}

/// `cargo xtask schematic [pasta]`: grava os dois SVGs em `hardware/` (ou na pasta dada).
pub fn schematic(args: &[String]) -> Result<bool, String> {
    let hw = repo().join("hardware");
    let dir = args.first().map_or(hw.clone(), std::path::PathBuf::from);
    let fonts = std::fs::read_to_string(hw.join("_fonts.svgfrag"))
        .map_err(|e| format!("_fonts.svgfrag: {e}"))?;
    wiring(&dir, &fonts)?;
    board_figure(&dir, &fonts)?;
    println!("ok");
    Ok(true)
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_standard() {
        assert_eq!(super::base64(b"Man"), "TWFu");
        assert_eq!(super::base64(b"Ma"), "TWE=");
        assert_eq!(super::base64(b"M"), "TQ==");
        assert_eq!(super::base64(b""), "");
    }

    #[test]
    fn numbers_like_python() {
        assert_eq!(super::N::F(160.0).to_string(), "160.0");
        assert_eq!(super::N::F(7.2 * 11.0 + 16.0).to_string(), "95.2");
        assert_eq!(super::N::I(160).to_string(), "160");
    }
}

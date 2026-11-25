//! As telas do frontend no layout do "MiSTer Menu" (ver `tema`): cabeçalho rosa com o título, coluna
//! lateral rosa com o nome da tela na vertical, painel vinho com a lista (seleção numa barra rosa
//! com texto vinho, centrada na 6ª das 11 linhas) e a legenda embaixo, com os botões do controle
//! desenhados, tudo sobre o papel de parede. Os textos vêm do idioma escolhido (`idioma`).
//!
//! Medidas: as do es-theme-misterfpga-fcamod em 640x480, divididas por 2 (a tela é 320x240,
//! dobrada para 640x480 na TV).

use crate::background::Background;
use crate::canvas::{BUTTON_SIZE, Button, Canvas, WIDTH, button_width};
use crate::catalog::{self, Item, Kind, System, Warning};
use crate::lang::Texts;
use crate::theme::*;

const X: usize = 28;
const PANEL_W: usize = 264;
const HEADER_Y: usize = 34;
const HEADER_H: usize = 20;
const BODY_Y: usize = 66;
const COLUMN_W: usize = 20;
const ROWS: usize = 11;
const ROW_H: usize = 12;
const BODY_H: usize = ROWS * ROW_H;
const LIST_X: usize = X + COLUMN_W;
const LIST_W: usize = PANEL_W - COLUMN_W;
const LEGEND_Y: usize = 212;
/// linha da seleção (a 6ª), como no carrossel do tema
const CENTER: usize = 5;
/// caracteres que cabem num item da lista (margem de 4 pixels de cada lado e 6 colunas à direita)
const NAME_MAX: usize = (LIST_W - 8) / 8 - 6;
/// espaço entre o botão e a ação, e entre um item da legenda e o seguinte
const LEGEND_GAP: usize = 4;
const LEGEND_SPACING: usize = 6;

/// Item da legenda: um ou mais botões e a ação deles ("L R letra").
pub type LegendItem<'a> = (&'a [Button], &'a str);

/// Largura que uma legenda ocupa.
pub fn legend_width(items: &[LegendItem]) -> usize {
    items
        .iter()
        .map(|(bs, txt)| {
            bs.iter().map(|&b| button_width(b) + 2).sum::<usize>() - 2
                + LEGEND_GAP
                + txt.chars().count() * 8
        })
        .sum::<usize>()
        + LEGEND_SPACING * items.len().saturating_sub(1)
}

/// Letras maiúsculas num buffer (o título do tema é sempre em caixa alta).
fn uppercase<'b>(s: &str, buf: &'b mut [u8; 64]) -> &'b str {
    let mut n = 0;
    for c in s.chars() {
        let mut tmp = [0u8; 4];
        let e = c.to_uppercase().next().unwrap_or(c).encode_utf8(&mut tmp);
        if n + e.len() > buf.len() {
            break;
        }
        buf[n..n + e.len()].copy_from_slice(e.as_bytes());
        n += e.len();
    }
    core::str::from_utf8(&buf[..n]).unwrap_or("")
}

/// Moldura comum: papel de parede (ou o que já está na tela), cabeçalho, coluna e painel.
fn frame(
    t: &mut Canvas,
    background: Option<&Background>,
    title: &str,
    right: &str,
    side: &str,
    info: &str,
) {
    if let Some(f) = background {
        f.draw(t);
    }
    t.text_right(X + PANEL_W, 14, info, BAR);
    t.rect(X, HEADER_Y, PANEL_W, HEADER_H, BAR);
    let mut b = [0u8; 64];
    let max = (PANEL_W - 8) / 8 - right.chars().count() - 1;
    t.text_clipped(X + 4, HEADER_Y + 6, uppercase(title, &mut b), max, BAR_TEXT);
    t.text_right(X + PANEL_W - 4, HEADER_Y + 6, right, BAR_TEXT_DIM);
    t.rect(X, BODY_Y, COLUMN_W, BODY_H, BAR);
    let mut b = [0u8; 64];
    let lat = uppercase(side, &mut b);
    let base = BODY_Y + BODY_H / 2 + lat.chars().count() * 4;
    t.text_vertical(X + 6, base, lat, BAR_TEXT);
    t.rect(LIST_X, BODY_Y, LIST_W, BODY_H, PANEL);
}

/// Legenda com os botões desenhados (rosa com a letra em vinho) seguidos da ação.
fn legend(t: &mut Canvas, items: &[LegendItem]) {
    legend_strip(t);
    // centrada na largura do painel
    let mut x = X + PANEL_W.saturating_sub(legend_width(items)) / 2;
    let by = LEGEND_Y + 4 - BUTTON_SIZE / 2;
    for (k, (buttons, txt)) in items.iter().enumerate() {
        if k > 0 {
            x += LEGEND_SPACING;
        }
        for (i, &b) in buttons.iter().enumerate() {
            if i > 0 {
                x += 2;
            }
            x += t.button(x, by, b, BAR, BAR_TEXT);
        }
        x += LEGEND_GAP;
        x += t.text(x, LEGEND_Y, txt, TEXT);
    }
}

fn warning(t: &mut Canvas, text: &str) {
    legend_strip(t);
    t.text(X, LEGEND_Y, text, WARN);
}

/// Faixa vinho atrás da legenda: lê bem sobre qualquer papel de parede, claro ou escuro.
fn legend_strip(t: &mut Canvas) {
    t.rect(X - 4, LEGEND_Y - 5, PANEL_W + 8, 18, PANEL);
}

/// Primeiro item visível para deixar a seleção na linha do meio sempre que der.
fn top(sel: usize, total: usize) -> usize {
    sel.saturating_sub(CENTER).min(total.saturating_sub(ROWS))
}

/// Uma linha da lista: barra rosa e texto vinho se marcada; texto nas cores dadas senão.
fn row(
    t: &mut Canvas,
    i: usize,
    selected: bool,
    name: &str,
    name_color: u8,
    right: &str,
    right_color: u8,
) {
    let y = BODY_Y + i * ROW_H;
    if selected {
        t.rect(LIST_X, y, LIST_W, ROW_H, BAR);
    }
    let cn = if selected { BAR_TEXT } else { name_color };
    let cd = match (selected, right_color) {
        (_, WARN) => WARN,
        (true, _) => BAR_TEXT_DIM,
        (false, c) => c,
    };
    t.text_clipped(LIST_X + 4, y + 2, name, NAME_MAX, cn);
    t.text_right(LIST_X + LIST_W - 4, y + 2, right, cd);
}

/// Item da tela inicial: um sistema (com a contagem) ou uma entrada fixa.
pub enum Home<'a> {
    System {
        system: System,
        games: Option<usize>,
    },
    Fixed(&'a str),
}

/// Tela inicial: sistemas e entradas fixas (Favoritos, Recentes, Ajustes), com a seleção no meio.
pub fn home(
    t: &mut Canvas,
    background: &Background,
    items: &[Home],
    sel: usize,
    version: &str,
    tx: &Texts,
) {
    let total: usize = items
        .iter()
        .map(|i| match i {
            Home::System { games: Some(n), .. } => *n,
            _ => 0,
        })
        .sum();
    let mut b = [0u8; 24];
    frame(
        t,
        Some(background),
        "pocketMiSTer",
        count_text(total, tx, &mut b),
        tx.systems,
        version,
    );
    // carrossel: a seleção fica na linha do meio e os outros itens em volta
    for r in 0..ROWS {
        let Some(k) = (sel + r).checked_sub(CENTER).filter(|&k| k < items.len()) else {
            continue;
        };
        let mut b = [0u8; 24];
        let mut nb = [0u8; 64];
        match &items[k] {
            Home::System { system, games } => {
                let active = system.has_core() && games.is_some_and(|n| n > 0);
                let dir = match (system.has_core(), games) {
                    (false, _) => tx.coming_soon,
                    (true, None) => tx.no_folder,
                    (true, Some(n)) => count_text(*n, tx, &mut b),
                };
                let name = uppercase(system.name(), &mut nb);
                row(
                    t,
                    r,
                    k == sel,
                    name,
                    if active { TEXT } else { TEXT_DIM },
                    dir,
                    TEXT_DIM,
                );
            }
            Home::Fixed(name) => row(t, r, k == sel, uppercase(name, &mut nb), TEXT, "", TEXT_DIM),
        }
    }
    legend(
        t,
        &[
            (&[Button::A], tx.enter),
            (&[Button::Select], tx.settings_action),
        ],
    );
}

/// "8 jogos" / "1 game"
fn count_text<'b>(n: usize, tx: &Texts, buf: &'b mut [u8; 24]) -> &'b str {
    let mut tmp = [0u8; 10];
    let mut k = tmp.len();
    let mut v = n;
    loop {
        k -= 1;
        tmp[k] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    let num = core::str::from_utf8(&tmp[k..]).unwrap_or("?");
    join(
        buf,
        &[num, " ", if n == 1 { tx.game_one } else { tx.games_n }],
    )
}

/// O que a lista de jogos mostra.
pub struct Folder<'a> {
    pub system: System,
    /// subpasta (vai no título depois do nome do sistema)
    pub subfolder: Option<&'a str>,
    pub items: &'a [Item<'a>],
    pub sel: usize,
    pub strip_tags: bool,
}

/// Lista de jogos de um sistema ou de uma subpasta.
pub fn games(
    t: &mut Canvas,
    background: &Background,
    p: &Folder,
    favorite: impl Fn(&Item) -> bool,
    tx: &Texts,
) {
    let system = p.system;
    let mut title = [0u8; 64];
    let sep = if p.subfolder.is_some() { " / " } else { "" };
    let tit = join(&mut title, &[system.name(), sep, p.subfolder.unwrap_or("")]);
    let mut b = [0u8; 24];
    let n_games = p
        .items
        .iter()
        .filter(|i| matches!(i.tipo, Kind::Game { .. }))
        .count();
    frame(
        t,
        Some(background),
        tit,
        count_text(n_games, tx, &mut b),
        tx.games,
        "",
    );
    if p.items.is_empty() {
        t.text(LIST_X + 8, BODY_Y + 8, tx.no_games, TEXT_DIM);
        let mut b = [0u8; 48];
        t.text(
            LIST_X + 8,
            BODY_Y + 24,
            join(&mut b, &[tx.copy_to, system.folder(), "/"]),
            TEXT_DIM,
        );
        legend(t, &[(&[Button::B], tx.back)]);
        return;
    }
    let top = top(p.sel, p.items.len());
    let mut buf = [0u8; 8];
    for (r, item) in p.items.iter().skip(top).take(ROWS).enumerate() {
        let k = top + r;
        let mut nb = [0u8; 64];
        match item.tipo {
            Kind::Folder => {
                let name = join(&mut nb, &[item.name, "/"]);
                row(t, r, k == p.sel, name, TEXT, tx.folder, TEXT_DIM);
            }
            Kind::Game { size, warning } => {
                let base = catalog::display_name(system, item.name, p.strip_tags);
                let name = if favorite(item) {
                    join(&mut nb, &["* ", base])
                } else {
                    base
                };
                match warning {
                    None => row(
                        t,
                        r,
                        k == p.sel,
                        name,
                        TEXT,
                        catalog::short_size(size, &mut buf),
                        TEXT_DIM,
                    ),
                    Some(_) => row(t, r, k == p.sel, name, TEXT_DIM, "!", WARN),
                }
            }
        }
    }
    // a legenda vira o aviso quando o jogo marcado não abre
    match p.items.get(p.sel).map(|i| i.tipo) {
        Some(Kind::Game {
            warning: Some(a), ..
        }) => {
            let mut b = [0u8; 48];
            warning(t, warning_text(system, a, tx, &mut b));
        }
        _ => legend(t, &games_legend(tx)),
    }
}

/// Legenda da lista de jogos.
fn games_legend(tx: &Texts) -> [LegendItem<'_>; 4] {
    [
        (&[Button::A], tx.open),
        (&[Button::B], tx.back),
        (&[Button::Y], tx.favorite),
        (&[Button::L, Button::R], tx.letter),
    ]
}

/// Junta pedaços de texto num buffer (sem alocação), cortando num limite de caractere.
fn join<'b>(buf: &'b mut [u8], partes: &[&str]) -> &'b str {
    let mut n = 0;
    for p in partes {
        for &b in p.as_bytes() {
            if n < buf.len() {
                buf[n] = b;
                n += 1;
            }
        }
    }
    let mut end = n;
    while core::str::from_utf8(&buf[..end]).is_err() {
        end -= 1;
    }
    core::str::from_utf8(&buf[..end]).unwrap_or("")
}

fn warning_text<'b>(system: System, a: Warning, tx: &Texts, buf: &'b mut [u8; 48]) -> &'b str {
    match a {
        Warning::Mapper(m) => {
            let mut num = [0u8; 5];
            let mut k = num.len();
            let mut v = m;
            loop {
                k -= 1;
                num[k] = b'0' + (v % 10) as u8;
                v /= 10;
                if v == 0 {
                    break;
                }
            }
            let n = core::str::from_utf8(&num[k..]).unwrap_or("?");
            join(buf, &[tx.mapper, n, tx.not_supported])
        }
        Warning::Bios(arq) => join(buf, &[tx.needs, arq, tx.inside, system.folder(), "/bios"]),
        Warning::Invalid => join(buf, &[tx.invalid]),
    }
}

/// Menu dentro do jogo, na mesma moldura, por cima da imagem do jogo (que o chamador já pôs na
/// tela). `opcoes` são (rótulo, valor); valor vazio = só ação.
pub fn game_menu(t: &mut Canvas, game: &str, options: &[(&str, &str)], sel: usize, tx: &Texts) {
    t.darken(0, 0, WIDTH, crate::canvas::HEIGHT);
    frame(t, None, game, "", tx.menu, "");
    for (i, (label, value)) in options.iter().enumerate().take(ROWS) {
        row(t, i, i == sel, label, TEXT, value, TEXT_DIM);
    }
    legend(t, &[(&[Button::A], tx.ok), (&[Button::B], tx.back_to_game)]);
}

/// Ajustes: cada linha é (rótulo, valor); esquerda e direita trocam o valor e a tela já mostra o
/// resultado (as cores e o fundo aplicados).
pub fn settings(
    t: &mut Canvas,
    background: &Background,
    items: &[(&str, &str)],
    sel: usize,
    tx: &Texts,
) {
    frame(t, Some(background), tx.settings, "", tx.settings, "");
    let mut b = [0u8; 64];
    for (i, (label, value)) in items.iter().enumerate().take(ROWS) {
        // o marcado mostra as setas de troca
        let v = if i == sel {
            join(&mut b, &["< ", value, " >"])
        } else {
            value
        };
        row(t, i, i == sel, label, TEXT, v, TEXT_DIM);
    }
    legend(t, &settings_legend(tx));
}

fn settings_legend(tx: &Texts) -> [LegendItem<'_>; 2] {
    [(&[Button::Dpad], tx.change), (&[Button::B], tx.back)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::{EN, PT_BR};

    /// as legendas cabem numa linha a partir da margem do tema, nos dois idiomas
    #[test]
    fn legends_fit() {
        for tx in [&PT_BR, &EN] {
            let todas: [&[LegendItem]; 4] = [
                &games_legend(tx),
                &[
                    (&[Button::A], tx.enter),
                    (&[Button::Select], tx.settings_action),
                ],
                &[(&[Button::A], tx.ok), (&[Button::B], tx.back_to_game)],
                &settings_legend(tx),
            ];
            for l in todas {
                assert!(legend_width(l) <= PANEL_W, "{} px", X + legend_width(l));
            }
        }
    }

    #[test]
    fn selection_in_middle() {
        assert_eq!(top(0, 50), 0);
        assert_eq!(top(5, 50), 0);
        assert_eq!(top(6, 50), 1);
        assert_eq!(top(49, 50), 39);
        assert_eq!(top(3, 4), 0);
    }
}

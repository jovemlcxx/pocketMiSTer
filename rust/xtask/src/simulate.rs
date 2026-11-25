//! `cargo xtask simulate [cartao] [saida]`: desenha as telas do frontend como a placa vai mostrar,
//! em cada modo de vídeo, e grava PNGs na resolução real de saída (o aplicativo 4:3 no centro, o
//! resto preto).
//!
//! O cartão é uma pasta do PC com a mesma árvore do SD (`docs/frontend-and-sd.md`). Se não existir,
//! é criado em `demo/sd` com os jogos de `roms/` (os homebrews numa subpasta, para mostrar a
//! navegação). O catálogo usa o mesmo código do firmware (`interface::catalog`); o suporte de
//! cada jogo é conferido abrindo a ROM no `nes-core`.

use crate::roms;
use interface::background::{Background, Style};
use interface::canvas::{Canvas, Overscan};
use interface::catalog::{self, Item, Kind, System, Warning};
use interface::lang::Language;
use interface::screens::{self, Folder, Home};
use interface::{theme, video};
use nes_core::nes::LoadError;
use std::path::{Path, PathBuf};

const HOMEBREW: [&str; 3] = ["Micro Mages", "Full Quiet", "Kharzoid"];

/// Cria o cartão de exemplo a partir de `roms/`.
fn create_card(sd: &Path) -> Result<(), String> {
    let mk = |p: PathBuf| std::fs::create_dir_all(&p).map_err(|e| format!("{}: {e}", p.display()));
    for s in System::ALL {
        for sub in catalog::RESERVED {
            mk(sd.join(s.folder()).join(sub))?;
        }
    }
    mk(sd.join("nes/Homebrew"))?;
    mk(sd.join("pocketmister/cache"))?;
    write_builtin_backgrounds(&sd.join("pocketmister/backgrounds"))?;
    for rom in roms::games() {
        let name = rom
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let dest = if HOMEBREW.iter().any(|h| name.contains(h)) {
            sd.join("nes/Homebrew")
        } else {
            sd.join("nes")
        };
        std::fs::copy(&rom, dest.join(&name)).map_err(|e| format!("{name}: {e}"))?;
    }
    std::fs::write(
        sd.join("pocketmister/favorites.txt"),
        "nes/Kirby's Adventure (USA).nes\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Uma entrada lida do cartão (o nome fica aqui; o catálogo trabalha com fatias dele).
struct Entry {
    name: String,
    tipo: Kind,
}

/// Lê uma pasta do sistema: subpastas da pessoa e jogos com a extensão certa.
fn read_folder(system: System, folder: &Path, system_root: bool) -> Result<Vec<Entry>, String> {
    let mut v = Vec::new();
    let rd = std::fs::read_dir(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let meta = e.metadata().map_err(|e| e.to_string())?;
        if meta.is_dir() {
            if !(system_root && catalog::is_reserved(&name)) {
                v.push(Entry {
                    name,
                    tipo: Kind::Folder,
                });
            }
        } else if catalog::accepts(system, &name) {
            let warning = match system {
                System::Nes => {
                    let rom = std::fs::read(e.path()).map_err(|e| e.to_string())?;
                    match nes_core::Nes::<false>::new(&rom) {
                        Ok(_) => None,
                        Err(LoadError::Mapper(m)) => Some(Warning::Mapper(u16::from(m))),
                        Err(_) => Some(Warning::Invalid),
                    }
                }
                _ => None,
            };
            v.push(Entry {
                name,
                tipo: Kind::Game {
                    size: meta.len() as u32,
                    warning,
                },
            });
        }
    }
    Ok(v)
}

fn items(entries: &[Entry], system: System) -> Vec<Item<'_>> {
    let mut v: Vec<Item> = entries
        .iter()
        .map(|e| Item {
            name: &e.name,
            tipo: e.tipo,
        })
        .collect();
    catalog::sort_items(system, &mut v);
    v
}

fn count_games(system: System, folder: &Path) -> Option<usize> {
    if !folder.is_dir() {
        return None;
    }
    let mut n = 0;
    let mut stack = vec![(folder.to_path_buf(), true)];
    while let Some((p, root)) = stack.pop() {
        for e in read_folder(system, &p, root).ok()? {
            match e.tipo {
                Kind::Folder => stack.push((p.join(&e.name), false)),
                Kind::Game { .. } => n += 1,
            }
        }
    }
    Some(n)
}

/// Monta a imagem de saída de um modo: preto, com o aplicativo ampliado no centro.
fn compose(t: &Canvas, mode: &video::Mode) -> image::RgbImage {
    let f = video::frame(mode);
    let mut img = image::RgbImage::new(u32::from(mode.h_active), u32::from(mode.v_active));
    let s = usize::from(f.scale);
    for (y, row) in t.px.iter().enumerate() {
        for (x, &c) in row.iter().enumerate() {
            let rgb = image::Rgb(t.palette[usize::from(c)]);
            for dy in 0..s {
                for dx in 0..s {
                    img.put_pixel(
                        (usize::from(f.x) + x * s + dx) as u32,
                        (usize::from(f.y) + y * s + dy) as u32,
                        rgb,
                    );
                }
            }
        }
    }
    img
}

/// Quadro de jogo (o Kirby depois do roteiro de botões) e a decisão de overscan da sessão.
fn game_frame() -> Option<(Box<[[u8; 256]; 240]>, bool)> {
    let rom = roms::games()
        .into_iter()
        .find(|p| p.to_string_lossy().contains("Kirby"))?;
    let rom = std::fs::read(rom).ok()?;
    let mut nes = Box::new(nes_core::Nes::<false>::new(&rom).ok()?);
    let mut overscan = Overscan::default();
    for f in 0..1000 {
        nes.bus.pad = [roms::buttons(f), 0];
        nes.run_frame();
        overscan.update(&nes.bus.ppu.fb);
    }
    Some((Box::new(nes.bus.ppu.fb), overscan.crop))
}

/// O papel de parede: `pocketmister/background.fnd` do cartão, ou a paisagem padrão.
fn background(sd: &Path) -> Box<Background> {
    std::fs::read(sd.join("pocketmister/backgrounds/landscape.fnd"))
        .ok()
        .and_then(|d| Background::from_file(&d))
        .map_or_else(|| Box::new(Background::generate(Style::Plain)), Box::new)
}

/// Os fundos gerados, um `.fnd` por estilo, numa pasta (a `pocketmister/backgrounds` do cartão).
fn write_builtin_backgrounds(dir: &Path) -> Result<usize, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut buf = vec![0u8; interface::background::FILE_SIZE];
    for s in Style::ALL {
        let out: &mut [u8; interface::background::FILE_SIZE] =
            buf.as_mut_slice().try_into().map_err(|_| "tamanho")?;
        Background::generate(s).write(out);
        let path = dir.join(format!("{}.fnd", s.id()));
        std::fs::write(&path, &buf).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(Style::ALL.len())
}

/// `cargo xtask backgrounds <pasta>`: grava os fundos gerados na pasta (ex.: `<cartão>/pocketmister/backgrounds`).
pub fn backgrounds(args: &[String]) -> Result<bool, String> {
    let [dir] = args else {
        return Err("uso: cargo xtask backgrounds <cartao>/pocketmister/backgrounds".into());
    };
    let n = write_builtin_backgrounds(Path::new(dir))?;
    println!("{n} fundos em {dir}");
    Ok(true)
}

pub fn simulate(args: &[String]) -> Result<bool, String> {
    let repo = roms::repo();
    let sd = args.first().map_or(repo.join("demo/sd"), PathBuf::from);
    let out_dir = args
        .get(1)
        .map_or(repo.join("demo/simulation"), PathBuf::from);
    if !sd.join("nes").is_dir() {
        create_card(&sd)?;
        println!("cartão de exemplo criado em {}", sd.display());
    }
    let background = background(&sd);
    let favorites =
        std::fs::read_to_string(sd.join("pocketmister/favorites.txt")).unwrap_or_default();
    let fav = |i: &Item| favorites.lines().any(|l| l.ends_with(i.name));
    let root = read_folder(System::Nes, &sd.join("nes"), true)?;
    let list = items(&root, System::Nes);
    let hb = read_folder(System::Nes, &sd.join("nes/Homebrew"), false)?;
    let homebrew_list = items(&hb, System::Nes);
    let game = game_frame();
    if let Some((_, crop)) = &game {
        let state = if *crop {
            "recortado (coluna esquerda apagada pelo jogo)"
        } else {
            "inteiro"
        };
        println!("Kirby: overscan {state}");
    }

    for lang in [Language::PtBr, Language::En] {
        let tx = lang.texts();
        let mut rendered: Vec<(&str, Box<Canvas>)> = Vec::new();

        // 1. tela inicial
        let mut home: Vec<Home> = System::ALL
            .iter()
            .map(|&s| Home::System {
                system: s,
                games: count_games(s, &sd.join(s.folder())),
            })
            .collect();
        home.extend([
            Home::Fixed(tx.favorites),
            Home::Fixed(tx.recent),
            Home::Fixed(tx.settings),
        ]);
        let mut t = Box::<Canvas>::default();
        screens::home(&mut t, &background, &home, 0, "alfa 0.1", tx);
        rendered.push(("1-home", t));

        // 2. lista da raiz do NES, com o 3º item marcado
        let mut t = Box::<Canvas>::default();
        let sel = 2.min(list.len().saturating_sub(1));
        let folder = Folder {
            system: System::Nes,
            subfolder: None,
            items: &list,
            sel,
            strip_tags: true,
        };
        screens::games(&mut t, &background, &folder, fav, tx);
        rendered.push(("2-games-nes", t));

        // 3. subpasta Homebrew
        let mut t = Box::<Canvas>::default();
        let folder = Folder {
            system: System::Nes,
            subfolder: Some("Homebrew"),
            items: &homebrew_list,
            sel: 1,
            strip_tags: true,
        };
        screens::games(&mut t, &background, &folder, |_| false, tx);
        rendered.push(("3-games-homebrew", t));

        // 4 e 5. o jogo, com e sem o menu do jogo por cima
        if let Some((fb, crop)) = &game {
            let mut t = Box::<Canvas>::default();
            t.nes_image(fb, *crop);
            let options = [
                (tx.resume, ""),
                (tx.reset, ""),
                (tx.save_state, "1"),
                (tx.load_state, "1"),
                (tx.shader, "OSSC"),
                (tx.settings, ""),
                (tx.quit_to_menu, ""),
            ];
            screens::game_menu(&mut t, "Kirby's Adventure", &options, 4, tx);
            rendered.push(("4-game-menu", t));
            let mut t = Box::<Canvas>::default();
            t.nes_image(fb, *crop);
            rendered.push(("5-game", t));
        }

        for mode in &video::MODES {
            let mode_name = format!("{}-{}", mode.name, mode.aspect.replace(':', "x"));
            let folder = out_dir.join(lang.code()).join(mode_name);
            std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
            for (name, t) in &rendered {
                compose(t, mode)
                    .save(folder.join(format!("{name}.png")))
                    .map_err(|e| e.to_string())?;
            }
        }
        println!(
            "{}: {} telas em cada um dos {} modos",
            lang.code(),
            rendered.len(),
            video::MODES.len()
        );
    }
    for mode in &video::MODES {
        let f = video::frame(mode);
        println!(
            "{:8} {:5}  app {}x{} (x{}) em ({},{})  relógio {} MHz  {}.{:02} Hz",
            mode.name,
            mode.aspect,
            f.width,
            f.height,
            f.scale,
            f.x,
            f.y,
            mode.clock_mhz(),
            mode.hz_x100() / 100,
            mode.hz_x100() % 100
        );
    }
    println!("telas em {}", out_dir.display());
    options(&out_dir.join("options"), &sd, &list)?;
    Ok(true)
}

/// `cargo xtask background imagem saida.fnd`: papel de parede para o cartão (`pocketmister/background.fnd`).
/// A imagem é cortada em 4:3 no centro, reduzida a 320x240, 160 cores, e escurecida como o tema.
pub fn convert_background(args: &[String]) -> Result<bool, String> {
    let [input, out_dir] = args else {
        return Err("uso: cargo xtask background imagem.png saida.fnd".into());
    };
    let img = image::ImageReader::open(input)
        .map_err(|e| format!("{input}: {e}"))?
        .with_guessed_format()
        .map_err(|e| format!("{input}: {e}"))?
        .decode()
        .map_err(|e| format!("{input}: {e}"))?
        .to_rgba8();
    let (w, h) = img.dimensions();
    // recorte 4:3 no centro (nunca esticar)
    let (cw, ch) = if w * 3 > h * 4 {
        (h * 4 / 3, h)
    } else {
        (w, w * 3 / 4)
    };
    let cropped = image::imageops::crop_imm(&img, (w - cw) / 2, (h - ch) / 2, cw, ch).to_image();
    let small = image::imageops::resize(&cropped, 320, 240, image::imageops::FilterType::Lanczos3);
    let raw = small.as_raw();
    let nq = color_quant::NeuQuant::new(10, interface::theme::BACKGROUND_COLORS, raw);
    let mut out = Vec::from(&interface::background::MAGIC[..]);
    let map = nq.color_map_rgba();
    for c in map
        .as_chunks::<4>()
        .0
        .iter()
        .take(interface::theme::BACKGROUND_COLORS)
    {
        // 60% de opacidade sobre preto, como o tema
        out.extend(c[..3].iter().map(|&v| (u16::from(v) * 0x99 / 0xFF) as u8));
    }
    while out.len() < 8 + interface::theme::BACKGROUND_COLORS * 3 {
        out.push(0);
    }
    for px in raw.as_chunks::<4>().0 {
        out.push(nq.index_of(&px[..]) as u8);
    }
    std::fs::write(out_dir, &out).map_err(|e| format!("{out_dir}: {e}"))?;
    println!(
        "{out_dir}: 320x240, {} cores",
        interface::theme::BACKGROUND_COLORS
    );
    Ok(true)
}

/// Um print de cada esquema de cores (lista de jogos) e de cada fundo (tela inicial), a tela de
/// Ajustes e duas pranchas com todos lado a lado, em 640x480.
fn options(out_dir: &Path, sd: &Path, list: &[Item]) -> Result<(), String> {
    let mode = &video::MODES[0];
    let tx = Language::PtBr.texts();
    let colors_dir = out_dir.join("colors");
    let backgrounds_dir = out_dir.join("backgrounds");
    std::fs::create_dir_all(&colors_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&backgrounds_dir).map_err(|e| e.to_string())?;
    let favorites =
        std::fs::read_to_string(sd.join("pocketmister/favorites.txt")).unwrap_or_default();
    let fav = |i: &Item| favorites.lines().any(|l| l.ends_with(i.name));
    let default_bg = background(sd);
    let sel = 2.min(list.len().saturating_sub(1));

    let mut color_sheet = Vec::new();
    for e in &theme::SCHEMES {
        let mut t = Box::<Canvas>::default();
        theme::apply(&mut t.palette, e);
        let folder = Folder {
            system: System::Nes,
            subfolder: None,
            items: list,
            sel,
            strip_tags: true,
        };
        screens::games(&mut t, &default_bg, &folder, fav, tx);
        // o nome do esquema no canto, para a prancha
        // branco da paleta do NES: lê em qualquer fundo
        t.text(28, 14, e.name_pt, 0x30);
        let img = compose(&t, mode);
        img.save(colors_dir.join(format!("{}.png", e.id)))
            .map_err(|e| e.to_string())?;
        color_sheet.push(img);
    }

    let mut background_sheet = Vec::new();
    let mut home: Vec<Home> = System::ALL
        .iter()
        .map(|&s| Home::System {
            system: s,
            games: count_games(s, &sd.join(s.folder())),
        })
        .collect();
    home.extend([
        Home::Fixed(tx.favorites),
        Home::Fixed(tx.recent),
        Home::Fixed(tx.settings),
    ]);
    // os fundos que estão no cartão, pelo nome do arquivo
    let mut files: Vec<PathBuf> = std::fs::read_dir(sd.join("pocketmister/backgrounds"))
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fnd")))
        .collect();
    files.sort();
    for file in &files {
        let stem = file
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let Some(f) = std::fs::read(file)
            .ok()
            .and_then(|d| Background::from_file(&d))
        else {
            continue;
        };
        let mut t = Box::<Canvas>::default();
        screens::home(&mut t, &f, &home, 0, &stem, tx);
        let img = compose(&t, mode);
        img.save(backgrounds_dir.join(format!("{stem}.png")))
            .map_err(|e| e.to_string())?;
        background_sheet.push(img);
    }

    // Ajustes, com o esquema e o fundo marcados
    let mut t = Box::<Canvas>::default();
    let items = [
        (tx.colors, theme::SCHEMES[0].name_pt),
        (tx.background, "landscape"),
        (tx.lang, tx.lang_name),
        (tx.video_mode, "640x480 4:3"),
    ];
    screens::settings(&mut t, &default_bg, &items, 1, tx);
    compose(&t, mode)
        .save(out_dir.join("settings.png"))
        .map_err(|e| e.to_string())?;

    sheet(&color_sheet, 3)
        .save(out_dir.join("sheet-colors.png"))
        .map_err(|e| e.to_string())?;
    sheet(&background_sheet, 3)
        .save(out_dir.join("sheet-backgrounds.png"))
        .map_err(|e| e.to_string())?;
    println!(
        "opções: {} esquemas de cores, {} fundos e os ajustes em {}",
        theme::SCHEMES.len(),
        Style::ALL.len(),
        out_dir.display()
    );
    Ok(())
}

/// Miniaturas (metade do tamanho) em grade, com 8 pixels pretos entre elas.
fn sheet(imgs: &[image::RgbImage], colunas: usize) -> image::RgbImage {
    let (w, h) = (imgs[0].width() / 2, imgs[0].height() / 2);
    let linhas = imgs.len().div_ceil(colunas) as u32;
    let mut out = image::RgbImage::new(colunas as u32 * (w + 8) + 8, linhas * (h + 8) + 8);
    for (i, img) in imgs.iter().enumerate() {
        let small = image::imageops::resize(img, w, h, image::imageops::FilterType::Triangle);
        let (cx, cy) = ((i % colunas) as u32, (i / colunas) as u32);
        image::imageops::overlay(
            &mut out,
            &small,
            i64::from(8 + cx * (w + 8)),
            i64::from(8 + cy * (h + 8)),
        );
    }
    out
}

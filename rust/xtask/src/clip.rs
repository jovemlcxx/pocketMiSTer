//! Vídeo de 60 quadros em 4:3 de um jogo rodando no núcleo, com entrada roteirizada.
//! Porte de `firmware/tools/make_clip.py`.
//!
//! uso: cargo xtask clip rom.nes saida.mp4 INICIO [--press botao:quadro:duracao ...]
//!
//! Roda o jogo até INICIO+60, guarda os quadros INICIO..INICIO+59, corta 8 pixels de cima e de
//! baixo (o overscan de uma TV de tubo) e, se o jogo recorta a coluna da esquerda, 8 de cada lado;
//! salva 1024x768 (4:3) a 60 fps, com o som da mesma janela (AAC). Precisa do ffmpeg no PATH.

use nes_core::Nes;
use nes_core::ppu::{H, PALETTE, W};
use std::path::PathBuf;
use std::process::Command;

const BUTTONS: [&str; 8] = ["a", "b", "select", "start", "up", "down", "left", "right"];

/// `botao:quadro:duracao` (p2. para o controle 2): (controle, máscara, de, até)
fn press(s: &str) -> Result<(usize, u8, u32, u32), String> {
    let mut p = s.split(':');
    let name = p.next().unwrap_or_default();
    let (pad, name) = name.strip_prefix("p2.").map_or((0, name), |n| (1, n));
    let bit = BUTTONS
        .iter()
        .position(|&b| b == name)
        .ok_or(format!("--press inválido: {s}"))?;
    let from: u32 = p
        .next()
        .and_then(|x| x.parse().ok())
        .ok_or(format!("--press inválido: {s}"))?;
    let dur: u32 = p
        .next()
        .map_or(Some(1), |x| x.parse().ok())
        .ok_or(format!("--press inválido: {s}"))?;
    Ok((pad, 1 << bit, from, from + dur))
}

fn rgb(fb: &[[u8; W]; H]) -> Vec<u8> {
    fb.as_flattened()
        .iter()
        .flat_map(|&i| PALETTE[usize::from(i & 0x3F)])
        .collect()
}

/// Cada linha visível dos 8 pixels da esquerda tem uma cor só (coluna recortada pelo jogo).
fn left_column_blank(rgb: &[u8]) -> bool {
    (8..232).all(|y| {
        let row = &rgb[y * W * 3..y * W * 3 + 8 * 3];
        row.as_chunks::<3>().0.iter().all(|p| p[..] == row[..3])
    })
}

pub fn clip(args: &[String]) -> Result<bool, String> {
    let mut pos = Vec::new();
    let mut presses = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--press" {
            presses.push(press(it.next().ok_or("--press sem valor")?)?);
        } else {
            pos.push(a.as_str());
        }
    }
    let [rom_path, out, start] = pos[..] else {
        return Err(
            "uso: cargo xtask clip rom.nes saida.mp4 INICIO [--press botao:quadro:duracao ...]"
                .into(),
        );
    };
    let start: u32 = start
        .parse()
        .map_err(|_| format!("INICIO inválido: {start}"))?;
    let rom = std::fs::read(rom_path).map_err(|e| format!("{rom_path}: {e}"))?;
    let mut nes =
        Box::new(Nes::<false>::new(&rom).map_err(|e| format!("não carregou ({})", e.code()))?);

    let tmp = std::env::temp_dir().join(format!("pocketmister-clip-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let mut frames = Vec::new();
    let mut audio: Vec<i16> = Vec::new();
    let mut buf = [0i16; 2048];
    for f in 0..start + 60 {
        nes.bus.pad = [0, 0];
        for &(pad, mask, from, to) in &presses {
            if (from..to).contains(&f) {
                nes.bus.pad[pad] |= mask;
            }
        }
        nes.run_frame();
        let n = nes.take_samples(&mut buf);
        if f >= start {
            frames.push(rgb(&nes.bus.ppu.fb));
            audio.extend_from_slice(&buf[..n]);
        }
    }
    // corta 8 pixels dos lados só se o jogo recorta a coluna da esquerda em todos os quadros
    // (como o Kirby); jogos que desenham ali (a barra do Castlevania III) ficam inteiros
    let w = if frames.iter().all(|f| left_column_blank(f)) {
        240
    } else {
        256
    };
    for (k, f) in frames.iter().enumerate() {
        let mut ppm = format!("P6\n{W} {H}\n255\n").into_bytes();
        ppm.extend_from_slice(f);
        let path: PathBuf = tmp.join(format!("{:05}.ppm", start as usize + k));
        std::fs::write(&path, ppm).map_err(|e| e.to_string())?;
    }
    let wav = tmp.join("som.wav");
    let mut bytes = nes_core::apu::wav_header(audio.len() as u32).to_vec();
    bytes.extend(audio.iter().flat_map(|s| s.to_le_bytes()));
    std::fs::write(&wav, bytes).map_err(|e| e.to_string())?;
    // nearest mantém os pixels nítidos; o lanczos final leva a 4:3 sem degraus
    let vf = format!(
        "crop={w}:224:{}:8,scale={}:896:flags=neighbor,scale=1024:768:flags=lanczos,format=yuv420p",
        (256 - w) / 2,
        w * 4
    );
    let st = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-framerate",
            "60",
            "-start_number",
            &start.to_string(),
            "-i",
        ])
        .arg(tmp.join("%05d.ppm"))
        .arg("-i")
        .arg(&wav)
        .args([
            "-vf",
            &vf,
            "-c:v",
            "libx264",
            "-crf",
            "16",
            "-r",
            "60",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-shortest",
            out,
        ])
        .status();
    let _ = std::fs::remove_dir_all(&tmp);
    match st {
        Ok(s) if s.success() => {
            println!("{out}: 60 quadros, 1024x768 (4:3), 60 fps");
            Ok(true)
        }
        Ok(_) => Err("o ffmpeg falhou".into()),
        Err(e) => Err(format!("ffmpeg: {e} (instale o ffmpeg)")),
    }
}

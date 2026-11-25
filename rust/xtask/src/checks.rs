//! nestest, blargg e identidade, rodando o núcleo direto (sem binário externo).

use crate::roms;
use nes_core::Nes;
use std::path::{Path, PathBuf};

fn read_file(p: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))
}

fn field(line: &str, key: &str) -> Option<u32> {
    let s = &line[line.find(key)? + key.len()..];
    let end = s
        .find(|c: char| !c.is_ascii_alphanumeric())
        .unwrap_or(s.len());
    if key == "CYC:" {
        s[..end].parse().ok()
    } else {
        u32::from_str_radix(&s[..end], 16).ok()
    }
}

/// nestest.nes em modo automático ($C000), instrução por instrução contra o nestest.log
/// (PC, A, X, Y, P, SP e ciclos), com o sistema inteiro.
pub fn nestest(args: &[String]) -> Result<bool, String> {
    let dir = roms::repo().join("rust/nes-core/tests/roms");
    let rom_path = args.first().map_or(dir.join("nestest.nes"), PathBuf::from);
    let log_path = args.get(1).map_or(dir.join("nestest.log"), PathBuf::from);
    let rom = read_file(&rom_path)?;
    let log = String::from_utf8(read_file(&log_path)?).map_err(|e| e.to_string())?;
    let mut nes =
        Box::new(Nes::<false>::new(&rom).map_err(|e| format!("ROM inválida ({})", e.code()))?);
    nes.cpu.pc = 0xC000;
    let mut n = 0;
    for line in log.lines() {
        let want = (|| {
            Some((
                u32::from_str_radix(line.get(..4)?, 16).ok()?,
                field(line, "A:")?,
                field(line, "X:")?,
                field(line, "Y:")?,
                field(line, "P:")?,
                field(line, "SP:")?,
                field(line, "CYC:")?,
            ))
        })()
        .ok_or(format!("linha {} do log não reconhecida", n + 1))?;
        let c = &nes.cpu;
        let got = (
            u32::from(c.pc),
            u32::from(c.a),
            u32::from(c.x),
            u32::from(c.y),
            u32::from(c.get_p(false)),
            u32::from(c.s),
            c.cycles,
        );
        if got != want {
            println!("DIFERENÇA na instrução {}\n  esperado: {line}", n + 1);
            println!(
                "  obtido:   PC:{:04X} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} CYC:{}",
                got.0, got.1, got.2, got.3, got.4, got.5, got.6
            );
            return Ok(false);
        }
        nes.step();
        n += 1;
    }
    println!(
        "OK: {n} instruções idênticas ao nestest.log (resultado em $02/$03: {:02X} {:02X})",
        nes.bus.ram[2], nes.bus.ram[3]
    );
    Ok(nes.bus.ram[2] == 0 && nes.bus.ram[3] == 0)
}

/// Roda um teste do blargg até ele assinar o resultado em $6000 (no máximo `frames` quadros).
/// Devolve o código ($6000) ou None se não terminou.
fn blargg_one(rom: &[u8], frames: u32) -> Result<Option<u8>, String> {
    let mut nes =
        Box::new(Nes::<false>::new(rom).map_err(|e| format!("não carregou ({})", e.code()))?);
    let mut reset_in: Option<u32> = None;
    let mut waiting = false;
    for _ in 0..frames {
        nes.run_frame();
        if nes.cpu.jammed {
            return Ok(None);
        }
        let m = &nes.bus.prg_ram;
        let signed = m[1] == 0xDE && m[2] == 0xB0 && m[3] == 0x61;
        if signed && m[0] < 0x80 {
            return Ok(Some(m[0]));
        }
        // $81: o teste pede o botão reset depois de pelo menos 100 ms (6 quadros)
        if signed && m[0] == 0x81 && !waiting {
            reset_in = Some(6);
            waiting = true;
        } else if m[0] != 0x81 {
            waiting = false;
        }
        if let Some(n) = reset_in {
            if n == 0 {
                reset_in = None;
                nes.reset();
            } else {
                reset_in = Some(n - 1);
            }
        }
    }
    Ok(None)
}

/// Placar dos testes do blargg, todos em paralelo.
pub fn blargg(args: &[String]) -> Result<bool, String> {
    let dir = args.first().map_or(
        roms::repo().join("referencias/nes-test-roms"),
        PathBuf::from,
    );
    let list = roms::blargg(&dir);
    if list.is_empty() {
        return Err(format!(
            "nenhuma ROM em {} (clone https://github.com/christopherpow/nes-test-roms)",
            dir.display()
        ));
    }
    let res: Vec<Result<Option<u8>, String>> = std::thread::scope(|s| {
        let h: Vec<_> = list
            .iter()
            .map(|p| s.spawn(move || blargg_one(&read_file(p)?, 3000)))
            .collect();
        h.into_iter()
            .map(|h| h.join().unwrap_or(Err("pânico".into())))
            .collect()
    });
    let (mut pass, mut fail) = (0, 0);
    for (p, r) in list.iter().zip(res) {
        let name = p.file_name().unwrap_or_default().to_string_lossy();
        if matches!(r, Ok(Some(0))) {
            pass += 1;
            println!("ok    {name}");
        } else {
            fail += 1;
            println!("FALHA {name}");
        }
    }
    println!("passaram {pass}, falharam {fail}");
    Ok(fail == 0)
}

/// Hashes (imagem, RAM, ciclos) de cada quadro, com o roteiro de botões.
fn hashes<const R: bool>(rom: &[u8], frames: u32) -> Result<Vec<(u32, u32, u32)>, String> {
    let mut nes = Box::new(Nes::<R>::new(rom).map_err(|e| format!("não carregou ({})", e.code()))?);
    let mut v = Vec::with_capacity(frames as usize);
    for f in 0..frames {
        nes.bus.pad = [roms::buttons(f), 0];
        nes.run_frame();
        v.push((
            roms::fnv(nes.bus.ppu.fb.as_flattened()),
            roms::fnv(&nes.bus.ram),
            nes.cpu.cycles,
        ));
        if nes.cpu.jammed {
            break;
        }
    }
    Ok(v)
}

/// Teste de identidade: a PPU rápida tem que produzir, quadro a quadro, a mesma imagem, a mesma
/// RAM e os mesmos ciclos que a referência ponto a ponto. As duas saem do código atual a cada
/// execução, então não há referência gravada para ficar velha.
pub fn identity(_args: &[String]) -> Result<bool, String> {
    let r = roms::repo();
    let mut list = roms::games();
    list.push(r.join("referencias/nes-test-roms/mmc3_test_2/rom_singles/4-scanline_timing.nes"));
    list.push(r.join("referencias/nes-test-roms/ppu_vbl_nmi/rom_singles/10-even_odd_timing.nes"));
    list.retain(|p| p.exists());
    if list.is_empty() {
        return Err("nenhuma ROM em roms/ nem em referencias/".into());
    }
    let res: Vec<Result<Option<usize>, String>> = std::thread::scope(|s| {
        let h: Vec<_> = list
            .iter()
            .map(|p| {
                s.spawn(move || {
                    let rom = read_file(p)?;
                    let (a, b) = (hashes::<false>(&rom, 1500)?, hashes::<true>(&rom, 1500)?);
                    Ok(if a == b && !a.is_empty() {
                        None
                    } else {
                        Some(a.iter().zip(&b).take_while(|(x, y)| x == y).count())
                    })
                })
            })
            .collect();
        h.into_iter()
            .map(|h| h.join().unwrap_or(Err("pânico".into())))
            .collect()
    });
    let (mut ok, mut bad) = (0, 0);
    for (p, r) in list.iter().zip(res) {
        let name = p.file_stem().unwrap_or_default().to_string_lossy();
        match r? {
            None => ok += 1,
            Some(f) => {
                bad += 1;
                println!("DIFERE {name}: a partir do quadro {f}");
            }
        }
    }
    println!("idênticos {ok}, diferentes {bad}");
    Ok(bad == 0)
}

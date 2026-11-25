//! Caminhos do repositório, a lista de ROMs de teste e o roteiro de botões dos jogos.

use std::path::{Path, PathBuf};

/// raiz do repositório (rust/xtask/../..)
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Os testes do blargg que relatam o resultado em $6000 (os mesmos do `run_blargg.sh`).
pub fn blargg(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut folder = |sub: &str, keep: &dyn Fn(&str) -> bool| {
        let Ok(rd) = std::fs::read_dir(dir.join(sub)) else {
            return;
        };
        let mut v: Vec<PathBuf> = rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
                n.ends_with(".nes") && keep(n)
            })
            .collect();
        v.sort();
        out.extend(v);
    };
    let any = |_: &str| true;
    folder("instr_test-v5/rom_singles", &any);
    folder("instr_misc/rom_singles", &any);
    folder("ppu_vbl_nmi/rom_singles", &any);
    folder("ppu_open_bus", &|n| n == "ppu_open_bus.nes");
    folder("oam_read", &|n| n == "oam_read.nes");
    folder("mmc3_test_2/rom_singles", &|n| {
        n.as_bytes()
            .first()
            .is_some_and(|c| (b'1'..=b'5').contains(c))
            && n.as_bytes().get(1) == Some(&b'-')
    });
    folder("apu_test/rom_singles", &any);
    folder("apu_reset", &any);
    folder("dmc_dma_during_read4", &any);
    out
}

/// Botões dos jogos: Start várias vezes para passar das telas de título, depois andar, pular e
/// atirar (o mesmo roteiro dos casos gravados e do `identity.sh`).
pub const SCRIPT: &[(u8, u32, u32)] = &[
    (nes_core::nes::pad::START, 240, 5),
    (nes_core::nes::pad::START, 420, 5),
    (nes_core::nes::pad::START, 600, 5),
    (nes_core::nes::pad::START, 800, 5),
    (nes_core::nes::pad::START, 1000, 5),
    (nes_core::nes::pad::RIGHT, 900, 600),
    (nes_core::nes::pad::A, 1100, 10),
    (nes_core::nes::pad::B, 1200, 30),
];

/// Controle 1 no quadro `f` segundo o roteiro.
pub fn buttons(f: u32) -> u8 {
    SCRIPT
        .iter()
        .filter(|(_, from, dur)| f >= *from && f < from + dur)
        .fold(0, |m, (b, _, _)| m | b)
}

pub fn fnv(bytes: &[u8]) -> u32 {
    bytes.iter().fold(2_166_136_261u32, |h, &b| {
        (h ^ u32::from(b)).wrapping_mul(16_777_619)
    })
}

/// Os jogos de `roms/` (fora do git, ADR-007).
pub fn games() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(repo().join("roms"))
        .map(|rd| {
            rd.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "nes"))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

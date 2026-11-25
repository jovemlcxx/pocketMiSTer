//! `cargo xtask <comando>` (rodar dentro de `rust/`). Substitui os scripts `.sh` e as
//! ferramentas em Python e C (ADR-001).
//!
//!   nestest [rom log]       CPU instrução por instrução contra o nestest.log
//!   blargg [pasta]          placar das ROMs de teste do blargg (padrão: ../referencias/nes-test-roms)
//!   identidade              PPU rápida contra a referência, quadro a quadro (imagem, RAM, ciclos)
//!   clip rom saida INICIO   vídeo de 60 quadros 4:3 (precisa do ffmpeg)
//!   esquema [pasta]         os dois SVGs do manual (hardware/)
//!   pacote saida.bin roms   jogos para gravar na flash da placa (picotool load -o 0x10800000)
//!   simular [sd] [saida]    telas do frontend em PNG, em cada modo de vídeo
//!   fundo imagem saida.fnd  papel de parede do menu (pocketmister/background.fnd no cartão)

mod checks;
mod clip;
mod pack;
mod roms;
mod schematic;
mod simulate;

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or(&[]);
    let ok = match args.first().map(String::as_str) {
        Some("nestest") => checks::nestest(rest),
        Some("blargg") => checks::blargg(rest),
        Some("identity") => checks::identity(rest),
        Some("clip") => clip::clip(rest),
        Some("schematic") => schematic::schematic(rest),
        Some("pack") => pack::pack(rest),
        Some("simulate") => simulate::simulate(rest),
        Some("background") => simulate::convert_background(rest),
        Some("backgrounds") => simulate::backgrounds(rest),
        _ => {
            eprintln!(
                "uso: cargo xtask <nestest|blargg|identity|clip|schematic|pack|simulate|background|backgrounds> [args]"
            );
            return ExitCode::from(2);
        }
    };
    match ok {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("erro: {e}");
            ExitCode::from(2)
        }
    }
}

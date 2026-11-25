//! `cargo xtask pack saida.bin rom1.nes rom2.nes ...`: junta os jogos no formato de
//! `nes_core::pack`, para gravar na flash da placa junto com o firmware:
//!
//!   picotool load -o 0x10800000 saida.bin
//!
//! (a 8 MB do começo da flash; o firmware procura o pacote ali).

use nes_core::pack::{ENTRY_LEN, HEADER_LEN, MAGIC, NAME_LEN};
use std::path::Path;

pub fn pack(args: &[String]) -> Result<bool, String> {
    let [out, roms @ ..] = args else {
        return Err("uso: cargo xtask pack saida.bin rom1.nes rom2.nes ...".into());
    };
    if roms.is_empty() {
        return Err("nenhuma ROM".into());
    }
    let mut head = Vec::from(&MAGIC[..]);
    head.extend((roms.len() as u32).to_le_bytes());
    let mut data = Vec::new();
    let base = HEADER_LEN + roms.len() * ENTRY_LEN;
    for r in roms {
        let rom = std::fs::read(r).map_err(|e| format!("{r}: {e}"))?;
        nes_core::Nes::<false>::new(&rom)
            .map_err(|e| format!("{r}: não carrega ({})", e.code()))?;
        let stem = Path::new(r)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        let mut name = [0u8; NAME_LEN];
        // corta no limite de um caractere UTF-8
        let mut n = stem.len().min(NAME_LEN);
        while !stem.is_char_boundary(n) {
            n -= 1;
        }
        name[..n].copy_from_slice(&stem.as_bytes()[..n]);
        while data.len() % 4 != 0 {
            data.push(0);
        }
        head.extend(name);
        head.extend(((base + data.len()) as u32).to_le_bytes());
        head.extend((rom.len() as u32).to_le_bytes());
        data.extend(rom);
    }
    head.extend(data);
    std::fs::write(out, &head).map_err(|e| format!("{out}: {e}"))?;
    // confere lendo de volta com o mesmo código que o firmware usa
    for i in 0..roms.len() {
        let (name, img) = nes_core::pack::entry(&head, i).ok_or("pacote inválido ao reler")?;
        println!("{:3}  {:>8} bytes  {name}", i + 1, img.len());
    }
    println!("{out}: {} jogos, {} bytes", roms.len(), head.len());
    Ok(true)
}

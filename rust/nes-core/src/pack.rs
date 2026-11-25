//! Pacote de jogos gravado na flash junto com o firmware (`cargo xtask pack`):
//!
//! - bytes 0-7: `PNESPACK`; 8-11: quantidade de jogos (u32, little endian);
//! - uma entrada de 64 bytes por jogo: nome (56 bytes, UTF-8 completado com zeros), início e
//!   tamanho da imagem iNES (u32 cada, a partir do começo do pacote);
//! - as imagens, cada uma alinhada a 4 bytes.

pub const MAGIC: &[u8; 8] = b"PNESPACK";
pub const NAME_LEN: usize = 56;
pub const ENTRY_LEN: usize = 64;
pub const HEADER_LEN: usize = 12;

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

/// Quantidade de jogos, se `data` começa com um pacote válido.
pub fn count(data: &[u8]) -> Option<usize> {
    if data.get(..8)? != MAGIC {
        return None;
    }
    Some(u32_at(data, 8)? as usize)
}

/// Nome e imagem iNES do jogo `i` (None se o pacote estiver corrompido).
pub fn entry(data: &[u8], i: usize) -> Option<(&str, &[u8])> {
    if i >= count(data)? {
        return None;
    }
    let e = HEADER_LEN + i * ENTRY_LEN;
    let raw = data.get(e..e + NAME_LEN)?;
    let end = raw.iter().position(|&b| b == 0).unwrap_or(NAME_LEN);
    let name = core::str::from_utf8(&raw[..end]).ok()?;
    let start = u32_at(data, e + NAME_LEN)? as usize;
    let len = u32_at(data, e + NAME_LEN + 4)? as usize;
    Some((name, data.get(start..start.checked_add(len)?)?))
}

/// Tamanho total do pacote (até o fim da última imagem), para saber quanto ler da flash.
pub fn total_len(data: &[u8]) -> Option<usize> {
    let n = count(data)?;
    let mut end = HEADER_LEN + n * ENTRY_LEN;
    for i in 0..n {
        let e = HEADER_LEN + i * ENTRY_LEN + NAME_LEN;
        end = end.max(u32_at(data, e)? as usize + u32_at(data, e + 4)? as usize);
    }
    Some(end)
}

//! Roda uma ROM no PC sem janela: N quadros, salva o último como PPM e, se for um teste do
//! blargg, mostra o resultado ($6000 = estado, $6001-$6003 = DE B0 61, texto a partir de $6004).
//! uso: nes_run rom.nes [quadros=600] [saida.ppm] [--press botao:quadro:duracao]...
//!      [--dump pasta:primeiro:quantidade] [--hashes arquivo|-] [--wav saida.wav] [--sav]
//! `--sav`: cartucho com bateria carrega e grava a RAM de $6000 em `rom.sav` (ao lado da ROM).
//! Teste do blargg que pede reset ($6000 = $81) leva o botão reset 6 quadros (100 ms) depois.
//! Porte de `firmware/host/nes_run.c`, com a mesma saída byte a byte.

use nes_core::nes::Nes;
use nes_core::ppu::{H, PALETTE, W};
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::process::ExitCode;
use std::time::Instant;

/// Saída em modo texto, como o `stdout` do C: no Windows cada `\n` vira `\r\n`.
struct Text<T: Write>(T);
impl<T: Write> Write for Text<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if cfg!(windows) {
            for part in buf.split_inclusive(|&b| b == b'\n') {
                match part.strip_suffix(b"\n") {
                    Some(line) => {
                        self.0.write_all(line)?;
                        self.0.write_all(b"\r\n")?;
                    }
                    None => self.0.write_all(part)?,
                }
            }
        } else {
            self.0.write_all(buf)?;
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

fn save_ppm(fb: &[[u8; W]; H], path: &str) {
    let mut out = Vec::with_capacity(16 + W * H * 3);
    out.extend_from_slice(format!("P6\n{W} {H}\n255\n").as_bytes());
    for px in fb.as_flattened() {
        out.extend_from_slice(&PALETTE[usize::from(px & 0x3F)]);
    }
    let _ = std::fs::write(path, out); // como o C: se não abrir, segue sem imagem
}

/// `atoi` do C: espaços, sinal e dígitos; o resto é ignorado; nada lido = 0.
fn atoi(s: &str) -> i32 {
    let s = s.trim_start();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let n = digits
        .bytes()
        .take_while(u8::is_ascii_digit)
        .fold(0i64, |n, d| (n * 10 + i64::from(d - b'0')).min(1 << 40));
    (if neg { -n } else { n }) as i32
}

/// `%d` do `sscanf`: exige pelo menos um dígito; devolve o valor e o resto.
fn scan_int(s: &str) -> Option<(i32, &str)> {
    let t = s.trim_start();
    let sign = usize::from(t.starts_with(['-', '+']));
    let n = t[sign..].bytes().take_while(u8::is_ascii_digit).count();
    if n == 0 {
        return None;
    }
    Some((atoi(&t[..sign + n]), &t[sign + n..]))
}

/// --press botão:quadro:duração (botões: a b select start up down left right; p2. para o 2º)
struct Press {
    pad: usize,
    mask: u8,
    from: i32,
    to: i32,
}

fn parse_press(s: &str) -> Option<Press> {
    const NAMES: [&str; 8] = ["a", "b", "select", "start", "up", "down", "left", "right"];
    let (name, rest) = s.split_once(':')?;
    if name.is_empty() || name.len() > 15 {
        return None;
    }
    let (from, rest) = scan_int(rest)?;
    let dur = rest
        .strip_prefix(':')
        .and_then(scan_int)
        .map_or(1, |(d, _)| d);
    let (pad, n) = match name.strip_prefix("p2.") {
        Some(n) => (1, n),
        None => (0, name),
    };
    let i = NAMES.iter().position(|&x| x == n)?;
    Some(Press {
        pad,
        mask: 1 << i,
        from,
        to: from + dur,
    })
}

fn fnv(bytes: &[u8]) -> u32 {
    bytes.iter().fold(2_166_136_261u32, |h, &b| {
        (h ^ u32::from(b)).wrapping_mul(16_777_619)
    })
}

enum Hashes {
    Stdout,
    File(Text<BufWriter<File>>),
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let mut err = Text(io::stderr());
    let mut out = Text(BufWriter::new(io::stdout()));
    if argv.len() < 2 {
        let _ = writeln!(
            err,
            "uso: {} rom.nes [quadros] [saida.ppm] [--press botao:quadro:duracao]... [--dump pasta:primeiro:quantidade]",
            argv[0]
        );
        return ExitCode::from(2);
    }
    let mut frames = 600;
    let mut out_ppm: Option<&str> = None;
    let mut dump: Option<(String, i32, i32)> = None;
    let mut press: Vec<Press> = Vec::new();
    let mut hashes: Option<Hashes> = None; // uma linha por quadro: hash da imagem, da RAM e ciclos
    let mut wav: Option<String> = None;
    let mut sav = false;
    let mut i = 2;
    let mut pos = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        let has_next = i + 1 < argv.len();
        if a == "--press" && has_next {
            i += 1;
            match parse_press(&argv[i]) {
                Some(p) if press.len() < 64 => press.push(p),
                _ => {
                    let _ = writeln!(err, "--press inválido: {}", argv[i]);
                    return ExitCode::from(2);
                }
            }
        } else if a == "--dump" && has_next {
            // pasta:primeiro:quantidade, lido do fim (a pasta pode ter "C:")
            i += 1;
            let parsed = argv[i].rsplit_once(':').and_then(|(head, count)| {
                let (dir, first) = head.rsplit_once(':')?;
                Some((dir.to_string(), atoi(first), atoi(count)))
            });
            match parsed {
                Some(d) => dump = Some(d),
                None => {
                    let _ = writeln!(err, "--dump inválido");
                    return ExitCode::from(2);
                }
            }
        } else if a == "--sav" {
            sav = true;
        } else if a == "--wav" && has_next {
            i += 1;
            wav = Some(argv[i].clone());
        } else if a == "--hashes" && has_next {
            i += 1;
            hashes = if argv[i] == "-" {
                Some(Hashes::Stdout)
            } else {
                File::create(&argv[i])
                    .ok()
                    .map(|f| Hashes::File(Text(BufWriter::new(f))))
            };
        } else if pos == 0 {
            frames = atoi(a);
            pos += 1;
        } else {
            out_ppm = Some(a);
            pos += 1;
        }
        i += 1;
    }

    let rom = std::fs::read(&argv[1]).ok();
    let mut nes = match rom.as_deref().map(Nes::<false>::new) {
        Some(Ok(n)) => Box::new(n),
        e => {
            let code = match e {
                Some(Err(e)) => e.code(),
                _ => -9,
            };
            let _ = writeln!(err, "não carregou ({code}): {}", argv[1]);
            return ExitCode::from(2);
        }
    };

    let sav_path = std::path::Path::new(&argv[1]).with_extension("sav");
    if sav
        && nes.battery
        && let Ok(data) = std::fs::read(&sav_path)
    {
        let n = data.len().min(nes.bus.prg_ram.len());
        nes.bus.prg_ram[..n].copy_from_slice(&data[..n]);
    }
    let mut result: i32 = -1;
    let mut f = 0;
    let t0 = Instant::now();
    let signed = |m: &[u8]| m[1] == 0xDE && m[2] == 0xB0 && m[3] == 0x61;
    let mut audio: Vec<i16> = Vec::new();
    let mut reset_in: Option<u32> = None;
    let mut waiting_reset = false;
    while f < frames {
        nes.bus.pad = [0, 0];
        for p in &press {
            if f >= p.from && f < p.to {
                nes.bus.pad[p.pad] |= p.mask;
            }
        }
        nes.run_frame();
        if wav.is_some() {
            let mut buf = [0i16; 2048];
            let n = nes.take_samples(&mut buf);
            audio.extend_from_slice(&buf[..n]);
        }
        // $81: o teste pede o botão reset depois de pelo menos 100 ms (6 quadros)
        let m = &nes.bus.prg_ram;
        if signed(m) && m[0] == 0x81 && !waiting_reset {
            reset_in = Some(6);
            waiting_reset = true;
        } else if m[0] != 0x81 {
            waiting_reset = false;
        }
        match reset_in {
            Some(0) => {
                reset_in = None;
                nes.reset();
            }
            Some(n) => reset_in = Some(n - 1),
            None => {}
        }
        if let Some(h) = hashes.as_mut() {
            let img = fnv(nes.bus.ppu.fb.as_flattened());
            let ram = fnv(&nes.bus.ram);
            let w: &mut dyn Write = match h {
                Hashes::Stdout => &mut out,
                Hashes::File(file) => file,
            };
            let _ = writeln!(w, "{f} {img:08x} {ram:08x} {}", nes.cpu.cycles);
        }
        if let Some((dir, first, count)) = &dump
            && f >= *first
            && f < first + count
        {
            save_ppm(&nes.bus.ppu.fb, &format!("{dir}/{f:05}.ppm"));
        }
        if nes.cpu.jammed {
            let _ = writeln!(err, "CPU travou (JAM) em ${:04X}", nes.cpu.pc);
            break;
        }
        let m = &nes.bus.prg_ram;
        if signed(m) && m[0] < 0x80 {
            result = i32::from(m[0]);
            break;
        }
        f += 1;
    }
    let secs = t0.elapsed().as_secs_f64();
    if secs > 0.0 {
        let _ = writeln!(
            err,
            "{f} quadros em {secs:.2} s no PC ({:.0} quadros/s)",
            f64::from(f) / secs
        );
    }
    let m = &nes.bus.prg_ram;
    if signed(m) {
        let text = &m[4..];
        let end = text.iter().position(|&b| b == 0).unwrap_or(text.len());
        let _ = out.write_all(&text[..end]);
        let status = if result == 0 {
            "PASSOU"
        } else if m[0] >= 0x80 {
            "(não terminou)"
        } else {
            "FALHOU"
        };
        let _ = writeln!(out, "[resultado $6000 = {} {status}]", m[0]);
    }
    let _ = out.flush();
    if let Some(Hashes::File(mut file)) = hashes {
        let _ = file.flush();
    }
    if sav
        && nes.battery
        && let Err(e) = std::fs::write(&sav_path, nes.bus.prg_ram)
    {
        let _ = writeln!(err, "{}: {e}", sav_path.display());
    }
    if let Some(path) = wav {
        let mut bytes = nes_core::apu::wav_header(audio.len() as u32).to_vec();
        bytes.extend(audio.iter().flat_map(|s| s.to_le_bytes()));
        if let Err(e) = std::fs::write(&path, bytes) {
            let _ = writeln!(err, "{path}: {e}");
        }
    }
    if let Some(path) = out_ppm {
        save_ppm(&nes.bus.ppu.fb, path);
    }
    ExitCode::from(u8::from(result > 0))
}

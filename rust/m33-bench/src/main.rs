//! Mede o núcleo no Cortex-M33 da RP2350: compila o `bench-m33-fw`, roda o ELF num emulador de
//! CPU ARM (Unicorn) e conta, por quadro, as instruções executadas e as leituras na janela da
//! PSRAM. Porte de `firmware/tools/m33_bench.py`.
//!
//! uso: m33-bench rom.nes [aquecer=700] [medir=10] [botao:quadro:dur ...] [--elf arquivo]
//!
//! O aquecimento roda sem contagem (rápido) até a parte jogável; depois mede quadro a quadro.
//! Estimativa de ciclos: instruções + 1 por leitura (LDR leva 2 ciclos no M33) + 2 por desvio no
//! fim de bloco (recarga do pipeline). É um teto: nem todo desvio é tomado.
//! `--elf` mede outro binário com a mesma interface (por exemplo o do C, para comparar).
//! `--blocks` lista os 20 blocos que mais gastaram instruções (para otimizar).

use object::{Object, ObjectSection, ObjectSymbol};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::rc::Rc;
use unicorn_engine::{Arch, HookType, Mode, Prot, RegisterARM, Unicorn};

const FLASH: u64 = 0x1000_0000;
const RAM: u64 = 0x2000_0000;
const PSRAM: u64 = 0x1100_0000;
const STOP: u64 = 0x1FFF_F000;
const BUTTONS: [&str; 8] = ["a", "b", "select", "start", "up", "down", "left", "right"];

/// Quantas instruções Thumb há no bloco e quantas são leituras de memória (LDR*, LDM, POP,
/// TBB/TBH), como a contagem por mnemônico do capstone na versão em Python.
fn decode_block(code: &[u8]) -> (u32, u32) {
    let hw = |i: usize| u16::from_le_bytes([code[i], code[i + 1]]);
    let (mut n, mut loads, mut i) = (0, 0, 0);
    while i + 1 < code.len() {
        let a = hw(i);
        let wide = matches!(a >> 11, 0b11101..=0b11111);
        let load = if wide && i + 3 < code.len() {
            let b = hw(i + 2);
            let ldm = a & 0xFE40 == 0xE800 && a & 0x0010 != 0; // LDM/POP.W
            let tb = a & 0xFFF0 == 0xE8D0 && b & 0xFFE0 == 0xF000; // TBB/TBH
            let dual = a & 0xFE40 == 0xE840 && a & 0x0010 != 0; // LDRD/LDREX
            let single = a & 0xFE10 == 0xF810; // LDR/LDRB/LDRH/LDRSB/LDRSH
            ldm || tb || dual || single
        } else {
            a >> 11 == 0b01001 // LDR literal
                || (a >> 12 == 0b0101 && (a >> 9) & 7 >= 3) // LDR*/LDRS* (registrador)
                || (a >> 13 == 0b011 && a & 0x0800 != 0) // LDR/LDRB imediato
                || (a >> 12 == 0b1000 && a & 0x0800 != 0) // LDRH imediato
                || (a >> 12 == 0b1001 && a & 0x0800 != 0) // LDR relativo a SP
                || a & 0xFE00 == 0xBC00 // POP
                || a >> 11 == 0b11001 // LDM
        };
        n += 1;
        loads += u32::from(load);
        i += if wide { 4 } else { 2 };
    }
    (n, loads)
}

fn press(args: &[String]) -> Result<Vec<(u32, u32, u32)>, String> {
    args.iter()
        .map(|a| {
            let mut p = a.split(':');
            let name = p.next().unwrap_or_default();
            let bit = BUTTONS
                .iter()
                .position(|&b| b == name)
                .ok_or(format!("botão desconhecido: {a}"))?;
            let start: u32 = p
                .next()
                .and_then(|s| s.parse().ok())
                .ok_or(format!("quadro inválido: {a}"))?;
            let dur: u32 = p
                .next()
                .map_or(Some(1), |s| s.parse().ok())
                .ok_or(format!("duração inválida: {a}"))?;
            Ok((1 << bit, start, start + dur))
        })
        .collect()
}

/// Compila o firmware de medição em Rust e devolve o ELF.
fn build_fw() -> Result<PathBuf, String> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bench-m33-fw");
    let st = Command::new("cargo")
        .args(["build", "--release", "-q"])
        .current_dir(&dir)
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    if !st.success() {
        return Err("o firmware de medição não compilou".into());
    }
    Ok(dir.join("target/thumbv8m.main-none-eabihf/release/bench-m33-fw"))
}

#[derive(Default, Clone, Copy)]
struct Stats {
    ins: u64,
    loads: u64,
    blocks: u64,
    psram: u64,
}

fn run(args: &[String]) -> Result<(), String> {
    let mut rest: Vec<String> = Vec::new();
    let mut elf_path = None;
    let mut blocks = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--press" => {}
            "--elf" => elf_path = it.next().map(PathBuf::from),
            "--blocks" => blocks = true,
            _ => rest.push(a.clone()),
        }
    }
    let rom_path = rest.first().ok_or(
        "uso: m33-bench rom.nes [aquecer=700] [medir=10] [botao:quadro:dur ...] [--elf arquivo]",
    )?;
    let nums: Vec<u32> = rest[1..]
        .iter()
        .filter(|a| !a.contains(':'))
        .filter_map(|a| a.parse().ok())
        .collect();
    let warm = nums.first().copied().unwrap_or(700);
    let meas = nums.get(1).copied().unwrap_or(10).max(1);
    let presses = press(
        &rest[1..]
            .iter()
            .filter(|a| a.contains(':'))
            .cloned()
            .collect::<Vec<_>>(),
    )?;
    let rom = std::fs::read(rom_path).map_err(|e| format!("{rom_path}: {e}"))?;
    let elf_path = match elf_path {
        Some(p) => p,
        None => build_fw()?,
    };
    let elf_bytes = std::fs::read(&elf_path).map_err(|e| format!("{}: {e}", elf_path.display()))?;
    let elf = object::File::parse(&*elf_bytes).map_err(|e| e.to_string())?;

    let sym: HashMap<String, u64> = elf
        .symbols()
        .filter_map(|s| Some((s.name().ok()?.to_string(), s.address())))
        .filter(|(n, _)| !n.is_empty())
        .collect();
    let addr = |n: &str| {
        sym.get(n)
            .copied()
            .ok_or(format!("símbolo {n} não está no ELF"))
    };
    let mut funcs: Vec<(u64, String)> = sym
        .iter()
        .filter(|(n, a)| (FLASH..FLASH + (16 << 20)).contains(*a) && !n.starts_with('$'))
        .map(|(n, a)| (*a & !1, format!("{:#}", rustc_demangle::demangle(n))))
        .collect();
    funcs.sort();

    let e = |x: unicorn_engine::uc_error| format!("unicorn: {x:?}");
    let mut uc = Unicorn::new(Arch::ARM, Mode::THUMB | Mode::MCLASS).map_err(e)?;
    uc.mem_map(FLASH, 16 << 20, Prot::ALL).map_err(e)?;
    uc.mem_map(PSRAM, 8 << 20, Prot::ALL).map_err(e)?;
    uc.mem_map(RAM, 1 << 20, Prot::ALL).map_err(e)?;
    uc.mem_map(STOP, 0x1000, Prot::ALL).map_err(e)?;
    for s in elf.sections() {
        let a = s.address();
        let in_mem =
            (FLASH..FLASH + (16 << 20)).contains(&a) || (RAM..RAM + (1 << 20)).contains(&a);
        if in_mem
            && s.kind() != object::SectionKind::UninitializedData
            && let Ok(data) = s.data()
            && !data.is_empty()
        {
            uc.mem_write(a, data).map_err(e)?;
        }
    }
    uc.mem_write(PSRAM, &rom).map_err(e)?;
    uc.mem_write(addr("rom_ptr")?, &(PSRAM as u32).to_le_bytes())
        .map_err(e)?;
    uc.mem_write(addr("rom_size")?, &(rom.len() as u32).to_le_bytes())
        .map_err(e)?;
    let (init, frames) = (addr("bench_init")?, addr("bench_frames")?);

    let call = |uc: &mut Unicorn<()>, f: u64, a0: u64, a1: u64| -> Result<u64, String> {
        uc.reg_write(RegisterARM::R0, a0).map_err(e)?;
        uc.reg_write(RegisterARM::R1, a1).map_err(e)?;
        uc.reg_write(RegisterARM::SP, RAM + (1 << 20) - 16)
            .map_err(e)?;
        uc.reg_write(RegisterARM::LR, STOP | 1).map_err(e)?;
        uc.emu_start(f | 1, STOP, 0, 0).map_err(e)?;
        uc.reg_read(RegisterARM::R0).map_err(e)
    };
    if call(&mut uc, init, 0, 0)? as u32 != 0 {
        return Err("ROM não suportada".into());
    }
    let pad_at = |f: u32| {
        presses
            .iter()
            .filter(|(_, a, b)| (*a..*b).contains(&f))
            .fold(0, |m, (p, _, _)| m | p)
    };
    for f in 0..warm {
        call(&mut uc, frames, 1, u64::from(pad_at(f)))?;
    }

    let st = Rc::new(RefCell::new(Stats::default()));
    let per_func: Rc<RefCell<HashMap<usize, u64>>> = Rc::default();
    let per_block: Rc<RefCell<HashMap<u64, u64>>> = Rc::default();
    let mut cache: HashMap<u64, (u32, u32, usize)> = HashMap::new();
    uc.ctl_flush_tb().map_err(e)?; // blocos traduzidos no aquecimento não passariam pelos ganchos
    {
        let (st, per_func, per_block) = (st.clone(), per_func.clone(), per_block.clone());
        let starts: Vec<u64> = funcs.iter().map(|f| f.0).collect();
        uc.add_block_hook(1, 0, move |uc, a, size| {
            let &mut (n, l, fi) = cache.entry(a).or_insert_with(|| {
                let code = uc.mem_read_as_vec(a, size as usize).unwrap_or_default();
                let (n, l) = decode_block(&code);
                (n, l, starts.partition_point(|&f| f <= a).saturating_sub(1))
            });
            let mut s = st.borrow_mut();
            s.ins += u64::from(n);
            s.loads += u64::from(l);
            s.blocks += 1;
            *per_func.borrow_mut().entry(fi).or_default() += u64::from(n);
            if blocks {
                *per_block.borrow_mut().entry(a).or_default() += u64::from(n);
            }
        })
        .map_err(e)?;
    }
    {
        let st = st.clone();
        uc.add_mem_hook(
            HookType::MEM_READ,
            PSRAM,
            PSRAM + (8 << 20) - 1,
            move |_, _, _, _, _| {
                st.borrow_mut().psram += 1;
                true
            },
        )
        .map_err(e)?;
    }
    let mut total = Stats::default();
    for f in warm..warm + meas {
        *st.borrow_mut() = Stats::default();
        call(&mut uc, frames, 1, u64::from(pad_at(f)))?;
        let s = *st.borrow();
        total.ins += s.ins;
        total.loads += s.loads;
        total.blocks += s.blocks;
        total.psram += s.psram;
    }
    let m = f64::from(meas);
    let ins = total.ins as f64 / m;
    let hi = ins + (total.loads as f64 + 2.0 * total.blocks as f64) / m;
    let name = Path::new(rom_path)
        .file_name()
        .unwrap_or_default()
        .to_string_lossy();
    println!("{name}: média de {meas} quadros a partir do {warm}");
    println!(
        "  instruções M33 por quadro: {:.2} M   (60 qps = {:.0} M instr/s)",
        ins / 1e6,
        ins * 60.0 / 1e6
    );
    println!(
        "  ciclos estimados por quadro: {:.2}-{:.2} M  -> {:.0}-{:.0} MHz para 60 qps",
        ins / 1e6,
        hi / 1e6,
        ins * 60.0 / 1e6,
        hi * 60.0 / 1e6
    );
    println!(
        "  leituras na PSRAM por quadro: {:.1} mil",
        total.psram as f64 / m / 1e3
    );
    let pf = per_func.borrow();
    let sum: u64 = pf.values().sum::<u64>().max(1);
    let mut v: Vec<_> = pf.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1));
    for (fi, n) in v.into_iter().take(14) {
        let full = funcs.get(*fi).map_or("?", |f| f.1.as_str());
        // os dois últimos pedaços do caminho (Tipo::funcao) bastam para ler
        let mut parts: Vec<&str> = full.rsplit("::").take(2).collect();
        parts.reverse();
        println!(
            "    {:32} {:5.1}%",
            parts.join("::"),
            100.0 * *n as f64 / sum as f64
        );
    }
    if blocks {
        let pb = per_block.borrow();
        let mut v: Vec<_> = pb.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        for (a, n) in v.into_iter().take(20) {
            println!("    bloco {a:08x} {:6.0} instr/quadro", *n as f64 / m);
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::decode_block;

    #[test]
    fn classifies_thumb_loads() {
        // ldr r0,[r1]; adds r0,#1; pop {r4,pc}; ldr.w r0,[r1,#4]; ldrb r2,[r3,r4]; str r0,[r1]
        let code = [
            0x08, 0x68, 0x01, 0x30, 0x10, 0xBD, 0xD1, 0xF8, 0x04, 0x00, 0x1A, 0x5D, 0x08, 0x60,
        ];
        assert_eq!(decode_block(&code), (6, 4));
    }
}

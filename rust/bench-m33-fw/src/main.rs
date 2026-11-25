//! Firmware mínimo para medir o núcleo no Cortex-M33: o `m33-bench` carrega este binário num
//! emulador de CPU ARM, escreve `rom_ptr`/`rom_size`, chama `bench_init` e `bench_frames` e
//! conta as instruções. Porte de `firmware/bench/m33_bench.c`.
//!
//! Aqui o `unsafe` é inevitável e fica só neste arquivo (ADR-002): a ROM chega por um ponteiro
//! para a janela da PSRAM, e o estado do console vive num `static` entre as chamadas do
//! emulador.
#![no_std]
#![no_main]

use core::ptr::{addr_of, addr_of_mut, read_volatile};
use nes_core::Nes;

/// Preenchido pelo `m33-bench` (nomes iguais aos do C): a ROM fica na janela da PSRAM (0x11000000).
#[unsafe(no_mangle)]
static mut rom_ptr: u32 = 0;
#[unsafe(no_mangle)]
static mut rom_size: u32 = 0;

static mut NES: Option<Nes<'static>> = None;

#[unsafe(no_mangle)]
pub extern "C" fn bench_init() -> i32 {
    // SAFETY: um só núcleo e nenhuma interrupção; o emulador escreveu as duas variáveis antes da
    // chamada (por isso a leitura volátil) e a ROM fica mapeada e intocada até o fim da medição.
    let rom: &'static [u8] = unsafe {
        let ptr = read_volatile(addr_of!(rom_ptr)) as usize as *const u8;
        let len = read_volatile(addr_of!(rom_size)) as usize;
        core::slice::from_raw_parts(ptr, len)
    };
    match Nes::new(rom) {
        Ok(nes) => {
            // SAFETY: um só núcleo, nenhuma interrupção e nenhuma outra referência a NES viva.
            unsafe { *addr_of_mut!(NES) = Some(nes) };
            0
        }
        Err(e) => e.code(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn bench_frames(n: u32, pad: u32) {
    // SAFETY: um só núcleo, nenhuma interrupção e nenhuma outra referência a NES viva.
    let nes = unsafe { (*addr_of_mut!(NES)).as_mut() };
    if let Some(nes) = nes {
        nes.bus.pad[0] = pad as u8;
        for _ in 0..n {
            nes.run_frame();
        }
    }
}

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

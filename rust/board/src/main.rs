//! Firmware do pocketMiSTer para a Waveshare RP2350-PiZero (RP2350B, 16 MB de flash).
//!
//! - Núcleo 0: menu e emulação (o `nes-core`, o mesmo dos testes do PC).
//! - Núcleo 1: vídeo DVI 640x480 pelo PIO0 (`video.rs`).
//! - Jogos: pacote gravado na flash a 8 MB (`cargo xtask pack`, depois
//!   `picotool load -o 0x10800000 pacote.bin`).
//! - Controles SNES nos pinos do pico-infonesPlus (`pads.rs`).
//!
//! Relógio de 252 MHz (10 x o pixel do 640x480). Ainda sem som (o DVI não leva áudio; HDMI com
//! áudio é passo seguinte), sem cartão SD e sem PSRAM: ver `rust/MAP.md`.
#![no_std]
#![no_main]

mod menu;
mod pads;
mod video;

use core::ptr::addr_of_mut;
use embedded_hal::digital::{InputPin, OutputPin};
use nes_core::Nes;
use nes_core::ppu::{H, W};
use panic_halt as _;
use rp235x_hal as hal;
use rp235x_hal::fugit::RateExtU32;
use rp235x_hal::multicore::{Multicore, Stack};

/// Bloco que a ROM de boot da RP2350 procura para rodar o programa.
#[unsafe(link_section = ".start_block")]
#[used]
pub static IMAGE_DEF: hal::block::ImageDef = hal::block::ImageDef::secure_exe();

const XTAL_HZ: u32 = 12_000_000;
/// onde o pacote de jogos fica na flash mapeada (8 MB depois do começo)
const PACK_ADDR: usize = 0x1080_0000;
const PACK_MAX: usize = 8 << 20;

/// pilha do núcleo 1 (o tamanho é em palavras: 2048 = 8 KB)
static CORE1_STACK: Stack<2048> = Stack::new();
/// o console fica num `static` (110 KB: grande demais para a pilha)
static mut NES: Option<Nes<'static>> = None;
static mut MENU_FB: [[u8; W]; H] = [[0; W]; H];

type Delay = hal::Timer<hal::timer::CopyableTimer0>;

/// Trava com o erro de inicialização (sem tela ainda não há onde mostrar).
fn halt() -> ! {
    loop {
        cortex_m::asm::wfi();
    }
}

#[hal::entry]
fn main() -> ! {
    let Some(mut p) = hal::pac::Peripherals::take() else {
        halt()
    };
    let mut watchdog = hal::Watchdog::new(p.WATCHDOG);
    let Ok(xosc) = hal::xosc::setup_xosc_blocking(p.XOSC, XTAL_HZ.Hz()) else {
        halt()
    };
    watchdog.enable_tick_generation((XTAL_HZ / 1_000_000) as u16);
    let mut clocks = hal::clocks::ClocksManager::new(p.CLOCKS);
    // 12 MHz x 126 = 1512 MHz / 6 = 252 MHz
    let sys = hal::pll::PLLConfig {
        vco_freq: 1512.MHz(),
        refdiv: 1,
        post_div1: 6,
        post_div2: 1,
    };
    let Ok(pll_sys) = hal::pll::setup_pll_blocking(
        p.PLL_SYS,
        xosc.operating_frequency(),
        sys,
        &mut clocks,
        &mut p.RESETS,
    ) else {
        halt()
    };
    let Ok(pll_usb) = hal::pll::setup_pll_blocking(
        p.PLL_USB,
        xosc.operating_frequency(),
        hal::pll::common_configs::PLL_USB_48MHZ,
        &mut clocks,
        &mut p.RESETS,
    ) else {
        halt()
    };
    if clocks.init_default(&xosc, &pll_sys, &pll_usb).is_err() {
        halt()
    }
    let mut timer = hal::Timer::new_timer0(p.TIMER0, &mut p.RESETS, &clocks);
    let mut sio = hal::Sio::new(p.SIO);
    let pins = hal::gpio::Pins::new(p.IO_BANK0, p.PADS_BANK0, sio.gpio_bank0, &mut p.RESETS);

    let mut pad1 = pads::Pad {
        clock: pins.gpio5.into_push_pull_output(),
        latch: pins.gpio9.into_push_pull_output(),
        data: pins.gpio6.into_pull_up_input(),
    };
    let mut pad2 = pads::Pad {
        clock: pins.gpio10.into_push_pull_output(),
        latch: pins.gpio11.into_push_pull_output(),
        data: pins.gpio12.into_pull_up_input(),
    };

    // núcleo 1: vídeo
    let mut mc = Multicore::new(&mut p.PSM, &mut p.PPB, &mut sio.fifo);
    let cores = mc.cores();
    if let Some(stack) = CORE1_STACK.take() {
        let _ = cores[1].spawn(stack, || video::run());
    }

    // SAFETY: a região do pacote é flash mapeada (XIP), só de leitura, e fica mapeada sempre.
    let flash: &'static [u8] =
        unsafe { core::slice::from_raw_parts(PACK_ADDR as *const u8, PACK_MAX) };
    let pack = nes_core::pack::total_len(flash).map_or(&flash[..0], |n| &flash[..n.min(PACK_MAX)]);

    loop {
        let i = choose(pack, &mut pad1, &mut timer);
        let Some((_, rom)) = nes_core::pack::entry(pack, i) else {
            continue;
        };
        // SAFETY: NES só é usado aqui, no núcleo 0; o anterior é descartado antes do novo.
        let nes = unsafe {
            let slot = &mut *addr_of_mut!(NES);
            *slot = None;
            match Nes::new(rom) {
                Ok(n) => slot.insert(n),
                Err(_) => continue,
            }
        };
        play(nes, &mut pad1, &mut pad2, &mut timer);
    }
}

/// Menu até o jogador escolher; devolve o índice do jogo no pacote.
fn choose(
    pack: &[u8],
    pad: &mut pads::Pad<impl OutputPin, impl OutputPin, impl InputPin>,
    timer: &mut Delay,
) -> usize {
    let mut names: [&str; 64] = [""; 64];
    let n = nes_core::pack::count(pack).unwrap_or(0).min(names.len());
    for (k, slot) in names.iter_mut().enumerate().take(n) {
        *slot = nes_core::pack::entry(pack, k).map_or("?", |e| e.0);
    }
    // SAFETY: MENU_FB só é usado pelo núcleo 0, aqui.
    let fb = unsafe { &mut *addr_of_mut!(MENU_FB) };
    if n == 0 {
        menu::clear(fb);
        menu::text(fb, 1, 1, "pocketMiSTer", 0x2C, 0x0F);
        menu::text(fb, 1, 3, "nenhum jogo na flash", 0x30, 0x0F);
        menu::text(fb, 1, 5, "cargo xtask pack", 0x00, 0x0F);
        loop {
            video::copy_frame(fb);
        }
    }
    let (mut sel, mut top, mut prev, mut hold) = (0usize, 0usize, 0u16, 0u32);
    loop {
        let s = pad.read(timer);
        let fresh = s & !prev;
        // segurar repete a cada 6 quadros, depois de 20
        hold = if s & (pads::snes::UP | pads::snes::DOWN) != 0 && s == prev {
            hold + 1
        } else {
            0
        };
        let moving = fresh | if hold > 20 && hold % 6 == 0 { s } else { 0 };
        if moving & pads::snes::DOWN != 0 && sel + 1 < n {
            sel += 1;
        }
        if moving & pads::snes::UP != 0 && sel > 0 {
            sel -= 1;
        }
        if fresh & (pads::snes::A | pads::snes::START) != 0 {
            return sel;
        }
        if sel < top {
            top = sel;
        } else if sel >= top + menu::LIST_ROWS {
            top = sel + 1 - menu::LIST_ROWS;
        }
        prev = s;
        menu::draw(fb, &names[..n], sel, top);
        video::copy_frame(fb);
    }
}

/// Roda o jogo até Select + Start ficarem apertados por 1 s no controle 1.
fn play(
    nes: &mut Nes<'static>,
    pad1: &mut pads::Pad<impl OutputPin, impl OutputPin, impl InputPin>,
    pad2: &mut pads::Pad<impl OutputPin, impl OutputPin, impl InputPin>,
    timer: &mut Delay,
) {
    let mut exit_hold = 0;
    let mut audio = [0i16; 2048];
    let combo = pads::snes::SELECT | pads::snes::START;
    loop {
        let s1 = pad1.read(timer);
        let s2 = pad2.read(timer);
        exit_hold = if s1 & combo == combo {
            exit_hold + 1
        } else {
            0
        };
        if exit_hold > 60 {
            return;
        }
        nes.bus.pad = [pads::to_nes(s1), pads::to_nes(s2)];
        nes.run_frame();
        // sem saída de som ainda: as amostras são descartadas para o buffer não encher
        let _ = nes.take_samples(&mut audio);
        video::copy_frame(&nes.bus.ppu.fb);
    }
}

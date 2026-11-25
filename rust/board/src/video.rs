//! Vídeo DVI 640x480 a 60 Hz pelo PIO0 nos GPIO 32-39 da Waveshare RP2350-PiZero (pares TMDS em
//! 36/37, 34/35, 32/33 e o clock em 38/39, como o pico-infonesPlus). Roda inteiro no núcleo 1.
//!
//! - Relógio do sistema = 252 MHz = 10 x 25,2 MHz: o PIO solta um bit TMDS por ciclo.
//! - Cada pista tem uma máquina do PIO que faz `out pins, 2`: cada palavra do FIFO leva um
//!   símbolo TMDS de 10 bits já expandido em pares (bit, bit negado) = 20 bits, um por pixel.
//! - Só se usam símbolos TMDS com 5 uns e 5 zeros (as cores são levadas ao valor mais próximo que
//!   tem um): o sinal fica balanceado sem contar disparidade, e cada pixel vira uma consulta de
//!   tabela.
//! - A imagem do NES (256x240) sai dobrada (512x480) no meio da tela, com bordas pretas.
//! - Por pista, dois canais de DMA se revezam linha a linha (um encadeia no outro); quando um
//!   termina, a interrupção aponta-o para a linha depois da seguinte. Uma linha inteira de folga.
//! - O núcleo 1 codifica cada linha do NES num de dois buffers, duas linhas de tela à frente.
//!
//! Tudo aqui é acesso direto a registrador: o `unsafe` fica isolado neste arquivo, cada bloco com
//! o seu `SAFETY`.

use core::sync::atomic::{AtomicU32, Ordering};
use nes_core::ppu::{H, PALETTE, W};
use rp235x_hal::pac;
use rp235x_hal::pac::interrupt;

/// pontos por linha e linhas por quadro (VESA 640x480@60, pixel de 25,2 MHz)
const H_ACTIVE: usize = 640;
const H_FRONT: usize = 16;
const H_SYNC: usize = 96;
const H_TOTAL: usize = 800;
const V_ACTIVE: u32 = 480;
const V_FRONT: u32 = 10;
const V_SYNC: u32 = 2;
const V_TOTAL: u32 = 525;
/// borda preta de cada lado da imagem dobrada (640 - 512) / 2
const BORDER: usize = 64;

/// pinos (GPIO) das pistas 0 (azul, sincronismos), 1 (verde), 2 (vermelho) e do clock
const PIN_LANE: [u8; 3] = [36, 34, 32];
const PIN_CLOCK: u8 = 38;
/// o PIO0 enxerga os GPIO 16-47 como 0-31
const GPIO_BASE: u8 = 16;

/// símbolos de controle (C1 C0) = 00, 01, 10, 11
const CTRL: [u16; 4] = [
    0b11_0101_0100,
    0b00_1010_1011,
    0b01_0101_0100,
    0b10_1010_1011,
];

/// A imagem mostrada (índices de cor do NES). O núcleo 0 escreve só entre as linhas 480 e 519 da
/// tela (ver [`copy_frame`]); o núcleo 1 só lê fora dessa janela.
pub static mut DISPLAY: [[u8; W]; H] = [[0; W]; H];
/// buffers de linha: [buffer][pista][palavra]
static mut LINES: [[[u32; H_TOTAL]; 3]; 2] = [[[0; H_TOTAL]; 3]; 2];
/// linha de apagamento vertical e linha de sincronismo vertical, por pista
static mut BLANK: [[u32; H_TOTAL]; 3] = [[0; H_TOTAL]; 3];
static mut VSYNC: [[u32; H_TOTAL]; 3] = [[0; H_TOTAL]; 3];
/// cor do NES (64) -> palavra de cada pista
static mut COLOR: [[u32; 3]; 64] = [[0; 3]; 64];

/// linha de tela (contagem absoluta, sem voltar a zero) que está saindo agora
static LINE: AtomicU32 = AtomicU32::new(0);
/// linha absoluta que cada canal de DMA vai mandar na próxima vez
static mut NEXT: [u32; 6] = [0; 6];

/// Codificação TMDS de `d` com disparidade 0; `Some` só se o símbolo tem 5 uns.
const fn tmds_balanced(d: u8) -> Option<u16> {
    let n1 = d.count_ones();
    let xnor = n1 > 4 || (n1 == 4 && d & 1 == 0);
    let mut q: u16 = (d & 1) as u16;
    let mut i = 1;
    while i < 8 {
        let prev = (q >> (i - 1)) & 1;
        let di = ((d >> i) & 1) as u16;
        let b = if xnor { !(prev ^ di) & 1 } else { prev ^ di };
        q |= b << i;
        i += 1;
    }
    let q8: u16 = if xnor { 0 } else { 1 };
    let low = q & 0xFF;
    let data = if q8 == 1 { low } else { !low & 0xFF };
    let sym = data | (q8 << 8) | ((1 - q8) << 9);
    if sym.count_ones() == 5 {
        Some(sym)
    } else {
        None
    }
}

/// Símbolo balanceado do valor mais próximo de `v`.
const fn tmds_near(v: u8) -> u16 {
    let mut dist = 0u16;
    loop {
        if v as u16 >= dist
            && let Some(s) = tmds_balanced((v as u16 - dist) as u8)
        {
            return s;
        }
        if v as u16 + dist <= 255
            && let Some(s) = tmds_balanced((v as u16 + dist) as u8)
        {
            return s;
        }
        dist += 1;
    }
}

/// Símbolo de 10 bits -> 20 bits em pares (bit, bit negado), o primeiro a sair no bit 0.
const fn expand(sym: u16) -> u32 {
    let mut w = 0u32;
    let mut i = 0;
    while i < 10 {
        let b = ((sym >> i) & 1) as u32;
        w |= (b << (2 * i)) | ((1 - b) << (2 * i + 1));
        i += 1;
    }
    w
}

/// Controle da pista `lane` com os sincronismos no nível dado (true = pulso, polaridade negativa).
fn ctrl(lane: usize, hsync: bool, vsync: bool) -> u32 {
    if lane == 0 {
        let c0 = u16::from(!hsync);
        let c1 = u16::from(!vsync);
        expand(CTRL[usize::from((c1 << 1) | c0)])
    } else {
        expand(CTRL[0])
    }
}

/// Fim da linha (apagamento horizontal) a partir da posição 640.
fn hblank(line: &mut [u32; H_TOTAL], lane: usize, vsync: bool) {
    for (i, w) in line[H_ACTIVE..].iter_mut().enumerate() {
        let in_sync = (H_FRONT..H_FRONT + H_SYNC).contains(&i);
        *w = ctrl(lane, in_sync, vsync);
    }
}

/// Monta as tabelas e as linhas fixas.
fn build_tables() {
    let black = expand(tmds_near(0));
    // SAFETY: só o núcleo 1 mexe nestas tabelas, e isto roda antes do vídeo ligar.
    unsafe {
        let color = &mut *core::ptr::addr_of_mut!(COLOR);
        for (c, rgb) in PALETTE.iter().enumerate() {
            // pista 0 = azul, 1 = verde, 2 = vermelho
            color[c] = [
                expand(tmds_near(rgb[2])),
                expand(tmds_near(rgb[1])),
                expand(tmds_near(rgb[0])),
            ];
        }
        let lines = &mut *core::ptr::addr_of_mut!(LINES);
        let blank = &mut *core::ptr::addr_of_mut!(BLANK);
        let vsync = &mut *core::ptr::addr_of_mut!(VSYNC);
        for lane in 0..3 {
            for buf in lines.iter_mut() {
                buf[lane][..H_ACTIVE].fill(black);
                hblank(&mut buf[lane], lane, false);
            }
            blank[lane][..H_ACTIVE].fill(ctrl(lane, false, false));
            hblank(&mut blank[lane], lane, false);
            vsync[lane][..H_ACTIVE].fill(ctrl(lane, false, true));
            hblank(&mut vsync[lane], lane, true);
        }
    }
}

/// Endereço da linha absoluta `abs` para a pista `lane`.
fn line_addr(abs: u32, lane: usize) -> u32 {
    let l = abs % V_TOTAL;
    // SAFETY: só se tira o endereço; o conteúdo é escrito pelo núcleo 1 fora da janela de leitura.
    unsafe {
        if l < V_ACTIVE {
            core::ptr::addr_of!(LINES[((l / 2) & 1) as usize][lane]) as u32
        } else if (V_ACTIVE + V_FRONT..V_ACTIVE + V_FRONT + V_SYNC).contains(&l) {
            core::ptr::addr_of!(VSYNC[lane]) as u32
        } else {
            core::ptr::addr_of!(BLANK[lane]) as u32
        }
    }
}

/// Codifica a linha `row` do NES no buffer dela.
fn encode_row(row: usize) {
    // SAFETY: o buffer `row & 1` não está saindo agora (ver `run`), e DISPLAY só é escrito pelo
    // núcleo 0 fora da janela em que o núcleo 1 lê.
    unsafe {
        let color = &*core::ptr::addr_of!(COLOR);
        let src = &(*core::ptr::addr_of!(DISPLAY))[row];
        let buf = &mut (*core::ptr::addr_of_mut!(LINES))[row & 1];
        for (x, &c) in src.iter().enumerate() {
            let w = color[usize::from(c & 0x3F)];
            let at = BORDER + 2 * x;
            for lane in 0..3 {
                buf[lane][at] = w[lane];
                buf[lane][at + 1] = w[lane];
            }
        }
    }
}

/// Configura PIO0 e DMA e liga o vídeo. Roda no núcleo 1.
fn start() {
    // SAFETY: o núcleo 1 é dono do PIO0, dos canais de DMA 0-5 e dos pinos 32-39 (o núcleo 0 não
    // os toca); os registradores são escritos como o datasheet manda, com o PIO e o DMA parados.
    unsafe {
        let p = pac::Peripherals::steal();
        let resets = &p.RESETS;
        resets
            .reset()
            .modify(|_, w| w.pio0().clear_bit().dma().clear_bit());
        while resets.reset_done().read().pio0().bit_is_clear()
            || resets.reset_done().read().dma().bit_is_clear()
        {}

        // pinos 32-39: função PIO0, saída rápida e forte
        for gpio in 32..40usize {
            p.PADS_BANK0.gpio(gpio).write(|w| {
                w.drive()
                    ._12m_a()
                    .slewfast()
                    .set_bit()
                    .ie()
                    .clear_bit()
                    .iso()
                    .clear_bit()
            });
            p.IO_BANK0
                .gpio(gpio)
                .gpio_ctrl()
                .write(|w| w.funcsel().bits(6)); // F6 = PIO0
        }

        let pio = &p.PIO0;
        pio.ctrl().write(|w| w.bits(0));
        pio.gpiobase().write(|w| w.bits(u32::from(GPIO_BASE)));
        // programa 0 (pistas): `out pins, 2`; programa 1-2 (clock): `set pins, 01 [4]`, `set pins, 10 [4]`
        pio.instr_mem(0).write(|w| w.bits(0x6002));
        pio.instr_mem(1).write(|w| w.bits(0xE401));
        pio.instr_mem(2).write(|w| w.bits(0xE402));
        #[expect(
            clippy::needless_range_loop,
            reason = "sm é o número da máquina do PIO"
        )]
        for sm in 0..4usize {
            let s = pio.sm(sm);
            s.sm_clkdiv().write(|w| w.bits(1 << 16));
            let (pin, out, wrap_bottom, wrap_top, start) = if sm < 3 {
                (PIN_LANE[sm] - GPIO_BASE, true, 0u32, 0u32, 0u32)
            } else {
                (PIN_CLOCK - GPIO_BASE, false, 1, 2, 1)
            };
            s.sm_execctrl()
                .write(|w| w.bits((wrap_top << 12) | (wrap_bottom << 7)));
            // saída para a direita, autopull com 20 bits, FIFO de transmissão dobrado
            s.sm_shiftctrl()
                .write(|w| w.bits((1 << 30) | (20 << 25) | (1 << 19) | (1 << 17)));
            let pin = u32::from(pin);
            let pinctrl = if out {
                (2 << 20) | pin
            } else {
                (2 << 26) | (pin << 5)
            };
            // direção dos 2 pinos: `set pindirs, 3` com a base de SET neles
            s.sm_pinctrl().write(|w| w.bits((2 << 26) | (pin << 5)));
            s.sm_instr().write(|w| w.bits(0xE083));
            s.sm_pinctrl().write(|w| w.bits(pinctrl));
            s.sm_instr().write(|w| w.bits(start)); // jmp para o começo do programa
        }

        // DMA: canais 2*pista e 2*pista+1 se revezam; cada um encadeia no outro
        let dma = &p.DMA;
        for lane in 0..3usize {
            for k in 0..2usize {
                let ch = lane * 2 + k;
                let partner = lane * 2 + (1 - k);
                let c = dma.ch(ch);
                c.ch_write_addr()
                    .write(|w| w.bits(pio.txf(lane).as_ptr() as u32));
                c.ch_trans_count().write(|w| w.bits(H_TOTAL as u32));
                NEXT[ch] = k as u32;
                c.ch_read_addr()
                    .write(|w| w.bits(line_addr(k as u32, lane)));
                NEXT[ch] = k as u32 + 2;
                c.ch_al1_ctrl().write(|w| {
                    w.en()
                        .set_bit()
                        .high_priority()
                        .set_bit()
                        .data_size()
                        .size_word()
                        .incr_read()
                        .set_bit()
                        .incr_write()
                        .clear_bit()
                        .chain_to()
                        .bits(partner as u8)
                        .treq_sel()
                        .bits(lane as u8) // DREQ_PIO0_TX0 + pista
                });
            }
        }
        dma.inte0().write(|w| w.bits(0b11_1111));
        LINE.store(0, Ordering::Relaxed);
        // dispara os canais da linha 0; os FIFOs enchem antes das máquinas andarem
        dma.multi_chan_trigger().write(|w| w.bits(0b01_0101));
        while (0..3).any(|sm| pio.fstat().read().bits() & (1 << (16 + sm)) == 0) {} // TX cheio
        // liga as 4 máquinas juntas (enable + restart + divisor)
        pio.ctrl().write(|w| w.bits(0xF | (0xF << 4) | (0xF << 8)));
        cortex_m::peripheral::NVIC::unmask(pac::Interrupt::DMA_IRQ_0);
    }
}

#[interrupt]
fn DMA_IRQ_0() {
    // SAFETY: só este tratador (núcleo 1) escreve NEXT e os endereços dos canais parados; um canal
    // que acabou de terminar está parado até o parceiro encadear nele, uma linha inteira depois.
    unsafe {
        let p = pac::Peripherals::steal();
        let dma = &p.DMA;
        let done = dma.ints0().read().bits() & 0b11_1111;
        dma.ints0().write(|w| w.bits(done));
        let next = &mut *core::ptr::addr_of_mut!(NEXT);
        #[expect(clippy::needless_range_loop, reason = "ch é o número do canal de DMA")]
        for ch in 0..6usize {
            if done & (1 << ch) != 0 {
                let lane = ch / 2;
                if lane == 0 {
                    // a linha que este canal acabou de mandar + 1 é a que sai agora
                    LINE.store(next[ch] - 1, Ordering::Release);
                }
                dma.ch(ch)
                    .ch_read_addr()
                    .write(|w| w.bits(line_addr(next[ch], lane)));
                next[ch] += 2;
            }
        }
    }
}

/// Começo do quadro de tela `f` (linha absoluta da primeira linha visível).
fn frame_start(f: u32) -> u32 {
    f * V_TOTAL
}

/// Laço do núcleo 1: liga o vídeo e codifica as linhas do NES à frente da varredura.
pub fn run() -> ! {
    build_tables();
    for row in 0..2 {
        encode_row(row);
    }
    start();
    let mut f = 0u32;
    loop {
        let base = frame_start(f);
        // linhas 2..239: o buffer da linha k fica livre quando a tela passa da linha 2k-2
        for row in 2..H {
            while (LINE
                .load(Ordering::Acquire)
                .wrapping_sub(base + 2 * row as u32 - 2) as i32)
                < 0
            {}
            encode_row(row);
        }
        // linhas 0 e 1 do próximo quadro, no fim do apagamento (o núcleo 0 copia antes)
        let next = frame_start(f + 1);
        while (LINE.load(Ordering::Acquire).wrapping_sub(next - 5) as i32) < 0 {}
        encode_row(0);
        encode_row(1);
        f += 1;
    }
}

/// Espera o começo do apagamento vertical (linha 480) e copia `fb` para a tela. Chamado pelo
/// núcleo 0 uma vez por quadro; a cópia (61 KB) acaba bem antes da linha 520.
pub fn copy_frame(fb: &[[u8; W]; H]) {
    let now = LINE.load(Ordering::Acquire);
    let f = now / V_TOTAL;
    let mut target = f * V_TOTAL + V_ACTIVE;
    if now >= target {
        target += V_TOTAL;
    }
    while (LINE.load(Ordering::Acquire).wrapping_sub(target) as i32) < 0 {}
    // SAFETY: entre as linhas 480 e 519 o núcleo 1 não lê DISPLAY (ver `run`).
    unsafe {
        *core::ptr::addr_of_mut!(DISPLAY) = *fb;
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn balanced_symbols() {
        for v in 0..=255u8 {
            if let Some(s) = super::tmds_balanced(v) {
                assert_eq!(s.count_ones(), 5);
            }
        }
    }
}

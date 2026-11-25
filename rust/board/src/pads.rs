//! Dois controles de SNES por GPIO (pinos do pico-infonesPlus para esta placa): controle 1 com
//! clock 5, data 6, latch 9; controle 2 com clock 10, data 12, latch 11. Leitura uma vez por
//! quadro: pulso de latch de 12 us e 16 bits (dado ativo em 0) na borda do clock.

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::{InputPin, OutputPin};
use nes_core::nes::pad;

/// bits do SNES na ordem em que saem: B Y Select Start Cima Baixo Esquerda Direita A X (L R e 4 fixos)
pub mod snes {
    pub const B: u16 = 1 << 0;
    pub const Y: u16 = 1 << 1;
    pub const SELECT: u16 = 1 << 2;
    pub const START: u16 = 1 << 3;
    pub const UP: u16 = 1 << 4;
    pub const DOWN: u16 = 1 << 5;
    pub const LEFT: u16 = 1 << 6;
    pub const RIGHT: u16 = 1 << 7;
    pub const A: u16 = 1 << 8;
    pub const X: u16 = 1 << 9;
}

pub struct Pad<C, L, D> {
    pub clock: C,
    pub latch: L,
    pub data: D,
}

impl<C: OutputPin, L: OutputPin, D: InputPin> Pad<C, L, D> {
    /// Os 16 bits do controle (1 = apertado). Sem controle ligado o dado fica em 1 (pull-up) e
    /// a leitura dá 0.
    pub fn read(&mut self, delay: &mut impl DelayNs) -> u16 {
        let _ = self.clock.set_high();
        let _ = self.latch.set_high();
        delay.delay_us(12);
        let _ = self.latch.set_low();
        delay.delay_us(6);
        let mut v = 0u16;
        for i in 0..16 {
            if self.data.is_low().unwrap_or(false) {
                v |= 1 << i;
            }
            let _ = self.clock.set_low();
            delay.delay_us(6);
            let _ = self.clock.set_high();
            delay.delay_us(6);
        }
        v
    }
}

/// SNES -> NES: A e X viram A, B e Y viram B; o resto é direto.
pub fn to_nes(s: u16) -> u8 {
    let mut n = 0;
    let map = [
        (snes::A | snes::X, pad::A),
        (snes::B | snes::Y, pad::B),
        (snes::SELECT, pad::SELECT),
        (snes::START, pad::START),
        (snes::UP, pad::UP),
        (snes::DOWN, pad::DOWN),
        (snes::LEFT, pad::LEFT),
        (snes::RIGHT, pad::RIGHT),
    ];
    for (bits, b) in map {
        if s & bits != 0 {
            n |= b;
        }
    }
    n
}

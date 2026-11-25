//! Modos de saída de vídeo e o enquadramento do aplicativo neles.
//!
//! O aplicativo (frontend e jogo) é sempre uma imagem 4:3 de 320x240. Em qualquer modo ele fica
//! cravado no centro, ampliado pela maior escala inteira que cabe, e o resto da tela é preto: em
//! telas mais largas que 4:3 sobram bordas nas laterais; quando a escala inteira não enche a
//! altura (800x600), sobra uma moldura em volta.
//!
//! O vídeo sai pelo PIO a 10 bits TMDS por pixel, com um bit por ciclo: o relógio do sistema é 10
//! vezes o pixel clock. Isso limita os modos ao que a RP2350 aguenta (até uns 400 MHz); 720p
//! (742,5 MHz) fica de fora nesta placa, que liga o HDMI em pinos que o HSTX não alcança.

use crate::canvas::{HEIGHT, WIDTH};

/// Tempos de um modo (pontos e linhas ativos, bordas e sincronismo), como nas tabelas VESA/CEA.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub name: &'static str,
    /// proporção da tela, para mostrar no menu
    pub aspect: &'static str,
    pub h_active: u16,
    pub h_front: u16,
    pub h_sync: u16,
    pub h_back: u16,
    pub v_active: u16,
    pub v_front: u16,
    pub v_sync: u16,
    pub v_back: u16,
    /// pixel clock em kHz
    pub pixel_khz: u32,
    /// sincronismos em nível baixo (polaridade negativa)
    pub negative_sync: bool,
}

impl Mode {
    pub fn h_total(&self) -> u32 {
        u32::from(self.h_active + self.h_front + self.h_sync + self.h_back)
    }
    pub fn v_total(&self) -> u32 {
        u32::from(self.v_active + self.v_front + self.v_sync + self.v_back)
    }
    /// relógio do sistema necessário (10 x pixel clock), em MHz
    pub fn clock_mhz(&self) -> u32 {
        self.pixel_khz * 10 / 1000
    }
    /// quadros por segundo resultantes (x100, para mostrar "60.00")
    pub fn hz_x100(&self) -> u32 {
        (u64::from(self.pixel_khz) * 100_000
            / (u64::from(self.h_total()) * u64::from(self.v_total()))) as u32
    }
}

/// Modos de saída, do mais seguro ao mais exigente para a placa.
pub const MODES: [Mode; 4] = [
    // VESA 640x480@60: o padrão que qualquer TV e monitor aceita
    Mode {
        name: "640x480",
        aspect: "4:3",
        h_active: 640,
        h_front: 16,
        h_sync: 96,
        h_back: 48,
        v_active: 480,
        v_front: 10,
        v_sync: 2,
        v_back: 33,
        pixel_khz: 25_200,
        negative_sync: true,
    },
    // 800x480@60: as telinhas de 5 e 7 polegadas (5:3)
    Mode {
        name: "800x480",
        aspect: "5:3",
        h_active: 800,
        h_front: 40,
        h_sync: 48,
        h_back: 88,
        v_active: 480,
        v_front: 13,
        v_sync: 3,
        v_back: 32,
        pixel_khz: 30_900,
        negative_sync: true,
    },
    // 848x480@60 (VESA CVT): 16:9 com 480 linhas, para TVs widescreen
    Mode {
        name: "848x480",
        aspect: "16:9",
        h_active: 848,
        h_front: 16,
        h_sync: 112,
        h_back: 112,
        v_active: 480,
        v_front: 6,
        v_sync: 8,
        v_back: 23,
        pixel_khz: 33_750,
        negative_sync: false,
    },
    // VESA 800x600@60: 4:3 maior, para monitores (precisa de 400 MHz)
    Mode {
        name: "800x600",
        aspect: "4:3",
        h_active: 800,
        h_front: 40,
        h_sync: 128,
        h_back: 88,
        v_active: 600,
        v_front: 1,
        v_sync: 4,
        v_back: 23,
        pixel_khz: 40_000,
        negative_sync: false,
    },
];

/// Onde o aplicativo 4:3 entra na saída: escala inteira e canto de cima à esquerda.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub scale: u16,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

/// Maior escala inteira em que o aplicativo cabe, centrado.
pub fn frame(m: &Mode) -> Frame {
    let scale = (m.h_active / WIDTH as u16)
        .min(m.v_active / HEIGHT as u16)
        .max(1);
    let (width, height) = (WIDTH as u16 * scale, HEIGHT as u16 * scale);
    Frame {
        scale,
        x: (m.h_active - width) / 2,
        y: (m.v_active - height) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sixty_hz_and_clock() {
        for m in &MODES {
            let hz = m.hz_x100();
            assert!((5950..=6050).contains(&hz), "{}: {hz}", m.name);
            assert!(m.clock_mhz() <= 400, "{}", m.name);
        }
        assert_eq!(MODES[0].clock_mhz(), 252);
    }

    #[test]
    fn always_4x3_centered() {
        for m in &MODES {
            let f = frame(m);
            // 4:3 exato
            assert_eq!(
                u32::from(f.width) * 3,
                u32::from(f.height) * 4,
                "{}",
                m.name
            );
            // centrado: as bordas dos dois lados são iguais
            assert_eq!(f.x * 2 + f.width, m.h_active, "{}", m.name);
            assert_eq!(f.y * 2 + f.height, m.v_active, "{}", m.name);
        }
        // 16:9 com 480 linhas: 640x480 no meio e 104 pixels pretos de cada lado
        assert_eq!(
            frame(&MODES[2]),
            Frame {
                scale: 2,
                x: 104,
                y: 0,
                width: 640,
                height: 480
            }
        );
    }
}

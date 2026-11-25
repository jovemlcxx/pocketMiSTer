//! Núcleo do pocketMiSTer. Sem `std`, sem alocação e sem `unsafe` (ADR-002): o mesmo código roda nos
//! testes do PC e no Cortex-M33 da RP2350.
#![no_std]
#![forbid(unsafe_code)]

pub mod apu;
pub mod cpu;
pub mod mapper;
pub mod nes;
pub mod pack;
pub mod ppu;

pub use cpu::{Bus, Cpu};
pub use mapper::Mapper;
pub use nes::Nes;
pub use ppu::Ppu;

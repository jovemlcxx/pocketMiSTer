//! Frontend do pocketMiSTer sem hardware: catálogo de jogos do cartão SD, navegação e telas no visual
//! do "MiSTer Menu" (ver `tema`), desenhadas num framebuffer de 320x240. O mesmo código roda na placa e no
//! simulador do PC (`cargo xtask simulate`), que grava as telas em PNG. Escopo em
//! `docs/frontend-and-sd.md`.
#![no_std]
#![forbid(unsafe_code)]

pub mod background;
pub mod canvas;
pub mod catalog;
pub mod lang;
pub mod screens;
pub mod theme;
pub mod video;

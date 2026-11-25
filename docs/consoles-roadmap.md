# Outros consoles: plano (22/11/2026)

Estado: **planejado, não começado.** O trabalho no SNES começa quando a placa (Waveshare RP2350-PiZero com a
PSRAM APS6404L-3SQR-SN de 8 MB) chegar e o NES estiver medido nela. Os números de viabilidade estão em
[`performance-and-snes.md`](performance-and-snes.md), seção "Quais consoles cabem".

## Base: Mesen2 para todos

O núcleo de NES foi escrito seguindo o Mesen. O Mesen2 (o emulador de confiança do usuário, GPL-3, compatível com a
nossa GPL-3.0-or-later) emula também SNES, Game Boy / Game Boy Color, GBA, PC Engine, Master System / Game Gear e
WonderSwan. Decisão: todo console novo segue o Mesen2 como referência de comportamento (ADR-010 em
`rust/DECISIONS.md`). Um só emulador de referência, cada console portado do mesmo jeito.

| Console (Mesen2) | Nesta placa | Prioridade |
|---|---|---|
| NES | feito | já feito |
| SNES | experimental: pulo de quadros, sem SA-1 e SuperFX | 1 (pedido do usuário) |
| Game Boy / Game Boy Color | cabe (~2-3 M instruções por quadro) | 2 |
| Master System / Game Gear | cabe (~2,2-3,3 M) | 2 |
| WonderSwan / Color | cabe: V30MZ a 3,07 MHz, 224x144 a 75 Hz, estimativa ~2-3 M | 3 |
| PC Engine | no limite (~5 M): overclock ou os dois núcleos | 3 |
| GBA | não cabe: memória (EWRAM + VRAM) e ROMs de 16-32 MB | fora |

Fora do Mesen2: Atari 2600 (cabe, sem pressa), Mega Drive (pesado) e PICO-8 (precisa de um interpretador Lua
`no_std` em Rust).

## Método (o mesmo do NES)

1. **Referência fiel ao Mesen2**, ponto a ponto e ciclo a ciclo, em Rust `no_std`, rodando no PC. É ela que passa
   nos testes de precisão.
2. **Medição na bancada do M33** (`m33-bench`), antes de otimizar: número real no lugar da estimativa.
3. **Versão rápida para a placa** (vídeo por linha, eventos previstos), comparada sempre com a referência. No NES
   ficou idêntica pixel a pixel; no SNES a meta é saber exatamente o que difere e em quais jogos.

## SNES: unidades

Crate novo `rust/snes-core` (`no_std`, sem `unsafe`), ao lado do `nes-core`. Os nomes de arquivo do Mesen2 abaixo
são de memória: conferir no repositório ao começar cada unidade.

| # | Unidade | Base no Mesen2 | Prova |
|---|---|---|---|
| S1 | CPU 65816 | `Core/SNES/SnesCpu*` | SingleStepTests do 65816 (cada opcode, ciclo a ciclo, modos 8/16 bits e emulação) |
| S2 | SPC700 e DSP de som | `Core/SNES/Spc*`, `SnesDsp*` | SingleStepTests do SPC700; som comparado com o Mesen2 em arquivos `.spc` |
| S3 | PPU de referência, ponto a ponto | `Core/SNES/SnesPpu*` | ROMs de teste de PPU; quadros comparados com capturas do Mesen2 |
| S4 | Barramento, DMA/HDMA, LoROM/HiROM, controles | `Core/SNES/SnesMemoryManager*`, `SnesDmaController*`, `BaseCartridge*` | os 8 jogos sem chip extra rodando no PC (`snes_run`, como o `nes_run`) |
| S5 | Medição no M33 | nenhuma | `m33-bench` com o núcleo SNES: confirma ou corrige a estimativa de 8-11 M instruções por quadro |
| S6 | PPU rápida por linha e pulo de quadros | nenhuma | comparação com a S3, lista do que difere |
| S7 | Na placa | nenhuma | ROM na PSRAM (cache de 16 KB do XIP), som, menu do frontend lendo `snes/` |

Fora do escopo: chips extras (SA-1, SuperFX, DSP-1, Cx4, S-DD1, SPC7110). O SA-1 é um segundo 65816 a 10,7 MHz e o
SuperFX um RISC de 10,7-21 MHz; não cabem no orçamento.

Jogos de teste (cabeçalhos já lidos, ver `performance-and-snes.md`): Desolate, Tetris Attack, Gundam W - Endless
Duel, A Link to the Past, Vampire's Kiss, Final Fight 3, Super Castlevania IV, Chrono Trigger. Super Mario World e
Mega Man X servem nas versões originais (as do usuário estão convertidas para SA-1).

## Riscos

- **PSRAM.** Os projetos que já rodam SNES na RP2350 (pico-snesPlus, frank-snes) dizem que a PSRAM, não o
  relógio, é o gargalo. Medir as leituras fora do cache já na S5.
- **Núcleo 1 ocupado com o vídeo.** O HDMI da placa está nos GPIO 32-39, fora do HSTX; o núcleo 1 monta os
  símbolos TMDS. Passar mais desse trabalho para PIO e DMA libera tempo para o som do SNES.
- **Precisão do Mesen2 custa caro.** A referência fiel não vai caber na placa; ela é o gabarito, não o que roda lá.

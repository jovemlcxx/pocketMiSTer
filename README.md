# pocketMiSTer

Um console de videogame pequeno, barato e aberto: uma placa de microcontrolador RP2350, um cartão micro SD e
controles de Super Nintendo, com um emulador escrito do zero em Rust.

O objetivo é que qualquer pessoa consiga montar o seu (em casa, numa escola ou num projeto de bairro) e que, no
caminho, aprenda um pouco de eletrônica e de programação. A documentação é em português, passo a passo.

> **Estado: alfa.** O núcleo de NES está completo e testado no PC. O firmware compila, mas ainda não foi testado
> na placa.

## Hardware

- **Placa:** Waveshare RP2350-PiZero (RP2350, 16 MB de flash, micro SD e saída mini HDMI)
- **Memória extra:** PSRAM APS6404L-3SQR-SN de 8 MB, soldada no lugar reservado da placa
- **Controles:** dois conectores de controle SNES
- **Vídeo:** adaptador mini HDMI para HDMI, imagem 4:3 centralizada em qualquer TV

Ligações em [`hardware/`](hardware/).

## Organização

| Pasta | Conteúdo |
|---|---|
| [`rust/`](rust/) | Todo o código: núcleo de emulação, firmware da placa, frontend, ferramentas e testes |
| [`hardware/`](hardware/) | Esquema de ligação dos controles e da placa |
| [`docs/`](docs/) | Desempenho, viabilidade de outros consoles, frontend e cartão SD |

Dentro de `rust/`:

| Crate | O que é |
|---|---|
| `nes-core/` | Núcleo do NES (CPU, PPU, APU, mappers), `no_std` e sem `unsafe`: o mesmo código roda no PC e na placa |
| `interface/` | Frontend sem hardware: catálogo do cartão SD, telas no estilo do menu do MiSTer, temas, fundos, modos de vídeo |
| `board/` | Firmware da RP2350: vídeo DVI pelo PIO, menu de jogos e controles |
| `app/` | `nes_run`: roda uma ROM no PC sem janela (imagem, som em WAV, resultado dos testes) |
| `xtask/` | `cargo xtask`: testes, clipes de vídeo, simulação das telas, esquemas e o pacote de jogos |
| `m33-bench/`, `bench-m33-fw/` | Bancada que mede o núcleo no Cortex-M33 da RP2350, instrução por instrução |

`rust/MAP.md` lista o que foi feito e o que falta; `rust/DECISIONS.md` registra as decisões de projeto.

## Situação

| Parte | Estado |
|---|---|
| CPU 6502 | idêntica ao log de referência `nestest`, ciclo a ciclo (8991/8991) |
| PPU | versão rápida idêntica à de referência ponto a ponto |
| APU | 2 pulsos, triângulo, ruído, DMC com o DMA exato; VRC6 e Sunsoft 5B |
| Testes do blargg | 51 de 56 (os 5 restantes só mostram um CRC na tela, sem valor esperado) |
| Mappers | NROM, MMC1, UxROM, CNROM, MMC3, VRC6, FME-7; save de bateria |
| Desempenho | 4,2 a 4,7 M instruções por quadro no Cortex-M33, com som ([`docs/performance-and-snes.md`](docs/performance-and-snes.md)) |
| Frontend | telas, temas e fundos prontos e simulados no PC; falta ligar ao firmware |
| Firmware | primeira versão, não testada na placa; falta som, cartão SD e PSRAM |
| Outros consoles | planejados, com o Mesen2 como referência ([`docs/consoles-roadmap.md`](docs/consoles-roadmap.md)) |

## Usando no PC

Precisa do Rust (`rustup`). Dentro de `rust/`:

```sh
cargo xtask nestest                      # CPU contra o log de referência
cargo xtask identity                     # PPU rápida contra a de referência, quadro a quadro
cargo xtask blargg                       # testes do blargg (ROMs de teste em referencias/nes-test-roms)
cargo test --workspace --release         # testes do núcleo e do frontend
cargo run --release -p nes_run -- jogo.nes 600 tela.ppm --wav som.wav
cargo xtask simulate                     # telas do frontend em PNG
```

## Gravando na placa

```sh
cargo xtask pack pacote.bin roms/*.nes           # junta os jogos
picotool load -o 0x10800000 pacote.bin           # grava o pacote a 8 MB da flash
cd board && cargo run --release                  # compila e grava o firmware (placa em modo BOOTSEL)
```

Nenhuma ROM acompanha o projeto. Use só jogos que você tem o direito de usar.

## Por que um núcleo próprio

Os emuladores que já rodam nessa classe de placa trocam precisão por velocidade. Este projeto escreve o seu
próprio núcleo, pensado para o RP2350 desde o início: um ciclo do processador emulado é um acesso ao barramento, o
código roda igual no PC (onde é testado contra referências) e na placa, e não depende de biblioteca nenhuma. A
NESdev Wiki e o Mesen (mesma licença GPL) são as referências de comportamento.

## Contribuindo

Testes, montagens em outras placas, tradução da documentação e oficinas usando o projeto são bem-vindos. Todo
comportamento novo no núcleo deve vir com um teste que compare contra uma referência.

## Licença

Código sob GPL-3.0-or-later; documentação e esquemas sob CC BY-SA 4.0. Detalhes em [`LICENSE.md`](LICENSE.md).

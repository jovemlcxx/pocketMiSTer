# pocketMiSTer

Um videogame pequeno, barato e aberto, que cabe numa caixinha de fita cassete.

Por dentro tem uma placa com o microcontrolador RP2350, um cartão micro SD com os jogos e duas entradas para
controle de Super Nintendo. O emulador foi escrito do zero, em Rust, pensado para essa placa.

A ideia é que qualquer pessoa consiga montar o seu, em casa, na escola ou num projeto do bairro. E que, montando,
pegue noções de eletrônica e de informática, entendendo como um computador funciona por dentro, do processador até
a imagem na tela.

Toda a documentação é em português e vai passo a passo.

> O projeto está em alfa. O emulador de NES já está completo e testado no computador, e o firmware da placa
> compila, mas ainda não rodou numa placa de verdade.

## Do que você precisa

Uma Waveshare RP2350-PiZero, que já vem com 16 MB de flash, entrada de micro SD e saída mini HDMI.

Um chip de memória PSRAM APS6404L-3SQR-SN, de 8 MB, soldado no espaço que a placa deixa reservado para ele.

Dois conectores de controle de Super Nintendo e um adaptador de mini HDMI para HDMI. A imagem sai em 4:3,
centralizada, em qualquer TV.

O esquema de ligação está na pasta [`hardware/`](hardware/).

## Como o projeto está organizado

Todo o código fica em [`rust/`](rust/), a parte elétrica em [`hardware/`](hardware/) e os textos sobre desempenho,
frontend e próximos consoles em [`docs/`](docs/).

Dentro de `rust/`, cada pasta cuida de uma coisa:

| Pasta | O que tem |
|---|---|
| `nes-core/` | O emulador de NES. O mesmo código roda no computador e na placa |
| `interface/` | O menu de jogos, no estilo do menu do MiSTer, com temas e fundos |
| `board/` | O firmware da placa, com vídeo, menu e controles |
| `app/` | O `nes_run`, que roda um jogo no computador e salva imagem e som |
| `xtask/` | Os comandos de apoio: testes, simulação das telas, esquemas e o pacote de jogos |
| `m33-bench/` e `bench-m33-fw/` | A bancada que mede quanto o emulador custa no processador da placa |

O que já foi feito e o que falta está em `rust/MAP.md`. As decisões de projeto, com o motivo de cada uma, estão em
`rust/DECISIONS.md`.

## Onde estamos

O processador do NES bate instrução por instrução com o `nestest`, o teste de referência mais usado.

O vídeo tem duas versões, uma exata e outra rápida para a placa, e as duas geram a mesma imagem, pixel por pixel.

O som tem todos os canais do NES e mais os chips extras do Castlevania III japonês e do Gimmick. Dos testes do
blargg, passa em 51 de 56. Os cinco que faltam só mostram um código na tela, sem um valor certo para comparar.

Os cartuchos suportados cobrem a maior parte da biblioteca do NES (NROM, MMC1, UxROM, CNROM, MMC3, VRC6 e FME-7),
e o save dos jogos que têm bateria funciona.

Na placa, cada quadro custa entre 4,2 e 4,7 milhões de instruções, com som. Os números e as contas estão em
[`docs/performance-and-snes.md`](docs/performance-and-snes.md).

O menu já está desenhado e simulado no computador, mas ainda não foi ligado ao firmware. Na placa ainda faltam
som, cartão SD e PSRAM.

Depois do NES vêm outros consoles, começando pelo Super Nintendo, sempre usando o Mesen2 como referência. O plano
está em [`docs/consoles-roadmap.md`](docs/consoles-roadmap.md).

## Rodando no computador

Você precisa do Rust instalado (pelo `rustup`). Os comandos rodam de dentro da pasta `rust/`:

```sh
cargo xtask nestest                      # processador contra o teste de referência
cargo xtask identity                     # vídeo rápido contra o exato, quadro a quadro
cargo xtask blargg                       # testes do blargg (ROMs de teste em referencias/nes-test-roms)
cargo test --workspace --release         # todos os testes
cargo run --release -p nes_run -- jogo.nes 600 tela.ppm --wav som.wav
cargo xtask simulate                     # telas do menu em PNG
```

## Gravando na placa

```sh
cargo xtask pack pacote.bin roms/*.nes           # junta os jogos num pacote
picotool load -o 0x10800000 pacote.bin           # grava o pacote na flash
cd board && cargo run --release                  # compila e grava o firmware (placa em modo BOOTSEL)
```

Nenhum jogo vem junto com o projeto. Use só os que você tem direito de usar.

## Por que um emulador próprio

Os emuladores que já rodam nesse tipo de placa abrem mão de precisão para ganhar velocidade.

Aqui a gente quis o contrário: começar exato e só depois ficar rápido, provando a cada passo que nada mudou. O
mesmo código roda no computador, onde é testado contra referências, e na placa, sem depender de nenhuma
biblioteca. As referências de comportamento são a NESdev Wiki e o Mesen, que tem a mesma licença.

## Quer ajudar?

Testar, montar em outras placas, traduzir a documentação ou usar o projeto numa oficina, tudo ajuda.

No emulador, a regra é uma só: comportamento novo chega junto com um teste que compare com uma referência.

## Licença

O código é GPL-3.0-or-later, e a documentação e os esquemas são CC BY-SA 4.0. Os detalhes estão em
[`LICENSE.md`](LICENSE.md).

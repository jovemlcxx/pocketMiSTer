# Mapa da migração para Rust

Objetivo: o projeto inteiro em Rust (ver ADR-001), com o mesmo comportamento do C bit a bit (ADR-003).
Decisões: [DECISIONS.md](DECISIONS.md). Termos: [GLOSSARY.md](GLOSSARY.md).
Referência gravada: `rust/app-cases/` (66 casos; fica só na máquina de desenvolvimento, porque guarda caminhos locais e nomes de ROMs comerciais), conferida com `rust-anything app replay .` contra o C: 66/66.

## Unidades

| # | Unidade | Origem em C | Estado |
|---|---|---|---|
| 1 | `nes_run` (`rust/app`) | `firmware/host/nes_run.c` | feita (21/11): começou como porta de entrada com fallback para o C; agora é só Rust, sem fallback. Replay do binário Rust: 66/66 |
| 2 | CPU 6502 (`rust/nes-core/src/cpu.rs`) | `firmware/src/nes/cpu6502*.h` | feita (21/11): nestest 8991/8991; diferencial contra o C igual acesso por acesso (8 sementes x 200 mil instruções, 256 opcodes, IRQ/NMI); `no_std` compila para o M33. ligado ao `nes_run` na unidade 5 |
| 3 | PPU, referência e rápida (`rust/nes-core/src/ppu.rs`) | `firmware/src/nes/ppu.*` | feita (21/11): diferencial contra o C igual operação por operação (8 sementes x 30 mil operações: registradores, bancos, espelhamento, `step` e `run` misturados, previsão de eventos; CHR-ROM e CHR-RAM, com e sem A12) e hash de imagem/VRAM/CHR; 3 mutações pegas. Ponteiros viraram deslocamentos e o callback do A12 virou contador (`take_a12_rises`) |
| 4 | Mappers 0, 1, 2, 3, 4, 24, 26, 69 (`rust/nes-core/src/mapper.rs`) | `firmware/src/nes/mapper.*` | feita (21/11): diferencial contra o C igual operação por operação (192 combinações de mapper, CHR-ROM/RAM 8 e 32 KB, PRG 128/256 KB, espelhamento e quatro telas; 20 mil escritas, subidas de A12 e ciclos cada); 4 mutações pegas. As páginas de $6000-$FFFF ficam no mapper (`prg`, `wram`); a linha de IRQ do CPU é `irq_flag` |
| 5 | Sistema (`rust/nes-core/src/nes.rs`: barramento, relógio-mestre, DMA, controles) | `firmware/src/nes/nes.*` | feita (21/11): `rust-anything app replay --build rust/target/release/nes_run.exe .` dá 66/66 (8 jogos x 1500 quadros com hash de imagem, RAM e ciclos; 57 ROMs de teste). Modo referência é `Nes<true>` |
| 6 | `nestest_check` (`cargo xtask nestest`) | `firmware/host/nestest_check.c` | feita (21/11): 8991/8991 com o sistema inteiro |
| 7 | Testes (`cargo xtask blargg`, `cargo xtask identity`, `cargo test`) | `firmware/*.sh`, `firmware/tests/` | feita (21/11): blargg 35 passam / 21 falham (as 21 são de APU), igual ao C; identidade 10/10, em paralelo no mesmo processo. ROMs de teste do repo em `rust/nes-core/tests/roms` |
| 8 | Ferramentas: `cargo xtask clip`, `m33-bench` + `bench-m33-fw` | `firmware/tools/`, `firmware/bench/` | feita (21/11): a bancada em Rust reproduz o Python no ELF do C (3,98 M, 40,8 mil leituras de PSRAM). Núcleo Rust no M33: Probotector II 3,95 M (C 3,98 M), Castlevania III 4,41 M (C 4,74 M); ADR-005 cumprido. `.text` 118 KB |
| 9 | Gerador dos esquemas (`cargo xtask schematic`) | `hardware/gerar_esquema.py` | feita (21/11): SVG de ligação byte a byte igual ao do Python; o da placa igual fora os bytes da foto embutida (outro codificador JPEG) |
| 10 | APU (`rust/nes-core/src/apu.rs`, base Mesen) | stash "APU em C pela metade" | feita (21/11): blargg 51/56 (apu_test 8/8, apu_reset 6/6, ppu_open_bus); os 5 que faltam (dmc_dma_during_read4) só mostram um CRC na tela, sem valor esperado para comparar. Som em `--wav` e nos clipes; VRC6 e 5B (sem envelope/ruído); IRQ do VRC6/FME-7 prevista; open bus da PPU com decaimento; save de bateria (`--sav`). APU preguiçosa igual à execução ciclo a ciclo em 21 ROMs. M33 com APU: Probotector II 4,51 M, Castlevania III 4,23 M |
| 11 | Firmware da RP2350 (`rust/board`, rp235x-hal) | não existe ainda | primeira versão (21/11), compila e passa no clippy; **não testada na placa**: DVI 640x480 pelo PIO0 nos GPIO 32-39 (núcleo 1, símbolos TMDS balanceados, DMA em revezamento), menu com os jogos do pacote na flash (`cargo xtask pack`, gravado a 8 MB), dois controles SNES, Select+Start volta ao menu. Falta: som (HDMI com ilhas de dados ou PWM), cartão SD, PSRAM (CS no GPIO 47), salvar `.sav`, e velocidade: a 252 MHz o núcleo faz cerca de 40 quadros/s (precisa de ~350 MHz, ver desempenho) |

## Pedidos para depois da migração (21/11)

Anotados a pedido do usuário; entram depois do firmware da RP2350 (unidade 11).

| # | Pedido | Notas |
|---|---|---|
| 12 | Frontend no estilo do menu do MiSTer FPGA | lista de jogos do cartão SD, navegação pelo controle; visual próprio inspirado no do MiSTer (sem copiar arte ou marca) |
| 13 | Menu dentro do jogo | abrir com uma combinação de botões: voltar ao menu, reset, salvar/carregar estado, trocar shader |
| 14 | Shaders iguais aos do emulador do usuário | os do emulador do usuário (repositório privado), em especial OSSC e PVM, os últimos adicionados. Na RP2350 não há GPU: viram filtros feitos na hora de mandar cada linha para o HDMI (linhas de varredura, máscara), em inteiro |
| 15 | Outros consoles seguindo o Mesen2 (ADR-010), começando pelo SNES quando a placa chegar | unidades S1-S7 e ordem dos consoles em [`docs/consoles-roadmap.md`](../docs/consoles-roadmap.md); viabilidade medida em `docs/performance-and-snes.md` |

Escopo do cartão SD, do backend e do caminho até a tela: [`docs/frontend-and-sd.md`](../docs/frontend-and-sd.md)
(pastas `nes/`, `snes/`, `pico8/` na raiz, cada uma com `bios/`, `saves/` e `estados/`).

Opções do frontend (22/11): 9 esquemas de cores (`interface::theme::SCHEMES`: mister, light, famicom, super, gameboy,
amber, phosphor, ocean, mono), todos com contraste de 4,5:1 ou mais conferido em teste; fundos como arquivos `.fnd`
no cartão (`pocketmister/backgrounds/`; `cargo xtask backgrounds` grava os 6 gerados, `cargo xtask background` converte
uma imagem); tela de Ajustes. Nomes do código, arquivos, pastas do cartão, chaves do `config.ini` e subcomandos em
inglês; comentários e documentação em português.

Frontend (22/11): crate `rust/interface` (sem hardware: catálogo do SD, navegação, telas no estilo do OSD do MiSTer
em 320x240, modos de vídeo) e `cargo xtask simulate`, que desenha as telas em PNG em cada modo (`demo/simulation/`).
Modos: 640x480 (4:3, 252 MHz), 800x480 (5:3, 309 MHz), 848x480 (16:9, 337 MHz), 800x600 (4:3, 400 MHz); o
aplicativo 4:3 sempre no centro com escala inteira e bordas pretas. Falta ligar ao firmware: cartão SD, vídeo com
os modos novos (hoje a placa só faz 640x480 com a imagem do NES) e a camada de menu por cima do jogo.

## Notas

- `rust/nes-core/tests/c/cpu_trace.c` e `ppu_trace.c` são C só de teste (o lado C dos diferenciais); saem junto com `firmware/` no lift.
- Otimizações que levaram o núcleo de 5,88 M para 3,95 M instruções por quadro sem mudar nada do comportamento
  (replay, identidade e diferenciais iguais): `repr(C)` com o CPU e os campos de todo ciclo primeiro; tabelas de
  páginas `&[u8; 0x800]`/`&[u8; 0x400]` para PRG e CHR-ROM (sem checagem de limite, como os ponteiros do C);
  caminho quente do barramento só com RAM e PRG; linha de IRQ copiada só quando o mapper muda; registradores de
  deslocamento da PPU em variáveis no laço de blocos; limpeza só dos blocos usados da linha de sprites.
- `m33-bench` compila o Unicorn (C) pelo crate `unicorn-engine`: precisa do CMake e do Ninja do Visual Studio
  Build Tools no PATH (`.../Common7/IDE/CommonExtensions/Microsoft/CMake/{CMake/bin,Ninja}`). Por isso ele não
  está nos `default-members`: `cargo build -p m33-bench --release`.

## Perguntas em aberto

- Como chegar a 60 quadros/s na placa: subir o relógio (com o DVI em outro modo de vídeo) ou dividir o trabalho entre os dois núcleos.

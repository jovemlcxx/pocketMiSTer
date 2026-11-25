# Decisões da migração para Rust

## ADR-001: Tudo em Rust
Status: aceita (21/11/2026)
Contexto: o usuário decidiu que o projeto inteiro passa a ser Rust.
Decisão: núcleo NES, ferramentas do PC (`nes_run`, `nestest_check`), testes, ferramentas que hoje são Python
(`make_clip.py`, `m33_bench.py`, `hardware/gerar_esquema.py`) e o firmware da RP2350 serão Rust. O C fica só como
referência até a troca, em `firmware/`.
Consequências: nenhum código novo em C ou Python; os scripts `.sh` viram comandos Rust (`cargo xtask` ou binários).

## ADR-002: Núcleo `no_std`, sem `unsafe`
Status: aceita (21/11/2026)
Contexto: o mesmo núcleo roda no PC (testes) e no Cortex-M33 da RP2350.
Decisão: crate do núcleo com `#![no_std]`, sem alocação na emulação, sem ponto flutuante e sem `unsafe`. Na camada de
hardware da placa, `unsafe` só onde for inevitável, isolado e com comentário `// SAFETY:`.
Consequências: a `rust-anything analyze` marcou `src/nes/` como "manter em C" por causa dos `__attribute__` de
otimização; é falso positivo, o código é portável e é portado.

## ADR-003: Critério de igualdade é bit a bit com o C
Status: aceita (21/11/2026)
Contexto: o C passa no nestest, em 35 testes do blargg e no teste de identidade da PPU.
Decisão: o Rust tem que reproduzir exatamente a saída do `nes_run` em C gravada em `rust/app-cases/` (66 casos: 8 jogos
com hash de imagem, RAM e ciclos de cada um de 1500 quadros, e 57 ROMs de teste com o texto e o resultado em $6000).
Consequências: `rust-anything app replay` tem que dar 66/66 depois de cada unidade. Uma diferença é bug do porte.
Revisão (21/11): com o núcleo todo em Rust e a APU nova (que muda a saída de propósito: blargg de 35 para 50
aprovados, IRQ de quadro e DMC nos jogos), os 66 casos foram regravados a partir do `nes_run` em Rust. Daqui
para frente a referência é essa; o C só serviu para provar o porte antes da APU.

## ADR-004: APU direto em Rust
Status: aceita (21/11/2026)
Contexto: a APU em C estava pela metade quando o projeto passou para Rust.
Decisão: o trabalho foi guardado em `git stash` ("APU em C pela metade"), só como referência; a APU é escrita em Rust
depois que o núcleo atual estiver portado.
Consequências: os 21 testes de APU continuam falhando até lá; eles já estão gravados como falha na referência.

## ADR-005: Desempenho no Cortex-M33 não pode piorar
Status: aceita (21/11/2026)
Contexto: o C faz 3,98 M instruções por quadro (Probotector II) e 4,74 M (Castlevania III) no Cortex-M33.
Decisão: o núcleo Rust compilado para `thumbv8m.main-none-eabihf` é medido pela mesma bancada de emulador ARM e não pode
passar desses números.
Consequências: a bancada também é portada para Rust (ADR-001).
Resultado (21/11): Probotector II 3,95 M, Castlevania III 4,41 M; os dois abaixo do C.

## ADR-006: Paradas
Status: revista (21/11/2026)
Decisão original: parar para o ok do usuário depois de cada unidade.
Revisão: depois dos mappers o usuário pediu para seguir todas as unidades sem parar; só um bloqueio real
(decisão dele, hardware) interrompe.

## ADR-007: ROMs fora do repositório
Status: aceita (21/11/2026)
Contexto: os casos gravados usam ROMs comerciais (`roms/`) e de teste (`referencias/`), que não vão para o git.
Decisão: os casos guardam os caminhos absolutos desta máquina e só a saída gravada é versionada.
Consequências: o replay só roda numa máquina com as mesmas ROMs nos mesmos caminhos.

## ADR-008: Firmware com rp235x-hal, sem runtime assíncrono
Status: aceita (21/11/2026)
Contexto: a pergunta embassy-rp ou rp235x-hal ficou aberta; o usuário pediu para seguir sem parar.
Decisão: rp235x-hal. O vídeo DVI precisa de PIO e DMA com tempo exato e do registrador GPIOBASE do PIO (a placa
liga o HDMI nos GPIO 32-39, que só o PIO alcança); isso é programado direto nos registradores, e um executor
assíncrono não ajudaria. Núcleo 1 só para o vídeo, núcleo 0 para menu e emulação.
Consequências: o `unsafe` do firmware fica em `video.rs` (registradores, buffers compartilhados entre os núcleos) e
em `main.rs` (a flash mapeada do pacote e o console em `static`), cada um com `SAFETY`.

## ADR-009: Jogos da flash nesta versão
Status: aceita (21/11/2026)
Contexto: gravar ROM na flash durante a execução exige rodar da RAM com o XIP desligado e religá-lo pela rotina
salva pela ROM de boot da RP2350; carregar do cartão para a SRAM não cabe junto com o console, a tela e os buffers
de vídeo (512 KB).
Decisão: os jogos vão num pacote (`cargo xtask pack`) gravado pelo picotool a 8 MB da flash, junto com o
firmware; o emulador lê a ROM direto da flash mapeada. Cartão SD e PSRAM ficam para a próxima etapa.
Consequências: trocar a lista de jogos é gravar o pacote de novo pelo USB.

## ADR-010: Mesen2 como referência de todo console novo
Status: aceita (22/11/2026)
Contexto: o NES seguiu o Mesen. Os projetos de SNES que já existem na RP2350 usam o snes9x em C; o usuário confia
no Mesen2, que emula SNES, Game Boy/Color, GBA, PC Engine, Master System/Game Gear e WonderSwan.
Decisão: cada console novo é escrito em Rust seguindo o Mesen2 (GPL-3, compatível com a licença do projeto), com o
mesmo método do NES: referência fiel no PC, medição no M33, versão rápida comparada com a referência. Primeiro o
SNES, quando a placa chegar. Plano em `docs/consoles-roadmap.md`.
Consequências: nada de snes9x ou outro núcleo pronto; o SNES é o maior trabalho da lista e roda na placa como modo
experimental (pulo de quadros, sem SA-1 e SuperFX).

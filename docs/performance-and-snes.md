# Desempenho na RP2350 e viabilidade por console

## Hoje: núcleo em Rust (21/11/2026)

Medido com `rust/m33-bench` (o núcleo compilado de verdade para Cortex-M33 num emulador de CPU ARM que conta cada
instrução; a ROM na janela da PSRAM). A bancada em Rust dá os mesmos números do script antigo no ELF do C.

| Jogo (quadros medidos) | C, sem som | Rust, sem som | Rust, com APU e som |
|---|---|---|---|
| Probotector II (860-862, jogando) | 3,98 M | **3,95 M** | 4,51 M |
| Castlevania III / Akumajou Densetsu (1230-1232) | 4,74 M | **4,41 M** | 4,23 M (IRQ do VRC6 prevista) |

Instruções por quadro. O Rust ficou abaixo do C (ADR-005). Com a APU, o Castlevania III ficou até mais leve que
antes porque a IRQ do VRC6 passou a ser prevista em vez de contada a cada ciclo.

Na placa: com 1,0 a 1,5 ciclo por instrução, 60 quadros por segundo pedem de **270 a 400 MHz**. O firmware atual
roda a 252 MHz (o relógio que o DVI 640x480 pede), então deve ficar perto de 40 quadros por segundo: a próxima
etapa é subir o relógio com outro modo de vídeo ou dividir a emulação entre os dois núcleos (`rust/MAP.md`).

## Quais consoles cabem nesta placa (22/11/2026)

### O que foi medido: o custo do nosso NES por parte

`m33-bench` com a divisão por função, três jogos, três quadros cada (Kirby's Adventure: 4,74 M instruções por
quadro). A divisão é quase igual nos três:

| Parte | Probotector II | Castlevania III | Kirby | Média | Custo por unidade |
|---|---|---|---|---|---|
| PPU (`Ppu::run`, sprites, A12) | 54,9% | 54,9% | 54,7% | 2,46 M | **40 instruções por pixel visível** (28 por ponto) |
| CPU 6502 com barramento e DMA | 39,1% | 39,5% | 40,9% | 1,79 M | **60 instruções por ciclo de CPU** |
| APU (5 canais) | 4,9% | 5,3% | 3,4% | 0,20 M | 0,2 M por quadro |

Esses números são a régua: um núcleo nosso, escrito do mesmo jeito (ciclo a ciclo na CPU, PPU por trechos),
custa isso por pixel e por ciclo no M33. Um núcleo que emula por instrução, e não por ciclo, gasta menos na CPU;
um vídeo com várias camadas e mistura de cores gasta mais por pixel.

**Orçamento** (um núcleo emulando, o outro com o vídeo, 1,0 a 1,5 ciclo por instrução):

| Relógio | Instruções por quadro a 60 qps |
|---|---|
| 252 MHz (hoje, DVI 640x480) | 2,8-4,2 M |
| 300 MHz | 3,3-5,0 M |
| 378 MHz (overclock alto, com tensão maior) | 4,2-6,3 M |

### Estimativa por console

Modelo: ciclos de CPU por quadro x custo por ciclo, mais pixels por quadro x custo por pixel (ajustado ao
número de camadas), mais som. A coluna "já existe" é quem roda o console na RP2040/RP2350 hoje, segundo o
README de cada projeto (números deles, não medidos aqui).

| Console | CPU por quadro | Vídeo por quadro | Estimativa (M instr/quadro) | MHz para 60 qps | Já existe | Veredito |
|---|---|---|---|---|---|---|
| Atari 2600 | 6507, 20 mil ciclos | 160x192, por ciclo de cor | 1,5-2 | 90-180 | sem dado | **cabe** |
| Game Boy | LR35902, 17,6 mil ciclos M | 160x144, 1 camada + janela | ~2 | 120-180 | [pico-peanutGB](https://github.com/fhoedemakers/pico-peanutGB) (até na RP2040) | **cabe** |
| Game Boy Color | o dobro na velocidade dupla | 160x144 | ~3 | 180-270 | pico-peanutGB | **cabe** |
| Master System / Game Gear | Z80, 59,7 mil ciclos | 256x192 (GG 160x144) | 2,2-3,3 | 130-300 | [pico-smsplus](https://github.com/fhoedemakers/pico-smsplus) ("melhor na RP2350") | **cabe** |
| WonderSwan / Color | V30MZ, 41 mil ciclos (75 Hz) | 224x144 | 2-3 | 150-270 (a 75 qps) | sem dado | **cabe** |
| PC Engine | HuC6280, 119 mil ciclos | 256x239 (até 512 de largura) | ~5 (até 7) | 300-450 | [pico-pcePlus](https://github.com/PicoPlus-devel/pico-pcePlus) (só RP2350) | **no limite**: overclock ou os dois núcleos |
| Mega Drive | 68000 128 mil + Z80 60 mil ciclos | 320x224, 2 planos + sprites | 9-10 (FM YM2612 sozinho 2-3) | 550-850 | [frank-genesis](https://github.com/rh1tech/frank-genesis), pico-genesisPlus (sem números) | **pesado**: pulo de quadros e FM simplificado |
| SNES | 65816 ~50 mil + SPC700 17 mil ciclos | 256x224, até 4 camadas + mistura | 8-11 | 480-1000 | [pico-snesPlus](https://github.com/fhoedemakers/pico-snesPlus), [frank-snes](https://github.com/rh1tech/frank-snes) (com pulo de quadros) | **pesado**: experimental, só sem chips extras |
| Game Boy Advance | ARM7, 280 mil ciclos | 240x160, 4 camadas + rotação | 6-8+ | 400+ | nenhum encontrado | **não cabe**: memória, ver abaixo |
| PICO-8 | máquina virtual Lua | 128x128, 16 cores | depende do cartucho | — | nenhum encontrado | **possível para cartuchos leves**, ver abaixo |

Conferência do modelo: o gwenesis (núcleo de Mega Drive do frank-genesis) diz rodar a 60 qps num Cortex-M7 de
280 MHz. O M7 faz cerca de duas instruções por ciclo, o que equivale a uns 560 MHz de M33, dentro da faixa
estimada acima.

**Por que a estimativa do SNES subiu.** A conta antiga (120-230 M instr/s, seção abaixo) supunha ~20-40
instruções por pixel com até 4 camadas. Medido, uma camada só no NES já custa 40 por pixel. Com 2 a 4 camadas e
mistura de cores, 100 a 160 por pixel é mais realista. Os projetos que existem chegam perto de 60 qps porque
pulam quadros (desenhar metade dos quadros corta o vídeo pela metade) e porque a PSRAM, não o relógio, é o
gargalo que eles relatam.

**Game Boy Advance.** Os 520 KB de SRAM da RP2350 teriam que guardar 256 KB de EWRAM, 32 KB de IWRAM, 96 KB de
VRAM, a linha de vídeo do DVI e o próprio emulador. A EWRAM na PSRAM fica lenta demais, porque os jogos a usam o
tempo todo. E muitas ROMs têm 16 ou 32 MB, mais que os 8 MB da PSRAM.

**PICO-8.** Não há hardware para emular: é uma máquina virtual Lua de 128x128. O vídeo é barato (16 mil pixels),
mas o custo é o interpretador Lua, que varia de cartucho para cartucho. A memória Lua (até 2 MB) cabe na PSRAM.
Falta um interpretador Lua no_std em Rust, o que é um trabalho à parte.

### Amostra de jogos de SNES (cabeçalhos lidos das ROMs, que não entram no repositório)

| Jogo | Tamanho | Mapa | Chip extra |
|---|---|---|---|
| Desolate (homebrew, SNESDEV 2025) | 0,25 MB | LoROM | — |
| Tetris Attack | 1 MB | LoROM | — |
| Shin Kidou Senki Gundam W - Endless Duel | 2 MB | LoROM | — |
| The Legend of Zelda - A Link to the Past | 2 MB | LoROM | — |
| Castlevania - Vampire's Kiss (custom) | 2 MB | LoROM | — |
| Final Fight 3 | 3 MB | HiROM | — |
| Super Castlevania IV | 3 MB | LoROM | — |
| Chrono Trigger (tradução BR) | 4 MB | HiROM | — |
| Super Mario World | 1 MB | SA-1 | SA-1 (versão convertida para SA-1) |
| Mega Man X | 1,5 MB | SA-1 | SA-1 (versão convertida para SA-1) |
| Super Mario Kart ("PSICOPOMPO-FULLSCREEN") | 4 MB | SA-1 | SA-1 (hack) |
| Super Mario RPG (tradução BR) | 4 MB | SA-1 | SA-1 (original) |
| Super Mario World 2 - Yoshi's Island | 2 MB | LoROM | SuperFX |
| The Ultimate Doom (Limited Run) | 4 MB | LoROM | SuperFX |

O arquivo `Xeno Crisis (...).sfc` tem só 128 KB e não parece a ROM completa. Os dois `.bin` de 16 MB ao lado são a
ROM preenchida até 16 MB, não um emulador.

- **Todos cabem na PSRAM de 8 MB** (o maior tem 4 MB).
- **8 de 14 não usam chip extra** e são os candidatos ao modo experimental.
- **6 de 14 dependem de SA-1 ou SuperFX.** O SA-1 é um segundo 65816 a 10,7 MHz (três vezes a CPU principal) e o
  SuperFX é um processador RISC de 10,7-21 MHz: ficam fora. O Super Mario World e o Mega Man X originais não têm
  chip. Para jogar aqui, use as versões originais, não as convertidas para SA-1.

### Ordem sugerida

1. NES a 60 qps na placa (falta pouco: relógio ou dividir entre os núcleos).
2. Game Boy / Game Boy Color e Master System / Game Gear: mesmo tamanho de problema do NES, mesma régua.
3. PC Engine: no limite, depois que o NES medir bem na placa.
4. PICO-8: precisa do interpretador Lua, é um projeto de software, não de desempenho.
5. SNES e Mega Drive: modo experimental, com pulo de quadros, só depois de tudo acima.

## Histórico: núcleo em C

As seções abaixo são as medições do núcleo em C, antes da migração (os scripts citados saíram do repositório).

Medido em 17/11/2026 com `firmware/tools/m33_bench.py`: o núcleo compilado de verdade para Cortex-M33
(clang -O2, mapa de memória da RP2350) roda num emulador de CPU ARM que conta cada instrução executada.
A ROM fica na janela da PSRAM, como na placa.

## NES hoje (PPU ponto a ponto, referência de precisão)

| Jogo (quadros medidos) | Instruções M33 por quadro | Para 60 qps | Leituras na PSRAM por quadro |
|---|---|---|---|
| Probotector II (860-862, jogando) | 15,8 M | 945 M instr/s | 40,8 mil |
| Kirby's Adventure (1140-1142, mapa) | 15,4 M | 924 M instr/s | 43,6 mil |
| Castlevania III / Akumajou Densetsu (1230-1232, fase 1-01) | 15,8 M | 947 M instr/s | 40,7 mil |

Ciclos estimados: entre 1,0 e 1,5 ciclo por instrução, ou seja **0,9 a 1,4 GHz para 60 quadros por segundo**.
A RP2350 tem 150 MHz de fábrica e chega a ~300 MHz com overclock, e um dos dois núcleos fica com o vídeo HDMI.
**O núcleo ponto a ponto está de 3 a 5 vezes acima do orçamento.**

## NES com PPU rápida (otimizada para RP2350, 100% idêntica à referência)

Medido com `firmware/tools/m33_bench.py`:

| Jogo (quadros medidos) | Instruções M33 por quadro | Para 60 qps | Frequência estimada (60 qps) | Leituras na PSRAM por quadro |
|---|---|---|---|---|
| Probotector II (860-862, jogando) | **3,98 M** | 239 M instr/s | 239-355 MHz | 40,8 mil |
| Castlevania III / Akumajou Densetsu (1230-1232, fase 1-01) | **4,74 M** | 284 M instr/s | 284-426 MHz | — |

Meta de **menos de 4,0 M instruções por quadro** atingida no Probotector II, mantendo identidade exata (imagem, RAM e ciclos) contra a referência ponto a ponto. O Castlevania III ainda passa da meta: o VRC6 conta a IRQ a cada ciclo de CPU (`mapper_cpu_cycle`), o que a previsão de eventos ainda não cobre.

Conferido em 18/11/2026, depois do trabalho do agente Antigravity: `build.sh` (nestest 100%), `tests/identity.sh` (10/10) e `tests/run_blargg.sh` (35/37) passando, e as duas medições acima refeitas de forma independente.

Onde vai o tempo (Probotector II, 3,98 M instruções):

| Função | Parte | O que é |
|---|---|---|
| `ppu_run` (com `fast_chunks` inlinado) | 50,1% | renderização rápida de fundo em blocos de 8 pontos |
| `nes_run_frame` | 27,3% | execução de opcodes 6502 inlinados no laço de quadros |
| `ppu_dots_to_event` | 6,2% | previsão de eventos (vblank, subida analítica de A12 do MMC3) |
| `am_abs` | 4,0% | modo de endereçamento absoluto do 6502 |
| `ppu_step` | 3,6% | linha pré-render e transições de borda |
| `eval_sprites` | 2,6% | avaliação de 64 sprites no ponto 257 |
| outros (`branch`, etc.) | ~6,2% | desvios e controle |

## Plano para caber

O modelo ponto a ponto continua sendo a **referência de precisão**: é ele que passa nos testes de timing
do blargg. A versão da placa troca só a forma de calcular, não o resultado:

1. **PPU por trechos ("catch-up").** Renderizar vários pixels de uma vez, com laços por tile, e só
   sincronizar quando o CPU mexe na PPU, quando um evento previsto acontece (vblank, NMI, sprite 0, IRQ
   do MMC3) ou no fim da linha. Custo esperado: ~2-6 instruções por pixel em vez de ~134 por ponto.
2. **Eventos previstos em vez de avançar a PPU a cada meio ciclo.** O momento do vblank, do sprite 0 e
   da subida de A12 é calculável de antemão.
3. **Teste de identidade.** No PC, as duas PPUs rodam lado a lado e o teste exige quadros idênticos,
   pixel a pixel, em todas as ROMs de teste e jogos.

Meta: **2-4 M instruções por quadro** (120-240 M instr/s), cabendo num núcleo da RP2350 com folga.
É estimativa; a medição volta a ser feita com a mesma ferramenta depois de cada etapa.

## SNES: viabilidade

Estimativa por componente, com as mesmas premissas da medição acima (núcleo otimizado, 60 qps):

| Parte | Trabalho por segundo | Instruções M33/s (estimado) |
|---|---|---|
| CPU 65816 | ~1,8-3,6 M ciclos | 30-60 M |
| SPC700 (som) | ~1,0 M ciclos | 10-15 M |
| DSP de som (8 vozes, 32 kHz) | 256 mil amostras-voz | 10-15 M |
| PPU por linha (até 4 camadas, 128 sprites, mistura de cores) | 3,4 M pixels | 70-140 M |
| **Total** | | **~120-230 M instr/s** |

O total cabe no papel, mas três coisas tornam o SNES muito mais arriscado que o NES:

- **ROM na PSRAM.** Jogos de SNES têm de 512 KB a 6 MB e leem a ROM de forma espalhada (gráficos,
  tabelas, código). A PSRAM QSPI responde em centenas de nanossegundos quando o dado não está no cache
  de 16 KB, e esse atraso não aparece na contagem de instruções.
- **Precisão.** Nesse orçamento a PPU do SNES tem que ser por linha, não por ponto. Isso já é o que
  fazem os emuladores de SNES em hardware pequeno, com efeitos de raster aproximados.
- **Evidência de quem já tentou.** O [frank-snes](https://github.com/rh1tech/frank-snes) (snes9x2010 no
  RP2350 com 8 MB de PSRAM) usa pulo de quadros configurável e, na placa topo de linha, um segundo RP2350
  só para o som.

**Conclusão:** SNES na RP2350-PiZero é possível como modo experimental, com pulo de quadros e
compatibilidade parcial, e só depois que o NES otimizado estiver medido na placa. NES, Game Boy, Game Boy
Color e Master System cabem e são o alvo de qualidade total (revisto em 22/11/2026, ver "Quais consoles cabem").

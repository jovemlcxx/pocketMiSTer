# Cartão SD, backend e frontend

Escopo traçado em 22/11/2026, para guiar o trabalho daqui em diante. Nada disto está implementado ainda: o firmware
atual (`rust/board`) só lê o pacote de jogos gravado na flash (ADR-009). Onde há uma decisão, ela vem com o porquê;
dá para mudar depois sem refazer o resto.

## 1. Árvore do cartão SD

O cartão é formatado em **FAT32** (a biblioteca `embedded-sdmmc` não lê exFAT; cartões acima de 32 GB vêm em exFAT
de fábrica e precisam ser formatados).

```
/
├── nes/
│   ├── bios/            disksys.rom (só para jogos .fds)
│   ├── saves/           <jogo>.sav  (RAM com bateria)
│   ├── states/         <jogo>.e1 ... <jogo>.e4  (save states)
│   ├── Plataforma/      subpastas livres, do jeito que a pessoa quiser organizar
│   │   └── jogo.nes
│   └── jogo.nes
├── snes/
│   ├── bios/            dsp1.rom, dsp2.rom, dsp3.rom, dsp4.rom, st010.rom, st011.rom
│   ├── saves/
│   ├── states/
│   └── jogo.sfc
├── pico8/
│   ├── bios/            (vazia por enquanto; reservada)
│   ├── saves/           dados persistentes dos cartuchos (cartdata)
│   └── jogo.p8.png
└── pocketmister/
    ├── config.ini       ajustes (vídeo, shader, som, controles, relógio)
    ├── favorites.txt    um caminho por linha
    ├── recent.txt       os últimos 20 jogos abertos, o mais novo em cima
    ├── backgrounds/     fundos do menu (.fnd): os gerados (cargo xtask backgrounds) e os seus
    │                    (cargo xtask background imagem.png nome.fnd); o firmware só tem o liso embutido
    └── cache/           índice de cada pasta de jogos (ver 2.3); pode ser apagado à vontade
```

Regras:

- **Uma pasta por sistema na raiz**, com nome em minúsculas: `nes`, `snes`, `pico8`. Sistemas novos (Game Boy
  Color, por exemplo) ganham a pasta deles quando o núcleo existir.
- **Cada sistema tem `bios/`, `saves/` e `states/`.** Essas três pastas são do firmware e não aparecem na lista
  de jogos; qualquer outra subpasta é da pessoa e aparece como pasta navegável.
- **Saves separados das ROMs**: a pasta de jogos pode ser copiada, trocada ou ficar só de leitura sem perder
  nada, e fazer backup dos saves é copiar `saves/`.
- **O save tem o nome da ROM** (sem a extensão): `Kirby's Adventure (USA).nes` salva em
  `nes/saves/Kirby's Adventure (USA).sav`. Renomear a ROM perde o vínculo; está escrito no LEIA-ME.
- **`pocketmister/` guarda o que é do console**, não de um sistema.
- **O firmware cria o que faltar** a cada boot (as pastas acima e um `LEIA-ME.txt` em cada `bios/` dizendo
  quais arquivos entram ali). Nunca apaga nem move nada da pessoa.

Extensões aceitas por sistema:

| Sistema | Jogos | BIOS opcional |
|---|---|---|
| NES | `.nes` (iNES e NES 2.0), `.fds` | `disksys.rom` para `.fds` |
| SNES | `.sfc`, `.smc` (com ou sem o cabeçalho de 512 bytes) | firmware dos chips DSP/ST para os jogos que usam |
| PICO-8 | `.p8.png`, `.p8` | nenhuma |

## 2. Backend (firmware)

O que o firmware faz por baixo do frontend. Cada parte é um módulo com uma responsabilidade.

### 2.1 Armazenamento

- **Cartão SD** por SPI1 (SCK 30, MOSI 31, MISO 40, CS 43), começando a 400 kHz e subindo para 25 MHz depois de
  iniciar. Nomes longos lidos pela tabela de LFN (`iterate_dir_lfn`).
- **PSRAM de 8 MB** (CS no GPIO 47) é onde a ROM aberta mora. Sem PSRAM, só jogos que cabem na SRAM livre (até
  ~128 KB) abrem do cartão, e o pacote da flash continua funcionando como hoje.
- **Escrita no cartão só fora do quadro de emulação** (no menu, ao sair do jogo, ou num intervalo programado), para
  não atrasar um quadro.

### 2.2 Sistemas (núcleos)

Todo núcleo segue a mesma interface, para o frontend não precisar saber qual é qual:

```rust
trait Sistema {
    fn abrir(rom: &[u8], bios: &Bios) -> Result<Self, ErroAbrir>;
    fn quadro(&mut self, controles: [u16; 2]);   // roda 1 quadro
    fn imagem(&self) -> Imagem;                   // tamanho e pixels do quadro
    fn som(&mut self, saida: &mut [i16]) -> usize;
    fn ram_bateria(&mut self) -> Option<&mut [u8]>;
    fn reset(&mut self);
}
```

`ErroAbrir` traz motivos legíveis ("mapper 5 ainda não suportado", "precisa de dsp1.rom em snes/bios"), que o
frontend mostra como estão.

### 2.3 Catálogo

- **Varre a pasta do sistema** (recursivo nas subpastas da pessoa, ignorando `bios/`, `saves/` e `states/`),
  filtrando pelas extensões aceitas.
- **Índice em cache** (`pocketmister/cache/<sistema>.idx`): nome, caminho curto 8.3, tamanho e um pequeno resumo por
  jogo (mapper, se é suportado). Pastas com centenas de jogos abrem na hora; o índice é refeito quando a
  quantidade de arquivos ou o tamanho da pasta mudam, ou pelo menu de ajustes.
- **Ordem**: pastas primeiro, depois jogos, os dois em ordem alfabética ignorando maiúsculas, acentos e artigos
  ("The Legend of Zelda" fica em L).
- **Nome exibido** = nome do arquivo sem a extensão. As etiquetas entre parênteses do padrão No-Intro (`(USA)`,
  `(Europe)`) aparecem menores ou somem, conforme o ajuste.
- **Favoritos e recentes** saem dos dois arquivos de `pocketmister/`.

### 2.4 Saves e estados

- **Bateria**: carregado ao abrir o jogo, gravado ao voltar para o menu e a cada 30 s se mudou.
- **Estados** (4 por jogo): salvar e carregar pelo menu do jogo. Exige que o núcleo serialize o próprio estado
  (o `nes-core` ainda não faz isso).

### 2.5 Ajustes

`pocketmister/config.ini`, texto simples, uma chave por linha, para dar para editar no PC:

```ini
[interface]
language = pt-br               ; pt-br ou en (só esses dois)
colors = mister                ; mister, light, famicom, super, gameboy, amber, phosphor, ocean, mono
background = landscape.fnd     ; um arquivo de pocketmister/backgrounds/

[video]
mode = 640x480                 ; 640x480, 800x480, 848x480, 800x600
shader = ossc                  ; none, ossc, pvm
scale = integer

[audio]
volume = 80

[controls]
turbo = no
menu_combo = select+start

[system]
clock_mhz = 252
```

## 3. Frontend

Visual: o "MiSTer Menu" do ES-DE (anthonycaccese/mister-menu-es-de), nas medidas da versão 640x480 do
jovemlcxx/es-theme-misterfpga-fcamod: cabeçalho rosa `FFDFDF` com o título em vinho `1E0000`, coluna lateral rosa
com o nome da tela na vertical, painel vinho com a lista (seleção em barra rosa, centrada na 6ª de 11 linhas),
legenda com os botões do controle desenhados, tudo sobre um papel de parede escurecido a 60%. Só o layout e as
cores vêm dos temas (que são CC BY-NC-SA); fontes e imagens são nossas. O aplicativo é 4:3 (320x240, 2x) e
nunca estica: em qualquer modo de vídeo fica no centro, com o resto preto (`interface::video`). Jogo que apaga a
coluna esquerda perde 8 pixels de cada lado, como o overscan da TV de tubo escondia.
Simulação de todas as telas, nos dois idiomas e em cada modo: `cargo xtask simulate` (PNGs em `demo/simulation/`).


A direção é a já combinada (`rust/MAP.md`, itens 12 a 14): telas no estilo do menu do MiSTer FPGA, com visual
próprio, menu dentro do jogo e os shaders OSSC e PVM do emulador do usuário. Aqui fica só o caminho que a lista de
jogos faz até a tela.

### 3.1 Fluxo

```
liga ─► abertura (logo, versão, cartão encontrado ou não)
          │
          ▼
     sistemas ─► lista do sistema ─► (pasta) ─► lista da pasta
          │            │
          │            └─ A abre o jogo ─► jogo ─► menu do jogo (combinação) ─► volta / reset / estados / ajustes / sair
          │
          ├─ Favoritos
          ├─ Recentes
          └─ Ajustes
```

- **Sistemas**: só aparecem as pastas que existem no cartão, com a quantidade de jogos. Sistema sem núcleo ainda
  aparece apagado ("em breve").
- **Lista**: o que o catálogo entrega, com uma linha de informação embaixo do item marcado (tamanho e se é
  suportado). Jogo não suportado aparece, marcado, e explica o motivo ao tentar abrir.
- **Sem cartão**: a lista vem do pacote da flash, como hoje.

### 3.2 Controles no menu

| Botão | Ação |
|---|---|
| Cima/baixo | anda um item (segurar repete) |
| L/R | pula para a letra anterior/seguinte |
| A | abre o jogo ou entra na pasta |
| B | volta uma tela |
| Y | marca ou desmarca favorito |
| Select | ajustes rápidos do sistema |
| Start | abre o jogo |

## 4. Código

O que é lógica pura (catálogo, ordenação, ajustes, telas) fica num crate sem hardware, testável no PC e com um
simulador que desenha as telas em PNG; a placa só liga isso ao cartão, ao vídeo e aos controles.

```
rust/
├── interface/            no_std, sem hardware: telas, navegação, tema, fonte, desenho num framebuffer
│   ├── catalogo.rs       varredura, ordenação, índice, favoritos/recentes
│   ├── ajustes.rs        leitura e escrita do config.ini
│   ├── telas/            abertura, sistemas, lista, menu do jogo, ajustes
│   └── tema.rs           cores, fonte, espaçamentos
├── sistemas/             a interface `Sistema` e a ligação de cada núcleo (nes-core primeiro)
├── placa/src/
│   ├── video/            DVI e camada de menu sobre o jogo
│   ├── armazenamento/    cartão SD, PSRAM, pacote da flash
│   └── main.rs
└── xtask/                `cargo xtask simulate`: telas do frontend em PNG, a partir de um SD de exemplo
```

## 5. Ordem de trabalho

1. Primeiro teste do firmware atual na placa (vídeo, controles, pacote).
2. Cartão SD: leitura, árvore criada no boot, catálogo, lista na tela no lugar do pacote.
3. PSRAM: abrir jogos grandes do cartão.
4. Saves de bateria no cartão.
5. Crate `interface` e simulador; telas no estilo MiSTer (item 12) e menu do jogo (item 13).
6. Som na placa.
7. Shaders OSSC e PVM (item 14).
8. Estados (save states).
9. Núcleos de SNES e PICO-8.

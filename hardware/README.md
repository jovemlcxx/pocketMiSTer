# hardware

| Arquivo | O que é |
|---|---|
| `pocketmister-wiring.svg` / `.png` | Header de 40 pinos da RP2350-PiZero e os dois conectores de controle SNES |
| `_fonts.svgfrag` | Fontes IBM Plex Sans e Barlow (licença OFL) embutidas nos SVGs |

## Regerar

O esquema sai do gerador em Rust (dentro de `rust/`):

```sh
cargo xtask schematic
```

O gerador também desenha `pocketmister-board.svg` (a placa com cada parte apontada, inclusive o lugar da PSRAM) e
há um manual de montagem em `pocketmister-manual.html`. Os dois usam uma foto da placa que não é nossa e por isso
ficam fora do repositório (`.gitignore`) até existir uma foto própria em `../referencias/fotos/rp2350-pizero-frente.jpg`.

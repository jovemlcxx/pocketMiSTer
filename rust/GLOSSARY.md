# Glossário

| Termo | Significado | Nome em Rust |
|---|---|---|
| Ciclo (de CPU) | um acesso ao barramento do 6502; 12 ticks do relógio-mestre | `Cpu::cycles` |
| Relógio-mestre | 12 ticks por ciclo de CPU, 4 por ponto da PPU (NTSC) | `Nes::master` |
| Ponto (dot) | uma unidade de tempo da PPU; 341 por linha, 262 linhas | `Ppu::dot` |
| Linha (scanline) | 0-239 visíveis, 240 pós, 241-260 vblank, 261 pré-render | `Ppu::scanline` |
| Página | 2 KB do barramento do CPU com ponteiro direto (RAM, PRG) | `Nes::rd_page` |
| Mapper | chip do cartucho que troca bancos de PRG/CHR | `mapper` |
| A12 | bit 12 do endereço da PPU; o MMC3 conta linhas pelas subidas dele | `Ppu::a12` |
| Sincronização preguiçosa | a PPU só anda quando algo depende dela | `ppu_sync` |
| Referência | a PPU ponto a ponto (`PPU_REFERENCE` no C) | `Ppu::step` (o rápido é `Ppu::run`) |
| Barramento | o que o CPU enxerga; cada leitura ou escrita é um ciclo | trait `Bus` |

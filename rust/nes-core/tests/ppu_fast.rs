//! A PPU rápida (`run`) contra a referência ponto a ponto (`step`): as duas recebem a mesma
//! sequência aleatória de operações (registradores, bancos de CHR, espelhamento, avanço de
//! pontos, previsão de eventos) e o estado tem que ser igual depois de cada uma, com a imagem,
//! a VRAM e a CHR a cada 32. Substitui o diferencial contra a PPU em C (que saiu no lift).

use nes_core::Ppu;

const OPERATIONS: u32 = 30_000;

struct Rng(u32);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1_103_515_245).wrapping_add(12345);
        self.0 >> 16
    }
}

fn fnv(b: &[u8]) -> u32 {
    b.iter().fold(2_166_136_261u32, |h, &x| {
        (h ^ u32::from(x)).wrapping_mul(16_777_619)
    })
}

fn state(p: &Ppu) -> [u32; 17] {
    [
        p.scanline as u32,
        u32::from(p.dot),
        u32::from(p.v),
        u32::from(p.t),
        u32::from(p.fine_x),
        u32::from(p.w),
        u32::from(p.status),
        u32::from(p.nmi_out),
        u32::from(p.a12),
        p.a12_rises,
        p.dots,
        u32::from(p.frame_done),
        u32::from(p.read_buf),
        u32::from(p.io_db),
        u32::from(p.oam_addr),
        p.frame,
        p.a12_low_since,
    ]
}

fn new_ppu<'a>(rng: &mut Rng, ram: bool, chr: &'a [u8], hook: bool) -> Box<Ppu<'a>> {
    let mut p = Box::new(Ppu::new(if ram { &[] } else { chr }));
    if ram {
        p.chr_ram.copy_from_slice(&chr[..0x8000]);
    }
    for x in p.vram.iter_mut() {
        *x = rng.next() as u8;
    }
    for x in p.palette.iter_mut() {
        *x = (rng.next() & 0x3F) as u8;
    }
    for x in p.oam.iter_mut() {
        *x = rng.next() as u8;
    }
    p.power();
    p.chr_writable = ram;
    p.a12_hook = hook;
    p
}

#[test]
fn fast_ppu_matches_reference() {
    for seed in [0u32, 1, 2, 3, 6, 7, 1234, 99_999] {
        let ram = seed & 1 != 0;
        let mut rng = Rng(seed);
        let chr: Vec<u8> = (0..0x20000).map(|_| rng.next() as u8).collect();
        let pages = if ram { 32 } else { 128 };
        // as duas PPUs começam iguais (mesmo gerador a partir do mesmo ponto)
        let mut ra = Rng(seed ^ 0x5555);
        let mut rb = Rng(seed ^ 0x5555);
        let mut a = new_ppu(&mut ra, ram, &chr, seed & 2 != 0);
        let mut b = new_ppu(&mut rb, ram, &chr, seed & 2 != 0);
        for slot in 0..8 {
            let o = (rng.next() % pages) * 0x400;
            a.set_chr(slot, o);
            b.set_chr(slot, o);
        }
        for k in 0..OPERATIONS {
            let r = rng.next() % 100;
            if r < 50 {
                let n = 1 + rng.next() % 700;
                for _ in 0..n {
                    a.step();
                }
                b.run(n);
            } else if r < 65 {
                let addr = (rng.next() & 7) as u16;
                let mut v = rng.next() as u8;
                if addr == 1 && !rng.next().is_multiple_of(4) {
                    v = 0x1E | (v & 0xE1); // quase sempre renderizando
                }
                a.write_reg(addr, v);
                b.write_reg(addr, v);
            } else if r < 75 {
                let addr = (rng.next() & 7) as u16;
                assert_eq!(
                    a.read_reg(addr),
                    b.read_reg(addr),
                    "semente {seed}, op {k}: leitura de ${:04X}",
                    0x2000 + addr
                );
            } else if r < 80 {
                let slot = (rng.next() & 7) as usize;
                let o = (rng.next() % pages) * 0x400;
                a.set_chr(slot, o);
                b.set_chr(slot, o);
            } else if r < 83 {
                let i = (rng.next() & 3) as usize;
                let o = ((rng.next() & 1) * 0x400) as u16;
                a.nt[i] = o;
                b.nt[i] = o;
            } else if r < 85 {
                a.frame_done = false;
                b.frame_done = false;
            } else {
                // como o sistema: anda até o próximo evento previsto
                let n = b.dots_to_event(262 * 341);
                for _ in 0..n {
                    a.step();
                }
                b.run(n);
            }
            assert_eq!(state(&a), state(&b), "semente {seed}, op {k}");
            if k & 31 == 31 {
                assert_eq!(
                    fnv(a.fb.as_flattened()),
                    fnv(b.fb.as_flattened()),
                    "semente {seed}, op {k}: imagem"
                );
                assert_eq!(fnv(&a.vram), fnv(&b.vram), "semente {seed}, op {k}: VRAM");
                assert_eq!(
                    fnv(&a.chr_ram),
                    fnv(&b.chr_ram),
                    "semente {seed}, op {k}: CHR-RAM"
                );
            }
        }
    }
}

//! Compares the page-table read path against the existing `Banks` + `Memory<Box<[u8]>>` path.
//!
//! Phase 2 of the mapper rework replaces per-mapper `banks.translate(addr)` lookups with a shared
//! 1 KiB page table. This isolates just the read cost of the two formulations, using an address
//! pattern that mimics the PPU's background fetches (nametable byte, attribute byte, then two
//! pattern table bytes per tile).
//!
//! It deliberately measures reads only. Bank switching happens on register writes, which are orders
//! of magnitude rarer, and the whole point of the redesign is to move work there.

#![allow(clippy::expect_used, reason = "fine in a benchmark")]

use std::{hint::black_box, time::Instant};
use tetanes_core::{
    mem::{Banks, Memory as Buffer},
    memory::{Memory, MemoryLayout, Src},
    ppu::Mirroring,
};

const CHR_SIZE: usize = 8 * 1024;
const ITERATIONS: usize = 200;
/// Background fetches for one frame: 4 reads per tile, 32 tiles per line, 240 lines.
const TILES: u16 = 32 * 240;

/// PPU background fetch pattern for one tile.
#[inline(always)]
fn tile_addrs(tile: u16) -> [u16; 4] {
    let nt = 0x2000 | (tile & 0x0FFF);
    let attr = 0x23C0 | (tile & 0x0038) | ((tile >> 3) & 0x0007);
    let pattern = (tile & 0xFF) << 4;
    [nt, attr, pattern, pattern + 8]
}

fn bench(name: &str, mut run: impl FnMut() -> u64) {
    // Warmup.
    for _ in 0..10 {
        black_box(run());
    }
    let mut best = f64::INFINITY;
    for _ in 0..ITERATIONS {
        let start = Instant::now();
        black_box(run());
        let elapsed = start.elapsed().as_secs_f64();
        best = best.min(elapsed);
    }
    let reads = f64::from(TILES) * 4.0;
    println!(
        "{name:<28} {:>8.3} ms/frame-of-fetches  {:>6.2} ns/read",
        best * 1000.0,
        (best / reads) * 1e9
    );
}

fn main() {
    println!("{} reads per iteration, best of {ITERATIONS}\n", TILES * 4);

    // New: page table over a unified allocation.
    let mut memory = Memory::new(MemoryLayout {
        chr: CHR_SIZE,
        chr_writable: false,
        ..Default::default()
    });
    for (i, page) in memory.region_mut(Src::Chr).iter_mut().enumerate() {
        *page = i as u8;
    }
    memory.map_chr(0x0000, CHR_SIZE, 0, Src::Chr);
    memory.set_mirroring(Mirroring::Horizontal);

    // Old: `Banks` translation into a separate `Memory<Box<[u8]>>`, with CIRAM handled by the
    // address-munging mirror function, as every mapper does today.
    let mut chr = Buffer::new(CHR_SIZE);
    for i in 0..CHR_SIZE {
        chr[i] = i as u8;
    }
    let chr_banks = Banks::new(0x0000, 0x1FFF, CHR_SIZE, CHR_SIZE).expect("valid banks");
    let mut ciram = Buffer::new(2 * 1024);
    for i in 0..2048 {
        ciram[i] = i as u8;
    }

    bench("page table", || {
        let mut sum = 0u64;
        for tile in 0..TILES {
            for addr in tile_addrs(tile) {
                sum += u64::from(memory.chr_peek(addr));
            }
        }
        sum
    });

    bench("banks + mirror", || {
        let mut sum = 0u64;
        for tile in 0..TILES {
            for addr in tile_addrs(tile) {
                // This is the body every mapper's `chr_peek` repeats today.
                let val = match addr {
                    0x0000..=0x1FFF => chr[chr_banks.translate(addr)],
                    0x2000..=0x3EFF => {
                        let nametable = (addr >> Mirroring::Horizontal as u16) & 0x2400;
                        ciram[(nametable | (!nametable & addr & 0x03FF)) as usize]
                    }
                    _ => 0,
                };
                sum += u64::from(val);
            }
        }
        sum
    });
}

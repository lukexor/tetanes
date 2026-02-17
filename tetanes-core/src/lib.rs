#![doc = include_str!("../README.md")]
#![doc(
    html_favicon_url = "https://github.com/lukexor/tetanes/blob/main/assets/linux/icon.png?raw=true",
    html_logo_url = "https://github.com/lukexor/tetanes/blob/main/assets/linux/icon.png?raw=true"
)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

pub mod action;
pub mod apu;
pub mod bus;
pub mod cart;
pub mod debug;
pub mod fs;
pub mod time;
#[macro_use]
pub mod common;
pub mod control_deck;
pub mod cpu;
pub mod error;
pub mod genie;
pub mod input;
pub mod mapper;
pub mod mem;
pub mod ppu;
pub mod sys;
pub mod video;

pub mod prelude {
    //! The prelude re-exports all the common structs/enums used for basic NES emulation.

    pub use crate::{
        action::Action,
        apu::{Apu, Channel},
        cart::Cart,
        common::{Clock, ClockTo, NesRegion, Regional, Reset, ResetKind, Sample},
        control_deck::{Config, ControlDeck, HeadlessMode},
        cpu::Cpu,
        genie::GenieCode,
        input::{FourPlayer, Input, Player},
        mapper::{Map, MappedRead, MappedWrite, Mapper, MapperRevision},
        mem::RamState,
        ppu::{Mirroring, Ppu},
        video::Frame,
    };
}

#[cfg(test)]
mod tests {
    use super::prelude::*;
    use crate::{
        apu::{
            dmc::Dmc, filter::FilterChain, frame_counter::FrameCounter, noise::Noise, pulse::Pulse,
            triangle::Triangle,
        },
        bus::Bus,
        cpu::{IrqFlags, Status, instr::AddrMode},
        debug::PpuDebugger,
        mem::{ConstArray, Memory},
        ppu::{
            SprFlags, bus::Bus as PpuBus, ctrl::Ctrl, mask::Mask, scroll::Scroll, sprite::Sprite,
            status::Status as PpuStatus,
        },
    };
    use std::collections::HashMap;

    macro_rules! print_layout {
        ($ty:ty, $($field:ident: $field_ty:ty),+$(,)?) => {{
            use ::std::mem::{offset_of, size_of};
            println!("{} total size: {} bytes", stringify!($ty), size_of::<$ty>());
            $(
                println!(
                    "  {:<25}: offset {:4}, size {}",
                    stringify!($field),
                    offset_of!($ty, $field),
                    size_of::<$field_ty>()
                );
            )+
        }};
    }

    // Utility to help print alignment and size of struct field for cache-optimization.
    #[test]
    fn print_layouts() {
        print_layout!(
            Cpu,
            cycle: u32,
            master_clock: u32,
            start_cycles: u8,
            end_cycles: u8,
            pc: u16,
            operand: u16,
            addr_mode: AddrMode,
            sp: u8,
            acc: u8,
            x: u8,
            y: u8,
            status: Status,
            irq_flags: IrqFlags,
            bus: Bus,
            corrupted: bool,
            disasm: String,
        );

        print_layout!(
            Bus,
            wram: Memory<Box<[u8]>>,
            prg_rom: Memory<Box<[u8]>>,
            open_bus: u8,
            prg_ram_protect: bool,
            ram_state: RamState,
            region: NesRegion,
            ppu: Ppu,
            apu: Apu,
            input: Input,
            prg_ram: Memory<Box<[u8]>>,
            genie_codes: HashMap<u16, GenieCode>,
        );

        print_layout!(
            Ppu,
            master_clock: u32,
            cycle: u16,
            scanline: u16,
            mask: Mask,
            clock_divider: u8,
            open_bus: u8,
            ctrl: Ctrl,
            reset_signal: bool,
            emulate_warmup: bool,

            scroll: Scroll,
            tile_shift_lo: u16,
            tile_shift_hi: u16,
            tile_addr: u16,
            tile_lo: u8,
            tile_hi: u8,
            curr_palette: u8,
            prev_palette: u8,
            next_palette: u8,
            skip_rendering: bool,

            spr_count: u8,
            spr_flags: SprFlags,
            oamaddr: u8,
            oamaddr_lo: u8,
            oamaddr_hi: u8,
            secondary_oamaddr: u8,
            overflow_count: u8,
            oam_fetch: u8,

            vblank_scanline: u16,
            prerender_scanline: u16,
            pal_spr_eval_scanline: u16,
            status: PpuStatus,
            vram_buffer: u8,
            prevent_vbl: bool,
            region: NesRegion,

            secondary_oamdata: ConstArray<u8, 32>,
            frame: Frame,

            spr_present: ConstArray<bool, 32>,
            sprites: Box<[Sprite]>,

            oamdata: ConstArray<u8, 256>,

            bus: PpuBus,

            debugger: PpuDebugger,
        );

        print_layout!(
            PpuBus,
            palette: Memory<ConstArray<u8, 32>>,
            chr: Memory<Box<[u8]>>,
            open_bus: u8,
            chr_ram: bool,
            ram_state: RamState,
            mapper: Mapper,
            ciram: Memory<Box<[u8]>>,
            exram: Memory<Box<[u8]>>,
        );

        print_layout!(
            Apu,
            master_clock: u32,
            cycle: u32,
            cpu_cycle: u32,
            should_clock: bool,
            sample_counter: f32,
            sample_period: f32,
            frame_counter: FrameCounter,
            pulse1: Pulse,
            pulse2: Pulse,
            triangle: Triangle,
            noise: Noise,
            dmc: Dmc,
            filter_chain: FilterChain,
            audio_samples: Vec<f32>,
            channel_outputs: Box<[f32]>,
            clock_rate: f32,
            sample_rate: f32,
            speed: f32,
            mapper_enabled: bool,
            region: NesRegion,
            skip_mixing: bool,
        );
    }

    // // Utility to help print sizes of Mapper and variants for cache-optimization.
    // #[test]
    // fn print_mapper_sizes() {
    //     use std::mem::size_of;
    //     println!("Mapper enum: {} bytes", size_of::<Mapper>());
    //     println!("  Nrom: {}", size_of::<Nrom>());
    //     println!("  Sxrom: {}", size_of::<Sxrom>());
    //     println!("  Uxrom: {}", size_of::<Uxrom>());
    //     println!("  Cnrom: {}", size_of::<Cnrom>());
    //     println!("  Txrom: {}", size_of::<Txrom>());
    //     println!("  Exrom: {}", size_of::<Box<Exrom>>());
    //     println!("  Axrom: {}", size_of::<Axrom>());
    //     println!("  Pxrom: {}", size_of::<Pxrom>());
    //     println!("  Fxrom: {}", size_of::<Fxrom>());
    //     println!("  ColorDreams: {}", size_of::<ColorDreams>());
    //     println!("  BandaiFCG: {}", size_of::<BandaiFCG>());
    //     println!("  JalecoSs88006: {}", size_of::<JalecoSs88006>());
    //     println!("  Namco163: {}", size_of::<Box<Namco163>>());
    //     println!("  Vrc6: {}", size_of::<Box<Vrc6>>());
    //     println!("  Bnrom: {}", size_of::<Bnrom>());
    //     println!("  Nina001: {}", size_of::<Nina001>());
    //     println!("  Gxrom: {}", size_of::<Gxrom>());
    //     println!("  SunsoftFme7: {}", size_of::<Box<SunsoftFme7>>());
    //     println!("  Bf909x: {}", size_of::<Bf909x>());
    //     println!("  Dxrom76: {}", size_of::<Dxrom76>());
    //     println!("  Nina003006: {}", size_of::<Nina003006>());
    //     println!("  Dxrom88: {}", size_of::<Dxrom88>());
    //     println!("  Dxrom95: {}", size_of::<Dxrom95>());
    //     println!("  Dxrom154: {}", size_of::<Dxrom154>());
    //     println!("  Dxrom206: {}", size_of::<Dxrom206>());
    // }
}

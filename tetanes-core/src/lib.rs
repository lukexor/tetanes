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
        mapper::{Map, Mapper, MapperRevision},
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
        bus::{self, Bus},
        cpu::{IrqFlags, Status, instr::AddrMode},
        debug::PpuDebugger,
        mapper::{
            Axrom, BandaiFCG, Bf909x, Bnrom, Cnrom, ColorDreams, Exrom, Fxrom, Gxrom,
            JalecoSs88006, Namco163, Nina001, Nina003006, Nrom, Pxrom, SunsoftFme7, Sxrom, Txrom,
            Uxrom, Vrc6,
        },
        mem::{ConstArray, Memory},
        ppu::{
            CIRam, PaletteRam, ctrl::Ctrl, mask::Mask, scroll::Scroll, sprite::Sprite,
            status::Status as PpuStatus,
        },
    };
    use std::collections::HashMap;

    /// Utility to aid in struct field layout size and alignment.
    macro_rules! print_struct_layout {
        ($ty:ty, $($field:ident: $field_ty:ty),+$(,)?) => {{
            use ::std::mem::{offset_of, size_of};
            let mut field_rows = vec![
                $(
                    (
                        stringify!($field),
                        offset_of!($ty, $field),
                        size_of::<$field_ty>()
                    ),
                )+
            ];
            field_rows.sort_by_key(|&(_, offset, _)| offset);

            println!("{} total size: {} bytes", stringify!($ty), size_of::<$ty>());
            for (field, offset, size) in field_rows {
                println!("  {field:<25}: offset {offset:4}, size {size:4}");
            }
        }};
    }

    /// Utility to aid in enum size and alignment.
    macro_rules! print_enum_layout {
        ($ty:ty, $($variant:ident($variant_ty:ty)),+$(,)?) => {{
            println!("{} enum: {} bytes", stringify!($ty), size_of::<$ty>());
                $(
                    println!("  {:<15}: size {:4}", stringify!($variant), size_of::<$variant_ty>());
                )+
        }}
    }

    // Utility to help print alignment and size of struct field for cache-optimization.
    #[test]
    fn print_layouts() {
        print_struct_layout!(
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

        print_struct_layout!(
            Bus,
            wram: Memory<ConstArray<u8, { bus::size::WRAM }>>,
            open_bus: u8,
            ram_state: RamState,
            region: NesRegion,
            ppu: Ppu,
            apu: Apu,
            input: Input,
            genie_codes: HashMap<u16, GenieCode>,
        );

        print_struct_layout!(
            Ppu,
            master_clock: u32,
            cycle: u16,
            scanline: u16,
            mask: Mask,
            ctrl: Ctrl,
            scroll: Scroll,
            tile_shift_lo: u16,
            tile_shift_hi: u16,
            tile_addr: u16,
            tile_lo: u8,
            tile_hi: u8,
            clock_divider: u8,
            open_bus: u8,
            reset_signal: bool,

            curr_palette: u8,
            prev_palette: u8,
            next_palette: u8,
            skip_rendering: bool,

            spr_count: u8,
            spr_in_range: bool,
            spr_zero_in_range: bool,
            spr_zero_visible: bool,
            oam_eval_done: bool,
            oamaddr: u8,
            oamaddr_lo: u8,
            oamaddr_hi: u8,
            secondary_oamaddr: u8,
            overflow_count: u8,
            oam_fetch: u8,

            vblank_scanline: u16,
            prerender_scanline: u16,
            is_visible_scanline: bool,
            is_prerender_scanline: bool,
            is_render_scanline: bool,
            is_pal_spr_eval_scanline: bool,

            status: PpuStatus,

            frame: Frame,
            ciram: CIRam,

            secondary_oamdata: ConstArray<u8, 32>,
            sprites: Box<[Sprite]>,
            spr_present: ConstArray<bool, 256>,
            oamdata: ConstArray<u8, 256>,

            palette: PaletteRam,
            mapper: Mapper,

            vram_buffer: u8,
            prevent_vbl: bool,
            region: NesRegion,
            emulate_warmup: bool,

            debugger: PpuDebugger,

        );

        print_struct_layout!(
            Apu,
            master_clock: u32,
            clock: u32,
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

        print_enum_layout!(
            Mapper,
            Nrom(Nrom),
            Sxrom(Sxrom),
            Uxrom(Uxrom),
            Cnrom(Cnrom),
            Txrom(Txrom),
            Exrom(Exrom),
            Axrom(Axrom),
            Pxrom(Pxrom),
            Fxrom(Fxrom),
            ColorDreams(ColorDreams),
            BandaiFCG(BandaiFCG),
            JalecoSs88006(JalecoSs88006),
            Namco163(Namco163),
            Vrc6(Vrc6),
            Bnrom(Bnrom),
            Nina001(Nina001),
            Gxrom(Gxrom),
            SunsoftFme7(SunsoftFme7),
            Bf909x(Bf909x),
            Nina003006(Nina003006),
        );
    }
}

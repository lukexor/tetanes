// use apu::APU;
// use controller::Controller;
// use image::RgbaImage;
use crate::cartridge::Cartridge;
use cpu::Cpu;
use memory::{Addr, Memory, MemoryMap, Ram};

// use memory::{read_ppu, readb};
// use ppu::PPU;
// use rom::Rom;
use std::{error::Error, fmt, fs, path::PathBuf};

// mod apu;
// mod controller;
mod cpu;
mod mapper;
mod memory;
// mod ppu;

const CPU_NUM_MAPS: usize = 11; // Ram, PPU, APU, I/O, Cartridge, Mirrors
const RAM_SIZE: usize = 0x0800; // 2K bytes
const MEMORY_SIZE: usize = 0xFFFF; // 64K bytes

type Cycles = u64;
type Frequency = f64;

// const FRAME_COUNTER_RATE: f64 = CPU_FREQUENCY / 240.0;

/// The NES Console
pub struct Console {
    cpu: Cpu,
    cpu_memory: MemoryMap,
    //     pub apu: Apu,
    //     pub ppu: Ppu,
    //     pub controller1: Controller,
    //     pub controller2: Controller,
    //     pub mapper: Option<Box<Mapper>>,
    //     pub trace: u8,
}

impl Console {
    pub fn new() -> Self {
        // http://wiki.nesdev.com/w/index.php/CPU_memory_map
        let mut cpu_memory = MemoryMap::new(CPU_NUM_MAPS);
        cpu_memory.map_mirrored(0x0000, 0x07FF, 0x0800, 0x1FFF, Box::new(Ram::new(RAM_SIZE)));
        // cpu.memory
        //     .map_mirrored(0x2000, 0x2007, 0x2008, 0x3FFF, PpuReg::new());
        // cpu.memory.map(0x4000, 0x4017, ApuIoMem::new())

        Self {
            cpu: Cpu::new(),
            cpu_memory: cpu_memory,
            //             apu: APU::new(),
            //             ppu: PPU::new(),
            //             mapper: None,
            //             controller1: Controller::new(),
            //             controller2: Controller::new(),
            //             ram: vec![0; RAM_SIZE],
            //             trace: 0,
        }
    }

    pub fn load_cartridge(&mut self, file: &PathBuf) -> Result<(), Box<Error>> {
        use mapper::Nrom;
        let cartridge = Cartridge::new(&file)?;
        // let mapper = mapper::new_mapper(cartridge)?;
        // self.cpu_memory.map(0x4020, 0xFFFF, mapper);
        self.cpu_memory
            .map(0x4020, 0xFFFF, Box::new(Nrom::new(cartridge)));

        // match &cartridge.mapper_num() {
        //     0 | 2 => self
        //         .cpu_memory
        //         .map(0x4020, 0xFFFF, Box::new(Nrom::new(cartridge))),
        //     _ => return Err(format!("unsupported mapper : {}", cartridge.mapper_num()).into()),
        // }

        self.reset();
        Ok(())
    }

    pub fn reset(&mut self) {
        self.cpu.reset(&mut self.cpu_memory);
    }

    fn set_pc(&mut self, addr: Addr) {
        self.cpu.set_pc(addr);
    }

    fn step_for(&mut self, steps: usize) {
        for _ in 0..steps {
            self.step();
        }
    }

    //     pub fn step_seconds(&mut self, seconds: f64) {
    //         // let mut cycles = (CPU_FREQUENCY * seconds) as u64;
    //         // while cycles > 0 {
    //         //     cycles = cycles.wrapping_sub(self.step());
    //         // }
    //     }

    pub fn step(&mut self) -> Cycles {
        let cpu_cycles = self.cpu.step(&mut self.cpu_memory);
        // for _ in 0..(cpu_cycles * 3) {
        //     self.step_ppu();
        //     self.step_mapper();
        // }
        // for _ in 0..cpu_cycles {
        //     self.step_apu();
        // }
        cpu_cycles
    }

    //     fn step_ppu(&mut self) {
    //         if self.ppu.tick() {
    //             self.cpu.trigger_nmi();
    //         }

    //         let rendering_enabled =
    //             self.ppu.flag_show_background != 0 || self.ppu.flag_show_sprites != 0;
    //         let pre_line = self.ppu.scan_line == 261;
    //         let visible_line = self.ppu.scan_line < 240;
    //         let render_line = pre_line || visible_line;
    //         let prefetch_cycle = self.ppu.cycle >= 321 && self.ppu.cycle <= 336;
    //         let visible_cycle = self.ppu.cycle >= 1 && self.ppu.cycle <= 256;
    //         let fetch_cycle = prefetch_cycle || visible_cycle;

    //         if rendering_enabled {
    //             if visible_line && visible_cycle {
    //                 self.ppu.render_pixel();
    //             }
    //             if render_line && fetch_cycle {
    //                 self.ppu.tile_data <<= 4;
    //                 match self.ppu.cycle % 8 {
    //                     0 => self.ppu.store_tile_data(),
    //                     1 => {
    //                         let addr = 0x2000 | (self.ppu.v & 0x0FFF);
    //                         self.ppu.name_table_byte = read_ppu(self, addr);
    //                     }
    //                     3 => {
    //                         let v = self.ppu.v;
    //                         let addr = 0x23C0 | (v & 0x0C00) | ((v >> 4) & 0x38) | ((v >> 2) & 0x07);
    //                         let shift = ((v >> 4) & 4) | (v & 2);
    //                         self.ppu.attribute_table_byte = ((read_ppu(self, addr) >> shift) & 3) << 2;
    //                     }
    //                     5 => {
    //                         self.ppu.low_tile_byte = read_ppu(self, self.ppu.get_tile_byte_addr());
    //                     }
    //                     7 => {
    //                         self.ppu.high_tile_byte =
    //                             read_ppu(self, self.ppu.get_tile_byte_addr().wrapping_add(8));
    //                     }
    //                     _ => (),
    //                 }
    //             }
    //             if pre_line && self.ppu.cycle >= 280 && self.ppu.cycle <= 304 {
    //                 self.ppu.copy_y();
    //             }
    //             if render_line {
    //                 if fetch_cycle && self.ppu.cycle % 8 == 0 {
    //                     self.ppu.increment_x();
    //                 }
    //                 if self.ppu.cycle == 256 {
    //                     self.ppu.increment_y();
    //                 }
    //                 if self.ppu.cycle == 257 {
    //                     self.ppu.copy_x();
    //                 }
    //             }
    //         }

    //         // sprite logic
    //         if rendering_enabled && self.ppu.cycle == 257 {
    //             if visible_line {
    //                 self.evaluate_ppu_sprites();
    //             } else {
    //                 self.ppu.sprite_count = 0;
    //             }
    //         }

    //         // vblank logic
    //         if self.ppu.scan_line == 241 && self.ppu.cycle == 1 {
    //             self.ppu.set_vertical_blank();
    //         }
    //         if pre_line && self.ppu.cycle == 1 {
    //             self.ppu.clear_vertical_blank();
    //             self.ppu.flag_sprite_zero_hit = 0;
    //             self.ppu.flag_sprite_overflow = 0;
    //         }
    //     }

    //     fn step_mapper(&mut self) {
    //         // match self.mapper.name() {
    //         //     "Mapper4" => {
    //         //         if self.ppu.cycle == 280
    //         //             && self.ppu.scan_line <= 239
    //         //             && self.ppu.scan_line >= 261
    //         //             && self.ppu.flag_show_background != 0
    //         //             && self.ppu.flag_show_sprites != 0
    //         //         {
    //         //             if self.mapper.counter == 0 {
    //         //                 self.mapper.counter = self.mapper.reload;
    //         //             } else {
    //         //                 self.mapper.counter -= 1;
    //         //                 if self.mapper.counter == 0 && self.mapper.irq_enable {
    //         //                     c.cpu.trigger_irq();
    //         //                 }
    //         //             }
    //         //         }
    //         //     }
    //         //     _ => (), // Do nothing
    //         // }
    //     }

    //     fn step_apu(&mut self) {
    //         let cycle1 = self.apu.cycle as f64;
    //         self.apu.cycle = self.apu.cycle.wrapping_add(1);
    //         let cycle2 = self.apu.cycle as f64;
    //         if self.apu.cycle % 2 == 0 {
    //             self.apu.pulse1.step_timer();
    //             self.apu.pulse2.step_timer();
    //             self.apu.noise.step_timer();
    //             self.step_dmc_timer();
    //         }
    //         self.apu.triangle.step_timer();
    //         // let frame1 = (cycle1 / FRAME_COUNTER_RATE) as isize;
    //         // let frame2 = (cycle2 / FRAME_COUNTER_RATE) as isize;
    //         // if frame1 != frame2 {
    //         //     // mode 0:    mode 1:       function
    //         //     // ---------  -----------  -----------------------------
    //         //     //  - - - f    - - - - -    IRQ (if bit 6 is clear)
    //         //     //  - l - l    l - l - -    Length counter and sweep
    //         //     //  e e e e    e e e e -    Envelope and linear counter
    //         //     match self.apu.frame_period {
    //         //         4 => {
    //         //             self.apu.frame_value = self.apu.frame_value.wrapping_add(1) % 4;
    //         //             self.apu.step_envelope();
    //         //             if self.apu.frame_value % 2 != 0 {
    //         //                 self.apu.step_sweep();
    //         //                 self.apu.step_length();
    //         //             }
    //         //             if self.apu.frame_value == 3 && self.apu.frame_irq {
    //         //                 self.cpu.trigger_irq();
    //         //             }
    //         //         }
    //         //         5 => {
    //         //             self.apu.frame_value = self.apu.frame_value.wrapping_add(1) % 5;
    //         //             self.apu.step_envelope();
    //         //             if self.apu.frame_value % 2 != 0 {
    //         //                 self.apu.step_sweep();
    //         //                 self.apu.step_length();
    //         //             }
    //         //         }
    //         //         _ => (),
    //         //     }
    //         // }
    //         let sample_rate = f64::from(self.apu.sample_rate);
    //         let sample1 = (cycle1 / sample_rate) as isize;
    //         let sample2 = (cycle2 / sample_rate) as isize;
    //         if sample1 != sample2 {
    //             self.apu.send_sample()
    //         }
    //     }

    //     fn step_dmc_timer(&mut self) {
    //         if self.apu.dmc.enabled {
    //             if self.apu.dmc.current_length > 0 && self.apu.dmc.bit_count == 0 {
    //                 self.cpu.stall += 4;
    //                 self.apu.dmc.shift_register = readb(self, self.apu.dmc.current_address);
    //                 self.apu.dmc.bit_count = 8;
    //                 self.apu.dmc.current_address += 1;
    //                 if self.apu.dmc.current_address == 0 {
    //                     self.apu.dmc.current_address = 0x8000;
    //                 }
    //                 self.apu.dmc.current_length -= 1;
    //                 if self.apu.dmc.current_length == 0 && self.apu.dmc.loops {
    //                     self.apu.dmc.restart();
    //                 }
    //             }

    //             if self.apu.dmc.tick_value == 0 {
    //                 self.apu.dmc.tick_value = self.apu.dmc.tick_period;

    //                 if self.apu.dmc.bit_count != 0 {
    //                     if self.apu.dmc.shift_register & 1 == 1 {
    //                         if self.apu.dmc.value <= 125 {
    //                             self.apu.dmc.value += 2;
    //                         }
    //                     } else if self.apu.dmc.value >= 2 {
    //                         self.apu.dmc.value -= 2;
    //                     }
    //                     self.apu.dmc.shift_register >>= 1;
    //                     self.apu.dmc.bit_count -= 1;
    //                 }
    //             } else {
    //                 self.apu.dmc.tick_value -= 1;
    //             }
    //         }
    //     }

    //     pub fn set_audio_channel(&mut self) {
    //         unimplemented!();
    //     }

    //     fn evaluate_ppu_sprites(&mut self) {
    //         let height = if self.ppu.flag_sprite_size == 0 {
    //             8
    //         } else {
    //             16
    //         };
    //         let mut count: usize = 0;
    //         for i in 0u8..64 {
    //             let y = self.ppu.oam_data[(i * 4) as usize];
    //             let a = self.ppu.oam_data[(i * 4 + 2) as usize];
    //             let x = self.ppu.oam_data[(i * 4 + 3) as usize];
    //             let row = self.ppu.scan_line.wrapping_sub(u32::from(y));
    //             if row >= height {
    //                 continue;
    //             }
    //             if count < 8 {
    //                 self.ppu.sprite_patterns[count] = self.fetch_ppu_sprite_pattern(i, row);
    //                 self.ppu.sprite_positions[count] = x;
    //                 self.ppu.sprite_priorities[count] = (a >> 5) & 1;
    //                 self.ppu.sprite_indexes[count] = i;
    //             }
    //             count += 1;
    //         }
    //         if count > 8 {
    //             count = 8;
    //             self.ppu.flag_sprite_overflow = 1;
    //         }
    //         self.ppu.sprite_count = count;
    //     }

    //     fn fetch_ppu_sprite_pattern(&mut self, i: u8, mut row: u32) -> u32 {
    //         let mut tile = self.ppu.oam_data[(i * 4 + 1) as usize];
    //         let attributes = self.ppu.oam_data[(i * 4 + 2) as usize];
    //         let addr = if self.ppu.flag_sprite_size == 0 {
    //             if attributes & 0x80 == 0x80 {
    //                 row = 7 - row;
    //             }
    //             0x1000 * u16::from(self.ppu.flag_sprite_table) + u16::from(tile) * 16 + (row as u16)
    //         } else {
    //             if attributes & 0x80 == 0x80 {
    //                 row = 15 - row;
    //             }
    //             let table = tile & 1;
    //             tile &= 0xFE;
    //             if row > 7 {
    //                 tile += 1;
    //                 row -= 8;
    //             }
    //             0x1000 * u16::from(table) + u16::from(tile) * 16 + (row as u16)
    //         };
    //         let a = (attributes & 3) << 2;
    //         let mut low_tile_byte = read_ppu(self, addr);
    //         let mut high_tile_byte = read_ppu(self, addr.wrapping_add(8));
    //         let mut data: u32 = 0;
    //         for _ in 0..8 {
    //             let (p1, p2): (u8, u8);
    //             if attributes & 0x40 == 0x40 {
    //                 p1 = low_tile_byte & 1;
    //                 p2 = (high_tile_byte & 1) << 1;
    //                 low_tile_byte >>= 1;
    //                 high_tile_byte >>= 1;
    //             } else {
    //                 p1 = (low_tile_byte & 0x80) >> 7;
    //                 p2 = (high_tile_byte & 0x80) >> 6;
    //                 low_tile_byte <<= 1;
    //                 high_tile_byte <<= 1;
    //             }
    //             data <<= 4;
    //             data |= u32::from(a | p1 | p2);
    //         }
    //         data
    //     }

    //     pub fn load_sram(&mut self, path: &PathBuf) -> Result<(), Box<Error>> {
    //         // TODO fix endianness
    //         // let data = fs::read(PathBuf::from(path))?;
    //         // self.rom.sram = data;
    //         Ok(())
    //     }

    //     pub fn save_sram(&mut self, path: &PathBuf) -> Result<(), Box<Error>> {
    //         // TODO Ensure directories exist
    //         // TODO fix endianness
    //         // fs::write(path, &self.rom.sram)?;
    //         Ok(())
    //     }

    //     pub fn buffer(&self) -> RgbaImage {
    //         let image = self.ppu.front.clone();
    //         image
    //     }
}

impl fmt::Debug for Console {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(
            f,
            "Console {{\n  cpu: {:?}\n  cpu_memory: {:?}\n}} ",
            self.cpu, self.cpu_memory
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const NESTEST_ADDR: Addr = 0xC000;
    const NESTEST_LEN: usize = 8991;
    const ROMS: &[&str] = &[
        "roms/Zelda II - The Adventure of Link (USA).nes",
        "roms/Super Mario Bros. (World).nes",
        "roms/Metroid (USA).nes",
        "roms/Gauntlet (USA).nes",
    ];

    fn new_game_console(index: usize) -> Console {
        let mut console = Console::new();
        console
            .load_cartridge(&PathBuf::from(ROMS[index]))
            .expect("cartridge loaded");
        console
    }

    #[test]
    fn test_cpu_memory() {
        let mut c = Console::new();
        let mut mem = c.cpu_memory;
        // let mut mem = c.cpu_memory.take().unwrap();
        mem.writeb(0x0005, 0x0015);
        mem.writeb(0x0015, 0x0050);
        mem.writeb(0x0016, 0x0025);

        assert_eq!(mem.readb(0x0008), 0x0000, "read uninitialized byte");
        assert_eq!(mem.readw(0x0008), 0x0000, "read uninitialized word");
        assert_eq!(mem.readb(0x0005), 0x0015, "read initialized byte");
        assert_eq!(mem.readw(0x0015), 0x2550, "read initialized word");
        assert_eq!(mem.readb(0x0808), 0x0000, "read uninitialized mirror1 byte");
        assert_eq!(mem.readw(0x0808), 0x0000, "read uninitialized mirror1 word");
        assert_eq!(mem.readb(0x0805), 0x0015, "read initialized mirror1 byte");
        assert_eq!(mem.readw(0x0815), 0x2550, "read initialized mirror1 word");
        assert_eq!(mem.readb(0x1008), 0x0000, "read uninitialized mirror2 byte");
        assert_eq!(mem.readw(0x1008), 0x0000, "read uninitialized mirror2 word");
        assert_eq!(mem.readb(0x1005), 0x0015, "read initialized mirror2 byte");
        assert_eq!(mem.readw(0x1015), 0x2550, "read initialized mirror2 word");
        // The following are test mode addresses, Not mapped
        assert_eq!(mem.readb(0x0418), 0x0000, "read unmapped byte");
        assert_eq!(mem.readb(0x0418), 0x0000, "write unmapped byte");
        assert_eq!(mem.readw(0x0418), 0x0000, "read unmapped word");
        assert_eq!(mem.readw(0x0418), 0x0000, "read unmapped word");
        // Reading a word from the max address should wrap
        assert_eq!(mem.readw(0xFFFF), 0x0000, "read max word");
    }

    #[test]
    #[should_panic]
    fn test_cpu_memory_invalid_map() {
        let mut c = Console::new();
        let mut mem = c.cpu_memory;
        // RAM should already be mapped to 0x0000..=0x07FF
        mem.map(0x0010, 0x1000, Box::new(Ram::new(RAM_SIZE)));
    }

    #[test]
    fn test_nestest() {
        let rom = "tests/nestest.nes";
        let cpu_log = "logs/cpu.log";
        let nestest_log = "tests/nestest.txt";

        let rom_path = PathBuf::from(rom);
        let mut c = Console::new();
        let err = c.load_cartridge(&rom_path);
        assert!(err.is_ok());

        eprintln!("{:?}", c);
        c.set_pc(NESTEST_ADDR);
        c.step_for(NESTEST_LEN);

        let nestest = fs::read_to_string(nestest_log);
        // let cpu_result = fs::read_to_string(cpu_log);
        assert!(nestest.is_ok());
        // assert!(cpu_result.is_ok());

        // assert!(cpu_result.unwrap() == nestest.unwrap());
        fs::write(cpu_log, &c.cpu.oplog).expect("Failed to write op.log");
        assert!(c.cpu.oplog == nestest.unwrap());
    }
}

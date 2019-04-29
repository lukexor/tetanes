use apu::APU;
use controller::Controller;
use cpu::{execute, php, push_stackw, readw, Interrupt, CPU, CPU_FREQUENCY};
use image::RgbaImage;
use mapper::Mapper;
use memory::{read_ppu, readb};
use ppu::PPU;
use rom::Rom;
use std::{error::Error, fs, path::PathBuf};

mod apu;
mod controller;
mod cpu;
mod mapper;
mod memory;
mod ppu;
mod rom;

const RAM_SIZE: usize = 2048;
const FRAME_COUNTER_RATE: f64 = CPU_FREQUENCY / 240.0;

/// The NES Console
pub struct Console {
    pub cpu: CPU,
    pub apu: APU,
    pub ppu: PPU,
    pub controller1: Controller,
    pub controller2: Controller,
    pub mapper: Box<Mapper>,
    pub ram: Vec<u8>,
    pub trace: u8,
}

impl Console {
    pub fn new(rom: &PathBuf) -> Result<Self, Box<Error>> {
        let rom = Rom::new(rom)?;
        let mapper = mapper::new_mapper(rom)?;
        let mut console = Self {
            cpu: CPU::new(),
            apu: APU::new(),
            ppu: PPU::new(),
            mapper,
            controller1: Controller::new(),
            controller2: Controller::new(),
            ram: vec![0; RAM_SIZE],
            trace: 0,
        };
        console.reset();
        Ok(console)
    }

    pub fn reset(&mut self) {
        self.cpu.pc = readw(self, 0xFFFC);
        self.cpu.reset();
    }

    pub fn step_seconds(&mut self, seconds: f64) {
        let mut cycles = (CPU_FREQUENCY * seconds) as u64;
        while cycles > 0 {
            cycles = cycles.wrapping_sub(self.step());
        }
    }

    fn step(&mut self) -> u64 {
        let cpu_cycles = if self.cpu.stall > 0 {
            self.cpu.stall -= 1;
            1
        } else {
            let start_cycles = self.cpu.cycles;
            match &self.cpu.interrupt {
                Interrupt::NMI => {
                    push_stackw(self, self.cpu.pc);
                    php(self);
                    self.cpu.pc = readw(self, 0xFFFA);
                    self.cpu.interrupt_disable = true;
                    self.cpu.cycles = self.cpu.cycles.wrapping_add(7);
                }
                Interrupt::IRQ => {
                    push_stackw(self, self.cpu.pc);
                    php(self);
                    self.cpu.pc = readw(self, 0xFFFE);
                    self.cpu.interrupt_disable = true;
                    self.cpu.cycles = self.cpu.cycles.wrapping_add(7);
                }
                _ => (),
            }
            self.cpu.interrupt = Interrupt::None;
            let opcode = readb(self, self.cpu.pc);
            execute(self, opcode);
            (self.cpu.cycles - start_cycles)
        };
        for _ in 0..cpu_cycles * 3 {
            self.step_ppu();
            self.step_mapper();
        }
        for _ in 0..cpu_cycles {
            self.step_apu();
        }
        cpu_cycles
    }

    fn step_ppu(&mut self) {
        if self.ppu.tick() {
            self.cpu.trigger_nmi();
        }

        let rendering_enabled =
            self.ppu.flag_show_background != 0 || self.ppu.flag_show_sprites != 0;
        let pre_line = self.ppu.scan_line == 261;
        let visible_line = self.ppu.scan_line < 240;
        let render_line = pre_line || visible_line;
        let prefetch_cycle = self.ppu.cycle >= 321 && self.ppu.cycle <= 336;
        let visible_cycle = self.ppu.cycle >= 1 && self.ppu.cycle <= 256;
        let fetch_cycle = prefetch_cycle || visible_cycle;

        if rendering_enabled {
            if visible_line && visible_cycle {
                self.ppu.render_pixel();
            }
            if render_line && fetch_cycle {
                self.ppu.tile_data <<= 4;
                match self.ppu.cycle % 8 {
                    0 => self.ppu.store_tile_data(),
                    1 => {
                        let addr = 0x2000 | (self.ppu.v & 0x0FFF);
                        self.ppu.name_table_byte = read_ppu(self, addr);
                    }
                    3 => {
                        let v = self.ppu.v;
                        let addr = 0x23C0 | (v & 0x0C00) | ((v >> 4) & 0x38) | ((v >> 2) & 0x07);
                        let shift = ((v >> 4) & 4) | (v & 2);
                        self.ppu.attribute_table_byte = ((read_ppu(self, addr) >> shift) & 3) << 2;
                    }
                    5 => {
                        self.ppu.low_tile_byte = read_ppu(self, self.ppu.get_tile_byte_addr());
                    }
                    7 => {
                        self.ppu.high_tile_byte =
                            read_ppu(self, self.ppu.get_tile_byte_addr().wrapping_add(8));
                    }
                    _ => (),
                }
            }
            if pre_line && self.ppu.cycle >= 280 && self.ppu.cycle <= 304 {
                self.ppu.copy_y();
            }
            if render_line {
                if fetch_cycle && self.ppu.cycle % 8 == 0 {
                    self.ppu.increment_x();
                }
                if self.ppu.cycle == 256 {
                    self.ppu.increment_y();
                }
                if self.ppu.cycle == 257 {
                    self.ppu.copy_x();
                }
            }
        }

        // sprite logic
        if rendering_enabled && self.ppu.cycle == 257 {
            if visible_line {
                self.evaluate_ppu_sprites();
            } else {
                self.ppu.sprite_count = 0;
            }
        }

        // vblank logic
        if self.ppu.scan_line == 241 && self.ppu.cycle == 1 {
            self.ppu.set_vertical_blank();
        }
        if pre_line && self.ppu.cycle == 1 {
            self.ppu.clear_vertical_blank();
            self.ppu.flag_sprite_zero_hit = 0;
            self.ppu.flag_sprite_overflow = 0;
        }
    }

    fn step_mapper(&mut self) {
        // match self.mapper.name() {
        //     "Mapper4" => {
        //         if self.ppu.cycle == 280
        //             && self.ppu.scan_line <= 239
        //             && self.ppu.scan_line >= 261
        //             && self.ppu.flag_show_background != 0
        //             && self.ppu.flag_show_sprites != 0
        //         {
        //             if self.mapper.counter == 0 {
        //                 self.mapper.counter = self.mapper.reload;
        //             } else {
        //                 self.mapper.counter -= 1;
        //                 if self.mapper.counter == 0 && self.mapper.irq_enable {
        //                     c.cpu.trigger_irq();
        //                 }
        //             }
        //         }
        //     }
        //     _ => (), // Do nothing
        // }
    }

    fn step_apu(&mut self) {
        let cycle1 = self.apu.cycle as f64;
        self.apu.cycle = self.apu.cycle.wrapping_add(1);
        let cycle2 = self.apu.cycle as f64;
        if self.apu.cycle % 2 == 0 {
            self.apu.pulse1.step_timer();
            self.apu.pulse2.step_timer();
            self.apu.noise.step_timer();
            self.step_dmc_timer();
        }
        self.apu.triangle.step_timer();
        let frame1 = (cycle1 / FRAME_COUNTER_RATE) as isize;
        let frame2 = (cycle2 / FRAME_COUNTER_RATE) as isize;
        if frame1 != frame2 {
            // mode 0:    mode 1:       function
            // ---------  -----------  -----------------------------
            //  - - - f    - - - - -    IRQ (if bit 6 is clear)
            //  - l - l    l - l - -    Length counter and sweep
            //  e e e e    e e e e -    Envelope and linear counter
            match self.apu.frame_period {
                4 => {
                    self.apu.frame_value = self.apu.frame_value.wrapping_add(1) % 4;
                    self.apu.step_envelope();
                    if self.apu.frame_value % 2 != 0 {
                        self.apu.step_sweep();
                        self.apu.step_length();
                    }
                    if self.apu.frame_value == 3 && self.apu.frame_irq {
                        self.cpu.trigger_irq();
                    }
                }
                5 => {
                    self.apu.frame_value = self.apu.frame_value.wrapping_add(1) % 5;
                    self.apu.step_envelope();
                    if self.apu.frame_value % 2 != 0 {
                        self.apu.step_sweep();
                        self.apu.step_length();
                    }
                }
                _ => (),
            }
        }
        let sample_rate = f64::from(self.apu.sample_rate);
        let sample1 = (cycle1 / sample_rate) as isize;
        let sample2 = (cycle2 / sample_rate) as isize;
        if sample1 != sample2 {
            self.apu.send_sample()
        }
    }

    fn step_dmc_timer(&mut self) {
        if self.apu.dmc.enabled {
            if self.apu.dmc.current_length > 0 && self.apu.dmc.bit_count == 0 {
                self.cpu.stall += 4;
                self.apu.dmc.shift_register = readb(self, self.apu.dmc.current_address);
                self.apu.dmc.bit_count = 8;
                self.apu.dmc.current_address += 1;
                if self.apu.dmc.current_address == 0 {
                    self.apu.dmc.current_address = 0x8000;
                }
                self.apu.dmc.current_length -= 1;
                if self.apu.dmc.current_length == 0 && self.apu.dmc.loops {
                    self.apu.dmc.restart();
                }
            }

            if self.apu.dmc.tick_value == 0 {
                self.apu.dmc.tick_value = self.apu.dmc.tick_period;

                if self.apu.dmc.bit_count != 0 {
                    if self.apu.dmc.shift_register & 1 == 1 {
                        if self.apu.dmc.value <= 125 {
                            self.apu.dmc.value += 2;
                        }
                    } else if self.apu.dmc.value >= 2 {
                        self.apu.dmc.value -= 2;
                    }
                    self.apu.dmc.shift_register >>= 1;
                    self.apu.dmc.bit_count -= 1;
                }
            } else {
                self.apu.dmc.tick_value -= 1;
            }
        }
    }

    pub fn set_audio_channel(&mut self) {
        unimplemented!();
    }

    fn evaluate_ppu_sprites(&mut self) {
        let height = if self.ppu.flag_sprite_size == 0 {
            8
        } else {
            16
        };
        let mut count: usize = 0;
        for i in 0u8..64 {
            let y = self.ppu.oam_data[(i * 4) as usize];
            let a = self.ppu.oam_data[(i * 4 + 2) as usize];
            let x = self.ppu.oam_data[(i * 4 + 3) as usize];
            let row = self.ppu.scan_line.wrapping_sub(u32::from(y));
            if row >= height {
                continue;
            }
            if count < 8 {
                self.ppu.sprite_patterns[count] = self.fetch_ppu_sprite_pattern(i, row);
                self.ppu.sprite_positions[count] = x;
                self.ppu.sprite_priorities[count] = (a >> 5) & 1;
                self.ppu.sprite_indexes[count] = i;
            }
            count += 1;
        }
        if count > 8 {
            count = 8;
            self.ppu.flag_sprite_overflow = 1;
        }
        self.ppu.sprite_count = count;
    }

    fn fetch_ppu_sprite_pattern(&mut self, i: u8, mut row: u32) -> u32 {
        let mut tile = self.ppu.oam_data[(i * 4 + 1) as usize];
        let attributes = self.ppu.oam_data[(i * 4 + 2) as usize];
        let addr = if self.ppu.flag_sprite_size == 0 {
            if attributes & 0x80 == 0x80 {
                row = 7 - row;
            }
            0x1000 * u16::from(self.ppu.flag_sprite_table) + u16::from(tile) * 16 + (row as u16)
        } else {
            if attributes & 0x80 == 0x80 {
                row = 15 - row;
            }
            let table = tile & 1;
            tile &= 0xFE;
            if row > 7 {
                tile += 1;
                row -= 8;
            }
            0x1000 * u16::from(table) + u16::from(tile) * 16 + (row as u16)
        };
        let a = (attributes & 3) << 2;
        let mut low_tile_byte = read_ppu(self, addr);
        let mut high_tile_byte = read_ppu(self, addr.wrapping_add(8));
        let mut data: u32 = 0;
        for _ in 0..8 {
            let (p1, p2): (u8, u8);
            if attributes & 0x40 == 0x40 {
                p1 = low_tile_byte & 1;
                p2 = (high_tile_byte & 1) << 1;
                low_tile_byte >>= 1;
                high_tile_byte >>= 1;
            } else {
                p1 = (low_tile_byte & 0x80) >> 7;
                p2 = (high_tile_byte & 0x80) >> 6;
                low_tile_byte <<= 1;
                high_tile_byte <<= 1;
            }
            data <<= 4;
            data |= u32::from(a | p1 | p2);
        }
        data
    }

    pub fn load_sram(&mut self, path: &PathBuf) -> Result<(), Box<Error>> {
        // TODO fix endianness
        // let data = fs::read(PathBuf::from(path))?;
        // self.rom.sram = data;
        Ok(())
    }

    pub fn save_sram(&mut self, path: &PathBuf) -> Result<(), Box<Error>> {
        // TODO Ensure directories exist
        // TODO fix endianness
        // fs::write(path, &self.rom.sram)?;
        Ok(())
    }

    pub fn buffer(&self) -> RgbaImage {
        self.ppu.front.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory::writeb;
    use std::fs;

    fn new_console() -> Console {
        let rom = "roms/Zelda II - The Adventure of Link (USA).nes";
        let rom_path = PathBuf::from(rom);
        Console::new(&rom_path).expect("valid console")
    }

    #[test]
    fn test_new_console() {
        let c = new_console();
        assert_eq!(c.ram.len(), RAM_SIZE);
    }

    #[test]
    fn test_nestest() {
        let rom = "roms/nestest.nes";
        let rom_path = PathBuf::from(rom);
        let mut console = Console::new(&rom_path).unwrap();
        console.trace = 1;
        console.cpu.pc = 0xC000;
        for _ in 0..8997 {
            console.step();
        }
        fs::write("tests/op.log", &console.cpu.oplog).expect("Failed to write op.log");
        let nestest = fs::read_to_string("tests/nestest.txt").expect("Failed to read nestest.txt");
        assert!(console.cpu.oplog == nestest);
    }

    #[test]
    fn test_console_step_seconds() {
        // TODO
    }

    #[test]
    fn test_console_stall() {
        // TODO
    }

    #[test]
    fn test_console_nmi_interrupt() {
        // TODO
    }

    #[test]
    fn test_console_irq_interrupt() {
        // TODO
    }

    #[test]
    // fn test_console_sound() {
    //     let mut c = new_console();
    //     // Test basic control flow by playing audio
    //     //   lda #$01 ; square 1 (opcode 161)
    //     //   sta $4015 (opcode 129)
    //     //   lda #$08 ; period low
    //     //   sta $4002
    //     //   lda #$02 ; period high
    //     //   sta $4003
    //     //   lda #$bf ; volume
    //     //   sta $4000

    //     // Load program into ram
    //     let start_addr = 0x0100;
    //     let lda = 161;
    //     let sta = 129;
    //     let jmp = 76;
    //     c.cpu.pc = start_addr;

    //     // Square 1
    //     writeb(&mut c, start_addr, lda);
    //     writeb(&mut c, start_addr + 1, 0x0001);
    //     writeb(&mut c, 0x0001, 0x0001);

    //     writeb(&mut c, start_addr + 2, sta);
    //     writeb(&mut c, start_addr + 3, 0x0003);
    //     writeb(&mut c, 0x0003, 0x0015);
    //     writeb(&mut c, 0x0004, 0x0040);

    //     // Period Low
    //     writeb(&mut c, start_addr + 4, lda);
    //     writeb(&mut c, start_addr + 5, 0x0005);
    //     writeb(&mut c, 0x0005, 0x0008);
    //     writeb(&mut c, start_addr + 6, sta);
    //     writeb(&mut c, start_addr + 7, 0x0007);
    //     writeb(&mut c, 0x0007, 0x0002);
    //     writeb(&mut c, 0x0008, 0x0040);

    //     // Period High
    //     writeb(&mut c, start_addr + 8, lda);
    //     writeb(&mut c, start_addr + 9, 0x0009);
    //     writeb(&mut c, 0x0009, 0x0002);
    //     writeb(&mut c, start_addr + 10, sta);
    //     writeb(&mut c, start_addr + 11, 0x0011);
    //     writeb(&mut c, 0x0011, 0x0003);
    //     writeb(&mut c, 0x0012, 0x0040);

    //     // Volume
    //     writeb(&mut c, start_addr + 12, lda);
    //     writeb(&mut c, start_addr + 13, 0x0013);
    //     writeb(&mut c, 0x0013, 0x00BF);
    //     writeb(&mut c, start_addr + 14, sta);
    //     writeb(&mut c, start_addr + 15, 0x0015);
    //     writeb(&mut c, 0x0015, 0x0000);
    //     writeb(&mut c, 0x0016, 0x0040);

    //     // jmp forever
    //     writeb(&mut c, start_addr + 16, jmp);
    //     writeb(&mut c, start_addr + 17, ((start_addr + 16) & 0xFF) as u8);
    //     writeb(&mut c, start_addr + 17, ((start_addr + 16) >> 8) as u8);

    //     // set pc to start address
    //     // step cpu 8 times
    //     for _ in 0..8 {
    //         c.step();
    //     }
    //     // Verify state
    // }
    #[test]
    fn test_console_load_state() {
        // TODO
    }

    #[test]
    fn test_console_load_sram() {
        // TODO
    }

    #[test]
    fn test_console_save_sram() {
        // TODO
    }
}

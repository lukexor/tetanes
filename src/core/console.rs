use super::{
    apu::APU,
    cartridge::Cartridge,
    controller::Controller,
    cpu::{Interrupt, CPU, CPU_FREQUENCY},
    cpu_instructions::{execute, php, print_instruction},
    mapper::{new_mapper, Mapper},
    memory::{push16, read16, read_byte},
    ppu::PPU,
};
use std::{error::Error, fs, path::PathBuf};

const RAM_SIZE: usize = 2048;

/// The NES Console
pub struct Console {
    pub cpu: CPU,
    pub apu: APU,
    pub ppu: PPU,
    pub cartridge: Cartridge,
    pub controller1: Controller,
    pub controller2: Controller,
    pub mapper: Box<Mapper>,
    pub ram: Vec<u8>,
}

impl Console {
    pub fn new(rom: &PathBuf) -> Result<Self, Box<Error>> {
        let cartridge = Cartridge::new(rom)?;
        let mapper = new_mapper(cartridge.mapper, cartridge.prg.len())?;
        let mut console = Self {
            cpu: CPU::new(),
            apu: APU::new(),
            ppu: PPU::new(),
            cartridge,
            mapper,
            controller1: Controller::new(),
            controller2: Controller::new(),
            ram: vec![0; RAM_SIZE],
        };
        console.reset();
        Ok(console)
    }

    pub fn reset(&mut self) {
        self.cpu.pc = read_byte(self, 0xFFFC).into();
        self.cpu.sp = 0xFD;
        self.cpu.set_flags(0x24);
    }

    pub fn step_seconds(&mut self, seconds: f64) {
        let mut cycles = (CPU_FREQUENCY * seconds) as u64;
        while cycles > 0 {
            cycles -= self.step();
        }
    }

    fn step(&mut self) -> u64 {
        print_instruction(self);
        let cpu_cycles = if self.cpu.stall > 0 {
            self.cpu.stall -= 1;
            1
        } else {
            let start_cycles = self.cpu.cycles;
            match &self.cpu.interrupt {
                Interrupt::NMI => {
                    push16(self, self.cpu.pc);
                    php(self);
                    self.cpu.pc = read16(self, 0xFFFA);
                    self.cpu.i = 1;
                    self.cpu.cycles += 7;
                }
                Interrupt::IRQ => {
                    push16(self, self.cpu.pc);
                    php(self);
                    self.cpu.pc = read16(self, 0xFFFE);
                    self.cpu.i = 1;
                    self.cpu.cycles += 7;
                }
                _ => (),
            }
            self.cpu.interrupt = Interrupt::None;
            let opcode = read_byte(self, self.cpu.pc);
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
        // self.ppu.tick(self);

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
                    // 1 => self.ppu.fetch_name_table_byte(self),
                    // 3 => self.ppu.fetch_attr_table_byte(self),
                    // 5 => self.ppu.fetch_low_tile_byte(self),
                    // 7 => self.ppu.fetch_high_tile_byte(self),
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
                // self.ppu.evaluate_sprites(self);
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

    fn step_apu(&mut self) {}

    pub fn set_audio_channel(&mut self) {
        unimplemented!();
    }

    pub fn load_sram(&mut self, path: &PathBuf) -> Result<(), Box<Error>> {
        // TODO fix endianness
        let data = fs::read(PathBuf::from(path))?;
        self.cartridge.sram = data;
        Ok(())
    }

    pub fn save_sram(&mut self, path: &PathBuf) -> Result<(), Box<Error>> {
        // TODO Ensure directories exist
        // TODO fix endianness
        fs::write(path, &self.cartridge.sram)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::memory::write_byte;

    fn new_console() -> Console {
        let rom = "roms/Zelda II - The Adventure of Link (USA).nes";
        let rom_path = PathBuf::from(rom);
        Console::new(&rom_path).expect("valid console")
    }

    #[test]
    fn test_new_console() {
        let c = new_console();
        assert_eq!(c.cartridge.prg.len(), 131_072);
        assert_eq!(c.cartridge.chr.len(), 131_072);
        assert_eq!(c.cartridge.sram.len(), 8192);
        assert_eq!(c.cartridge.mapper, 1);
        assert_eq!(c.cartridge.mirror, 0);
        assert_eq!(c.cartridge.battery, 1);
        assert_eq!("Mapper1", c.mapper.name());
        assert_eq!(c.ram.len(), RAM_SIZE);
        assert_eq!(c.cpu.pc, 112);
        assert_eq!(c.cpu.sp, 0xFD);
        assert_eq!(c.cpu.flags(), 0x24);
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
    fn test_console_sound() {
        let mut c = new_console();
        // Test basic control flow by playing audio
        //   lda #$01 ; square 1 (opcode 161)
        //   sta $4015 (opcode 129)
        //   lda #$08 ; period low
        //   sta $4002
        //   lda #$02 ; period high
        //   sta $4003
        //   lda #$bf ; volume
        //   sta $4000

        // Load program into ram
        let start_addr = 0x0100;
        let lda = 161;
        let sta = 129;
        let jmp = 76;
        c.cpu.pc = start_addr;

        // Square 1
        write_byte(&mut c, start_addr, lda);
        write_byte(&mut c, start_addr + 1, 0x0001);
        write_byte(&mut c, 0x0001, 0x0001);

        write_byte(&mut c, start_addr + 2, sta);
        write_byte(&mut c, start_addr + 3, 0x0003);
        write_byte(&mut c, 0x0003, 0x0015);
        write_byte(&mut c, 0x0004, 0x0040);

        // Period Low
        write_byte(&mut c, start_addr + 4, lda);
        write_byte(&mut c, start_addr + 5, 0x0005);
        write_byte(&mut c, 0x0005, 0x0008);
        write_byte(&mut c, start_addr + 6, sta);
        write_byte(&mut c, start_addr + 7, 0x0007);
        write_byte(&mut c, 0x0007, 0x0002);
        write_byte(&mut c, 0x0008, 0x0040);

        // Period High
        write_byte(&mut c, start_addr + 8, lda);
        write_byte(&mut c, start_addr + 9, 0x0009);
        write_byte(&mut c, 0x0009, 0x0002);
        write_byte(&mut c, start_addr + 10, sta);
        write_byte(&mut c, start_addr + 11, 0x0011);
        write_byte(&mut c, 0x0011, 0x0003);
        write_byte(&mut c, 0x0012, 0x0040);

        // Volume
        write_byte(&mut c, start_addr + 12, lda);
        write_byte(&mut c, start_addr + 13, 0x0013);
        write_byte(&mut c, 0x0013, 0x00BF);
        write_byte(&mut c, start_addr + 14, sta);
        write_byte(&mut c, start_addr + 15, 0x0015);
        write_byte(&mut c, 0x0015, 0x0000);
        write_byte(&mut c, 0x0016, 0x0040);

        // jmp forever
        write_byte(&mut c, start_addr + 16, jmp);
        write_byte(&mut c, start_addr + 17, ((start_addr + 16) & 0xFF) as u8);
        write_byte(&mut c, start_addr + 17, ((start_addr + 16) >> 8) as u8);

        // set pc to start address
        // step cpu 8 times
        for _ in 0..8 {
            c.step();
        }
        // Verify state
    }

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

/// CNROM Board (Mapper 3)
///
/// https://wiki.nesdev.com/w/index.php/CNROM
/// https://wiki.nesdev.com/w/index.php/INES_Mapper_003

#[derive(Debug)]
pub struct Cnrom {
    cart: Cartridge,
    chr_bank: u16, // $0000-$1FFF 8K CHR-ROM
    prg_bank_1: u16,
    prg_bank_2: u16,
}

impl Cnrom {
    pub fn load(cart: Cartridge) -> Self {
        let prg_bank_2 = (cart.header.prg_size - 1) as u16;
        Self {
            cart,
            chr_bank: 016,
            prg_bank_1: 016,
            prg_bank_2,
        }
    }
}

impl Memory for Cnrom {
    fn readb(&mut self, addr: u16) -> u8 {
        match addr {
            // $0000-$1FFF PPU
            0x0000..=0x1FFF => {
                let addr = self.chr_bank * 0x2000 + addr;
                if self.cart.header.chr_size == 0 {
                    self.cart.prg_rom[addr as usize]
                } else {
                    self.cart.chr_rom[addr as usize]
                }
            }
            0x6000..=0x7FFF => self.cart.prg_ram[(addr - 0x6000) as usize],
            // $8000-$FFFF CPU
            0x8000..=0xBFFF => {
                let addr = self.prg_bank_1 * 0x4000 + (addr - 0x8000);
                self.cart.prg_rom[addr as usize]
            }
            0xC000..=0xFFFF => {
                let addr = self.prg_bank_2 * 0x4000 + (addr - 0xC000);
                self.cart.prg_rom[addr as usize]
            }
            _ => {
                eprintln!("unhandled Cnrom readb at address: 0x{:04X}", addr);
                0
            }
        }
    }

    fn writeb(&mut self, addr: u16, val: u8) {
        match addr {
            // $0000-$1FFF PPU
            0x0000..=0x1FFF => {
                if self.cart.header.chr_size == 0 {
                    let addr = self.chr_bank * 0x2000 + addr;
                    self.cart.prg_rom[addr as usize] = val;
                }
            }
            0x6000..=0x7FFF => self.cart.prg_ram[(addr - 0x6000) as usize] = val,
            // $8000-$FFFF CPU
            0x8000..=0xFFFF => self.chr_bank = u16::from(val & 3),
            _ => eprintln!("unhandled Cnrom readb at address: 0x{:04X}", addr),
        }
    }
}

impl Board for Cnrom {
    fn scanline_irq(&self) -> bool {
        false
    }
    fn mirroring(&self) -> Mirroring {
        self.cart.mirroring
    }
    fn step(&mut self) {
        // NOOP
    }
}

//! TxROM/MMC3 (Mapper 4)
//!
//! [https://wiki.nesdev.com/w/index.php/TxROM]()
//! [https://wiki.nesdev.com/w/index.php/MMC3]()

use crate::{
    cartridge::Cartridge,
    common::{Clocked, Powered},
    logging::Loggable,
    mapper::{Mapper, MapperType, Mirroring},
    memory::{Banks, MemRead, MemWrite, Memory},
    serialization::Savable,
    NesResult,
};
use std::io::{Read, Write};

const PRG_ROM_BANK_SIZE: usize = 8 * 1024; // 8 KB ROM
const CHR_BANK_SIZE: usize = 1024; // 1 KB ROM/RAM

const FOUR_SCREEN_RAM_SIZE: usize = 4 * 1024;
const PRG_RAM_SIZE: usize = 8 * 1024;
const CHR_RAM_SIZE: usize = 8 * 1024;

const PRG_MODE_MASK: u8 = 0x40; // Bit 6 of bank select
const CHR_INVERSION_MASK: u8 = 0x80; // Bit 7 of bank select

/// TxROM
#[derive(Debug)]
pub struct Txrom {
    regs: TxRegs,
    has_chr_ram: bool,
    mirroring: Mirroring,
    irq_pending: bool,
    // http://forums.nesdev.com/viewtopic.php?p=62546#p62546
    // MMC3
    // Conquest of the Crystal Palace (MMC3B S 9039 1 DB)
    // Kickle Cubicle (MMC3B S 9031 3 DA)
    // M.C. Kids (MMC3B S 9152 3 AB)
    // Mega Man 3 (MMC3B S 9046 1 DB)
    // Super Mario Bros. 3 (MMC3B S 9027 5 A)
    // Startropics (MMC6B P 03'5)

    // MMC3_revb:
    // Batman (MMC3B 9006KP006)
    // Golgo 13: The Mafat Conspiracy (MMC3B 9016KP051)
    // Crystalis (MMC3B 9024KPO53)
    // Legacy of the Wizard (MMC3A 8940EP)
    mmc3_revb: bool,
    mmc_acc: bool, // Acclaims MMC3 clone - clocks on falling edge
    battery_backed: bool,
    prg_rom_bank_idx: [usize; 4],
    chr_bank_idx: [usize; 8],
    four_screen_ram: Memory,
    prg_ram: Memory, // CPU $6000..=$7FFF 8K PRG RAM Bank (optional)
    // CPU $8000..=$9FFF (or $C000..=$DFFF) 8 KB PRG ROM Bank 1 Switchable
    // CPU $A000..=$BFFF 8 KB PRG ROM Bank 2 Switchable
    // CPU $C000..=$DFFF (or $8000..=$9FFF) 8 KB PRG ROM Bank 3 Fixed to second-to-last Bank
    // CPU $E000..=$FFFF 8 KB PRG ROM Bank 4 Fixed to Last
    prg_rom_banks: Banks<Memory>,
    // PPU $0000..=$07FF (or $1000..=$17FF) 2 KB CHR ROM/RAM Bank 1 Switchable --+
    // PPU $0800..=$0FFF (or $1800..=$1FFF) 2 KB CHR ROM/RAM Bank 2 Switchable --|-+
    // PPU $1000..=$13FF (or $0000..=$03FF) 1 KB CHR ROM/RAM Bank 3 Switchable --+ |
    // PPU $1400..=$17FF (or $0400..=$07FF) 1 KB CHR ROM/RAM Bank 4 Switchable --+ |
    // PPU $1800..=$1BFF (or $0800..=$0BFF) 1 KB CHR ROM/RAM Bank 5 Switchable ----+
    // PPU $1C00..=$1FFF (or $0C00..=$0FFF) 1 KB CHR ROM/RAM Bank 6 Switchable ----+
    chr_banks: Banks<Memory>,
}

#[derive(Debug)]
struct TxRegs {
    bank_select: u8,
    bank_values: [u8; 8],
    irq_latch: u8,
    irq_counter: u8,
    irq_enabled: bool,
    irq_reload: bool,
    last_clock: u16,
    open_bus: u8,
}

impl TxRegs {
    fn new() -> Self {
        Self {
            bank_select: 0u8,
            bank_values: [0u8; 8],
            irq_latch: 0u8,
            irq_counter: 0u8,
            irq_enabled: false,
            irq_reload: false,
            last_clock: 0u16,
            open_bus: 0u8,
        }
    }
}

impl Txrom {
    pub fn load(cart: Cartridge) -> MapperType {
        let mirroring = cart.mirroring();
        let four_screen_ram = if mirroring == Mirroring::FourScreen {
            Memory::ram(FOUR_SCREEN_RAM_SIZE)
        } else {
            Memory::new()
        };

        let prg_ram = Memory::ram(PRG_RAM_SIZE);
        let prg_rom_banks = Banks::init(&cart.prg_rom, PRG_ROM_BANK_SIZE);
        let mut has_chr_ram = false;
        let chr_banks = if cart.chr_rom.is_empty() {
            let chr_ram_size = if let Ok(chr_ram_size) = cart.chr_ram_size() {
                if chr_ram_size > 0 {
                    chr_ram_size
                } else {
                    CHR_RAM_SIZE
                }
            } else {
                CHR_RAM_SIZE
            };
            let chr_ram = Memory::ram(chr_ram_size);
            has_chr_ram = true;
            Banks::init(&chr_ram, CHR_BANK_SIZE)
        } else {
            Banks::init(&cart.chr_rom, CHR_BANK_SIZE)
        };

        let prg_len = prg_rom_banks.len();
        let txrom = Self {
            regs: TxRegs::new(),
            has_chr_ram,
            mirroring,
            irq_pending: false,
            mmc3_revb: true, // TODO compare to known games
            mmc_acc: false,  // TODO - compare to known games
            battery_backed: cart.battery_backed(),
            prg_rom_bank_idx: [0, 1, prg_len - 2, prg_len - 1],
            chr_bank_idx: [0, 1, 2, 3, 4, 5, 6, 7],
            four_screen_ram,
            prg_ram,
            prg_rom_banks,
            chr_banks,
        };
        txrom.into()
    }

    /// 7654 3210
    /// CPMx xRRR
    /// |||   +++- Specify which bank register to update on next write to Bank Data register
    /// |||        0: Select 2 KB CHR bank at PPU $0000-$07FF (or $1000-$17FF);
    /// |||        1: Select 2 KB CHR bank at PPU $0800-$0FFF (or $1800-$1FFF);
    /// |||        2: Select 1 KB CHR bank at PPU $1000-$13FF (or $0000-$03FF);
    /// |||        3: Select 1 KB CHR bank at PPU $1400-$17FF (or $0400-$07FF);
    /// |||        4: Select 1 KB CHR bank at PPU $1800-$1BFF (or $0800-$0BFF);
    /// |||        5: Select 1 KB CHR bank at PPU $1C00-$1FFF (or $0C00-$0FFF);
    /// |||        6: Select 8 KB PRG ROM bank at $8000-$9FFF (or $C000-$DFFF);
    /// |||        7: Select 8 KB PRG ROM bank at $A000-$BFFF
    /// ||+------- Nothing on the MMC3, see MMC6
    /// |+-------- PRG ROM bank mode (0: $8000-$9FFF swappable,
    /// |                                $C000-$DFFF fixed to second-last bank;
    /// |                             1: $C000-$DFFF swappable,
    /// |                                $8000-$9FFF fixed to second-last bank)
    /// +--------- CHR A12 inversion (0: two 2 KB banks at $0000-$0FFF,
    ///                                 four 1 KB banks at $1000-$1FFF;
    ///                             1: two 2 KB banks at $1000-$1FFF,
    ///                                 four 1 KB banks at $0000-$0FFF)
    fn write_register(&mut self, addr: u16, val: u8) {
        // Match only $80/1, $A0/1, $C0/1, and $E0/1
        match addr & 0xE001 {
            // Memory mapping
            0x8000 => {
                self.regs.bank_select = val;
                self.update_banks();
            }
            0x8001 => {
                let bank = self.regs.bank_select & 0x07;
                self.regs.bank_values[bank as usize] = val;
                self.update_banks();
            }
            0xA000 => {
                if self.mirroring != Mirroring::FourScreen {
                    self.mirroring = match val & 0x01 {
                        0 => Mirroring::Vertical,
                        1 => Mirroring::Horizontal,
                        _ => panic!("impossible mirroring"),
                    };
                    self.update_banks();
                }
            }
            0xA001 => {
                // TODO RAM protect? Might conflict with MMC6
            }
            // IRQ
            0xC000 => self.regs.irq_latch = val,
            0xC001 => self.regs.irq_reload = true,
            0xE000 => {
                self.irq_pending = false;
                self.regs.irq_enabled = false;
            }
            0xE001 => self.regs.irq_enabled = true,
            _ => panic!("impossible address"),
        }
    }

    fn update_banks(&mut self) {
        let prg_len = self.prg_rom_banks.len();
        if self.regs.bank_select & PRG_MODE_MASK == PRG_MODE_MASK {
            self.prg_rom_bank_idx[0] = prg_len - 2;
            self.prg_rom_bank_idx[1] = (self.regs.bank_values[7] as usize) % prg_len;
            self.prg_rom_bank_idx[2] = (self.regs.bank_values[6] as usize) % prg_len;
            self.prg_rom_bank_idx[3] = prg_len - 1;
        } else {
            self.prg_rom_bank_idx[0] = (self.regs.bank_values[6] as usize) % prg_len;
            self.prg_rom_bank_idx[1] = (self.regs.bank_values[7] as usize) % prg_len;
            self.prg_rom_bank_idx[2] = prg_len - 2;
            self.prg_rom_bank_idx[3] = prg_len - 1;
        }

        // 1: two 2 KB banks at $1000-$1FFF, four 1 KB banks at $0000-$0FFF
        // 0: two 2 KB banks at $0000-$0FFF, four 1 KB banks at $1000-$1FFF
        let chr_len = self.chr_banks.len();
        if self.regs.bank_select & CHR_INVERSION_MASK == CHR_INVERSION_MASK {
            self.chr_bank_idx[0] = (self.regs.bank_values[2] as usize) % chr_len;
            self.chr_bank_idx[1] = (self.regs.bank_values[3] as usize) % chr_len;
            self.chr_bank_idx[2] = (self.regs.bank_values[4] as usize) % chr_len;
            self.chr_bank_idx[3] = (self.regs.bank_values[5] as usize) % chr_len;
            self.chr_bank_idx[4] = ((self.regs.bank_values[0] & 0xFE) as usize) % chr_len;
            self.chr_bank_idx[5] = ((self.regs.bank_values[0] | 0x01) as usize) % chr_len;
            self.chr_bank_idx[6] = ((self.regs.bank_values[1] & 0xFE) as usize) % chr_len;
            self.chr_bank_idx[7] = ((self.regs.bank_values[1] | 0x01) as usize) % chr_len;
        } else {
            self.chr_bank_idx[0] = ((self.regs.bank_values[0] & 0xFE) as usize) % chr_len;
            self.chr_bank_idx[1] = ((self.regs.bank_values[0] | 0x01) as usize) % chr_len;
            self.chr_bank_idx[2] = ((self.regs.bank_values[1] & 0xFE) as usize) % chr_len;
            self.chr_bank_idx[3] = ((self.regs.bank_values[1] | 0x01) as usize) % chr_len;
            self.chr_bank_idx[4] = (self.regs.bank_values[2] as usize) % chr_len;
            self.chr_bank_idx[5] = (self.regs.bank_values[3] as usize) % chr_len;
            self.chr_bank_idx[6] = (self.regs.bank_values[4] as usize) % chr_len;
            self.chr_bank_idx[7] = (self.regs.bank_values[5] as usize) % chr_len;
        }
    }
}

impl Mapper for Txrom {
    fn irq_pending(&mut self) -> bool {
        self.irq_pending
    }
    fn mirroring(&self) -> Mirroring {
        self.mirroring
    }
    fn vram_change(&mut self, addr: u16) {
        if addr < 0x2000 {
            let next_clock = (addr >> 12) & 1;
            // MMC_ACC = Falling edge, otherwise Rising edge
            let (last, next) = if self.mmc_acc { (1, 0) } else { (0, 1) };
            if self.regs.last_clock == last && next_clock == next {
                let counter = self.regs.irq_counter;
                if counter == 0 || self.regs.irq_reload {
                    self.regs.irq_counter = self.regs.irq_latch;
                } else {
                    self.regs.irq_counter -= 1;
                }
                // if (counter > 0 || self.regs.irq_reload)
                if (((counter & 0x01) | self.mmc3_revb as u8) == 0x01 || self.regs.irq_reload)
                    && self.regs.irq_counter == 0
                    && self.regs.irq_enabled
                {
                    self.irq_pending = true;
                }
                self.regs.irq_reload = false;
            }
            self.regs.last_clock = next_clock;
        }
    }
    fn battery_backed(&self) -> bool {
        self.battery_backed
    }
    fn save_sram<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        if self.battery_backed {
            self.prg_ram.save(fh)?;
        }
        Ok(())
    }
    fn load_sram<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        if self.battery_backed {
            self.prg_ram.load(fh)?;
        }
        Ok(())
    }
    fn use_ciram(&self, _addr: u16) -> bool {
        Mirroring::FourScreen != self.mirroring
    }
    fn open_bus(&mut self, _addr: u16, val: u8) {
        self.regs.open_bus = val;
    }
}

impl MemRead for Txrom {
    fn read(&mut self, addr: u16) -> u8 {
        self.peek(addr)
    }

    fn peek(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x1FFF => {
                let bank = addr as usize / CHR_BANK_SIZE;
                let addr = addr % CHR_BANK_SIZE as u16;
                let idx = self.chr_bank_idx[bank];
                self.chr_banks[idx].peek(addr)
            }
            0x2000..=0x3EFF if self.mirroring == Mirroring::FourScreen => {
                self.four_screen_ram.peek(addr - 0x2000)
            }
            0x6000..=0x7FFF => self.prg_ram.peek(addr - 0x6000),
            0x8000..=0xFFFF => {
                let bank = (addr - 0x8000) as usize / PRG_ROM_BANK_SIZE;
                let addr = addr % PRG_ROM_BANK_SIZE as u16;
                let idx = self.prg_rom_bank_idx[bank];
                self.prg_rom_banks[idx].peek(addr)
            }
            // 0x4020..=0x5FFF Nothing at this range
            _ => self.regs.open_bus,
        }
    }
}

impl MemWrite for Txrom {
    fn write(&mut self, addr: u16, val: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if self.has_chr_ram {
                    let bank = addr as usize / CHR_BANK_SIZE;
                    let addr = addr % CHR_BANK_SIZE as u16;
                    let idx = self.chr_bank_idx[bank];
                    self.chr_banks[idx].write(addr, val);
                }
            }
            0x2000..=0x3EFF if self.mirroring == Mirroring::FourScreen => {
                self.four_screen_ram.write(addr - 0x2000, val)
            }
            0x6000..=0x7FFF => self.prg_ram.write(addr - 0x6000, val),
            0x8000..=0xFFFF => self.write_register(addr, val),
            // 0x4020..=0x5FFF Nothing at this range
            _ => (),
        }
    }
}

impl Clocked for Txrom {}

impl Powered for Txrom {
    fn reset(&mut self) {
        self.irq_pending = false;
        self.regs = TxRegs::new();
    }
    fn power_cycle(&mut self) {
        self.reset();
    }
}

impl Loggable for Txrom {}

impl Savable for Txrom {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.regs.save(fh)?;
        self.has_chr_ram.save(fh)?;
        self.mirroring.save(fh)?;
        self.irq_pending.save(fh)?;
        self.mmc3_revb.save(fh)?;
        self.mmc_acc.save(fh)?;
        self.battery_backed.save(fh)?;
        self.prg_rom_bank_idx.save(fh)?;
        self.chr_bank_idx.save(fh)?;
        self.four_screen_ram.save(fh)?;
        self.prg_ram.save(fh)?;
        self.prg_rom_banks.save(fh)?;
        self.chr_banks.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.regs.load(fh)?;
        self.has_chr_ram.load(fh)?;
        self.mirroring.load(fh)?;
        self.irq_pending.load(fh)?;
        self.mmc3_revb.load(fh)?;
        self.mmc_acc.load(fh)?;
        self.battery_backed.load(fh)?;
        self.prg_rom_bank_idx.load(fh)?;
        self.chr_bank_idx.load(fh)?;
        self.four_screen_ram.load(fh)?;
        self.prg_ram.load(fh)?;
        self.prg_rom_banks.load(fh)?;
        self.chr_banks.load(fh)?;
        Ok(())
    }
}

impl Savable for TxRegs {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.bank_select.save(fh)?;
        self.bank_values.save(fh)?;
        self.irq_latch.save(fh)?;
        self.irq_counter.save(fh)?;
        self.irq_enabled.save(fh)?;
        self.last_clock.save(fh)?;
        self.open_bus.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.bank_select.load(fh)?;
        self.bank_values.load(fh)?;
        self.irq_latch.load(fh)?;
        self.irq_counter.load(fh)?;
        self.irq_enabled.load(fh)?;
        self.last_clock.load(fh)?;
        self.open_bus.load(fh)?;
        Ok(())
    }
}

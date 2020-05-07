use super::NesFormat;
use crate::{serialization::Savable, NesResult};
use std::io::{Read, Write};

// PPUSCROLL masks
// yyy NN YYYYY XXXXX
// ||| || ||||| +++++- 5 bit coarse X
// ||| || +++++------- 5 bit coarse Y
// ||| |+------------- Nametable X offset
// ||| +-------------- Nametable Y offset
// +++---------------- 3 bit fine Y
pub(super) const COARSE_X_MASK: u16 = 0x001F;
pub(super) const COARSE_Y_MASK: u16 = 0x03E0;
pub(super) const NT_X_MASK: u16 = 0x0400;
pub(super) const NT_Y_MASK: u16 = 0x0800;
pub(super) const FINE_Y_MASK: u16 = 0x7000;
pub(super) const X_MAX_COL: u16 = 31; // last column of tiles - 255 pixel width / 8 pixel wide tiles
pub(super) const Y_MAX_COL: u16 = 29; // last row of tiles - (240 pixel height / 8 pixel tall tiles) - 1
pub(super) const Y_OVER_COL: u16 = 31; // overscan row

#[derive(Debug, Clone)]
pub struct PpuRegs {
    ctrl: u8,         // $2000 PPUCTRL write-only
    mask: u8,         // $2001 PPUMASK write-only
    status: u8,       // $2002 PPUSTATUS read-only
    pub oamaddr: u8,  // $2003 OAMADDR write-only
    pub v: u16,       // $2006 PPUADDR write-only 2x 15 bits: yyy NN YYYYY XXXXX
    pub t: u16,       // Temporary v - Also the addr of top-left onscreen tile
    pub x: u16,       // Fine X
    pub w: bool,      // 1st or 2nd write toggle
    pub open_bus: u8, // This open bus gets set during any write to PPU registers
}

impl PpuRegs {
    pub(super) fn new() -> Self {
        Self {
            ctrl: 0x00,
            mask: 0x00,
            status: 0x00,
            oamaddr: 0x00,
            v: 0x0000,
            t: 0x0000,
            x: 0x0000,
            w: false,
            open_bus: 0x00,
        }
    }

    /*
     * $2000 PPUCTRL
     *
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUCTRL
     * VPHB SINN
     * |||| ||++- Nametable Select: 0 = $2000 (upper-left); 1 = $2400 (upper-right);
     * |||| ||                      2 = $2800 (lower-left); 3 = $2C00 (lower-right)
     * |||| |||+-   Also For PPUSCROLL: 1 = Add 256 to X scroll
     * |||| ||+--   Also For PPUSCROLL: 1 = Add 240 to Y scroll
     * |||| |+--- VRAM Increment Mode: 0 = add 1, going across; 1 = add 32, going down
     * |||| +---- Sprite Pattern Select for 8x8: 0 = $0000, 1 = $1000, ignored in 8x16 mode
     * |||+------ Background Pattern Select: 0 = $0000, 1 = $1000
     * ||+------- Sprite Height: 0 = 8x8, 1 = 8x16
     * |+-------- PPU Master/Slave: 0 = read from EXT, 1 = write to EXT
     * +--------- NMI Enable: NMI at next vblank: 0 = off, 1: on
     */
    pub(super) fn write_ctrl(&mut self, val: u8) {
        let nn_mask = NT_Y_MASK | NT_X_MASK;
        // val: ......BA
        // t: ....BA.. ........
        self.t = (self.t & !nn_mask) | (u16::from(val) & 0x03) << 10; // take lo 2 bits and set NN
        self.ctrl = val;
    }
    pub(super) fn sprite_select(&self) -> u16 {
        if self.ctrl & 0x08 > 0 {
            0x1000
        } else {
            0x0000
        }
    }
    pub(super) fn background_select(&self) -> u16 {
        if self.ctrl & 0x10 > 0 {
            0x1000
        } else {
            0x0000
        }
    }
    pub(super) fn sprite_height(&self) -> u16 {
        if self.ctrl & 0x20 > 0 {
            16
        } else {
            8
        }
    }
    pub(super) fn nmi_enabled(&self) -> bool {
        self.ctrl & 0x80 > 0
    }

    /*
     * $2001 PPUMASK
     *
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUMASK
     * BGRs bMmG
     * |||| |||+- Grayscale (0: normal color, 1: produce a grayscale display)
     * |||| ||+-- 1: Show background in leftmost 8 pixels of screen, 0: Hide
     * |||| |+--- 1: Show sprites in leftmost 8 pixels of screen, 0: Hide
     * |||| +---- 1: Show background
     * |||+------ 1: Show sprites
     * ||+------- Emphasize red
     * |+-------- Emphasize green
     * +--------- Emphasize blue
     */
    pub(super) fn write_mask(&mut self, val: u8) {
        self.mask = val;
    }
    pub(super) fn show_left_background(&self) -> bool {
        self.mask & 0x02 > 0
    }
    pub(super) fn show_left_sprites(&self) -> bool {
        self.mask & 0x04 > 0
    }
    pub(super) fn show_background(&self) -> bool {
        self.mask & 0x08 > 0
    }
    pub(super) fn show_sprites(&self) -> bool {
        self.mask & 0x10 > 0
    }
    pub(super) fn grayscale(&self) -> bool {
        self.mask & 0x01 > 0
    }
    pub(super) fn emphasis(&self, format: NesFormat) -> u8 {
        match format {
            NesFormat::Ntsc => (self.mask & 0xE0) >> 5,
            _ => {
                // Red/Green are swapped for PAL/Dendy
                let red = (self.mask & 0x20) << 1;
                let green = (self.mask & 0x40) >> 1;
                let blue = self.mask & 0x80;
                (blue | red | green) >> 5
            }
        }
    }

    /*
     * $2002 PPUSTATUS
     *
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUSTATUS
     * VSO. ....
     * |||+-++++- Least significant bits previously written into a PPU register
     * ||+------- Sprite overflow.
     * |+-------- Sprite 0 Hit.
     * +--------- Vertical blank has started (0: not in vblank; 1: in vblank)
     */
    pub(super) fn read_status(&mut self) -> u8 {
        self.reset_rw();
        let vblank_started = self.status & 0x80;
        self.status &= !0x80; // Set vblank to 0
        self.status | vblank_started // return status with original vblank
    }
    pub(super) fn peek_status(&self) -> u8 {
        self.status
    }

    pub(super) fn set_sprite_overflow(&mut self, val: bool) {
        self.status = if val {
            self.status | 0x20
        } else {
            self.status & !0x20
        };
    }
    pub(super) fn sprite_zero_hit(&self) -> bool {
        self.status & 0x40 == 0x40
    }
    pub(super) fn set_sprite_zero_hit(&mut self, val: bool) {
        self.status = if val {
            self.status | 0x40
        } else {
            self.status & !0x40
        };
    }
    pub(super) fn vblank_started(&self) -> bool {
        self.status & 0x80 > 0
    }
    pub(super) fn start_vblank(&mut self) {
        self.status |= 0x80;
    }
    pub(super) fn stop_vblank(&mut self) {
        self.status &= !0x80;
    }

    /*
     * $2005 PPUSCROLL
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUSCROLL
     * http://wiki.nesdev.com/w/index.php/PPU_scrolling
     */

    // Returns Coarse X: XXXXX from PPUADDR v
    // yyy NN YYYYY XXXXX
    pub(super) fn coarse_x(&self) -> u16 {
        self.v & COARSE_X_MASK
    }

    // Returns Fine X: xxx from x register
    pub(super) fn fine_x(&self) -> u16 {
        self.x
    }

    // Returns Coarse Y: YYYYY from PPUADDR v
    // yyy NN YYYYY XXXXX
    pub(super) fn coarse_y(&self) -> u16 {
        (self.v & COARSE_Y_MASK) >> 5
    }

    // Returns Fine Y: yyy from PPUADDR v
    // yyy NN YYYYY XXXXX
    pub(super) fn fine_y(&self) -> u16 {
        (self.v & FINE_Y_MASK) >> 12
    }

    // Writes val to PPUSCROLL
    // 1st write writes X
    // 2nd write writes Y
    pub(super) fn write_scroll(&mut self, val: u8) {
        let val = u16::from(val);
        let lo_5_bit_mask: u16 = 0x1F;
        let fine_mask: u16 = 0x07;
        let fine_rshift = 3;
        if !self.w {
            // Write X on first write
            // lo 3 bits goes into fine x, remaining 5 bits go into t for coarse x
            // val: HGFEDCBA
            // t: ........ ...HGFED
            // x:               CBA
            self.t &= !COARSE_X_MASK; // Empty coarse X
            self.t |= (val >> fine_rshift) & lo_5_bit_mask; // Set coarse X
            self.x = val & fine_mask; // Set fine X
        } else {
            // Write Y on second write
            // lo 3 bits goes into fine y, remaining 5 bits go into t for coarse y
            // val: HGFEDCBA
            // t: .CBA..HG FED.....
            let coarse_y_lshift = 5;
            let fine_y_lshift = 12;
            self.t &= !(FINE_Y_MASK | COARSE_Y_MASK); // Empty Y
            self.t |= ((val >> fine_rshift) & lo_5_bit_mask) << coarse_y_lshift; // Set coarse Y
            self.t |= (val & fine_mask) << fine_y_lshift; // Set fine Y
        }
        self.w = !self.w;
    }

    // Copy Coarse X from register t and add it to PPUADDR v
    pub(super) fn copy_x(&mut self) {
        //    .....N.. ...XXXXX
        // t: .....F.. ...EDCBA
        // v: .....F.. ...EDCBA
        let x_mask = NT_X_MASK | COARSE_X_MASK;
        self.v = (self.v & !x_mask) | (self.t & x_mask);
    }

    // Copy Fine y and Coarse Y from register t and add it to PPUADDR v
    pub(super) fn copy_y(&mut self) {
        //    .yyyN.YY YYY.....
        // t: .IHGF.ED CBA.....
        // v: .IHGF.ED CBA.....
        let y_mask = FINE_Y_MASK | NT_Y_MASK | COARSE_Y_MASK;
        self.v = (self.v & !y_mask) | (self.t & y_mask);
    }

    // Increment Coarse X
    // 0-4 bits are incremented, with overflow toggling bit 10 which switches the horizontal
    // nametable
    // http://wiki.nesdev.com/w/index.php/PPU_scrolling#Wrapping_around
    pub(super) fn increment_x(&mut self) {
        // let v = self.v;
        // If we've reached the last column, toggle horizontal nametable
        if (self.v & COARSE_X_MASK) == X_MAX_COL {
            self.v = (self.v & !COARSE_X_MASK) ^ NT_X_MASK; // toggles X nametable
        } else {
            self.v += 1;
        }
    }

    // Increment Fine Y
    // Bits 12-14 are incremented for Fine Y, with overflow incrementing coarse Y in bits 5-9 with
    // overflow toggling bit 11 which switches the vertical nametable
    // http://wiki.nesdev.com/w/index.php/PPU_scrolling#Wrapping_around
    pub(super) fn increment_y(&mut self) {
        if (self.v & FINE_Y_MASK) != FINE_Y_MASK {
            // If fine y < 7 (0b111), increment
            self.v += 0x1000;
        } else {
            self.v &= !FINE_Y_MASK; // set fine y = 0 and overflow into coarse y
            let mut y = (self.v & COARSE_Y_MASK) >> 5; // Get 5 bits of coarse y
            if y == Y_MAX_COL {
                y = 0;
                // switches vertical nametable
                self.v ^= NT_Y_MASK;
            } else if y == Y_OVER_COL {
                // Out of bounds. Does not switch nametable
                // Some games use this
                y = 0;
            } else {
                y += 1; // increment coarse y
            }
            self.v = (self.v & !COARSE_Y_MASK) | (y << 5); // put coarse y back into v
        }
    }

    // Increment PPUADDR v
    // Address wraps and uses vram_increment which is either 1 (going across) or 32 (going down)
    // based on bit 7 in PPUCTRL
    pub(super) fn increment_v(&mut self) {
        self.v = self.v.wrapping_add(self.vram_increment());
    }

    /*
     * $2006 PPUADDR
     * http://wiki.nesdev.com/w/index.php/PPU_registers#PPUADDR
     */
    pub(super) fn read_addr(&self) -> u16 {
        self.v & 0x3FFF // Bits 0-14
    }

    // Write val to PPUADDR v
    // 1st write writes hi 6 bits
    // 2nd write writes lo 8 bits
    // Total size is a 14 bit addr
    pub(super) fn write_addr(&mut self, val: u16) {
        if !self.w {
            // Write hi address on first write
            let hi_bits_mask = 0x00FF;
            let hi_lshift = 8;
            let six_bits_mask = 0x003F;
            // val: ..FEDCBA
            //    FEDCBA98 76543210
            // t: 00FEDCBA ........
            self.t = (self.t & hi_bits_mask) | ((val & six_bits_mask) << hi_lshift);
        } else {
            // Write lo address on second write
            let lo_bits_mask = 0x7F00;
            // val: HGFEDCBA
            // t: ........ HGFEDCBA
            // v: t
            self.t = (self.t & lo_bits_mask) | val;
            self.v = self.t;
        }
        self.w = !self.w;
    }

    fn vram_increment(&self) -> u16 {
        if self.ctrl & 0x04 > 0 {
            32
        } else {
            1
        }
    }
    // Resets 1st/2nd Write latch for PPUSCROLL and PPUADDR
    fn reset_rw(&mut self) {
        self.w = false;
    }
}

impl Savable for PpuRegs {
    fn save<F: Write>(&self, fh: &mut F) -> NesResult<()> {
        self.ctrl.save(fh)?;
        self.mask.save(fh)?;
        self.status.save(fh)?;
        self.oamaddr.save(fh)?;
        self.v.save(fh)?;
        self.t.save(fh)?;
        self.x.save(fh)?;
        self.w.save(fh)?;
        self.open_bus.save(fh)?;
        Ok(())
    }
    fn load<F: Read>(&mut self, fh: &mut F) -> NesResult<()> {
        self.ctrl.load(fh)?;
        self.mask.load(fh)?;
        self.status.load(fh)?;
        self.oamaddr.load(fh)?;
        self.v.load(fh)?;
        self.t.load(fh)?;
        self.x.load(fh)?;
        self.w.load(fh)?;
        self.open_bus.load(fh)?;
        Ok(())
    }
}

impl Default for PpuRegs {
    fn default() -> Self {
        Self::new()
    }
}

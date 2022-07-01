use crate::common::{Kind, Reset};
use serde::{Deserialize, Serialize};

#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize)]
#[must_use]
pub struct PpuScroll {
    v: u16,            // Subject to ADDR_MIRROR
    t: u16,            // Temporary v - Also the addr of top-left onscreen tile
    x: u16,            // Fine X
    write_latch: bool, // 1st or 2nd write toggle
}

impl PpuScroll {
    // PPUSCROLL masks
    //   1 00 00000 00000
    // yyy NN YYYYY XXXXX
    // ||| || ||||| +++++- 5 bit coarse X
    // ||| || +++++------- 5 bit coarse Y
    // ||| |+------------- Nametable X offset
    // ||| +-------------- Nametable Y offset
    // +++---------------- 3 bit fine Y
    pub(crate) const COARSE_X_MASK: u16 = 0x001F;
    pub(crate) const COARSE_Y_MASK: u16 = 0x03E0;
    pub(crate) const NT_X_MASK: u16 = 0x0400;
    pub(crate) const NT_Y_MASK: u16 = 0x0800;
    const FINE_Y_MASK: u16 = 0x7000;
    const X_MAX_COL: u16 = 31; // last column of tiles - 255 pixel width / 8 pixel wide tiles
    const Y_MAX_COL: u16 = 29; // last row of tiles - (240 pixel height / 8 pixel tall tiles) - 1
    const Y_OVER_COL: u16 = 31; // overscan row
    const Y_INCREMENT: u16 = 0x1000; // Increment y in bit 12

    const ATTR_START: u16 = 0x23C0;
    const ADDR_MIRROR: u16 = 0x3FFF; // 15 bits: yyy NN YYYYY XXXXX

    pub const fn new() -> Self {
        Self {
            v: 0x0000,
            t: 0x0000,
            x: 0x00,
            write_latch: false,
        }
    }

    // https://wiki.nesdev.com/w/index.php/PPU_scrolling#Tile_and_attribute_fetching
    // NN 1111 YYY XXXXX
    // || |||| ||| +++-- high 3 bits of coarse X (x/4)
    // || |||| +++------ high 3 bits of coarse Y (y/4)
    // || ++++---------- attribute offset (960 bytes)
    // ++--------------- nametable select
    #[inline]
    #[must_use]
    pub const fn attr_addr(&self) -> u16 {
        let nametable_select = self.v & (Self::NT_X_MASK | Self::NT_Y_MASK);
        let y_bits = (self.v >> 4) & 0x38;
        let x_bits = (self.v >> 2) & 0x07;
        Self::ATTR_START | nametable_select | y_bits | x_bits
    }

    #[inline]
    #[must_use]
    pub const fn attr_shift(&self) -> u16 {
        (self.v & 0x02) | ((self.v >> 4) & 0x04)
    }

    #[inline]
    #[must_use]
    pub const fn read_addr(&self) -> u16 {
        self.v
    }

    // Writes to PPUSCROLL affect v and t
    // 1st write writes X
    // 2nd write writes Y
    pub fn write(&mut self, val: u8) {
        let val = u16::from(val);
        let lo_5_bit_mask: u16 = 0x1F;
        let fine_mask: u16 = 0x07;
        let fine_rshift = 3;
        if self.write_latch {
            // Write Y on second write
            // lo 3 bits goes into fine y, remaining 5 bits go into t for coarse y
            // val: HGFEDCBA
            // t: .CBA..HG FED.....
            let coarse_y_lshift = 5;
            let fine_y_lshift = 12;
            self.t &= !(Self::FINE_Y_MASK | Self::COARSE_Y_MASK); // Empty Y
            self.t |= ((val >> fine_rshift) & lo_5_bit_mask) << coarse_y_lshift; // Set coarse Y
            self.t |= (val & fine_mask) << fine_y_lshift; // Set fine Y
        } else {
            // Write X on first write
            // lo 3 bits goes into fine x, remaining 5 bits go into t for coarse x
            // val: HGFEDCBA
            // t: ........ ...HGFED
            // x:               CBA
            self.t &= !Self::COARSE_X_MASK; // Empty coarse X
            self.t |= (val >> fine_rshift) & lo_5_bit_mask; // Set coarse X
            self.x = val & fine_mask; // Set fine X
        }
        self.write_latch = !self.write_latch;
    }

    // Write to PPUADDR affect v and t
    // 1st write writes hi 6 bits
    // 2nd write writes lo 8 bits
    // Total size is a 14 bit addr
    #[inline]
    pub fn write_addr(&mut self, val: u8) {
        if self.write_latch {
            // Write lo address on second write
            let lo_bits_mask = 0x7F00;
            // val: HGFEDCBA
            // t: ........ HGFEDCBA
            // v: t
            self.t = (self.t & lo_bits_mask) | u16::from(val);
            self.v = self.t;
            self.v &= Self::ADDR_MIRROR;
        } else {
            // Write hi address on first write
            let hi_bits_mask = 0x00FF;
            let six_bits_mask = 0x003F;
            // val: ..FEDCBA
            //    FEDCBA98 76543210
            // t: 00FEDCBA ........
            self.t = (self.t & hi_bits_mask) | ((u16::from(val) & six_bits_mask) << 8);
        }
        self.write_latch = !self.write_latch;
    }

    // Returns Coarse X: XXXXX from PPUADDR v
    // yyy NN YYYYY XXXXX
    #[inline]
    #[must_use]
    pub const fn coarse_x(&self) -> u16 {
        self.v & Self::COARSE_X_MASK
    }

    // Returns Fine X: xxx from x register
    #[inline]
    #[must_use]
    pub const fn fine_x(&self) -> u16 {
        self.x
    }

    // Returns Coarse Y: YYYYY from PPUADDR v
    // yyy NN YYYYY XXXXX
    #[inline]
    #[must_use]
    pub const fn coarse_y(&self) -> u16 {
        (self.v & Self::COARSE_Y_MASK) >> 5
    }

    // Returns Fine Y: yyy from PPUADDR v
    // yyy NN YYYYY XXXXX
    #[inline]
    #[must_use]
    pub const fn fine_y(&self) -> u16 {
        self.v >> 12
    }

    // Increment PPUADDR v by either 1 (going across) or 32 (going down)
    // Address wraps around
    #[inline]
    pub fn increment(&mut self, val: u16) {
        self.v = self.v.wrapping_add(val);
        self.v &= Self::ADDR_MIRROR;
    }

    // Copy Coarse X from register t and add it to PPUADDR v
    #[inline]
    pub fn copy_x(&mut self) {
        //    .....N.. ...XXXXX
        // t: .....F.. ...EDCBA
        // v: .....F.. ...EDCBA
        let x_mask = Self::NT_X_MASK | Self::COARSE_X_MASK;
        self.v = (self.v & !x_mask) | (self.t & x_mask);
    }

    // Copy Fine y and Coarse Y from register t and add it to PPUADDR v
    #[inline]
    pub fn copy_y(&mut self) {
        //    .yyyN.YY YYY.....
        // t: .IHGF.ED CBA.....
        // v: .IHGF.ED CBA.....
        let y_mask = Self::FINE_Y_MASK | Self::NT_Y_MASK | Self::COARSE_Y_MASK;
        self.v = (self.v & !y_mask) | (self.t & y_mask);
    }

    // Increment Coarse X
    // 0-4 bits are incremented, with overflow toggling bit 10 which switches the horizontal
    // nametable
    // http://wiki.nesdev.com/w/index.php/PPU_scrolling#Wrapping_around
    pub fn increment_x(&mut self) {
        // let v = self.v;
        // If we've reached the last column, toggle horizontal nametable
        if (self.v & Self::COARSE_X_MASK) == Self::X_MAX_COL {
            self.v = (self.v & !Self::COARSE_X_MASK) ^ Self::NT_X_MASK; // toggles X nametable
        } else {
            self.v += 1;
        }
    }

    // Increment Fine Y
    // Bits 12-14 are incremented for Fine Y, with overflow incrementing coarse Y in bits 5-9 with
    // overflow toggling bit 11 which switches the vertical nametable
    // http://wiki.nesdev.com/w/index.php/PPU_scrolling#Wrapping_around
    pub fn increment_y(&mut self) {
        if (self.v & Self::FINE_Y_MASK) == Self::FINE_Y_MASK {
            self.v &= !Self::FINE_Y_MASK; // set fine y = 0 and overflow into coarse y
            let mut y = (self.v & Self::COARSE_Y_MASK) >> 5; // Get 5 bits of coarse y
            if y == Self::Y_MAX_COL {
                y = 0;
                // switches vertical nametable
                self.v ^= Self::NT_Y_MASK;
            } else if y == Self::Y_OVER_COL {
                // Out of bounds. Does not switch nametable
                // Some games use this
                y = 0;
            } else {
                y += 1; // increment coarse y
            }
            self.v = (self.v & !Self::COARSE_Y_MASK) | (y << 5); // put coarse y back into v
        } else {
            // If fine y < 7 (0b111), increment
            self.v += Self::Y_INCREMENT;
        }
    }

    #[inline]
    pub fn reset_latch(&mut self) {
        self.write_latch = false;
    }

    #[inline]
    pub fn write_nametable_select(&mut self, val: u8) {
        let nt_mask = Self::NT_Y_MASK | Self::NT_X_MASK;
        // val: ......BA
        // t: ....BA.. ........
        self.t = (self.t & !nt_mask) | (u16::from(val) & 0x03) << 10; // take lo 2 bits and set NN
    }
}

impl Reset for PpuScroll {
    // https://www.nesdev.org/wiki/PPU_power_up_state
    fn reset(&mut self, kind: Kind) {
        if kind == Kind::Hard {
            // v is not cleared on a a soft reset
            self.v = 0x0000;
        }
        self.t = 0x0000;
        self.x = 0x00;
        self.write_latch = false;
    }
}

use image::{ImageBuffer, Rgba};

const COLORS: [u32; 64] = [
    0x0066_6666,
    0x0000_2A88,
    0x0014_12A7,
    0x003B_00A4,
    0x005C_007E,
    0x006E_0040,
    0x006C_0600,
    0x0056_1D00,
    0x0033_3500,
    0x000B_4800,
    0x0000_5200,
    0x0000_4F08,
    0x0000_404D,
    0x0000_0000,
    0x0000_0000,
    0x0000_0000,
    0x00AD_ADAD,
    0x0015_5FD9,
    0x0042_40FF,
    0x0075_27FE,
    0x00A0_1ACC,
    0x00B7_1E7B,
    0x00B5_3120,
    0x0099_4E00,
    0x006B_6D00,
    0x0038_8700,
    0x000C_9300,
    0x0000_8F32,
    0x0000_7C8D,
    0x0000_0000,
    0x0000_0000,
    0x0000_0000,
    0x00FF_FEFF,
    0x0064_B0FF,
    0x0092_90FF,
    0x00C6_76FF,
    0x00F3_6AFF,
    0x00FE_6ECC,
    0x00FE_8170,
    0x00EA_9E22,
    0x00BC_BE00,
    0x0088_D800,
    0x005C_E430,
    0x0045_E082,
    0x0048_CDDE,
    0x004F_4F4F,
    0x0000_0000,
    0x0000_0000,
    0x00FF_FEFF,
    0x00C0_DFFF,
    0x00D3_D2FF,
    0x00E8_C8FF,
    0x00FB_C2FF,
    0x00FE_C4EA,
    0x00FE_CCC5,
    0x00F7_D8A5,
    0x00E4_E594,
    0x00CF_EF96,
    0x00BD_F4AB,
    0x00B3_F3CC,
    0x00B5_EBF2,
    0x00B8_B8B8,
    0x0000_0000,
    0x0000_0000,
];

// Picture Processing Unit
pub struct PPU {
    pub cycle: u32,     // 0-340
    pub scan_line: u32, // 0-261, 0-239=visible, 240=post, 241-260=vblank, 261=pre
    frame: u64,         // frame counter

    // storage variables
    palette_data: [u8; 32],
    pub name_table_data: [u8; 2048],
    pub oam_data: [u8; 256], // Object Attribute Memory
    front: ImageBuffer<Rgba<u8>, Vec<u8>>,
    back: ImageBuffer<Rgba<u8>, Vec<u8>>,

    // PPU registers
    pub v: u16, // current vram address (15 bit)
    pub t: u16, // temporary vram address (15 bit)
    pub x: u8,  // fine x scroll (3 bit)
    pub w: u8,  // write toggle (1 bit)
    pub f: u8,  // even/odd frame flag (1 bit)

    pub register: u8,

    // NMI flags
    pub nmi_occurred: bool,
    nmi_output: bool,
    nmi_previous: bool,
    nmi_delay: u8,

    // background temporary variables
    pub name_table_byte: u8,
    pub attribute_table_byte: u8,
    pub low_tile_byte: u8,
    pub high_tile_byte: u8,
    pub tile_data: u64,

    // sprite temporary variables
    pub sprite_count: usize,
    pub sprite_patterns: [u32; 8],
    pub sprite_positions: [u8; 8],
    pub sprite_priorities: [u8; 8],
    pub sprite_indexes: [u8; 8],

    // $2000 PPUCTRL
    flag_name_table: u8,       // 0: $2000; 1: $2400; 2: $2800; 3: $2C00
    pub flag_increment: bool,  // false: add 1; true: add 32
    pub flag_sprite_table: u8, // 0: $0000; 1: $1000; ignored in 8x16 mode
    flag_background_table: u8, // 0: $0000; 1: $1000
    pub flag_sprite_size: u8,  // 0: 8x8; 1: 8x16
    flag_master_slave: u8,     // 0: read EXT; 1: write EXT

    // $2001 PPUMASK
    flag_grayscale: u8,            // 0: color; 1: grayscale
    flag_show_left_background: u8, // 0: hide; 1: show
    flag_show_left_sprites: u8,    // 0: hide; 1: show
    pub flag_show_background: u8,  // 0: hide; 1: show
    pub flag_show_sprites: u8,     // 0: hide; 1: show
    flag_red_tint: u8,             // 0: normal; 1: emphasized
    flag_green_tint: u8,           // 0: normal; 1: emphasized
    flag_blue_tint: u8,            // 0: normal; 1: emphasized

    // $2002 PPUSTATUS
    pub flag_sprite_zero_hit: u8,
    pub flag_sprite_overflow: u8,

    // $2003 OAMADDR
    pub oam_address: u8,

    // $2007 PPUDATA
    pub buffered_data: u8, // for buffered reads

    palette: Vec<image::Rgba<u8>>,
}

impl PPU {
    pub fn new() -> Self {
        let mut ppu = Self {
            cycle: 340,
            scan_line: 250,
            frame: 0,
            palette_data: [0; 32],
            name_table_data: [0; 2048],
            oam_data: [0; 256],
            front: ImageBuffer::new(256, 240),
            back: ImageBuffer::new(256, 240),
            v: 0,
            t: 0,
            x: 0,
            w: 0,
            f: 0,
            register: 0,
            nmi_occurred: false,
            nmi_output: false,
            nmi_previous: false,
            nmi_delay: 0,
            name_table_byte: 0,
            attribute_table_byte: 0,
            low_tile_byte: 0,
            high_tile_byte: 0,
            tile_data: 0,
            sprite_count: 0,
            sprite_patterns: [0; 8],
            sprite_positions: [0; 8],
            sprite_priorities: [0; 8],
            sprite_indexes: [0; 8],
            flag_name_table: 0,
            flag_increment: false,
            flag_sprite_table: 0,
            flag_background_table: 0,
            flag_sprite_size: 0,
            flag_master_slave: 0,
            flag_grayscale: 0,
            flag_show_left_background: 0,
            flag_show_left_sprites: 0,
            flag_show_background: 0,
            flag_show_sprites: 0,
            flag_red_tint: 0,
            flag_green_tint: 0,
            flag_blue_tint: 0,
            flag_sprite_zero_hit: 0,
            flag_sprite_overflow: 0,
            oam_address: 0,
            buffered_data: 0,
            palette: PPU::new_palette(),
        };
        ppu.reset();
        ppu
    }

    fn new_palette() -> Vec<image::Rgba<u8>> {
        let mut palette: Vec<image::Rgba<u8>> = Vec::with_capacity(64);
        for c in COLORS.iter() {
            let r = (*c >> 16) as u8;
            let g = (*c >> 8) as u8;
            let b = *c as u8;
            palette.push(image::Rgba([r, g, b, 0xFF]));
        }
        palette
    }

    // Getters/Setters

    pub fn name_table_data(&mut self, addr: u16) -> u8 {
        self.name_table_data[addr as usize]
    }

    pub fn set_name_table_data(&mut self, addr: u16, val: u8) {
        self.name_table_data[addr as usize] = val;
    }

    pub fn read_palette(&mut self, mut addr: u16) -> u8 {
        if addr >= 16 && addr % 4 == 0 {
            addr -= 16;
        }
        self.palette_data[addr as usize]
    }

    pub fn write_palette(&mut self, mut addr: u16, val: u8) {
        if addr >= 16 && addr % 4 == 0 {
            addr -= 16;
        }
        self.palette_data[addr as usize] = val;
    }

    pub fn get_tile_byte_addr(&self) -> u16 {
        let fine_y = (self.v >> 12) & 7;
        let table = self.flag_background_table;
        let tile = self.name_table_byte;
        0x1000 * u16::from(table) + u16::from(tile) * 16 + fine_y
    }

    pub fn fetch_tile_data(&self) -> u32 {
        (self.tile_data >> 32) as u32
    }

    pub fn store_tile_data(&mut self) {
        let mut data: u32 = 0;
        for _ in 0..8 {
            let a = self.attribute_table_byte;
            let p1 = (self.low_tile_byte & 0x80) >> 7;
            let p2 = (self.high_tile_byte & 0x80) >> 6;
            self.low_tile_byte <<= 1;
            self.high_tile_byte <<= 1;
            data <<= 4;
            data |= u32::from(a | p1 | p2);
        }
        self.tile_data |= u64::from(data);
    }

    pub fn background_pixel(&self) -> u8 {
        if self.flag_show_background == 0 {
            0
        } else {
            let data = self.fetch_tile_data() >> ((7 - self.x) * 4);
            (data & 0x0F) as u8
        }
    }

    pub fn sprite_pixel(&self) -> (usize, u8) {
        if self.flag_show_sprites != 0 {
            for i in 0..self.sprite_count {
                let mut offset = (self.cycle - 1) - u32::from(self.sprite_positions[i]);
                if offset > 7 {
                    continue;
                }
                offset = 7 - offset;
                let color = (self.sprite_patterns[i] >> (offset * 4) as u8 & 0x0F) as u8;
                if color % 4 == 0 {
                    continue;
                }
                return (i, color);
            }
        }
        (0, 0)
    }

    // Operations

    pub fn write_control(&mut self, val: u8) {
        self.flag_name_table = val & 3;
        self.flag_increment = (val >> 2) & 1 != 0;
        self.flag_sprite_table = (val >> 3) & 1;
        self.flag_background_table = (val >> 4) & 1;
        self.flag_sprite_size = (val >> 5) & 1;
        self.flag_master_slave = (val >> 6) & 1;
        self.nmi_output = (val >> 7) & 1 == 1;
        self.nmi_change();
        self.t = (self.t & 0xF3FF) | ((u16::from(val) & 0x03) << 10);
    }

    pub fn write_mask(&mut self, val: u8) {
        self.flag_grayscale = val & 1;
        self.flag_show_left_background = (val >> 1) & 1;
        self.flag_show_left_sprites = (val >> 2) & 1;
        self.flag_show_background = (val >> 3) & 1;
        self.flag_show_sprites = (val >> 4) & 1;
        self.flag_red_tint = (val >> 5) & 1;
        self.flag_green_tint = (val >> 6) & 1;
        self.flag_blue_tint = (val >> 7) & 1;
    }

    pub fn nmi_change(&mut self) {
        let nmi = self.nmi_output && self.nmi_occurred;
        if nmi && !self.nmi_previous {
            self.nmi_delay = 15;
        }
        self.nmi_previous = nmi;
    }

    // Update cycle, scan_line and frame counters
    pub fn tick(&mut self) -> bool {
        let mut trigger_nmi = false;
        if self.nmi_delay > 0 {
            self.nmi_delay -= 1;
            if self.nmi_delay == 0 && self.nmi_output && self.nmi_occurred {
                trigger_nmi = true;
            }
        }

        let rendering_enabled = self.flag_show_background != 0 || self.flag_show_sprites != 0;
        if rendering_enabled && self.f == 1 && self.scan_line == 261 && self.cycle == 339 {
            self.cycle = 0;
            self.scan_line = 0;
            self.frame += 1;
            self.f ^= 1;
        }
        self.cycle += 1;
        if self.cycle > 340 {
            self.cycle = 0;
            self.scan_line += 1;
            if self.scan_line > 261 {
                self.scan_line = 0;
                self.frame += 1;
                self.f ^= 1;
            }
        }
        trigger_nmi
    }

    pub fn render_pixel(&mut self) {
        let x = self.cycle - 1 as u32;
        let y = self.scan_line as u32;
        let mut background = self.background_pixel();
        let (i, mut sprite) = self.sprite_pixel();
        if x < 8 && self.flag_show_left_background == 0 {
            background = 0;
        }
        if x < 8 && self.flag_show_left_sprites == 0 {
            sprite = 0;
        }
        let bg = background % 4 != 0;
        let sp = sprite % 4 != 0;
        let color = if !bg && !sp {
            0
        } else if !bg && sp {
            sprite | 0x10
        } else if bg && !sp {
            background
        } else {
            if self.sprite_indexes[i] == 0 && x < 255 {
                self.flag_sprite_zero_hit = 1;
            }
            if self.sprite_priorities[i] == 0 {
                sprite | 0x10
            } else {
                background
            }
        };
        let palette_idx = self.read_palette(u16::from(color)) % 64;
        let color = self.palette[palette_idx as usize];
        self.back.put_pixel(x, y, color);
    }

    pub fn set_vertical_blank(&mut self) {
        std::mem::swap(&mut self.front, &mut self.back);
        self.nmi_occurred = true;
        self.nmi_change();
    }

    pub fn clear_vertical_blank(&mut self) {}

    fn reset(&mut self) {
        self.cycle = 340;
        self.scan_line = 240;
        self.frame = 0;
        self.oam_address = 0;
        self.write_control(0);
        self.write_mask(0);
    }

    // NTSC Timing Helper Functions

    pub fn increment_x(&mut self) {
        // increment hori(v)
        // if coarse X == 31
        if self.v & 0x001F == 31 {
            // coarse X = 0
            self.v &= 0xFFE0;
            // switch horizontal nametable
            self.v ^= 0x0400;
        } else {
            // increment coarse X
            self.v += 1;
        }
    }

    pub fn increment_y(&mut self) {
        // increment vert(v)
        // if fine Y < 7
        if self.v & 0x7000 != 0x7000 {
            // increment fine Y
            self.v += 0x1000;
        } else {
            // fine Y = 0
            self.v &= 0x8FFF;
            // coarse Y
            let mut y = (self.v & 0x03E0) >> 5;
            if y == 29 {
                y = 0;
                // switch vertical nametable
                self.v ^= 0x0800;
            } else if y == 31 {
                // nametable not switched
                y = 0;
            } else {
                y += 1;
            }
            // put coarse Y back into v
            self.v = (self.v & 0xFC1F) | (y << 5);
        }
    }

    pub fn copy_x(&mut self) {
        // Copy X
        // hori(v) = hori(t)
        // v: .....F.. ...EDCBA = t: .....F.. ...EDCBA
        self.v = (self.v & 0xFBE0) | (self.t & 0x041F);
    }

    pub fn copy_y(&mut self) {
        // vert(v) = vert(t)
        // v: .IHGF.ED CBA..... = t: .IHGF.ED CBA.....
        self.v = (self.v & 0x841F) | (self.t & 0x7BE0);
    }
}

impl Default for PPU {
    fn default() -> Self {
        Self::new()
    }
}

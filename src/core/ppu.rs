use image;

// Picture Processing Unit
pub struct PPU {
    pub cycle: isize,     // 0-340
    pub scan_line: isize, // 0-261, 0-239=visible, 240=post, 241-260=vblank, 261=pre
    pub frame: u64,       // frame counter

    // storage variables
    pub palette_data: [u8; 32],
    pub name_table_data: [u8; 2048],
    pub oam_data: [u8; 256], // Object Attribute Memory
    pub front: image::Rgba<u16>,
    pub back: image::Rgba<u16>,

    // PPU registers
    pub v: u16, // current vram address (15 bit)
    pub t: u16, // temporary vram address (15 bit)
    pub x: u8,  // fine x scroll (3 bit)
    pub w: u8,  // write toggle (1 bit)
    pub f: u8,  // even/odd frame flag (1 bit)

    pub register: u8,

    // NMI flags
    pub nmi_occurred: bool,
    pub nmi_output: bool,
    pub nmi_previous: bool,
    pub nmi_delay: u8,

    // background temporary variables
    pub name_table_byte: u8,
    pub attribute_table_byte: u8,
    pub low_tile_byte: u8,
    pub high_tile_byte: u8,
    pub tile_data: u64,

    // sprite temporary variables
    pub sprite_count: isize,
    pub sprite_patterns: [u32; 8],
    pub sprite_positions: [u8; 8],
    pub sprite_priorities: [u8; 8],
    pub sprite_indexes: [u8; 8],

    // $2000 PPUCTRL
    pub flag_name_table: u8,       // 0: $2000; 1: $2400; 2: $2800; 3: $2C00
    pub flag_increment: u8,        // 0: add 1; 1: add 32
    pub flag_sprite_table: u8,     // 0: $0000; 1: $1000; ignored in 8x16 mode
    pub flag_background_table: u8, // 0: $0000; 1: $1000
    pub flag_sprite_size: u8,      // 0: 8x8; 1: 8x16
    pub flag_master_slave: u8,     // 0: read EXT; 1: write EXT

    // $2001 PPUMASK
    pub flag_grayscale: u8,            // 0: color; 1: grayscale
    pub flag_show_left_background: u8, // 0: hide; 1: show
    pub flag_show_left_sprites: u8,    // 0: hide; 1: show
    pub flag_show_background: u8,      // 0: hide; 1: show
    pub flag_show_sprites: u8,         // 0: hide; 1: show
    pub flag_red_tint: u8,             // 0: normal; 1: emphasized
    pub flag_green_tint: u8,           // 0: normal; 1: emphasized
    pub flag_blue_tint: u8,            // 0: normal; 1: emphasized

    // $2002 PPUSTATUS
    pub flag_sprite_zero_hit: u8,
    pub flag_sprite_overflow: u8,

    // $2003 OAMADDR
    pub oam_address: u8,
    // $2007 PPUDATA
    pub buffered_data: u8, // for buffered reads
}

impl PPU {
    pub fn new() -> Self {
        PPU {
            cycle: 340,
            scan_line: 250,
            frame: 0,
            palette_data: [0; 32],
            name_table_data: [0; 2048],
            oam_data: [0; 256],
            front: image::Rgba([0, 0, 256, 240]),
            back: image::Rgba([0, 0, 256, 240]),
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
            flag_increment: 0,
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
        }
    }
}

impl Default for PPU {
    fn default() -> Self {
        Self::new()
    }
}

use png;
use std::{
    ffi::OsStr,
    fs,
    io::{BufReader, BufWriter, Error, ErrorKind, Result},
    path::Path,
};

/// Represents an RGBA pixel
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Pixel blending mode
///   Normal: Ignores alpha channel blending
///   Mask: Only displays pixels if alpha == 255
///   Blend: Blends together alpha channels
#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum AlphaMode {
    Normal, // Ignore alpha channel
    Mask,   // Only blend alpha if less than 255
    Blend,  // Always blend alpha
}

impl Pixel {
    /// Create a black pixel
    pub fn new() -> Self {
        BLACK
    }
    /// Create a pixel using RGB values with alpha at 255
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    /// Create a pixel using RGBA values
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    /// Convert pixel to a u32 integer
    // More efficient to pass by value since Pixel is less than 8 bytes
    pub fn to_u32(self) -> u32 {
        (u32::from(self.r) << 24)
            | (u32::from(self.g) << 16)
            | (u32::from(self.b) << 8)
            | u32::from(self.a)
    }
    /// Create a pixel from a u32 integer
    pub fn from_u32(p: u32) -> Self {
        Self {
            r: (p >> 24) as u8,
            g: (p >> 16) as u8,
            b: (p >> 8) as u8,
            a: (p & 0xFF) as u8,
        }
    }
    /// Convnience function to create a highlight version of a pixel
    pub fn highlight(&self) -> Pixel {
        Self {
            r: self.r.saturating_add(128),
            g: self.g.saturating_add(128),
            b: self.b.saturating_add(128),
            a: self.a,
        }
    }
    /// Convnience function to create a shadowed version of a pixel
    pub fn shadow(&self) -> Pixel {
        Self {
            r: self.r.saturating_sub(128),
            g: self.g.saturating_sub(128),
            b: self.b.saturating_sub(128),
            a: self.a,
        }
    }
}

impl Default for Pixel {
    fn default() -> Self {
        WHITE
    }
}

/// Represents a 2D array of pixels
#[derive(Clone)]
pub struct Sprite {
    width: i32,
    height: i32,
    pixels: Vec<Pixel>,
}

impl Sprite {
    /// Creates an empty sprite with no width or height
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        }
    }
    /// Creates a new sprite with given size
    pub fn with_size(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            pixels: vec![BLANK; (width * height) as usize],
        }
    }
    /// Create a new sprite from an existing set of pixels
    pub fn from_pixels(width: i32, height: i32, pixels: Vec<Pixel>) -> Self {
        Self {
            width,
            height,
            pixels,
        }
    }
    /// Create a new sprite from a PNG file
    /// Only 8-bit RGBA formats are supported currently
    pub fn from_file<P: AsRef<Path>>(file: P) -> Result<Self> {
        let path = file.as_ref();
        if path.extension() != Some(OsStr::new("png")) {
            return Err(Error::new(ErrorKind::Other, "invalid png file"));
        }

        let png_file = BufReader::new(fs::File::open(&path)?);
        let png = png::Decoder::new(png_file);
        let (info, mut reader) = png.read_info()?;
        let mut raw_pixels = vec![0; info.buffer_size()];
        reader.next_frame(&mut raw_pixels).unwrap();

        assert_eq!(
            info.color_type,
            png::ColorType::RGBA,
            "Only RGBA formats supported right now."
        );
        assert_eq!(
            info.bit_depth,
            png::BitDepth::Eight,
            "Only 8-bit formats supported right now."
        );
        let mut pixels = Vec::with_capacity((info.width * info.height) as usize);
        for y in 0..info.height {
            for x in 0..info.width {
                let index = 4 * (y * info.width + x) as usize;
                let pixel = Pixel::rgba(
                    raw_pixels[index],
                    raw_pixels[index + 1],
                    raw_pixels[index + 2],
                    raw_pixels[index + 3],
                );
                pixels.push(pixel);
            }
        }
        Ok(Self {
            width: info.width as i32,
            height: info.height as i32,
            pixels,
        })
    }
    /// Saves a sprite out to a png file
    pub fn save_to_file<P: AsRef<Path>>(&mut self, file: P) -> Result<()> {
        let path = file.as_ref();
        let png_file = BufWriter::new(fs::File::create(&path)?);
        let mut png = png::Encoder::new(png_file, self.width as u32, self.height as u32);
        png.set_color(png::ColorType::RGBA);
        let mut writer = png.write_header()?;
        writer.write_image_data(&self.to_bytes())?;
        Ok(())
    }
    /// Gets the pixel at the given (x, y) coords
    pub fn get_pixel(&self, x: i32, y: i32) -> Pixel {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.pixels[(y * self.width + x) as usize]
        } else {
            BLANK
        }
    }
    /// Sets the pixel at the given (x, y) coords
    pub fn set_pixel(&mut self, x: i32, y: i32, p: Pixel) -> bool {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.pixels[(y * self.width + x) as usize] = p;
            true
        } else {
            false
        }
    }
    /// Width of the sprite
    pub fn width(&self) -> i32 {
        self.width
    }
    /// Updates the sprite width
    pub fn set_width(&mut self, w: i32) {
        let old_width = self.width;
        self.width = w;
        let mut pixels = vec![BLANK; (self.width * self.height) as usize];
        for x in 0..self.width {
            for y in 0..self.height {
                let p = if x <= old_width {
                    self.get_pixel(x, y)
                } else {
                    BLANK
                };
                pixels[(y * self.width + x) as usize] = p;
            }
        }
        self.pixels = pixels;
    }
    /// Height of the sprite
    pub fn height(&self) -> i32 {
        self.height
    }
    /// Updates the sprite height
    pub fn set_height(&mut self, h: i32) {
        let old_height = self.height;
        self.height = h;
        let mut pixels = vec![BLANK; (self.width * self.height) as usize];
        for x in 0..self.width {
            for y in 0..self.height {
                let p = if y <= old_height {
                    self.get_pixel(x, y)
                } else {
                    BLANK
                };
                pixels[(y * self.width + x) as usize] = p;
            }
        }
        self.pixels = pixels;
    }
    /// Returns the sprite as raw u8 bytes
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for p in self.pixels.iter() {
            bytes.push(p.r);
            bytes.push(p.g);
            bytes.push(p.b);
            bytes.push(p.a);
        }
        bytes
    }
    /// Returns a reference to the pixels within the sprite
    pub fn as_pixels(&self) -> &Vec<Pixel> {
        &self.pixels
    }
}

impl Default for Sprite {
    fn default() -> Self {
        Self::new()
    }
}

/// Pixel Constants

// White/Black/Blank
pub static WHITE: Pixel = Pixel {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
pub static BLACK: Pixel = Pixel {
    r: 0,
    g: 0,
    b: 0,
    a: 255,
};
pub static BLANK: Pixel = Pixel {
    r: 0,
    g: 0,
    b: 0,
    a: 0,
};

// Gray
pub static GRAY: Pixel = Pixel {
    r: 192,
    g: 192,
    b: 192,
    a: 255,
};
pub static DARK_GRAY: Pixel = Pixel {
    r: 128,
    g: 128,
    b: 128,
    a: 255,
};
pub static VERY_DARK_GRAY: Pixel = Pixel {
    r: 64,
    g: 64,
    b: 64,
    a: 255,
};

// Red
pub static RED: Pixel = Pixel {
    r: 255,
    g: 0,
    b: 0,
    a: 255,
};
pub static DARK_RED: Pixel = Pixel {
    r: 128,
    g: 0,
    b: 0,
    a: 255,
};
pub static VERY_DARK_RED: Pixel = Pixel {
    r: 64,
    g: 0,
    b: 0,
    a: 255,
};

// Orange
pub static ORANGE: Pixel = Pixel {
    r: 255,
    g: 128,
    b: 0,
    a: 255,
};

// Yellow
pub static YELLOW: Pixel = Pixel {
    r: 255,
    g: 255,
    b: 0,
    a: 255,
};
pub static DARK_YELLOW: Pixel = Pixel {
    r: 128,
    g: 128,
    b: 0,
    a: 255,
};
pub static VERY_DARK_YELLOW: Pixel = Pixel {
    r: 64,
    g: 64,
    b: 0,
    a: 255,
};

// Green
pub static GREEN: Pixel = Pixel {
    r: 0,
    g: 255,
    b: 0,
    a: 255,
};
pub static DARK_GREEN: Pixel = Pixel {
    r: 0,
    g: 128,
    b: 0,
    a: 255,
};
pub static VERY_DARK_GREEN: Pixel = Pixel {
    r: 0,
    g: 64,
    b: 0,
    a: 255,
};

// Cyan
pub static CYAN: Pixel = Pixel {
    r: 0,
    g: 255,
    b: 255,
    a: 255,
};
pub static DARK_CYAN: Pixel = Pixel {
    r: 0,
    g: 128,
    b: 128,
    a: 255,
};
pub static VERY_DARK_CYAN: Pixel = Pixel {
    r: 0,
    g: 64,
    b: 64,
    a: 255,
};

// Blue
pub static BLUE: Pixel = Pixel {
    r: 0,
    g: 0,
    b: 255,
    a: 255,
};
pub static DARK_BLUE: Pixel = Pixel {
    r: 0,
    g: 0,
    b: 128,
    a: 255,
};
pub static VERY_DARK_BLUE: Pixel = Pixel {
    r: 0,
    g: 0,
    b: 64,
    a: 255,
};

// Magenta
pub static MAGENTA: Pixel = Pixel {
    r: 255,
    g: 0,
    b: 255,
    a: 255,
};
pub static DARK_MAGENTA: Pixel = Pixel {
    r: 128,
    g: 0,
    b: 128,
    a: 255,
};
pub static VERY_DARK_MAGENTA: Pixel = Pixel {
    r: 64,
    g: 0,
    b: 64,
    a: 255,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_new() {
        let pixel = Pixel::new();
        assert_eq!(pixel.r, 0);
        assert_eq!(pixel.g, 0);
        assert_eq!(pixel.b, 0);
        assert_eq!(pixel.a, 255);
    }

    #[test]
    fn test_pixel_rgb() {
        let pixel = Pixel::rgb(1, 2, 3);
        assert_eq!(pixel.r, 1);
        assert_eq!(pixel.g, 2);
        assert_eq!(pixel.b, 3);
        assert_eq!(pixel.a, 255);
    }

    #[test]
    fn test_pixel_rgba() {
        let pixel = Pixel::rgba(1, 2, 3, 5);
        assert_eq!(pixel.r, 1);
        assert_eq!(pixel.g, 2);
        assert_eq!(pixel.b, 3);
        assert_eq!(pixel.a, 5);
    }

    #[test]
    fn test_pixel_to_u32() {
        let pixel = Pixel::rgb(1, 2, 3);
        assert_eq!(pixel.to_u32(), 0x010203FF);

        let pixel = Pixel::new_with_alpha(1, 2, 3, 5);
        assert_eq!(pixel.to_u32(), 0x01020305);
    }

    #[test]
    fn test_pixel_from_u32() {
        let pixel = Pixel::from_u32(0x010203FF);
        assert_eq!(pixel.r, 1);
        assert_eq!(pixel.g, 2);
        assert_eq!(pixel.b, 3);
        assert_eq!(pixel.a, 255);
    }
}

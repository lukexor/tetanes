use png;
use std::{
    ffi::OsStr,
    fs,
    io::{BufReader, Error, ErrorKind, Result},
    path::Path,
};

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub struct Pixel {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, PartialEq, Eq, Copy, Clone)]
pub enum AlphaMode {
    Normal, // Ignore alpha channel
    Mask,   // Only blend alpha if less than 255
    Blend,  // Always blend alpha
}

impl Pixel {
    pub fn new() -> Self {
        Self {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    }
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
    // More efficient to pass by value since Pixel is less than 8 bytes
    pub fn to_u32(self) -> u32 {
        (u32::from(self.r) << 24)
            | (u32::from(self.g) << 16)
            | (u32::from(self.b) << 8)
            | u32::from(self.a)
    }
    pub fn from_u32(p: u32) -> Self {
        Self {
            r: (p >> 24) as u8,
            g: (p >> 16) as u8,
            b: (p >> 8) as u8,
            a: (p & 0xFF) as u8,
        }
    }
}

impl Default for Pixel {
    fn default() -> Self {
        WHITE
    }
}

// Stores a 2D array of Pixels
#[derive(Clone)]
pub struct Sprite {
    width: i32,
    height: i32,
    pixels: Vec<Pixel>,
}

impl Sprite {
    pub fn new() -> Self {
        Self {
            width: 0,
            height: 0,
            pixels: Vec::new(),
        }
    }

    pub fn with_size(width: i32, height: i32) -> Self {
        Self {
            width,
            height,
            pixels: vec![BLACK; (width * height) as usize],
        }
    }

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

    pub fn get_pixel(&self, x: i32, y: i32) -> Pixel {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.pixels[(y * self.width + x) as usize]
        } else {
            BLANK
        }
    }

    pub fn set_pixel(&mut self, x: i32, y: i32, p: Pixel) -> bool {
        if x >= 0 && x < self.width && y >= 0 && y < self.height {
            self.pixels[(y * self.width + x) as usize] = p;
            true
        } else {
            false
        }
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    pub fn as_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pixels.len() * 4);
        for p in self.pixels.iter() {
            bytes.push(p.r);
            bytes.push(p.g);
            bytes.push(p.b);
            bytes.push(p.a);
        }
        bytes
    }

    pub fn into_pixels(self) -> Vec<Pixel> {
        self.pixels
    }
}

impl Default for Sprite {
    fn default() -> Self {
        Self::new()
    }
}

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

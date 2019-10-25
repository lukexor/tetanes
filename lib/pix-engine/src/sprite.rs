use crate::{
    pixel::{self, ColorType, Pixel},
    PixEngineErr, PixEngineResult,
};
use png;
use std::{
    ffi::OsStr,
    io::{BufReader, BufWriter},
};

#[derive(Clone)]
pub struct Sprite {
    width: u32,
    height: u32,
    channels: u8,
    color_type: ColorType,
    data: Vec<u8>,
}

impl Sprite {
    /// Creates a new sprite with given size
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            channels: 4,
            color_type: ColorType::Rgba,
            data: vec![0; 4 * (width * height) as usize],
        }
    }

    pub fn rgb(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            channels: 3,
            color_type: ColorType::Rgb,
            data: vec![0; 3 * (width * height) as usize],
        }
    }

    /// Creates a new sprite from an array of bytes
    pub fn from_bytes(width: u32, height: u32, bytes: &[u8]) -> PixEngineResult<Self> {
        if bytes.len() != (4 * width * height) as usize {
            Err(PixEngineErr::new(
                "width/height does not match bytes length",
            ))
        } else {
            Ok(Self {
                width,
                height,
                channels: 4,
                color_type: ColorType::Rgba,
                data: bytes.to_vec(),
            })
        }
    }

    /// Gets the pixel at the given (x, y) coords
    pub fn get_pixel(&self, x: u32, y: u32) -> Pixel {
        if x < self.width && y < self.height {
            let idx = self.channels as usize * (y * self.width + x) as usize;
            Pixel([
                self.data[idx],
                self.data[idx + 1],
                self.data[idx + 2],
                if self.channels == 3 {
                    255
                } else {
                    self.data[idx + 3]
                },
            ])
        } else {
            pixel::TRANSPARENT
        }
    }

    /// Sets the pixel at the given (x, y) coords
    pub fn put_pixel(&mut self, x: u32, y: u32, p: Pixel) {
        if x < self.width && y < self.height {
            let idx = self.channels as usize * (y * self.width + x) as usize;
            for i in 0..self.channels as usize {
                self.data[idx + i] = p[i];
            }
        }
    }

    /// ColorType of the sprite
    pub fn color_type(&self) -> ColorType {
        self.color_type
    }

    /// Width of the sprite
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Height of the sprite
    pub fn height(&self) -> u32 {
        self.height
    }

    /// Returns a reference to the pixels within the sprite
    pub fn bytes(&self) -> &Vec<u8> {
        &self.data
    }

    /// Returns a mutable reference to the pixels within the sprite
    pub fn bytes_mut(&mut self) -> &mut Vec<u8> {
        &mut self.data
    }

    /// Create a new sprite from a PNG file
    /// Only 8-bit RGBA formats are supported currently
    pub fn from_file(file: &str) -> PixEngineResult<Self> {
        use std::path::PathBuf;
        let path = PathBuf::from(file);
        if path.extension() != Some(OsStr::new("png")) {
            return Err(PixEngineErr::new("invalid png file"));
        }

        let png_file = BufReader::new(std::fs::File::open(&path)?);
        let png = png::Decoder::new(png_file);
        let (info, mut reader) = png.read_info()?;

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

        let mut data = vec![0; info.buffer_size()];
        reader.next_frame(&mut data).unwrap();

        Sprite::from_bytes(info.width, info.height, &data)
    }

    /// Saves a sprite out to a png file
    pub fn save_to_file(&mut self, file: &str) -> PixEngineResult<()> {
        use std::path::PathBuf;
        let path = PathBuf::from(file);
        let png_file = BufWriter::new(std::fs::File::create(&path)?);
        let mut png = png::Encoder::new(png_file, self.width, self.height);
        png.set_color(png::ColorType::RGBA);
        let mut writer = png.write_header()?;
        writer.write_image_data(self.bytes())?;
        Ok(())
    }
}

impl From<png::DecodingError> for PixEngineErr {
    fn from(err: png::DecodingError) -> Self {
        Self::new(err)
    }
}
impl From<png::EncodingError> for PixEngineErr {
    fn from(err: png::EncodingError) -> Self {
        Self::new(err)
    }
}

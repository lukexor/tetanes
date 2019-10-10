use image::{self, DynamicImage, GenericImage, GenericImageView, ImageFormat, RgbImage, Rgba};
use png;
use std::{
    ffi::OsStr,
    fs,
    io::{BufReader, BufWriter, Error, ErrorKind, Result},
    path::Path,
};

pub type Sprite = DynamicImage;
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ColorType {
    RGB,
    RGBA,
}

/// Create a new sprite from a PNG file
/// Only 8-bit RGBA formats are supported currently
pub fn load_from_file<P: AsRef<Path>>(file: P) -> Result<DynamicImage> {
    let path = file.as_ref();
    if path.extension() != Some(OsStr::new("png")) {
        return Err(Error::new(ErrorKind::Other, "invalid png file"));
    }
    let png_file = BufReader::new(fs::File::open(&path)?);
    let png = png::Decoder::new(png_file);
    let (info, mut reader) = png.read_info()?;
    let mut data = vec![0; info.buffer_size()];
    reader.next_frame(&mut data).unwrap();
    let image = image::load_from_memory_with_format(&data, ImageFormat::PNG);
    if let Ok(img) = image {
        Ok(img)
    } else {
        Err(Error::new(ErrorKind::Other, "failed to load png file"))
    }
}

pub fn save_to_file<P: AsRef<Path>>(image: DynamicImage, file: P) -> Result<()> {
    let path = file.as_ref();
    let png_file = BufWriter::new(fs::File::create(&path)?);
    let mut png = png::Encoder::new(png_file, image.width(), image.height());
    png.set_color(png::ColorType::RGBA);
    let mut writer = png.write_header()?;
    writer.write_image_data(&image.raw_pixels())?;
    Ok(())
}

pub fn rgb_from_bytes(width: u32, height: u32, bytes: Vec<u8>) -> Result<DynamicImage> {
    let image = RgbImage::from_raw(width, height, bytes).expect("Loaded image");
    Ok(DynamicImage::ImageRgb8(image))
}

/// Rgba Constants

// White/Black/Blank
pub static WHITE: Rgba<u8> = Rgba([255, 255, 255, 255]);
pub static BLACK: Rgba<u8> = Rgba([0, 0, 0, 255]);
pub static TRANSPARENT: Rgba<u8> = Rgba([0, 0, 0, 0]);

// Gray
pub static GRAY: Rgba<u8> = Rgba([192, 192, 192, 255]);
pub static DARK_GRAY: Rgba<u8> = Rgba([128, 128, 128, 255]);
pub static VERY_DARK_GRAY: Rgba<u8> = Rgba([64, 64, 64, 255]);

// Red
pub static RED: Rgba<u8> = Rgba([255, 0, 0, 255]);
pub static DARK_RED: Rgba<u8> = Rgba([128, 0, 0, 255]);
pub static VERY_DARK_RED: Rgba<u8> = Rgba([64, 0, 0, 255]);

// Orange
pub static ORANGE: Rgba<u8> = Rgba([255, 128, 0, 255]);

// Yellow
pub static YELLOW: Rgba<u8> = Rgba([255, 255, 0, 255]);
pub static DARK_YELLOW: Rgba<u8> = Rgba([128, 128, 0, 255]);
pub static VERY_DARK_YELLOW: Rgba<u8> = Rgba([64, 64, 0, 255]);

// Green
pub static GREEN: Rgba<u8> = Rgba([0, 255, 0, 255]);
pub static DARK_GREEN: Rgba<u8> = Rgba([0, 128, 0, 255]);
pub static VERY_DARK_GREEN: Rgba<u8> = Rgba([0, 64, 0, 255]);

// Cyan
pub static CYAN: Rgba<u8> = Rgba([0, 255, 255, 255]);
pub static DARK_CYAN: Rgba<u8> = Rgba([0, 128, 128, 255]);
pub static VERY_DARK_CYAN: Rgba<u8> = Rgba([0, 64, 64, 255]);

// Blue
pub static BLUE: Rgba<u8> = Rgba([0, 0, 255, 255]);
pub static DARK_BLUE: Rgba<u8> = Rgba([0, 0, 128, 255]);
pub static VERY_DARK_BLUE: Rgba<u8> = Rgba([0, 0, 64, 255]);

// Magenta
pub static MAGENTA: Rgba<u8> = Rgba([255, 0, 255, 255]);
pub static DARK_MAGENTA: Rgba<u8> = Rgba([128, 0, 128, 255]);
pub static VERY_DARK_MAGENTA: Rgba<u8> = Rgba([64, 0, 64, 255]);

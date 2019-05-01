use dirs;
use gl::types::*;
use image::{ImageFormat, RgbaImage};
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    fs::File,
    io::{BufReader, Read},
    path::PathBuf,
};

pub fn hash_file(path: &PathBuf) -> Result<String, Box<Error>> {
    let mut file = File::open(path)?;
    let mut buf = [0u8; 255];
    file.read_exact(&mut buf)?;
    Ok(format!("{:x}", Sha256::digest(&buf)))
}

pub fn home_dir() -> String {
    dirs::home_dir()
        .unwrap()
        .into_os_string()
        .into_string()
        .unwrap()
}

pub fn load_image(path: &str) -> Result<RgbaImage, Box<Error>> {
    let file = File::open(path)?;
    let file = BufReader::new(file);
    Ok(image::load(file, ImageFormat::PNG)?.to_rgba())
}

pub fn create_texture() -> u32 {
    let mut texture: GLuint = 0;
    unsafe {
        gl::GenTextures(1, &mut texture);
        gl::BindTexture(gl::TEXTURE_2D, texture);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as GLint);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as GLint);
        gl::TexParameteri(
            gl::TEXTURE_2D,
            gl::TEXTURE_WRAP_S,
            gl::CLAMP_TO_EDGE as GLint,
        );
        gl::TexParameteri(
            gl::TEXTURE_2D,
            gl::TEXTURE_WRAP_T,
            gl::CLAMP_TO_EDGE as GLint,
        );
        let color: [GLfloat; 4] = [1.0, 0.0, 0.0, 1.0];
        gl::TexParameterfv(gl::TEXTURE_2D, gl::TEXTURE_BORDER_COLOR, color.as_ptr());
        gl::GenerateMipmap(gl::TEXTURE_2D);
        gl::BindTexture(gl::TEXTURE_2D, 0);
    }
    texture
}

pub fn set_texture(image: &RgbaImage, offset: usize) {
    let pixels = image.as_flat_samples().samples;
    let pixels = &pixels[offset..];
    unsafe {
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as GLint,
            image.width() as GLint,
            image.height() as GLint,
            0,
            gl::RGBA as GLuint,
            gl::UNSIGNED_BYTE,
            pixels.as_ptr() as *const GLvoid,
        );
    }
}

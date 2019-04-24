use dirs;
use image::RgbaImage;
use sha2::{Digest, Sha256};
use std::{error::Error, fs::File, io::Read, path::PathBuf};

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

pub fn create_texture() -> u32 {
    let mut texture = 0u32;
    unsafe {
        gl::GenTextures(1, &mut texture);
        gl::BindTexture(gl::TEXTURE_2D, texture);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
        gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
        gl::BindTexture(gl::TEXTURE_2D, 0);
    }
    texture
}

pub fn set_texture(image: RgbaImage) {
    let pixels: Vec<&image::Rgba<u8>> = image.pixels().collect();
    unsafe {
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGBA as i32,
            image.width() as i32,
            image.height() as i32,
            0,
            gl::RGBA as u32,
            gl::UNSIGNED_BYTE,
            pixels.as_ptr() as *const gl::types::GLvoid,
        );
    }
}

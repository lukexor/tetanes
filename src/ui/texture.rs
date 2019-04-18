use super::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::ptr;

pub struct Texture {
    texture: u32,
    lookup: HashMap<String, isize>,
    reverse: [&'static str; TEXTURE_COUNT as usize],
    access: [isize; TEXTURE_COUNT as usize],
    counter: isize,
    channel: String,
}

impl Texture {
    pub fn new() -> Self {
        let mut texture: u32 = 0;
        unsafe {
            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as i32,
                TEXTURE_SIZE,
                TEXTURE_SIZE,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                ptr::null(),
            );
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }
        Texture {
            texture,
            lookup: HashMap::new(),
            reverse: [""; TEXTURE_COUNT as usize],
            access: [0; TEXTURE_COUNT as usize],
            counter: 0,
            channel: String::with_capacity(1024),
        }
    }
    pub fn lookup_index(&mut self, rom: &PathBuf) -> i32 {
        0
        // let mut index = 0;
        // {
        //     let mut min = self.counter + 1;
        //     for (i, n) in t.access.enumerate() {
        //         if n < min {
        //             index = i;
        //             min = n;
        //         }
        //     }
        // }

        // t.lookup.remove(t.reverse[index]).unwrap();
        // t.counter += 1;
        // t.access[index] = t.counter;
        // t.lookup.insert(rom.to_string(), index);
        // t.reverse[index] = rom;
    }
}

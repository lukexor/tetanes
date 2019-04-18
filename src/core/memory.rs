use super::*;

pub fn prg_bank_offset(cartridge: &Cartridge, mut index: isize, offset: isize) -> isize {
    if index >= 0x80 {
        index -= 0x100;
    }
    index %= cartridge.prg.len() as isize / offset;
    let mut offset = index * offset;
    if offset < 0 {
        offset += cartridge.prg.len() as isize;
    }
    offset
}

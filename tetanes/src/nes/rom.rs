#[derive(Clone, PartialEq)]
pub struct RomData(pub Vec<u8>);

impl std::fmt::Debug for RomData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RomData({} bytes)", self.0.len())
    }
}

impl AsRef<[u8]> for RomData {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Debug)]
#[must_use]
pub struct RomAsset<const N: usize> {
    pub name: &'static str,
    pub data: &'static [u8; N],
}

impl<const N: usize> RomAsset<N> {
    pub const fn new(name: &'static str, data: &'static [u8; N]) -> Self {
        Self { name, data }
    }
}

#[derive(Debug)]
#[must_use]
pub struct RomAssets {
    alter_ego: RomAsset<40976>,
    ao: RomAsset<24592>,
    assimilate: RomAsset<262160>,
    blade_buster: RomAsset<393232>,
    from_below: RomAsset<40976>,
    lan_master: RomAsset<40976>,
    streemerz: RomAsset<131088>,
}

impl RomAssets {
    pub fn names<'a>(&'a self) -> impl Iterator<Item = &'static str> + 'a {
        RomAssetsIter {
            index: 0,
            roms: self,
        }
    }

    pub fn data(&self, name: &'static str) -> Option<RomData> {
        let data = if name == self.alter_ego.name {
            self.alter_ego.data.to_vec()
        } else if name == self.ao.name {
            self.ao.data.to_vec()
        } else if name == self.assimilate.name {
            self.assimilate.data.to_vec()
        } else if name == self.blade_buster.name {
            self.blade_buster.data.to_vec()
        } else if name == self.from_below.name {
            self.from_below.data.to_vec()
        } else if name == self.lan_master.name {
            self.lan_master.data.to_vec()
        } else if name == self.streemerz.name {
            self.streemerz.data.to_vec()
        } else {
            return None;
        };
        Some(RomData(data))
    }
}

#[derive(Debug)]
#[must_use]
struct RomAssetsIter<'a> {
    index: usize,
    roms: &'a RomAssets,
}

impl<'a> Iterator for RomAssetsIter<'a> {
    type Item = &'static str;

    fn next(&mut self) -> Option<Self::Item> {
        let item = match self.index {
            0 => self.roms.alter_ego.name,
            1 => self.roms.ao.name,
            2 => self.roms.assimilate.name,
            3 => self.roms.blade_buster.name,
            4 => self.roms.from_below.name,
            5 => self.roms.lan_master.name,
            6 => self.roms.streemerz.name,
            _ => return None,
        };
        if self.index < 7 {
            self.index += 1;
        }
        Some(item)
    }
}

macro_rules! rom_asset {
    ($name:expr, $filename:expr) => {
        RomAsset::new(
            $name,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/roms/",
                $filename
            )),
        )
    };
}

pub const HOMEBREW_ROMS: RomAssets = RomAssets {
    alter_ego: rom_asset!("Alter Ego", "alter_ego.nes"),
    ao: rom_asset!("AO", "ao_demo.nes"),
    assimilate: rom_asset!("Assimilate", "assimilate.nes"),
    blade_buster: rom_asset!("Blade Buster", "blade_buster.nes"),
    from_below: rom_asset!("From Below", "from_below.nes"),
    lan_master: rom_asset!("Lan Master", "lan_master.nes"),
    streemerz: rom_asset!("Streemerz", "streemerz.nes"),
};

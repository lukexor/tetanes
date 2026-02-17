#[derive(Clone, PartialEq)]
pub(crate) struct RomData(pub(crate) Vec<u8>);

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

#[derive(Copy, Clone)]
#[must_use]
pub(crate) struct RomAsset {
    pub(crate) name: &'static str,
    pub(crate) authors: &'static str,
    pub(crate) description: &'static str,
    pub(crate) source: &'static str,
    pub(crate) data_fn: &'static dyn Fn() -> Vec<u8>,
}

impl std::fmt::Debug for RomAsset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RomAsset")
            .field("name", &self.name)
            .field("authors", &self.authors)
            .field("description", &self.description)
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl RomAsset {
    pub(crate) const fn new(
        name: &'static str,
        authors: &'static str,
        description: &'static str,
        source: &'static str,
        data_fn: &'static dyn Fn() -> Vec<u8>,
    ) -> Self {
        Self {
            name,
            authors,
            description,
            source,
            data_fn,
        }
    }

    pub(crate) fn data(&self) -> RomData {
        RomData((self.data_fn)())
    }
}

macro_rules! rom_assets {
    ($(($name:expr, $filename:expr, $authors:expr, $description:expr, $source:expr$(,)?)),*$(,)?) => {[$(
        {
            fn data_fn() -> Vec<u8> {
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/roms/",
                    $filename
                )).to_vec()
            }
            RomAsset::new(
                $name,
                $authors,
                $description,
                $source,
                &data_fn
            )
        },
    )*]};
}

pub(crate) const HOMEBREW_ROMS: [RomAsset; 18] = rom_assets!(
    (
        "Alter Ego",
        "alter_ego.nes",
        "Denis Grachev, Shiru, and Kulor",
        "The game is a logic platformer. You control a hero and his alter ego. You have to switch between them to clear a level. It is a bit similar to Binary Land.",
        "https://shiru.untergrund.net/software.shtml",
    ),
    (
        "AO Demo",
        "ao_demo.nes",
        "Second Dimension",
        "If you like puzzle games and you're up for a challenge, AO is the game for you. The objective is to roll the brick around the game board and drop it through the goal pit.

It sounds easy, right? You'll have to avoid falling off of the edge while maneuvering around tight areas. Do you think you can make it through all the levels before your score reaches zero? If you run into a wall and get stuck, you won't lose when the timer runs out, so you can still finish all of the levels.

AO features 30 challenging puzzles for 1 or 2 players.",
        "https://www.second-dimension.com/catalog/ao",
    ),
    (
        "Assimilate",
        "assimilate.nes",
        "Nessylum Games",
        "Ever wanted to anal probe someone? Well the wait is over!

Assimilate is the game you and your dopey little friends have been waiting for. Join the super-ship Ossan for twenty plus levels of zombifying, brainwashing, human-dominating excitement. Will you succeed in conquering the entire human race? Or will you cry yourself to sleep after watching Ossan explode in a fiery ball of humiliating destruction?
What happens is up to you.

Begin assimilation.",
        "https://forums.nesdev.org/viewtopic.php?t=7087&hilit=assimilate"
    ),
    (
        "Blade Buster",
        "blade_buster.nes",
        "High Level Challenge",
        "You have 2 minutes in a caravan format to compete the score at 5 minutes. It has become a game with a sense of exhilaration that has good old shoot before being shot playing style.",
        "http://hlc6502.web.fc2.com/Bbuster.htm",
    ),
    (
        "Cheril the Goddess",
        "cheril_the_goddess.nes",
        "The Mojon Twins",
        "You control Cheril. There’s plenty of things to do, and you’ll need special objects. Cheril can only carry ONE object at a time, so if you try to get a new one while you are carring an object, you will drop the one you carry in the place of the one you take.

To get/interchange objects press DOWN. Besides, you’ll have to give those objects a use. To use an object, just walk to the place you want to use it and press DOWN. For Cheril to fly, she has to «push». You can make her «push» by pressing UP.

But beware: «pushing» for too long drains Cheril’s vitality, so you have to do this carefully. Timing short «pushes» can make her gain momentum. Take your time to master the technique, it won’t take long.

Cheril can also fire power balls. You can fire pressing FIRE. Power balls also drain Cheril, but think that enemies do quite a lot of harm. Use this feature wisely, and just when needed!

Cheril can also regain her vitality by means of a special action we won’t reveal ‘cause it’s easy enough to find out!

You can choose the difficulty level, but the easiest level won’t show the real ending.",
        "https://forums.nesdev.org/viewtopic.php?t=15367",
    ),
    (
        "Data Man Demo",
        "data_man_demo.nes",
        "Darkbits (Olof Naessen, Per Larsson, Ted Steen)",
        "Do you have what it takes to save the System?

The Master and his evil minions have invaded the system and will not stop until every piece of data is corrupted. It’s up to you to save it before time runs out and the system crashes!

It won’t be easy. You’ll have to protect the Central Processing Unit from hordes of attacking minions and ultimately from the Master himself. Face it alone, or with a friend!",
        "https://datamangame.com/",
    ),
    (
        "Dushlan",
        "dushlan.nes",
        "Peter McQuillan",
        "The game itself is based on the classic Tetris, but with a few twists on the
game and some extra features that are not commonly available like ghost (where
you can see where your piece would go if you dropped it) and save (where you
can swap a piece in play for later usage).",
        "https://github.com/soiaf/Dushlan",
    ),
    (
        "From Below",
        "from_below.nes",
        "Matt Hughson",
        "FROM BELOW is a falling block puzzle game featuring:

Soft Drops Hard Drops Wall Kicks T-Spins Lock Delay 3 modes of play: Kraken
Battle Mode

The signature mode of FROM BELOW. Battle the Kraken by clear lines across the
onslaught of attacking Kraken Tentacles. The Tentacles push more blocks onto the
screen every few seconds, forcing to act quickly, and strategize on an every
changing board.",
        "https://mhughson.itch.io/from-below/devlog/212679/vs-system-beta-0100",
    ),
    (
        "Lan Master",
        "lan_master.nes",
        "Shiru",
        "Lan Master is a puzzle game for NES, inspired by the game NetWalk. The goal is to connect all of the computers on each level. Rotate the pieces and connect the wires before the timer runs out!

There are fifty levels in all, with increasing difficulty. A password system is included so whether you’re playing in an emulator or on a console, you can come back later and pick up where you left off.",
        "https://shiru.untergrund.net/software.shtml",
    ),
    (
        "Lawn Mower",
        "lawn_mower.nes",
        "Shiru",
        "The goal of this game is to mow all the grass before you run out of gas. Collect gas cans to keep yourself from running on empty.",
        "https://shiru.untergrund.net/software.shtml",
    ),
    (
        "Mad Wizard",
        "mad_wizard.nes",
        "Sly Dog Studios",
        "Take an adventure in the world of Candelabra!

The evil summoner Amondus from The Order of the Talon has taken over Prim, Hekl's once happy homeland. And nothing drives a wizard more crazy than having their territory trampled on!

Can you help Hekl defeat the enemies that Amondus has populated throughout the landscape? To do so, you will need to master the art of levitation, find magic spells that will assist you in reaching new areas, and upgrade your weapons. All of these will be necessary in order to give Hekl the power he needs to restore peace to Prim. Do you have what it takes? If you dare, venture into this, the first installment of the Candelabra series!",
        "The goal of this game is to mow all the grass before you run out of gas. Collect gas cans to keep yourself from running on empty.",
    ),
    (
        "Micro Knight",
        "micro_knight.nes",
        "SDM",
        "",
        "https://forums.nesdev.org/viewtopic.php?t=13450",
    ),
    (
        "Nebs 'n Debs Demo",
        "nebs_n_debs_demo.nes",
        "Dullahan Software",
        "Run, jump, and dash your way through 12 levels as you search for the missing parts of Debs's ship to escape the hostile alien planet Vespasian 7MV! Nebs 'n Debs runs on the same type of game cartridge as the original Super Mario Bros.",
        "https://dullahan-software.itch.io/nebs-n-debs",
    ),
    (
        "Owlia",
        "owlia.nes",
        "Gradual Games",
        "The Legends of Owlia is Gradual Games' second release for the NES. It is an action-adventure game inspired by StarTropics, Crystalis, and the Legend of Zelda. ",
        "https://www.infiniteneslives.com/owlia.php",
    ),
    (
        "Streemerz",
        "streemerz.nes",
        "Mr. Podunkian & Faux Game Co.",
        r#""Try climbing to the top of this one by throwing streamers and climbing them. On your way up you better watch out for the various pie throwing clowns, burning candles and bouncing balls, because if they get you, you'll die a little each time."

These were the orders given to you, Operative JOE when you were ordered to infiltrate the evil MASTER Y's floating fortress to destroy the TIGER ARMY's top secret weapon."#,
        "https://www.fauxgame.com/",
    ),
    (
        "Super Painter",
        "super_painter.nes",
        "RetroSouls & Kulor",
        "Trapped in a colorless world, armed with a paintbrush - there’s only one thing to do! As Super Painter, you’ll have to fill in all the missing color from the walls and ledges of 25 charming stages. Watch out for enemies and pits to the bottom, and don’t box yourself in - when you’re done painting, you’ll have to race to the magic door to the next level. It’s platform puzzling at its finest!",
        "https://www.retrosouls.net/?page_id=901",
    ),
    (
        "Tiger Jenny",
        "tiger_jenny.nes",
        "Ludosity",
        "Tiger Jenny by Ludosity is a NES game set in the same universe as “Ittle Dew” it takes place a thousand years before the events of that game. Battle your way through the forests to seek vengeance on the Turnip Witch who dwells in her castle.",
        "https://pdroms.de/files/nintendo-nintendoentertainmentsystem-nes-famicom-fc/tiger-jenny",
    ),
    (
        "Yun",
        "yun.nes",
        "The Mojon Twins",
        "The main goal is helping yun capturing every single being to fill the pantry of her restaurant. The Big Marsh near Lake Potoña (province of Badajoz), formed by three areas (the marsh wood, the marsh abandoned factory and the mash desert) is full of walking flesh Yun must capture.

To capture her enemies, Yun must stun them by means of hitting them with a bubble. Once they are stunned, they can be captured just touching them.

Yun’s bubbles are quite resistant. You can hump on them and let them carry you upwards, which is sometimes the only way to progress in the level.

Besides, there’s some points where Yun might need a key to keep going.",
        "https://www.mojontwins.com/juegos_mojonos/yun/",
    ),
);

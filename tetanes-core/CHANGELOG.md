<!-- markdownlint-disable-file no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.15.0](https://github.com/lukexor/tetanes/compare/0.14.2..0.15.0) - 2026-08-06

### ⛰️  Features

- *(apu)* Add band-limited step synthesis - ([a6f0df0](https://github.com/lukexor/tetanes/commit/a6f0df0edbcb4eb8f905bbe75e38820412f6a5fa))
- *(bench)* TETANES_BENCH_FILTER selects the output filter - ([36b2646](https://github.com/lukexor/tetanes/commit/36b26463ac6bdec04f4f57f9879b4624674fd345))
- *(core)* [**breaking**] One patch table for every kind of cheat - ([5214d3b](https://github.com/lukexor/tetanes/commit/5214d3bf2a7f54a8320ada1d4545814cf07321a8))
**BREAKING**: `Bus::genie_codes` is replaced by `Bus::patches`, and a console state no longer carries cheats.
- *(core)* [**breaking**] Default to a 48 kHz sample rate - ([7f2071a](https://github.com/lukexor/tetanes/commit/7f2071a3e875ee1c750d2ffe77e40e48b4b459e6))
**BREAKING**: `Apu::DEFAULT_SAMPLE_RATE` is 48 kHz, so a deck that does not call `set_sample_rate` produces different samples than before.
- *(core)* Serialize console state into a caller's buffer - ([52e00af](https://github.com/lukexor/tetanes/commit/52e00af6d7876922af707cace6060b7986248c4b))
- *(core)* Add page-based cartridge memory translation - ([398793e](https://github.com/lukexor/tetanes/commit/398793e54fd22944d0a0403eb45cff05bf617bac))
- *(libretro)* Expose the console's memory to the frontend - ([1538183](https://github.com/lukexor/tetanes/commit/1538183b92974582527501009d18689f759b07f3))
- *(mapper)* Add Taito TC0190/TC0350 and TC0690 (Mappers 33, 48) - ([b127fe6](https://github.com/lukexor/tetanes/commit/b127fe62b37edcfd95ed9ed23cbb7822bb60779b))
- *(mapper)* Add VRC2/VRC4 (Mappers 021, 022, 023, 025) - ([6cde160](https://github.com/lukexor/tetanes/commit/6cde1600af04cbe17251ab58ae4be29190b660e1))
- *(mapper)* Report board registers for a debugger pane - ([571d870](https://github.com/lukexor/tetanes/commit/571d87051c513549ece940eb19fee7f046722ab3))
- *(mapper)* Implement MMC5's vertical split screen - ([bdfd9e8](https://github.com/lukexor/tetanes/commit/bdfd9e8f2235dcbef55589660babc852022389cd))
- *(mapper)* [**breaking**] Serve MMC5 from page tables - ([1299ca9](https://github.com/lukexor/tetanes/commit/1299ca91f836945f7f28f1490c84e89762345411))
**BREAKING**: `Exrom::read_ex_ram`, `write_ex_ram`, `rom_select`, `set_prg_bank_range`, `update_prg_banks` and `update_chr_banks` are gone; MMC5 serves its memory from page tables and republishes banking through `Map::update_banks`. `Cart::chr_size` goes with the last board that needed it.
- *(mapper)* Add a PPU read hook for boards that synthesise CHR reads - ([1a0a68e](https://github.com/lukexor/tetanes/commit/1a0a68e648a3a4942af06c963d1fbd68399cab5c))
- *(mapper)* Serve FK23C from page tables - ([98fb652](https://github.com/lukexor/tetanes/commit/98fb6523fc6da2c56209e2219b03ad4625d9c7d4))
- *(mapper)* Serve BandaiFCG from page tables - ([53db1d7](https://github.com/lukexor/tetanes/commit/53db1d78f6a5e0cb5efef722fe0680d03c942135))
- *(mapper)* Serve Namco163 from page tables - ([7e121ec](https://github.com/lukexor/tetanes/commit/7e121eceb453718b69f8c179d43bd9f6791981e6))
- *(mapper)* Serve MMC2 and MMC4 from page tables - ([91aa8c5](https://github.com/lukexor/tetanes/commit/91aa8c5322ffc760e00e2dbe67edab0b8e358d40))
- *(mapper)* Serve NES-EVENT from page tables - ([bde36ff](https://github.com/lukexor/tetanes/commit/bde36ff887ecc64c9a39911fbf3df6f358438568))
- *(mapper)* Serve VRC6 from page tables - ([621acb0](https://github.com/lukexor/tetanes/commit/621acb0a492861d5f9253ec9e5dabd45f2d3db3e))
- *(mapper)* Serve FME7 and Jaleco SS88006 from page tables - ([cd60771](https://github.com/lukexor/tetanes/commit/cd607715a0d39cbdfc3e891b9363de6b4a11978b))
- *(mapper)* [**breaking**] Serve MMC3 from page tables - ([c3af9a0](https://github.com/lukexor/tetanes/commit/c3af9a065f1dad9f4b2d662b09dbc86c421ef2de))
**BREAKING**: `Mmc3::set_chr_banks` is gone, along with the board's own `update_prg_banks`/`update_chr_banks`. MMC3 republishes its banking through `Map::update_banks` from the register file, and four-screen carts allocate 4K of CIRAM rather than a separate `ex_ram` buffer.
- *(mapper)* Serve MMC1 from page tables - ([00a62b4](https://github.com/lukexor/tetanes/commit/00a62b461e034e7a2001e1816e070eee06ad1ae7))
- *(mapper)* Serve tier 3a bank-switchers from page tables - ([9b63b40](https://github.com/lukexor/tetanes/commit/9b63b40efd92f8a48920b639107b3e306a2df0f2))
- *(mapper)* Serve NROM from page tables - ([9e7e71e](https://github.com/lukexor/tetanes/commit/9e7e71e08c1ad1dac3b77d680e3eaba65e51cdf8))


### 🐛 Bug Fixes

- *(apu)* Size the BLEP buffer from the rate it is tuned to - ([8e5179b](https://github.com/lukexor/tetanes/commit/8e5179b7c2ec1c73a9a712c74f879b44731bb2e7))
- *(audio)* Pace on a deadline and hold the buffer with rate control - ([8d27d27](https://github.com/lukexor/tetanes/commit/8d27d27bcb8572e6da5e9c2f46de52c045a6c437))
- *(cart)* Load Dream Master as mapper 210 - ([05fdef3](https://github.com/lukexor/tetanes/commit/05fdef3e82b8a48bf907092e662d151df26c3c55))
- *(cart)* Load Famicom Jump II as mapper 153 - ([ea94174](https://github.com/lukexor/tetanes/commit/ea94174b82616a690c51fa374deda73178d2b311))
- *(cart)* Read the NES 2.0 ram size bytes as two nibbles - ([2110e9f](https://github.com/lukexor/tetanes/commit/2110e9f90093a2f9328271db8eed733f05c9c4db))
- *(cart)* Correct Top Rider's mapper and stop the game db self-referencing - ([fa6098f](https://github.com/lukexor/tetanes/commit/fa6098fcbde1b2ef7b0de558d16decc6ece88a95))
- *(control-deck)* Stop a restored state reverting the player's settings - ([e3b64a8](https://github.com/lukexor/tetanes/commit/e3b64a8295a37f0f2b673b1749d80014a73bcbf1))
- *(control-deck)* Refresh the frame when the console moves without a clock - ([ea4575f](https://github.com/lukexor/tetanes/commit/ea4575f19360afd74aaf0aede47b369b3f087437))
- *(core)* Power-cycle cart RAM on a hard reset - ([9baa46e](https://github.com/lukexor/tetanes/commit/9baa46e4277bf1ca251183215f35c64582ccbada))
- *(core)* [**breaking**] Match a save state to its cart by ROM CRC, not by size - ([52632c2](https://github.com/lukexor/tetanes/commit/52632c233c9ee3d24407129e7aa03ec415218f4d))
**BREAKING**: save states record the cart's ROM CRC32; states written by earlier builds no longer load.
- *(core)* [**breaking**] Hand out one nes frame per clock_frame call - ([7cca0af](https://github.com/lukexor/tetanes/commit/7cca0af3c7cc03a83bdf6ae6329a6b17c8a8c8b3))
**BREAKING**: `ControlDeck::clock_frame` returns `Result<Clocked>` rather than `Result<()>` and clocks at most one NES frame per call; drive it with `while deck.clock_frame()? == Clocked::Continue {}` to clock a whole display frame as before.
- *(core)* Keep the player's settings out of restored states - ([8afede0](https://github.com/lukexor/tetanes/commit/8afede031ebe80fb198d59d41ca566749466435a))
- *(core)* Keep load_bytes on the save version - ([be52911](https://github.com/lukexor/tetanes/commit/be52911a24cecca50c57c857fb668c8ece83408d))
- *(core)* Wrap page offsets within their region - ([fa7f637](https://github.com/lukexor/tetanes/commit/fa7f637a3f1620102fed38c744c46e799614dc0e))
- *(fs)* Report header versions as text - ([7e2795b](https://github.com/lukexor/tetanes/commit/7e2795b496a68cb355fcabc1dc7a4d12f47d86e1))
- *(mapper)* Stop FK23C write-enabling its unmapped WRAM windows - ([e070fde](https://github.com/lukexor/tetanes/commit/e070fded8c58c43ac01bbc7bc54b58388d8b2360))
- *(mapper)* Decode VRC2's mirroring register at all four indices - ([0f5c29f](https://github.com/lukexor/tetanes/commit/0f5c29f29149e0c0ec512c29971f7bdeaef1fc13))
- *(mapper)* Decode FME-7's R:8 select bit - ([878b82e](https://github.com/lukexor/tetanes/commit/878b82eef94ccbfeb2735a88b8d698397480e264))
- *(mapper)* Apply mapper 210's mirroring - ([138f134](https://github.com/lukexor/tetanes/commit/138f13477ab795d9a15b05e914a48ca0aa86bf88))
- *(mapper)* Correct MMC5's split prefetch scroll and $5204 readback - ([7b7b190](https://github.com/lukexor/tetanes/commit/7b7b19017ce352b03eac8155c72945b71f4cfd8e))
- *(mapper)* Let mapper 153 read its own PRG-RAM - ([d53b2db](https://github.com/lukexor/tetanes/commit/d53b2db570740b87e32da4f8514bb85ec5008928))
- *(mapper)* [**breaking**] Tag serialized boards by a stable id, not declaration order - ([8440bb3](https://github.com/lukexor/tetanes/commit/8440bb3335f352a5fc14384b97ff5838fba82702))
**BREAKING**: `Mapper::Fk23C` is no longer boxed, and a mapper number no board serves is now `mapper::Error::Unimplemented` rather than a silent `Mapper::none()` that read as open bus. Tools that survey ROMs rather than run them should use `Cart::from_rom_unmapped`/`Cart::from_path_unmapped`. The on-disk format is byte-for-byte unchanged by this commit.
- *(mapper)* Honour MMC5's sprite-size rule for CHR bank sets - ([4218c4d](https://github.com/lukexor/tetanes/commit/4218c4d5304a219336904e313306366251a36909))
- *(mapper)* Ignore FK23C's CHR-RAM select when the cart has no CHR-RAM - ([69e754f](https://github.com/lukexor/tetanes/commit/69e754f16e3b003af8e56014e1becafdaef8758b))
- *(mapper)* Apply Namco163 nametable select on every board variant - ([0bf93f4](https://github.com/lukexor/tetanes/commit/0bf93f4fa182afab2aecc5d1a01a567d0ebdb8cc))
- *(mapper)* Make VRC6 helpers const - ([79d3766](https://github.com/lukexor/tetanes/commit/79d376696b38010be8b25e18d6c047d4aa64cb27))
- *(mapper)* Restore nametable mirroring in sync, and unpin the game db - ([c47a60c](https://github.com/lukexor/tetanes/commit/c47a60cfbe10b4aad2ffb2218344ad7feb1a82f9))
- *(mapper)* Rebuild page tables when a save state is loaded - ([9de76c8](https://github.com/lukexor/tetanes/commit/9de76c81b9bd34d385e5a7cf4628fb50242c8987))
- *(ppu)* Read the dummy NT bytes on dots 337 and 339 only - ([e38ecb6](https://github.com/lukexor/tetanes/commit/e38ecb6f8ca484cc87ec8e02305ad7b26a6cda90))
- *(ppu)* Stop rotating the palette latches on the dummy NT fetches - ([bd2a8db](https://github.com/lukexor/tetanes/commit/bd2a8dba42e237646bb5f03ef4c3ffbc0e225e13))
- *(ppu)* [**breaking**] Forward the region to the mapper, and box SunsoftFme7 - ([f421867](https://github.com/lukexor/tetanes/commit/f4218677ee52d56004a888db6310cb2673b42fd4))
**BREAKING**: `Mapper::SunsoftFme7` is now boxed. The `boards!` stable ids become each board's primary mapper number, which changes the tag written into save states - riding the same undeployed SAVE_VERSION 2 bump as the rest of this release.
- *(sram)* Back up a save that cannot be read - ([5b2b671](https://github.com/lukexor/tetanes/commit/5b2b671a64fd627c215373e148a2ed712a1b74c1))
- *(sram)* Version battery saves independently of save states - ([7f23d4b](https://github.com/lukexor/tetanes/commit/7f23d4b094b1e312cabe50e6b50fc9ea1b2551f1))
- *(test)* Stop UPDATE_SNAPSHOT clobbering parallel snapshot writes - ([fcb1e0d](https://github.com/lukexor/tetanes/commit/fcb1e0dd54a35c49ed5c6bbeca30021ffb0a2d6d))

- Drop Game Genie codes when the cart is swapped - ([3ae10ff](https://github.com/lukexor/tetanes/commit/3ae10ff924781ca3d4dcb70704ae2fa3bcd49244))

### 🚜 Refactor

- *(core)* [**breaking**] Give a cart's battery one contiguous slice - ([6d3586b](https://github.com/lukexor/tetanes/commit/6d3586b350bdfe34880e4397ca250a26295b07a8))
**BREAKING**: `Map::save_sram`/`load_sram` are replaced by `sync_battery`/`restore_battery`, `MemoryLayout` gains `battery_ext`, and `ControlDeck::sram`/`save_sram` take `&mut self`.
- *(core)* [**breaking**] Take readers and writers, not paths - ([04bc7ae](https://github.com/lukexor/tetanes/commit/04bc7aec2f2ad5a72d35c795b8144890dbdedd37))
**BREAKING**: `fs` and `ControlDeck` save/load now take `Read`/`Write`, the path-taking forms are `*_path`, `fs::load_bytes` is gone since `&[u8]` is already a reader, and `Config::data_dir` is replaced by `Config::sram_dir: Option<PathBuf>`.
- *(core)* [**breaking**] Drop the accessors that only return a public field - ([4c8891d](https://github.com/lukexor/tetanes/commit/4c8891dbaa034a99aab83a4d2ec5a98abd9cfd85))
**BREAKING**: Cart::region, Cart::ram_state, Bus::region, Ppu::region, Apu::region, Dmc::region, Noise::region, Joypad::index, Zapper::x, Zapper::y and Mmc3::irq_pending are removed; read the field of the same name.
- *(core)* Put run-ahead's rewind through the restore funnel - ([4748ffe](https://github.com/lukexor/tetanes/commit/4748ffedb447bf36bb84fde86c8a206166f48894))
- *(core)* [**breaking**] Name the debugger accessors for the one debugger there is - ([1902979](https://github.com/lukexor/tetanes/commit/19029792c850326caa511cca5f0674ce23567ff5))
**BREAKING**: `ControlDeck::add_debugger`/`remove_debugger` are now `set_debugger`/`clear_debugger`, and the latter takes no argument.
- *(core)* [**breaking**] Make memory::Buffer crate-internal - ([9ef4227](https://github.com/lukexor/tetanes/commit/9ef42277d9571d5b65a155f554994ef503a0975d))
**BREAKING**: `memory::Buffer` and its unused constructors are no longer public.
- *(core)* Drop the unused BandLimited accessors - ([9875f48](https://github.com/lukexor/tetanes/commit/9875f481e579daaf95090171b32669abaad8efef))
- *(core)* Move the mapper and debugger bus methods out of ppu.rs - ([b13767e](https://github.com/lukexor/tetanes/commit/b13767e550c839b69a33cd3e070815cfb7ec0c07))
- *(core)* [**breaking**] Consolidate the Bus surface after flattening - ([7afe6cc](https://github.com/lukexor/tetanes/commit/7afe6cc106756d68dd619c703994d3ccb04c07ea))
**BREAKING**: `Input::read`/`Input::peek` are `read_port`/`peek_port` and no longer take a `&Ppu`; `Bus::input_read`/`input_peek` are what read $4016/$4017. `Bus::cpu_bus_read`/`cpu_bus_write`/`cpu_bus_peek` and `ppu_bus_read`/`ppu_bus_write` are now crate-internal; use `Bus::peek`, `Bus::ppu_bus_peek`, `Bus::chr_peek` or `Bus::copy_ppu_bus` to observe.
- *(core)* [**breaking**] Flatten ownership so Bus holds the console - ([c34e8b8](https://github.com/lukexor/tetanes/commit/c34e8b883817bb2d882ec4dd26600d0dcabc45de))
**BREAKING**: `ControlDeck::cpu()` returns registers only; the console is `ControlDeck::bus()`, and save states, rewind and replay are now `Bus` rather than `Cpu`. `ControlDeck::load_cpu` is `load_bus`. `Mapper` and `Memory` moved from `Ppu` to `Bus`, as did the debugger. `PpuDebugger` is `Debugger` and its callback takes `&Bus`. `Ppu::load_nametables`/`load_pattern_tables`/`load_oam` take the CHR window to read from. The `memory::Read` and `memory::Write` traits are removed in favor of inherent methods. Save states from earlier builds will not load.
- *(core)* [**breaking**] Make video::Frame a fixed-size buffer - ([b79a2fe](https://github.com/lukexor/tetanes/commit/b79a2fefc16376587eb8c2f044f3583bcd43789b))
**BREAKING**: `video::Frame` no longer derefs to `Vec<u8>`. Callers using it as a `Vec` should use `as_slice`, `as_array`, or indexing; callers that resized it have no replacement, as the size is now part of the type.
- *(core)* [**breaking**] Collapse the clocking API and clean up public surface - ([2b4fd1e](https://github.com/lukexor/tetanes/commit/2b4fd1eb4e4b102c9342f4d42a75d6eff077e1e7))
**BREAKING**: MSRV is now 1.88; 1.85 could not build the crate at all. `Map::sync` is `Map::update_banks`, `Ppu::sync_mapper` is `Ppu::rebuild_mapper_state`, and MMC5's colliding `update_chr_banks` is `select_chr_set`. Frame buffers are fixed-size: `Ppu::frame_buffer` and `ControlDeck::frame_buffer_raw` return `&[u16; ppu::size::FRAME]`, `ControlDeck::frame_buffer` returns `&[u8; Frame::SIZE]`, and `frame_buffer_into` takes `&mut [u8; Frame::SIZE]`. `ControlDeck::apu_mut` returns `&mut Apu`, `joypad` takes `&self`, and `wram` returns `&[u8; bus::size::WRAM]`. Clocking clears the previous call's audio samples instead of accumulating; opt out with `Config::clear_audio_on_clock = false`. The five older `clock_*` entry points are deprecated shims rather than removals.
- *(core)* [**breaking**] Delete the convention-only traits from common.rs - ([61cedf4](https://github.com/lukexor/tetanes/commit/61cedf42a12cfc94537d523fb21d63e40ecba8ee))
**BREAKING**: the `Clock`, `Reset`, `Regional`, `Sample` and `Sram` traits are gone, and the prelude drops them. `clock`, `reset`, `region`, `set_region`, `output`, `save` and `load` are inherent methods on each component and forward exactly as before, so a call like `deck.cpu_mut().clock()` is unchanged - only the trait import goes away.
- *(core)* [**breaking**] Delete the four single-purpose traits - ([9e9bee0](https://github.com/lukexor/tetanes/commit/9e9bee0bf45f8f2cdd21eca976d89def65fec105))
**BREAKING**: the `PpuAddr`, `InputRegisters`, `TimerCycle` and `Consume` traits are gone. `PpuAddr` becomes the free functions `ppu::is_attr` and `ppu::is_palette`; the rest become inherent methods on the types that had the impls.
- *(core)* [**breaking**] Delete the pre-page-table memory path - ([9ca8ae6](https://github.com/lukexor/tetanes/commit/9ca8ae64e1c69b267a14621bb0cecb6a1a1ad85a))
**BREAKING**: `mapper::Banks`, `mapper::BankAccess`, `ppu::CIRam` and the `mem` module are gone. `mem::Memory<D>` is now `memory::Buffer<D>`, and `memory::Memory` is the page-table address space. The `Cart::prg_rom` and `Cart::chr_rom` fields become the `Cart::memory` arena, with `prg_rom()` and `chr_rom()` accessors returning the unpadded bytes. `Ppu::ciram` is gone - nametables are page entries. `Map`'s read hooks lose their `_hook` suffix: `prg_read`, `prg_peek`, `chr_read`, `chr_peek`.
- *(mapper)* Drop unused VRC6 nametable setters and MMC3 chr_window - ([1354e7a](https://github.com/lukexor/tetanes/commit/1354e7a3439a7219a9d05fcb725e993da680289f))
- *(mapper)* Match the documented polarity of mapper 071's mirroring bit - ([421acec](https://github.com/lukexor/tetanes/commit/421acec6f78d01c1696ec56eed14e686dd821b01))
- *(mapper)* Generate the per-board plumbing from one boards! table - ([c872dd9](https://github.com/lukexor/tetanes/commit/c872dd96e58841e07f0be91ea7c07e15add9752e))
- *(mapper)* [**breaking**] Give Map every method a board needs, drop the supertraits - ([7a6439a](https://github.com/lukexor/tetanes/commit/7a6439a9968a3702b607e46ff3c2b4d14d8838d1))
**BREAKING**: `Map` no longer requires `Clock + Regional + Reset + Sram`. `clock`, `reset`, `region` and `set_region` are defaulted methods on `Map` itself, and `impl Map for Mapper` becomes inherent methods on `Mapper`, so `Map` no longer needs importing to call them.
- *(mapper)* [**breaking**] Unify per-board dispatch flags into MapperOps - ([8abfaee](https://github.com/lukexor/tetanes/commit/8abfaee32986e65a33873b5d775835b94bd27ac7))
**BREAKING**: `Map::watches_ppu_bus`, `Map::serves_prg_reads` and `Map::serves_chr_reads` are replaced by a single `Map::mapper_ops` returning `MapperOps` bitflags, which also declares `CLOCKED`, `IRQ`, `AUDIO` and `DMA`. A board that does not declare a hook no longer has it called.
- *(ppu)* Derive fine Y from v instead of caching it - ([c44bca0](https://github.com/lukexor/tetanes/commit/c44bca0954ab346511f811232ad8a3fb188f518f))
- *(ppu)* [**breaking**] Hold the PPU registers as fields, not structs - ([145a954](https://github.com/lukexor/tetanes/commit/145a95453b8111839df58840df0ce0529385ea09))
**BREAKING**: `ppu::ctrl::Ctrl`, `ppu::mask::Mask` and `ppu::status::Status` are gone; read the `ctrl_*`, `mask_*` and `status_*` fields on `Ppu` instead.


### 📚 Documentation

- *(bench)* Rewrite the benchmark README around running it - ([84357fc](https://github.com/lukexor/tetanes/commit/84357fc5acfa1e252c362cb72fa8a20f09cb2d6a))
- *(bench)* Record Raspberry Pi 5 on-target results - ([fae0367](https://github.com/lukexor/tetanes/commit/fae0367be3de371e15839c7eb12bbddfccbdcb2a))
- *(bench)* Decline the MesenCE sprite-shifter port with reasons - ([89fa791](https://github.com/lukexor/tetanes/commit/89fa7912a2fef619d12194327e120c5ff3d1a58e))
- *(bench)* Scope the two-layout rule to perf verdicts, record fine_y and dummy-fetch results - ([b2e26d8](https://github.com/lukexor/tetanes/commit/b2e26d8dd1a9aaf7e5382e664c2a948937603808))
- *(bench)* Record the dispatch-reshape layout draw - ([96ef17a](https://github.com/lukexor/tetanes/commit/96ef17ac67fb22c35ddc39a6e179f21b1b1a8c77))
- *(bench)* Record the shifter-reload layout result - ([54e6b4d](https://github.com/lukexor/tetanes/commit/54e6b4d02f5c31a3835606a32a7411430103dde4))
- *(bench)* Measure candidates at two code layouts - ([f8e4282](https://github.com/lukexor/tetanes/commit/f8e4282e1b67c357c204fb993024978aae5ec048))
- *(bench)* Correct profile shares read from the wrong denominator - ([4166301](https://github.com/lukexor/tetanes/commit/416630137aaa520e1dda59193d9dd2e505180009))
- *(bench)* Correct the boxing conclusion with FME7 ROMs measured - ([97e5da7](https://github.com/lukexor/tetanes/commit/97e5da7c8541af1ed519ce7718f211b55a5bdf02))
- *(bench)* Record the trait-diet and boards! table results - ([c07971d](https://github.com/lukexor/tetanes/commit/c07971d5b604d03d3b9eb0a5f332046717b9be23))
- *(bench)* Record the MapperOps results and correct the catch-up conclusion - ([13cae45](https://github.com/lukexor/tetanes/commit/13cae459c7ec7db8290b85b715245006af2f1e24))
- *(bench)* Record the PPU results and the catch-up investigation - ([9fe62c0](https://github.com/lukexor/tetanes/commit/9fe62c00be75133b816f164dedb40b46ddf2c5c2))
- *(bench)* Record the frame times after deleting the old memory path - ([17596d1](https://github.com/lukexor/tetanes/commit/17596d1defc5c10c040cceeac4e7b85e317210a8))
- *(bench)* Record frame times after the mapper rework - ([3968038](https://github.com/lukexor/tetanes/commit/396803876b50d0a4fd5aa1b05ef9222ed9ff042b))
- *(core)* Correct the mapper, PPU peek and Bus-state docs - ([dfb4207](https://github.com/lukexor/tetanes/commit/dfb420739d1ec2529023265b4dfc104426bcd35d))
- *(core)* Correct stale board, filter and audio-profile references - ([fff8802](https://github.com/lukexor/tetanes/commit/fff880262c7cb9cc320fd32881b15861bc05da23))
- *(core)* Describe the code as it stands, not its history - ([5a5b65b](https://github.com/lukexor/tetanes/commit/5a5b65bddb1a806d113bfa48683f4000a47d9176))
- *(core)* Turn on missing_docs and say which half is API - ([4511ddf](https://github.com/lukexor/tetanes/commit/4511ddfc0981fa6a2309ec4bf37c05f352a55b75))
- *(mapper)* Link the save-state rebuild to Bus, not Ppu - ([3e24c14](https://github.com/lukexor/tetanes/commit/3e24c14c8def4e428f44b7312af94bba73a409c9))
- *(mapper)* Correct what FK23C's $A001 handler relies on - ([5e2a93e](https://github.com/lukexor/tetanes/commit/5e2a93eb6fe417366205318730555ea5e95c2129))
- *(mapper)* Record that Color Dreams boards have no bus conflicts - ([fceecca](https://github.com/lukexor/tetanes/commit/fceecca4ff9104f2cf374a29fa15c52a5daf4491))
- *(mapper)* Drop the transitional phrasing from the Map trait - ([ac1725d](https://github.com/lukexor/tetanes/commit/ac1725ddebd4d49e367a5e7dced12cca4b09d223))
- *(memory)* Say why the region wrap is a divide - ([878c8d0](https://github.com/lukexor/tetanes/commit/878c8d0580677710e2fbcbc7372d20c8050b622c))

- Correct stale facts in CLAUDE.md, the mapper docs and the README table - ([b702137](https://github.com/lukexor/tetanes/commit/b70213730005ffcf5e91b3bc073a1cfe8869ec67))
- Fix stale references and drop history from the branch's comments - ([97a2850](https://github.com/lukexor/tetanes/commit/97a2850fc67cdd2fdec4a43e16c7519cb1728b7d))

### ⚡ Performance

- *(apu)* [**breaking**] Synthesise the output from channel deltas - ([cb9a45b](https://github.com/lukexor/tetanes/commit/cb9a45b3c091e84a3c613010dfb9dd256cb85a8a))
**BREAKING**: `Apu::sample_period`, `Apu::sample_counter` and `Apu::mapper_outputs` are gone, along with `apu::filter`'s `Fir`, `windowed_sinc_kernel` and the decimation half of `FilterChain`.
- *(apu)* Run the channels between waveform steps, not every cycle - ([9b9699c](https://github.com/lukexor/tetanes/commit/9b9699c46de7bfbe319af9a2d75797a90b97d48d))
- *(apu)* Flush denormals and mix only when the filter chain samples - ([9934043](https://github.com/lukexor/tetanes/commit/99340432488e0ed8ac6fca9b1abb726eb920ffc1))
- *(apu)* [**breaking**] Run the filter chain at two rates instead of six - ([fa2966e](https://github.com/lukexor/tetanes/commit/fa2966eaf00b605cd2ae58497615936222228853))
**BREAKING**: `apu::filter::Filter`, `apu::filter::SampledFilter` and `FilterKind::Identity` are removed, and `FilterChain`'s fields are now the named stages rather than a `[SampledFilter; 6]`, which changes the save-state layout - covered by the `SAVE_VERSION` bump already on this branch.
- *(apu)* Mix from integer channel levels in the MMC5 hot path - ([31186ed](https://github.com/lukexor/tetanes/commit/31186edb5b74ad3e97dcbd0fbfda36b73ef1ff1d))
- *(apu)* Avoid a libm call in the FIR filter hot loop - ([f91f59e](https://github.com/lukexor/tetanes/commit/f91f59ef780b9e0e258223ba66f604c075384e08))
- *(core)* Snapshot run-ahead into last frame's console - ([42e9378](https://github.com/lukexor/tetanes/commit/42e9378f06ffe76bc7069f61c502a21c15cab045))
- *(core)* Snapshot run-ahead with a clone instead of bincode - ([605f917](https://github.com/lukexor/tetanes/commit/605f91721346daf39356d28cbe90bb5f143e6449))
- *(core)* [**breaking**] Keep the cart's ROM out of save states - ([12156e5](https://github.com/lukexor/tetanes/commit/12156e56b69a12774b37e06ad45772cbf03ed1da))
**BREAKING**: save states and rewind snapshots no longer carry the cart's ROM, so a state can only be applied to the game it came from. `Cpu::load` and `ControlDeck::load_cpu` are fallible, returning `cpu::StateMismatch` when the state does not belong to the loaded cart.
- *(core)* Hoist per-pixel invariants in the NTSC filter - ([bf6dd6e](https://github.com/lukexor/tetanes/commit/bf6dd6e97a2d8140fafe0aa8d144f1b3e6eb956f))
- *(core)* Un-box Ppu::sprites; document why wram/frame buffer stay boxed - ([5fac025](https://github.com/lukexor/tetanes/commit/5fac025c4c53306c1e462bd909d4e79841a0113d))
- *(core)* Split Ppu::clock's branch chain and flag-gate the debugger - ([8656742](https://github.com/lukexor/tetanes/commit/8656742657df18de24d128bdd4f4bc4ff52734ae))
- *(mapper)* Re-bank only on writes that change banking - ([f1d6f9a](https://github.com/lukexor/tetanes/commit/f1d6f9a10cd0a76098c6b4f079d29a96600ea4e6))
- *(mapper)* Only re-map the CHR window MMC2/MMC4's latch changed - ([5b2371a](https://github.com/lukexor/tetanes/commit/5b2371a41fe9877d43bbc9ac39a130d173b8f4b0))
- *(ppu)* Reload BG shifters only on real tile fetches - ([82ad8af](https://github.com/lukexor/tetanes/commit/82ad8afea77fd7b84de6087864de03c09886bf0a))
- *(ppu)* Gate the pixel path on a dot threshold, not on mask flags - ([41d8ea5](https://github.com/lukexor/tetanes/commit/41d8ea577b3a551ecf34ea679d5f5bf3ff2a2ee1))
- *(ppu)* Fold greyscale and emphasis in by the run, not per pixel - ([71b7ff1](https://github.com/lukexor/tetanes/commit/71b7ff106abfcc43d17ae9b5571d57ad7d9d1e13))
- *(ppu)* Visit only the sprites covering a dot - ([341f832](https://github.com/lukexor/tetanes/commit/341f83208885b819067c07e10867b23e1ae68820))
- *(video)* [**breaking**] Bake the NTSC palette at build time - ([830b850](https://github.com/lukexor/tetanes/commit/830b850da01eee5c9fd683d030c72a6c833d0ae5))
**BREAKING**: `video::NTSC_PALETTE` is a `&'static [u8; N]` of red, green, blue triples baked in at build time, not a `OnceLock<Vec<u32>>` packing each color into one `u32`.
- *(wasm)* Store local-storage payloads as base64, not json arrays - ([abb4488](https://github.com/lukexor/tetanes/commit/abb44881dfc258ede42689ef50517c9cc7b99d0d))


### 🧪 Testing

- *(apu)* Assert audio with a coarse profile instead of a sample hash - ([e411661](https://github.com/lukexor/tetanes/commit/e4116615f5ee744c708e05b3e4e5148346bc4127))
- *(core)* Implement the bus routing tests and drop the two unassertable ROMs - ([dc60b4b](https://github.com/lukexor/tetanes/commit/dc60b4b178e1f7e1f64df16b28a2e0bb3f24065d))
- *(core)* Assert blargg result codes and report tones instead of ignoring - ([5c2e4e6](https://github.com/lukexor/tetanes/commit/5c2e4e6906979ffed444d4041dc3d4ecb1d2cab9))
- *(core)* Cover the save-state and SRAM round trips - ([1b8c0d8](https://github.com/lukexor/tetanes/commit/1b8c0d8fde247d81e0ab9d88f7fe67f11c01cd98))
- *(input)* Assert the zapper through the channel each ROM reports on - ([c483467](https://github.com/lukexor/tetanes/commit/c4834678f9c9ff2feb17a3941e94693f7bcd3d2f))
- *(mapper)* Make the board tests take the bus's route - ([e260d05](https://github.com/lukexor/tetanes/commit/e260d0584decaffbaf8cf5c87b11329c5bbfb963))
- *(mapper)* Cover the last 13 boards in the table - ([9a113c3](https://github.com/lukexor/tetanes/commit/9a113c377c771ddada1efeda8a83722eeab44a00))
- *(mapper)* Cover Jaleco SS88006, Bandai FCG and FK23C - ([a63ddf8](https://github.com/lukexor/tetanes/commit/a63ddf8a9f0a6f144fe0d518472ddbde52d30682))
- *(mapper)* Cover Namco163, Sunsoft FME7 and MMC1 banking and IRQs - ([32ea7dc](https://github.com/lukexor/tetanes/commit/32ea7dcd1f9473e50851f4ba729cac0a53185581))
- *(mapper)* Cover the VRC IRQ counter and VRC6, fixing two overflow panics - ([e42aebf](https://github.com/lukexor/tetanes/commit/e42aebfae551ec84095db8d733e8981d14fdfb63))
- *(ppu)* Assert the field placement the dot loop depends on - ([b063118](https://github.com/lukexor/tetanes/commit/b063118e621843bb1c4ff32d40c6b96c106d337f))


### ⚙️ Miscellaneous Tasks

- *(bench)* Allow sweeping a ROM library for load and clock failures - ([a12c552](https://github.com/lukexor/tetanes/commit/a12c552eb4c0578128c72d342989525b2f8491f4))
- *(bench)* Benchmark a ROM corpus and report variance - ([2d9452e](https://github.com/lukexor/tetanes/commit/2d9452e04c121ecb160fc3e3024f19f64003c409))

- Clear the must_use, const fn and rustdoc warnings - ([5ef3404](https://github.com/lukexor/tetanes/commit/5ef3404c090e5ce564f5a08dd888bc4732060389))


## [0.14.2](https://github.com/lukexor/tetanes/compare/0.14.1..0.14.2) - 2026-06-11

### ⛰️  Features


- Map San Guo Zhi 2 CRC to mapper 176 submapper 2 - ([87f14db](https://github.com/lukexor/tetanes/commit/87f14dbf5c4d4f29d3798af8c6a996f377b06616))
- Add mapper 176 (Waixing FK23C / FS303) - ([978c9fe](https://github.com/lukexor/tetanes/commit/978c9fe1251ef25920472f23f85af5271b1ba859))
- Add nes-event mapper - ([518087f](https://github.com/lukexor/tetanes/commit/518087f67cb0ce774ce6f691c5c82b7ab61c9cfe))

### 🐛 Bug Fixes

- *(cpu)* JAM/KIL rewinds PC and stays cycling instead of halting - ([55dce2a](https://github.com/lukexor/tetanes/commit/55dce2afec7d949549fec85ff9adad69d65d6a50))
- *(n163)* Persist PRG-RAM as battery save (was audio RAM) - ([1ab150d](https://github.com/lukexor/tetanes/commit/1ab150d25088689b3c7f110bef60fc69da357963))
- *(n163)* Keep the audio phase accumulator outside the sound RAM - ([85bd35f](https://github.com/lukexor/tetanes/commit/85bd35f0e99134c8f57c1a8f4f50e66a3c31586f))
- *(n163)* Drop the $F800 PRG-RAM write protect - ([3b91d1a](https://github.com/lukexor/tetanes/commit/3b91d1a0cedf64458c07d5be769230e8fef32107))
- *(n163)* Disable variant auto detection when ROM is in CRC database - ([3cef338](https://github.com/lukexor/tetanes/commit/3cef3388d8ab01f0bb96cd6dcf295843d1480f70))


### 🚜 Refactor


- Move mmc3 to a shared module - ([80ed579](https://github.com/lukexor/tetanes/commit/80ed57953896e736900625c2f0c1de5e0a3b9407))
- Rename Regs to Mmc3 in mapper::m004_txrom - ([816e6bf](https://github.com/lukexor/tetanes/commit/816e6bf3493f9c97e818fa99787c1cc3c09e18d6))
- Move mmc1 to new module - ([2bdfc23](https://github.com/lukexor/tetanes/commit/2bdfc23f0664a810a6b18a0ef74cff4bbe36a1bd))
- Move mmc1-specific functionality from Sxrom to Mmc1 - ([297f4a1](https://github.com/lukexor/tetanes/commit/297f4a17b69f684a022a8eda62e851d64cb11371))
- Simplify Sxrom initialization - ([5d8a3cf](https://github.com/lukexor/tetanes/commit/5d8a3cf39ea106db59b18cd70b64a1dc05cbfc6a))
- Rename Regs to Mmc1 in mapper::m001_sxrom - ([35ff708](https://github.com/lukexor/tetanes/commit/35ff708f333fc8f7fbe1853b433869bc93692c3b))

### ⚙️ Miscellaneous Tasks


- Updated deps - ([7c66287](https://github.com/lukexor/tetanes/commit/7c6628786cc3d6254b498564592dc7c6f63976c1))


## [0.14.1](https://github.com/lukexor/tetanes/compare/0.13.0..0.14.1) - 2026-04-20

### 🐛 Bug Fixes


- Fixed palette x offset - ([350a892](https://github.com/lukexor/tetanes/commit/350a8925dffcd49e737884971b90f11d2a737996))

### 🎨 Styling


- Fixed 1.85 lints - ([ddfb3eb](https://github.com/lukexor/tetanes/commit/ddfb3eb318397984df76fbd3cb36c8a383ab0368))
- Fixed nightly lints - ([801bb3c](https://github.com/lukexor/tetanes/commit/801bb3c95a95a7f98efb5a993f2efe7061daf00b))


## [0.13.0](https://github.com/lukexor/tetanes/compare/0.12.2..0.13.0) - 2026-02-14

### 🐛 Bug Fixes


- Change cycle to u32 to improve 32-bit platforms like wasm - ([86f597c](https://github.com/lukexor/tetanes/commit/86f597c3c1422040e8f98ac4dfb29f3a2ffdc81f))
- Fixed turbo - ([5f149c3](https://github.com/lukexor/tetanes/commit/5f149c3be9eaa74bd5583dd41daf4cbae2cbf0a9))
- Ignore header bytes 14/15. closed #430 - ([8ff2bd9](https://github.com/lukexor/tetanes/commit/8ff2bd96d055c5a2551e44deb69c8a5e1c5b191e))
- Fixed memory methods to allow working with &[u8] - ([6c2cf4a](https://github.com/lukexor/tetanes/commit/6c2cf4aa33e5fa7ba3c903c775b6ecfcf47f059d))

### ⚡ Performance


- Cpu opcode refactor - ([8916097](https://github.com/lukexor/tetanes/commit/8916097c39ee18828d9fd7bf70185a19eaf8e098))
- Convert Vec<u8> to Box<[u8]> for ~2.5% gain - ([6f34112](https://github.com/lukexor/tetanes/commit/6f34112aa193499ed69115fe8f70774bbe6e4a74))

### ⚙️ Miscellaneous Tasks


- Updated deps - ([ccb7463](https://github.com/lukexor/tetanes/commit/ccb7463c98be1e35fe7e6a47c2df9d76796ee349))
- Update deps - ([782dc21](https://github.com/lukexor/tetanes/commit/782dc213f25f34cc47af0c2f71cdf9bfd44ae28b))


## [0.12.2](https://github.com/lukexor/tetanes/compare/0.12.1..0.12.2) - 2025-04-05

### 🐛 Bug Fixes


- Revert input serialization change, as it broke run-ahead - ([6489c57](https://github.com/lukexor/tetanes/commit/6489c579c738f88068423affb3833edd9105e523))
- Fix touch events - ([1a8d3da](https://github.com/lukexor/tetanes/commit/1a8d3dad5243bb31858575e99e6961c33a2521d4))
- Basic cpu error recovery options - ([c60d8d1](https://github.com/lukexor/tetanes/commit/c60d8d1d02dde9e3383849e1af43111f26db29c7))

### 🚜 Refactor


- Changed input serializing - ([ad37b47](https://github.com/lukexor/tetanes/commit/ad37b47ab07a54597eeb91b95fb524a0ea6b4e43))
- Cleaned up Memory struct - ([7cb31fb](https://github.com/lukexor/tetanes/commit/7cb31fb3878ee4e7e9b4ca9eabe123d570cc3176))

### 📚 Documentation


- Fix urls - ([8136c42](https://github.com/lukexor/tetanes/commit/8136c42bdab474e17ceda78819225349a3f1c520))
- Add notes about stability - ([af0ce20](https://github.com/lukexor/tetanes/commit/af0ce20410f4088453788a264f8102823c7cf7da))
- Ensure features display in docs.rs - ([f58b31c](https://github.com/lukexor/tetanes/commit/f58b31cc9d27f0187d759796c64e34ccea3f784a))

### 🧪 Testing


- Fix tracing init in tests - ([e368ad5](https://github.com/lukexor/tetanes/commit/e368ad5ab41d3c484dcaf17a7a8ff8abae48aef9))


## [0.12.1](https://github.com/lukexor/tetanes/compare/0.12.0..0.12.1) - 2025-03-13

### ⛰️  Features


- Added shortcuts for shaders and ppu warmup flag - ([408b122](https://github.com/lukexor/tetanes/commit/408b122ed98f7edb7a26085fb921aa006bde7091))

### 🐛 Bug Fixes


- Fixed issues with some mmc1 games - ([496cf41](https://github.com/lukexor/tetanes/commit/496cf41ced63949fd6d8be5402989e927baf92b8))

### 📚 Documentation


- Fixed cargo doc url - ([782f7c5](https://github.com/lukexor/tetanes/commit/782f7c51b68c5fb52b483a3f151bd3db227286a9))
- Updated changelog and readmes - ([a4a3e8c](https://github.com/lukexor/tetanes/commit/a4a3e8c0775a7261b91f4238756ac5a20d2c4b48))

### ⚙️ Miscellaneous Tasks


- Fix/update ci, docs, and fixed nightly issue with tetanes-core - ([a6150ba](https://github.com/lukexor/tetanes/commit/a6150bad6703bbc661d7d5c8b63f5a6d47991868))


<!-- markdownlint-disable-file no-duplicate-heading no-multiple-blanks line-length -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.12.0](https://github.com/lukexor/tetanes/compare/tetanes-v0.11.0..tetanes-v0.12.0) - 2025-03-12

### ⛰️  Features


- Jalecoss88006 - ([406777a](https://github.com/lukexor/tetanes/commit/406777abad8d61490aae2a33e2e71fc617db3f55))
- Namco163 - ([89d7fb4](https://github.com/lukexor/tetanes/commit/89d7fb4617bf844ad2090cd92f0e92cda9cc91fc))
- Added sunsoft/fme-7 - ([303dad8](https://github.com/lukexor/tetanes/commit/303dad85d0a6586a88b7067f2070c5ac9e4da6e4))
- Added nina003/nina006 - ([29503c3](https://github.com/lukexor/tetanes/commit/29503c3efc81fb3110eef63e9666de1e0c912015))
- Added dxrom ([#340](https://github.com/lukexor/tetanes/issues/340)) - ([906af59](https://github.com/lukexor/tetanes/commit/906af59038e95874dda254e02998030c913d8c61))
- Ppu-viewer ([#339](https://github.com/lukexor/tetanes/issues/339)) - ([fce7d89](https://github.com/lukexor/tetanes/commit/fce7d89f78148e9a367d47122eef7e6e8fe45b34))
- Bandai mappers 016, 153, 157, 159 ([#335](https://github.com/lukexor/tetanes/issues/335)) - ([f555ea4](https://github.com/lukexor/tetanes/commit/f555ea48d0273bc9d41b998926d451398acbb73c))
- Allow exporting save states in web ([#311](https://github.com/lukexor/tetanes/issues/311)) - ([627bbec](https://github.com/lukexor/tetanes/commit/627bbece49739ff479e69ba9e83df828c4d4a633))
- Add a debug build label - ([46b3d94](https://github.com/lukexor/tetanes/commit/46b3d94e5fd24900a95554a295257f0891ac1c53))
- Add test panic debug button - ([3866efa](https://github.com/lukexor/tetanes/commit/3866efab39ced8f3431ee27d24962a55397a9f07))
- Added screen reader/accesskit support - ([5fd1a73](https://github.com/lukexor/tetanes/commit/5fd1a73f112f74a6c0a81e722485842dd37e0a38))
- Added ui setting/debug windows - ([db8b122](https://github.com/lukexor/tetanes/commit/db8b122af6c5a52ad23ed89ffd6f2feb35515603))
- Enable webgpu for browsers that support it. closes #297 ([#298](https://github.com/lukexor/tetanes/issues/298)) - ([a6bde61](https://github.com/lukexor/tetanes/commit/a6bde619454bf8d77f98d462d89eccea4b0e42fc))

### 🐛 Bug Fixes


- Fixed several issues - ([60fcd90](https://github.com/lukexor/tetanes/commit/60fcd90e740833e94deb98896a17a51fcda38998))
- Fix cycle overflow - ([a4e1f05](https://github.com/lukexor/tetanes/commit/a4e1f058c6e899e9fd11578bfb40a36d6ea1980e))
- Add temporary webgpu flag - ([179e868](https://github.com/lukexor/tetanes/commit/179e868c9e1cee92df1d0568b60403c2df7579cb))
- Temporary wasm fix for check-cfg - ([30c6a61](https://github.com/lukexor/tetanes/commit/30c6a61c0d562f875a0d979ad655e199d7c7019a))
- Fix tetanes-core compiling on stable. closes #360 - ([adc5673](https://github.com/lukexor/tetanes/commit/adc5673a3ed5d80aff339c3ab6d95013fcb2d715))
- Fixed deny.toml - ([2c1f186](https://github.com/lukexor/tetanes/commit/2c1f18603f043c2dcb17db8fa8958ea1cbfd88d4))
- Fixed bank size check - ([c84c012](https://github.com/lukexor/tetanes/commit/c84c012c310ad466c5167b94f0228f5a482dec43))
- Fixed wasm - ([bd27814](https://github.com/lukexor/tetanes/commit/bd278140bcc7e7d433917f14e99038f2e6453027))
- Fixed video frame size - ([153094d](https://github.com/lukexor/tetanes/commit/153094d81d444376b112224409544375588c4f97))
- Fix scroll issues - ([218d786](https://github.com/lukexor/tetanes/commit/218d7860421eb4cfc4d7b833132f4c476935777a))
- Fixed increasing scale on web - ([8c4265e](https://github.com/lukexor/tetanes/commit/8c4265e10fc8b62cd7dcaa8a828fed1a07100a9f))
- Fixed shortcut text - ([cb73c21](https://github.com/lukexor/tetanes/commit/cb73c216936ad49dca4e2595485df4ccea957eaa))
- Fixed joypad keybinds and some UI styling - ([bc2f093](https://github.com/lukexor/tetanes/commit/bc2f093b4d02c54744f791f336a102424a7e5af1))
- Enable puffin on wasm - ([0b6f794](https://github.com/lukexor/tetanes/commit/0b6f79429c5d2a642c0ef6301bbcc9818973a234))
- Fix window theme - ([e3c42c7](https://github.com/lukexor/tetanes/commit/e3c42c7720f558c7348e2b82b3573d4748158850))
- Fixed window aspect ratio - ([17db5c8](https://github.com/lukexor/tetanes/commit/17db5c8a037ab3aefab560bca67545964069658f))
- Don't log/error when sending frames while paused - ([50825f8](https://github.com/lukexor/tetanes/commit/50825f82e9f04418fdefd56707ef2ec50cddd5ed))
- Fixed pause state when loading replay - ([d743b31](https://github.com/lukexor/tetanes/commit/d743b31c190cd93e42e3ab78b497e59bcc4ade88))
- Fixed roms path to default to current directory, if valid, and canonicalize - ([e00273f](https://github.com/lukexor/tetanes/commit/e00273f740f7fc095bc02c7ce6d0ba132a14c9bc))
- Ensure pixel brightness is using the same palette - ([ad2f873](https://github.com/lukexor/tetanes/commit/ad2f873f5652016b96317c000b4abbe0e35de421))
- Move some calculations to vertex shader that don't depend on v_uv - ([a6f262d](https://github.com/lukexor/tetanes/commit/a6f262db5d83950e86e0ec78bb74fc63e5c2bf85))
- Fixed logging location - ([ff36033](https://github.com/lukexor/tetanes/commit/ff36033d7bbbf64924d97d6e9a88dcf4db7dc60c))
- Fixed issue with lower end platforms not supporting larger texture dimensions - ([ef214db](https://github.com/lukexor/tetanes/commit/ef214dbc2f2eee016b7abdb0c2b0ee1858381ee4))
- Fix window resizing while handling zoom changes - ([6b3f690](https://github.com/lukexor/tetanes/commit/6b3f690b8ec21b907d353a7cad8561217e8d9dcf))

### 🚜 Refactor


- [**breaking**] Split mapper traits - ([3e4a372](https://github.com/lukexor/tetanes/commit/3e4a372dfdc4295851c93cca96044f84645ae14e))
- Removed egui-wgpu and egui-winit dependencies. ([#315](https://github.com/lukexor/tetanes/issues/315)) - ([b3d4e2c](https://github.com/lukexor/tetanes/commit/b3d4e2c70c6ee4cfa9aaf53a11c1ae802610ff99))
- Platform/ui cleanup - ([39f66e6](https://github.com/lukexor/tetanes/commit/39f66e6e912f9c95cf9c458cd072e5e041af09e3))
- Moved around platform code to condense it - ([0f18928](https://github.com/lukexor/tetanes/commit/0f18928b8f8ed031cac7a170557c0296916c99bc))
- Prefer deferred viewports ([#306](https://github.com/lukexor/tetanes/issues/306)) - ([e1e60d1](https://github.com/lukexor/tetanes/commit/e1e60d19599ab883cbb034047519e6eb831d6c6c))

### 📚 Documentation


- Extra cpu comments - ([80f3366](https://github.com/lukexor/tetanes/commit/80f3366e3fab1257201ab0d9af673c4318edabef))

### ⚡ Performance


- Restore sprite presence check, ~2% gain - ([c6d353a](https://github.com/lukexor/tetanes/commit/c6d353a8fc12b506656a8cd70561ef1830ba9284))
- More perf and added flamegraph - ([31edf0c](https://github.com/lukexor/tetanes/commit/31edf0c63bcc30867f0049a231e7d366db4bde8d))
- Performance tweaks - ([d9a3019](https://github.com/lukexor/tetanes/commit/d9a3019ec0c0014d8850158d38c27289dc885020))

### 🎨 Styling


- Fix lints - ([bc9f6bc](https://github.com/lukexor/tetanes/commit/bc9f6bc293d413cf780a2aa0253ad7d64951d193))
- Slight cleanup - ([63e31a9](https://github.com/lukexor/tetanes/commit/63e31a9755266bec88d5c79e064506999f03aea2))
- Fixed format - ([d62ea28](https://github.com/lukexor/tetanes/commit/d62ea285cb5fe73ac41e7364f0ca3f32281a0e88))

### ⚙️ Miscellaneous Tasks


- Update deps - ([5b077c0](https://github.com/lukexor/tetanes/commit/5b077c01b1e68a60d3e295fe108732a3b8abbbd6))
- Bumped version - ([28fa93f](https://github.com/lukexor/tetanes/commit/28fa93f226447fd409b5d3846cd0f7e14a793f83))
- Update deps - ([509dbd4](https://github.com/lukexor/tetanes/commit/509dbd48a34cd6a360da0fba3786ed73445381fc))
- Fix ci - ([da64229](https://github.com/lukexor/tetanes/commit/da64229966295d85b0f62b0e3827d76767116602))
- Fix deny.toml - ([64a2401](https://github.com/lukexor/tetanes/commit/64a24010c72926c555ae74ffb4f1acb2c0aefffb))
- Updated deps - ([906c877](https://github.com/lukexor/tetanes/commit/906c877700d551fd74e0545e03f544ea2255823f))
- Updated deps - ([825719e](https://github.com/lukexor/tetanes/commit/825719e7f56ef6263f22a6da82d31f02d05af570))
- Updated deps - ([4712d6d](https://github.com/lukexor/tetanes/commit/4712d6d6de3ce7eccec8f1971fcb0f2411f91e3d))
- Restore nightly ci - ([eb2a2c5](https://github.com/lukexor/tetanes/commit/eb2a2c58ecd802810709f5e367253857d51a47d0))
- Update dependencies - ([4947a8c](https://github.com/lukexor/tetanes/commit/4947a8cf6883eda0b0c55fcd7bcf98cf8fd7dee9))
- Remove puffin_egui reference in wasm - ([16845f3](https://github.com/lukexor/tetanes/commit/16845f39e28c816c847a9d403dbedde38c815c1d))
- More dependency cleanup - ([1971e4f](https://github.com/lukexor/tetanes/commit/1971e4f2c5aaf6f8a2d6ce2a03c978362d44afe1))
- Clean up dependencies - ([254fe54](https://github.com/lukexor/tetanes/commit/254fe543293b0c96c78ce25bdaeef2f250a9fb14))
- Remove auto-assign from triage - ([9a2804b](https://github.com/lukexor/tetanes/commit/9a2804b94b1a412214159495d2e6410a63555572))
- Restrict homebrew cd to .rb files - ([3c1e390](https://github.com/lukexor/tetanes/commit/3c1e3907d7477dbe9f6953d9b8b9b0aeb1ef5966))
- Fix update homebrew formula runs-on - ([9e66a07](https://github.com/lukexor/tetanes/commit/9e66a073fa1ef9e276a2ca85ccc4e4281b50e7bc))
- Fix cd upload - ([892d184](https://github.com/lukexor/tetanes/commit/892d184cc25ca7903cb4a5f7372f47e722866125))
- Restore RELEASE_PLZ_TOKEN - ([18de294](https://github.com/lukexor/tetanes/commit/18de2946b82a44efdba96e5918eba381ad3a1a75))
- Remove need for RELEASE_PLZ_TOKEN - ([b6c8478](https://github.com/lukexor/tetanes/commit/b6c84780123ca5d9dfc841e2a3e6266b7d3cc4b9))
- Try to fix release cd - ([c7d5f51](https://github.com/lukexor/tetanes/commit/c7d5f514a84bd3b728686893e3211b63ec21a9c9))


## [0.11.0](https://github.com/lukexor/tetanes/compare/tetanes-core-v0.10.0..tetanes-core-v0.11.0) - 2024-06-12

### ⛰️  Features


- Added config and save/sram state persistence to web ([#274](https://github.com/lukexor/tetanes/pull/274)) - ([8c7f6df](https://github.com/lukexor/tetanes/commit/8c7f6df4a8894b544da1c6480659ee26ea28f342))
- Added mapper 11 - ([03d2074](https://github.com/lukexor/tetanes/commit/03d2074d3d58fcf652fecb9d77f4e96e8c007aae))
- Updated game database mapper names - ([86d246b](https://github.com/lukexor/tetanes/commit/86d246be9a52b64ed4191c970c6a727a31c21cb5))

### 🐛 Bug Fixes


- Ntsc tweaks - ([3042fa7](https://github.com/lukexor/tetanes/commit/3042fa7b928faf69e10040b4eb981a4c4f8f3ce3))
- Fixed fast forwarding - ([a6f87bb](https://github.com/lukexor/tetanes/commit/a6f87bb58ac3728471f673ade821e18579686b1a))
- Cleaned up pausing, parking, and control flow. Closes [#251](https://github.com/lukexor/tetanes/pull/251) - ([72cf88a](https://github.com/lukexor/tetanes/commit/72cf88ac6991953222bd3dd1d395f7f9035c98ef))
- Disable rewind when low on memory. clear rewind memory when disabled - ([4d5e1c4](https://github.com/lukexor/tetanes/commit/4d5e1c4dbe43cceb9ab8d4c33ca832830b2d31d8))

### 🚜 Refactor


- Removed a number of panic cases and cleaned up platform checks - ([bdb71a9](https://github.com/lukexor/tetanes/commit/bdb71a96792778cb0ad6bedf44e0ef5cbfa703e4))
- Add Sram trait and some mapper cleanup - ([ad03755](https://github.com/lukexor/tetanes/commit/ad0375506644f990e726c536f29bdf62d34d9e84))

### 📚 Documentation


- Fixed docs and changelog - ([4c7a694](https://github.com/lukexor/tetanes/commit/4c7a6949e52b6734fd6a78f6d9567c70e12b3ae4))
- Fixed docs - ([7a491c1](https://github.com/lukexor/tetanes/commit/7a491c14a2cb93db489c8bcb05d65f63bd1ed9d7))

### 🧪 Testing


- Update tests after ntsc change - ([f47f6c0](https://github.com/lukexor/tetanes/commit/f47f6c08ec2678c90e66b58d1297d20a6a72090b))
- Avoid serde_json::from_reader in tests as it's faster to just … ([#244](https://github.com/lukexor/tetanes/pull/244)) - ([3ca03ac](https://github.com/lukexor/tetanes/commit/3ca03ac68fab4d809dee39466fd661f887d2575d))


## [0.10.0](https://github.com/lukexor/tetanes/compare/tetanes-v0.9.0..tetanes-core-v0.10.0) - 2024-05-16

Initial release.

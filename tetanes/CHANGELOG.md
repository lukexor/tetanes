<!-- markdownlint-disable-file no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [tetanes-v0.9.0](https://github.com/lukexor/pix-engine/compare/v0.8.0..tetanes-v0.9.0) - 2023-10-31

### ⛰️ Features

- Added famicom 4-player support (fixes #78 - ([141e4ed](https://github.com/lukexor/pix-engine/commit/141e4ed7b33e93d1cf183be327070d6532a16324))
- Added clock_inspect to Cpu - ([34944e6](https://github.com/lukexor/pix-engine/commit/34944e63a4b0c72626c3313d2b849f0fa64a1c62))
- Added `Mapper::empty()` - ([30678c1](https://github.com/lukexor/pix-engine/commit/30678c127231614316b3df97d7c95501ae77287c))

### 🐛 Bug Fixes

- _(events)_ Fixed toggling menus - ([f30ade8](https://github.com/lukexor/pix-engine/commit/f30ade860c3dd5ff995cadd4e409159d7fce9d91))

- Fixed wasm - ([7abd62a](https://github.com/lukexor/pix-engine/commit/7abd62ad5a3178b5f77ae6c5c026d336d3237908))
- Fixed warnings - ([f88d760](https://github.com/lukexor/pix-engine/commit/f88d760fa4d8c6fd9f532e70ba76f95b54273e5c))
- Fixed a number of bugs - ([5fd85af](https://github.com/lukexor/pix-engine/commit/5fd85afd53efb8321f767b98372218eefc6e06a5))
- Fixed default tile attr - ([9002fa8](https://github.com/lukexor/pix-engine/commit/9002fa87806fcd56009369bbcd2644400ae7c5a1))
- Fixed exram mode - ([8507984](https://github.com/lukexor/pix-engine/commit/85079842d9ffe89b3e8ae61143642e09b3c471e6))
- Fix crosshair changes - ([10d843e](https://github.com/lukexor/pix-engine/commit/10d843e78cb04a69ad8d40e334a21268f294861b))
- Fix audio on loading another rom - ([d7cc16c](https://github.com/lukexor/pix-engine/commit/d7cc16cf7475f2b8bd632c2fc74a7b3c09447127))
- Improved wasm render performance - ([561be90](https://github.com/lukexor/pix-engine/commit/561be907f652a7f4879e943b44274547c0e43172))
- Web audio tweaks - ([5184e11](https://github.com/lukexor/pix-engine/commit/5184e11ba6fd104b59668e57744fb03b993483b5))
- Fixed game genie codes - ([0206d6f](https://github.com/lukexor/pix-engine/commit/0206d6fd20e5aa66da716eb07c6e077bfe4c5eed))
- Fixed update rate clamping - ([2133b84](https://github.com/lukexor/pix-engine/commit/2133b84ba865a0e715fa98129d9d6997540bd3e5))
- Fix resetting sprite presence - ([d219ce0](https://github.com/lukexor/pix-engine/commit/d219ce02320573498c84651af1585df08ed0c44e))
- Fixed missing Reset changes - ([808fcac](https://github.com/lukexor/pix-engine/commit/808fcac032d3b10731e330ecdcb9c468117a5425))
- Fixed missed clock changes - ([1b5313b](https://github.com/lukexor/pix-engine/commit/1b5313bf61115457beb50d919bb1129e54adc7cc))
- Fixed toggling debugger - ([e7bcfc1](https://github.com/lukexor/pix-engine/commit/e7bcfc1238fd21f957eed582b92a35e236e9884c))
- Fixed resetting output_buffer - ([0802b2b](https://github.com/lukexor/pix-engine/commit/0802b2b35c14f34fc8d3e73f3bd1ce940b4a8f48))
- Fixed confirm quit - ([48d6538](https://github.com/lukexor/pix-engine/commit/48d6538d25833a0817b0a27344a58a2a4918ab68))

### 🚜 Refactor

- Various updates - ([da213ae](https://github.com/lukexor/pix-engine/commit/da213ae36aa0b4763c643d089e390d184d69dc19))
- Small renames - ([0dea0b6](https://github.com/lukexor/pix-engine/commit/0dea0b6d15204f1fdfbe91ef8f9365993ffafaf2))
- Various cleanup - ([8d25103](https://github.com/lukexor/pix-engine/commit/8d251030a9782bfb9f18fb29ce67b3853b8cf9bd))
- Cleaned up some interfaces - ([da3ba1b](https://github.com/lukexor/pix-engine/commit/da3ba1b1b93f7ecacec3a93746026c0da174cbd4))
- Genie code cleanup - ([e483eb5](https://github.com/lukexor/pix-engine/commit/e483eb5e9a793b8883710399a7bb39c3cc6ed3ee))
- Added region getters - ([74d4a76](https://github.com/lukexor/pix-engine/commit/74d4a769fd089e3ff18295d88c5eaa60dcc208be))
- Cleaned up setting region - ([45dc2a4](https://github.com/lukexor/pix-engine/commit/45dc2a42928a7d9c507351965969b7976ca7b25c))
- Flatten NTSC palette - ([792d7db](https://github.com/lukexor/pix-engine/commit/792d7dbc45ec230df4de63b552d24bb4bbabc5c6))
- Converted system palette to array of tuples - ([284f54b](https://github.com/lukexor/pix-engine/commit/284f54b877ccbf6103920cba483ee7d0175f4c5d))
- Condensed MapRead and MapWrite to MemMap trait - ([bce1c77](https://github.com/lukexor/pix-engine/commit/bce1c7794ab0dc6ab493618f697fcc088864afb0))
- Made control methods consistent - ([f93040d](https://github.com/lukexor/pix-engine/commit/f93040d25128f50c20226ba3d52d638dbdd85ac3))
- Switch u16 addresses to use from_le_bytes - ([d8936af](https://github.com/lukexor/pix-engine/commit/d8936afaf8e3da54616d430cf3488f64a1aae5ef))
- Moved genie to it's own module - ([77b571f](https://github.com/lukexor/pix-engine/commit/77b571f990c4f86d30e160382ff773486a8a54a9))
- Cleaned up Power and Clock traits - ([533c0c3](https://github.com/lukexor/pix-engine/commit/533c0c3485cc73f880c4d43b2f937c0e606d0360))
- Cleaned up bg tile fetching - ([0710f16](https://github.com/lukexor/pix-engine/commit/0710f162928964209898ef7fdf6aacd3a3e4a1a0))
- Move NTSC palette declaration - ([9edffd1](https://github.com/lukexor/pix-engine/commit/9edffd1be33b3f79e1fb1a187bc89d3aede58804))
- Cleaned up memory traits - ([c98f7ff](https://github.com/lukexor/pix-engine/commit/c98f7fffc59f3ac399864c7ff130cbaad99762f6))
- Swapped lazy_static for once_cell - ([cc9e67f](https://github.com/lukexor/pix-engine/commit/cc9e67f643cf60ad982c88979b92f0ca843d505a))

### ⚡ Performance

- Cleaned up inlines - ([b791cc3](https://github.com/lukexor/pix-engine/commit/b791cc3ef7ece4fe0b627ff7332020453aa086ce))
- Added inline to cart clock - ([eb9a0e0](https://github.com/lukexor/pix-engine/commit/eb9a0e0d04fa6ec2d7f36935841dbda36a365bf8))
- Changed decoding loops - ([a181ed4](https://github.com/lukexor/pix-engine/commit/a181ed46e23534294463856ca3fec3c55cf2938f))
- Performance tweaks - ([0c06758](https://github.com/lukexor/pix-engine/commit/0c0675811709a2b590c273cd9010766e055b61ad))

### 🎨 Styling

- Fixed nightly lints - ([3bab00d](https://github.com/lukexor/pix-engine/commit/3bab00dd1478be71f11baaed97c3aa8f3cc6d241))

### 🧪 Testing

- Disable broken test for now - ([93857cb](https://github.com/lukexor/pix-engine/commit/93857cbd81d5b3f82ed157fdb87c0fa22aff1bc7))
- Remove vimspector for global config - ([56edc15](https://github.com/lukexor/pix-engine/commit/56edc15af9285d27d2e180ae585ecf91cc6f1e2a))

### ⚙️ Miscellaneous Tasks

- Increase MSRV - ([684e771](https://github.com/lukexor/pix-engine/commit/684e771a488f1cb88541b62265556b0fc79664a8))

## [0.8.0] - 2022-06-20

### Added

- Added `Mapper 024` and `Mapper 026` support.
- Added `Mapper 071` support.
- Added `Mapper 066` support.
- Added `Mapper 155` support. [#36](https://github.com/lukexor/tetanes/pull/36)
- Added configurable keybindings via `config.json`.
- Added `Config` menu.
- Added `Keybind` menu (still a WIP).
- Added `Load ROM` menu.
- Added `About` menu.
- Added `Zapper` light gun support with a mouse.
- Added lots of automated test roms.
- Added `4-Player` support. [#32](https://github.com/lukexor/tetanes/issues/32)
- Added audio `Dynamic Rate Control` feature.
- Added `Cycle Accurate` feature.

### Changed

- Various `README` improvements.
- Default `VSync` to `true`.
- Default `MMC1` to PRG RAM enable.
- Changed audio filtering and playback.
- Redesigned `TetaNES Web` UI and improved performance.

### Fixed

- Fixed Power Cycle/Reset affecting `ppuaddr`.
- Fixed reset causing
  segfault. [#50](https://github.com/lukexor/tetanes/issues/50)
- Fixed reset and load updating the correct ROM
  banks. [#51](https://github.com/lukexor/tetanes/issues/51)
- Fixed `OAM` emulation. [#31](https://github.com/lukexor/tetanes/issues/31)
- Fixed `DMA` emulation. [#30](https://github.com/lukexor/tetanes/issues/30)
- Fixed 512k `SxROM` games.
- Fixed `IRQ` and `NMI` emulation.

### Removed

- Removed `vcpkg` feature support due to flaky failures.

### Breaking

- Major refactor of all features, affecting save and replay files.
- Removed several command-line flags in favor of `config.json` and `Config`
  menu.

[unreleased]: https://github.com/lukexor/tetanes/compare/v0.8.0...HEAD
[0.8.0]: https://github.com/lukexor/tetanes/compare/v0.7.0...v0.8.0

<!-- markdownlint-disable-file no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

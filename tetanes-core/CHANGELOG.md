<!-- markdownlint-disable-file no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.11.1](https://github.com/lukexor/tetanes/compare/0.11.0..0.11.1) - 2025-03-07

### ⛰️  Features


- Jalecoss88006 - ([406777a](https://github.com/lukexor/tetanes/commit/406777abad8d61490aae2a33e2e71fc617db3f55))
- Namco163 - ([89d7fb4](https://github.com/lukexor/tetanes/commit/89d7fb4617bf844ad2090cd92f0e92cda9cc91fc))
- Added sunsoft/fme-7 - ([303dad8](https://github.com/lukexor/tetanes/commit/303dad85d0a6586a88b7067f2070c5ac9e4da6e4))
- Added nina003/nina006 - ([29503c3](https://github.com/lukexor/tetanes/commit/29503c3efc81fb3110eef63e9666de1e0c912015))
- Added dxrom ([#340](https://github.com/lukexor/tetanes/issues/340)) - ([906af59](https://github.com/lukexor/tetanes/commit/906af59038e95874dda254e02998030c913d8c61))
- Ppu-viewer ([#339](https://github.com/lukexor/tetanes/issues/339)) - ([fce7d89](https://github.com/lukexor/tetanes/commit/fce7d89f78148e9a367d47122eef7e6e8fe45b34))
- Bandai mappers 016, 153, 157, 159 ([#335](https://github.com/lukexor/tetanes/issues/335)) - ([f555ea4](https://github.com/lukexor/tetanes/commit/f555ea48d0273bc9d41b998926d451398acbb73c))
- Allow exporting save states in web ([#311](https://github.com/lukexor/tetanes/issues/311)) - ([627bbec](https://github.com/lukexor/tetanes/commit/627bbece49739ff479e69ba9e83df828c4d4a633))

### 🐛 Bug Fixes


- Fix cycle overflow - ([aa88b5e](https://github.com/lukexor/tetanes/commit/aa88b5e1057fb17e9700f9b7866278fe37ea4256))
- Fix tetanes-core compiling on stable. closes #360 - ([adc5673](https://github.com/lukexor/tetanes/commit/adc5673a3ed5d80aff339c3ab6d95013fcb2d715))
- Fixed bank size check - ([c84c012](https://github.com/lukexor/tetanes/commit/c84c012c310ad466c5167b94f0228f5a482dec43))
- Fixed video frame size - ([153094d](https://github.com/lukexor/tetanes/commit/153094d81d444376b112224409544375588c4f97))
- Ensure pixel brightness is using the same palette - ([ad2f873](https://github.com/lukexor/tetanes/commit/ad2f873f5652016b96317c000b4abbe0e35de421))

### 📚 Documentation


- Extra cpu comments - ([c2134c8](https://github.com/lukexor/tetanes/commit/c2134c825d456d7c4cb0e6e1d8b7bf6b9b6c783e))

### ⚡ Performance


- More perf and added flamegraph - ([968158b](https://github.com/lukexor/tetanes/commit/968158b115ea9b64f22bb60fbe64848edfba054a))
- Performance tweaks - ([1b1f5b5](https://github.com/lukexor/tetanes/commit/1b1f5b5bac3c41ab5a6fb158662d07cee17478f4))

### 🎨 Styling


- Slight cleanup - ([63e31a9](https://github.com/lukexor/tetanes/commit/63e31a9755266bec88d5c79e064506999f03aea2))

### ⚙️ Miscellaneous Tasks


- More dependency cleanup - ([1971e4f](https://github.com/lukexor/tetanes/commit/1971e4f2c5aaf6f8a2d6ce2a03c978362d44afe1))


## [0.11.0](https://github.com/lukexor/tetanes/compare/0.10.0..0.11.0) - 2024-06-12

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

<!-- markdownlint-disable-file no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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

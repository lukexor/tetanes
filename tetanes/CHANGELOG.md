<!-- markdownlint-disable-file no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.11.1](https://github.com/lukexor/tetanes/compare/0.11.0..0.11.1) - 2024-11-11

### ⛰️  Features


- Namco163 - ([89d7fb4](https://github.com/lukexor/tetanes/commit/89d7fb4617bf844ad2090cd92f0e92cda9cc91fc))
- Ppu-viewer ([#339](https://github.com/lukexor/tetanes/pull/339)) - ([fce7d89](https://github.com/lukexor/tetanes/commit/fce7d89f78148e9a367d47122eef7e6e8fe45b34))
- Allow exporting save states in web ([#311](https://github.com/lukexor/tetanes/pull/311)) - ([627bbec](https://github.com/lukexor/tetanes/commit/627bbece49739ff479e69ba9e83df828c4d4a633))
- Add a debug build label - ([46b3d94](https://github.com/lukexor/tetanes/commit/46b3d94e5fd24900a95554a295257f0891ac1c53))
- Add test panic debug button - ([3866efa](https://github.com/lukexor/tetanes/commit/3866efab39ced8f3431ee27d24962a55397a9f07))
- Added screen reader/accesskit support - ([5fd1a73](https://github.com/lukexor/tetanes/commit/5fd1a73f112f74a6c0a81e722485842dd37e0a38))
- Added ui setting/debug windows - ([db8b122](https://github.com/lukexor/tetanes/commit/db8b122af6c5a52ad23ed89ffd6f2feb35515603))

### 🐛 Bug Fixes


- Fixed wasm - ([84e02c9](https://github.com/lukexor/tetanes/commit/84e02c97fcf27f8f1d2e09004ea91efe874cb161))
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
- Move some calculations to vertex shader that don't depend on v_uv - ([a6f262d](https://github.com/lukexor/tetanes/commit/a6f262db5d83950e86e0ec78bb74fc63e5c2bf85))
- Fixed logging location - ([ff36033](https://github.com/lukexor/tetanes/commit/ff36033d7bbbf64924d97d6e9a88dcf4db7dc60c))
- Fixed issue with lower end platforms not supporting larger texture dimensions - ([ef214db](https://github.com/lukexor/tetanes/commit/ef214dbc2f2eee016b7abdb0c2b0ee1858381ee4))
- Fix window resizing while handling zoom changes - ([6b3f690](https://github.com/lukexor/tetanes/commit/6b3f690b8ec21b907d353a7cad8561217e8d9dcf))

### 🚜 Refactor


- Removed egui-wgpu and egui-winit dependencies. ([#315](https://github.com/lukexor/tetanes/pull/315)) - ([b3d4e2c](https://github.com/lukexor/tetanes/commit/b3d4e2c70c6ee4cfa9aaf53a11c1ae802610ff99))
- Platform/ui cleanup - ([39f66e6](https://github.com/lukexor/tetanes/commit/39f66e6e912f9c95cf9c458cd072e5e041af09e3))
- Moved around platform code to condense it - ([0f18928](https://github.com/lukexor/tetanes/commit/0f18928b8f8ed031cac7a170557c0296916c99bc))
- Prefer deferred viewports ([#306](https://github.com/lukexor/tetanes/pull/306)) - ([e1e60d1](https://github.com/lukexor/tetanes/commit/e1e60d19599ab883cbb034047519e6eb831d6c6c))

### 🎨 Styling


- Fixed format - ([d62ea28](https://github.com/lukexor/tetanes/commit/d62ea285cb5fe73ac41e7364f0ca3f32281a0e88))

### ⚙️ Miscellaneous Tasks


- Updated deps - ([4712d6d](https://github.com/lukexor/tetanes/commit/4712d6d6de3ce7eccec8f1971fcb0f2411f91e3d))
- More dependency cleanup - ([1971e4f](https://github.com/lukexor/tetanes/commit/1971e4f2c5aaf6f8a2d6ce2a03c978362d44afe1))
- Clean up dependencies - ([254fe54](https://github.com/lukexor/tetanes/commit/254fe543293b0c96c78ce25bdaeef2f250a9fb14))


## [0.11.0](https://github.com/lukexor/tetanes/compare/0.10.0..0.11.0) - 2024-06-12

### ⛰️  Features


- Shader support with crt-easymode ([#285](https://github.com/lukexor/tetanes/pull/285)) - ([e5042ef](https://github.com/lukexor/tetanes/commit/e5042efd45642ac2a13d7ac695bba1cce77c69c9))
- Auto-save cfg at a set interval ([#279](https://github.com/lukexor/tetanes/pull/279)) - ([e6941d8](https://github.com/lukexor/tetanes/commit/e6941d8e47c73910cf99c5ae9d52e9d5f4b7bade))
- Add UI persistence. closes [#257](https://github.com/lukexor/tetanes/pull/257) ([#277](https://github.com/lukexor/tetanes/pull/277)) - ([4c861f7](https://github.com/lukexor/tetanes/commit/4c861f7f59d99ee536e135a238ae3b621178bd08))
- Added config and save/sram state persistence to web ([#274](https://github.com/lukexor/tetanes/pull/274)) - ([8c7f6df](https://github.com/lukexor/tetanes/commit/8c7f6df4a8894b544da1c6480659ee26ea28f342))
- Added always on top option. enabled shortcut for embed viewports - ([489f61e](https://github.com/lukexor/tetanes/commit/489f61ef668094fa8e685a592cad0ec8b46d1c38))
- Added data man demo and changed name of nebs n' debs to demo - ([d7d2bae](https://github.com/lukexor/tetanes/commit/d7d2bae10da45239cf3e8142a00e893dd95fdc23))
- Added mapper 11 - ([03d2074](https://github.com/lukexor/tetanes/commit/03d2074d3d58fcf652fecb9d77f4e96e8c007aae))

### 🐛 Bug Fixes


- Fixed a number of issues caused by the crt-shader PR - ([8c31927](https://github.com/lukexor/tetanes/commit/8c31927ee332d0593402b7c6b632dbcafe4fa964))
- Ntsc tweaks - ([3042fa7](https://github.com/lukexor/tetanes/commit/3042fa7b928faf69e10040b4eb981a4c4f8f3ce3))
- Fixed some frame clocking issues - ([80ef7b5](https://github.com/lukexor/tetanes/commit/80ef7b50df3ea00500df70f86904d0a3cfcfdb53))
- Fixed blocking checking for updates on start - ([f48c634](https://github.com/lukexor/tetanes/commit/f48c63445bf2f2be224c7632781101c9a4075dbe))
- Revert rfd features back - ([30cec26](https://github.com/lukexor/tetanes/commit/30cec26fd44d284e124e33422d2cbcccd5d0814a))
- Fixed Data Man url - ([882004a](https://github.com/lukexor/tetanes/commit/882004ac9f44aa0c51ca88bf99552f9187fad8e9))
- Cleaned up pausing, parking, and control flow. Closes [#251](https://github.com/lukexor/tetanes/pull/251) - ([72cf88a](https://github.com/lukexor/tetanes/commit/72cf88ac6991953222bd3dd1d395f7f9035c98ef))
- Remove unfocused/occluded pausing for now until a less error-prone cross-platform solution can be designed - ([a5549e6](https://github.com/lukexor/tetanes/commit/a5549e6f026d201e9e6c7f0acfc56ee734c85a95))
- Remove bold from controls - ([0cfa0e9](https://github.com/lukexor/tetanes/commit/0cfa0e9edaad86b7566763ef8444649bef462af1))
- Fix excess redraw requests - ([caf88c0](https://github.com/lukexor/tetanes/commit/caf88c002fb8f18072e5fad95c967dd79f09afff))
- Fixed wasm resizing to be restricted by browser viewport ([#243](https://github.com/lukexor/tetanes/pull/243)) - ([b59d4c9](https://github.com/lukexor/tetanes/commit/b59d4c906fbd41d95e21e58bffc28074028947c4))
- Disable rewind when low on memory. clear rewind memory when disabled - ([4d5e1c4](https://github.com/lukexor/tetanes/commit/4d5e1c4dbe43cceb9ab8d4c33ca832830b2d31d8))
- Remove redrawing every clock - ([8cea6c1](https://github.com/lukexor/tetanes/commit/8cea6c14718dfd564b1f8ec9e60fc57b2d0602d0))
- Fixed web build relative urls - ([1423bdb](https://github.com/lukexor/tetanes/commit/1423bdb75766da2d656d24a6075ee38d36002ec9))
- Fixed a number of issues with loading roms and unintentionally blocking wasm - ([e257575](https://github.com/lukexor/tetanes/commit/e257575c24cde809d156ae6451575ec7cfd70aad))
- Fix clock timing on web. closes [#234](https://github.com/lukexor/tetanes/pull/234) - ([57d323d](https://github.com/lukexor/tetanes/commit/57d323d44408269b1c72932baa4b3b534e69f70d))
- Fix frame stats when toggled via menu. closes [#233](https://github.com/lukexor/tetanes/pull/233) - ([347066b](https://github.com/lukexor/tetanes/commit/347066b8f4000dcd88e3a76e8b948b55198ecceb))
- Add scrolling to lists - ([62ff074](https://github.com/lukexor/tetanes/commit/62ff0745a846cc8aed910244448becd98f155abf))
- Fix changing slider/drag values - ([8580135](https://github.com/lukexor/tetanes/commit/8580135f6e1e14a84214ca24efd57dbaf7595997))

### 🚜 Refactor


- Removed a number of panic cases and cleaned up platform checks - ([bdb71a9](https://github.com/lukexor/tetanes/commit/bdb71a96792778cb0ad6bedf44e0ef5cbfa703e4))
- Frame timing cleanup - ([1e920fd](https://github.com/lukexor/tetanes/commit/1e920fdd4ca56010ebe380152c817025a1b4c127))
- Some initialization error handling cleanup - ([507d9a0](https://github.com/lukexor/tetanes/commit/507d9a04c439081afc585761311082b0069c7111))
- Small gui cleanup - ([880e9ee](https://github.com/lukexor/tetanes/commit/880e9ee4b33d8b12924598e688f01b5e79289590))

### 📚 Documentation


- Fixed docs and changelog - ([4c7a694](https://github.com/lukexor/tetanes/commit/4c7a6949e52b6734fd6a78f6d9567c70e12b3ae4))

### ⚙️ Miscellaneous Tasks


- Split out web build so it can run on any platform - ([dcaec14](https://github.com/lukexor/tetanes/commit/dcaec14828288d758c713861a7aef6c03e4da47b))
- Upgrade ringbuf - ([5d7abe2](https://github.com/lukexor/tetanes/commit/5d7abe291493f43b14a0239157dfed930db17d6c))


## [0.10.0](https://github.com/lukexor/tetanes/compare/tetanes-v0.9.0..tetanes-v0.10.0) - 2024-05-16

### ⛰️  Features

- *(mapper)* Added Vrc6 mapper - ([fd2075d](https://github.com/lukexor/tetanes/commit/fd2075d98c7b4ef8643e5ea433c936201e414a04))

- Added controller support - ([7550bce](https://github.com/lukexor/tetanes/commit/7550bce09738cbfc5360c08e8323e70e68d0eb54))
- Initial re-structure of painter and viewports - ([5feabbe](https://github.com/lukexor/tetanes/commit/5feabbe197e59a9999960b40c2f069f7527eb421))
- Perf stats and ui cleanup - ([8d7d0d4](https://github.com/lukexor/tetanes/commit/8d7d0d482958d63d3b2c853dc01bf81f7bf54b84))
- Switched to lazy APU catch-up - ([4a95de3](https://github.com/lukexor/tetanes/commit/4a95de3e1eb364ac13e189b321d4cbe480e67da3))
- Add headless options, run_ahead methods, audio fixes, and performance improvements - ([a1a1b9b](https://github.com/lukexor/tetanes/commit/a1a1b9bae19530dfc4592a33cfb318f8d5e0df75))
- Added run-ahead feature - ([3349045](https://github.com/lukexor/tetanes/commit/3349045adce0ecdf9c38f0ccd41ddcbd72c9f7a8))
- Add cycle-accurate feature - ([6d0db9f](https://github.com/lukexor/tetanes/commit/6d0db9f1c0a5b1b7b83ab3c29b055b2addbad40f))
- Added rewind - ([4cc7b65](https://github.com/lukexor/tetanes/commit/4cc7b657b085666b5500c0f0c6f2eabcb7cdabd2))

### 🐛 Bug Fixes

- *(ppu)* Fixed oam read stomping on sprite0's y-byte - ([dc51191](https://github.com/lukexor/tetanes/commit/dc511914a14d5ccf2071022981a3126b9ff47b40))
- *(wasm)* Overhauled wasm build - ([2892587](https://github.com/lukexor/tetanes/commit/28925874661988423acab72be73b6ffb405924ee))

- Revert frame buffer back to u16 to fix emphasis - ([bc7f5fa](https://github.com/lukexor/tetanes/commit/bc7f5fa741a93877087e3b6f71c7ecfdacc06637))
- Made ppu warm optional, default to false - ([1693681](https://github.com/lukexor/tetanes/commit/16936817a5bd1278c835eedf6fa63d410f047c2a))
- Revert 240pee rename - ([f82f763](https://github.com/lukexor/tetanes/commit/f82f76397ea7e00afe171235e776207ed8b10c2b))
- Fixed 240pee test rom path - ([77f9702](https://github.com/lukexor/tetanes/commit/77f9702f0c2ff6caa073cdbbcf4816614daf8675))
- Fixed saving config - ([361447f](https://github.com/lukexor/tetanes/commit/361447fc1bc178498a23c2464c3f0fe353e5c165))
- Fixed setting APU sample_rate - ([ed52eb7](https://github.com/lukexor/tetanes/commit/ed52eb78d3b2cfb921622c19760d87fe72b1189e))
- Fixed selecting audio sample rate - ([24d6bfc](https://github.com/lukexor/tetanes/commit/24d6bfc78d5ce2021fcea7edbf53a9f2af3f74e9))
- Disabled webgpu since it panics on a double borrow currently - ([c93e7ad](https://github.com/lukexor/tetanes/commit/c93e7adbd2dd5ad3ba8c7fef5f60f874ccf839da))
- Remove toggling vsync and fixed wasm frame rate - ([f937396](https://github.com/lukexor/tetanes/commit/f9373964e5f91c5dc75b7bbd93c456f7253eae8d))
- Some timing and ui fixes - ([fe9d123](https://github.com/lukexor/tetanes/commit/fe9d1232df8294445d60db6ccc448f71df489d44))
- Fixed exrom irq - ([7cc2540](https://github.com/lukexor/tetanes/commit/7cc254034f40c76af6a5c444895afc34e1509e46))
- Removed unused revision - ([4376c9a](https://github.com/lukexor/tetanes/commit/4376c9a1396186dbfcd6a70f8c5d77940eb3f0b8))
- Cleaned up mappers - ([22c97e2](https://github.com/lukexor/tetanes/commit/22c97e228c8afb521be25c7a45eab5bb8b8e83cd))
- Documentation and ui updates - ([38fc88f](https://github.com/lukexor/tetanes/commit/38fc88f873bf2b5b27f0d82bbff82895747581ae))
- Fixed apu tests - ([6ff1362](https://github.com/lukexor/tetanes/commit/6ff1362a080143632a8384207c82adf9f9d244dc))
- More audio fixes - ([7e0f2f5](https://github.com/lukexor/tetanes/commit/7e0f2f5de552ef0b99a07f08e5bd47deced44df4))
- Fixed mmc5 pulse channels - ([0588103](https://github.com/lukexor/tetanes/commit/05881036c965e8ffb9231f0fd0a1738298e9b869))
- Some fixes for audio channels sounding off - ([46a773d](https://github.com/lukexor/tetanes/commit/46a773d81d86a60c6dded568bb1525fcd868468d))
- Fixed apu region sample_period - ([2a440ca](https://github.com/lukexor/tetanes/commit/2a440caa9d1cb43f9097b0a5c5e60a9ab746fa81))
- Fixed some frame rate performance issues - ([ca304b0](https://github.com/lukexor/tetanes/commit/ca304b05dc762478be54a2d365249bb4e05a8b68))
- Fixed replay and rewind - ([55dc8d7](https://github.com/lukexor/tetanes/commit/55dc8d7e06c3ec9366936d02599739119a8cc6f1))
- Fixed some path and config issues - ([ef60f1b](https://github.com/lukexor/tetanes/commit/ef60f1b9bd3f996e8bcb7435ed9da21f827cd327))
- Fixed some region, configs, and features - ([e5d4f4a](https://github.com/lukexor/tetanes/commit/e5d4f4a21cb96bf0051b8827150f76f1b3a579bc))
- Fixed chr_ram test - ([d24009b](https://github.com/lukexor/tetanes/commit/d24009bab6575a1c74cbd138745f57ad7a2c5cd7))
- Fixed apu linear counter loading - ([76ae795](https://github.com/lukexor/tetanes/commit/76ae7958df948e174cc3d9c1fe1f8ce5d3d7279a))
- Read nes2.0 region header - ([0c70e87](https://github.com/lukexor/tetanes/commit/0c70e87a4edc002e018d01598584c6bab1d0a2ae))
- Fixed chr-rom writing - ([50724a6](https://github.com/lukexor/tetanes/commit/50724a6a485cac0c4d87d941931ef2129cab6d4b))
- Improved PAL support - ([acb4db8](https://github.com/lukexor/tetanes/commit/acb4db8cc79af64c594a0990fd5d80043b2bb5cd))

### 🚜 Refactor

- *(cpu)* Moved DMA values inside CPU - ([7257d18](https://github.com/lukexor/tetanes/commit/7257d18ed2154787e8ad25fa1ea309d5442098c4))

- Various event/UI cleanup - ([bd1f984](https://github.com/lukexor/tetanes/commit/bd1f984ffbc41ca0ff73a80a000670c25d0f3361))
- Config overhaul and keybind menus - ([1fbb4ba](https://github.com/lukexor/tetanes/commit/1fbb4bafc66190b7d406434e12cf0666c3453bac))
- Thread local irq - ([cc9cbea](https://github.com/lukexor/tetanes/commit/cc9cbea644acf745ed5d99e2f80a92271c8d8d63))
- Various cleanup - ([889e41f](https://github.com/lukexor/tetanes/commit/889e41fb91f4db51f2269be7ce51d290fba48cca))
- Some audio cleanup - ([6eeff9e](https://github.com/lukexor/tetanes/commit/6eeff9e33a606869f46b29212dd06da18be26ab7))
- Major config overhaul - ([34076be](https://github.com/lukexor/tetanes/commit/34076be0266d3c3838630b57aa705371e9460c2d))
- Major platform and error handling overhaul - ([eb6e546](https://github.com/lukexor/tetanes/commit/eb6e5468ccb9a1d711658d55e6b3f0f84c154ddb))
- Audio mixer overhaul and .raw recording - ([44cc47c](https://github.com/lukexor/tetanes/commit/44cc47c932afb0ba136458fb962cf6ac39e865ac))
- Fixed audio - ([fe26c1b](https://github.com/lukexor/tetanes/commit/fe26c1bbb240ba447cd9487b5f0fb9adead6c2be))
- Clean up some wasm code - ([dd489d3](https://github.com/lukexor/tetanes/commit/dd489d38eca98e180534fe03b4920cc3b5fa4fa3))
- Inlined puffin for now - ([945ff0e](https://github.com/lukexor/tetanes/commit/945ff0e750727717dffab39528e321bf6dc8bc2e))
- Moved audio filtering/decimation to apu - ([4a38d23](https://github.com/lukexor/tetanes/commit/4a38d23df45594def155d1e11ed8869230195580))
- Major module overhaul - ([ca92f51](https://github.com/lukexor/tetanes/commit/ca92f5176855a635cb1244b912030d9c9859f7cd))
- Various updates - ([da213ae](https://github.com/lukexor/tetanes/commit/da213ae36aa0b4763c643d089e390d184d69dc19))
- Cleaned up menus - ([dd9e726](https://github.com/lukexor/tetanes/commit/dd9e726e8e3c9616aa09ff2cebf726462fe96f87))
- Made ram_state consistent - ([81d3bc9](https://github.com/lukexor/tetanes/commit/81d3bc9849c2dd30bca22933c5478b7c787259c7))

### 📚 Documentation


- Updated docs - ([806c078](https://github.com/lukexor/tetanes/commit/806c0789b65644d5a4e7d0365026fcf011d8770b))
- Updated readme - ([16db37d](https://github.com/lukexor/tetanes/commit/16db37d3fc8e1a5507e613e498bf239098e691e4))
- Fixed docs - ([3de4078](https://github.com/lukexor/tetanes/commit/3de40789ba4df94711d5dbf9361857479147e73b))
- Added temporary readme - ([bf0d5db](https://github.com/lukexor/tetanes/commit/bf0d5db99ec0baff09c0e4bdc2c2500d1025080f))
- Fixed README - ([6d48eec](https://github.com/lukexor/tetanes/commit/6d48eec5bb8fb4bfadea5f836776db1f64bbd04a))
- Fix README for real - ([a5fd672](https://github.com/lukexor/tetanes/commit/a5fd672c176c7c6ce52ae75cce656c18c62a221a))
- Fix README - ([05abf3c](https://github.com/lukexor/tetanes/commit/05abf3c89c7003559885c3b40b9d36dd591a9b2b))
- Updated README - ([ffb7b21](https://github.com/lukexor/tetanes/commit/ffb7b2129642d872b968b8017fdd7149e7e71d1d))
- Updated README roadmap - ([0941d86](https://github.com/lukexor/tetanes/commit/0941d863d6bb1a4d5b2347132127c4b350dcebbb))

### ⚡ Performance


- Improved cpu usage - ([e17a901](https://github.com/lukexor/tetanes/commit/e17a901c1a3bc0ce37ff416b19a2ec484234b87f))

### 🧪 Testing


- Fixed test - ([4beb787](https://github.com/lukexor/tetanes/commit/4beb78713c5a71eb3fb31fd293abee535b702184))
- Fixed tests - ([531683d](https://github.com/lukexor/tetanes/commit/531683d24e32ec5ebc17ba54420204c4411b531c))
- Moved tests to tetanes-core - ([3d105ff](https://github.com/lukexor/tetanes/commit/3d105ff5fcc28503844ed6187cf9560361a3f90f))

### ⚙️ Miscellaneous Tasks


- Added linux builds - ([3f9c244](https://github.com/lukexor/tetanes/commit/3f9c244c3b87817d679a91ad9e596bcad79ad498))
- Updated ci for release - ([da465d7](https://github.com/lukexor/tetanes/commit/da465d7f535e02b8526026ca4e443e2a23d58da4))
- Moved lints and added const fns - ([cfe5678](https://github.com/lukexor/tetanes/commit/cfe5678f541a125a742a01fdc157333fb60b3e7e))
- Updated deps and msrv - ([f66b75e](https://github.com/lukexor/tetanes/commit/f66b75ea2be1846c5aa70a4a94616134ebcaabe2))
- Updated deps and msrv - ([01d5f68](https://github.com/lukexor/tetanes/commit/01d5f6877b561c1bc3d5ea1e3b8fed294ee71439))
- Updated tetanes-web dependencies - ([3fe0dba](https://github.com/lukexor/tetanes/commit/3fe0dbaf59d508cddffe1b197e811d3089d22170))
- Updated ci - ([f34a125](https://github.com/lukexor/tetanes/commit/f34a125b98c164566905d9962368fff5bfb346a8))
- Increase MSRV - ([684e771](https://github.com/lukexor/tetanes/commit/684e771a488f1cb88541b62265556b0fc79664a8))
- Refactor workflows - ([41f4bc5](https://github.com/lukexor/tetanes/commit/41f4bc51bdc163704c3b481f72413c77d51a4a59))
- Update Cargo.lock - ([98168f8](https://github.com/lukexor/tetanes/commit/98168f8ef734a7fc5a5ad3592fb76fe07b4d9a62))
- Update license - ([7e84f7f](https://github.com/lukexor/tetanes/commit/7e84f7f38e54d216f5a97fac9af4d9690ed5ea40))
- Add readme badges - ([60c185c](https://github.com/lukexor/tetanes/commit/60c185c61dec39407eee8af5c069ffc456daec52))

## [0.9.0](https://github.com/lukexor/tetanes/compare/v0.8.0..tetanes-v0.9.0) - 2023-10-31

### ⛰️ Features

- Added famicom 4-player support (fixes #78 - ([141e4ed](https://github.com/lukexor/tetanes/commit/141e4ed7b33e93d1cf183be327070d6532a16324))
- Added clock_inspect to Cpu - ([34944e6](https://github.com/lukexor/tetanes/commit/34944e63a4b0c72626c3313d2b849f0fa64a1c62))
- Added `Mapper::empty()` - ([30678c1](https://github.com/lukexor/tetanes/commit/30678c127231614316b3df97d7c95501ae77287c))

### 🐛 Bug Fixes

- _(events)_ Fixed toggling menus - ([f30ade8](https://github.com/lukexor/tetanes/commit/f30ade860c3dd5ff995cadd4e409159d7fce9d91))

- Fixed wasm - ([7abd62a](https://github.com/lukexor/tetanes/commit/7abd62ad5a3178b5f77ae6c5c026d336d3237908))
- Fixed warnings - ([f88d760](https://github.com/lukexor/tetanes/commit/f88d760fa4d8c6fd9f532e70ba76f95b54273e5c))
- Fixed a number of bugs - ([5fd85af](https://github.com/lukexor/tetanes/commit/5fd85afd53efb8321f767b98372218eefc6e06a5))
- Fixed default tile attr - ([9002fa8](https://github.com/lukexor/tetanes/commit/9002fa87806fcd56009369bbcd2644400ae7c5a1))
- Fixed exram mode - ([8507984](https://github.com/lukexor/tetanes/commit/85079842d9ffe89b3e8ae61143642e09b3c471e6))
- Fix crosshair changes - ([10d843e](https://github.com/lukexor/tetanes/commit/10d843e78cb04a69ad8d40e334a21268f294861b))
- Fix audio on loading another rom - ([d7cc16c](https://github.com/lukexor/tetanes/commit/d7cc16cf7475f2b8bd632c2fc74a7b3c09447127))
- Improved wasm render performance - ([561be90](https://github.com/lukexor/tetanes/commit/561be907f652a7f4879e943b44274547c0e43172))
- Web audio tweaks - ([5184e11](https://github.com/lukexor/tetanes/commit/5184e11ba6fd104b59668e57744fb03b993483b5))
- Fixed game genie codes - ([0206d6f](https://github.com/lukexor/tetanes/commit/0206d6fd20e5aa66da716eb07c6e077bfe4c5eed))
- Fixed update rate clamping - ([2133b84](https://github.com/lukexor/tetanes/commit/2133b84ba865a0e715fa98129d9d6997540bd3e5))
- Fix resetting sprite presence - ([d219ce0](https://github.com/lukexor/tetanes/commit/d219ce02320573498c84651af1585df08ed0c44e))
- Fixed missing Reset changes - ([808fcac](https://github.com/lukexor/tetanes/commit/808fcac032d3b10731e330ecdcb9c468117a5425))
- Fixed missed clock changes - ([1b5313b](https://github.com/lukexor/tetanes/commit/1b5313bf61115457beb50d919bb1129e54adc7cc))
- Fixed toggling debugger - ([e7bcfc1](https://github.com/lukexor/tetanes/commit/e7bcfc1238fd21f957eed582b92a35e236e9884c))
- Fixed resetting output_buffer - ([0802b2b](https://github.com/lukexor/tetanes/commit/0802b2b35c14f34fc8d3e73f3bd1ce940b4a8f48))
- Fixed confirm quit - ([48d6538](https://github.com/lukexor/tetanes/commit/48d6538d25833a0817b0a27344a58a2a4918ab68))

### 🚜 Refactor

- Various updates - ([da213ae](https://github.com/lukexor/tetanes/commit/da213ae36aa0b4763c643d089e390d184d69dc19))
- Small renames - ([0dea0b6](https://github.com/lukexor/tetanes/commit/0dea0b6d15204f1fdfbe91ef8f9365993ffafaf2))
- Various cleanup - ([8d25103](https://github.com/lukexor/tetanes/commit/8d251030a9782bfb9f18fb29ce67b3853b8cf9bd))
- Cleaned up some interfaces - ([da3ba1b](https://github.com/lukexor/tetanes/commit/da3ba1b1b93f7ecacec3a93746026c0da174cbd4))
- Genie code cleanup - ([e483eb5](https://github.com/lukexor/tetanes/commit/e483eb5e9a793b8883710399a7bb39c3cc6ed3ee))
- Added region getters - ([74d4a76](https://github.com/lukexor/tetanes/commit/74d4a769fd089e3ff18295d88c5eaa60dcc208be))
- Cleaned up setting region - ([45dc2a4](https://github.com/lukexor/tetanes/commit/45dc2a42928a7d9c507351965969b7976ca7b25c))
- Flatten NTSC palette - ([792d7db](https://github.com/lukexor/tetanes/commit/792d7dbc45ec230df4de63b552d24bb4bbabc5c6))
- Converted system palette to array of tuples - ([284f54b](https://github.com/lukexor/tetanes/commit/284f54b877ccbf6103920cba483ee7d0175f4c5d))
- Condensed MapRead and MapWrite to MemMap trait - ([bce1c77](https://github.com/lukexor/tetanes/commit/bce1c7794ab0dc6ab493618f697fcc088864afb0))
- Made control methods consistent - ([f93040d](https://github.com/lukexor/tetanes/commit/f93040d25128f50c20226ba3d52d638dbdd85ac3))
- Switch u16 addresses to use from_le_bytes - ([d8936af](https://github.com/lukexor/tetanes/commit/d8936afaf8e3da54616d430cf3488f64a1aae5ef))
- Moved genie to it's own module - ([77b571f](https://github.com/lukexor/tetanes/commit/77b571f990c4f86d30e160382ff773486a8a54a9))
- Cleaned up Power and Clock traits - ([533c0c3](https://github.com/lukexor/tetanes/commit/533c0c3485cc73f880c4d43b2f937c0e606d0360))
- Cleaned up bg tile fetching - ([0710f16](https://github.com/lukexor/tetanes/commit/0710f162928964209898ef7fdf6aacd3a3e4a1a0))
- Move NTSC palette declaration - ([9edffd1](https://github.com/lukexor/tetanes/commit/9edffd1be33b3f79e1fb1a187bc89d3aede58804))
- Cleaned up memory traits - ([c98f7ff](https://github.com/lukexor/tetanes/commit/c98f7fffc59f3ac399864c7ff130cbaad99762f6))
- Swapped lazy_static for once_cell - ([cc9e67f](https://github.com/lukexor/tetanes/commit/cc9e67f643cf60ad982c88979b92f0ca843d505a))

### ⚡ Performance

- Cleaned up inlines - ([b791cc3](https://github.com/lukexor/tetanes/commit/b791cc3ef7ece4fe0b627ff7332020453aa086ce))
- Added inline to cart clock - ([eb9a0e0](https://github.com/lukexor/tetanes/commit/eb9a0e0d04fa6ec2d7f36935841dbda36a365bf8))
- Changed decoding loops - ([a181ed4](https://github.com/lukexor/tetanes/commit/a181ed46e23534294463856ca3fec3c55cf2938f))
- Performance tweaks - ([0c06758](https://github.com/lukexor/tetanes/commit/0c0675811709a2b590c273cd9010766e055b61ad))

### 🎨 Styling

- Fixed nightly lints - ([3bab00d](https://github.com/lukexor/tetanes/commit/3bab00dd1478be71f11baaed97c3aa8f3cc6d241))

### 🧪 Testing

- Disable broken test for now - ([93857cb](https://github.com/lukexor/tetanes/commit/93857cbd81d5b3f82ed157fdb87c0fa22aff1bc7))
- Remove vimspector for global config - ([56edc15](https://github.com/lukexor/tetanes/commit/56edc15af9285d27d2e180ae585ecf91cc6f1e2a))

### ⚙️ Miscellaneous Tasks

- Increase MSRV - ([684e771](https://github.com/lukexor/tetanes/commit/684e771a488f1cb88541b62265556b0fc79664a8))

## [0.8.0](https://github.com/lukexor/tetanes/compare/v0.7.0...v0.8.0) - 2022-06-20

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

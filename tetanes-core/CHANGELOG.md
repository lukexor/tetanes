<!-- markdownlint-disable-file no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.10.0] - 2024-05-16

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



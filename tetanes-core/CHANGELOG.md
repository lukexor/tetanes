<!-- markdownlint-disable-file no-duplicate-heading -->

# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.10.1](https://github.com/lukexor/tetanes/compare/0.10.0..0.10.1) - 2024-06-12

### ⛰️  Features


- Shader support with crt-easymode ([#285](https://github.com/lukexor/tetanes/pull/285)) - ([e5042ef](https://github.com/lukexor/tetanes/commit/e5042efd45642ac2a13d7ac695bba1cce77c69c9))
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


- Avoid serde_json::from_reader in tests as it's faster to just … ([#244](https://github.com/lukexor/tetanes/pull/244)) - ([3ca03ac](https://github.com/lukexor/tetanes/commit/3ca03ac68fab4d809dee39466fd661f887d2575d))


## [0.10.0](https://github.com/lukexor/tetanes/compare/tetanes-v0.9.0..tetanes-core-v0.10.0) - 2024-05-16

Initial release.

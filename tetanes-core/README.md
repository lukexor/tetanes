# TetaNES Core

## Summary

This is the core emulation library for TetaNES. Savvy developers can build their
own libraries or UIs on top of it.

## Minimum Supported Rust Version (MSRV)

The current minimum Rust version is `1.74.0`.

## Features

### Crate Feature Flags

- **cycle-accurate** - Enables cycle-accurate emulation. More CPU intensive, but
  supports a wider range of games requiring precise timing. Disabling may
  improve performance on lower-end machines. Enabled by default.
- **profiling** - Enables [puffin](https://github.com/EmbarkStudios/puffin)
  profiling.

### Building

To build the project, you'll need a nightly version of the compiler and run
`cargo build` or `cargo build --release` (if you want better framerates).

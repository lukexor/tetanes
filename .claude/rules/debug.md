---
paths:
  - "tetanes-core/src/debug.rs"
  - "tetanes-core/src/debug/**/*"
  - "tetanes/src/nes/renderer/gui/ppu_viewer.rs"
---

# Debugging support

What the core exposes to a debugger.

**A debugger callback is handed the whole `Bus`** (`debug.rs`), not one component's state, because
what a debugger needs differs per debugger. A CPU debugger wants registers and the disassembly
around PC, an APU viewer the channels, a hex viewer an arbitrary range, the PPU viewer CHR resolved
through the board. Each viewer's closure runs at the break point and copies out only what it ships
to its own thread, so `ppu_viewer.rs`'s `PpuSnapshot` is one such choice, not the API. Core's part
is `Bus::copy_ppu_bus`, which fills a buffer with `$0000-$2FFF` as currently banked, so no consumer
needs board knowledge. The dot is the only trigger today, and `Debugger` is the struct to extend
when breakpoints land.

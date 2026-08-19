---
paths:
  - "tetanes-core/src/debug.rs"
  - "tetanes-core/src/debug/**/*"
  - "tetanes/src/nes/renderer/gui/debugger.rs"
  - "tetanes/src/nes/renderer/gui/ppu_viewer.rs"
---

# Debugging support

What the core exposes to a debugger, and what a debugger records as it runs.

**A debugger callback is handed the whole `Bus`** (`debug.rs`), not one component's state, because
what a debugger needs differs per debugger. A CPU debugger wants registers and the disassembly
around PC, an APU viewer the channels, a hex viewer an arbitrary range, the PPU viewer CHR resolved
through the board. Each viewer's closure runs at the break point and copies out only what it ships
to its own thread, so `ppu_viewer.rs`'s `PpuSnapshot` is one such choice, not the API. Core's part
is `Bus::copy_ppu_bus`, which fills a buffer with `$0000-$2FFF` as currently banked, so no consumer
needs board knowledge. The dot is the only trigger `Debugger` has.

**A breakpoint is a stop condition the caller passes in, not a callback.**
`ControlDeck::clock_frame_until` takes a predicate over PC, checks it between instructions, and
reports `Clocked::Stopped` with the frame left half clocked for the next call to finish, so
stopping unwinds to whoever drives the console instead of running arbitrary work in the middle of
a frame. `clock_frame` shares its display-frame accounting through `clock_frame_with` and so adds
nothing for it. The UI owns the list of addresses (`renderer/gui/debugger.rs`), and an empty list
keeps a console with no breakpoints on the frame-at-a-time path.

**What a debugger records as it runs lives on `Bus` behind an `Option`,** the way `pc_history` and
`code_map` (`debug.rs`) both do: `None` by default, so a console with no debugger open pays a
branch on a cold field rather than the work. Each is `#[serde(skip)]` and moved across
`Bus::swap_state` rather than swapped with the state, because what has been learned belongs to the
session. Rewinding a few frames does not unlearn which bytes are instructions.

`CodeMap` is **keyed by `Memory` offset, not CPU address** (`Memory::prg_offset` translates), so a
mark survives the bank switch that moves the byte and two banks sharing an address do not share
marks. It answers the one question a disassembler cannot: 6502 instructions are one to three bytes
with no alignment, so a sweep that starts inside data stays out of step until it happens to
realign. Only execution writes to it, and nothing decodes ahead, so `Bus::load_cart` starts it over
at the new cart's size and a fresh session shows unrun ROM as `unknown` until the game reaches it.
Closing a debugger calls `Bus::detach_code_map`, which stops the recording and hands the marks
back. They stay true for as long as the cart is loaded, and `CodeMap::covers` refuses a map
recorded against a different one.

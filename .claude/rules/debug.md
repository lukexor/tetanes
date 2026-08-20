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
`ControlDeck::clock_frame_until` and `clock_scanline_until` take a predicate over the bus, check it
between instructions, and report that they stopped short with the frame or scanline left half
clocked for the next call to finish, so stopping unwinds to whoever drives the console instead of
running arbitrary work in the middle of a frame. `clock_frame` and `clock_scanline` are those two
with a predicate that never fires, and `clock_frame` shares its display-frame accounting through
`clock_frame_with`. The UI owns the list of addresses (`renderer/gui/debugger.rs`), and an empty
list keeps a console with no breakpoints on the frame-at-a-time path.

**A condition is parsed once and evaluated per access.** `debug/expr.rs` compiles the text to a
stack machine with no strings in it, and `Expr::eval` walks that against a `Bus` with a fixed-size
stack, since a condition is asked on the emulation thread at the moment of the access. Memory is
read through `peek`, so asking moves nothing. The grammar is a JavaScript subset - `mem[addr]`,
`mem16[addr]`, lowercase registers and flags, `0x`/`0b` literals - because plugins are meant to
drive these same hooks from outside through a JS engine, and one surface syntax beats two. `$FF`
parses as well, since every other box in the debugger writes an address that way.

Deciding is split from recording (`Breakpoints::check` and `record`) because a condition reads the
whole console and the console owns the breakpoints. The window leaves a breakpoint whose condition
does not parse **unarmed**, rather than arming it without the condition, since the second stops on
far more than was asked for.

**Every way of running the console asks one condition,** `emulation.rs`'s `breaks_here`, so a step
stops at a breakpoint the way a resume does. Step into and step over clock an instruction, step out
and step over's tail run `step_until`, and stepping a scanline or a frame goes through the `_until`
pair. A step that stops reports through `on_breakpoint`, the same path the running console takes.

**A breakpoint carries the arena offset of its start address,** taken when it is set, and
`Breakpoint::matches` stops only where the mapping still puts those bytes at that address. A CPU
address names a window rather than a byte, so without it a breakpoint set on one bank's instruction
also fires on whatever another bank puts there. `None` where the address has no offset - work RAM,
the registers, an unmapped page - which leaves those keyed by address. The window resolves the
offset from the `prg_pages` copy on `CpuSnapshot` through `Memory::offset_in`, so a breakpoint is
pinned to the bank that was on screen when it was set, one address lists one breakpoint per bank,
and a breakpoint whose bank has been switched out greys in the list rather than silently going
quiet.

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

use crate::nes::{
    action::{Debug, DebugInterrupt, DebugStep},
    config::Config,
    debug::{AddrLabel, LabelKey, Marks},
    event::{ConfigEvent, DebugRequest, DebugWrite, EmulationEvent, NesEventProxy, UiEvent},
    renderer::gui::{
        MessageType,
        lib::ViewportOptions,
        palette::Palette,
        panes::{self, Column, Pane as _},
    },
};
use egui::{
    CentralPanel, Color32, Context, Grid, Label, Panel, Rect, RichText, ScrollArea, Sense, Ui,
    Vec2, ViewportClass, ViewportId, text::CCursor,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tetanes_core::{
    bus::Bus,
    cpu::{Cpu, Disasm, Status, instr::InstrRef},
    debug::{
        Access, AccessHit, Breakpoint as DeckBreakpoint, Breakpoints as DeckBreakpoints, CallFrame,
        FrameKind, RunTo,
        expr::{Expr, ParseError},
    },
    memory::{Memory, PRG_PAGES, Page},
};

/// A range of addresses the console stops at when one is accessed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub struct Breakpoint {
    /// Names this breakpoint for as long as it is listed.
    ///
    /// An address does not, since several breakpoints can cover one - a range and a single
    /// address, or two accesses of the same byte under different conditions. Handed out by
    /// [`Breakpoints::add`], so one read back from a file is renumbered as it is listed.
    #[serde(skip)]
    pub id: u32,
    /// First address covered.
    pub addr: u16,
    /// Last address covered, inclusive. Equal to `addr` for a single address.
    pub end: u16,
    /// Where `addr` sat in the cart arena when the breakpoint was set. See
    /// [`DeckBreakpoint::offset`].
    pub offset: Option<u32>,
    /// Which accesses trip it.
    pub access: Access,
    /// Cleared to keep a breakpoint in the list without stopping at it.
    pub enabled: bool,
    /// Cleared to record the access in the list and let the console run on.
    pub breaks: bool,
    /// An expression that has to hold as well, as typed. Empty to trip on every access.
    ///
    /// Kept as text rather than as a parsed [`Expr`], since the box holds whatever is being typed
    /// and half an expression is not one yet.
    pub condition: String,
}

impl Breakpoint {
    /// A breakpoint on a single address, stopping before it executes.
    ///
    /// `offset` pins it to the bank mapped at `addr` as the window draws it. The id is handed out
    /// by [`Breakpoints::add`], so one built here carries a placeholder until it is listed.
    pub const fn execute(addr: u16, offset: Option<u32>) -> Self {
        Self {
            id: 0,
            addr,
            end: addr,
            offset,
            access: Access::EXEC,
            enabled: true,
            breaks: true,
            condition: String::new(),
        }
    }

    /// A breakpoint over a range, stopping on any access to it.
    ///
    /// A range is typed rather than clicked, which usually means data, so it watches reads and
    /// writes as well as execution.
    pub fn range(addr: u16, end: u16, offset: Option<u32>) -> Self {
        Self {
            end,
            access: Access::all(),
            ..Self::execute(addr, offset)
        }
    }

    /// Whether `addr` falls in this breakpoint's range.
    ///
    /// Says nothing about the bank, which `is_mapped` answers.
    pub const fn covers(&self, addr: u16) -> bool {
        self.addr <= addr && addr <= self.end
    }

    /// The range as the list writes it, one address or two.
    pub fn range_text(&self) -> String {
        if self.addr == self.end {
            format!("${:04X}", self.addr)
        } else {
            format!("${:04X}-${:04X}", self.addr, self.end)
        }
    }

    /// The condition as parsed, or `None` when the box is empty.
    pub fn condition(&self) -> Option<Result<Expr, ParseError>> {
        let text = self.condition.trim();
        (!text.is_empty()).then(|| Expr::parse(text))
    }

    /// What the console is told, which drops the parts only the list draws.
    ///
    /// `None` for a condition that does not parse. Arming it without the condition would stop on
    /// far more than was asked for, so the breakpoint stays listed and unarmed until the
    /// expression is one.
    fn armed(&self) -> Option<DeckBreakpoint> {
        Some(DeckBreakpoint {
            start: self.addr,
            end: self.end,
            offset: self.offset,
            access: self.access,
            breaks: self.breaks,
            condition: self.condition().transpose().ok()?,
        })
    }
}

/// The Debugger's breakpoints, in address order so the list reads like the disassembly.
///
/// The console is only ever told the enabled addresses, since that is all it can act on. The
/// window draws the rest.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[must_use]
pub struct Breakpoints {
    list: Vec<Breakpoint>,
    /// The next id to hand out, which only ever counts up.
    ///
    /// Reusing the id of a removed breakpoint would let a stale reference - the open editor, a
    /// pending row action - land on whatever took its place.
    next_id: u32,
}

impl Breakpoints {
    /// Add `breakpoint`, naming it and reporting the id it was given.
    ///
    /// Overlap is allowed: one address holds as many breakpoints as are set on it, since a range
    /// and a single address, or two conditions on one byte, are different questions.
    pub fn add(&mut self, mut breakpoint: Breakpoint) -> Option<u32> {
        if self.list.len() >= DeckBreakpoints::MAX {
            return None;
        }
        let id = self.next_id;
        self.next_id += 1;
        breakpoint.id = id;
        let index = self
            .list
            .partition_point(|other| other.addr < breakpoint.addr);
        self.list.insert(index, breakpoint);
        Some(id)
    }

    /// Remove the breakpoint `id` names, reporting whether it was listed.
    pub fn remove(&mut self, id: u32) -> bool {
        let held = self.list.len();
        self.list.retain(|breakpoint| breakpoint.id != id);
        self.list.len() != held
    }

    /// Stop before the instruction at `addr` in the bank at `offset` executes, or clear the
    /// breakpoint already on exactly that address.
    ///
    /// A range covering `addr` is left alone. Removing a range by clicking one row inside it
    /// would take away far more than the row the click landed on.
    pub fn toggle(&mut self, addr: u16, offset: Option<u32>) {
        match self.single_at(addr, offset) {
            Some(id) => {
                self.remove(id);
            }
            None => {
                self.add(Breakpoint::execute(addr, offset));
            }
        }
    }

    /// The breakpoint set on exactly `addr` in the bank at `offset`, which is the one a gutter
    /// click owns.
    pub fn single_at(&self, addr: u16, offset: Option<u32>) -> Option<u32> {
        self.list
            .iter()
            .find(|breakpoint| {
                breakpoint.addr == addr && breakpoint.end == addr && breakpoint.offset == offset
            })
            .map(|breakpoint| breakpoint.id)
    }

    /// Every breakpoint covering `addr` as `pages` has it mapped, in list order.
    ///
    /// A range covers every row in it, not only the one it starts on.
    pub fn covering<'a>(
        &'a self,
        addr: u16,
        pages: &'a [Page; PRG_PAGES],
    ) -> impl Iterator<Item = &'a Breakpoint> {
        self.list
            .iter()
            .filter(move |breakpoint| breakpoint.covers(addr) && is_mapped(pages, breakpoint))
    }

    /// Whether another breakpoint would be refused.
    pub const fn is_full(&self) -> bool {
        self.list.len() >= DeckBreakpoints::MAX
    }

    /// The breakpoint `id` names, whether or not it is enabled.
    pub fn get(&self, id: u32) -> Option<&Breakpoint> {
        self.list.iter().find(|breakpoint| breakpoint.id == id)
    }

    /// The breakpoint `id` names, to edit in place.
    pub fn get_mut(&mut self, id: u32) -> Option<&mut Breakpoint> {
        self.list.iter_mut().find(|breakpoint| breakpoint.id == id)
    }

    /// What the console is to act on, which is the enabled ones with an access selected.
    pub fn armed(&self) -> Vec<DeckBreakpoint> {
        self.list
            .iter()
            .filter(|breakpoint| breakpoint.enabled && !breakpoint.access.is_empty())
            .filter_map(Breakpoint::armed)
            .collect()
    }

    /// The breakpoints in address order, for the window to edit in place.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Breakpoint> {
        self.list.iter_mut()
    }

    /// Every breakpoint listed, in address order.
    pub fn iter(&self) -> impl Iterator<Item = &Breakpoint> {
        self.list.iter()
    }

    /// Put the list back in address order, so it reads like the disassembly again.
    ///
    /// [`Breakpoints::add`] inserts in order, which an edit to a breakpoint's address moves out
    /// from under. Stable, so breakpoints sharing an address keep the order they were added in.
    pub fn sort(&mut self) {
        self.list.sort_by_key(|breakpoint| breakpoint.addr);
    }

    /// Drop the breakpoints pinned to a cart, keeping the ones any cart shares.
    pub fn retain_without_cart(&mut self) {
        self.list.retain(|breakpoint| breakpoint.offset.is_none());
    }

    /// Whether no breakpoint is listed, enabled or not.
    pub const fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

/// A range of the address space shown collapsed and not disassembled.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum BlockKind {
    /// Work RAM. Code can run from it, but nothing yet says which bytes are code.
    Ram,
    /// PPU, APU and IO registers.
    Registers,
    /// Cart RAM, usually save data.
    SaveRam,
    /// No cart window is mapped here.
    Unmapped,
    /// Cart ROM that nothing has shown to be instructions.
    Unknown,
}

impl BlockKind {
    /// The rendered label for an address block.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Ram => "work ram",
            Self::Registers => "registers",
            Self::SaveRam => "save ram",
            Self::Unmapped => "unmapped",
            Self::Unknown => "unknown",
        }
    }
}

/// A line of the address space view.
#[derive(Debug, Clone)]
#[must_use]
pub enum Row {
    /// A disassembled instruction.
    Instruction(Disasm),
    /// A collapsed range, inclusive of both ends.
    Block {
        start: u16,
        end: u16,
        kind: BlockKind,
    },
    /// The name given to the address the next row starts at, on a line of its own above it.
    Label { addr: u16, name: String },
}

impl Row {
    /// The address this row starts at.
    pub const fn addr(&self) -> u16 {
        match self {
            Self::Instruction(disasm) => disasm.addr,
            Self::Block { start, .. } | Self::Label { addr: start, .. } => *start,
        }
    }

    /// Whether `addr` falls in this row, which for a block is anywhere in its range.
    /// A label covers nothing: it sits above the row that owns its address, so PC and the
    /// selection mark that row rather than the name over it.
    pub const fn covers(&self, addr: u16) -> bool {
        match self {
            Self::Instruction(disasm) => disasm.addr == addr,
            Self::Block { start, end, .. } => *start <= addr && addr <= *end,
            Self::Label { .. } => false,
        }
    }
}

/// Snapshot of the Control Deck CPU state for use by the Debugger.
#[derive(Debug, Clone)]
#[must_use]
pub struct CpuSnapshot {
    /// CPU state and registers.
    pub cpu: Cpu,
    /// CPU stack.
    pub stack: Vec<u8>,
    /// The calls execution is inside, outermost first, from [`DebugRequest::call_stack`].
    pub call_stack: Vec<CallFrame>,
    /// Previously executed instructions, oldest first, ending just before PC from
    /// [`DebugRequest::history_lines`].
    pub history: Vec<Disasm>,
    /// The range requested from [`DebugRequest::memory`], if any.
    pub memory: Vec<u8>,
    /// The address [`CpuSnapshot::memory`] starts at, so its bytes line up with the rows however
    /// far behind the window the snapshot is.
    pub memory_start: u16,
    /// Accesses that breakpoints recorded without stopping, oldest first.
    pub access_log: Vec<AccessHit>,
    /// What the Watches pane's expressions came to, in the order it listed them.
    ///
    /// `None` where the row holds something that does not parse, so the values line up with the
    /// rows whatever is half-written.
    pub watches: Vec<Option<i32>>,
    /// The PRG page table, which resolves an address to the cart byte currently mapped there.
    ///
    /// Sent every frame while running and after every step, so a breakpoint is pinned to the bank
    /// the window was drawing when it was set, and a row draws the mark only while that bank is
    /// still in.
    pub prg_pages: [Page; PRG_PAGES],
    /// The frame the console is on, which scales the cycle count into something readable.
    pub frame: u32,
}

impl Default for CpuSnapshot {
    fn default() -> Self {
        Self {
            cpu: Cpu::default(),
            stack: Vec::new(),
            call_stack: Vec::new(),
            history: Vec::new(),
            memory: Vec::new(),
            memory_start: 0,
            access_log: Vec::new(),
            watches: Vec::new(),
            prg_pages: [Page::UNMAPPED; PRG_PAGES],
            frame: 0,
        }
    }
}

impl CpuSnapshot {
    /// Capture the requested snapshot.
    pub fn capture(bus: &Bus, request: &DebugRequest) -> Self {
        // Disassembled from memory as it is banked *now*, not as it was when these ran, so a line
        // whose bank has since been swapped shows what currently lives at that address. The
        // code map does not help here - it says which bytes are code, not which bank an address
        // was mapped to at the time. Recording the bytes alongside the address would.
        let history = bus.pc_history.as_ref().map_or_else(Vec::new, |history| {
            let skip = history
                .len()
                .saturating_sub(usize::from(request.history_lines));
            history
                .iter()
                .skip(skip)
                .map(|addr| {
                    let mut pc = addr;
                    bus.disassemble(&mut pc)
                })
                .collect()
        });

        Self {
            cpu: bus.cpu.clone(),
            stack: if request.stack {
                (0..0x0100u16)
                    .map(|offset| bus.peek(Cpu::SP_BASE + offset))
                    .collect()
            } else {
                Vec::new()
            },
            call_stack: match (request.call_stack, &bus.call_stack) {
                (true, Some(call_stack)) => call_stack.frames().to_vec(),
                _ => Vec::new(),
            },
            history,
            memory: request.memory.map_or_else(Vec::new, |(start, len)| {
                (0..len).map(|i| bus.peek(start.wrapping_add(i))).collect()
            }),
            memory_start: request.memory.map_or(0, |(start, _)| start),
            // Filled by the caller, which owns the console the log is drained from.
            access_log: Vec::new(),
            // Filled by the caller, which owns the expressions the window asked for.
            watches: Vec::new(),
            prg_pages: *bus.memory.prg_pages(),
            frame: bus.ppu.frame_number(),
        }
    }
}

/// The CPU address space as rows.
///
/// Rebuilt when the board's PRG mapping changes, since that is the only thing that can move an
/// instruction, and when the [`CodeMap`](tetanes_core::debug::CodeMap) marks something new, since
/// that determines whether an address is an instruction. The rows represent the currently mapped
/// banks, not where PC happens to be.
#[derive(Debug, Default, Clone)]
#[must_use]
pub struct AddressSpace {
    pub rows: Vec<Row>,
}

impl AddressSpace {
    /// Capture the current address space into rows.
    ///
    /// Only mapped cart ROM is disassembled, and only where the
    /// [`CodeMap`](tetanes_core::debug::CodeMap) marks instructions or where straight-line flow
    /// from PC reaches. Everything else is a collapsed block.
    pub fn capture(bus: &Bus, labels: &HashMap<LabelKey, AddrLabel>) -> Self {
        let mut rows = Vec::new();
        let mut disasm = Disasm::default();
        let mut addr = 0u32;
        let pc = bus.cpu.pc;
        // Set while decoding forward from PC. The map contains only what has executed, so a
        // debugger that has just attached knows nothing about the routine PC is sitting in.
        let mut following = false;

        while addr <= u32::from(u16::MAX) {
            let start = addr as u16;
            // Ahead of the row that owns the address, so the name reads as a heading over it the
            // way an assembler listing writes one. A row with only a comment gets no line here,
            // since the comment is drawn on the instruction itself.
            //
            // A board that mirrors one bank into two windows shows the name at both, which is
            // what filing it by cart offset means: those addresses are the same bytes.
            let key = LabelKey::new(start, prg_offset(bus.memory.prg_pages(), start));
            if let Some(name) = labels.get(&key).map(|label| label.name.trim())
                && !name.is_empty()
            {
                rows.push(Row::Label {
                    addr: start,
                    name: name.to_string(),
                });
            }
            let next = match Self::block_kind(bus, start) {
                Some(kind) => {
                    following = false;
                    let mut end = start;
                    while end < u16::MAX && Self::block_kind(bus, end + 1) == Some(kind) {
                        end += 1;
                    }
                    rows.push(Row::Block { start, end, kind });
                    u32::from(end) + 1
                }
                None if following || Self::starts_instruction(bus, start, pc) => {
                    if start == pc {
                        following = true;
                    }
                    if following {
                        following = Cpu::INSTR_REF[usize::from(bus.peek(start))]
                            .instr
                            .falls_through();
                    }
                    let mut next = start;
                    bus.disassemble_into(&mut next, &mut disasm);
                    // Taken from the addressing mode rather than from where the decode landed,
                    // which wraps past $FFFF.
                    let len = disasm.len();
                    let end = start.wrapping_add(len - 1);

                    // PC is known to start an instruction, so a decode that overlaps it does not
                    // align with real instruction boundaries inside an instruction that has run
                    // (e.g. stepping into a jump computed at runtime). Skip decoding those bytes
                    // and resume there. Ensures the PC keeps a row of its own for the Debugger to
                    // highlight.
                    //
                    // Strictly after `start`, not from it: a decode that *begins* at PC is the
                    // aligned case. Including it would end the row where it started and resume at
                    // the same address, decoding forever.
                    if pc > start && pc <= end {
                        rows.push(Row::Block {
                            start,
                            end: pc - 1,
                            kind: BlockKind::Unknown,
                        });
                        u32::from(pc)
                    } else {
                        rows.push(Row::Instruction(disasm.clone()));
                        addr + u32::from(len)
                    }
                }
                // Mapped ROM that nothing has shown to be an instruction - the operands of the
                // instruction above, a jump table, graphics, or code that has yet to run.
                None => {
                    following = false;
                    let mut end = start;
                    while end < u16::MAX
                        && Self::block_kind(bus, end + 1).is_none()
                        && !Self::starts_instruction(bus, end + 1, pc)
                    {
                        end += 1;
                    }
                    rows.push(Row::Block {
                        start,
                        end,
                        kind: BlockKind::Unknown,
                    });
                    u32::from(end) + 1
                }
            };
            // Every branch should progress by at least one address otherwise it'll spin forever.
            debug_assert!(next > addr, "sweep made no progress at ${start:04X}");
            addr = next.max(addr + 1);
        }

        Self { rows }
    }

    /// Whether `addr` collapses into a block. `None` means cart memory, which
    /// [`AddressSpace::starts_instruction`] then decides how to render.
    fn block_kind(bus: &Bus, addr: u16) -> Option<BlockKind> {
        match addr {
            0x0000..=0x1FFF => Some(BlockKind::Ram),
            0x2000..=0x401F => Some(BlockKind::Registers),
            _ => match bus.memory.prg_offset(addr) {
                Some(_) if addr >= 0x8000 => None,
                // Cart RAM, which some boards run code out of. It reads as save data until the
                // code map has seen something execute there.
                Some(offset) => {
                    let executed = bus
                        .code_map
                        .as_ref()
                        .is_some_and(|code_map| code_map.is_code(offset));
                    if executed {
                        None
                    } else {
                        Some(BlockKind::SaveRam)
                    }
                }
                None => Some(BlockKind::Unmapped),
            },
        }
    }

    /// Whether the capture sweep should start decoding at `addr`.
    ///
    /// Execution is the only thing that says where an instruction begins, so this checks the
    /// [`CodeMap`](tetanes_core::debug::CodeMap). PC counts as well - it is about to run, so it
    /// starts an instruction whether or not it has yet, and it is the row the view highlights.
    fn starts_instruction(bus: &Bus, addr: u16, pc: u16) -> bool {
        addr == pc
            || match &bus.code_map {
                // With nothing recorded there is no way to tell code from data, so everything
                // mapped is decoded and the capture sweep drifts out of step wherever data
                // resides.
                None => true,
                Some(code_map) => bus
                    .memory
                    .prg_offset(addr)
                    .is_some_and(|offset| code_map.is_code(offset)),
            }
    }

    /// The row holding `addr`, or the one just before it when `addr` is mid-instruction.
    pub fn row_at(&self, addr: u16) -> Option<usize> {
        self.rows
            .partition_point(|row| row.addr() <= addr)
            .checked_sub(1)
    }
}

/// A view in the debugger window.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[must_use]
pub enum Pane {
    /// The address space as rows, which is the pane the window is built around.
    Disassembly,
    /// The CPU registers and status flags.
    Registers,
    /// The stack page, top of stack first.
    Stack,
    /// The calls execution is inside, innermost first.
    CallStack,
    /// The breakpoint list, and the box that adds one.
    Breakpoints,
    /// The instructions that ran most recently.
    History,
    /// Expressions and what they come to on the console as it stands.
    Watches,
    /// The CPU address space as hex bytes, where RAM can be typed over.
    Memory,
}

impl Pane {
    /// The id the window's panes and columns are keyed by, which keeps them apart from another
    /// window's.
    const WINDOW: &'static str = "debugger";

    /// The panes a window opens with.
    ///
    /// Everything but the history and the memory. The history answers what has run, where the call
    /// stack answers how execution got here, and it takes a ring buffer plus the bottom of the
    /// window to do it. The memory pane takes the same bottom and a copy of the console's bytes
    /// each frame. Both wait for the View menu to ask.
    pub const DEFAULT: [Self; 6] = [
        Self::Disassembly,
        Self::Registers,
        Self::Stack,
        Self::CallStack,
        Self::Watches,
        Self::Breakpoints,
    ];
}

impl panes::Pane for Pane {
    const ALL: &'static [Self] = &[
        Self::Disassembly,
        Self::Registers,
        Self::Stack,
        Self::CallStack,
        Self::Watches,
        Self::Breakpoints,
        Self::History,
        Self::Memory,
    ];

    fn title(self) -> &'static str {
        match self {
            Self::Disassembly => "Disassembly",
            Self::Registers => "Registers",
            Self::Stack => "Stack",
            Self::CallStack => "Call stack",
            Self::Breakpoints => "Breakpoints",
            Self::History => "Recently executed",
            Self::Watches => "Watches",
            Self::Memory => "Memory",
        }
    }

    fn column(self) -> Column {
        match self {
            Self::Disassembly => Column::Center,
            Self::Registers | Self::Stack | Self::CallStack | Self::Watches | Self::Breakpoints => {
                Column::Right
            }
            // A hex row is sixteen bytes plus their text, which is wider than the right column
            // and about what the bottom one spans.
            Self::History | Self::Memory => Column::Bottom,
        }
    }

    fn default_size(self) -> f32 {
        match self {
            Self::Disassembly => 0.0,
            Self::Registers => 92.0,
            Self::Stack => 160.0,
            Self::CallStack => 120.0,
            Self::Breakpoints => 160.0,
            Self::History => 140.0,
            Self::Watches => 120.0,
            Self::Memory => 180.0,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Disassembly => "pane_disassembly",
            Self::Registers => "pane_registers",
            Self::Stack => "pane_stack",
            Self::CallStack => "pane_call_stack",
            Self::Breakpoints => "pane_breakpoints",
            Self::History => "pane_history",
            Self::Watches => "pane_watches",
            Self::Memory => "pane_memory",
        }
    }
}

/// What a click on a disassembly row asked for.
///
/// Reported rather than applied, since the row draws from state it borrows.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
enum RowAction {
    /// Add a breakpoint on this address in the bank at this arena offset, or remove the one
    /// already on exactly it.
    ToggleBreakpoint(u16, Option<u32>),
    /// Remove the breakpoint this id names.
    RemoveBreakpoint(u32),
    /// Arm or disarm the breakpoint this id names, keeping it listed either way.
    ArmBreakpoint(u32, bool),
    /// Make this the row later commands act on.
    Select(u16),
    /// Open the label editor on this address, filed under this key.
    EditLabel(LabelKey, u16),
    /// Center the disassembly on this address and select it.
    GoTo(u16),
    /// Show this address in the memory pane, opening it if it is closed.
    ViewInMemory(u16),
    /// Watch the byte at this address, opening the watch pane if it is closed.
    AddWatch(u16),
    /// Start the console running from this address.
    MovePc(u16),
    /// Resume until execution reaches this address.
    RunTo(u16),
}

#[derive(Debug)]
#[must_use]
struct State {
    tx: NesEventProxy,
    snapshot: CpuSnapshot,
    address_space: AddressSpace,
    /// Address to center in the disassembly, cleared once its row has been drawn and centered.
    scroll_to: Option<u16>,
    /// Rows the last draw put on screen. A center request for a row outside them needs a coarse
    /// jump first.
    visible_rows: Range<usize>,
    goto: String,
    /// The row later commands act on, by address rather than row index so it survives a capture
    /// that moves the rows under it.
    selected: Option<u16>,
    /// What recording breakpoints have caught, oldest first, capped so a hot address cannot grow
    /// it without limit.
    access_log: Vec<AccessHit>,
    breakpoints: Breakpoints,
    /// What is typed in the breakpoint box, which is not a breakpoint until it is added.
    breakpoint_goto: String,
    /// What the addresses of this ROM have been named and annotated.
    ///
    /// Kept beside the breakpoints because the ROM's session file holds the two together.
    labels: HashMap<LabelKey, AddrLabel>,
    /// The watched expressions as typed, in the order the pane lists them.
    watches: Vec<String>,
    /// What is typed in the watch box, which is not a watch until it is added.
    watch_entry: String,
    /// Which breakpoint the editor window is open on, by [`Breakpoint::id`].
    ///
    /// An id rather than an index, so removing another breakpoint while the editor is open cannot
    /// slide it onto a different one.
    editing: Option<u32>,
    /// What the editor's address box holds, which is not the breakpoint's range until it parses.
    editing_range: String,
    /// Which address the label editor is open on, and where the window is drawing it.
    ///
    /// The address travels with the key so the editor can name what it is editing, which a cart
    /// offset on its own does not say.
    editing_label: Option<(LabelKey, u16)>,
    /// The register whose cell is open as a box, and what has been typed into it.
    ///
    /// One at a time, so clicking a second cell closes the first without writing it.
    register_edit: Option<(Register, String)>,
    /// What is typed in the memory pane's address box, which moves the view once it parses.
    memory_goto: String,
    /// The address to center the memory pane on, cleared once its row has been drawn.
    memory_scroll_to: Option<u16>,
    /// The `(start, len)` the console is asked to copy for the memory pane, as
    /// [`memory_window()`] works it out.
    memory_window: Option<(u16, u16)>,
    /// The byte the memory pane has selected, and the hex digits typed onto it so far.
    memory_edit: Option<(u16, String)>,
    /// Rows the last draw of the memory pane put on screen. The window it asks the console for
    /// covers these, and a selection has to leave them before the view follows it.
    memory_rows: Range<usize>,
    history_lines: u16,
    /// The panes that are open, in [`Pane::ALL`] order.
    panes: Vec<Pane>,
}

/// Width of the breakpoint column left of the disassembly. Wide enough to click at, where one
/// character of the monospace font is not.
const GUTTER_WIDTH: f32 = 16.0;

/// Parse an address as typed into one of the window's address boxes.
///
/// Bare hex, or prefixed with the assembler's `$` or the C-style `0x`, since both turn up in
/// documentation the address is likely to be copied out of.
fn parse_addr(text: &str) -> Option<u16> {
    let text = text.trim();
    let digits = text
        .strip_prefix('$')
        .or_else(|| text.strip_prefix("0x"))
        .or_else(|| text.strip_prefix("0X"))
        .unwrap_or(text);
    u16::from_str_radix(digits, 16).ok()
}

/// Parse an address range as typed into the breakpoint box: one address, or `$lo-$hi`.
///
/// Reversed ends are put back in order rather than refused, since a range typed backwards says
/// plainly enough what was meant.
fn parse_range(text: &str) -> Option<(u16, u16)> {
    match text.trim().split_once('-') {
        Some((start, end)) => {
            let (start, end) = (parse_addr(start)?, parse_addr(end)?);
            Some((start.min(end), start.max(end)))
        }
        None => {
            let addr = parse_addr(text)?;
            Some((addr, addr))
        }
    }
}

/// The expression grammar, laid out in real columns.
///
/// A grid rather than one preformatted block, since the monospace font is a pixel font whose
/// advance widths round differently at scaled sizes, and columns padded with spaces drift.
fn syntax_help(ui: &mut Ui) {
    Grid::new("expression_syntax")
        .num_columns(2)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            for (name, detail) in Expr::SYNTAX {
                ui.label(*name);
                ui.monospace(*detail);
                ui.end_row();
            }
        });
    ui.label(Expr::SYNTAX_NOTE);
}

/// A watched expression's value.
///
/// A comparison reads as true or false. Anything else names a number, at the width the value
/// asks for: a byte reads as two digits and an address as four, so a column of bytes does not
/// pretend to be addresses.
fn watch_value(value: i32, boolean: bool) -> String {
    if boolean {
        return if value == 0 { "false" } else { "true" }.to_string();
    }
    match u16::try_from(value) {
        Ok(value) if value <= 0xFF => format!("${value:02X} {value}"),
        Ok(value) => format!("${value:04X} {value}"),
        Err(_) => value.to_string(),
    }
}

/// The three access letters a breakpoint watches, as the list and the hover write them.
fn access_text(access: Access) -> String {
    [
        (Access::EXEC, 'X'),
        (Access::READ, 'R'),
        (Access::WRITE, 'W'),
    ]
    .into_iter()
    .filter(|(flag, _)| access.contains(*flag))
    .map(|(_, letter)| letter)
    .collect()
}

/// What the gutter draws for the breakpoints covering a row.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum GutterMark {
    /// One breakpoint, drawn in the color of the access it watches.
    One { access: Access, enabled: bool },
    /// Several, drawn as a cross so a single click is never a surprise.
    Several { access: Access, enabled: bool },
}

impl GutterMark {
    /// What to draw for `covering`, or `None` where nothing covers the row.
    fn of(covering: &[&Breakpoint]) -> Option<Self> {
        let (first, rest) = covering.split_first()?;
        // The strongest access any of them watches, since one mark cannot show three. Execution
        // is the strongest: it says the row itself runs, where a read or a write says only that
        // something reaches the byte.
        let access = covering.iter().fold(Access::empty(), |access, breakpoint| {
            access | breakpoint.access
        });
        let enabled = covering.iter().any(|breakpoint| breakpoint.enabled);
        Some(if rest.is_empty() {
            Self::One {
                access: first.access,
                enabled,
            }
        } else {
            Self::Several { access, enabled }
        })
    }

    /// The color the strongest access is drawn in.
    const fn color(access: Access, enabled: bool, palette: &Palette) -> Color32 {
        if !enabled {
            return palette.breakpoint_disabled;
        }
        if access.contains(Access::EXEC) {
            palette.breakpoint_exec
        } else if access.contains(Access::WRITE) {
            palette.breakpoint_write
        } else {
            palette.breakpoint_read
        }
    }

    /// Paint into `gutter`, filled for armed and hollow for listed.
    fn paint(self, painter: &egui::Painter, gutter: Rect, palette: &Palette) {
        let center = gutter.center();
        let radius = GUTTER_WIDTH / 4.0;
        match self {
            Self::One { access, enabled } => {
                let color = Self::color(access, enabled, palette);
                if enabled {
                    painter.circle_filled(center, radius, color);
                } else {
                    painter.circle_stroke(center, radius, egui::Stroke::new(1.5, color));
                }
            }
            Self::Several { access, enabled } => {
                let color = Self::color(access, enabled, palette);
                let stroke = egui::Stroke::new(1.5, color);
                painter.line_segment(
                    [
                        center - Vec2::new(radius, 0.0),
                        center + Vec2::new(radius, 0.0),
                    ],
                    stroke,
                );
                painter.line_segment(
                    [
                        center - Vec2::new(0.0, radius),
                        center + Vec2::new(0.0, radius),
                    ],
                    stroke,
                );
            }
        }
    }
}

/// What the mnemonic stands for, what it does, and how the console runs it.
fn instruction_tooltip(ui: &mut Ui, instr: &InstrRef) {
    ui.strong(format!("{instr} - {}", instr.instr.name()));
    ui.label(instr.instr.describe());
    let affects = instr.instr.affects();
    detail_rows(
        ui,
        "instruction",
        &[
            ("Opcode", format!("${:02X}", instr.opcode)),
            ("Mode", instr.mode_name().to_string()),
            ("Cycles", instr.cycles.to_string()),
            (
                "Flags",
                if affects.is_empty() {
                    "none".to_string()
                } else {
                    FLAGS
                        .iter()
                        .filter(|(flag, _)| affects.contains(*flag))
                        .map(|(_, name)| *name)
                        .collect::<Vec<_>>()
                        .join(" ")
                },
            ),
        ],
    );
}

/// The status flags in the order the register prints them.
const FLAGS: [(Status, &str); 8] = [
    (Status::N, "N"),
    (Status::V, "V"),
    (Status::U, "U"),
    (Status::B, "B"),
    (Status::D, "D"),
    (Status::I, "I"),
    (Status::Z, "Z"),
    (Status::C, "C"),
];

/// A byte's value in the three bases worth reading it in.
fn byte_rows(value: u8) -> Vec<(&'static str, String)> {
    vec![
        ("Hex", format!("${value:02X}")),
        ("Decimal", value.to_string()),
        ("Binary", format!("%{value:08b}")),
    ]
}

/// Lay a hover out as one label and value per line, so several readings of the same thing can be
/// compared down the column rather than picked out of a sentence.
fn detail_rows(ui: &mut Ui, id: &str, rows: &[(&str, String)]) {
    Grid::new(("hover", id))
        .num_columns(2)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            for (label, value) in rows {
                ui.label(*label);
                ui.monospace(value);
                ui.end_row();
            }
        });
}

/// A register cell, where the name and the value both hover with `rows`.
/// How a register's value is written, in the box and out of it.
fn register_text(value: u16, digits: usize) -> String {
    format!("{value:0digits$X}")
}

/// A CPU register the Registers pane draws, and writes when one is typed over.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
enum Register {
    /// The program counter.
    Pc,
    /// The accumulator.
    Acc,
    /// The X index register.
    X,
    /// The Y index register.
    Y,
    /// The stack pointer.
    Sp,
}

impl Register {
    /// The name the cell is labelled with.
    const fn name(self) -> &'static str {
        match self {
            Self::Pc => "PC",
            Self::Acc => "A",
            Self::X => "X",
            Self::Y => "Y",
            Self::Sp => "SP",
        }
    }

    /// What the cell's hover says the register is.
    const fn heading(self) -> &'static str {
        match self {
            Self::Pc => "Program counter - the instruction about to run",
            Self::Acc => "Accumulator",
            Self::X => "Index register X",
            Self::Y => "Index register Y",
            Self::Sp => "Stack pointer",
        }
    }

    /// How many hex digits the register is written in.
    const fn digits(self) -> usize {
        match self {
            Self::Pc => 4,
            Self::Acc | Self::X | Self::Y | Self::Sp => 2,
        }
    }

    /// The write that sets the register to `val`. Only PC has a high byte to take.
    const fn write(self, val: u16) -> DebugWrite {
        match self {
            Self::Pc => DebugWrite::Pc(val),
            Self::Acc => DebugWrite::Acc(val as u8),
            Self::X => DebugWrite::X(val as u8),
            Self::Y => DebugWrite::Y(val as u8),
            Self::Sp => DebugWrite::Sp(val as u8),
        }
    }
}

/// A register cell, where the name and the value both hover with `rows`.
///
/// Clicking the value opens a box with what the register reads now, so it can be edited in
/// place, and Enter writes what parses. `edit` names the one register being typed, so opening a
/// second cell closes the first without committing it.
fn register_cell(
    ui: &mut Ui,
    register: Register,
    value: u16,
    rows: &[(&str, String)],
    edit: &mut Option<(Register, String)>,
) -> Option<DebugWrite> {
    let name = register.name();
    let digits = register.digits();
    let hover = |ui: &mut Ui| {
        ui.strong(register.heading());
        detail_rows(ui, name, rows);
    };
    ui.strong(name).on_hover_ui(hover);

    match edit {
        Some((editing, text)) if *editing == register => {
            let response = ui.add(
                egui::TextEdit::singleline(text)
                    .font(egui::TextStyle::Monospace)
                    // Room for the digits plus the longest sigil `parse_addr` takes.
                    .char_limit(digits + 2)
                    // No taller than the label it stands in for. A row that grows on the click
                    // pushes the grid past the pane and scrolls the top of it off.
                    .margin(egui::Margin::symmetric(2, 0))
                    .desired_width(60.0),
            );
            // Focused on the frame the box opens, and never again, so clicking away can take it.
            if !response.has_focus() && !response.lost_focus() {
                response.request_focus();
            }
            if !submitted(ui, &response) {
                // Focus lost to anything but Enter drops what was typed, the way Escape does.
                if response.lost_focus() {
                    *edit = None;
                }
                return None;
            }
            let written = parse_addr(text).map(|val| register.write(val));
            *edit = None;
            written
        }
        _ => {
            let text = format!("${}", register_text(value, digits));
            if ui
                .add(egui::Label::new(RichText::new(text).monospace()).sense(Sense::click()))
                .on_hover_ui(hover)
                .on_hover_text("Click to write it")
                .clicked()
            {
                *edit = Some((register, register_text(value, digits)));
            }
            None
        }
    }
}

/// Where `addr` sits in the cart arena under `pages`, the mapping the window is drawing.
fn prg_offset(pages: &[Page; PRG_PAGES], addr: u16) -> Option<u32> {
    Memory::offset_in(pages, addr).map(|offset| offset as u32)
}

/// The `(start, len)` the console is asked to copy to cover `rows` of the memory pane.
///
/// Widened by [`State::MEMORY_MARGIN`] and rounded out to whole pages, so a scroll of a row or two
/// reuses the bytes already in hand rather than asking again. Clamped to the address space at
/// both ends, since a length of 64K has no `u16` to say it in.
fn memory_window(rows: &Range<usize>) -> (u16, u16) {
    let start = (rows.start * State::MEMORY_ROW_BYTES).saturating_sub(State::MEMORY_MARGIN) & !0xFF;
    let end = (rows.end * State::MEMORY_ROW_BYTES + State::MEMORY_MARGIN)
        .next_multiple_of(0x100)
        .min(0x1_0000);
    (start as u16, (end - start).min(0xFF00) as u16)
}

/// Whether a byte at `addr` can be typed over, under `pages`, the mapping the window is drawing.
///
/// The rule [`Bus::poke`] applies, resolved from the page table the window has rather than the one
/// the console has moved on to, so a cell greys out rather than taking a write that is refused.
const fn is_writable(pages: &[Page; PRG_PAGES], addr: u16) -> bool {
    match addr {
        0x0000..=0x1FFF => true,
        0x4100..=0xFFFF => Memory::writable_in(pages, addr),
        _ => false,
    }
}

/// Whether the row starting at `base` covers `addr`.
///
/// Measured as a distance rather than as a range, since the last row starts at `$FFF0` and a range
/// to one past its end is empty.
const fn memory_row_holds(base: u16, addr: u16) -> bool {
    (addr.wrapping_sub(base) as usize) < State::MEMORY_ROW_BYTES
}

/// The address a formatted operand names, and the span of text writing it.
///
/// Only a four digit address counts. A zero page operand is two digits, and taking those would
/// put a name over `$10` in every row that touches zero page.
fn operand_address(operand: &str) -> Option<(Range<usize>, u16)> {
    let start = operand.find('$')?;
    let digits = operand.get(start + 1..start + 5)?;
    if !digits.chars().all(|digit| digit.is_ascii_hexdigit()) {
        return None;
    }
    let addr = u16::from_str_radix(digits, 16).ok()?;
    Some((start..start + 5, addr))
}

/// Whether the bytes `breakpoint` was set over are still mapped where it was set.
///
/// A breakpoint keyed by address alone is always in place, since no bank switch moves work RAM or
/// the registers.
fn is_mapped(pages: &[Page; PRG_PAGES], breakpoint: &Breakpoint) -> bool {
    breakpoint.offset.is_none() || breakpoint.offset == prg_offset(pages, breakpoint.addr)
}

/// Whether the box that was just drawn was submitted with Enter.
fn submitted(ui: &Ui, response: &egui::Response) -> bool {
    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
}

/// Whether the row covering `addr` falls inside `visible`, the rows the last draw put on screen.
///
/// False for an empty `visible`, so a window that has yet to draw centers on its first snapshot.
fn row_is_visible(address_space: &AddressSpace, visible: &Range<usize>, addr: u16) -> bool {
    address_space
        .row_at(addr)
        .is_some_and(|row| visible.contains(&row))
}

/// What the console is asked to capture, folded over the panes in `open`.
///
/// A closed pane draws nothing, so what feeds it is not captured either.
fn request(open: &[Pane], history_lines: u16, memory: Option<(u16, u16)>) -> DebugRequest {
    DebugRequest {
        history_lines: if open.contains(&Pane::History) {
            history_lines
        } else {
            0
        },
        stack: open.contains(&Pane::Stack),
        call_stack: open.contains(&Pane::CallStack),
        memory: open.contains(&Pane::Memory).then_some(memory).flatten(),
    }
}

#[derive(Debug)]
#[must_use]
pub struct CpuDebugger {
    pub id: ViewportId,
    open: Arc<AtomicBool>,
    state: Arc<Mutex<State>>,
}

impl CpuDebugger {
    const TITLE: &'static str = "TetaNES - Debugger";
    /// Enough context to see how the current instruction was reached without pushing it off-screen.
    const HISTORY_LINES: u16 = 8;

    /// Create a debugger with `panes` open, as saved in config by the last run.
    pub fn new(tx: NesEventProxy, panes: &[Pane]) -> Self {
        // The center pane cannot be closed, so a config that lost it gets it back rather than an
        // empty center with the columns still drawn.
        let mut panes = Pane::ALL
            .iter()
            .copied()
            .filter(|pane| panes.contains(pane))
            .collect::<Vec<_>>();
        if !panes.contains(&Pane::Disassembly) {
            panes.insert(0, Pane::Disassembly);
        }
        Self {
            id: ViewportId::from_hash_of(Self::TITLE),
            open: Arc::new(AtomicBool::new(false)),
            state: Arc::new(Mutex::new(State {
                tx,
                snapshot: CpuSnapshot::default(),
                address_space: AddressSpace::default(),
                scroll_to: Some(0),
                visible_rows: 0..0,
                goto: String::new(),
                selected: None,
                access_log: Vec::new(),
                breakpoints: Breakpoints::default(),
                breakpoint_goto: String::new(),
                labels: HashMap::new(),
                watches: Vec::new(),
                watch_entry: String::new(),
                editing: None,
                editing_range: String::new(),
                editing_label: None,
                register_edit: None,
                memory_goto: String::new(),
                memory_scroll_to: None,
                memory_window: None,
                memory_edit: None,
                memory_rows: 0..0,
                history_lines: Self::HISTORY_LINES,
                panes,
            })),
        }
    }

    pub const fn id(&self) -> ViewportId {
        self.id
    }

    pub fn open(&self) -> bool {
        self.open.load(Ordering::Acquire)
    }

    pub fn set_open(&self, open: bool, ctx: &Context) {
        self.open.store(open, Ordering::Release);
        self.state.lock().subscribe(open);
        if !open {
            ctx.send_viewport_cmd_to(self.id, egui::ViewportCommand::Close);
        }
    }

    pub fn toggle_open(&self, ctx: &Context) {
        let Ok(was_open) = self
            .open
            .try_update(Ordering::Release, Ordering::Acquire, |open| Some(!open))
        else {
            return;
        };
        self.state.lock().subscribe(!was_open);
        if was_open {
            ctx.send_viewport_cmd_to(self.id, egui::ViewportCommand::Close);
        }
    }

    /// Take a new CPU snapshot, centering the disassembly on PC once it leaves the view.
    pub fn update_snapshot(&mut self, mut snapshot: CpuSnapshot) {
        let mut state = self.state.lock();
        // Each snapshot brings only what was caught since the last one, so the list accumulates
        // them and drops the oldest at its cap.
        state.access_log.append(&mut snapshot.access_log);
        let over = state
            .access_log
            .len()
            .saturating_sub(State::ACCESS_LOG_LINES);
        state.access_log.drain(..over);
        // Stepping through a routine walks the highlight down the rows already on screen. Only a
        // jump that lands off screen moves the view, so the lines around PC stay where they were.
        if state.snapshot.cpu.pc != snapshot.cpu.pc && !state.pc_is_visible(snapshot.cpu.pc) {
            state.scroll_to = Some(snapshot.cpu.pc);
        }
        state.snapshot = snapshot;
    }

    /// Center the disassembly on PC, whatever the view was showing.
    ///
    /// A stop is the one time the view has to move: the console jumped somewhere the user did not
    /// scroll to, and the row indices it was following belong to an address space captured before
    /// the jump.
    pub fn center_on_pc(&mut self) {
        let mut state = self.state.lock();
        state.scroll_to = Some(state.snapshot.cpu.pc);
    }

    /// Forget the breakpoints that name the cart being unloaded.
    ///
    /// An `offset` indexes the arena of the cart it was set against, so against the next one it
    /// names an unrelated byte and would stop wherever that byte happens to be mapped. The ones
    /// with no offset cover work RAM and the registers, which every cart shares, so those stay.
    /// Take what the ROM that has just loaded left behind last session.
    pub fn adopt_marks(&mut self, marks: Marks) {
        self.state.lock().adopt_marks(marks);
    }

    pub fn drop_cart_breakpoints(&mut self) {
        let mut state = self.state.lock();
        state.breakpoints.retain_without_cart();
        state.access_log.clear();
        // Arming only: the console clears its own marks as the cart comes out and reads the next
        // one's back in, so writing this list over them would undo that.
        state.arm_breakpoints();
    }

    /// Take a new address space, keeping PC centered for a view that was following it.
    pub fn update_address_space(&mut self, address_space: AddressSpace) {
        let mut state = self.state.lock();
        // A capture arrives whenever the code map marks something new, and rows added above PC
        // move its row index. Re-center so PC stays on the same line, but only for a view that
        // had it on screen, since one scrolled elsewhere is reading something else.
        let following = state.pc_is_visible(state.snapshot.cpu.pc);
        state.address_space = address_space;
        if following {
            state.scroll_to = Some(state.snapshot.cpu.pc);
        }
    }

    /// Draw the debugger's viewport, as a window when viewports are embedded.
    pub fn show(&mut self, ui: &mut Ui, opts: ViewportOptions, cfg: Config) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }

        let open = Arc::clone(&self.open);
        let state = Arc::clone(&self.state);

        let mut viewport_builder = egui::ViewportBuilder::default()
            .with_title(Self::TITLE)
            .with_inner_size(Vec2::new(940.0, 720.0));
        if opts.always_on_top {
            viewport_builder = viewport_builder.with_always_on_top();
        }

        ui.show_viewport_deferred(self.id, viewport_builder, move |ui, class| {
            if class == ViewportClass::EmbeddedWindow {
                let was_open = open.load(Ordering::Acquire);
                let mut window_open = was_open;
                egui::Window::new(CpuDebugger::TITLE)
                    .open(&mut window_open)
                    .show(ui, |ui| state.lock().ui(ui, opts.enabled, &cfg));
                open.store(window_open, Ordering::Release);
                // An embedded window raises no viewport close event, so this ✖ is the only word
                // that the Debugger has gone. Unsubscribing disarms the breakpoints and stops the
                // recording, and a console stopped with no window to say why just looks frozen.
                if was_open && !window_open {
                    state.lock().subscribe(false);
                }
            } else {
                CentralPanel::default().show(ui, |ui| state.lock().ui(ui, opts.enabled, &cfg));
                if ui.input(|i| i.viewport().close_requested()) {
                    open.store(false, Ordering::Release);
                }
            }
        });
    }
}

impl State {
    /// Start or stop subscribing to debug events based on the window being open.
    fn subscribe(&self, open: bool) {
        self.tx
            .event(EmulationEvent::DebugSubscribe(open.then(|| self.request())));
        // Closing disarms them, since a console that stopped with nothing to show it would just
        // look frozen. The list is kept here, so opening puts back what was armed.
        if open {
            self.arm_breakpoints();
        }
        self.send_watches();
    }

    /// Say that the list is full, which is the one reason adding a breakpoint does nothing.
    fn warn_breakpoints_full(&self) {
        self.tx.event(UiEvent::Message((
            MessageType::Warn,
            format!("Only {} breakpoints can be enabled.", DeckBreakpoints::MAX),
        )));
    }

    /// Tell the console which addresses to stop at.
    ///
    /// Arming only. The session file is not written from here, so a lifecycle event that re-arms
    /// what is listed cannot overwrite the marks a ROM has just been loaded with.
    fn arm_breakpoints(&self) {
        self.tx
            .event(EmulationEvent::DebugBreakpoints(self.breakpoints.armed()));
    }

    /// Tell the console which addresses to stop at, and what to keep for this ROM.
    ///
    /// For a change the user made. The console is armed with what parses and is enabled, and the
    /// session file takes the list as it stands, so a breakpoint left disarmed or half-written is
    /// still there next session.
    fn send_breakpoints(&self) {
        self.arm_breakpoints();
        self.send_marks();
    }

    /// Tell the console what this ROM's session file should keep.
    fn send_marks(&self) {
        self.tx.event(EmulationEvent::DebugMarks(Box::new(Marks {
            breakpoints: self.breakpoints.iter().cloned().collect(),
            labels: self.labels.clone(),
        })));
    }

    /// Take what the session file kept for the ROM that has just loaded.
    ///
    /// The ids are handed out again as the breakpoints are listed, since they name a breakpoint
    /// for one run of the window and a file cannot know what this run has already used.
    fn adopt_marks(&mut self, marks: Marks) {
        self.breakpoints = Breakpoints::default();
        for breakpoint in marks.breakpoints {
            self.breakpoints.add(breakpoint);
        }
        self.labels = marks.labels;
        // Armed straight away, so a breakpoint left enabled last session stops the console this
        // one without being ticked again. Not sent back: these came from the console.
        self.arm_breakpoints();
    }

    /// How many recorded accesses the breakpoint pane keeps, oldest dropped first.
    const ACCESS_LOG_LINES: usize = 128;

    /// Whether `pane` is drawn.
    fn is_open(&self, pane: Pane) -> bool {
        self.panes.contains(&pane)
    }

    /// Whether `pc`'s row was on screen in the last draw of the disassembly.
    fn pc_is_visible(&self, pc: u16) -> bool {
        row_is_visible(&self.address_space, &self.visible_rows, pc)
    }

    /// Where `addr` sits in the cart arena, as the last snapshot had it mapped.
    ///
    /// `None` for work RAM, the registers and an unmapped page, none of which the arena addresses.
    fn prg_offset(&self, addr: u16) -> Option<u32> {
        prg_offset(&self.snapshot.prg_pages, addr)
    }

    /// Open or close `pane`, and tell config and the console what changed.
    fn set_pane_open(&mut self, pane: Pane, open: bool) {
        // Rebuilt in `Pane::ALL` order, so a reopened pane goes back where it was in its column.
        self.panes = Pane::ALL
            .iter()
            .copied()
            .filter(|other| {
                if *other == pane {
                    open
                } else {
                    self.panes.contains(other)
                }
            })
            .collect();
        self.tx
            .event(ConfigEvent::DebuggerPanes(self.panes.clone()));
        self.resubscribe();
        self.send_watches();
    }

    /// What the console is asked to capture for the panes that are open.
    fn request(&self) -> DebugRequest {
        request(&self.panes, self.history_lines, self.memory_window)
    }

    /// Ask the console for what the panes now want.
    ///
    /// A subscribe answers with a snapshot of its own, so a memory pane scrolled while the console
    /// is stopped fills from that rather than waiting for a frame that never comes.
    fn resubscribe(&self) {
        self.tx
            .event(EmulationEvent::DebugSubscribe(Some(self.request())));
    }

    fn ui(&mut self, ui: &mut Ui, enabled: bool, cfg: &Config) {
        let mut closed = None;
        ui.add_enabled_ui(enabled, |ui| {
            Panel::top("debugger_toolbar").show(ui, |ui| self.toolbar(ui, cfg));
            // Drawn through a closure rather than by the layout, since a body reads the window's
            // own state. The pane the closure cannot reach is the one it reports closed.
            let open = self.panes.clone();
            closed = panes::columns(
                ui,
                Pane::WINDOW,
                &open,
                &Pane::default_size,
                &mut |ui, pane| self.pane(ui, pane),
            );
        });
        if let Some(pane) = closed {
            self.set_pane_open(pane, false);
        }
        self.breakpoint_editor(ui.ctx());
        self.label_editor(ui.ctx());
    }

    /// The window that edits one breakpoint, open while a row's ✏ names it.
    ///
    /// A window rather than a modal, so the disassembly stays readable beside it while a
    /// condition is written against what is on screen.
    /// The window that names an address and writes a note about it.
    ///
    /// Edits land as they are typed rather than behind an OK, the way the watch and condition
    /// boxes do. An entry emptied back out is dropped, so clearing both fields removes it.
    fn label_editor(&mut self, ctx: &Context) {
        let Some((key, addr)) = self.editing_label else {
            return;
        };
        let mut open = true;
        let mut label = self.labels.get(&key).cloned().unwrap_or_default();
        let before = label.clone();
        egui::Window::new("Edit label")
            .resizable(true)
            .default_width(320.0)
            .open(&mut open)
            .show(ctx, |ui| {
                Grid::new("label_editor")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.strong("Address");
                        ui.monospace(match key {
                            LabelKey::Cart(offset) => format!("${addr:04X}  cart ${offset:06X}"),
                            LabelKey::Cpu(addr) => format!("${addr:04X}"),
                        })
                        .on_hover_text(match key {
                            LabelKey::Cart(_) => {
                                "Filed by cart offset, so the name follows these bytes when the \
                                 board switches banks"
                            }
                            LabelKey::Cpu(_) => {
                                "Filed by address, which is what work RAM and the registers have"
                            }
                        });
                        ui.end_row();

                        ui.strong("Name");
                        ui.add(
                            egui::TextEdit::singleline(&mut label.name)
                                .hint_text("what to call it")
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();

                        ui.strong("Comment");
                        ui.add(
                            egui::TextEdit::multiline(&mut label.comment)
                                .hint_text("what to remember about it")
                                .desired_rows(3)
                                .desired_width(f32::INFINITY),
                        );
                        ui.end_row();
                    });
                ui.separator();
                ui.weak(
                    "The name replaces the address wherever an operand reaches it, and heads the \
                     row it names. The comment follows the instruction.",
                );
            });

        if label != before {
            if label.is_empty() {
                self.labels.remove(&key);
            } else {
                self.labels.insert(key, label);
            }
            self.send_marks();
        }
        if !open {
            self.editing_label = None;
        }
    }

    fn breakpoint_editor(&mut self, ctx: &Context) {
        let Some(id) = self.editing else {
            return;
        };
        // Whatever the editor named has been removed, so there is nothing left to edit.
        if self.breakpoints.get(id).is_none() {
            self.editing = None;
            return;
        }

        let mut open = true;
        let mut armed_changed = false;
        let mut removed = false;
        let mut range = std::mem::take(&mut self.editing_range);
        let mut moved = false;
        // Copied out because the closure borrows the breakpoint mutably and still has to resolve
        // an address.
        let pages = self.snapshot.prg_pages;
        let response = egui::Window::new("Edit breakpoint")
            // Resizable, and wide enough by default that the syntax below reads without
            // wrapping, since a condition can run well past one line.
            .resizable(true)
            .default_width(400.0)
            .open(&mut open)
            .show(ctx, |ui| {
                let Some(breakpoint) = self.breakpoints.get_mut(id) else {
                    return;
                };
                Grid::new("breakpoint_editor")
                    .num_columns(2)
                    .spacing([12.0, 6.0])
                    .show(ui, |ui| {
                        ui.strong("Address or range");
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut range)
                                .hint_text("$addr or $lo-$hi")
                                .desired_width(160.0),
                        );
                        // Applied as it is typed, and left alone while it is not a range, so
                        // clearing the box to retype it does not move the breakpoint to $0000.
                        if response.changed()
                            && let Some((addr, end)) = parse_range(&range)
                        {
                            breakpoint.addr = addr;
                            breakpoint.end = end;
                            // Resolved from the address just typed. Taking it from the one the
                            // box held when the frame began pins the breakpoint to the bank it
                            // has left, where nothing can ever match it.
                            breakpoint.offset = prg_offset(&pages, addr);
                            moved = true;
                            armed_changed = true;
                        }
                        ui.end_row();

                        ui.strong("Break on");
                        ui.horizontal(|ui| {
                            for (access, letter, hover) in [
                                (Access::EXEC, "X", "Execution"),
                                (Access::READ, "R", "Reads"),
                                (Access::WRITE, "W", "Writes"),
                            ] {
                                let mut on = breakpoint.access.contains(access);
                                if ui
                                    .toggle_value(&mut on, letter)
                                    .on_hover_text(hover)
                                    .changed()
                                {
                                    breakpoint.access.set(access, on);
                                    armed_changed = true;
                                }
                            }
                        });
                        ui.end_row();

                        ui.strong("Condition");
                        ui.vertical(|ui| {
                            let width = ui.available_width();
                            let response = ui.add(
                                egui::TextEdit::multiline(&mut breakpoint.condition)
                                    .font(egui::TextStyle::Monospace)
                                    .hint_text("a == 0xFF && mem[0x300] != 0")
                                    .desired_width(width)
                                    .desired_rows(3),
                            );
                            armed_changed |= response.changed();
                            // Reported under the box rather than on a hover: there is room here,
                            // and a condition that does not parse leaves the breakpoint unarmed.
                            if let Some(Err(error)) = breakpoint.condition() {
                                ui.label(
                                    RichText::new(error.to_string()).small().color(Color32::RED),
                                );
                            }
                        });
                        ui.end_row();

                        ui.strong("Then");
                        ui.vertical(|ui| {
                            let mut logs = !breakpoint.breaks;
                            if ui
                                .checkbox(&mut logs, "Log and keep running")
                                .on_hover_text("Cleared, the console stops instead")
                                .changed()
                            {
                                breakpoint.breaks = !logs;
                                armed_changed = true;
                            }
                            armed_changed |=
                                ui.checkbox(&mut breakpoint.enabled, "Enabled").changed();
                        });
                        ui.end_row();
                    });

                ui.collapsing("Expression syntax", syntax_help);

                ui.separator();
                ui.horizontal(|ui| {
                    removed = ui.button("Delete").clicked();
                });
            });

        self.editing_range = range;
        if moved {
            // `add` inserts in address order and an edit moves one out from under that.
            self.breakpoints.sort();
        }
        if let Some(response) = &response {
            // Focused and on top the way the keybind window is, since it is opened by a click on
            // the row behind it.
            ctx.move_to_top(response.response.layer_id);
        }
        if removed {
            self.breakpoints.remove(id);
            armed_changed = true;
        }
        if removed || !open {
            self.editing = None;
        }
        if armed_changed {
            self.send_breakpoints();
        }
    }

    /// The step buttons and the View menu.
    fn toolbar(&mut self, ui: &mut Ui, cfg: &Config) {
        ui.horizontal(|ui| {
            for (step, label, hover) in [
                (DebugStep::Into, "➡", "Step a single CPU instruction."),
                (DebugStep::Out, "⬆", "Step out of the current CPU function."),
                (DebugStep::Over, "⮫", "Step over the next CPU instruction."),
                (DebugStep::Scanline, "➖", "Step an entire PPU scanline."),
                (DebugStep::Frame, "🖼", "Step an entire PPU frame."),
            ] {
                let shortcut = cfg.shortcut(Debug::Step(step));
                let button = egui::Button::new(label);
                if ui
                    .add(button)
                    .on_hover_text(format!("{hover} ({shortcut})"))
                    .clicked()
                {
                    self.tx.event(EmulationEvent::DebugStep(step));
                }
            }
            ui.separator();
            for (interrupt, run_to, label, hover) in [
                (
                    DebugInterrupt::Nmi,
                    RunTo::Nmi,
                    "NMI",
                    "Resume until the console takes the NMI vector.",
                ),
                (
                    DebugInterrupt::Irq,
                    RunTo::Irq,
                    "IRQ",
                    "Resume until the console takes the IRQ vector. A `BRK` reaches it by \
                     executing, so break on one with an execute breakpoint.",
                ),
            ] {
                let shortcut = cfg.shortcut(Debug::RunTo(interrupt));
                if ui
                    .add(egui::Button::new(label))
                    .on_hover_text(format!("{hover} ({shortcut})"))
                    .clicked()
                {
                    self.tx.event(EmulationEvent::DebugRunTo(Some(run_to)));
                }
            }
            ui.separator();
            if let Some((pane, open)) = panes::view_menu(ui, Pane::WINDOW, &self.panes) {
                self.set_pane_open(pane, open);
            }
        });
    }

    /// Draw `pane`'s view. The layout draws the heading above it.
    fn pane(&mut self, ui: &mut Ui, pane: Pane) {
        match pane {
            Pane::Disassembly => self.disassembly(ui),
            Pane::Registers => self.registers(ui),
            Pane::Stack => self.stack(ui),
            Pane::CallStack => self.call_stack(ui),
            Pane::Breakpoints => self.breakpoint_list(ui),
            Pane::Watches => self.watch_list(ui),
            // Its own pane rather than inline above PC: the disassembly is ordered by address and
            // this is ordered by time, so the two only coincide in straight-line code.
            Pane::History => self.history(ui),
            Pane::Memory => self.memory(ui),
        }
    }

    fn registers(&mut self, ui: &mut Ui) {
        // Every pane scrolls, so its panel keeps the height its splitter was dragged to.
        ScrollArea::vertical()
            .id_salt("registers")
            .auto_shrink([false, false])
            .show(ui, |ui| self.register_grid(ui));
    }

    /// The registers. Clicking one opens a box that writes it.
    ///
    /// The console applies a write between instructions and answers with a fresh snapshot, so a
    /// cell reads back what actually landed rather than what was typed.
    fn register_grid(&mut self, ui: &mut Ui) {
        // Copied out so the cells can borrow the edit box, which lives beside the snapshot.
        let cpu = self.snapshot.cpu.clone();
        let address_rows = self.address_rows(cpu.pc);
        let stack_rows = self.stack_rows();
        let cycle_rows = [
            ("Cycles", cpu.cycle.to_string()),
            ("Frame", self.snapshot.frame.to_string()),
        ];
        let edit = &mut self.register_edit;
        let mut written = None;
        Grid::new("cpu_registers")
            .num_columns(6)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                let mut cell = |ui: &mut Ui, register, value, rows: &[(&str, String)]| {
                    written = register_cell(ui, register, value, rows, edit).or(written);
                };
                cell(ui, Register::Pc, cpu.pc, &address_rows);
                cell(ui, Register::Acc, cpu.acc.into(), &byte_rows(cpu.acc));
                cell(ui, Register::Sp, cpu.sp.into(), &stack_rows);
                ui.end_row();

                cell(ui, Register::X, cpu.x.into(), &byte_rows(cpu.x));
                cell(ui, Register::Y, cpu.y.into(), &byte_rows(cpu.y));
                // The cycle count reports how far the console has run rather than naming a
                // register, so nothing writes it and it draws as a plain pair.
                let hover = |ui: &mut Ui| {
                    ui.strong("CPU cycles since power on");
                    detail_rows(ui, "Cycle", &cycle_rows);
                };
                ui.strong("Cycle").on_hover_ui(hover);
                ui.monospace(cpu.cycle.to_string()).on_hover_ui(hover);
                ui.end_row();
            });

        ui.horizontal(|ui| {
            ui.strong("P").on_hover_text(
                "Status register - the flags the last instruction to write one left behind.",
            );
            // Uppercase for set, lowercase and dimmed for clear, in NVUBDIZC order.
            for (flag, name, meaning) in [
                (Status::N, 'N', "Negative"),
                (Status::V, 'V', "Overflow"),
                (Status::U, 'U', "Unused - set whenever the status is pushed"),
                (Status::B, 'B', "Break - set by the status PHP and BRK push"),
                (
                    Status::D,
                    'D',
                    "Decimal - the NES CPU's ADC and SBC ignore it",
                ),
                (Status::I, 'I', "Interrupt disable"),
                (Status::Z, 'Z', "Zero"),
                (Status::C, 'C', "Carry"),
            ] {
                let set = cpu.status.contains(flag);
                let text = if set {
                    RichText::new(name).monospace().strong()
                } else {
                    RichText::new(name.to_ascii_lowercase())
                        .monospace()
                        .color(Color32::DARK_GRAY)
                };
                // A flag is one bit, so a click is the whole edit. The other seven come back
                // unchanged, since the write names the register rather than the bit.
                if ui
                    .add(egui::Label::new(text).sense(Sense::click()))
                    .on_hover_text(format!(
                        "{meaning}: {}. Click to {}.",
                        if set { "set" } else { "clear" },
                        if set { "clear" } else { "set" }
                    ))
                    .clicked()
                {
                    written = Some(DebugWrite::Status(cpu.status ^ flag));
                }
            }
        });

        if let Some(write) = written {
            self.tx.event(EmulationEvent::DebugWrite(write));
        }
    }

    /// An address in both the terms it can be named in: where the CPU sees it, and where the byte
    /// it reaches sits in the cart. Only cart memory has the second.
    fn address_rows(&self, addr: u16) -> Vec<(&'static str, String)> {
        let mut rows = vec![("CPU", format!("${addr:04X}"))];
        if let Some(offset) = self.prg_offset(addr) {
            rows.push(("Cart", format!("${offset:06X}")));
        }
        rows
    }

    /// What the stack pointer says about the stack, for its hover.
    ///
    /// SP names the next free slot, so what a pull would take sits one above it. With the Stack
    /// pane closed nothing has been captured to name, so only the push address is listed.
    fn stack_rows(&self) -> Vec<(&'static str, String)> {
        let cpu = &self.snapshot.cpu;
        let pull = cpu.sp.wrapping_add(1);
        let mut rows = vec![(
            "Next push",
            format!("${:04X}", Cpu::SP_BASE | u16::from(cpu.sp)),
        )];
        if let Some(value) = self.snapshot.stack.get(usize::from(pull)) {
            rows.push((
                "Next pull",
                format!("${:04X} = ${value:02X}", Cpu::SP_BASE | u16::from(pull)),
            ));
        }
        rows
    }

    /// The instructions that ran most recently, oldest first, ending just before PC.
    fn history(&mut self, ui: &mut Ui) {
        // Newest last, so the view follows the end the way a log does. Scrolling up unsticks it
        // until it is dragged back down.
        ScrollArea::vertical()
            .id_salt("history")
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| self.history_rows(ui));
    }

    fn history_rows(&mut self, ui: &mut Ui) {
        if self.snapshot.history.is_empty() {
            ui.weak("Nothing recorded - step or resume to start executing.");
            return;
        }
        // A run of one address collapses to a count. A CPU waiting on NMI spins on a single
        // instruction, and eight identical lines say far less than one line and a repeat count.
        let mut history = self.snapshot.history.iter().peekable();
        while let Some(line) = history.next() {
            let mut repeats = 1;
            while history.peek().is_some_and(|next| next.addr == line.addr) {
                history.next();
                repeats += 1;
            }
            let text = if repeats > 1 {
                format!("{line}  ×{repeats}")
            } else {
                line.to_string()
            };
            ui.label(RichText::new(text).monospace().color(Color32::DARK_GRAY));
        }
    }

    /// How the console reached where it is, innermost call first.
    ///
    /// The frames are recorded as the calls are made, so the pane fills in from wherever it was
    /// opened rather than from the game's first `JSR`.
    fn call_stack(&mut self, ui: &mut Ui) {
        ScrollArea::vertical()
            .id_salt("call_stack")
            .auto_shrink([false, false])
            .show(ui, |ui| self.call_stack_rows(ui));
    }

    fn call_stack_rows(&mut self, ui: &mut Ui) {
        // PC first, so the column reads as one path from where execution is out to whoever
        // started it.
        let pc = self.snapshot.cpu.pc;
        let mut scroll_to = None;
        let executing = Label::new(
            RichText::new(format!("     ${pc:04X}"))
                .monospace()
                .color(Color32::LIGHT_GRAY),
        )
        .sense(Sense::click());
        if ui
            .add(executing)
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Executing here - go to it")
            .clicked()
        {
            scroll_to = Some(pc);
        }

        if self.snapshot.call_stack.is_empty() {
            ui.weak("No call recorded - step or resume to enter one.");
        }
        for frame in self.snapshot.call_stack.iter().rev() {
            let kind = match frame.kind {
                FrameKind::Call => "JSR",
                FrameKind::Nmi => "NMI",
                FrameKind::Irq => "IRQ",
                FrameKind::Brk => "BRK",
            };
            let label = Label::new(
                RichText::new(format!(
                    "{kind}  ${:04X} from ${:04X}",
                    frame.entry, frame.caller
                ))
                .monospace()
                .color(Color32::DARK_GRAY),
            )
            .sense(Sense::click());
            if ui
                .add(label)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text("Go to the call")
                .clicked()
            {
                scroll_to = Some(frame.caller);
            }
        }

        if let Some(addr) = scroll_to {
            self.scroll_to = scroll_to;
            self.selected = Some(addr);
        }
    }

    /// The watched expressions and what each comes to, with the box that adds one.
    ///
    /// Values arrive on the snapshot, so they follow the console: they refresh every frame while
    /// it runs and after every step.
    fn watch_list(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.watch_entry)
                    .hint_text("expression")
                    .desired_width(140.0),
            );
            let add = submitted(ui, &response) | ui.button("Add").clicked();
            if add && !self.watch_entry.trim().is_empty() {
                self.watches.push(std::mem::take(&mut self.watch_entry));
                self.send_watches();
            }
            ui.menu_button("?", syntax_help)
                .response
                .on_hover_text("Expression syntax");
        });

        if self.watches.is_empty() {
            ui.weak("None. Add an expression above, like `a` or `mem16[0xFFFC]`.");
            return;
        }

        let mut changed = false;
        let mut removed = None;
        let values = &self.snapshot.watches;
        let watches = &mut self.watches;
        ScrollArea::vertical()
            .id_salt("watches")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for (index, watch) in watches.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        // The value leads the row, so a column of them reads down the pane rather
                        // than being hunted for past expressions of every length.
                        match Expr::parse(watch.trim()) {
                            Ok(expr) => {
                                // A value arrives a frame after the expression it answers, so a
                                // row that has just been typed has none yet.
                                let text = values.get(index).copied().flatten().map_or_else(
                                    || "…".to_string(),
                                    |value| watch_value(value, expr.is_boolean()),
                                );
                                ui.monospace(text);
                            }
                            Err(error) => {
                                ui.label(RichText::new("error").monospace().color(Color32::RED))
                                    .on_hover_text(error.to_string());
                            }
                        }
                        // Laid right to left so ✖ ends the row the way it ends a breakpoint's,
                        // with the box taking what is left rather than pushing it off.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✖").clicked() {
                                removed = Some(index);
                            }
                            // Sized from what the button left, rather than asked to fill, so a
                            // long expression cannot push it back off the row.
                            let width = ui.available_width();
                            changed |= ui
                                .add(
                                    egui::TextEdit::singleline(watch)
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(width),
                                )
                                .changed();
                        });
                    });
                }
            });
        if let Some(index) = removed {
            self.watches.remove(index);
            changed = true;
        }
        if changed {
            self.send_watches();
        }
    }

    /// Tell the console which expressions to evaluate each snapshot.
    ///
    /// Nothing while the pane is closed, which leaves the console evaluating nothing, the way a
    /// closed pane captures nothing.
    fn send_watches(&self) {
        if !self.is_open(Pane::Watches) {
            self.tx.event(EmulationEvent::DebugWatches(Vec::new()));
            return;
        }
        self.tx.event(EmulationEvent::DebugWatches(
            self.watches
                .iter()
                .map(|watch| Expr::parse(watch.trim()).ok())
                .collect(),
        ));
    }

    /// The breakpoints, and the box that adds one.
    fn breakpoint_list(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.breakpoint_goto)
                    .hint_text("$addr or $lo-$hi")
                    .desired_width(140.0),
            );
            let add = submitted(ui, &response) | ui.button("Add").clicked();
            if let Some((addr, end)) = parse_range(&self.breakpoint_goto)
                && add
            {
                if self.breakpoints.is_full() {
                    self.warn_breakpoints_full();
                } else {
                    self.breakpoint_goto.clear();
                    let offset = self.prg_offset(addr);
                    self.breakpoints.add(Breakpoint::range(addr, end, offset));
                    self.send_breakpoints();
                }
            }
        });

        self.access_log_rows(ui);

        if self.breakpoints.is_empty() {
            ui.weak("None. Add one above, or click the gutter beside an instruction.");
            return;
        }

        // The list borrows itself for the walk, so what each row asks for is applied after it.
        let mut armed_changed = false;
        let mut removed = None;
        let mut scroll_to = None;
        let pages = &self.snapshot.prg_pages;
        let mut editing = None;
        let breakpoints = &mut self.breakpoints;
        ScrollArea::vertical()
            .id_salt("breakpoints")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for breakpoint in breakpoints.iter_mut() {
                    ui.horizontal(|ui| {
                        let enabled = breakpoint.enabled;
                        armed_changed |= ui
                            .checkbox(&mut breakpoint.enabled, "")
                            .on_hover_text(if enabled {
                                "Disable breakpoint"
                            } else {
                                "Enable breakpoint"
                            })
                            .changed();
                        // One letter each, since three of them plus the range have to fit a
                        // column narrower than the disassembly.
                        for (access, letter, hover) in [
                            (Access::EXEC, "X", "Break on execution"),
                            (Access::READ, "R", "Break on read"),
                            (Access::WRITE, "W", "Break on write"),
                        ] {
                            let mut on = breakpoint.access.contains(access);
                            if ui
                                .toggle_value(&mut on, letter)
                                .on_hover_text(hover)
                                .changed()
                            {
                                breakpoint.access.set(access, on);
                                armed_changed = true;
                            }
                        }
                        // A breakpoint whose bank has been switched out cannot fire, and nothing
                        // else on screen would say why, so the range says it here.
                        let mapped = is_mapped(pages, breakpoint);
                        let label =
                            Label::new(RichText::new(breakpoint.range_text()).monospace().color(
                                if mapped {
                                    Color32::LIGHT_RED
                                } else {
                                    Color32::DARK_GRAY
                                },
                            ))
                            .sense(Sense::click());
                        if ui
                            .add(label)
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(if mapped {
                                "Go to location"
                            } else {
                                "Go to location - its bank is not mapped there now"
                            })
                            .clicked()
                        {
                            scroll_to = Some(breakpoint.addr);
                        }
                        // What only the editor sets still shows here, so the collapsed list
                        // says which breakpoints are narrowed and which only log.
                        if !breakpoint.breaks {
                            ui.label(RichText::new("log").small().color(Color32::DARK_GRAY))
                                .on_hover_text("Records the access and keeps running");
                        }
                        match breakpoint.condition() {
                            Some(Ok(expr)) => {
                                ui.label(RichText::new("if").small())
                                    .on_hover_text(expr.source().to_string());
                            }
                            Some(Err(error)) => {
                                ui.label(RichText::new("if").small().color(Color32::LIGHT_RED))
                                    .on_hover_text(error.to_string());
                            }
                            None => (),
                        }
                        if ui
                            .small_button("✏")
                            .on_hover_text("Edit breakpoint")
                            .clicked()
                        {
                            editing = Some(breakpoint.id);
                        }
                        if ui.small_button("✖").clicked() {
                            removed = Some(breakpoint.id);
                        }
                    });
                }
            });
        if let Some(id) = editing {
            // The box starts from the breakpoint's own range, so opening the editor and closing
            // it again changes nothing.
            self.editing_range = self
                .breakpoints
                .get(id)
                .map(Breakpoint::range_text)
                .unwrap_or_default();
            self.editing = Some(id);
        }
        if let Some(id) = removed {
            self.breakpoints.remove(id);
            armed_changed = true;
        }
        if let Some(addr) = scroll_to {
            self.scroll_to = scroll_to;
            // Selected as well as scrolled to, so the row it lands on is marked when it arrives.
            // A block takes the mark by covering the address, which is how an address in a range
            // nothing has decoded still shows where it went.
            self.selected = Some(addr);
        }
        if armed_changed {
            self.send_breakpoints();
        }
    }

    /// What the breakpoints that keep running have caught, newest last.
    fn access_log_rows(&mut self, ui: &mut Ui) {
        if self.access_log.is_empty() {
            return;
        }
        let mut cleared = false;
        let log = &self.access_log;
        // Claims its strip before the list, whose scroll area takes whatever is left.
        Panel::bottom("breakpoint_log")
            .resizable(true)
            .default_size(ui.text_style_height(&egui::TextStyle::Monospace) * 7.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Caught");
                    cleared = ui.small_button("Clear").clicked();
                });
                ScrollArea::vertical()
                    .id_salt("access_log")
                    .auto_shrink([false, false])
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for hit in log {
                            // The instruction first, since "what touches this address" is the
                            // question a recording breakpoint is set to answer.
                            let text = if hit.access.contains(Access::EXEC) {
                                // An execution names one address, and the disassembly already
                                // says what sits there.
                                format!("${:04X}  ran", hit.pc)
                            } else {
                                let verb = if hit.access.contains(Access::WRITE) {
                                    "wrote"
                                } else {
                                    "read"
                                };
                                format!(
                                    "${:04X}  {verb} ${:04X} = ${:02X}",
                                    hit.pc, hit.addr, hit.value
                                )
                            };
                            ui.label(RichText::new(text).monospace());
                        }
                    });
            });
        if cleared {
            self.access_log.clear();
        }
    }

    fn disassembly(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.goto)
                    .hint_text("$addr")
                    .desired_width(90.0),
            );
            let go = submitted(ui, &response) | ui.button("Go").clicked();
            if let Some(addr) = parse_addr(&self.goto)
                && go
            {
                self.scroll_to = Some(addr);
                self.selected = Some(addr);
            }
        });

        if self.address_space.rows.is_empty() {
            ui.weak("No ROM is loaded.");
            return;
        }

        let pc = self.snapshot.cpu.pc;
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        // Rows have to stay exactly one line tall: `show_rows` maps scroll offset to row index by
        // multiplying, so a row that wraps desynchronizes both the virtual window and the jump.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        let pitch = row_height + ui.spacing().item_spacing.y;
        let viewport_height = ui.available_height();
        // Scrolls sideways as well: a row runs to about 44 columns at its longest, and the
        // window is resizable down past that.
        let mut scroll_area = ScrollArea::both()
            .id_salt("disassembly")
            .auto_shrink([false, false]);

        // A row per instruction rather than per address, so the only way to scroll to an address
        // is to look up which row covers it.
        let target = self
            .scroll_to
            .and_then(|addr| self.address_space.row_at(addr));

        // `show_rows` only builds widgets for the rows it believes are visible, so a row that was
        // not drawn cannot center itself. This brings it into the window, and the exact centering
        // happens below.
        if let Some(row) = target.filter(|row| !self.visible_rows.contains(row)) {
            let approx = (row as f32).mul_add(pitch, -(viewport_height - row_height) / 2.0);
            scroll_area = scroll_area.vertical_scroll_offset(approx.max(0.0));
        }

        let palette = Palette::new(ui.visuals());
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let mut drawn = 0..0;
        let mut centered = false;
        let mut act = None;
        scroll_area.show_rows(
            ui,
            row_height,
            self.address_space.rows.len(),
            |ui, range| {
                drawn = range.clone();
                for (offset, row) in self.address_space.rows[range.clone()].iter().enumerate() {
                    let (response, clicked) = self.row(ui, row, pc, &palette, &font, row_height);
                    if clicked.is_some() {
                        act = clicked;
                    }
                    if target == Some(range.start + offset) {
                        // egui measures the drawn row, where the offset arithmetic above can only
                        // estimate it, so this lands the row on the center line.
                        // Unanimated, to match the rest of the window updating in one step.
                        response.scroll_to_me_animation(
                            Some(egui::Align::Center),
                            egui::style::ScrollAnimation::none(),
                        );
                        centered = true;
                    }
                }
            },
        );
        self.visible_rows = drawn;
        if centered {
            self.scroll_to = None;
        }
        match act {
            Some(RowAction::ToggleBreakpoint(addr, offset)) => {
                // A full list refuses the add, so say so. Clicking a gutter and having nothing
                // appear reads as a broken window.
                if self.breakpoints.is_full() && self.breakpoints.single_at(addr, offset).is_none()
                {
                    self.warn_breakpoints_full();
                } else {
                    self.breakpoints.toggle(addr, offset);
                    self.send_breakpoints();
                }
            }
            Some(RowAction::RemoveBreakpoint(id)) => {
                self.breakpoints.remove(id);
                self.send_breakpoints();
            }
            Some(RowAction::ArmBreakpoint(id, enabled)) => {
                if let Some(breakpoint) = self.breakpoints.get_mut(id) {
                    breakpoint.enabled = enabled;
                }
                self.send_breakpoints();
            }
            Some(RowAction::Select(addr)) => self.selected = Some(addr),
            Some(RowAction::EditLabel(key, addr)) => self.editing_label = Some((key, addr)),
            Some(RowAction::GoTo(addr)) => {
                self.scroll_to = Some(addr);
                self.selected = Some(addr);
            }
            Some(RowAction::ViewInMemory(addr)) => {
                if !self.is_open(Pane::Memory) {
                    self.set_pane_open(Pane::Memory, true);
                }
                self.memory_scroll_to = Some(addr);
                self.memory_edit = Some((addr, String::new()));
            }
            Some(RowAction::AddWatch(addr)) => {
                if !self.is_open(Pane::Watches) {
                    self.set_pane_open(Pane::Watches, true);
                }
                self.watches.push(format!("mem[${addr:04X}]"));
                self.send_watches();
            }
            Some(RowAction::MovePc(addr)) => {
                self.tx
                    .event(EmulationEvent::DebugWrite(DebugWrite::Pc(addr)));
            }
            Some(RowAction::RunTo(addr)) => {
                self.tx
                    .event(EmulationEvent::DebugRunTo(Some(RunTo::Address(addr))));
            }
            None => (),
        }
    }

    /// Draw one row, reporting it and what a click on it asked for.
    ///
    /// The row is split by hand rather than laid out with [`Ui::horizontal`], which sizes to its
    /// contents. `show_rows` maps scroll offset to row index by multiplying, so a row a pixel
    /// taller desynchronizes both the virtual window and the jump to an address.
    fn row(
        &self,
        ui: &mut Ui,
        row: &Row,
        pc: u16,
        palette: &Palette,
        font: &egui::FontId,
        row_height: f32,
    ) -> (egui::Response, Option<RowAction>) {
        let (galley, mnemonic_span) = self.row_galley(ui, row, pc, palette, font);
        // At least the viewport's width, so the gutter and the row highlights span it, and at
        // least the text's, so the scroll area learns how far right the disassembly reaches.
        let width = (GUTTER_WIDTH + galley.size().x).max(ui.available_width());
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, row_height), Sense::hover());
        let (gutter, text) = rect.split_left_right_at_x(rect.left() + GUTTER_WIDTH);

        // Only an instruction has one address to arm. A block spans a range, so its gutter is
        // inert and the address box stays the way to break inside it.
        let instruction = match row {
            Row::Instruction(disasm) => Some(disasm),
            Row::Block { .. } | Row::Label { .. } => None,
        };
        let addr = instruction.map(|disasm| disasm.addr);
        // A block holds PC and the selection by covering their address, so a console stopped
        // inside a collapsed range still shows where it is.
        let holds_pc = row.covers(pc);
        let holds_selection = self.selected.is_some_and(|selected| row.covers(selected));

        // Interacted with before it is painted, so hovering the gutter can light it up. A row
        // whose gutter has no breakpoint would otherwise give no sign of being clickable.
        let gutter_response =
            addr.map(|addr| ui.interact(gutter, ui.id().with(("gutter", addr)), Sense::click()));
        let painter = ui.painter();
        painter.rect_filled(
            gutter,
            0.0,
            match &gutter_response {
                Some(response) if response.hovered() => palette.gutter_hovered,
                _ => palette.gutter,
            },
        );

        if holds_pc {
            painter.rect_filled(text, 0.0, palette.pc_background);
        }
        if holds_selection {
            // An outline rather than a fill, so a selected row that PC is also on keeps both
            // marks.
            painter.rect_stroke(text, 0.0, palette.selection, egui::StrokeKind::Inside);
        }
        // Where the mnemonic was laid, measured before the galley is handed to the painter, so
        // its span can be hovered over. The row is painted from `text.left()`.
        let mnemonic = Rect::from_x_y_ranges(
            text.left()
                + galley
                    .pos_from_cursor(CCursor::new(mnemonic_span.start))
                    .left()
                ..=text.left()
                    + galley
                        .pos_from_cursor(CCursor::new(mnemonic_span.end))
                        .left(),
            text.y_range(),
        );
        // Centered on the galley's own height rather than the font's, which is shorter than the
        // row and leaves the line sitting low in it, off center inside the selection outline.
        painter.galley(
            egui::Pos2::new(text.left(), text.center().y - galley.size().y / 2.0),
            galley,
            palette.operand,
        );

        // A name is a heading over the row below it, not a row to act on, so it interacts with
        // nothing. Interacting would also collide with that row: both are keyed by the address.
        if matches!(row, Row::Label { .. }) {
            return (response, None);
        }
        let (Some(addr), Some(disasm)) = (addr, instruction) else {
            let row_response =
                ui.interact(text, ui.id().with(("block", row.addr())), Sense::click());
            let selected = row_response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
                .then(|| RowAction::Select(row.addr()));
            return (response, selected);
        };
        // Every breakpoint covering this row, so a range marks the whole of itself rather than
        // the row it starts on. One set on the same address in another bank belongs to code that
        // is not on screen, which `covering` leaves out.
        let offset = self.prg_offset(addr);
        let covering = self
            .breakpoints
            .covering(addr, &self.snapshot.prg_pages)
            .collect::<Vec<_>>();
        if let Some(mark) = GutterMark::of(&covering) {
            mark.paint(painter, gutter, palette);
        }

        let gutter_response = gutter_response.expect("an instruction row interacts");
        let mut action = gutter_response
            .clicked()
            .then_some(RowAction::ToggleBreakpoint(addr, offset));
        // The click owns the breakpoint on exactly this address. Anything else covering the row
        // is reachable from the menu, so one click never means two things.
        let single = self.breakpoints.single_at(addr, offset);
        let gutter_response = if covering.is_empty() {
            gutter_response.on_hover_text("Add breakpoint")
        } else {
            gutter_response.on_hover_ui(|ui| {
                ui.label(if single.is_some() {
                    "Click to remove the breakpoint on this address"
                } else {
                    "Click to add a breakpoint on this address"
                });
                for breakpoint in &covering {
                    ui.monospace(format!(
                        "{}  {}",
                        breakpoint.range_text(),
                        access_text(breakpoint.access)
                    ));
                }
            })
        };
        gutter_response.context_menu(|ui| {
            for breakpoint in &covering {
                let enabled = breakpoint.enabled;
                ui.menu_button(breakpoint.range_text(), |ui| {
                    if ui
                        .button(if enabled { "Disable" } else { "Enable" })
                        .clicked()
                    {
                        action = Some(RowAction::ArmBreakpoint(breakpoint.id, !enabled));
                        ui.close();
                    }
                    if ui.button("Remove").clicked() {
                        action = Some(RowAction::RemoveBreakpoint(breakpoint.id));
                        ui.close();
                    }
                });
            }
            if single.is_none() && ui.button("Add breakpoint here").clicked() {
                action = Some(RowAction::ToggleBreakpoint(addr, offset));
                ui.close();
            }
        });

        let row_response = ui.interact(text, ui.id().with(("row", addr)), Sense::click());
        if row_response.clicked() {
            action = Some(RowAction::Select(addr));
        }
        // Hung off the row rather than given a widget of its own, so nothing overlays the row and
        // takes the click that selects it.
        let row_response = if row_response
            .hover_pos()
            .is_some_and(|pos| mnemonic.contains(pos))
        {
            row_response.on_hover_ui(|ui| instruction_tooltip(ui, &disasm.instr))
        } else {
            row_response
        };
        // Every address the row names is worth copying, not only the one it starts at: the
        // effective address is where an indexed operand actually landed.
        row_response.context_menu(|ui| {
            let copy = |ui: &mut Ui, label: &str, text: String| {
                if ui.button(label).clicked() {
                    ui.ctx().copy_text(text);
                    ui.close();
                }
            };
            let mut act = |ui: &mut Ui, label: &str, row_action: RowAction| {
                if ui.button(label).clicked() {
                    action = Some(row_action);
                    ui.close();
                }
            };
            act(
                ui,
                "Toggle breakpoint",
                RowAction::ToggleBreakpoint(addr, offset),
            );
            act(ui, "Add to watch", RowAction::AddWatch(addr));
            act(
                ui,
                "Edit label",
                RowAction::EditLabel(LabelKey::new(addr, offset), addr),
            );
            act(ui, "View in memory", RowAction::ViewInMemory(addr));
            ui.separator();
            act(ui, "Run to location", RowAction::RunTo(addr));
            act(ui, "Move program counter here", RowAction::MovePc(addr));
            act(ui, "Go to location", RowAction::GoTo(addr));
            ui.separator();
            copy(ui, "Copy address", format!("${addr:04X}"));
            if let Some(effective) = disasm.effective_text() {
                copy(ui, "Copy effective address", effective);
            }
            if let Some(value) = disasm.value {
                copy(ui, "Copy value", value.to_string());
            }
        });
        (response, action)
    }

    /// Lay a row out as one galley, each part in its own color.
    ///
    /// One galley rather than a painted string per part, so the row is measured once and the
    /// columns stay where the monospace padding puts them.
    fn row_galley(
        &self,
        ui: &Ui,
        row: &Row,
        pc: u16,
        palette: &Palette,
        font: &egui::FontId,
    ) -> (Arc<egui::Galley>, Range<usize>) {
        let mut job = egui::text::LayoutJob::default();
        let disasm = match row {
            Row::Instruction(disasm) => disasm,
            Row::Block { start, end, kind } => {
                job.append(
                    &format!("${start:04X}-${end:04X}  {}", kind.label()),
                    0.0,
                    egui::TextFormat {
                        font_id: font.clone(),
                        color: palette.block,
                        ..Default::default()
                    },
                );
                return (ui.painter().layout_job(job), 0..0);
            }
            // Inset a little from the address column and closed with a colon, which is how an
            // assembler listing writes one, so it reads as a heading over the rows below it
            // without lining up with any of their columns.
            Row::Label { name, .. } => {
                job.append(
                    &format!("  {name}:"),
                    0.0,
                    egui::TextFormat {
                        font_id: font.clone(),
                        color: palette.label,
                        ..Default::default()
                    },
                );
                return (ui.painter().layout_job(job), 0..0);
            }
        };

        // The console is about to run the row at PC, so it reads as one line rather than as a
        // handful of tinted parts.
        let at_pc = disasm.addr == pc;
        // Each part reports the characters it laid down, so the mnemonic's own span can be found
        // again to hover over.
        let mut laid = 0;
        let mut part = |text: String, color: Color32| {
            let start = laid;
            laid += text.chars().count();
            job.append(
                &text,
                0.0,
                egui::TextFormat {
                    font_id: font.clone(),
                    color: if at_pc { palette.pc_text } else { color },
                    ..Default::default()
                },
            );
            start..laid
        };

        let _ = part(format!("${:04X} ", disasm.addr), palette.address);
        let mut bytes = format!("${:02X} ", disasm.instr.opcode);
        let mut columns = 0;
        for byte in disasm.operands() {
            bytes.push_str(&format!("${byte:02X} "));
            columns += 4;
        }
        for _ in columns..Disasm::BYTE_COLUMNS {
            bytes.push(' ');
        }
        let _ = part(bytes, palette.bytes);
        let mnemonic = disasm.instr.to_string();
        let unofficial = mnemonic.starts_with('*');
        let mnemonic_span = part(
            mnemonic,
            if unofficial {
                palette.mnemonic_unofficial
            } else {
                palette.mnemonic
            },
        );
        if !disasm.operand.is_empty() {
            let _ = part(
                format!(" {}", self.named_operand(&disasm.operand)),
                palette.operand,
            );
        }
        // Bracketed and tinted apart from the operand that computed it, since it names a second
        // address the row did not write down.
        if let Some(effective) = disasm.effective_text() {
            let _ = part(format!(" [{effective}]"), palette.effective);
        }
        if let Some(value) = disasm.value {
            let _ = part(format!(" = {value}"), palette.resolved);
        }
        // Last on the line, the way an assembler listing writes one, so the columns before it
        // stay where they are whatever was written.
        if let Some(comment) = self
            .label_at(disasm.addr)
            .map(|label| label.comment.trim())
            .filter(|comment| !comment.is_empty())
        {
            let _ = part(format!("  ;{comment}"), palette.comment);
        }
        (ui.painter().layout_job(job), mnemonic_span)
    }

    /// What was written about `addr`, under the mapping the window is drawing.
    fn label_at(&self, addr: u16) -> Option<&AddrLabel> {
        self.labels.get(&LabelKey::new(addr, self.prg_offset(addr)))
    }

    /// `operand` with the address it names replaced by that address's name.
    ///
    /// Rewritten as text because the operand is already formatted, so `$D094,X` becomes
    /// `handler,X` and the addressing mode's punctuation stays where the disassembler put it.
    fn named_operand(&self, operand: &str) -> String {
        let Some((span, addr)) = operand_address(operand) else {
            return operand.to_string();
        };
        match self.label_at(addr).map(|label| label.name.trim()) {
            Some(name) if !name.is_empty() => {
                format!("{}{name}{}", &operand[..span.start], &operand[span.end..])
            }
            _ => operand.to_string(),
        }
    }

    /// Bytes on one row of the memory pane.
    const MEMORY_ROW_BYTES: usize = 16;

    /// Rows the memory pane covers, which is the whole CPU address space.
    const MEMORY_ROWS: usize = 0x1_0000 / Self::MEMORY_ROW_BYTES;

    /// How far either side of the rows on screen the console is asked to copy.
    const MEMORY_MARGIN: usize = 0x200;

    /// The CPU address space as hex bytes, sixteen to a row.
    ///
    /// The console copies a window around the rows on screen rather than the whole 64K, so a byte
    /// scrolled to draws as `--` until the snapshot answering the new window arrives.
    fn memory(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.memory_goto)
                    .hint_text("$addr")
                    .desired_width(90.0),
            );
            let go = submitted(ui, &response) | ui.button("Go").clicked();
            if let Some(addr) = parse_addr(&self.memory_goto)
                && go
            {
                self.memory_scroll_to = Some(addr);
                self.memory_edit = Some((addr, String::new()));
            }
            if ui
                .button("PC")
                .on_hover_text("Go to the instruction about to run")
                .clicked()
            {
                let pc = self.snapshot.cpu.pc;
                self.memory_scroll_to = Some(pc);
                self.memory_edit = Some((pc, String::new()));
            }
            ui.weak(match self.memory_edit {
                Some((addr, _)) => {
                    format!("${addr:04X} selected - type two hex digits to write it.")
                }
                None => "Click a byte to select it.".to_string(),
            });
        });

        // Typed straight onto the selected byte rather than into a box of its own, which would
        // make one row taller than the rest and desynchronize the virtual window below. Skipped
        // while a text box has focus, since the same keys are meant to land there.
        if ui.memory(|memory| memory.focused().is_none()) {
            self.type_memory(ui);
        }

        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        let pitch = row_height + ui.spacing().item_spacing.y;
        let viewport_height = ui.available_height();
        // Scrolls sideways too, since a row runs to about 74 columns and the pane is resizable
        // down past that.
        let mut scroll_area = ScrollArea::both()
            .id_salt("memory")
            .auto_shrink([false, false]);
        // Every row is the same height and covers the same count of bytes, so the offset a row
        // sits at is exact and one step lands it on the center line.
        if let Some(addr) = self.memory_scroll_to.take() {
            let row = usize::from(addr) / Self::MEMORY_ROW_BYTES;
            let offset = (row as f32).mul_add(pitch, -(viewport_height - row_height) / 2.0);
            scroll_area = scroll_area.vertical_scroll_offset(offset.max(0.0));
        }

        let palette = Palette::new(ui.visuals());
        let font = egui::TextStyle::Monospace.resolve(ui.style());
        let mut drawn = 0..0;
        let mut clicked = None;
        scroll_area.show_rows(ui, row_height, Self::MEMORY_ROWS, |ui, range| {
            drawn = range.clone();
            for row in range {
                let base = (row * Self::MEMORY_ROW_BYTES) as u16;
                let galley = self.memory_galley(ui, base, &palette, &font);
                let width = galley.size().x.max(ui.available_width());
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(width, row_height), Sense::hover());
                // Centered on the galley's own height rather than the font's, which is shorter
                // than the row and would leave the bytes sitting low inside the selection box.
                let origin = egui::Pos2::new(rect.left(), rect.center().y - galley.size().y / 2.0);
                // Outlined rather than filled, so the digits already typed into it stay readable.
                // Measured off the galley rather than off a character width, so the box sits on
                // the two digits whatever the font does with them.
                if let Some((addr, _)) = self.memory_edit
                    && memory_row_holds(base, addr)
                {
                    let column = Self::memory_column(usize::from(addr) % Self::MEMORY_ROW_BYTES);
                    let left = galley.pos_from_cursor(CCursor::new(column)).left();
                    let right = galley.pos_from_cursor(CCursor::new(column + 2)).left();
                    let cell =
                        Rect::from_x_y_ranges(origin.x + left..=origin.x + right, rect.y_range());
                    ui.painter().rect_stroke(
                        cell.expand(1.0),
                        2.0,
                        palette.selection,
                        egui::StrokeKind::Inside,
                    );
                }
                let response = ui.interact(rect, ui.id().with(("memory", base)), Sense::click());
                if let Some(pos) = response
                    .interact_pointer_pos()
                    .filter(|_| response.clicked())
                {
                    let column = galley.cursor_from_pos(pos - origin).index.into();
                    clicked = Self::memory_index(column).map(|index| base + index as u16);
                }
                ui.painter().galley(origin, galley, palette.memory_writable);
            }
        });
        self.memory_rows = drawn;
        if let Some(addr) = clicked {
            self.memory_edit = Some((addr, String::new()));
        }
        self.follow_memory_rows();
    }

    /// Where the hex digits of the `index`th byte of a row start, in characters across it.
    ///
    /// Past the address and its two spaces, then two digits and a space each, with the gap after
    /// the eighth widened so a byte can be counted off in eights.
    const fn memory_column(index: usize) -> usize {
        7 + index * 3 + if index >= 8 { 1 } else { 0 }
    }

    /// Which byte of a row the `column`th character falls on, or `None` between two of them.
    fn memory_index(column: usize) -> Option<usize> {
        (0..Self::MEMORY_ROW_BYTES).find(|index| {
            (Self::memory_column(*index)..Self::memory_column(*index) + 2).contains(&column)
        })
    }

    /// Ask the console for a window covering the rows the last draw put on screen.
    fn follow_memory_rows(&mut self) {
        let window = memory_window(&self.memory_rows);
        if self.memory_window != Some(window) {
            self.memory_window = Some(window);
            self.resubscribe();
        }
    }

    /// Take what was typed onto the selected byte.
    ///
    /// Two hex digits write it and step on to the next, the arrows move the selection, and Escape
    /// drops it. The view follows the selection only on the frame it moves, and only off screen,
    /// the way the disassembly follows PC. Following it every frame pulls the pane back as soon
    /// as a scroll carries the selection off screen.
    fn type_memory(&mut self, ui: &Ui) {
        if self.memory_edit.is_none() {
            return;
        }
        let (typed, escape, step) = ui.input(|input| {
            let typed = input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect::<String>();
            let row = Self::MEMORY_ROW_BYTES as i16;
            let step = i16::from(input.key_pressed(egui::Key::ArrowRight))
                - i16::from(input.key_pressed(egui::Key::ArrowLeft))
                + row * i16::from(input.key_pressed(egui::Key::ArrowDown))
                - row * i16::from(input.key_pressed(egui::Key::ArrowUp));
            (typed, input.key_pressed(egui::Key::Escape), step)
        });
        if escape {
            self.memory_edit = None;
            return;
        }

        let mut written = Vec::new();
        let Some((addr, digits)) = self.memory_edit.as_mut() else {
            return;
        };
        let mut moved = step != 0;
        if step != 0 {
            *addr = addr.wrapping_add_signed(step);
            digits.clear();
        }
        for digit in typed.chars().filter(char::is_ascii_hexdigit) {
            digits.push(digit);
            if digits.len() < 2 {
                continue;
            }
            let val = u8::from_str_radix(digits, 16).expect("two hex digits");
            written.push(DebugWrite::Memory { addr: *addr, val });
            *addr = addr.wrapping_add(1);
            digits.clear();
            moved = true;
        }

        let addr = *addr;
        for write in written {
            self.tx.event(EmulationEvent::DebugWrite(write));
        }
        if moved
            && !self
                .memory_rows
                .contains(&(usize::from(addr) / Self::MEMORY_ROW_BYTES))
        {
            self.memory_scroll_to = Some(addr);
        }
    }

    /// One row of the memory pane laid out: the address, sixteen bytes, then the same as text.
    ///
    /// A byte the console has not copied draws as `--`, and one nothing can write greys.
    fn memory_galley(
        &self,
        ui: &Ui,
        base: u16,
        palette: &Palette,
        font: &egui::FontId,
    ) -> Arc<egui::Galley> {
        let mut job = egui::text::LayoutJob::default();
        let mut part = |text: &str, color: Color32| {
            job.append(
                text,
                0.0,
                egui::TextFormat {
                    font_id: font.clone(),
                    color,
                    ..Default::default()
                },
            );
        };
        part(&format!("${base:04X}  "), palette.address);
        let mut text = String::with_capacity(Self::MEMORY_ROW_BYTES);
        for index in 0..Self::MEMORY_ROW_BYTES {
            let addr = base.wrapping_add(index as u16);
            let byte = self.memory_byte(addr);
            part(
                &byte.map_or_else(|| "--".to_string(), |byte| format!("{byte:02X}")),
                match byte {
                    None => palette.bytes,
                    Some(_) if is_writable(&self.snapshot.prg_pages, addr) => {
                        palette.memory_writable
                    }
                    Some(_) => palette.memory_readonly,
                },
            );
            part(if index == 7 { "  " } else { " " }, palette.address);
            text.push(match byte {
                Some(byte) if byte.is_ascii_graphic() || byte == b' ' => byte as char,
                _ => '.',
            });
        }
        part(&format!(" {text}"), palette.resolved);
        ui.painter().layout_job(job)
    }

    /// What the last snapshot had at `addr`, or `None` where the window it answered did not reach.
    fn memory_byte(&self, addr: u16) -> Option<u8> {
        let offset = addr.wrapping_sub(self.snapshot.memory_start);
        self.snapshot.memory.get(usize::from(offset)).copied()
    }

    fn stack(&mut self, ui: &mut Ui) {
        ScrollArea::vertical()
            .id_salt("stack")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.snapshot.stack.is_empty() {
                    ui.weak("No ROM is loaded.");
                    return;
                }
                // Top of stack first: SP points at the next free slot, so the most recently pushed
                // byte is at SP + 1 and the walk runs upward from there.
                let sp = self.snapshot.cpu.sp;
                for offset in (u16::from(sp) + 1)..0x0100 {
                    let Some(value) = self.snapshot.stack.get(usize::from(offset)) else {
                        break;
                    };
                    ui.label(RichText::new(format!("$01{offset:02X}  ${value:02X}")).monospace());
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AddrLabel, AddressSpace, BlockKind, Breakpoint, Breakpoints, Column, CpuSnapshot, HashMap,
        LabelKey, PRG_PAGES, Page, Pane, Row, State, is_mapped, memory_row_holds, memory_window,
        operand_address, parse_addr, parse_range, prg_offset, request, row_is_visible, watch_value,
    };
    use crate::nes::renderer::gui::panes::Pane as _;

    /// The addresses of what the console was told to arm, which is all these tests look at.
    fn armed_at(breakpoints: &Breakpoints) -> Vec<u16> {
        breakpoints
            .armed()
            .into_iter()
            .map(|breakpoint| breakpoint.start)
            .collect()
    }
    use tetanes_core::{control_deck::ControlDeck, cpu::Cpu, debug::Access};

    const HISTORY_LINES: u16 = 8;

    /// The column stacks all but one pane at a fixed height and gives the last what is left, so
    /// closing a pane redistributes space inside the column rather than resizing the window.
    #[test]
    fn the_last_open_pane_in_a_column_takes_what_is_left() {
        let (sized, filling) = Column::Right
            .tiling(Pane::ALL)
            .expect("the right column has panes");
        // Read off `Pane::ALL` rather than written out, so this pins the rule and adding a pane
        // does not rewrite it.
        let (last, rest) = Pane::ALL
            .iter()
            .copied()
            .filter(|pane| pane.column() == Column::Right)
            .collect::<Vec<_>>()
            .split_last()
            .map(|(last, rest)| (*last, rest.to_vec()))
            .expect("the right column has panes");
        assert_eq!(sized, rest);
        assert_eq!(filling, last);
    }

    /// A column that reports nothing is not drawn, so it takes no width or height from the rest.
    #[test]
    fn a_column_with_nothing_open_is_not_drawn() {
        assert_eq!(Column::Bottom.tiling(&[Pane::Disassembly]), None);
        assert_eq!(Column::Right.tiling(&[Pane::Disassembly]), None);
    }

    /// [`Column::ALL`] reaches every column, and every column places its panes, so no open pane
    /// goes undrawn.
    #[test]
    fn every_pane_is_laid_out_in_exactly_one_column() {
        let mut placed = Vec::new();
        for column in Column::ALL {
            if let Some((sized, filling)) = column.tiling(Pane::ALL) {
                placed.extend(sized);
                placed.push(filling);
            }
        }
        for pane in Pane::ALL.iter().copied() {
            let times = placed.iter().filter(|other| **other == pane).count();
            assert_eq!(times, 1, "{pane:?} was laid out {times} times");
        }
        assert_eq!(placed.len(), Pane::ALL.len());
    }

    /// Some boards run code out of cart RAM, and a range that only ever reads as `save ram` gives
    /// a console stopped in there no row to sit on. The code map has seen it execute, so the
    /// sweep asks rather than taking everything below `$8000` for data.
    #[test]
    fn cart_ram_that_has_executed_is_disassembled() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("load rom");
        deck.attach_code_map(None);

        // `LDA #$00` in cart RAM, run from there the way a board that copies a routine into RAM
        // reaches it.
        deck.bus_mut().write(0x6000, 0xA9);
        deck.bus_mut().write(0x6001, 0x00);
        deck.bus_mut().cpu.pc = 0x6000;
        deck.bus_mut().clock_instr();

        let space = AddressSpace::capture(deck.bus(), &HashMap::new());
        let row = space.row_at(0x6000).expect("covered");
        assert!(
            matches!(space.rows[row], Row::Instruction(_)),
            "$6000 executed but reads as {:?}",
            space.rows[row]
        );

        // A neighbour nothing has run stays collapsed, so the whole range does not decode blind.
        let untouched = space.row_at(0x7F00).expect("covered");
        assert!(matches!(
            space.rows[untouched],
            Row::Block {
                kind: BlockKind::SaveRam,
                ..
            }
        ));
    }

    /// Stepping walks the highlight down rows already on screen, so only a PC that lands outside
    /// them moves the view. Centering on every step jogs the disassembly once per instruction.
    #[test]
    fn only_a_pc_off_screen_moves_the_disassembly() {
        let deck = ControlDeck::new();
        let address_space = AddressSpace::capture(deck.bus(), &HashMap::new());
        let row = address_space.row_at(0x1234).expect("covered");

        assert!(row_is_visible(&address_space, &(row..row + 4), 0x1234));
        assert!(!row_is_visible(&address_space, &(row + 1..row + 4), 0x1234));
        assert!(
            !row_is_visible(&address_space, &(0..0), 0x1234),
            "a window that has yet to draw has to center on its first snapshot"
        );
    }

    /// A closed pane draws nothing, so the console is asked for nothing on its behalf.
    #[test]
    fn closing_a_pane_drops_what_only_it_draws() {
        let window = Some((0x0200, 0x0400));
        let all = request(Pane::ALL, HISTORY_LINES, window);
        assert_eq!(all.history_lines, HISTORY_LINES);
        assert!(all.stack);
        assert!(all.call_stack);
        assert_eq!(all.memory, window);

        let closed = Pane::ALL
            .iter()
            .copied()
            .filter(|pane| {
                !matches!(
                    pane,
                    Pane::History | Pane::Stack | Pane::CallStack | Pane::Memory
                )
            })
            .collect::<Vec<_>>();
        let request = request(&closed, HISTORY_LINES, window);
        assert_eq!(request.history_lines, 0);
        assert!(!request.stack);
        assert!(!request.call_stack);
        assert_eq!(request.memory, None);
    }

    /// A click lands on the byte it points at. This is the arithmetic tying the hit test to the
    /// row `memory_galley` lays out, and one column out of step puts a neighbor into edit.
    #[test]
    fn every_hex_cell_hit_tests_back_to_its_own_byte() {
        assert_eq!(
            State::memory_column(0),
            7,
            "past the address and the two spaces after it"
        );
        assert_eq!(
            State::memory_column(8),
            32,
            "past the wider gap down the middle"
        );
        for index in 0..State::MEMORY_ROW_BYTES {
            let column = State::memory_column(index);
            assert_eq!(State::memory_index(column), Some(index), "byte {index}");
            assert_eq!(
                State::memory_index(column + 1),
                Some(index),
                "byte {index}, second digit"
            );
            assert_eq!(
                State::memory_index(column + 2),
                None,
                "the gap after byte {index}"
            );
        }
        assert_eq!(State::memory_index(0), None, "the address column");
    }

    /// The window has to cover the rows on screen, or they draw as `--` however long it is looked
    /// at, and it has to stay inside the address space, since a `u16` length cannot say 64K.
    #[test]
    fn the_memory_window_covers_the_rows_on_screen() {
        for rows in [0..12usize, 0..1, 100..140, 4084..4096, 0..4096] {
            let (start, len) = memory_window(&rows);
            let first = rows.start * State::MEMORY_ROW_BYTES;
            let last = rows.end * State::MEMORY_ROW_BYTES;
            assert!(
                usize::from(start) <= first,
                "{rows:?} starts at ${start:04X}"
            );
            assert!(
                usize::from(start) + usize::from(len) >= last || len == 0xFF00,
                "{rows:?} runs to ${:04X}",
                usize::from(start) + usize::from(len)
            );
            assert!(
                usize::from(start) + usize::from(len) <= 0x1_0000,
                "{rows:?} runs past the address space"
            );
        }
    }

    /// A name heads the row it belongs to rather than replacing it, and `row_at` still lands on
    /// the row that owns the address, so a go-to selects the instruction and not the name over it.
    #[test]
    fn a_named_address_gains_a_row_above_the_one_it_names() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("load rom");
        let plain = AddressSpace::capture(deck.bus(), &HashMap::new());
        let pc = deck.bus().cpu.pc;

        let key = LabelKey::new(pc, prg_offset(deck.bus().memory.prg_pages(), pc));
        let labels = HashMap::from([(
            key,
            AddrLabel {
                name: "entry".to_string(),
                comment: String::new(),
            },
        )]);
        let named = AddressSpace::capture(deck.bus(), &labels);

        // At least one: a board that mirrors a bank into two windows puts the same cart bytes at
        // two addresses, and a name filed by offset belongs to both of them.
        assert!(named.rows.len() > plain.rows.len(), "rows were added");
        let row = named.row_at(pc).expect("covered");
        assert!(
            named.rows[row].covers(pc),
            "a go-to lands on the row the name is over, not the name"
        );
        assert!(
            matches!(&named.rows[row - 1], Row::Label { name, .. } if name == "entry"),
            "the name sits directly above it"
        );
    }

    /// A name is put into an operand by rewriting the address it names, so what counts as an
    /// address decides which rows get one. Two digits do not: a name over those would rewrite
    /// `$10` in every row that touches zero page.
    /// The last row starts at `$FFF0`, where one past its end wraps to `$0000`. A range there is
    /// empty, so the byte a click selects would be written with nothing on screen to say so.
    #[test]
    fn the_memory_row_at_the_top_of_the_address_space_still_holds_its_bytes() {
        assert!(
            memory_row_holds(0xFFF0, 0xFFF0),
            "the first of the last row"
        );
        assert!(memory_row_holds(0xFFF0, 0xFFFF), "and the last of it");
        assert!(!memory_row_holds(0xFFF0, 0x0000), "which does not wrap on");
        assert!(!memory_row_holds(0xFFF0, 0xFFEF), "or reach back");
        assert!(memory_row_holds(0x0000, 0x000F));
        assert!(!memory_row_holds(0x0000, 0x0010));
    }

    #[test]
    fn only_a_full_address_in_an_operand_takes_a_name() {
        let found = |operand: &str| {
            operand_address(operand).map(|(span, addr)| (span.start, span.end, addr))
        };
        assert_eq!(found("$D094"), Some((0, 5, 0xD094)));
        assert_eq!(
            found("$D094,X"),
            Some((0, 5, 0xD094)),
            "the index stays outside the span"
        );
        assert_eq!(
            found("($D094),Y"),
            Some((1, 6, 0xD094)),
            "and so do the brackets"
        );
        assert_eq!(found("$94"), None, "a zero page operand");
        assert_eq!(found("#$42"), None, "an immediate");
        assert_eq!(found(""), None, "no operand at all");
    }

    #[test]
    fn an_address_parses_with_or_without_its_sigil() {
        assert_eq!(parse_addr("$C04F"), Some(0xC04F));
        assert_eq!(parse_addr(" c04f "), Some(0xC04F));
        assert_eq!(parse_addr(""), None);
        assert_eq!(parse_addr("$1C04F"), None, "wider than the address bus");
        assert_eq!(parse_addr("$C04G"), None);
    }

    /// Clicking a row in the disassembly is both how a breakpoint is set and how it is cleared.
    #[test]
    fn toggling_an_address_adds_a_breakpoint_and_toggling_it_again_removes_it() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.toggle(0xC000, None);
        assert_eq!(armed_at(&breakpoints), [0xC000]);

        breakpoints.toggle(0xC000, None);
        assert!(breakpoints.is_empty());
    }

    /// The list is drawn in the order it is kept, which is the order the disassembly reads in.
    #[test]
    fn breakpoints_are_held_in_address_order_however_they_are_added() {
        let mut breakpoints = Breakpoints::default();
        for addr in [0xE000, 0x8000, 0xC000] {
            breakpoints.add(Breakpoint::execute(addr, None));
        }
        assert_eq!(armed_at(&breakpoints), [0x8000, 0xC000, 0xE000]);
    }

    /// The box takes one address or a range, and a range typed backwards says plainly enough
    /// what was meant.
    #[test]
    fn a_breakpoint_range_parses_either_way_round() {
        assert_eq!(parse_range("$C000"), Some((0xC000, 0xC000)));
        assert_eq!(parse_range("$C000-$C0FF"), Some((0xC000, 0xC0FF)));
        assert_eq!(parse_range("0xC0FF-0xC000"), Some((0xC000, 0xC0FF)));
        assert_eq!(parse_range("$C000-"), None);
    }

    /// An `offset` indexes one cart's arena, so a breakpoint carrying one means nothing against
    /// the next cart. A breakpoint on work RAM or the registers is as true for one cart as the
    /// next.
    #[test]
    fn a_cart_change_drops_the_breakpoints_that_named_it() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.add(Breakpoint::execute(0x8000, Some(0x4000)));
        breakpoints.add(Breakpoint::execute(0x0300, None));

        breakpoints.retain_without_cart();
        assert_eq!(armed_at(&breakpoints), [0x0300]);
    }

    /// A condition still being typed does not parse, and arming the breakpoint without it would
    /// stop on every access the range covers rather than the few that were asked for.
    #[test]
    fn a_breakpoint_whose_condition_does_not_parse_is_not_armed() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.add(Breakpoint::execute(0xC000, None));
        for breakpoint in breakpoints.iter_mut() {
            breakpoint.condition = "a ==".to_string();
        }
        assert!(breakpoints.armed().is_empty());
        assert!(
            !breakpoints.is_empty(),
            "it stays listed so the rest can be typed"
        );

        for breakpoint in breakpoints.iter_mut() {
            breakpoint.condition = "a == 0x42".to_string();
        }
        assert_eq!(armed_at(&breakpoints), [0xC000]);
    }

    /// A breakpoint with no access ticked stops nothing, so the console is not told about it.
    #[test]
    fn a_breakpoint_with_no_access_selected_is_not_armed() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.add(Breakpoint::execute(0xC000, None));
        for breakpoint in breakpoints.iter_mut() {
            breakpoint.access = Access::empty();
        }
        assert!(breakpoints.armed().is_empty());
        assert!(
            !breakpoints.is_empty(),
            "it stays listed so its ticks can be put back"
        );
    }

    /// The same address in two banks is two instructions, so each keeps a breakpoint of its own
    /// and removing one leaves the other.
    #[test]
    fn an_address_holds_a_breakpoint_for_each_bank() {
        let mut breakpoints = Breakpoints::default();
        let first = breakpoints
            .add(Breakpoint::execute(0x8000, Some(0x4000)))
            .expect("added");
        let second = breakpoints
            .add(Breakpoint::execute(0x8000, Some(0x8000)))
            .expect("added");
        assert_eq!(armed_at(&breakpoints), [0x8000, 0x8000]);

        assert!(breakpoints.remove(first));
        assert_eq!(
            breakpoints.get(second).map(|bp| bp.offset),
            Some(Some(0x8000)),
            "removing one bank's breakpoint took the other bank's with it"
        );
    }

    /// One address holds as many breakpoints as are set on it. A range and a single address are
    /// different questions, and so are two accesses of one byte under different conditions.
    #[test]
    fn one_address_holds_as_many_breakpoints_as_are_set_on_it() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.add(Breakpoint::execute(0x6000, None));
        breakpoints.add(Breakpoint::range(0x6000, 0x7FFF, None));
        assert_eq!(armed_at(&breakpoints), [0x6000, 0x6000]);

        let unmapped = [Page::UNMAPPED; PRG_PAGES];
        assert_eq!(breakpoints.covering(0x6000, &unmapped).count(), 2);
        assert_eq!(
            breakpoints.covering(0x6500, &unmapped).count(),
            1,
            "a range marks every row it covers, not only the one it starts on"
        );
        assert_eq!(breakpoints.covering(0x8000, &unmapped).count(), 0);
    }

    /// A gutter click owns the breakpoint on exactly its address, and a range covering the row
    /// is not it.
    #[test]
    fn a_gutter_toggle_leaves_a_range_covering_the_row_alone() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.add(Breakpoint::range(0x6000, 0x7FFF, None));

        breakpoints.toggle(0x6000, None);
        assert_eq!(armed_at(&breakpoints), [0x6000, 0x6000], "added its own");

        breakpoints.toggle(0x6000, None);
        assert_eq!(armed_at(&breakpoints), [0x6000], "took its own back");
        assert!(
            breakpoints.single_at(0x6000, None).is_none(),
            "the range answered for a single address"
        );
    }

    /// The window keys a breakpoint by the offset it resolves from its own copy of the page
    /// table, and the console checks it against the arena. A copy that disagreed would arm
    /// breakpoints on bytes the console never maps there.
    #[test]
    fn the_window_resolves_the_same_offsets_the_console_does() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("load rom");
        let snapshot = CpuSnapshot::capture(deck.bus(), &request(Pane::ALL, 8, None));

        for addr in [0x0300u16, 0x2000, 0x6000, 0x8000, 0xC000, 0xFFFF] {
            assert_eq!(
                prg_offset(&snapshot.prg_pages, addr),
                deck.bus()
                    .memory
                    .prg_offset(addr)
                    .map(|offset| u32::try_from(offset).expect("fits the arena")),
                "${addr:04X}"
            );
        }
    }

    /// A bank that is no longer mapped resolves to no offset, so the list greys the breakpoint
    /// rather than leaving it looking armed.
    #[test]
    fn a_breakpoint_whose_bank_is_gone_reads_as_unmapped() {
        let unmapped = [Page::UNMAPPED; PRG_PAGES];
        assert!(is_mapped(&unmapped, &Breakpoint::execute(0x0300, None)));
        assert!(!is_mapped(
            &unmapped,
            &Breakpoint::execute(0x8000, Some(0x4000))
        ));
    }

    /// Adding is not toggling: typing an address that is already listed adds a second breakpoint
    /// on it rather than clearing the first.
    #[test]
    fn adding_an_address_twice_leaves_both_breakpoints() {
        let mut breakpoints = Breakpoints::default();
        let first = breakpoints
            .add(Breakpoint::execute(0xC000, None))
            .expect("added");
        let second = breakpoints
            .add(Breakpoint::execute(0xC000, None))
            .expect("added");

        assert_eq!(armed_at(&breakpoints), [0xC000, 0xC000]);
        assert_ne!(first, second, "the second took the first one's name");
    }

    /// A watch's width follows its value, so a column of bytes does not read as addresses. Only a
    /// comparison or a subtraction goes negative, where hex says nothing.
    #[test]
    fn a_watch_is_written_at_the_width_of_its_value() {
        assert_eq!(watch_value(0x42, false), "$42 66");
        assert_eq!(watch_value(0xFF, false), "$FF 255");
        assert_eq!(watch_value(0x0100, false), "$0100 256");
        assert_eq!(watch_value(0xFFFF, false), "$FFFF 65535");
        assert_eq!(watch_value(-1, false), "-1");
        assert_eq!(watch_value(0, true), "false");
        assert_eq!(watch_value(1, true), "true");
    }

    /// The editor's address box round-trips a breakpoint's own range, so opening the editor and
    /// closing it again leaves the breakpoint where it was.
    #[test]
    fn the_editor_reads_back_the_range_it_writes() {
        for (addr, end) in [(0xC000, 0xC000), (0x6000, 0x7FFF)] {
            let breakpoint = Breakpoint::range(addr, end, None);
            assert_eq!(
                parse_range(&breakpoint.range_text()),
                Some((addr, end)),
                "${addr:04X}-${end:04X}"
            );
        }
    }

    /// An id names one breakpoint for as long as it is listed. Handing a removed one out again
    /// would let the open editor, or a click already in flight, land on whatever took its place.
    #[test]
    fn an_id_is_never_handed_out_twice() {
        let mut breakpoints = Breakpoints::default();
        let first = breakpoints
            .add(Breakpoint::execute(0xC000, None))
            .expect("added");
        breakpoints.remove(first);
        let second = breakpoints
            .add(Breakpoint::execute(0xC000, None))
            .expect("added");

        assert_ne!(first, second);
        assert!(breakpoints.get(first).is_none());
    }

    /// Disabling keeps a breakpoint in the list and out of what the console is told to stop at.
    #[test]
    fn a_disabled_breakpoint_stays_listed_but_is_not_armed() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.add(Breakpoint::execute(0xC000, None));
        breakpoints.add(Breakpoint::execute(0xD000, None));
        let disabled = breakpoints.single_at(0xC000, None).expect("listed");
        breakpoints.get_mut(disabled).expect("listed").enabled = false;

        assert_eq!(armed_at(&breakpoints), [0xD000]);
        assert!(
            breakpoints.get(disabled).is_some_and(|bp| !bp.enabled),
            "a disabled breakpoint was dropped rather than kept"
        );
    }

    /// With no cart loaded nothing is disassembled, which makes this a test of the sweep itself:
    /// that it covers every address exactly once and terminates at `$FFFF` rather than wrapping.
    #[test]
    fn the_sweep_covers_the_whole_address_space_in_order() {
        let deck = ControlDeck::new();
        let address_space = AddressSpace::capture(deck.bus(), &HashMap::new());

        let mut next = 0u32;
        for row in &address_space.rows {
            assert_eq!(u32::from(row.addr()), next, "gap or overlap at ${next:04X}");
            next = match row {
                Row::Block { end, .. } => u32::from(*end) + 1,
                Row::Instruction(_) | Row::Label { .. } => u32::from(row.addr()) + 1,
            };
        }
        assert_eq!(next, 0x1_0000, "sweep stopped short of the end");
    }

    #[test]
    fn ram_and_registers_are_blocks_rather_than_disassembly() {
        let deck = ControlDeck::new();
        let address_space = AddressSpace::capture(deck.bus(), &HashMap::new());

        let kind_at = |addr: u16| {
            let row = address_space.row_at(addr).expect("covered");
            match address_space.rows[row] {
                Row::Block { kind, .. } => Some(kind),
                Row::Instruction(_) | Row::Label { .. } => None,
            }
        };
        assert_eq!(kind_at(0x0000), Some(BlockKind::Ram));
        assert_eq!(kind_at(0x1FFF), Some(BlockKind::Ram));
        assert_eq!(kind_at(0x2000), Some(BlockKind::Registers));
        // Nothing is banked in without a cart.
        assert_eq!(kind_at(0x8000), Some(BlockKind::Unmapped));
    }

    /// A decoded range cannot tell where instructions begin, so it decodes straight through data
    /// and stays misaligned. Landing mid-row leaves PC with no row of its own and nothing for the
    /// view to highlight.
    #[test]
    fn pc_always_starts_a_row_however_misaligned_the_sweep_is() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("load rom");
        for _ in 0..10 {
            let _ = deck.clock_frame().expect("clock frame");
        }

        // An address strictly inside a multi-byte instruction: the sweep decoded across it, so
        // without a pc anchor there is no row that starts there and nothing for the view to mark.
        let swept = AddressSpace::capture(deck.bus(), &HashMap::new());
        let interior = swept
            .rows
            .windows(2)
            .find_map(|pair| match &pair[0] {
                Row::Instruction(line) if pair[1].addr() > line.addr + 1 => Some(line.addr + 1),
                _ => None,
            })
            .expect("a multi-byte instruction somewhere in the sweep");
        assert_ne!(
            swept.rows[swept.row_at(interior).expect("covered")].addr(),
            interior,
            "the chosen address was already a row start, so this proves nothing"
        );

        // Landing there is what stepping into a call does when the sweep is out of step with the
        // real instruction boundaries.
        deck.bus_mut().cpu.pc = interior;
        let anchored = AddressSpace::capture(deck.bus(), &HashMap::new());
        let row = anchored.row_at(interior).expect("covered");
        assert_eq!(
            anchored.rows[row].addr(),
            interior,
            "PC ${interior:04X} fell inside a row instead of starting one"
        );
    }

    /// The aligned case, which is the common one: anchoring on it rather than strictly after it
    /// ends a row where it began and resumes at the same address, sweeping forever.
    #[test]
    fn pc_already_starting_a_row_still_terminates() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("load rom");
        for _ in 0..10 {
            let _ = deck.clock_frame().expect("clock frame");
        }

        let swept = AddressSpace::capture(deck.bus(), &HashMap::new());
        let aligned = swept
            .rows
            .iter()
            .find_map(|row| match row {
                Row::Instruction(line) => Some(line.addr),
                Row::Block { .. } | Row::Label { .. } => None,
            })
            .expect("a disassembled row");

        deck.bus_mut().cpu.pc = aligned;
        let anchored = AddressSpace::capture(deck.bus(), &HashMap::new());
        let row = anchored.row_at(aligned).expect("covered");
        assert_eq!(anchored.rows[row].addr(), aligned);
        // One row per address at worst, so anything beyond that means rows were emitted without
        // advancing.
        assert!(anchored.rows.len() <= 0x1_0000, "sweep emitted extra rows");
    }

    /// Without a code map the sweep decodes every mapped byte, which is how data ends up rendered
    /// as instructions and the decode stays out of step with the real boundaries. With one, only
    /// bytes that have run are decoded and the rest collapses into `unknown`.
    #[test]
    fn only_bytes_that_have_executed_are_disassembled() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("load rom");
        deck.attach_code_map(None);
        for _ in 0..10 {
            let _ = deck.clock_frame().expect("clock frame");
        }

        let mapped = AddressSpace::capture(deck.bus(), &HashMap::new());
        let pc = deck.bus().cpu.pc;
        let code_map = deck.code_map().expect("recording");
        for row in &mapped.rows {
            if let Row::Instruction(line) = row
                && line.addr != pc
            {
                let offset = deck.bus().memory.prg_offset(line.addr).expect("mapped");
                assert!(
                    code_map.is_code(offset),
                    "${:04X} was disassembled without having run",
                    line.addr
                );
            }
        }
        assert!(
            mapped.rows.iter().any(|row| matches!(
                row,
                Row::Block {
                    kind: BlockKind::Unknown,
                    ..
                }
            )),
            "ten frames cannot have executed the whole ROM"
        );

        // The same console with the map taken away, which leaves the sweep decoding all of it.
        deck.detach_code_map();
        let blind = AddressSpace::capture(deck.bus(), &HashMap::new());
        let instructions = |space: &AddressSpace| {
            space
                .rows
                .iter()
                .filter(|row| matches!(row, Row::Instruction(_)))
                .count()
        };
        assert!(
            instructions(&mapped) < instructions(&blind),
            "the map disassembled as much as decoding blind did, so it changed nothing"
        );
    }

    /// A debugger that has just attached has an empty map, so PC would otherwise be the only
    /// decoded row on screen. Straight-line flow from PC is decodable without having run.
    #[test]
    fn the_routine_at_pc_is_decoded_before_anything_has_run() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("load rom");
        deck.attach_code_map(None);

        let rows = AddressSpace::capture(deck.bus(), &HashMap::new()).rows;
        let pc = deck.bus().cpu.pc;
        let from_pc = rows
            .iter()
            .skip_while(|row| row.addr() != pc)
            .take_while(|row| matches!(row, Row::Instruction(_)))
            .count();
        assert!(
            from_pc > 1,
            "only {from_pc} row(s) decoded at PC ${pc:04X} with an empty code map"
        );

        // The run ends at the first instruction that does not reach the next address, and the
        // bytes after it are unknown until something executes them.
        let end = rows
            .iter()
            .skip_while(|row| row.addr() != pc)
            .take(from_pc)
            .last()
            .expect("a decoded run");
        let Row::Instruction(line) = end else {
            unreachable!("take_while kept only instructions")
        };
        let instr = Cpu::INSTR_REF[usize::from(deck.bus().peek(line.addr))].instr;
        assert!(
            !instr.falls_through(),
            "the run stopped at {instr:?} at ${:04X}, which reaches the next address",
            line.addr
        );
    }

    /// PC is about to run whether or not it has run before, so it starts an instruction even where
    /// the map says nothing. Without that it would fall inside an `unknown` block with no row of
    /// its own for the view to highlight.
    #[test]
    fn pc_starts_a_row_even_where_nothing_has_executed() {
        let mut deck = ControlDeck::new();
        deck.load_rom_path("../tetanes-core/test_roms/spritecans.nes")
            .expect("load rom");
        deck.attach_code_map(None);
        for _ in 0..10 {
            let _ = deck.clock_frame().expect("clock frame");
        }

        // Strictly inside an unknown block, so nothing but PC itself can start a row there.
        let unexecuted = AddressSpace::capture(deck.bus(), &HashMap::new())
            .rows
            .iter()
            .find_map(|row| match row {
                Row::Block {
                    start,
                    end,
                    kind: BlockKind::Unknown,
                } if *start >= 0x8000 && end > start => Some(start + 1),
                _ => None,
            })
            .expect("some of the ROM has not run");

        deck.bus_mut().cpu.pc = unexecuted;
        let anchored = AddressSpace::capture(deck.bus(), &HashMap::new());
        let row = anchored.row_at(unexecuted).expect("covered");
        assert_eq!(
            anchored.rows[row].addr(),
            unexecuted,
            "PC ${unexecuted:04X} fell inside an unknown block instead of starting a row"
        );
    }

    #[test]
    fn an_address_inside_a_block_finds_that_block() {
        let deck = ControlDeck::new();
        let address_space = AddressSpace::capture(deck.bus(), &HashMap::new());

        let row = address_space.row_at(0x1234).expect("covered");
        match address_space.rows[row] {
            Row::Block { start, end, .. } => assert!(start <= 0x1234 && 0x1234 <= end),
            Row::Instruction(_) | Row::Label { .. } => panic!("work ram should be a block"),
        }
    }
}

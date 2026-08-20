use crate::nes::{
    action::{Debug, DebugStep},
    config::Config,
    event::{ConfigEvent, DebugRequest, EmulationEvent, NesEventProxy, UiEvent},
    renderer::gui::{MessageType, lib::ViewportOptions, palette::Palette},
};
use egui::{
    CentralPanel, Color32, Context, Grid, Label, Panel, Rect, RichText, ScrollArea, Sense, Ui,
    Vec2, ViewportClass, ViewportId, text::CCursor,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tetanes_core::{
    bus::Bus,
    cpu::{Cpu, Disasm, Status, instr::InstrRef},
    debug::{Access, AccessHit, Breakpoint as DeckBreakpoint, Breakpoints as DeckBreakpoints},
    memory::{Memory, PRG_PAGES, Page},
};

/// A range of addresses the console stops at when one is accessed.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub struct Breakpoint {
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
}

impl Breakpoint {
    /// A breakpoint on a single address, stopping before it executes.
    ///
    /// `offset` pins it to the bank mapped at `addr` as the window draws it.
    pub const fn execute(addr: u16, offset: Option<u32>) -> Self {
        Self {
            addr,
            end: addr,
            offset,
            access: Access::EXEC,
            enabled: true,
            breaks: true,
        }
    }

    /// The range as the list writes it, one address or two.
    pub fn range_text(&self) -> String {
        if self.addr == self.end {
            format!("${:04X}", self.addr)
        } else {
            format!("${:04X}-${:04X}", self.addr, self.end)
        }
    }

    /// What the console is told, which drops the parts only the list draws.
    const fn armed(&self) -> DeckBreakpoint {
        DeckBreakpoint {
            start: self.addr,
            end: self.end,
            offset: self.offset,
            access: self.access,
            breaks: self.breaks,
            condition: None,
        }
    }
}

/// The Debugger's breakpoints, in address order so the list reads like the disassembly.
///
/// The console is only ever told the enabled addresses, since that is all it can act on. The
/// window draws the rest.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[must_use]
pub struct Breakpoints(Vec<Breakpoint>);

impl Breakpoints {
    /// Add `breakpoint`, if there is not one on the same bytes already.
    pub fn add(&mut self, breakpoint: Breakpoint) {
        if self.get(breakpoint.addr, breakpoint.offset).is_none()
            && self.0.len() < DeckBreakpoints::MAX
        {
            let index = self.0.partition_point(|other| other.addr < breakpoint.addr);
            self.0.insert(index, breakpoint);
        }
    }

    /// Remove the breakpoint on `addr` in the bank at `offset`, reporting whether there was one.
    pub fn remove(&mut self, addr: u16, offset: Option<u32>) -> bool {
        let held = self.0.len();
        self.0
            .retain(|breakpoint| breakpoint.addr != addr || breakpoint.offset != offset);
        self.0.len() != held
    }

    /// Stop before the instruction at `addr` in the bank at `offset` executes, or clear the
    /// breakpoint already on it.
    pub fn toggle(&mut self, addr: u16, offset: Option<u32>) {
        if !self.remove(addr, offset) {
            self.add(Breakpoint::execute(addr, offset));
        }
    }

    /// Whether another breakpoint would be refused.
    pub const fn is_full(&self) -> bool {
        self.0.len() >= DeckBreakpoints::MAX
    }

    /// The breakpoint on `addr` in the bank at `offset`, whether or not it is enabled.
    ///
    /// One address holds one breakpoint per bank, since the same address in two banks is two
    /// different instructions.
    pub fn get(&self, addr: u16, offset: Option<u32>) -> Option<&Breakpoint> {
        self.0
            .iter()
            .find(|breakpoint| breakpoint.addr == addr && breakpoint.offset == offset)
    }

    /// What the console is to act on, which is the enabled ones with an access selected.
    pub fn armed(&self) -> Vec<DeckBreakpoint> {
        self.0
            .iter()
            .filter(|breakpoint| breakpoint.enabled && !breakpoint.access.is_empty())
            .map(Breakpoint::armed)
            .collect()
    }

    /// The breakpoints in address order, for the window to edit in place.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Breakpoint> {
        self.0.iter_mut()
    }

    /// Whether no breakpoint is listed, enabled or not.
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
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
}

impl Row {
    /// The address this row starts at.
    pub const fn addr(&self) -> u16 {
        match self {
            Self::Instruction(disasm) => disasm.addr,
            Self::Block { start, .. } => *start,
        }
    }

    /// Whether `addr` falls in this row, which for a block is anywhere in its range.
    pub const fn covers(&self, addr: u16) -> bool {
        match self {
            Self::Instruction(disasm) => disasm.addr == addr,
            Self::Block { start, end, .. } => *start <= addr && addr <= *end,
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
    /// Previously executed instructions, oldest first, ending just before PC from
    /// [`DebugRequest::history_lines`].
    pub history: Vec<Disasm>,
    /// The range requested from [`DebugRequest::memory`], if any.
    pub memory: Vec<u8>,
    /// Accesses that breakpoints recorded without stopping, oldest first.
    pub access_log: Vec<AccessHit>,
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
            history: Vec::new(),
            memory: Vec::new(),
            access_log: Vec::new(),
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
            history,
            memory: request.memory.map_or_else(Vec::new, |(start, len)| {
                (0..len).map(|i| bus.peek(start.wrapping_add(i))).collect()
            }),
            // Filled by the caller, which owns the console the log is drained from.
            access_log: Vec::new(),
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
    pub fn capture(bus: &Bus) -> Self {
        let mut rows = Vec::new();
        let mut disasm = Disasm::default();
        let mut addr = 0u32;
        let pc = bus.cpu.pc;
        // Set while decoding forward from PC. The map contains only what has executed, so a
        // debugger that has just attached knows nothing about the routine PC is sitting in.
        let mut following = false;

        while addr <= u32::from(u16::MAX) {
            let start = addr as u16;
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
    /// The breakpoint list, and the box that adds one.
    Breakpoints,
    /// The instructions that ran most recently.
    History,
}

impl Pane {
    /// Every pane, in the order a column stacks them.
    pub const ALL: [Self; 5] = [
        Self::Disassembly,
        Self::Registers,
        Self::Stack,
        Self::Breakpoints,
        Self::History,
    ];

    /// The heading the pane draws above its view.
    pub const fn title(self) -> &'static str {
        match self {
            Self::Disassembly => "Disassembly",
            Self::Registers => "Registers",
            Self::Stack => "Stack",
            Self::Breakpoints => "Breakpoints",
            Self::History => "Recently executed",
        }
    }

    /// Where the pane is placed.
    pub const fn column(self) -> Column {
        match self {
            Self::Disassembly => Column::Center,
            Self::Registers | Self::Stack | Self::Breakpoints => Column::Right,
            Self::History => Column::Bottom,
        }
    }

    /// The pane's height before anything drags its splitter.
    ///
    /// The center column's single pane takes what is left, so its height is never asked for.
    pub const fn default_size(self) -> f32 {
        match self {
            Self::Disassembly => 0.0,
            Self::Registers => 92.0,
            Self::Stack => 220.0,
            Self::Breakpoints => 160.0,
            Self::History => 140.0,
        }
    }

    /// The [`egui::Id`] the pane's [`Panel`] and its stored size are keyed by.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Disassembly => "debugger_pane_disassembly",
            Self::Registers => "debugger_pane_registers",
            Self::Stack => "debugger_pane_stack",
            Self::Breakpoints => "debugger_pane_breakpoints",
            Self::History => "debugger_pane_history",
        }
    }
}

/// What a click on a disassembly row asked for.
///
/// Reported rather than applied, since the row draws from state it borrows.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
enum RowAction {
    /// Add a breakpoint at this address in the bank at this arena offset, or remove the one
    /// already there.
    ToggleBreakpoint(u16, Option<u32>),
    /// Arm or disarm the breakpoint there, keeping it listed either way.
    ArmBreakpoint(u16, Option<u32>, bool),
    /// Make this the row later commands act on.
    Select(u16),
}

/// Where a pane is placed. Panes keep their column, and only redistribute height within it.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub enum Column {
    /// What is left once the other columns have taken their space.
    Center,
    /// Down the right edge, above the bottom column.
    Right,
    /// Across the full width of the window.
    Bottom,
}

impl Column {
    /// Every column, in the order it claims space. The center takes what the others leave.
    const ORDER: [Self; 3] = [Self::Bottom, Self::Right, Self::Center];

    /// The column's own size before anything drags its splitter: a width on the right, a height
    /// along the bottom.
    const fn default_size(self) -> f32 {
        match self {
            // Wide enough for the register grid's six columns without wrapping.
            Self::Center | Self::Right => 380.0,
            Self::Bottom => 160.0,
        }
    }

    /// How the column divides its space among the panes of `open` that belong to it: those sized
    /// by [`Pane::default_size`], then the one taking what is left.
    ///
    /// `None` when none of them are open, which is when the column is not drawn and so takes no
    /// space.
    fn tiling(self, open: &[Pane]) -> Option<(Vec<Pane>, Pane)> {
        let mut panes = Pane::ALL
            .into_iter()
            .filter(|pane| pane.column() == self && open.contains(pane))
            .collect::<Vec<_>>();
        let filling = panes.pop()?;
        Some((panes, filling))
    }

    /// The [`egui::Id`] the column's [`Panel`] and its stored size are keyed by.
    const fn id(self) -> &'static str {
        match self {
            Self::Center => "debugger_column_center",
            Self::Right => "debugger_column_right",
            Self::Bottom => "debugger_column_bottom",
        }
    }
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
            ("Mode", instr.addr_mode.name().to_string()),
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
fn register_cell(ui: &mut Ui, name: &str, value: &str, heading: &str, rows: &[(&str, String)]) {
    let hover = |ui: &mut Ui| {
        ui.strong(heading);
        detail_rows(ui, name, rows);
    };
    ui.strong(name).on_hover_ui(hover);
    ui.monospace(value).on_hover_ui(hover);
}

/// Where `addr` sits in the cart arena under `pages`, the mapping the window is drawing.
fn prg_offset(pages: &[Page; PRG_PAGES], addr: u16) -> Option<u32> {
    Memory::offset_in(pages, addr).map(|offset| offset as u32)
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
fn request(open: &[Pane], history_lines: u16) -> DebugRequest {
    DebugRequest {
        history_lines: if open.contains(&Pane::History) {
            history_lines
        } else {
            0
        },
        stack: open.contains(&Pane::Stack),
        memory: None,
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
            .into_iter()
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
                let mut window_open = open.load(Ordering::Acquire);
                egui::Window::new(CpuDebugger::TITLE)
                    .open(&mut window_open)
                    .show(ui, |ui| state.lock().ui(ui, opts.enabled, &cfg));
                open.store(window_open, Ordering::Release);
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
            self.send_breakpoints();
        }
    }

    /// Say that the list is full, which is the one reason adding a breakpoint does nothing.
    fn warn_breakpoints_full(&self) {
        self.tx.event(UiEvent::Message((
            MessageType::Warn,
            format!("Only {} breakpoints can be enabled.", DeckBreakpoints::MAX),
        )));
    }

    /// Tell the console which addresses to stop at.
    fn send_breakpoints(&self) {
        self.tx
            .event(EmulationEvent::DebugBreakpoints(self.breakpoints.armed()));
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
            .into_iter()
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
        self.tx
            .event(EmulationEvent::DebugSubscribe(Some(self.request())));
    }

    /// What the console is asked to capture for the panes that are open.
    fn request(&self) -> DebugRequest {
        request(&self.panes, self.history_lines)
    }

    /// Forget every dragged splitter, so the next frame lays out from the default sizes.
    fn reset_layout(ctx: &Context) {
        ctx.data_mut(|data| {
            for id in Pane::ALL
                .into_iter()
                .map(Pane::id)
                .chain([Column::Right.id(), Column::Bottom.id()])
            {
                data.remove::<egui::containers::PanelState>(egui::Id::new(id));
            }
        });
    }

    fn ui(&mut self, ui: &mut Ui, enabled: bool, cfg: &Config) {
        ui.add_enabled_ui(enabled, |ui| {
            Panel::top("debugger_toolbar").show(ui, |ui| self.toolbar(ui, cfg));
            for column in Column::ORDER {
                self.column(ui, column);
            }
        });
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
            ui.menu_button("View", |ui| self.view_menu(ui));
        });
    }

    /// Which panes are open, and the button that undoes every splitter drag.
    fn view_menu(&mut self, ui: &mut Ui) {
        for pane in Pane::ALL {
            // The center pane has no toggle: an empty center with the columns still drawn reads
            // as a broken window.
            if pane.column() == Column::Center {
                continue;
            }
            let mut open = self.is_open(pane);
            if ui.checkbox(&mut open, pane.title()).changed() {
                self.set_pane_open(pane, open);
            }
        }
        ui.separator();
        if ui
            .button("Reset layout")
            .on_hover_text("Restore panes to their default sizes")
            .clicked()
        {
            Self::reset_layout(ui.ctx());
        }
    }

    /// Stack `column`'s open panes inside a panel of its own, drawing nothing when it is empty.
    ///
    /// Every pane but the last is sized by [`Pane::default_size`], and the last takes what is
    /// left, so a splitter sits between each pair and one between the column and the center.
    fn column(&mut self, ui: &mut Ui, column: Column) {
        let Some((sized, filling)) = column.tiling(&self.panes) else {
            return;
        };
        let mut closed = None;
        let mut tile = |ui: &mut Ui, this: &mut Self| {
            for pane in &sized {
                let close = Panel::top(pane.id())
                    .resizable(true)
                    .default_size(pane.default_size())
                    .show(ui, |ui| this.pane(ui, *pane))
                    .inner;
                if close {
                    closed = Some(*pane);
                }
            }
            if CentralPanel::default()
                .show(ui, |ui| this.pane(ui, filling))
                .inner
            {
                closed = Some(filling);
            }
        };
        match column {
            Column::Center => tile(ui, self),
            Column::Right => {
                Panel::right(column.id())
                    .default_size(column.default_size())
                    .show(ui, |ui| tile(ui, self));
            }
            Column::Bottom => {
                Panel::bottom(column.id())
                    .resizable(true)
                    .default_size(column.default_size())
                    .show(ui, |ui| tile(ui, self));
            }
        }
        if let Some(pane) = closed {
            self.set_pane_open(pane, false);
        }
    }

    /// Draw `pane`'s heading and its view, reporting whether the heading's ✖ was clicked.
    ///
    /// A panel has no title bar of its own, so the heading is part of what the pane draws.
    fn pane(&mut self, ui: &mut Ui, pane: Pane) -> bool {
        let closed = ui
            .horizontal(|ui| {
                ui.strong(pane.title());
                if pane.column() == Column::Center {
                    return false;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.small_button("✖").on_hover_text("Close pane").clicked()
                })
                .inner
            })
            .inner;
        match pane {
            Pane::Disassembly => self.disassembly(ui),
            Pane::Registers => self.registers(ui),
            Pane::Stack => self.stack(ui),
            Pane::Breakpoints => self.breakpoint_list(ui),
            // Its own pane rather than inline above PC: the disassembly is ordered by address and
            // this is ordered by time, so the two only coincide in straight-line code.
            Pane::History => self.history(ui),
        }
        closed
    }

    fn registers(&mut self, ui: &mut Ui) {
        // Every pane scrolls, so its panel keeps the height its splitter was dragged to.
        ScrollArea::vertical()
            .id_salt("registers")
            .auto_shrink([false, false])
            .show(ui, |ui| self.register_grid(ui));
    }

    fn register_grid(&mut self, ui: &mut Ui) {
        let cpu = &self.snapshot.cpu;
        Grid::new("cpu_registers")
            .num_columns(6)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                register_cell(
                    ui,
                    "PC",
                    &format!("${:04X}", cpu.pc),
                    "Program counter - the instruction about to run",
                    &self.address_rows(cpu.pc),
                );
                register_cell(
                    ui,
                    "A",
                    &format!("${:02X}", cpu.acc),
                    "Accumulator",
                    &byte_rows(cpu.acc),
                );
                register_cell(
                    ui,
                    "SP",
                    &format!("${:02X}", cpu.sp),
                    "Stack pointer",
                    &self.stack_rows(),
                );
                ui.end_row();

                register_cell(
                    ui,
                    "X",
                    &format!("${:02X}", cpu.x),
                    "Index register X",
                    &byte_rows(cpu.x),
                );
                register_cell(
                    ui,
                    "Y",
                    &format!("${:02X}", cpu.y),
                    "Index register Y",
                    &byte_rows(cpu.y),
                );
                register_cell(
                    ui,
                    "Cycle",
                    &cpu.cycle.to_string(),
                    "CPU cycles since power on",
                    &[
                        ("Cycles", cpu.cycle.to_string()),
                        ("Frame", self.snapshot.frame.to_string()),
                    ],
                );
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
                ui.label(text)
                    .on_hover_text(format!("{meaning}: {}", if set { "set" } else { "clear" }));
            }
        });
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
                    self.breakpoints.add(Breakpoint {
                        addr,
                        end,
                        ..Breakpoint::execute(addr, offset)
                    });
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
                        let breaks = breakpoint.breaks;
                        armed_changed |= ui
                            .toggle_value(&mut breakpoint.breaks, "⏸")
                            .on_hover_text(if breaks {
                                "Continue execution"
                            } else {
                                "Break execution"
                            })
                            .changed();
                        if ui.small_button("✖").clicked() {
                            removed = Some((breakpoint.addr, breakpoint.offset));
                        }
                    });
                }
            });
        if let Some((addr, offset)) = removed {
            self.breakpoints.remove(addr, offset);
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
                if self.breakpoints.is_full() && self.breakpoints.get(addr, offset).is_none() {
                    self.warn_breakpoints_full();
                } else {
                    self.breakpoints.toggle(addr, offset);
                    self.send_breakpoints();
                }
            }
            Some(RowAction::ArmBreakpoint(addr, offset, enabled)) => {
                for breakpoint in self.breakpoints.iter_mut() {
                    if breakpoint.addr == addr && breakpoint.offset == offset {
                        breakpoint.enabled = enabled;
                    }
                }
                self.send_breakpoints();
            }
            Some(RowAction::Select(addr)) => self.selected = Some(addr),
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
            Row::Block { .. } => None,
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
        painter.galley(
            text.left_center() - Vec2::new(0.0, font.size / 2.0),
            galley,
            palette.operand,
        );

        let (Some(addr), Some(disasm)) = (addr, instruction) else {
            let row_response =
                ui.interact(text, ui.id().with(("block", row.addr())), Sense::click());
            let selected = row_response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
                .then(|| RowAction::Select(row.addr()));
            return (response, selected);
        };
        // The row draws the breakpoint set on the bank it shows. A breakpoint on the same address
        // in another bank belongs to code that is not on screen.
        let offset = self.prg_offset(addr);
        if let Some(breakpoint) = self.breakpoints.get(addr, offset) {
            // Filled for armed and hollow for listed, a difference that reads without the
            // palette being involved.
            let center = gutter.center();
            let radius = GUTTER_WIDTH / 4.0;
            if breakpoint.enabled {
                painter.circle_filled(center, radius, palette.breakpoint);
            } else {
                painter.circle_stroke(
                    center,
                    radius,
                    egui::Stroke::new(1.5, palette.breakpoint_disabled),
                );
            }
        }

        let gutter_response = gutter_response.expect("an instruction row interacts");
        let mut action = gutter_response
            .clicked()
            .then_some(RowAction::ToggleBreakpoint(addr, offset));
        match self.breakpoints.get(addr, offset) {
            Some(breakpoint) => {
                let enabled = breakpoint.enabled;
                gutter_response
                    .on_hover_text("Remove breakpoint")
                    .context_menu(|ui| {
                        if ui
                            .button(if enabled { "Disable" } else { "Enable" })
                            .clicked()
                        {
                            action = Some(RowAction::ArmBreakpoint(addr, offset, !enabled));
                            ui.close();
                        }
                        if ui.button("Remove").clicked() {
                            action = Some(RowAction::ToggleBreakpoint(addr, offset));
                            ui.close();
                        }
                    });
            }
            None => {
                gutter_response.on_hover_text("Add breakpoint");
            }
        }

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
            let _ = part(format!(" {}", disasm.operand), palette.operand);
        }
        // Bracketed and tinted apart from the operand that computed it, since it names a second
        // address the row did not write down.
        if let Some(effective) = disasm.effective_text() {
            let _ = part(format!(" [{effective}]"), palette.effective);
        }
        if let Some(value) = disasm.value {
            let _ = part(format!(" = {value}"), palette.resolved);
        }
        (ui.painter().layout_job(job), mnemonic_span)
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
        AddressSpace, BlockKind, Breakpoint, Breakpoints, Column, CpuSnapshot, PRG_PAGES, Page,
        Pane, Row, is_mapped, parse_addr, parse_range, prg_offset, request, row_is_visible,
    };

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
            .tiling(&Pane::ALL)
            .expect("the right column has panes");
        assert_eq!(sized, [Pane::Registers, Pane::Stack]);
        assert_eq!(filling, Pane::Breakpoints);
    }

    /// A column that reports nothing is not drawn, so it takes no width or height from the rest.
    #[test]
    fn a_column_with_nothing_open_is_not_drawn() {
        assert_eq!(Column::Bottom.tiling(&[Pane::Disassembly]), None);
        assert_eq!(Column::Right.tiling(&[Pane::Disassembly]), None);
    }

    /// [`Column::ORDER`] reaches every column, and every column places its panes, so no open pane
    /// goes undrawn.
    #[test]
    fn every_pane_is_laid_out_in_exactly_one_column() {
        let mut placed = Vec::new();
        for column in Column::ORDER {
            if let Some((sized, filling)) = column.tiling(&Pane::ALL) {
                placed.extend(sized);
                placed.push(filling);
            }
        }
        for pane in Pane::ALL {
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

        let space = AddressSpace::capture(deck.bus());
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
        let address_space = AddressSpace::capture(deck.bus());
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
        let all = request(&Pane::ALL, HISTORY_LINES);
        assert_eq!(all.history_lines, HISTORY_LINES);
        assert!(all.stack);

        let closed = Pane::ALL
            .into_iter()
            .filter(|pane| !matches!(pane, Pane::History | Pane::Stack))
            .collect::<Vec<_>>();
        let request = request(&closed, HISTORY_LINES);
        assert_eq!(request.history_lines, 0);
        assert!(!request.stack);
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
        breakpoints.add(Breakpoint::execute(0x8000, Some(0x4000)));
        breakpoints.add(Breakpoint::execute(0x8000, Some(0x8000)));
        assert_eq!(armed_at(&breakpoints), [0x8000, 0x8000]);

        assert!(breakpoints.remove(0x8000, Some(0x4000)));
        assert_eq!(
            breakpoints.get(0x8000, Some(0x8000)).map(|bp| bp.offset),
            Some(Some(0x8000)),
            "removing one bank's breakpoint took the other bank's with it"
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
        let snapshot = CpuSnapshot::capture(deck.bus(), &request(&Pane::ALL, 8));

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

    /// Adding is not toggling: typing an address that is already listed must not clear it.
    #[test]
    fn adding_an_address_twice_leaves_one_breakpoint() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.add(Breakpoint::execute(0xC000, None));
        breakpoints.add(Breakpoint::execute(0xC000, None));
        assert_eq!(armed_at(&breakpoints), [0xC000]);
    }

    /// Disabling keeps a breakpoint in the list and out of what the console is told to stop at.
    #[test]
    fn a_disabled_breakpoint_stays_listed_but_is_not_armed() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.add(Breakpoint::execute(0xC000, None));
        breakpoints.add(Breakpoint::execute(0xD000, None));
        for breakpoint in breakpoints.iter_mut() {
            breakpoint.enabled = breakpoint.addr != 0xC000;
        }

        assert_eq!(armed_at(&breakpoints), [0xD000]);
        assert!(
            breakpoints.get(0xC000, None).is_some_and(|bp| !bp.enabled),
            "a disabled breakpoint was dropped rather than kept"
        );
    }

    /// With no cart loaded nothing is disassembled, which makes this a test of the sweep itself:
    /// that it covers every address exactly once and terminates at `$FFFF` rather than wrapping.
    #[test]
    fn the_sweep_covers_the_whole_address_space_in_order() {
        let deck = ControlDeck::new();
        let address_space = AddressSpace::capture(deck.bus());

        let mut next = 0u32;
        for row in &address_space.rows {
            assert_eq!(u32::from(row.addr()), next, "gap or overlap at ${next:04X}");
            next = match row {
                Row::Block { end, .. } => u32::from(*end) + 1,
                Row::Instruction(_) => u32::from(row.addr()) + 1,
            };
        }
        assert_eq!(next, 0x1_0000, "sweep stopped short of the end");
    }

    #[test]
    fn ram_and_registers_are_blocks_rather_than_disassembly() {
        let deck = ControlDeck::new();
        let address_space = AddressSpace::capture(deck.bus());

        let kind_at = |addr: u16| {
            let row = address_space.row_at(addr).expect("covered");
            match address_space.rows[row] {
                Row::Block { kind, .. } => Some(kind),
                Row::Instruction(_) => None,
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
        let swept = AddressSpace::capture(deck.bus());
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
        let anchored = AddressSpace::capture(deck.bus());
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

        let swept = AddressSpace::capture(deck.bus());
        let aligned = swept
            .rows
            .iter()
            .find_map(|row| match row {
                Row::Instruction(line) => Some(line.addr),
                Row::Block { .. } => None,
            })
            .expect("a disassembled row");

        deck.bus_mut().cpu.pc = aligned;
        let anchored = AddressSpace::capture(deck.bus());
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

        let mapped = AddressSpace::capture(deck.bus());
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
        let blind = AddressSpace::capture(deck.bus());
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

        let rows = AddressSpace::capture(deck.bus()).rows;
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
        let unexecuted = AddressSpace::capture(deck.bus())
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
        let anchored = AddressSpace::capture(deck.bus());
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
        let address_space = AddressSpace::capture(deck.bus());

        let row = address_space.row_at(0x1234).expect("covered");
        match address_space.rows[row] {
            Row::Block { start, end, .. } => assert!(start <= 0x1234 && 0x1234 <= end),
            Row::Instruction(_) => panic!("work ram should be a block"),
        }
    }
}

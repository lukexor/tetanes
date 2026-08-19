use crate::nes::{
    event::{DebugRequest, EmulationEvent, NesEventProxy},
    renderer::gui::lib::ViewportOptions,
};
use egui::{
    CentralPanel, Color32, Context, Grid, Label, Panel, RichText, ScrollArea, Sense, Ui, Vec2,
    ViewportClass, ViewportId,
};
use parking_lot::Mutex;
use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tetanes_core::{
    bus::Bus,
    cpu::{Cpu, Status},
};

/// An address the console stops at before executing.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[must_use]
pub struct Breakpoint {
    /// The address the console stops at.
    pub addr: u16,
    /// Cleared to keep a breakpoint in the list without stopping at it.
    pub enabled: bool,
}

/// The Debugger's breakpoints, in address order so the list reads like the disassembly.
///
/// The console is only ever told the enabled addresses, since that is all it can act on. The
/// window draws the rest.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[must_use]
pub struct Breakpoints(Vec<Breakpoint>);

impl Breakpoints {
    /// Add a breakpoint at `addr`, if there is not one there already.
    pub fn add(&mut self, addr: u16) {
        if self.get(addr).is_none() {
            let index = self.0.partition_point(|breakpoint| breakpoint.addr < addr);
            self.0.insert(
                index,
                Breakpoint {
                    addr,
                    enabled: true,
                },
            );
        }
    }

    /// Remove the breakpoint at `addr`, reporting whether there was one.
    pub fn remove(&mut self, addr: u16) -> bool {
        let held = self.0.len();
        self.0.retain(|breakpoint| breakpoint.addr != addr);
        self.0.len() != held
    }

    /// Add a breakpoint at `addr`, or remove the one already there.
    pub fn toggle(&mut self, addr: u16) {
        if !self.remove(addr) {
            self.add(addr);
        }
    }

    /// The breakpoint at `addr`, whether or not it is enabled.
    pub fn get(&self, addr: u16) -> Option<&Breakpoint> {
        self.0.iter().find(|breakpoint| breakpoint.addr == addr)
    }

    /// The addresses the console is to stop at.
    pub fn armed(&self) -> Vec<u16> {
        self.0
            .iter()
            .filter(|breakpoint| breakpoint.enabled)
            .map(|breakpoint| breakpoint.addr)
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
    Instruction(DisasmLine),
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
            Self::Instruction(line) => line.addr,
            Self::Block { start, .. } => *start,
        }
    }
}

/// One disassembled instruction, with the address it was read from.
#[derive(Debug, Clone)]
#[must_use]
pub struct DisasmLine {
    /// Address of the disassembled instruction.
    pub addr: u16,
    /// Disassembly text.
    pub text: String,
}

/// Snapshot of the Control Deck CPU state for use by the Debugger.
#[derive(Debug, Default, Clone)]
#[must_use]
pub struct CpuSnapshot {
    /// CPU state and registers.
    pub cpu: Cpu,
    /// CPU stack.
    pub stack: Vec<u8>,
    /// Previously executed instructions, oldest first, ending just before PC from
    /// [`DebugRequest::history_lines`].
    pub history: Vec<DisasmLine>,
    /// Disassembled instructions from the current PC from [`DebugRequest::disasm_lines`].
    pub disasm: Vec<DisasmLine>,
    /// The range requested from [`DebugRequest::memory`], if any.
    pub memory: Vec<u8>,
}

impl CpuSnapshot {
    /// Capture the requested snapshot.
    pub fn capture(bus: &Bus, request: &DebugRequest) -> Self {
        let mut text = String::with_capacity(64);
        let mut pc = bus.cpu.pc;
        let mut disasm = Vec::with_capacity(usize::from(request.disasm_lines));
        for _ in 0..request.disasm_lines {
            let addr = pc;
            bus.disassemble_into(&mut pc, &mut text);
            disasm.push(DisasmLine {
                addr,
                text: text.clone(),
            });
        }

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
                    bus.disassemble_into(&mut pc, &mut text);
                    DisasmLine {
                        addr,
                        text: text.clone(),
                    }
                })
                .collect()
        });

        Self {
            cpu: bus.cpu.clone(),
            stack: (0..0x0100u16)
                .map(|offset| bus.peek(Cpu::SP_BASE + offset))
                .collect(),
            history,
            disasm,
            memory: request.memory.map_or_else(Vec::new, |(start, len)| {
                (0..len).map(|i| bus.peek(start.wrapping_add(i))).collect()
            }),
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
        let mut text = String::with_capacity(64);
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
                    bus.disassemble_into(&mut next, &mut text);
                    // `disassemble_into` wraps past $FFFF, so take the length rather than the
                    // address it landed on, and never advance by zero.
                    let len = next.wrapping_sub(start).max(1);
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
                        rows.push(Row::Instruction(DisasmLine {
                            addr: start,
                            text: text.clone(),
                        }));
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

    /// Whether `addr` collapses into a block. `None` means it is mapped cart ROM, which
    /// [`AddressSpace::starts_instruction`] then decides how to render.
    const fn block_kind(bus: &Bus, addr: u16) -> Option<BlockKind> {
        match addr {
            0x0000..=0x1FFF => Some(BlockKind::Ram),
            0x2000..=0x401F => Some(BlockKind::Registers),
            _ => match bus.memory.prg_offset(addr) {
                Some(_) if addr >= 0x8000 => None,
                Some(_) => Some(BlockKind::SaveRam),
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
    breakpoints: Breakpoints,
    /// What is typed in the breakpoint box, which is not a breakpoint until it is added.
    breakpoint_goto: String,
    disasm_lines: u16,
    history_lines: u16,
}

/// Parse an address as typed into one of the window's address boxes, with or without the `$`.
fn parse_addr(text: &str) -> Option<u16> {
    u16::from_str_radix(text.trim().trim_start_matches('$'), 16).ok()
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
    /// Enough to fill the disassembly pane without scrolling at the default window size.
    const DISASM_LINES: u16 = 24;
    /// Enough context to see how the current instruction was reached without pushing it off-screen.
    const HISTORY_LINES: u16 = 8;

    pub fn new(tx: NesEventProxy) -> Self {
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
                breakpoints: Breakpoints::default(),
                breakpoint_goto: String::new(),
                disasm_lines: Self::DISASM_LINES,
                history_lines: Self::HISTORY_LINES,
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

    pub fn update_snapshot(&mut self, snapshot: CpuSnapshot) {
        let mut state = self.state.lock();
        // Follow PC only when it moved, so scrolling away stays put while the console is stopped.
        if state.snapshot.cpu.pc != snapshot.cpu.pc {
            state.scroll_to = Some(snapshot.cpu.pc);
        }
        state.snapshot = snapshot;
    }

    pub fn update_address_space(&mut self, address_space: AddressSpace) {
        let mut state = self.state.lock();
        state.address_space = address_space;
        // A capture arrives whenever the code map marks something new, and rows added above PC move
        // its row index. Re-center against the new rows so PC stays on the same line.
        state.scroll_to = Some(state.snapshot.cpu.pc);
    }

    pub fn show(&mut self, ui: &mut Ui, opts: ViewportOptions) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }

        let open = Arc::clone(&self.open);
        let state = Arc::clone(&self.state);

        let mut viewport_builder = egui::ViewportBuilder::default()
            .with_title(Self::TITLE)
            .with_inner_size(Vec2::new(760.0, 720.0));
        if opts.always_on_top {
            viewport_builder = viewport_builder.with_always_on_top();
        }

        ui.show_viewport_deferred(self.id, viewport_builder, move |ui, class| {
            if class == ViewportClass::EmbeddedWindow {
                let mut window_open = open.load(Ordering::Acquire);
                egui::Window::new(CpuDebugger::TITLE)
                    .open(&mut window_open)
                    .show(ui, |ui| state.lock().ui(ui, opts.enabled));
                open.store(window_open, Ordering::Release);
            } else {
                CentralPanel::default().show(ui, |ui| state.lock().ui(ui, opts.enabled));
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
        self.tx.event(EmulationEvent::DebugSubscribe(open.then_some(
            DebugRequest {
                disasm_lines: self.disasm_lines,
                history_lines: self.history_lines,
                memory: None,
            },
        )));
        // Closing disarms them, since a console that stopped with nothing to show it would just
        // look frozen. The list is kept here, so opening puts back what was armed.
        if open {
            self.send_breakpoints();
        }
    }

    /// Tell the console which addresses to stop at.
    fn send_breakpoints(&self) {
        self.tx
            .event(EmulationEvent::DebugBreakpoints(self.breakpoints.armed()));
    }

    fn ui(&mut self, ui: &mut Ui, enabled: bool) {
        ui.add_enabled_ui(enabled, |ui| {
            self.registers(ui);
            // Its own section rather than inline above PC: the disassembly is ordered by address
            // and this is ordered by time, so the two only coincide in straight-line code.
            egui::CollapsingHeader::new("Recently executed")
                .default_open(false)
                .show(ui, |ui| self.history(ui));
            egui::CollapsingHeader::new("Breakpoints")
                .default_open(false)
                .show(ui, |ui| self.breakpoint_list(ui));
            ui.separator();
            // The stack is a fixed two columns of hex. The disassembly wants every pixel it can
            // get, so it takes what is left rather than an even half.
            Panel::right("stack")
                .resizable(false)
                .exact_size(110.0)
                .show(ui, |ui| self.stack(ui));
            CentralPanel::default().show(ui, |ui| self.disassembly(ui));
        });
    }

    fn registers(&mut self, ui: &mut Ui) {
        let cpu = &self.snapshot.cpu;
        Grid::new("cpu_registers")
            .num_columns(6)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                ui.strong("PC");
                ui.monospace(format!("${:04X}", cpu.pc));
                ui.strong("A");
                ui.monospace(format!("${:02X}", cpu.acc));
                ui.strong("SP");
                ui.monospace(format!("${:02X}", cpu.sp));
                ui.end_row();

                ui.strong("X");
                ui.monospace(format!("${:02X}", cpu.x));
                ui.strong("Y");
                ui.monospace(format!("${:02X}", cpu.y));
                ui.strong("Cycle");
                ui.monospace(cpu.cycle.to_string());
                ui.end_row();
            });

        ui.horizontal(|ui| {
            ui.strong("P");
            // Uppercase for set, lowercase and dimmed for clear, in NVUBDIZC order.
            for (flag, name) in [
                (Status::N, 'N'),
                (Status::V, 'V'),
                (Status::U, 'U'),
                (Status::B, 'B'),
                (Status::D, 'D'),
                (Status::I, 'I'),
                (Status::Z, 'Z'),
                (Status::C, 'C'),
            ] {
                let set = cpu.status.contains(flag);
                let text = if set {
                    RichText::new(name).monospace().strong()
                } else {
                    RichText::new(name.to_ascii_lowercase())
                        .monospace()
                        .color(Color32::DARK_GRAY)
                };
                ui.label(text);
            }
        });
    }

    /// The instructions that ran most recently, oldest first, ending just before PC.
    fn history(&mut self, ui: &mut Ui) {
        if self.snapshot.history.is_empty() {
            ui.weak("Nothing recorded yet - step or resume to start executing.");
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
                format!("{}  ×{repeats}", line.text)
            } else {
                line.text.clone()
            };
            ui.label(RichText::new(text).monospace().color(Color32::DARK_GRAY));
        }
    }

    /// The breakpoints, and the box that adds one.
    fn breakpoint_list(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.breakpoint_goto)
                    .hint_text("break at $addr")
                    .desired_width(90.0),
            );
            let addr = parse_addr(&self.breakpoint_goto);
            if let Some(addr) = addr
                && ui.button("Add").clicked()
            {
                self.breakpoint_goto.clear();
                self.breakpoints.add(addr);
                self.send_breakpoints();
            }
        });

        if self.breakpoints.is_empty() {
            ui.weak("None. Add one above, or click an instruction in the disassembly.");
            return;
        }

        // The list borrows itself for the walk, so what each row asks for is applied after it.
        let mut armed_changed = false;
        let mut removed = None;
        let mut scroll_to = None;
        for breakpoint in self.breakpoints.iter_mut() {
            ui.horizontal(|ui| {
                armed_changed |= ui.checkbox(&mut breakpoint.enabled, "").changed();
                let label = Label::new(
                    RichText::new(format!("${:04X}", breakpoint.addr))
                        .monospace()
                        .color(Color32::LIGHT_RED),
                )
                .sense(Sense::click());
                if ui
                    .add(label)
                    .on_hover_text("Show this address in the disassembly.")
                    .clicked()
                {
                    scroll_to = Some(breakpoint.addr);
                }
                if ui.small_button("✖").clicked() {
                    removed = Some(breakpoint.addr);
                }
            });
        }
        if let Some(addr) = removed {
            self.breakpoints.remove(addr);
            armed_changed = true;
        }
        if scroll_to.is_some() {
            self.scroll_to = scroll_to;
        }
        if armed_changed {
            self.send_breakpoints();
        }
    }

    fn disassembly(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.strong("Disassembly");
            ui.add(
                egui::TextEdit::singleline(&mut self.goto)
                    .hint_text("go to $addr")
                    .desired_width(90.0),
            );
            let addr = parse_addr(&self.goto);
            if let Some(addr) = addr
                && ui.button("Go").clicked()
            {
                self.scroll_to = Some(addr);
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
        let mut scroll_area = ScrollArea::vertical()
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

        let mut drawn = 0..0;
        let mut centered = false;
        let mut toggled = None;
        let breakpoints = &self.breakpoints;
        scroll_area.show_rows(
            ui,
            row_height,
            self.address_space.rows.len(),
            |ui, range| {
                drawn = range.clone();
                for (offset, row) in self.address_space.rows[range.clone()].iter().enumerate() {
                    let response = match row {
                        Row::Instruction(line) => {
                            let mut text = RichText::new(&line.text).monospace();
                            if line.addr == pc {
                                text = text.strong().background_color(Color32::DARK_BLUE);
                            }
                            // Tinted rather than given a marker column, so every row stays the
                            // same shape and the addresses down the left stay in one column.
                            if let Some(breakpoint) = breakpoints.get(line.addr) {
                                text = match (breakpoint.enabled, line.addr == pc) {
                                    // PC's own highlight has the background, so a breakpoint on
                                    // the instruction stopped at takes the text instead.
                                    (true, true) => text.color(Color32::LIGHT_RED),
                                    (true, false) => text.background_color(Color32::DARK_RED),
                                    // Listed, but not something the console will stop at.
                                    (false, _) => text.color(Color32::GRAY),
                                };
                            }
                            let response = ui.add(Label::new(text).sense(Sense::click()));
                            if response.clicked() {
                                toggled = Some(line.addr);
                            }
                            response
                        }
                        Row::Block { start, end, kind } => ui.label(
                            RichText::new(format!("${start:04X}-${end:04X}  {}", kind.label()))
                                .monospace()
                                .color(Color32::DARK_GRAY),
                        ),
                    };
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
        if let Some(addr) = toggled {
            self.breakpoints.toggle(addr);
            self.send_breakpoints();
        }
    }

    fn stack(&mut self, ui: &mut Ui) {
        ui.strong("Stack");
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
    use super::{AddressSpace, BlockKind, Breakpoints, Row, parse_addr};
    use tetanes_core::{control_deck::ControlDeck, cpu::Cpu};

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
        breakpoints.toggle(0xC000);
        assert_eq!(breakpoints.armed(), [0xC000]);

        breakpoints.toggle(0xC000);
        assert!(breakpoints.is_empty());
    }

    /// The list is drawn in the order it is kept, which is the order the disassembly reads in.
    #[test]
    fn breakpoints_are_held_in_address_order_however_they_are_added() {
        let mut breakpoints = Breakpoints::default();
        for addr in [0xE000, 0x8000, 0xC000] {
            breakpoints.add(addr);
        }
        assert_eq!(breakpoints.armed(), [0x8000, 0xC000, 0xE000]);
    }

    /// Adding is not toggling: typing an address that is already listed must not clear it.
    #[test]
    fn adding_an_address_twice_leaves_one_breakpoint() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.add(0xC000);
        breakpoints.add(0xC000);
        assert_eq!(breakpoints.armed(), [0xC000]);
    }

    /// Disabling keeps a breakpoint in the list and out of what the console is told to stop at.
    #[test]
    fn a_disabled_breakpoint_stays_listed_but_is_not_armed() {
        let mut breakpoints = Breakpoints::default();
        breakpoints.add(0xC000);
        breakpoints.add(0xD000);
        for breakpoint in breakpoints.iter_mut() {
            breakpoint.enabled = breakpoint.addr != 0xC000;
        }

        assert_eq!(breakpoints.armed(), [0xD000]);
        assert!(
            breakpoints.get(0xC000).is_some_and(|bp| !bp.enabled),
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

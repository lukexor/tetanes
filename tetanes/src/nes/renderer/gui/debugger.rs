use crate::nes::{
    event::{DebugRequest, EmulationEvent, NesEventProxy},
    renderer::gui::lib::ViewportOptions,
};
use egui::{
    CentralPanel, Color32, Context, Grid, Panel, RichText, ScrollArea, Ui, Vec2, ViewportClass,
    ViewportId,
};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tetanes_core::{
    bus::Bus,
    cpu::{Cpu, Status},
};

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
    /// Cart ROM that could not be decoded as instructions.
    Unknown,
}

impl BlockKind {
    /// The rendered label for a address block.
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
        // whose bank has since been swapped shows what currently lives at that address. Recording
        // the bytes as well as the address is what a code/data log would fix.
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
/// Rebuilt when the board's PRG mapping changes, since that is the only thing that can move
/// an instruction. The rows represent the currently mapped banks, not where PC happens to be.
#[derive(Debug, Default, Clone)]
#[must_use]
pub struct AddressSpace {
    pub rows: Vec<Row>,
}

impl AddressSpace {
    /// Capture the current address space into rows.
    ///
    /// Only mapped cart ROM is disassembled. Everything else is a collapsed block. Nothing yet
    /// records which bytes are instructions.
    pub fn capture(bus: &Bus) -> Self {
        let mut rows = Vec::new();
        let mut text = String::with_capacity(64);
        let mut addr = 0u32;
        let pc = bus.cpu.pc;

        while addr <= u32::from(u16::MAX) {
            let start = addr as u16;
            let next = match Self::block_kind(bus, start) {
                Some(kind) => {
                    let mut end = start;
                    while end < u16::MAX && Self::block_kind(bus, end + 1) == Some(kind) {
                        end += 1;
                    }
                    rows.push(Row::Block { start, end, kind });
                    u32::from(end) + 1
                }
                None => {
                    let mut next = start;
                    bus.disassemble_into(&mut next, &mut text);
                    // `disassemble_into` wraps past $FFFF, so take the length rather than the
                    // address it landed on, and never advance by zero.
                    let len = next.wrapping_sub(start).max(1);
                    let end = start.wrapping_add(len - 1);

                    // A decoded range has no way to know where instructions begin, so it decodes
                    // straight through data and stays misaligned until it happens to realign. PC is
                    // known to start an instruction, so a decode that overlaps it is wrong: give up
                    // on those bytes and resume there. This keeps PC on a row of its own for the
                    // view to highlight correctly, even if the instructions around it can't be
                    // decoded correctly.
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
            };
            // Every branch should progress by at least one address otherwise it'll spin forever.
            debug_assert!(next > addr, "sweep made no progress at ${start:04X}");
            addr = next.max(addr + 1);
        }

        Self { rows }
    }

    /// Whether `addr` collapses into a block. `None` means disassemble it.
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
    /// Set when a new capture or a step should scroll PC back into view.
    scroll_to_pc: bool,
    goto: String,
    /// An address the user asked to jump to, consumed by the next draw.
    goto_addr: Option<u16>,
    disasm_lines: u16,
    history_lines: u16,
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
                scroll_to_pc: true,
                goto: String::new(),
                goto_addr: None,
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
        state.scroll_to_pc |= state.snapshot.cpu.pc != snapshot.cpu.pc;
        state.snapshot = snapshot;
    }

    pub fn update_address_space(&mut self, address_space: AddressSpace) {
        let mut state = self.state.lock();
        state.address_space = address_space;
        state.scroll_to_pc = true;
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
    }

    fn ui(&mut self, ui: &mut Ui, enabled: bool) {
        ui.add_enabled_ui(enabled, |ui| {
            self.registers(ui);
            // Its own section rather than inline above PC: the disassembly is ordered by address
            // and this is ordered by time, so the two only coincide in straight-line code.
            egui::CollapsingHeader::new("Recently executed")
                .default_open(false)
                .show(ui, |ui| self.history(ui));
            ui.separator();
            // The stack is a fixed two columns of hex; the disassembly is the part that wants
            // every pixel it can get, so it takes what is left rather than an even half.
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

    fn disassembly(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.strong("Disassembly");
            ui.add(
                egui::TextEdit::singleline(&mut self.goto)
                    .hint_text("go to $addr")
                    .desired_width(90.0),
            );
            let addr = u16::from_str_radix(self.goto.trim().trim_start_matches('$'), 16);
            if let Ok(addr) = addr
                && ui.button("Go").clicked()
            {
                self.goto_addr = Some(addr);
            }
        });

        if self.address_space.rows.is_empty() {
            ui.weak("No ROM is loaded.");
            return;
        }

        let pc = self.snapshot.cpu.pc;
        let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
        // Rows have to stay exactly one line tall: `show_rows` maps scroll offset to row index by
        // multiplying, so a row that wraps desynchronises both the virtual window and the jump.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        let pitch = row_height + ui.spacing().item_spacing.y;
        let viewport_height = ui.available_height();
        let mut scroll_area = ScrollArea::vertical()
            .id_salt("disassembly")
            .auto_shrink([false, false]);

        // A row per instruction rather than per address, so the only way to scroll to an address
        // is to look up which row covers it.
        if let Some(target) = self
            .goto_addr
            .take()
            .or_else(|| self.scroll_to_pc.then_some(pc))
            && let Some(row) = self.address_space.row_at(target)
        {
            self.scroll_to_pc = false;
            // Centred rather than at the top: egui fades the first and last rows of a scroll area,
            // and the instructions either side of PC are the context worth having.
            let centred = (row as f32).mul_add(pitch, -(viewport_height - pitch) / 2.0);
            scroll_area = scroll_area.vertical_scroll_offset(centred.max(0.0));
        }

        scroll_area.show_rows(
            ui,
            row_height,
            self.address_space.rows.len(),
            |ui, range| {
                for row in &self.address_space.rows[range] {
                    match row {
                        Row::Instruction(line) => {
                            let text = RichText::new(&line.text).monospace();
                            if line.addr == pc {
                                ui.label(text.strong().background_color(Color32::DARK_BLUE));
                            } else {
                                ui.label(text);
                            }
                        }
                        Row::Block { start, end, kind } => {
                            ui.label(
                                RichText::new(format!("${start:04X}-${end:04X}  {}", kind.label()))
                                    .monospace()
                                    .color(Color32::DARK_GRAY),
                            );
                        }
                    }
                }
            },
        );
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
    use super::{AddressSpace, BlockKind, Row};
    use tetanes_core::control_deck::ControlDeck;

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

use crate::nes::{
    event::{DebugRequest, EmulationEvent, NesEventProxy},
    renderer::gui::lib::ViewportOptions,
};
use egui::{
    CentralPanel, Color32, Context, Grid, RichText, ScrollArea, Ui, Vec2, ViewportClass, ViewportId,
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

#[derive(Debug)]
#[must_use]
struct State {
    tx: NesEventProxy,
    snapshot: CpuSnapshot,
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
        self.state.lock().snapshot = snapshot;
    }

    pub fn show(&mut self, ui: &mut Ui, opts: ViewportOptions) {
        if !self.open.load(Ordering::Relaxed) {
            return;
        }

        let open = Arc::clone(&self.open);
        let state = Arc::clone(&self.state);

        let mut viewport_builder = egui::ViewportBuilder::default()
            .with_title(Self::TITLE)
            .with_inner_size(Vec2::new(640.0, 720.0));
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
            ui.separator();
            ui.columns(2, |columns| {
                self.disassembly(&mut columns[0]);
                self.stack(&mut columns[1]);
            });
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

    fn disassembly(&mut self, ui: &mut Ui) {
        ui.strong("Disassembly");
        ScrollArea::vertical()
            .id_salt("disassembly")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.snapshot.disasm.is_empty() {
                    ui.weak("No ROM is loaded.");
                    return;
                }
                // What already ran, dimmed to separate it from what is about to.
                // The newest recorded instruction is the one that advanced PC to the line
                // highlighted below it.
                for line in &self.snapshot.history {
                    ui.label(
                        RichText::new(&line.text)
                            .monospace()
                            .color(Color32::DARK_GRAY),
                    );
                }
                let pc = self.snapshot.cpu.pc;
                for line in &self.snapshot.disasm {
                    let text = RichText::new(&line.text).monospace();
                    if line.addr == pc {
                        ui.label(text.strong().background_color(Color32::DARK_BLUE));
                    } else {
                        ui.label(text);
                    }
                }
            });
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

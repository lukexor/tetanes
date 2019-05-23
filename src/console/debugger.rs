use crate::console::cpu::{Cpu, Interrupt};

pub struct Debugger {
    enabled: bool,         // Whether debugger is enabled at all or not
    tracing: bool,         // Whether we want to print each CPU instruction
    breakpoint: u64,       // A specific CPU instruction step to break at
    current_step: u64,     // Current CPU instruction we're at
    steps: u64,            // Number of CPU instructions to step through
    break_type: BreakType, // Type of breakpoint
}
#[derive(PartialEq, Eq, Debug)]
enum BreakType {
    Unset,
    Step,
    NMI,
    IRQ,
}
use BreakType::*;

impl Debugger {
    pub fn new() -> Self {
        Self {
            enabled: false,
            tracing: true,
            breakpoint: 0u64,
            current_step: 0u64,
            steps: 0u64,
            break_type: Unset,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn start(&mut self) {
        self.enabled = true;
        self.steps = 1;
        self.break_type = Step;
    }

    pub fn stop(&mut self) {
        self.enabled = false;
        self.steps = 0;
        self.break_type = Unset;
    }

    pub fn on_step(&mut self, cpu: &mut Cpu, opcode: u8, num_args: u8, disasm: String) {
        if self.tracing && (self.break_type == Step || cpu.interrupt != Interrupt::None) {
            cpu.print_instruction(opcode, num_args, disasm);
        }
        self.current_step = cpu.step;
        if self.enabled && self.break_type == Step {
            if self.steps > 0 {
                self.steps -= 1;
                if self.steps == 0 {
                    self.prompt();
                }
                return;
            } else if self.breakpoint > 0 && self.breakpoint == self.current_step {
                self.prompt();
                self.breakpoint = 0;
            }
        }
    }

    pub fn on_nmi(&mut self, cpu: &Cpu) {
        self.current_step = cpu.step;
        if self.enabled && self.break_type == NMI {
            eprintln!("DEBUG - VBLANK");
            self.prompt();
        }
    }

    pub fn on_irq(&mut self, cpu: &Cpu) {
        self.current_step = cpu.step;
        if self.enabled && self.break_type == IRQ {
            eprintln!("DEBUG - SCANLINE");
            self.prompt();
        }
    }

    fn prompt(&mut self) {
        let mut input = String::new();
        eprint!("debugger (step: {}) > ", self.current_step);
        match std::io::stdin().read_line(&mut input) {
            Ok(bytes) => match input.trim() {
                "h" => self.usage(),
                "q" => self.enabled = false,
                "s" => {
                    self.steps = 1;
                    self.break_type = Step;
                }
                "" => {
                    // Ctrl-D was pressed
                    if bytes == 0 {
                        std::process::exit(0);
                    }
                    if self.break_type == Step {
                        self.steps = 1;
                    }
                }
                "c" => {
                    if self.breakpoint == 0 {
                        self.break_type = Unset;
                    }
                }
                "nmi" => self.break_type = NMI,
                "irq" => self.break_type = IRQ,
                cmd => {
                    if cmd.starts_with('b') {
                        self.break_type = Step;
                        self.set_breakpoint(cmd);
                        self.prompt();
                    } else if cmd.starts_with('c') {
                        self.break_type = Step;
                        self.set_breakpoint(cmd);
                    } else if cmd.starts_with('s') {
                        self.break_type = Step;
                        self.set_steps(cmd);
                    } else {
                        eprintln!("unknown command {:?}", cmd);
                        self.prompt();
                    }
                }
            },
            Err(x) => panic!("error reading input: {}", x),
        }
    }

    fn usage(&mut self) {
        eprintln!(
            "List of commands:
    h         This help
    q         Disable debugger
    b <step>  Set a breakpoint on a given CPU step
    s [steps] Step CPU [steps] (defaults to 1)
    c [step]  Continue CPU execution until [step] or the next breakpoint (if any)
    nmi       Step until the next NMI (Vertical Blank)
    irq       Step until the next IRQ (Horizontal Blank/Scanline)
    <Enter>   Repeat the last command
"
        );
        self.prompt();
    }

    fn set_breakpoint(&mut self, cmd: &str) {
        let bp = self.extract_num(cmd);
        if bp > 0 {
            self.breakpoint = bp;
        } else {
            self.usage();
        }
    }

    fn set_steps(&mut self, cmd: &str) {
        let steps = self.extract_num(cmd);
        if steps > 0 {
            self.steps = steps;
        } else {
            self.usage();
        }
    }

    fn extract_num(&mut self, cmd: &str) -> u64 {
        if cmd.len() > 2 {
            let (_, num) = cmd.split_at(2);
            if let Ok(num) = num.parse::<u64>() {
                return num;
            }
        }
        0
    }
}

impl Default for Debugger {
    fn default() -> Self {
        Self::new()
    }
}

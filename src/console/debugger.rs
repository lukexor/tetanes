pub struct Debugger {
    enabled: bool,
    breakpoint: u64,
    current_step: u64,
    steps: u64,
    break_type: BreakType,
}
#[derive(PartialEq, Eq, Debug)]
enum BreakType {
    Unset,
    Step,
    NMI,
    IRQ,
}

impl Debugger {
    pub fn new() -> Self {
        Self {
            enabled: false,
            breakpoint: 0u64,
            current_step: 0u64,
            steps: 0u64,
            break_type: BreakType::Unset,
        }
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn start(&mut self) {
        self.enabled = true;
        self.steps = 1;
        self.break_type = BreakType::Step;
    }

    pub fn stop(&mut self) {
        self.enabled = false;
        self.steps = 0;
        self.break_type = BreakType::Unset;
    }

    pub fn on_step(&mut self, step: u64) {
        self.current_step = step;
        if self.enabled && self.break_type == BreakType::Step {
            if self.steps > 0 {
                self.steps -= 1;
                if self.steps == 0 {
                    self.prompt();
                }
                return;
            } else if self.breakpoint > 0 {
                if self.breakpoint == step {
                    self.prompt();
                    self.breakpoint = 0;
                }
            }
        }
    }

    pub fn on_nmi(&mut self, step: u64) {
        self.current_step = step;
        if self.enabled && self.break_type == BreakType::NMI {
            eprintln!("DEBUG - VBLANK");
            self.prompt();
        }
    }

    pub fn on_irq(&mut self, step: u64) {
        self.current_step = step;
        if self.enabled && self.break_type == BreakType::IRQ {
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
                "q" => std::process::exit(0),
                "" | "s" => {
                    // Ctrl-D was pressed
                    if bytes == 0 {
                        std::process::exit(0);
                    }
                    self.steps = 1;
                    self.break_type = BreakType::Step;
                }
                "c" => {
                    if self.breakpoint == 0 {
                        self.break_type = BreakType::Unset;
                    }
                }
                "nmi" => self.break_type = BreakType::NMI,
                "irq" => self.break_type = BreakType::IRQ,
                cmd => {
                    if cmd.starts_with("b") {
                        self.break_type = BreakType::Step;
                        self.set_breakpoint(cmd);
                        self.prompt();
                    } else if cmd.starts_with("c") {
                        self.break_type = BreakType::Step;
                        self.set_breakpoint(cmd);
                    } else if cmd.starts_with("s") {
                        self.break_type = BreakType::Step;
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
    q         Quit program
    b <step>  Set a breakpoint on a given CPU step
    s [steps] Step CPU [steps] (defaults to 1)
    c [step]  Continue CPU execution until [step] or the next breakpoint (if any)
    nmi       Step until the next NMI (Vertical Blank)
    irq       Step until the next IRQ (Horizontal Blank/Scanline)
    <Enter>   Shortcut for s
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

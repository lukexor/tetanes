pub trait Powered {
    fn reset(&mut self) {}
    fn power_cycle(&mut self) {
        self.reset();
    }
}

pub trait Clocked {
    fn clock(&mut self) -> u64 {
        0
    }
}

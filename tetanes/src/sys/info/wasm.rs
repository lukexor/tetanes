use crate::sys::{SystemInfo, SystemStats};

#[derive(Default, Debug)]
pub(crate) struct System {}

impl SystemInfo for System {
    fn update(&mut self) {}

    fn stats(&self) -> Option<SystemStats> {
        None
    }
}

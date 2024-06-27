pub mod info;
pub mod logging;
pub mod platform;
pub mod thread;

#[derive(Debug)]
pub struct DiskUsage {
    pub read_bytes: u64,
    pub total_read_bytes: u64,
    pub written_bytes: u64,
    pub total_written_bytes: u64,
}

#[derive(Debug)]
pub struct SystemStats {
    pub cpu_usage: f32,
    pub memory: u64,
    pub disk_usage: DiskUsage,
}

pub trait SystemInfo {
    fn update(&mut self);
    fn stats(&self) -> Option<SystemStats>;
}

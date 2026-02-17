pub(crate) mod info;
pub(crate) mod logging;
pub(crate) mod platform;
pub(crate) mod thread;

#[derive(Debug)]
pub(crate) struct DiskUsage {
    pub(crate) read_bytes: u64,
    pub(crate) total_read_bytes: u64,
    pub(crate) written_bytes: u64,
    pub(crate) total_written_bytes: u64,
}

#[derive(Debug)]
pub(crate) struct SystemStats {
    pub(crate) cpu_usage: f32,
    pub(crate) memory: u64,
    pub(crate) disk_usage: DiskUsage,
}

pub(crate) trait SystemInfo {
    fn update(&mut self);
    fn stats(&self) -> Option<SystemStats>;
}

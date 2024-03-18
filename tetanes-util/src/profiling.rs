// TODO: Add custom profiling similar to puffin

pub fn init() {
    #[cfg(feature = "profiling")]
    enable(true);
}

#[cfg(feature = "profiling")]
pub fn enable(enabled: bool) {}

use crate::sys::{DiskUsage, SystemInfo, SystemStats};
use std::time::{Duration, Instant};
use sysinfo::{ProcessRefreshKind, RefreshKind};

#[derive(Debug)]
pub struct System {
    sys: Option<sysinfo::System>,
    updated: Instant,
}

impl Default for System {
    fn default() -> Self {
        let sys = if sysinfo::IS_SUPPORTED_SYSTEM {
            let mut sys = sysinfo::System::new_with_specifics(
                RefreshKind::nothing().with_processes(
                    ProcessRefreshKind::nothing()
                        .with_cpu()
                        .with_memory()
                        .with_disk_usage(),
                ),
            );
            sys.refresh_specifics(
                RefreshKind::nothing().with_processes(
                    ProcessRefreshKind::nothing()
                        .with_cpu()
                        .with_memory()
                        .with_disk_usage(),
                ),
            );
            Some(sys)
        } else {
            None
        };

        Self {
            sys,
            updated: Instant::now(),
        }
    }
}

impl SystemInfo for System {
    fn update(&mut self) {
        if let Some(sys) = &mut self.sys {
            // NOTE: refreshing sysinfo is cpu-intensive if done too frequently and skews the
            // results
            let update_interval = Duration::from_secs(1);
            assert!(update_interval > sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
            if self.updated.elapsed() >= update_interval {
                sys.refresh_specifics(
                    sysinfo::RefreshKind::nothing().with_processes(
                        sysinfo::ProcessRefreshKind::nothing()
                            .with_cpu()
                            .with_memory()
                            .with_disk_usage(),
                    ),
                );
                self.updated = Instant::now();
            }
        }
    }

    fn stats(&self) -> Option<SystemStats> {
        self.sys
            .as_ref()
            .and_then(|sys| sys.process(sysinfo::Pid::from_u32(std::process::id())))
            .map(|proc| {
                let du = proc.disk_usage();
                SystemStats {
                    cpu_usage: proc.cpu_usage(),
                    memory: proc.memory(),
                    disk_usage: DiskUsage {
                        read_bytes: du.read_bytes,
                        total_read_bytes: du.total_read_bytes,
                        written_bytes: du.written_bytes,
                        total_written_bytes: du.total_written_bytes,
                    },
                }
            })
    }
}

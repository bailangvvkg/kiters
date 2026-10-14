//! The instruments: a CPU heat map of the whole build.
//!
//! The sampling is pprof-rs, which rides POSIX signals — a Unix thing,
//! and the deploy target is a Unix thing too, so there it samples and
//! here it says so plainly. The output is Go's pprof format on purpose:
//! `go tool pprof` reads it, so one viewer looks at both the Go kite and
//! the Rust kiters.

use std::path::Path;

/// What a build was measured with, if it was measured.
pub struct Instruments {
    #[cfg(unix)]
    guard: Option<pprof::ProfilerGuard<'static>>,
}

impl Instruments {
    /// Starts sampling, whole-build: called before the first file is
    /// read, when either output was asked for.
    pub fn start(wanted: bool) -> Self {
        Instruments {
            #[cfg(unix)]
            guard: wanted
                .then(|| {
                    pprof::ProfilerGuardBuilder::default()
                        .frequency(997)
                        .blocklist(&["libc", "pthread"])
                        .build()
                        .ok()
                })
                .flatten(),
        }
    }

    /// Reads the sampling out, after the last file is written.
    pub fn finish(&self, cpuprofile: Option<&Path>, flamegraph: Option<&Path>) -> anyhow::Result<()> {
        #[cfg(unix)]
        if let Some(guard) = &self.guard {
            use pprof::protos::Message;
            let report = guard
                .report()
                .build()
                .map_err(|e| anyhow::anyhow!("reading the sampling: {}", e))?;
            if let Some(file) = cpuprofile {
                let profile = report
                    .pprof()
                    .map_err(|e| anyhow::anyhow!("writing {}: {}", file.display(), e))?;
                std::fs::write(file, profile.encode_to_vec())?;
                println!("CPU 热图: {} (go tool pprof 可读)", file.display());
            }
            if let Some(file) = flamegraph {
                let mut out = std::fs::File::create(file)?;
                report
                    .flamegraph(&mut out)
                    .map_err(|e| anyhow::anyhow!("writing {}: {}", file.display(), e))?;
                println!("火焰图: {}", file.display());
            }
        }
        #[cfg(not(unix))]
        if cpuprofile.is_some() || flamegraph.is_some() {
            eprintln!("此平台不支持 CPU 采样(pprof-rs 是 POSIX 信号采样,Linux 可用)");
        }
        Ok(())
    }
}

/// The process's peak resident memory, as the operating system counts
/// it; Linux says it in `/proc`, other platforms say they are not
/// saying.
pub fn peak_rss() -> String {
    #[cfg(target_os = "linux")]
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmHWM:") {
                return format!("{} (/proc/self/status VmHWM)", rest.trim());
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        "此平台暂不可读(Linux 读 /proc VmHWM)".to_string()
    }
    #[cfg(target_os = "linux")]
    {
        "未知".to_string()
    }
}

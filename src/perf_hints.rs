/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Host scheduling and power-management hints for the emulator thread.
//!
//! Everything the emulator does — the JIT'd guest code for every guest
//! thread, the host implementations of the frameworks, GL submission and the
//! presentation of frames — runs on one host thread. On Android that thread
//! typically spends a well-defined chunk of each display refresh busy and
//! then sleeps until the next frame (the emulator paces frames itself, see
//! `--fps-limit`). The CPU frequency governor sees that idle time as
//! headroom and clocks the core down; the busy chunk then grows into the
//! whole interval, frames start to miss it, and the game drops to half the
//! refresh rate even though the device could easily have kept up.
//!
//! Two standard remedies, both Android-only and both harmless no-ops
//! elsewhere:
//!
//! - **ADPF performance hint session**: report the per-frame CPU time of the
//!   emulator thread to the OS together with the frame-time target, through
//!   the NDK's `APerformanceHint_*` API (Android 13+, loaded dynamically so
//!   older releases are unaffected). The power HAL then keeps the core
//!   clocked for the reported load instead of for the idle time between
//!   frames. This is what games use for the same reason.
//! - **Thread priority**: raise the emulator thread to Android's "display"
//!   priority band (the same one the UI toolkit's render thread runs at),
//!   so background work on the device doesn't steal time slices from it.
//!
//! Both can be switched off with `--no-perf-hints` / `TOUCHHLE_PERF_HINTS=0`.

use std::time::Duration;

/// Handle for the hints. Create it on the emulator thread, then call
/// [PerfHints::frame_presented] once per presented frame from that thread.
pub struct PerfHints {
    #[cfg(target_os = "android")]
    inner: Option<android::HintSession>,
}

impl PerfHints {
    /// `target_frame_time` is the frame interval the emulator paces to (or
    /// the display's interval if it doesn't pace). `affinity` comes from
    /// `--affinity=` (`big` / `all` / `off` / a CPU list); the
    /// `TOUCHHLE_AFFINITY` environment variable, when set, takes precedence.
    pub fn new(enabled: bool, target_frame_time: Duration, affinity: Option<&str>) -> PerfHints {
        #[cfg(not(target_os = "android"))]
        {
            let _ = (enabled, target_frame_time, affinity);
            PerfHints {}
        }
        #[cfg(target_os = "android")]
        {
            if !enabled {
                log!("Performance hints disabled (--no-perf-hints).");
                return PerfHints { inner: None };
            }
            android::raise_thread_priority();
            android::pin_to_big_cores(affinity);
            PerfHints {
                inner: android::HintSession::open(target_frame_time),
            }
        }
    }

    /// Report that a frame was just presented. Cheap when hints are
    /// unavailable or disabled.
    #[inline]
    pub fn frame_presented(&mut self) {
        #[cfg(target_os = "android")]
        {
            if let Some(session) = self.inner.as_mut() {
                session.frame_presented();
            }
        }
    }
}

#[cfg(any(target_os = "android", test))]
fn select_high_capacity_cores(values: &[(usize, u64)]) -> Option<Vec<usize>> {
    if values.len() < 2 {
        return None;
    }
    let max = values.iter().map(|(_, capacity)| *capacity).max()?;
    let threshold = max.saturating_sub(max / 10);
    let cores: Vec<usize> = values
        .iter()
        .filter(|(_, capacity)| *capacity >= threshold)
        .map(|(cpu, _)| *cpu)
        .collect();
    if cores.len() == values.len() {
        None
    } else {
        Some(cores)
    }
}

#[cfg(any(target_os = "android", test))]
fn arm_core_performance_cores(cpuinfo: &str) -> Option<Vec<usize>> {
    let mut records = Vec::new();
    let mut processor = None;
    let mut implementer = None;
    let mut part = None;

    for line in cpuinfo.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "processor" => {
                if processor.is_some() {
                    records.push((processor.take()?, implementer.take()?, part.take()?));
                }
                processor = value.trim().parse::<usize>().ok();
                if processor.is_none() {
                    return None;
                }
            }
            "CPU implementer" => implementer = parse_cpuinfo_hex(value),
            "CPU part" => part = parse_cpuinfo_hex(value),
            _ => {}
        }
    }

    if processor.is_some() {
        records.push((processor.take()?, implementer.take()?, part.take()?));
    }
    if records.len() < 2 {
        return None;
    }

    let mut high_performance_cores = Vec::new();
    let mut has_efficiency_core = false;
    for (processor, implementer, part) in records {
        if arm_core_is_high_performance(implementer, part)? {
            high_performance_cores.push(processor);
        } else {
            has_efficiency_core = true;
        }
    }

    if high_performance_cores.is_empty() || !has_efficiency_core {
        None
    } else {
        Some(high_performance_cores)
    }
}

#[cfg(any(target_os = "android", test))]
fn parse_cpuinfo_hex(value: &str) -> Option<u32> {
    let value = value.trim();
    let value = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    u32::from_str_radix(value, 16).ok()
}

#[cfg(any(target_os = "android", test))]
fn arm_core_is_high_performance(implementer: u32, part: u32) -> Option<bool> {
    if implementer != 0x41 {
        return None;
    }
    match part {
        0xd03 | 0xd04 | 0xd05 | 0xd06 | 0xd46 | 0xd80 | 0xd8f => Some(false),
        0xd07 | 0xd08 | 0xd09 | 0xd0a | 0xd0b | 0xd0c | 0xd0d | 0xd0e | 0xd41 | 0xd42 | 0xd44
        | 0xd47 | 0xd48 | 0xd49 | 0xd4b | 0xd4c | 0xd4d | 0xd4e | 0xd4f | 0xd81 | 0xd82 | 0xd83
        | 0xd84 | 0xd85 | 0xd87 | 0xd89 | 0xd8e => Some(true),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{arm_core_performance_cores, select_high_capacity_cores};

    #[test]
    fn selects_high_capacity_cluster() {
        assert_eq!(
            select_high_capacity_cores(&[(0, 512), (1, 512), (2, 1024), (3, 1024)]),
            Some(vec![2, 3])
        );
    }

    #[test]
    fn includes_cores_within_ten_percent_of_maximum() {
        assert_eq!(
            select_high_capacity_cores(&[(0, 100), (1, 90), (2, 89)]),
            Some(vec![0, 1])
        );
    }

    #[test]
    fn leaves_homogeneous_or_unknown_topology_unpinned() {
        assert_eq!(select_high_capacity_cores(&[(0, 512), (1, 512)]), None);
        assert_eq!(select_high_capacity_cores(&[(0, 512)]), None);
        assert_eq!(select_high_capacity_cores(&[]), None);
    }

    #[test]
    fn selects_cortex_a75_cores_when_frequency_limits_are_identical() {
        let mut cpuinfo = String::new();
        for processor in 0..8 {
            let part = if processor < 6 { 0xd05 } else { 0xd0a };
            cpuinfo.push_str(&format!(
                "processor : {processor}\nCPU implementer : 0x41\nCPU part : 0x{part:x}\n\n"
            ));
        }
        assert_eq!(arm_core_performance_cores(&cpuinfo), Some(vec![6, 7]));
    }

    #[test]
    fn does_not_guess_for_homogeneous_unknown_or_incomplete_cpuinfo() {
        let homogeneous = "processor : 0\nCPU implementer : 0x41\nCPU part : 0xd05\n\nprocessor : 1\nCPU implementer : 0x41\nCPU part : 0xd05";
        let unknown = "processor : 0\nCPU implementer : 0x51\nCPU part : 0xd05\n\nprocessor : 1\nCPU implementer : 0x51\nCPU part : 0xd0a";
        let incomplete = "processor : 0\nCPU implementer : 0x41\n\nprocessor : 1\nCPU implementer : 0x41\nCPU part : 0xd0a";
        assert_eq!(arm_core_performance_cores(homogeneous), None);
        assert_eq!(arm_core_performance_cores(unknown), None);
        assert_eq!(arm_core_performance_cores(incomplete), None);
    }
}

#[cfg(target_os = "android")]
mod android {
    use std::ffi::{c_int, c_void, CStr};
    use std::time::{Duration, Instant};

    /// `THREAD_PRIORITY_URGENT_DISPLAY` from `android.os.Process`: the band
    /// used for compositing the screen and delivering input. Apps may use it
    /// for their own threads; SurfaceFlinger and the input pipeline run at
    /// real-time priorities anyway, so this can't starve them.
    const EMULATOR_THREAD_NICE: c_int = -8;

    pub fn raise_thread_priority() {
        // SAFETY: plain libc calls with valid arguments; `gettid` has no
        // preconditions and `setpriority` on our own thread is always
        // permitted to fail gracefully.
        unsafe {
            let tid = ::libc::gettid();
            let before = ::libc::getpriority(::libc::PRIO_PROCESS, tid as ::libc::id_t);
            if ::libc::setpriority(
                ::libc::PRIO_PROCESS,
                tid as ::libc::id_t,
                EMULATOR_THREAD_NICE,
            ) == 0
            {
                log!(
                    "Emulator thread priority raised (nice {} -> {}).",
                    before,
                    EMULATOR_THREAD_NICE
                );
            } else {
                log!(
                    "Could not raise the emulator thread priority (nice {}): {}",
                    before,
                    std::io::Error::last_os_error()
                );
            }
        }
    }

    /// Pin the calling thread to the "big" CPU cluster: the cores whose
    /// maximum frequency matches the fastest core on the device (typically a
    /// big.LITTLE or tri-cluster SoC where the scheduler is otherwise free to
    /// migrate the single hot emulator thread between clusters, losing L2/L3
    /// locality and risking idle-clock time on the slower cores).
    ///
    /// Cores are discovered from sysfs (`cpufreq/cpuinfo_max_freq`); every
    /// core within 10% of the fastest core's frequency is included, which
    /// keeps a homogeneous (all-big or all-mid) device un-pinned when the
    /// "cluster" would be the whole CPU anyway.
    ///
    /// `affinity_override` comes from `--affinity=`; the `TOUCHHLE_AFFINITY`
    /// environment variable, when set, takes precedence over it.
    pub fn pin_to_big_cores(affinity_override: Option<&str>) {
        let override_ = std::env::var("TOUCHHLE_AFFINITY")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| affinity_override.map(|v| v.to_string()))
            .unwrap_or_default();
        let override_ = override_.trim();
        if override_.is_empty() || override_.eq_ignore_ascii_case("big") {
            if let Some(cores) = big_core_cpus() {
                if set_affinity(&cores, "big cluster") {
                    return;
                }
            }
            return;
        }
        if override_.eq_ignore_ascii_case("off") || override_.eq_ignore_ascii_case("all") {
            log!("CPU affinity override: staying on all cores.");
            return;
        }
        let cores: Vec<usize> = override_
            .split(',')
            .flat_map(|part| match part.split_once('-') {
                Some((a, b)) => {
                    let a: usize = a.trim().parse().unwrap_or(usize::MAX);
                    let b: usize = b.trim().parse().unwrap_or(usize::MAX);
                    (a..=b).collect::<Vec<usize>>()
                }
                None => match part.trim().parse() {
                    Ok(cpu) => vec![cpu],
                    Err(_) => Vec::new(),
                },
            })
            .collect();
        if cores.is_empty() {
            log!("TOUCHHLE_AFFINITY={override_}: no valid CPU list, ignoring.");
            return;
        }
        set_affinity(&cores, &format!("CPU list {override_}"));
    }

    fn scheduler_capacity_cores() -> Option<Vec<usize>> {
        let mut capacities = Vec::new();
        for cpu in 0..64 {
            let path = format!("/sys/devices/system/cpu/cpu{cpu}/cpu_capacity");
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(capacity) = text.trim().parse::<u64>() {
                    capacities.push((cpu, capacity));
                }
            }
        }
        super::select_high_capacity_cores(&capacities)
    }

    /// Discover the fastest cluster without guessing when the host exposes no topology.
    fn big_core_cpus() -> Option<Vec<usize>> {
        if let Some(cores) = scheduler_capacity_cores() {
            log!(
                "CPU affinity: scheduler capacity selected high-performance cores {:?}.",
                cores
            );
            return Some(cores);
        }

        let mut freqs: Vec<(usize, u64)> = Vec::new();
        for cpu in 0..64 {
            let path = format!("/sys/devices/system/cpu/cpu{cpu}/cpufreq/cpuinfo_max_freq");
            if let Ok(text) = std::fs::read_to_string(path) {
                if let Ok(khz) = text.trim().parse::<u64>() {
                    freqs.push((cpu, khz));
                }
            }
        }
        if freqs.len() >= 2 {
            let max = freqs.iter().map(|(_, f)| *f).max()?;
            let big: Vec<usize> = freqs
                .iter()
                .filter(|(_, f)| *f * 10 >= max * 9)
                .map(|(cpu, _)| *cpu)
                .collect();
            if big.len() < freqs.len() {
                return Some(big);
            }
            log!(
                "CPU affinity: all {} cores report the same top frequency; checking ARM core types.",
                freqs.len()
            );
        }

        let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").ok()?;
        let cores = super::arm_core_performance_cores(&cpuinfo)?;
        log!(
            "CPU affinity: ARM CPU part IDs selected high-performance cores {:?}.",
            cores
        );
        Some(cores)
    }

    /// Apply a CPU affinity mask to the calling thread. Logging is best
    /// effort: affinity is an optimization, never a correctness requirement.
    fn set_affinity(cores: &[usize], what: &str) -> bool {
        let cpu_set_size = ::libc::CPU_SETSIZE as usize;
        if cores.is_empty() {
            log!(
                "Could not pin the emulator thread to the {}: empty CPU set.",
                what
            );
            return false;
        }
        if let Some(&cpu) = cores.iter().find(|&&cpu| cpu >= cpu_set_size) {
            log!(
                "Could not pin the emulator thread to the {}: CPU {} exceeds this process's {}-CPU affinity mask.",
                what,
                cpu,
                cpu_set_size
            );
            return false;
        }

        // SAFETY: each CPU index is below CPU_SETSIZE and `set` is zeroed.
        unsafe {
            let mut set: ::libc::cpu_set_t = std::mem::zeroed();
            ::libc::CPU_ZERO(&mut set);
            for &cpu in cores {
                ::libc::CPU_SET(cpu, &mut set);
            }
            let res = ::libc::sched_setaffinity(0, std::mem::size_of::<::libc::cpu_set_t>(), &set);
            if res == 0 {
                log!(
                    "Emulator thread pinned to the {} (cores {:?}).",
                    what,
                    cores
                );
                true
            } else {
                log!(
                    "Could not pin the emulator thread to the {} (cores {:?}): {}",
                    what,
                    cores,
                    std::io::Error::last_os_error()
                );
                false
            }
        }
    }

    /// Opaque NDK types.
    type APerformanceHintManager = c_void;
    type APerformanceHintSession = c_void;

    /// The subset of `<android/performance_hint.h>` (API 33+) we use,
    /// resolved at runtime from `libandroid.so`.
    struct Api {
        get_manager: unsafe extern "C" fn() -> *mut APerformanceHintManager,
        get_preferred_update_rate_nanos: unsafe extern "C" fn(*mut APerformanceHintManager) -> i64,
        create_session: unsafe extern "C" fn(
            *mut APerformanceHintManager,
            *const i32,
            usize,
            i64,
        ) -> *mut APerformanceHintSession,
        report_actual_work_duration:
            unsafe extern "C" fn(*mut APerformanceHintSession, i64) -> c_int,
        close_session: unsafe extern "C" fn(*mut APerformanceHintSession),
    }

    impl Api {
        fn load() -> Option<Api> {
            // SAFETY: dlopen/dlsym with valid NUL-terminated names; the
            // function pointer types match the NDK header declarations. The
            // handle is intentionally leaked: libandroid.so stays loaded for
            // the life of the process anyway (SDL links against it).
            unsafe {
                let lib = ::libc::dlopen(c"libandroid.so".as_ptr(), ::libc::RTLD_NOW);
                if lib.is_null() {
                    return None;
                }
                let sym = |name: &CStr| -> Option<*mut c_void> {
                    let p = ::libc::dlsym(lib, name.as_ptr());
                    if p.is_null() {
                        None
                    } else {
                        Some(p)
                    }
                };
                Some(Api {
                    get_manager: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn() -> *mut APerformanceHintManager,
                    >(sym(c"APerformanceHint_getManager")?),
                    get_preferred_update_rate_nanos: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(*mut APerformanceHintManager) -> i64,
                    >(sym(
                        c"APerformanceHint_getPreferredUpdateRateNanos",
                    )?),
                    create_session: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(
                            *mut APerformanceHintManager,
                            *const i32,
                            usize,
                            i64,
                        )
                            -> *mut APerformanceHintSession,
                    >(sym(c"APerformanceHint_createSession")?),
                    report_actual_work_duration: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(*mut APerformanceHintSession, i64) -> c_int,
                    >(sym(
                        c"APerformanceHint_reportActualWorkDuration",
                    )?),
                    close_session: std::mem::transmute::<
                        *mut c_void,
                        unsafe extern "C" fn(*mut APerformanceHintSession),
                    >(sym(c"APerformanceHint_closeSession")?),
                })
            }
        }
    }

    /// CPU time consumed by the calling thread so far.
    fn thread_cpu_time() -> Duration {
        // SAFETY: an all-zero timespec is a valid value (and avoids naming
        // the padding fields the libc crate adds on some targets).
        let mut ts: ::libc::timespec = unsafe { std::mem::zeroed() };
        // SAFETY: valid out-pointer; CLOCK_THREAD_CPUTIME_ID always exists on
        // Android.
        let res = unsafe { ::libc::clock_gettime(::libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
        if res != 0 {
            return Duration::ZERO;
        }
        Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
    }

    pub struct HintSession {
        api: Api,
        session: *mut APerformanceHintSession,
        /// Minimum spacing between reports the HAL wants.
        min_report_interval: Duration,
        last_report: Option<Instant>,
        /// Thread CPU time at the previous report.
        cpu_time_at_last_report: Duration,
        /// Frames since the previous report.
        frames_since_report: u32,
        errors_logged: u32,
    }

    impl HintSession {
        pub fn open(target_frame_time: Duration) -> Option<HintSession> {
            let api = Api::load()?;
            // SAFETY: the function pointers were resolved from libandroid.so
            // and are called with the argument shapes the NDK documents.
            unsafe {
                let manager = (api.get_manager)();
                if manager.is_null() {
                    log!("ADPF performance hints unavailable on this device (no hint manager).");
                    return None;
                }
                let preferred_rate = (api.get_preferred_update_rate_nanos)(manager);
                let tid = ::libc::gettid();
                let session = (api.create_session)(
                    manager,
                    &tid,
                    1,
                    target_frame_time.as_nanos().min(i64::MAX as u128) as i64,
                );
                if session.is_null() {
                    log!("ADPF performance hints unavailable on this device (could not create a hint session).");
                    return None;
                }
                let min_report_interval = if preferred_rate > 0 {
                    Duration::from_nanos(preferred_rate as u64)
                } else {
                    Duration::ZERO
                };
                log!(
                    "ADPF performance hint session opened for the emulator thread (target {:.2} ms per frame, preferred update rate {:.2} ms).",
                    target_frame_time.as_secs_f64() * 1000.0,
                    min_report_interval.as_secs_f64() * 1000.0
                );
                Some(HintSession {
                    api,
                    session,
                    min_report_interval,
                    last_report: None,
                    cpu_time_at_last_report: thread_cpu_time(),
                    frames_since_report: 0,
                    errors_logged: 0,
                })
            }
        }

        pub fn frame_presented(&mut self) {
            self.frames_since_report += 1;
            let now = Instant::now();
            if let Some(last) = self.last_report {
                if now.duration_since(last) < self.min_report_interval {
                    return;
                }
            }
            let cpu_now = thread_cpu_time();
            let busy = cpu_now.saturating_sub(self.cpu_time_at_last_report);
            // Average CPU time per frame since the last report. Normally the
            // report interval is shorter than a frame, so this is simply the
            // last frame's CPU time.
            let per_frame = busy / self.frames_since_report.max(1);
            self.cpu_time_at_last_report = cpu_now;
            self.frames_since_report = 0;
            self.last_report = Some(now);
            // The HAL rejects zero durations.
            let nanos = per_frame.as_nanos().clamp(1, i64::MAX as u128) as i64;
            // SAFETY: valid session pointer from `open`.
            let res = unsafe { (self.api.report_actual_work_duration)(self.session, nanos) };
            if res != 0 && self.errors_logged < 3 {
                self.errors_logged += 1;
                log!(
                    "ADPF: reportActualWorkDuration failed with {} (will keep trying quietly).",
                    res
                );
            }
        }
    }

    impl Drop for HintSession {
        fn drop(&mut self) {
            // SAFETY: valid session pointer from `open`, closed exactly once.
            unsafe { (self.api.close_session)(self.session) };
        }
    }
}

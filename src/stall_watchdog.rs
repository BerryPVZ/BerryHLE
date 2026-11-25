//! Stall watchdog: says something when the emulator stops making progress.
//!
//! A black screen with a silent log is the hardest kind of bug to report, so
//! this watches the diagnostics globals in [`crate::environment`] — all of
//! them atomics — from its own thread. It never touches guest state.

use crate::environment::{
    GUEST_CPU_RUNNING, HOST_CALL_RING_IDX, LAST_GUEST_LR, LAST_GUEST_PC,
    LAST_HOST_CALL_THREAD,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long the emulator may make no progress before the watchdog reports it.
const STALL_AFTER: Duration = Duration::from_secs(15);
/// A report repeats only after this much further silence, so a game paused on
/// a menu cannot fill the log.
const REPORT_EVERY: Duration = Duration::from_secs(60);

static STARTED: AtomicBool = AtomicBool::new(false);

/// Set when a report wants guest register/stack dumps. The watchdog cannot
/// read guest state itself (the host loop owns it), so the host loop picks
/// this up and dumps on its next iteration.
static DUMP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Whether the host loop should dump guest thread state now. Clears the
/// request.
pub fn take_guest_dump_request() -> bool {
    DUMP_REQUESTED.swap(false, Ordering::Relaxed)
}

/// What counts as progress. Both halves move while anything at all is
/// happening, including a game that runs but never presents a frame.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Progress {
    guest_pc: u32,
    guest_lr: u32,
    host_calls: usize,
    thread: u32,
    cpu_running: bool,
}

fn progress() -> Progress {
    Progress {
        guest_pc: LAST_GUEST_PC.load(Ordering::Relaxed),
        guest_lr: LAST_GUEST_LR.load(Ordering::Relaxed),
        host_calls: HOST_CALL_RING_IDX.load(Ordering::Relaxed),
        thread: LAST_HOST_CALL_THREAD.load(Ordering::Relaxed),
        cpu_running: GUEST_CPU_RUNNING.load(Ordering::Relaxed),
    }
}

fn should_report(idle: Duration, reported_at: Option<Instant>, now: Instant) -> bool {
    idle >= STALL_AFTER && reported_at.map_or(true, |at| now.duration_since(at) >= REPORT_EVERY)
}

fn cpu_state_description(cpu_running: bool) -> &'static str {
    if cpu_running {
        "still active; guest may be spinning in JIT"
    } else {
        "not active; guest may be idle or host-side work may be stalled"
    }
}

/// Starts the watchdog thread. Safe to call more than once: only the first
/// call spawns the thread.
pub fn start() {
    if STARTED.swap(true, Ordering::Relaxed) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("stall watchdog".to_owned())
        .spawn(|| {
            let mut last = progress();
            let mut unchanged_since = Instant::now();
            let mut reported_at: Option<Instant> = None;
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let current = progress();
                if current != last {
                    last = current;
                    unchanged_since = Instant::now();
                    reported_at = None;
                    continue;
                }
                let idle = unchanged_since.elapsed();
                if !should_report(idle, reported_at, Instant::now()) {
                    continue;
                }
                reported_at = Some(Instant::now());
                DUMP_REQUESTED.store(true, Ordering::Relaxed);
                log!(
                    "Stall watchdog: no host-call progress for {}s; CPU execution call {} (guest PC={:#010x}, LR={:#010x}, thread={}, {} host calls so far). Recent host calls:{}",
                    idle.as_secs(),
                    cpu_state_description(last.cpu_running),
                    last.guest_pc,
                    last.guest_lr,
                    last.thread,
                    last.host_calls,
                    crate::mem::Mem::host_call_ring_dump()
                );
            }
        });
    if spawned.is_err() {
        STARTED.store(false, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_pause_is_not_reported() {
        let now = Instant::now();
        assert!(!should_report(Duration::from_secs(5), None, now));
    }

    #[test]
    fn silence_past_the_threshold_is_reported_once_then_again_later() {
        let now = Instant::now();
        assert!(should_report(STALL_AFTER, None, now));
        assert!(!should_report(
            STALL_AFTER,
            Some(now - Duration::from_secs(30)),
            now
        ));
        assert!(should_report(
            STALL_AFTER,
            Some(now - REPORT_EVERY),
            now
        ));
    }

    #[test]
    fn cpu_state_descriptions_avoid_claiming_a_deadlock() {
        let active = cpu_state_description(true);
        let inactive = cpu_state_description(false);
        assert!(active.contains("may be spinning"));
        assert!(inactive.contains("may be idle"));
        assert!(!inactive.contains("deadlocked"));
    }
}

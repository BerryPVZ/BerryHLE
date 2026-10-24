/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Host-owned home control, independent of the trainer and guest UI.
use std::sync::atomic::{AtomicBool, Ordering};
static ENABLED: AtomicBool = AtomicBool::new(false);
static AUTO_RETURN: AtomicBool = AtomicBool::new(false);
static PRESSED: AtomicBool = AtomicBool::new(false);
static FORCE_CLOSE: AtomicBool = AtomicBool::new(false);
pub fn configure(enabled: bool, auto_return: bool) {
    crate::trainer_ui::reset_cached_context();
    ENABLED.store(enabled, Ordering::Relaxed);
    AUTO_RETURN.store(auto_return, Ordering::Relaxed);
    PRESSED.store(false, Ordering::Relaxed);
    FORCE_CLOSE.store(false, Ordering::Relaxed);
}
fn session_controls(options: &crate::options::Options) -> (bool, bool) {
    let desktop = !cfg!(any(target_os = "android", target_os = "ios")) && !options.headless;
    (
        desktop && options.home_button,
        desktop && options.return_to_picker,
    )
}
pub fn configure_for_options(options: &crate::options::Options) {
    let (home, auto) = session_controls(options);
    configure(home, auto);
}
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}
pub fn auto_return() -> bool {
    AUTO_RETURN.load(Ordering::Relaxed)
}
pub fn force_close() -> bool {
    FORCE_CLOSE.load(Ordering::Relaxed)
}
pub fn rect((_, _, width, height): (u32, u32, u32, u32)) -> (f32, f32, f32, f32) {
    let scale = (width.min(height) as f32 / 480.0).clamp(1.0, 2.0);
    let (w, h, m) = (54.0 * scale, 32.0 * scale, 8.0 * scale);
    (
        (width as f32 - w - m).max(0.0),
        (height as f32 - h - m).max(0.0),
        w,
        h,
    )
}
fn contains(abs: (f32, f32), viewport: (u32, u32, u32, u32)) -> bool {
    let (x, y, w, h) = rect(viewport);
    let p = (abs.0 - viewport.0 as f32, abs.1 - viewport.1 as f32);
    p.0 >= x && p.0 < x + w && p.1 >= y && p.1 < y + h
}
pub fn down(abs: (f32, f32), viewport: (u32, u32, u32, u32)) -> bool {
    if !enabled() || !contains(abs, viewport) {
        return false;
    }
    PRESSED.store(true, Ordering::Relaxed);
    true
}
pub fn motion() -> bool {
    enabled() && PRESSED.load(Ordering::Relaxed)
}
pub fn up(abs: (f32, f32), viewport: (u32, u32, u32, u32)) -> bool {
    if !enabled() || !PRESSED.swap(false, Ordering::Relaxed) {
        return false;
    }
    if contains(abs, viewport) {
        FORCE_CLOSE.store(true, Ordering::Relaxed);
    }
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn headless_sessions_never_enable_gui_recovery_even_when_requested() {
        let mut options = crate::options::Options::default();
        options.headless = true;
        options.parse_argument("--home-button").unwrap();
        options.parse_argument("--return-to-picker").unwrap();
        assert_eq!(session_controls(&options), (false, false));
        options.headless = false;
        options.parse_argument("--no-home-button").unwrap();
        options.parse_argument("--no-return-to-picker").unwrap();
        assert_eq!(session_controls(&options), (false, false));
    }
    #[test]
    fn bottom_right_bounds_follow_letterboxed_viewport() {
        let v = (600, 0, 720, 1080);
        let (x, y, w, h) = rect(v);
        assert!(x + w < 720.0 && y + h < 1080.0);
        assert!(contains((600.0 + x + w / 2.0, y + h / 2.0), v));
        assert!(!contains((0.0, y + h / 2.0), v));
    }
}

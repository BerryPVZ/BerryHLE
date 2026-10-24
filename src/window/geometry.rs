/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Coordinate conversions shared by window presentation and input.

/// Convert display-aligned normalized input to the app's screen coordinates.
pub(super) fn normalized_to_screen((x, y): (f32, f32), (width, height): (u32, u32)) -> (f32, f32) {
    (
        ((x + 0.5) * width as f32)
            .clamp(0.0, width.saturating_sub(1) as f32)
            .round(),
        ((y + 0.5) * height as f32)
            .clamp(0.0, height.saturating_sub(1) as f32)
            .round(),
    )
}

#[cfg(test)]
mod landscape_input_tests {
    use super::*;
    #[test]
    fn landscape_input_matches_display_and_clamps_letterbox_edges() {
        let viewport = (100, 50, 960, 640);
        for (input, expected) in [
            ((580.0, 370.0), (240.0, 160.0)),
            ((100.0, 50.0), (0.0, 0.0)),
            ((1060.0, 690.0), (479.0, 319.0)),
            ((0.0, 370.0), (0.0, 160.0)),
        ] {
            assert_eq!(
                normalized_to_screen(normalize_viewport_coords(input, viewport), (480, 320)),
                expected
            );
        }
    }
}

/// Convert SDL window coordinates to drawable pixels. Query both sizes from
/// the same window; their ratio can change when moving between displays.
pub(super) fn window_to_drawable(
    (x, y): (f32, f32),
    (window_width, window_height): (u32, u32),
    (drawable_width, drawable_height): (u32, u32),
) -> (f32, f32) {
    if window_width == 0 || window_height == 0 {
        return (0.0, 0.0);
    }
    (
        x * drawable_width as f32 / window_width as f32,
        y * drawable_height as f32 / window_height as f32,
    )
}

/// Fullscreen always fits the game; windowed mode follows its resize setting.
/// Native window constraints may bypass fitting only outside fullscreen.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub(super) fn desktop_viewport(
    app_size: (u32, u32),
    drawable_size: (u32, u32),
    fullscreen: bool,
    window_preserves_aspect: bool,
    native_ratio_matches: bool,
) -> (u32, u32, u32, u32) {
    if fullscreen || (window_preserves_aspect && !native_ratio_matches) {
        fit_viewport(app_size, drawable_size)
    } else {
        (0, 0, drawable_size.0, drawable_size.1)
    }
}

/// Center the app in the drawable, preserving its aspect ratio.
pub(super) fn fit_viewport(
    (app_width, app_height): (u32, u32),
    (screen_width, screen_height): (u32, u32),
) -> (u32, u32, u32, u32) {
    if app_width == 0 || app_height == 0 || screen_width == 0 || screen_height == 0 {
        return (0, 0, 0, 0);
    }
    let app_aspect = app_width as f32 / app_height as f32;
    let screen_aspect = screen_width as f32 / screen_height as f32;
    let (scaled_width, scaled_height) = if app_aspect < screen_aspect {
        (
            (screen_height as f32 * app_aspect).round() as u32,
            screen_height,
        )
    } else {
        (
            screen_width,
            (screen_width as f32 / app_aspect).round() as u32,
        )
    };
    let x = (screen_width - scaled_width) / 2;
    let y = (screen_height - scaled_height) / 2;
    (x, y, scaled_width, scaled_height)
}

/// Map coordinates to a unit square centered on the viewport. Coordinates
/// outside the viewport are clamped to its edges (-0.5 and 0.5).
/// An empty viewport maps to the center, avoiding division by zero while hidden.
pub(super) fn normalize_viewport_coords(
    (x, y): (f32, f32),
    (vx, vy, vw, vh): (u32, u32, u32, u32),
) -> (f32, f32) {
    if vw == 0 || vh == 0 {
        return (0.0, 0.0);
    }
    (
        ((x - vx as f32) / vw as f32).clamp(0.0, 1.0) - 0.5,
        ((y - vy as f32) / vh as f32).clamp(0.0, 1.0) - 0.5,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    #[test]
    fn fullscreen_fits_portrait_landscape_and_ultrawide_with_matching_input() {
        for (app, display, expected) in [
            ((320, 480), (1920, 1080), (600, 0, 720, 1080)),
            ((1024, 768), (1920, 1080), (240, 0, 1440, 1080)),
            ((1024, 768), (3440, 1440), (760, 0, 1920, 1440)),
            ((480, 320), (1920, 1200), (60, 0, 1800, 1200)),
        ] {
            for window_preserves_aspect in [false, true] {
                for native_ratio_matches in [false, true] {
                    let viewport = desktop_viewport(app, display, true,
                        window_preserves_aspect, native_ratio_matches);
                    assert_eq!(viewport, expected);
                    let center = (display.0 as f32 / 2.0, display.1 as f32 / 2.0);
                    assert_eq!(normalize_viewport_coords(center, viewport), (0.0, 0.0));
                    assert_eq!(normalize_viewport_coords((0.0, center.1), viewport).0, -0.5);
                }
            }
        }
        // Leaving fullscreen restores the windowed stretch preference.
        assert_eq!(desktop_viewport((320, 480), (1920, 1080), false, false, false),
            (0, 0, 1920, 1080));
        assert_eq!(desktop_viewport((320, 480), (1920, 1080), false, true, false),
            (600, 0, 720, 1080));
    }

    #[test]
    fn matching_sizes_preserve_coordinates() {
        assert_eq!(fit_viewport((480, 320), (480, 320)), (0, 0, 480, 320));
        assert_eq!(
            window_to_drawable((123.0, 45.0), (480, 320), (480, 320)),
            (123.0, 45.0)
        );
    }

    #[test]
    fn mouse_and_finger_agree_on_hidpi_viewport() {
        let viewport = fit_viewport((480, 320), (960, 640));
        assert_eq!(viewport, (0, 0, 960, 640));
        let mouse = window_to_drawable((120.0, 240.0), (480, 320), (960, 640));
        // SDL finger coordinates are normalized to the entire drawable.
        let finger = (0.25 * 960.0, 0.75 * 640.0);
        assert_eq!(mouse, finger);
        assert_eq!(normalize_viewport_coords(mouse, viewport), (-0.25, 0.25));
    }

    #[test]
    fn fractional_scale_uses_current_window_dimensions() {
        assert_eq!(
            window_to_drawable((400.0, 300.0), (800, 600), (1000, 750)),
            (500.0, 375.0)
        );
        assert_eq!(fit_viewport((480, 320), (1000, 750)), (0, 41, 1000, 667));
    }

    #[test]
    fn pillarbox_center_edges_and_bars() {
        let viewport = fit_viewport((320, 480), (1280, 960));
        assert_eq!(viewport, (320, 0, 640, 960));
        assert_eq!(
            normalize_viewport_coords((640.0, 480.0), viewport),
            (0.0, 0.0)
        );
        assert_eq!(
            normalize_viewport_coords((320.0, 480.0), viewport),
            (-0.5, 0.0)
        );
        assert_eq!(
            normalize_viewport_coords((960.0, 480.0), viewport),
            (0.5, 0.0)
        );
        assert_eq!(
            normalize_viewport_coords((0.0, 480.0), viewport),
            (-0.5, 0.0)
        );
        assert_eq!(
            normalize_viewport_coords((1280.0, 480.0), viewport),
            (0.5, 0.0)
        );
    }

    #[test]
    fn landscape_in_portrait_window_is_centered() {
        let viewport = fit_viewport((480, 320), (960, 1280));
        assert_eq!(viewport, (0, 320, 960, 640));
        assert_eq!(
            normalize_viewport_coords((480.0, 640.0), viewport),
            (0.0, 0.0)
        );
        assert_eq!(
            normalize_viewport_coords((480.0, 320.0), viewport),
            (0.0, -0.5)
        );
        assert_eq!(
            normalize_viewport_coords((480.0, 960.0), viewport),
            (0.0, 0.5)
        );
    }

    #[test]
    fn mouse_accelerometer_center_is_neutral_after_letterboxing() {
        let viewport = fit_viewport((480, 320), (960, 1280));
        let mouse = window_to_drawable((240.0, 320.0), (480, 640), (960, 1280));
        let (x, y) = normalize_viewport_coords(mouse, viewport);
        assert_eq!((x * 2.0, y * 2.0), (0.0, 0.0));
    }

    #[test]
    fn empty_dimensions_produce_finite_coordinates() {
        assert_eq!(window_to_drawable((1.0, 1.0), (0, 0), (0, 0)), (0.0, 0.0));
        assert_eq!(fit_viewport((480, 320), (0, 0)), (0, 0, 0, 0));
        assert_eq!(
            normalize_viewport_coords((1.0, 1.0), (0, 0, 0, 0)),
            (0.0, 0.0)
        );
    }
}

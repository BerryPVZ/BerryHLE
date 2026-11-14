/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Quartz blend equations, operating on linear premultiplied RGBA.
//!
//! Apple defines the enum values and Porter-Duff equations:
//! https://developer.apple.com/documentation/coregraphics/cgblendmode
//! The separable/nonseparable equations are specified in PDF 1.7 section 7.2
//! and https://www.w3.org/TR/compositing-1/#blending (also used by touchHLE).

type Color = (f32, f32, f32, f32);

fn channel(back: f32, source: f32, mode: i32) -> f32 {
    match mode {
        1 => back * source,
        2 => back + source - back * source,
        3 => channel(source, back, 9), // Overlay is reversed hard-light.
        4 => back.min(source),
        5 => back.max(source),
        6 => {
            if back == 0.0 {
                0.0
            } else if source == 1.0 {
                1.0
            } else {
                (back / (1.0 - source)).min(1.0)
            }
        }
        7 => {
            if back == 1.0 {
                1.0
            } else if source == 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - back) / source).min(1.0)
            }
        }
        8 => {
            if source <= 0.5 {
                back - (1.0 - 2.0 * source) * back * (1.0 - back)
            } else {
                let d = if back <= 0.25 {
                    ((16.0 * back - 12.0) * back + 4.0) * back
                } else {
                    back.sqrt()
                };
                back + (2.0 * source - 1.0) * (d - back)
            }
        }
        9 => {
            if source <= 0.5 {
                2.0 * back * source
            } else {
                1.0 - 2.0 * (1.0 - back) * (1.0 - source)
            }
        }
        10 => (back - source).abs(),
        11 => back + source - 2.0 * back * source,
        _ => source,
    }
}

fn luminosity(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}
fn saturation(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}
fn set_luminosity(mut c: [f32; 3], lum: f32) -> [f32; 3] {
    let delta = lum - luminosity(c);
    c = c.map(|v| v + delta);
    let low = c[0].min(c[1]).min(c[2]);
    let high = c[0].max(c[1]).max(c[2]);
    if low < 0.0 {
        c = c.map(|v| lum + (v - lum) * lum / (lum - low));
    }
    if high > 1.0 {
        c = c.map(|v| lum + (v - lum) * (1.0 - lum) / (high - lum));
    }
    c
}
fn set_saturation(mut c: [f32; 3], sat: f32) -> [f32; 3] {
    let mut order = [0, 1, 2];
    order.sort_by(|a, b| c[*a].total_cmp(&c[*b]));
    let [low, mid, high] = order;
    if c[high] > c[low] {
        c[mid] = (c[mid] - c[low]) * sat / (c[high] - c[low]);
        c[high] = sat;
    } else {
        c[mid] = 0.0;
        c[high] = 0.0;
    }
    c[low] = 0.0;
    c
}

/// Both inputs and the output include alpha in their color components.
pub(super) fn composite(back: Color, source: Color, mode: i32) -> Color {
    let (br, bg, bb, ba) = back;
    let (sr, sg, sb, sa) = source;
    // The common UI path needs no unpremultiplication or blend function.
    if mode == 0 {
        return (
            sr + br * (1.0 - sa),
            sg + bg * (1.0 - sa),
            sb + bb * (1.0 - sa),
            sa + ba * (1.0 - sa),
        );
    }
    let (source_factor, back_factor) = match mode {
        16 => (0.0, 0.0),
        17 => (1.0, 0.0),
        18 => (ba, 0.0),
        19 => (1.0 - ba, 0.0),
        20 => (ba, 1.0 - sa),
        21 => (1.0 - ba, 1.0),
        22 => (0.0, sa),
        23 => (0.0, 1.0 - sa),
        24 => (1.0 - ba, sa),
        25 => (1.0 - ba, 1.0 - sa),
        27 => (1.0, 1.0),
        _ => (1.0, 1.0 - sa),
    };
    if mode >= 16 && mode != 26 {
        return (
            (sr * source_factor + br * back_factor).min(1.0),
            (sg * source_factor + bg * back_factor).min(1.0),
            (sb * source_factor + bb * back_factor).min(1.0),
            (sa * source_factor + ba * back_factor).min(1.0),
        );
    }
    if mode == 26 {
        return (
            (sr + br - 1.0).max(0.0),
            (sg + bg - 1.0).max(0.0),
            (sb + bb - 1.0).max(0.0),
            (sa + ba - 1.0).max(0.0),
        );
    }
    let unpremultiply = |c: [f32; 3], a: f32| {
        if a > 0.0 {
            c.map(|v| (v / a).clamp(0.0, 1.0))
        } else {
            [0.0; 3]
        }
    };
    let b = unpremultiply([br, bg, bb], ba);
    let s = unpremultiply([sr, sg, sb], sa);
    let mixed = match mode {
        12 => set_luminosity(set_saturation(s, saturation(b)), luminosity(b)),
        13 => set_luminosity(set_saturation(b, saturation(s)), luminosity(b)),
        14 => set_luminosity(s, luminosity(b)),
        15 => set_luminosity(b, luminosity(s)),
        _ => std::array::from_fn(|i| channel(b[i], s[i], mode)),
    };
    (
        sr * (1.0 - ba) + br * (1.0 - sa) + mixed[0] * sa * ba,
        sg * (1.0 - ba) + bg * (1.0 - sa) + mixed[1] * sa * ba,
        sb * (1.0 - ba) + bb * (1.0 - sa) + mixed[2] * sa * ba,
        sa + ba * (1.0 - sa),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn near(actual: Color, expected: Color) {
        for (a, e) in [actual.0, actual.1, actual.2, actual.3]
            .into_iter()
            .zip([expected.0, expected.1, expected.2, expected.3])
        {
            assert!((a - e).abs() < 0.00001, "{actual:?} != {expected:?}");
        }
    }
    #[test]
    fn partial_alpha_uses_uncovered_source_and_destination() {
        let b = (0.2, 0.1, 0.0, 0.5);
        let s = (0.0, 0.15, 0.25, 0.25);
        near(composite(b, s, 0), (0.15, 0.225, 0.25, 0.625));
        near(composite(b, s, 1), (0.15, 0.165, 0.125, 0.625));
        near(composite(b, s, 17), s);
        near(composite(b, s, 16), (0.0, 0.0, 0.0, 0.0));
        near(composite(b, s, 18), (0.0, 0.075, 0.125, 0.125));
    }
    #[test]
    fn blend_modes_retain_luminosity_and_transparent_backdrop() {
        let b = (0.2, 0.4, 0.7, 1.0);
        let s = (0.7, 0.1, 0.3, 1.0);
        for mode in 12..=14 {
            let c = composite(b, s, mode);
            assert!((luminosity([c.0, c.1, c.2]) - luminosity([b.0, b.1, b.2])).abs() < 0.00001);
        }
        let c = composite(b, s, 15);
        assert!((luminosity([c.0, c.1, c.2]) - luminosity([s.0, s.1, s.2])).abs() < 0.00001);
        for mode in 0..=15 {
            near(composite((0.0, 0.0, 0.0, 0.0), s, mode), s);
        }
    }
    #[test]
    fn component_modes_have_the_expected_middle_tones() {
        let b = (0.25, 0.25, 0.25, 1.0);
        let s = (0.75, 0.75, 0.75, 1.0);
        for (mode, expected) in [
            (1, 0.1875),
            (2, 0.8125),
            (3, 0.375),
            (4, 0.25),
            (5, 0.75),
            (6, 1.0),
            (7, 0.0),
            (8, 0.375),
            (9, 0.625),
            (10, 0.5),
            (11, 0.625),
        ] {
            near(composite(b, s, mode), (expected, expected, expected, 1.0));
        }
    }
}

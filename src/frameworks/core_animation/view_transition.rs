/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Core Animation transitions: legacy UIView flips and CATransition. The
//! outgoing hierarchy must survive removal from the container until the
//! animation finishes.
//! https://developer.apple.com/documentation/uikit/uiview/setanimationtransition(_:for:cache:)
//! https://developer.apple.com/documentation/quartzcore/catransition

use super::{ca_animation, ca_layer, ca_layer::CALayerHostObject, composition, CACurrentMediaTime};
use crate::frameworks::core_graphics::CGRect;
use crate::matrix::Matrix;
use crate::objc::{id, msg, msg_class, release, retain};
use crate::Environment;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct State {
    active: HashMap<id, Active>,
}

/// Style code for `CATransition` fade. `Pending.kind` is an integer, but
/// a `CATransition` type is a string, so it gets a synthetic code.
const FADE: i32 = -1;

pub(crate) struct Pending {
    layer: id,
    outgoing: ca_layer::TransitionTree,
    kind: i32,
    cache: bool,
    delegates: Vec<id>,
}

struct Active {
    pending: Pending,
    incoming: Option<ca_layer::TransitionTree>,
    start: f64,
    duration: f64,
    curve: i32,
}

/// Snapshot `layer` so it can keep being drawn (as the outgoing layer) after
/// the caller replaces or removes its contents. `kind` is a
/// `UIViewAnimationTransition`, or a negative `CATransition` style code.
pub(crate) fn prepare(env: &mut Environment, layer: id, kind: i32, cache: bool) -> Option<Pending> {
    if layer.is_null() || (kind > 0 && !matches!(kind, 1 | 2)) {
        return None;
    }
    composition::display_layers(env, layer);
    retain(env, layer);
    let delegates = ca_layer::retain_transition_delegates(env, layer);
    let outgoing = ca_layer::copy_transition_tree(env, layer, cache);
    Some(Pending {
        layer,
        outgoing,
        kind,
        cache,
        delegates,
    })
}

/// Drop a snapshot that will never be animated.
pub(crate) fn discard(env: &mut Environment, pending: Pending) {
    ca_layer::release_transition_tree(env, pending.outgoing);
    release(env, pending.layer);
    for delegate in pending.delegates {
        release(env, delegate);
    }
}

fn release_active(env: &mut Environment, active: Active) {
    if let Some(incoming) = active.incoming {
        ca_layer::release_transition_tree(env, incoming);
    }
    discard(env, active.pending);
}

/// Begin animating a snapshot returned by [`prepare`]. Non-positive or
/// non-finite durations settle immediately.
pub(crate) fn start(
    env: &mut Environment,
    pending: Pending,
    duration: f64,
    delay: f64,
    curve: i32,
) {
    let old = env
        .framework_state
        .core_animation
        .view_transition
        .active
        .remove(&pending.layer);
    if let Some(old) = old {
        release_active(env, old);
    }
    if duration <= 0.0 || !duration.is_finite() {
        discard(env, pending);
        return;
    }
    let incoming = if pending.cache {
        composition::display_layers(env, pending.layer);
        Some(ca_layer::copy_transition_tree(env, pending.layer, true))
    } else {
        None
    };
    let start = CACurrentMediaTime(env) + delay.max(0.0);
    let layer = pending.layer;
    env.framework_state
        .core_animation
        .view_transition
        .active
        .insert(
            layer,
            Active {
                pending,
                incoming,
                start,
                duration,
                curve,
            },
        );
}

/// Release transitions whose duration has elapsed. Called every frame.
pub(crate) fn expire(env: &mut Environment) {
    let now = CACurrentMediaTime(env);
    let expired: Vec<_> = env
        .framework_state
        .core_animation
        .view_transition
        .active
        .iter()
        .filter_map(|(&layer, a)| (now >= a.start + a.duration).then_some(layer))
        .collect();
    for layer in expired {
        if let Some(active) = env
            .framework_state
            .core_animation
            .view_transition
            .active
            .remove(&layer)
        {
            release_active(env, active);
        }
    }
}

/// Whether any transition is still running, i.e. whether the compositor must
/// keep drawing (and keep refreshing the outgoing snapshots).
pub(crate) fn has_active(env: &Environment) -> bool {
    !env.framework_state
        .core_animation
        .view_transition
        .active
        .is_empty()
}

pub(super) fn display_outgoing(env: &mut Environment) {
    let sources: Vec<_> = env
        .framework_state
        .core_animation
        .view_transition
        .active
        .values()
        .flat_map(|a| a.pending.outgoing.sources.iter().copied())
        .collect();
    if !sources.is_empty() {
        ca_layer::refresh_transition_tree(env, &sources);
    }
}

/// Cross-dissolve state for `layer`: the outgoing snapshot to draw
/// underneath it, the outgoing opacity, and the live layer's opacity.
/// `None` when no fade is running.
pub(super) fn fade(env: &mut Environment, layer: id) -> Option<(id, f32, f32)> {
    let (root, start, duration, curve) = {
        let a = env
            .framework_state
            .core_animation
            .view_transition
            .active
            .get(&layer)?;
        if a.pending.kind >= 0 {
            return None;
        }
        (a.pending.outgoing.root, a.start, a.duration, a.curve)
    };
    let now = CACurrentMediaTime(env);
    let progress = ((now - start) / duration).clamp(0.0, 1.0) as f32;
    let progress = super::ca_media_timing_function::solve_view_curve(curve, progress);
    let (outgoing, incoming) = cross_dissolve(progress);
    Some((root, outgoing, incoming))
}

/// The layer that should actually be drawn in `layer`'s place this frame,
/// and the extra model-view transform needed for a flip.
pub(super) fn presentation(env: &mut Environment, layer: id) -> Option<(id, Matrix<4>)> {
    let (kind, start, duration, curve, outgoing, incoming) = {
        let a = env
            .framework_state
            .core_animation
            .view_transition
            .active
            .get(&layer)?;
        // A fade draws the live layer at increasing opacity with the
        // outgoing snapshot underneath it, so nothing is substituted here.
        if a.pending.kind < 0 {
            return None;
        }
        (
            a.pending.kind,
            a.start,
            a.duration,
            a.curve,
            a.pending.outgoing.root,
            a.incoming.as_ref().map(|tree| tree.root),
        )
    };
    let now = CACurrentMediaTime(env);
    let progress = ((now - start) / duration).clamp(0.0, 1.0) as f32;
    let progress = super::ca_media_timing_function::solve_view_curve(curve, progress);
    let source = if progress < 0.5 {
        outgoing
    } else {
        incoming.unwrap_or(layer)
    };
    let bounds = env.objc.borrow::<CALayerHostObject>(source).bounds;
    Some((source, flip_transform(bounds, kind, progress)))
}

fn flip_transform(bounds: CGRect, kind: i32, progress: f32) -> Matrix<4> {
    if progress <= 0.0 || progress >= 1.0 {
        return Matrix::identity();
    }
    let angle = if progress < 0.5 {
        progress
    } else {
        progress - 1.0
    } * std::f32::consts::PI
        * if kind == 1 { -1.0 } else { 1.0 };
    let cx = bounds.origin.x + bounds.size.width * 0.5;
    let cy = bounds.origin.y + bounds.size.height * 0.5;
    let distance = bounds.size.width.max(bounds.size.height).max(1.0) * 2.0;
    let perspective = Matrix::from_columns([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, -1.0 / distance],
        [0.0, 0.0, 0.0, 1.0],
    ]);
    Matrix::translate_3d(-cx, -cy, 0.0)
        .multiply(&Matrix::<4>::from(&Matrix::<3>::y_rotation(angle)))
        .multiply(&perspective)
        // The compositor uses an orthographic [-1, 1] depth clip volume.
        // Perspective still uses the original depth to calculate w.
        .multiply(&Matrix::from_columns([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0 / distance, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]))
        .multiply(&Matrix::translate_3d(cx, cy, 0.0))
}

/// Start a `CATransition` added to `layer`. The caller has already
/// checked that `anim` is a `CATransition`. Only `kCATransitionFade`
/// is rendered; every other type cross-dissolves too, which is what
/// Apple does for a type it does not recognise. As on iOS, a zero
/// duration takes the `CATransaction` default.
pub(super) fn begin_ca_transition(env: &mut Environment, layer: id, anim: id) {
    let (duration, begin_time, started) = {
        let t: &ca_animation::CATransitionHostObject = env
            .objc
            .borrow::<ca_animation::CATransitionHostObject>(anim);
        (
            t.superclass.duration,
            t.superclass.begin_time,
            t.superclass.started_at.is_some(),
        )
    };
    let duration = if duration > 0.0 {
        duration
    } else {
        msg_class![env; CATransaction animationDuration]
    };
    // `beginTime` is either a delay in seconds, or a timestamp already past.
    let delay = if begin_time > 0.0 {
        (begin_time - CACurrentMediaTime(env)).max(0.0)
    } else {
        0.0
    };
    if duration <= 0.0 || !duration.is_finite() {
        return;
    }
    if started {
        // The same transition object was added again; it is already
        // running, so keep the existing snapshot instead of restarting.
        return;
    }
    let Some(pending) = prepare(env, layer, FADE, false) else {
        return;
    };
    let start_time = CACurrentMediaTime(env) + delay;
    () = msg![env; anim setDuration:duration];
    ca_animation::set_animation_start_time(env, anim, start_time);
    start(env, pending, duration, delay, 0);
}

/// Opacities of the outgoing and incoming layers at `progress` through a
/// cross-dissolve. The outgoing layer is drawn first, so the two weights
/// always sum to 1 and the background never shows through.
fn cross_dissolve(progress: f32) -> (f32, f32) {
    (1.0 - progress, progress)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frameworks::core_graphics::{CGPoint, CGSize};

    #[test]
    fn flip_has_identity_endpoints_and_edge_on_midpoint() {
        let bounds = CGRect {
            origin: CGPoint { x: 10.0, y: 20.0 },
            size: CGSize {
                width: 320.0,
                height: 480.0,
            },
        };
        for kind in [1, 2] {
            for progress in [0.0, 1.0] {
                let m = flip_transform(bounds, kind, progress);
                assert_eq!(m, Matrix::identity());
                for point in [[10.0, 20.0, 0.0, 1.0], [330.0, 500.0, 0.0, 1.0]] {
                    assert_eq!(m.transform(point), point);
                }
            }
            let m = flip_transform(bounds, kind, 0.5);
            for x in [10.0, 170.0, 330.0] {
                let v = m.transform([x, 260.0, 0.0, 1.0]);
                assert!((v[0] / v[3] - 170.0).abs() < 0.001);
            }
        }
    }

    #[test]
    fn flip_preserves_center_and_has_opposite_directions() {
        let bounds = CGRect {
            origin: CGPoint::default(),
            size: CGSize {
                width: 320.0,
                height: 480.0,
            },
        };
        let left = flip_transform(bounds, 1, 0.25);
        let right = flip_transform(bounds, 2, 0.25);
        for m in [left, right] {
            let c = m.transform([160.0, 240.0, 0.0, 1.0]);
            assert!((c[0] / c[3] - 160.0).abs() < 0.001);
            assert!((c[1] / c[3] - 240.0).abs() < 0.001);
        }
        let l = left.transform([0.0, 0.0, 0.0, 1.0]);
        let r = right.transform([320.0, 0.0, 0.0, 1.0]);
        assert!((l[0] / l[3] + r[0] / r[3] - 320.0).abs() < 0.001);
        assert!((l[1] / l[3] - r[1] / r[3]).abs() < 0.001);
    }

    #[test]
    fn cross_dissolve_sums_to_one_and_never_shows_background() {
        for progress in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let (outgoing, incoming) = cross_dissolve(progress);
            assert!((outgoing + incoming - 1.0).abs() < 0.001);
            assert!((0.0..=1.0).contains(&outgoing));
            assert!((0.0..=1.0).contains(&incoming));
        }
        assert_eq!(cross_dissolve(0.0), (1.0, 0.0));
        assert_eq!(cross_dissolve(1.0), (0.0, 1.0));
    }
}

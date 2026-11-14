/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Legacy UIView flip transitions. The outgoing hierarchy must survive removal
//! from the container until the animation finishes.
//! https://developer.apple.com/documentation/uikit/uiview/setanimationtransition(_:for:cache:)

use super::{ca_layer, ca_layer::CALayerHostObject, composition, CACurrentMediaTime};
use crate::frameworks::core_graphics::CGRect;
use crate::matrix::Matrix;
use crate::objc::{id, release, retain};
use crate::Environment;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct State {
    active: HashMap<id, Active>,
}

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

pub(crate) fn prepare(env: &mut Environment, layer: id, kind: i32, cache: bool) -> Option<Pending> {
    if layer.is_null() || !matches!(kind, 1 | 2) {
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

pub(super) fn presentation(env: &mut Environment, layer: id) -> Option<(id, Matrix<4>)> {
    let now = CACurrentMediaTime(env);
    let a = env
        .framework_state
        .core_animation
        .view_transition
        .active
        .get(&layer)?;
    let progress = ((now - a.start) / a.duration).clamp(0.0, 1.0) as f32;
    let progress = super::ca_media_timing_function::solve_view_curve(a.curve, progress);
    let source = if progress < 0.5 {
        a.pending.outgoing.root
    } else {
        a.incoming.as_ref().map_or(layer, |tree| tree.root)
    };
    let bounds = env.objc.borrow::<CALayerHostObject>(source).bounds;
    Some((source, flip_transform(bounds, a.pending.kind, progress)))
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
}

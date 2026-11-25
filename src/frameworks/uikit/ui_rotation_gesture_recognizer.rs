/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIRotationGestureRecognizer`.
//!
//! The base class `UIGestureRecognizer` is implemented in
//! `ui_gesture_recognizer.rs`. This module only provides the
//! `UIRotationGestureRecognizer` subclass with its `rotation` and `velocity`
//! properties. `rotation` is in radians.
//!
//! Apple documentation:
//! - <https://developer.apple.com/documentation/uikit/uirotationgesturerecognizer>

use super::ui_gesture_recognizer::{GestureKind, UIGestureRecognizerHostObject};
use crate::frameworks::core_graphics::CGFloat;
use crate::objc::{objc_classes, ClassExports, NSZonePtr, id};

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// =========================================================================
// MARK: - UIRotationGestureRecognizer
// =========================================================================

@implementation UIRotationGestureRecognizer: UIGestureRecognizer

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIGestureRecognizerHostObject::new(GestureKind::Rotation));
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

// MARK: - Properties

- (CGFloat)rotation {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).rotation
}

- (())setRotation:(CGFloat)rotation {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).rotation = rotation;
}

- (CGFloat)velocity {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).rotation_velocity
}

@end

};

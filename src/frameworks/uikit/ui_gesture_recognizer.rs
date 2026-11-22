/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIGestureRecognizer` and the tap/swipe recognizers available in iOS 3.2.

use std::collections::HashMap;
use std::time::{Duration, Instant};
use crate::frameworks::core_graphics::{CGPoint, CGFloat};
use crate::frameworks::foundation::{NSInteger, NSUInteger};
use crate::objc::{id, msg, msg_class, msg_send, nil, release, retain, objc_classes, ClassExports, HostObject, NSZonePtr, SEL};
use crate::Environment;

pub type UIGestureRecognizerState = NSInteger;
pub const UIGestureRecognizerStatePossible: UIGestureRecognizerState = 0;
pub const UIGestureRecognizerStateBegan: UIGestureRecognizerState = 1;
pub const UIGestureRecognizerStateChanged: UIGestureRecognizerState = 2;
/// `UIGestureRecognizerStateRecognized` is `UIGestureRecognizerStateEnded`.
pub const UIGestureRecognizerStateRecognized: UIGestureRecognizerState = 3;
pub const UIGestureRecognizerStateFailed: UIGestureRecognizerState = 5;

pub type UISwipeGestureRecognizerDirection = NSUInteger;
pub const UISwipeGestureRecognizerDirectionRight: UISwipeGestureRecognizerDirection = 1 << 0;
pub const UISwipeGestureRecognizerDirectionLeft: UISwipeGestureRecognizerDirection = 1 << 1;
pub const UISwipeGestureRecognizerDirectionUp: UISwipeGestureRecognizerDirection = 1 << 2;
pub const UISwipeGestureRecognizerDirectionDown: UISwipeGestureRecognizerDirection = 1 << 3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum GestureKind {
    Generic,
    Tap,
    Swipe,
    LongPress,
    Pan,
    Pinch,
    Rotation,
}

pub(super) struct UIGestureRecognizerHostObject {
    // UIKit does not retain targets, delegates or the associated view.
    target: id,
    action: Option<SEL>,
    delegate: id,
    pub(super) view: id,
    kind: GestureKind,
    state: UIGestureRecognizerState,
    enabled: bool,
    cancels_touches_in_view: bool,
    delays_touches_began: bool,
    delays_touches_ended: bool,
    number_of_taps_required: NSUInteger,
    number_of_touches_required: NSUInteger,
    direction: UISwipeGestureRecognizerDirection,
    initial_location: CGPoint,
    current_location: CGPoint,
    tracking: bool,
    minimum_press_duration: f64,
    allowable_movement: CGFloat,
    press_timer: id,
    completed_taps: NSUInteger,
    previous_tap: Option<(Instant, CGPoint)>,
    press_started: Option<Instant>,
    active_touches: HashMap<id, (CGPoint, CGPoint)>,
    /// Recognizers that must fail before this one may recognize, in the
    /// order `-requireGestureRecognizerToFail:` was called. UIKit does not
    /// retain them (same as targets and the delegate).
    required_to_fail: Vec<id>,
    /// Translation and velocity tracked by `UIPanGestureRecognizer`, in the
    /// recognizer view's coordinates. Pinch and rotation keep their values in
    /// the same object so every recognizer subclass shares one host type.
    pub(super) pan_translation: CGPoint,
    pub(super) pan_velocity: CGPoint,
    pub(super) pan_last_move: Option<Instant>,
    pub(super) minimum_number_of_touches: NSUInteger,
    pub(super) maximum_number_of_touches: NSUInteger,
    pub(super) pinch_scale: CGFloat,
    pub(super) pinch_velocity: CGFloat,
    pub(super) pinch_start_distance: CGFloat,
    pub(super) rotation: CGFloat,
    pub(super) rotation_velocity: CGFloat,
    pub(super) rotation_start_angle: CGFloat,
}
impl HostObject for UIGestureRecognizerHostObject {}

impl UIGestureRecognizerHostObject {
    pub(super) fn new(kind: GestureKind) -> Self {
        Self {
            target: nil,
            action: None,
            delegate: nil,
            view: nil,
            kind,
            state: UIGestureRecognizerStatePossible,
            enabled: true,
            cancels_touches_in_view: true,
            delays_touches_began: false,
            delays_touches_ended: true,
            number_of_taps_required: if kind == GestureKind::LongPress { 0 } else { 1 },
            number_of_touches_required: 1,
            direction: UISwipeGestureRecognizerDirectionRight,
            initial_location: CGPoint { x: 0.0, y: 0.0 },
            current_location: CGPoint { x: 0.0, y: 0.0 },
            tracking: false,
            minimum_press_duration: 0.5,
            allowable_movement: 10.0,
            press_timer: nil,
            completed_taps: 0,
            previous_tap: None,
            press_started: None,
            active_touches: HashMap::new(),
            required_to_fail: Vec::new(),
            pan_translation: CGPoint { x: 0.0, y: 0.0 },
            pan_velocity: CGPoint { x: 0.0, y: 0.0 },
            pan_last_move: None,
            minimum_number_of_touches: 1,
            maximum_number_of_touches: NSUInteger::MAX,
            pinch_scale: if kind == GestureKind::Pinch { 1.0 } else { 0.0 },
            pinch_velocity: 0.0,
            pinch_start_distance: 0.0,
            rotation: 0.0,
            rotation_velocity: 0.0,
            rotation_start_angle: 0.0,
        }
    }
}

impl Default for UIGestureRecognizerHostObject {
    fn default() -> Self {
        Self::new(GestureKind::Generic)
    }
}

fn init_with_target(env: &mut Environment, this: id, target: id, action: SEL) -> id {
    let recognizer = env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this);
    recognizer.target = target;
    recognizer.action = if action.is_null() { None } else { Some(action) };
    this
}

fn send_action(env: &mut Environment, recognizer: id) {
    let (target, action) = {
        let host = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
        (host.target, host.action)
    };
    let Some(action) = action else { return };
    if target == nil {
        log!("TODO: UIGestureRecognizer target:nil responder-chain dispatch is unsupported");
        return;
    }

    let colon_count = action
        .as_str(&env.mem)
        .bytes()
        .filter(|&b| b == b':')
        .count();
    match colon_count {
        0 => () = msg_send(env, (target, action)),
        1 => () = msg_send(env, (target, action, recognizer)),
        // The selector comes straight from guest code; a malformed selector
        // must not take down the host. Log and skip, like UIControl does.
        _ => log!(
            "Warning: gesture recognizer action {:?} has unsupported \
             argument count {}; skipping.",
            action.as_str(&env.mem),
            colon_count
        ),
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIGestureRecognizer: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIGestureRecognizerHostObject::new(GestureKind::Generic));
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithTarget:(id)target action:(SEL)action {
    init_with_target(env, this, target, action)
}

- (id)delegate { env.objc.borrow::<UIGestureRecognizerHostObject>(this).delegate }
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).delegate = delegate;
}
- (id)view { env.objc.borrow::<UIGestureRecognizerHostObject>(this).view }
- (())setView:(id)view {
    set_view(env, this, view);
}
- (UIGestureRecognizerState)state {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).state
}
// Apple: "Creates a dependency relationship between the receiver and
// another gesture recognizer ... the receiver will not recognize until the
// other recognizer fails." Recognition is driven per recognizer here, so we
// record the dependency and consult it when the receiver would recognize.
// <https://developer.apple.com/documentation/uikit/uigesturerecognizer/1624410-requiregesturerecognizertofail>
- (())requireGestureRecognizerToFail:(id)other {
    if other == nil || other == this { return; }
    let mut host = env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this);
    if !host.required_to_fail.contains(&other) {
        host.required_to_fail.push(other);
    }
}
- (bool)isEnabled { env.objc.borrow::<UIGestureRecognizerHostObject>(this).enabled }
- (())setEnabled:(bool)enabled {
    if !enabled { cancel_long_press(env, this); }
    let host = env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this);
    host.enabled = enabled;
    host.tracking = false;
    host.state = UIGestureRecognizerStatePossible;
}
- (bool)cancelsTouchesInView {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).cancels_touches_in_view
}
- (())setCancelsTouchesInView:(bool)value {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).cancels_touches_in_view = value;
}
- (bool)delaysTouchesBegan {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).delays_touches_began
}
- (())setDelaysTouchesBegan:(bool)value {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).delays_touches_began = value;
}
- (bool)delaysTouchesEnded {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).delays_touches_ended
}
- (())setDelaysTouchesEnded:(bool)value {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).delays_touches_ended = value;
}
- (CGPoint)locationInView:(id)view {
    let (current_location, own_view) = {
        let host = env.objc.borrow::<UIGestureRecognizerHostObject>(this);
        (host.current_location, host.view)
    };
    if view == nil || view == own_view {
        current_location
    } else {
        msg![env; view convertPoint:current_location fromView:own_view]
    }
}

@end

@implementation UILongPressGestureRecognizer: UIGestureRecognizer
+ (id)allocWithZone:(NSZonePtr)_zone {
    let host = UIGestureRecognizerHostObject::new(GestureKind::LongPress);
    env.objc.alloc_object(this, Box::new(host), &mut env.mem)
}
- (f64)minimumPressDuration { env.objc.borrow::<UIGestureRecognizerHostObject>(this).minimum_press_duration }
- (())setMinimumPressDuration:(f64)value {
    if value.is_finite() && value >= 0.0 {
        env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).minimum_press_duration = value;
    }
}
- (CGFloat)allowableMovement { env.objc.borrow::<UIGestureRecognizerHostObject>(this).allowable_movement }
- (())setAllowableMovement:(CGFloat)value {
    if value.is_finite() && value >= 0.0 {
        env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).allowable_movement = value;
    }
}
- (NSUInteger)numberOfTapsRequired { env.objc.borrow::<UIGestureRecognizerHostObject>(this).number_of_taps_required }
- (())setNumberOfTapsRequired:(NSUInteger)value {
    cancel_long_press(env, this);
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).number_of_taps_required = value;
}
- (NSUInteger)numberOfTouchesRequired { env.objc.borrow::<UIGestureRecognizerHostObject>(this).number_of_touches_required }
- (())setNumberOfTouchesRequired:(NSUInteger)value {
    cancel_long_press(env, this);
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).number_of_touches_required = value.max(1);
}
- (())_touchHLE_longPressTimer:(id)timer { long_press_timer(env, this, timer); }
- (())reset {
    cancel_long_press(env, this);
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).state = UIGestureRecognizerStatePossible;
}
- (())dealloc {
    stop_press_timer(env, this);
    env.objc.dealloc_object(this, &mut env.mem)
}
@end

@implementation UIPanGestureRecognizer: UIGestureRecognizer

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIGestureRecognizerHostObject::new(GestureKind::Pan));
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (CGPoint)translationInView:(id)_view {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).pan_translation
}
- (())setTranslation:(CGPoint)translation inView:(id)_view {
    // UIKit treats this as "start counting again from here": the next
    // translationInView: is measured from the new origin.
    let host = env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this);
    host.initial_location = CGPoint {
        x: host.initial_location.x + translation.x,
        y: host.initial_location.y + translation.y,
    };
    host.pan_translation = CGPoint { x: 0.0, y: 0.0 };
}
- (CGPoint)velocityInView:(id)_view {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).pan_velocity
}
- (NSUInteger)minimumNumberOfTouches {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).minimum_number_of_touches
}
- (())setMinimumNumberOfTouches:(NSUInteger)count {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this)
        .minimum_number_of_touches = count;
}
- (NSUInteger)maximumNumberOfTouches {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).maximum_number_of_touches
}
- (())setMaximumNumberOfTouches:(NSUInteger)count {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this)
        .maximum_number_of_touches = count;
}
- (NSUInteger)numberOfTouches {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this)
        .active_touches
        .len() as NSUInteger
}

@end

@implementation UITapGestureRecognizer: UIGestureRecognizer

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIGestureRecognizerHostObject::new(GestureKind::Tap));
    env.objc.alloc_object(this, host_object, &mut env.mem)
}
- (NSUInteger)numberOfTapsRequired {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).number_of_taps_required
}
- (())setNumberOfTapsRequired:(NSUInteger)value {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).number_of_taps_required = value;
}
- (NSUInteger)numberOfTouchesRequired {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).number_of_touches_required
}
- (())setNumberOfTouchesRequired:(NSUInteger)value {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).number_of_touches_required = value;
}

@end


@implementation UISwipeGestureRecognizer: UIGestureRecognizer

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIGestureRecognizerHostObject::new(GestureKind::Swipe));
    env.objc.alloc_object(this, host_object, &mut env.mem)
}
- (UISwipeGestureRecognizerDirection)direction {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).direction
}
- (())setDirection:(UISwipeGestureRecognizerDirection)value {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).direction = value;
}
- (NSUInteger)numberOfTouchesRequired {
    env.objc.borrow::<UIGestureRecognizerHostObject>(this).number_of_touches_required
}
- (())setNumberOfTouchesRequired:(NSUInteger)value {
    env.objc.borrow_mut::<UIGestureRecognizerHostObject>(this).number_of_touches_required = value;
}

@end

};

pub(super) fn set_view(env: &mut Environment, recognizer: id, view: id) {
    retain(env, recognizer);
    if env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer).view != view {
        cancel_long_press(env, recognizer);
    }
    env.objc
        .borrow_mut::<UIGestureRecognizerHostObject>(recognizer)
        .view = view;
    release(env, recognizer);
}

fn delegate_allows_touch(env: &mut Environment, recognizer: id, touch: id) -> bool {
    let delegate = env
        .objc
        .borrow::<UIGestureRecognizerHostObject>(recognizer)
        .delegate;
    if delegate == nil {
        return true;
    }
    let Some(selector) = env
        .objc
        .lookup_selector("gestureRecognizer:shouldReceiveTouch:")
    else {
        return true;
    };
    if msg![env; delegate respondsToSelector:selector] {
        msg_send(env, (delegate, selector, recognizer, touch))
    } else {
        true
    }
}

fn delegate_allows_begin(env: &mut Environment, recognizer: id) -> bool {
    let delegate = env
        .objc
        .borrow::<UIGestureRecognizerHostObject>(recognizer)
        .delegate;
    if delegate == nil {
        return true;
    }
    let Some(selector) = env.objc.lookup_selector("gestureRecognizerShouldBegin:") else {
        return true;
    };
    if msg![env; delegate respondsToSelector:selector] {
        msg_send(env, (delegate, selector, recognizer))
    } else {
        true
    }
}

pub(super) fn touches_began(env: &mut Environment, view: id, touches: id) {
    if view == nil {
        return;
    }
    let touch: id = msg![env; touches anyObject];
    if touch == nil {
        return;
    }
    let recognizers = recognizers_in_chain(env, view);
    for &recognizer in &recognizers { retain(env, recognizer); }
    for &recognizer in &recognizers {
        if env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer).kind == GestureKind::LongPress {
            long_press_touches(env, recognizer, touches, 0);
            continue;
        }
        if matches!(
            env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer).kind,
            GestureKind::Pinch | GestureKind::Rotation
        ) {
            two_finger_touches(env, recognizer, touches, 0);
            continue;
        }
        let (enabled, recognizer_view) = {
            let host = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
            (host.enabled, host.view)
        };
        if !enabled || !delegate_allows_touch(env, recognizer, touch) {
            continue;
        }
        let location: CGPoint = msg![env; touch locationInView:recognizer_view];
        let host = env
            .objc
            .borrow_mut::<UIGestureRecognizerHostObject>(recognizer);
        host.initial_location = location;
        host.current_location = location;
        host.state = UIGestureRecognizerStatePossible;
        host.tracking = true;
        if host.kind == GestureKind::Pan {
            host.pan_translation = CGPoint { x: 0.0, y: 0.0 };
            host.pan_velocity = CGPoint { x: 0.0, y: 0.0 };
            host.pan_last_move = Some(Instant::now());
        }
    }
    for recognizer in recognizers { release(env, recognizer); }
}

pub(super) fn touches_moved(env: &mut Environment, view: id, touches: id) {
    if view == nil {
        return;
    }
    let touch: id = msg![env; touches anyObject];
    if touch == nil {
        return;
    }
    let recognizers = recognizers_in_chain(env, view);
    for &recognizer in &recognizers { retain(env, recognizer); }
    for &recognizer in &recognizers {
        if env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer).kind == GestureKind::LongPress {
            long_press_touches(env, recognizer, touches, 1);
            continue;
        }
        if matches!(
            env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer).kind,
            GestureKind::Pinch | GestureKind::Rotation
        ) {
            two_finger_touches(env, recognizer, touches, 1);
            continue;
        }
        let (tracking, recognizer_view, kind) = {
            let host = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
            (host.tracking, host.view, host.kind)
        };
        if !tracking {
            continue;
        }
        if kind == GestureKind::Pan {
            pan_touches_moved(env, recognizer, touch, recognizer_view);
            continue;
        }
        let location: CGPoint = msg![env; touch locationInView:recognizer_view];
        env.objc
            .borrow_mut::<UIGestureRecognizerHostObject>(recognizer)
            .current_location = location;
    }
    for recognizer in recognizers { release(env, recognizer); }
}

pub(super) fn touches_ended(env: &mut Environment, view: id, touches: id) {
    if view == nil {
        return;
    }
    let touch: id = msg![env; touches anyObject];
    if touch == nil {
        return;
    }
    let recognizers = recognizers_in_chain(env, view);
    for &recognizer in &recognizers { retain(env, recognizer); }
    for &recognizer in &recognizers {
        if env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer).kind == GestureKind::LongPress {
            long_press_touches(env, recognizer, touches, 2);
            continue;
        }
        if matches!(
            env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer).kind,
            GestureKind::Pinch | GestureKind::Rotation
        ) {
            two_finger_touches(env, recognizer, touches, 2);
            continue;
        }
        let (tracking, recognizer_view, kind) = {
            let host = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
            (host.tracking, host.view, host.kind)
        };
        if !tracking {
            continue;
        }
        if kind == GestureKind::Pan {
            pan_touches_ended(env, recognizer, touch, recognizer_view);
            continue;
        }
        let location: CGPoint = msg![env; touch locationInView:recognizer_view];
        {
            let host = env
                .objc
                .borrow_mut::<UIGestureRecognizerHostObject>(recognizer);
            host.current_location = location;
            host.tracking = false;
        }

        let recognized = {
            let host = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
            let dx = host.current_location.x - host.initial_location.x;
            let dy = host.current_location.y - host.initial_location.y;
            match host.kind {
                GestureKind::Tap => host.number_of_taps_required == 1 && dx * dx + dy * dy <= 100.0,
                GestureKind::Swipe => {
                    let (direction, distance, cross_distance) = if dx.abs() >= dy.abs() {
                        (
                            if dx >= 0.0 {
                                UISwipeGestureRecognizerDirectionRight
                            } else {
                                UISwipeGestureRecognizerDirectionLeft
                            },
                            dx.abs(),
                            dy.abs(),
                        )
                    } else {
                        (
                            if dy >= 0.0 {
                                UISwipeGestureRecognizerDirectionDown
                            } else {
                                UISwipeGestureRecognizerDirectionUp
                            },
                            dy.abs(),
                            dx.abs(),
                        )
                    };
                    distance >= 24.0
                        && cross_distance <= distance
                        && host.direction & direction != 0
                }
                GestureKind::Generic
                | GestureKind::LongPress
                | GestureKind::Pan
                | GestureKind::Pinch
                | GestureKind::Rotation => false,
            }
        };

        // A recognizer gated by `-requireGestureRecognizerToFail:` may only
        // recognize while none of its dependencies has begun or recognized.
        let superseded = {
            let required = env
                .objc
                .borrow::<UIGestureRecognizerHostObject>(recognizer)
                .required_to_fail
                .clone();
            required.iter().any(|&other| {
                env.objc
                    .get_host_object(other)
                    .and_then(|host| host.as_any().downcast_ref::<UIGestureRecognizerHostObject>())
                    .map(|host| {
                    matches!(
                        host.state,
                        UIGestureRecognizerStateBegan
                            | UIGestureRecognizerStateChanged
                            | UIGestureRecognizerStateRecognized
                    )
                })
                    .unwrap_or(false)
            })
        };

        if recognized && !superseded && delegate_allows_begin(env, recognizer) {
            env.objc
                .borrow_mut::<UIGestureRecognizerHostObject>(recognizer)
                .state = UIGestureRecognizerStateRecognized;
            send_action(env, recognizer);
        } else {
            env.objc
                .borrow_mut::<UIGestureRecognizerHostObject>(recognizer)
                .state = UIGestureRecognizerStateFailed;
        }
    }
    for recognizer in recognizers { release(env, recognizer); }
}

// A recognizer on a container observes touches hitting any descendant.
fn recognizers_in_chain(env: &mut Environment, mut view: id) -> Vec<id> {
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    while view != nil && seen.insert(view) {
        for r in super::ui_view::gesture_recognizers(env, view) {
            if !result.contains(&r) { result.push(r); }
        }
        view = msg![env; view superview];
    }
    result
}

fn stop_press_timer(env: &mut Environment, recognizer: id) {
    let timer = std::mem::replace(&mut env.objc.borrow_mut::<UIGestureRecognizerHostObject>(recognizer).press_timer, nil);
    if timer != nil {
        () = msg![env; timer invalidate];
        release(env, timer);
    }
}

fn cancel_long_press(env: &mut Environment, recognizer: id) {
    let h = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
    if h.kind != GestureKind::LongPress { return; }
    retain(env, recognizer);
    stop_press_timer(env, recognizer);
    let h = env.objc.borrow_mut::<UIGestureRecognizerHostObject>(recognizer);
    let active = matches!(h.state, 1 | 2);
    h.state = if active { 4 } else { UIGestureRecognizerStateFailed };
    h.active_touches.clear();
    h.completed_taps = 0;
    h.previous_tap = None;
    h.press_started = None;
    h.tracking = false;
    if active { send_action(env, recognizer); }
    release(env, recognizer);
}

fn pan_touches_moved(
    env: &mut Environment,
    recognizer: id,
    touch: id,
    view: id,
) {
    let location: CGPoint = msg![env; touch locationInView:view];
    let now = Instant::now();
    let (enabled, start, state, last_move, previous) = {
        let host = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
        (
            host.enabled,
            host.initial_location,
            host.state,
            host.pan_last_move,
            host.current_location,
        )
    };
    if !enabled {
        return;
    }
    let translation = CGPoint {
        x: location.x - start.x,
        y: location.y - start.y,
    };
    // UIKit reports velocity in points per second since the previous event.
    let velocity = match last_move {
        Some(last) => {
            // Clamp to a millisecond so a pair of events in the same tick
            // cannot divide by zero or report an infinite velocity.
            let seconds = now.duration_since(last).as_secs_f64().max(0.001) as CGFloat;
            CGPoint {
                x: (location.x - previous.x) / seconds,
                y: (location.y - previous.y) / seconds,
            }
        }
        None => CGPoint { x: 0.0, y: 0.0 },
    };
    let began = state == UIGestureRecognizerStatePossible
        && (translation.x != 0.0 || translation.y != 0.0);
    {
        let host = env
            .objc
            .borrow_mut::<UIGestureRecognizerHostObject>(recognizer);
        host.current_location = location;
        host.pan_translation = translation;
        host.pan_velocity = velocity;
        host.pan_last_move = Some(now);
        host.state = if began {
            UIGestureRecognizerStateBegan
        } else {
            UIGestureRecognizerStateChanged
        };
    }
    send_action(env, recognizer);
}

fn pan_touches_ended(
    env: &mut Environment,
    recognizer: id,
    touch: id,
    view: id,
) {
    let location: CGPoint = msg![env; touch locationInView:view];
    let active = {
        let host = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
        matches!(
            host.state,
            UIGestureRecognizerStateBegan | UIGestureRecognizerStateChanged
        )
    };
    {
        let host = env
            .objc
            .borrow_mut::<UIGestureRecognizerHostObject>(recognizer);
        host.pan_translation = CGPoint {
            x: location.x - host.initial_location.x,
            y: location.y - host.initial_location.y,
        };
        host.pan_velocity = CGPoint { x: 0.0, y: 0.0 };
        host.pan_last_move = None;
        host.tracking = false;
        host.state = if active {
            UIGestureRecognizerStateRecognized
        } else {
            UIGestureRecognizerStateFailed
        };
    }
    if active {
        send_action(env, recognizer);
    }
}

fn movement_allowed(a: CGPoint, b: CGPoint, limit: CGFloat) -> bool {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy <= limit * limit
}

fn long_press_timer(env: &mut Environment, recognizer: id, timer: id) {
    let h = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
    if h.press_timer != timer { return; }
    let ready = h.enabled && h.view != nil && h.tracking && h.state == 0
        && h.active_touches.len() == h.number_of_touches_required as usize;
    stop_press_timer(env, recognizer);
    if !ready || !delegate_allows_begin(env, recognizer) {
        env.objc.borrow_mut::<UIGestureRecognizerHostObject>(recognizer).state = UIGestureRecognizerStateFailed;
        return;
    }
    // Delegate code may have detached or disabled the recognizer.
    let h = env.objc.borrow_mut::<UIGestureRecognizerHostObject>(recognizer);
    if !h.enabled || h.view == nil || !h.tracking { return; }
    h.state = UIGestureRecognizerStateBegan; // While the finger is still down.
    h.completed_taps = 0;
    h.previous_tap = None;
    let cancel = h.cancels_touches_in_view;
    let touches: Vec<id> = h.active_touches.keys().copied().collect();
    if cancel { super::ui_touch::cancel_for_gesture(env, &touches); }
    send_action(env, recognizer);
}

fn pair_distance_and_angle(a: CGPoint, b: CGPoint) -> (CGFloat, CGFloat) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    ((dx * dx + dy * dy).sqrt(), dy.atan2(dx))
}

fn two_finger_touches(env: &mut Environment, recognizer: id, touches: id, phase: u8) {
    retain(env, recognizer);
    two_finger_touches_inner(env, recognizer, touches, phase);
    release(env, recognizer);
}

fn two_finger_touches_inner(env: &mut Environment, recognizer: id, touches: id, phase: u8) {
    let h = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
    if !h.enabled { return; }
    let view = h.view;
    let array: id = msg![env; touches allObjects];
    let count: NSUInteger = msg![env; array count];
    for i in 0..count {
        let touch: id = msg![env; array objectAtIndex:i];
        if phase == 0 && !delegate_allows_touch(env, recognizer, touch) { continue; }
        let point: CGPoint = msg![env; touch locationInView:view];
        let h = env.objc.borrow_mut::<UIGestureRecognizerHostObject>(recognizer);
        if phase == 0 {
            h.active_touches.insert(touch, (point, point));
        } else if let Some((_, current)) = h.active_touches.get_mut(&touch) {
            *current = point;
        }
        if phase == 2 {
            h.active_touches.remove(&touch);
        }
    }

    let h = env.objc.borrow_mut::<UIGestureRecognizerHostObject>(recognizer);
    if h.active_touches.len() < 2 {
        let was_active = h.state == UIGestureRecognizerStateBegan
            || h.state == UIGestureRecognizerStateChanged;
        if phase == 2 {
            h.state = if was_active {
                UIGestureRecognizerStateRecognized
            } else {
                UIGestureRecognizerStateFailed
            };
            h.active_touches.clear();
            h.tracking = false;
        }
        let active = h.enabled && was_active && h.state == UIGestureRecognizerStateRecognized;
        if active { send_action(env, recognizer); }
        return;
    }

    // Two touches: use their distance and angle. Order the pair so the
    // sign of the rotation does not flip between events.
    let mut points: Vec<CGPoint> =
        h.active_touches.values().map(|&(_, current)| current).collect();
    points.sort_by(|a, b| {
        let (ax, ay) = (a.x, a.y);
        let (bx, by) = (b.x, b.y);
        ax.partial_cmp(&bx)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(ay.partial_cmp(&by).unwrap_or(std::cmp::Ordering::Equal))
    });
    let (distance, angle) = pair_distance_and_angle(points[0], points[1]);
    if phase == 0 {
        h.pinch_scale = 1.0;
        h.pinch_velocity = 0.0;
        h.pinch_start_distance = distance;
        h.rotation = 0.0;
        h.rotation_velocity = 0.0;
        h.rotation_start_angle = angle;
        h.state = UIGestureRecognizerStateBegan;
        h.tracking = true;
    } else if phase == 1 {
        if h.pinch_start_distance > 0.0 {
            h.pinch_scale = distance / h.pinch_start_distance;
        }
        let mut delta = angle - h.rotation_start_angle;
        while delta > std::f32::consts::PI { delta -= 2.0 * std::f32::consts::PI; }
        while delta < -std::f32::consts::PI { delta += 2.0 * std::f32::consts::PI; }
        h.rotation = delta;
        h.state = UIGestureRecognizerStateChanged;
    }
    let dispatch = phase != 2;
    if dispatch { send_action(env, recognizer); }
}

fn long_press_touches(env: &mut Environment, recognizer: id, touches: id, phase: u8) {
    retain(env, recognizer);
    long_press_touches_inner(env, recognizer, touches, phase);
    release(env, recognizer);
}

fn long_press_touches_inner(env: &mut Environment, recognizer: id, touches: id, phase: u8) {
    let h = env.objc.borrow::<UIGestureRecognizerHostObject>(recognizer);
    if !h.enabled { return; }
    let view = h.view;
    let array: id = msg![env; touches allObjects];
    let count: NSUInteger = msg![env; array count];
    let mut failed = false;
    for i in 0..count {
        let touch: id = msg![env; array objectAtIndex:i];
        if phase == 0 && !delegate_allows_touch(env, recognizer, touch) { continue; }
        let point: CGPoint = msg![env; touch locationInView:view];
        let h = env.objc.borrow_mut::<UIGestureRecognizerHostObject>(recognizer);
        if phase == 0 {
            if h.active_touches.is_empty() {
                let follows_tap = h.previous_tap.map(|(when, at)|
                    when.elapsed() <= Duration::from_millis(300) && movement_allowed(at, point, h.allowable_movement)
                ).unwrap_or(false);
                if !follows_tap { h.completed_taps = 0; }
                h.press_started = Some(Instant::now());
                h.state = 0;
                h.tracking = true;
            }
            h.active_touches.insert(touch, (point, point));
        } else if let Some((start, current)) = h.active_touches.get_mut(&touch) {
            *current = point;
            // Once recognised, motion reports Changed rather than failing.
            if h.state == 0 && !movement_allowed(*start, point, h.allowable_movement) { failed = true; }
        }
        if phase == 2 { h.active_touches.remove(&touch); }
    }
    let h = env.objc.borrow_mut::<UIGestureRecognizerHostObject>(recognizer);
    let n = h.active_touches.len();
    if n > 0 {
        let (x, y) = h.active_touches.values().fold((0.0, 0.0), |(x, y), (_, p)| (x + p.x, y + p.y));
        h.current_location = CGPoint { x: x / n as f32, y: y / n as f32 };
    }
    if n > h.number_of_touches_required as usize { failed = true; }
    let active = matches!(h.state, 1 | 2);
    if failed {
        cancel_long_press(env, recognizer);
    } else if phase == 2 && n < h.number_of_touches_required as usize {
        if !active && h.tracking && h.number_of_taps_required > 0
            && h.press_started.map(|at| at.elapsed() <= Duration::from_millis(300)).unwrap_or(false) {
            h.completed_taps = (h.completed_taps + 1).min(h.number_of_taps_required);
            h.previous_tap = Some((Instant::now(), h.current_location));
        }
        h.tracking = false;
        h.state = if active {
            UIGestureRecognizerStateRecognized
        } else {
            UIGestureRecognizerStateFailed
        };
        stop_press_timer(env, recognizer);
        if active { send_action(env, recognizer); }
    } else if active && phase == 1 {
        h.state = UIGestureRecognizerStateChanged;
        send_action(env, recognizer);
    } else if h.state == 0 && h.tracking && h.press_timer == nil && n == h.number_of_touches_required as usize
        && h.completed_taps == h.number_of_taps_required {
        let duration = h.minimum_press_duration;
        let selector = env.objc.lookup_selector("_touchHLE_longPressTimer:").unwrap();
        let timer: id = msg_class![env; NSTimer scheduledTimerWithTimeInterval:duration target:recognizer selector:selector userInfo:nil repeats:false];
        retain(env, timer);
        env.objc.borrow_mut::<UIGestureRecognizerHostObject>(recognizer).press_timer = timer;
    }
}

#[cfg(test)]
mod long_press_tests {
    use super::*;
    #[test]
    fn defaults_and_motion_threshold() {
        let h = UIGestureRecognizerHostObject::new(GestureKind::LongPress);
        assert_eq!(h.minimum_press_duration, 0.5);
        assert_eq!(h.number_of_taps_required, 0);
        assert_eq!(h.number_of_touches_required, 1);
        assert!(h.press_timer == nil);
        assert!(movement_allowed(CGPoint { x: 0.0, y: 0.0 }, CGPoint { x: 6.0, y: 8.0 }, h.allowable_movement));
        assert!(!movement_allowed(CGPoint { x: 0.0, y: 0.0 }, CGPoint { x: 6.0, y: 8.1 }, h.allowable_movement));
    }
}

#[cfg(test)]
mod two_finger_tests {
    use super::*;

    #[test]
    fn distance_and_angle_follow_the_touch_pair() {
        let (distance, angle) = pair_distance_and_angle(
            CGPoint { x: 0.0, y: 0.0 },
            CGPoint { x: 3.0, y: 4.0 },
        );
        assert_eq!(distance, 5.0);
        assert!((angle - 4.0f32.atan2(3.0)).abs() < 1e-6);

        let (_, down) = pair_distance_and_angle(
            CGPoint { x: 0.0, y: 0.0 },
            CGPoint { x: 0.0, y: 1.0 },
        );
        assert!((down - std::f32::consts::FRAC_PI_2).abs() < 1e-6);

        // Swapping the pair flips the angle, hence the sort in the caller.
        let (_, flipped) = pair_distance_and_angle(
            CGPoint { x: 3.0, y: 4.0 },
            CGPoint { x: 0.0, y: 0.0 },
        );
        assert!(flipped < 0.0);
    }
}

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! GLKit.
//!
//! Apple documents `GLKView` as "a default implementation for views that
//! draw their content using OpenGL ES" and `GLKViewController` as "a view
//! controller that manages an OpenGL ES rendering loop":
//! <https://developer.apple.com/documentation/glkit/glkview>
//! <https://developer.apple.com/documentation/glkit/glkviewcontroller>
//!
//! touchHLE does not implement the full GLKit stack, but apps built against
//! iOS 5+ commonly link the framework and allocate these classes at startup
//! (previously this hit the `UnimplementedClass` "SUPER HACK" warning and
//! left the app without a working render loop). This module provides real
//! class implementations with the documented inheritance (`GLKView:
//! UIView`, `GLKViewController: UIViewController`) and the documented
//! properties, plus the `GLKMatrix4Identity` constant that math-only users
//! of GLKit import.

use crate::abi::{impl_GuestRet_for_large_struct, GuestArg};
use crate::dyld::{export_c_func, ConstantExports, FunctionExports, HostConstant};
use crate::frameworks::core_graphics::{CGFloat, CGRect};
use crate::frameworks::foundation::{NSInteger, NSTimeInterval};
use crate::frameworks::uikit::ui_view::UIViewHostObject;
use crate::frameworks::uikit::ui_view_controller::UIViewControllerHostObject;
use crate::mem::{ConstPtr, ConstVoidPtr, MutPtr, SafeRead};
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, nil, objc_classes, release, retain, ClassExports,
    NSZonePtr,
};
use crate::Environment;

// GLKMatrix4Identity — 4x4 identity matrix of 32-bit floats (16 × 4 = 64
// bytes). Layout matches Apple's `_GLKMatrix4 { float m[16]; }`.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub(super) struct GLKMatrix4 {
    m: [f32; 16],
}
unsafe impl SafeRead for GLKMatrix4 {}
impl_GuestRet_for_large_struct!(GLKMatrix4);
impl GuestArg for GLKMatrix4 {
    const REG_COUNT: usize = 16;

    fn from_regs(regs: &[u32]) -> Self {
        GLKMatrix4 {
            m: std::array::from_fn(|index| f32::from_bits(regs[index])),
        }
    }

    fn to_regs(self, regs: &mut [u32]) {
        for (destination, value) in regs.iter_mut().zip(self.m) {
            *destination = value.to_bits();
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub(super) struct GLKVector2 {
    pub(super) x: f32,
    pub(super) y: f32,
}

impl GuestArg for GLKVector2 {
    const REG_COUNT: usize = 2;

    fn from_regs(regs: &[u32]) -> Self {
        Self {
            x: f32::from_bits(regs[0]),
            y: f32::from_bits(regs[1]),
        }
    }

    fn to_regs(self, regs: &mut [u32]) {
        regs[0] = self.x.to_bits();
        regs[1] = self.y.to_bits();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub(super) struct GLKVector3 {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) z: f32,
}

unsafe impl SafeRead for GLKVector3 {}
impl_GuestRet_for_large_struct!(GLKVector3);
impl GuestArg for GLKVector3 {
    const REG_COUNT: usize = 3;

    fn from_regs(regs: &[u32]) -> Self {
        Self {
            x: f32::from_bits(regs[0]),
            y: f32::from_bits(regs[1]),
            z: f32::from_bits(regs[2]),
        }
    }

    fn to_regs(self, regs: &mut [u32]) {
        regs[0] = self.x.to_bits();
        regs[1] = self.y.to_bits();
        regs[2] = self.z.to_bits();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub(super) struct GLKVector4 {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) z: f32,
    pub(super) w: f32,
}

impl GuestArg for GLKVector4 {
    const REG_COUNT: usize = 4;

    fn from_regs(regs: &[u32]) -> Self {
        Self {
            x: f32::from_bits(regs[0]),
            y: f32::from_bits(regs[1]),
            z: f32::from_bits(regs[2]),
            w: f32::from_bits(regs[3]),
        }
    }

    fn to_regs(self, regs: &mut [u32]) {
        regs[0] = self.x.to_bits();
        regs[1] = self.y.to_bits();
        regs[2] = self.z.to_bits();
        regs[3] = self.w.to_bits();
    }
}

fn matrix4_identity() -> GLKMatrix4 {
    GLKMatrix4 {
        m: [
            1.0, 0.0, 0.0, 0.0, //
            0.0, 1.0, 0.0, 0.0, //
            0.0, 0.0, 1.0, 0.0, //
            0.0, 0.0, 0.0, 1.0, //
        ],
    }
}

fn glk_matrix4_identity(env: &mut Environment) -> ConstVoidPtr {
    env.mem
        .alloc_and_write(matrix4_identity())
        .cast_void()
        .cast_const()
}

fn invert_matrix4(matrix: GLKMatrix4) -> (GLKMatrix4, bool) {
    let mut augmented = [[0.0_f64; 8]; 4];
    for row in 0..4 {
        for column in 0..4 {
            augmented[row][column] = matrix.m[column * 4 + row] as f64;
        }
        augmented[row][row + 4] = 1.0;
    }

    for column in 0..4 {
        let mut pivot_row = column;
        for row in column + 1..4 {
            if augmented[row][column].abs() > augmented[pivot_row][column].abs() {
                pivot_row = row;
            }
        }
        let pivot = augmented[pivot_row][column];
        if !pivot.is_finite() || pivot == 0.0 {
            return (matrix4_identity(), false);
        }
        augmented.swap(column, pivot_row);
        for value in &mut augmented[column] {
            *value /= pivot;
        }
        for row in 0..4 {
            if row == column {
                continue;
            }
            let factor = augmented[row][column];
            for index in 0..8 {
                augmented[row][index] -= factor * augmented[column][index];
            }
        }
    }

    let mut inverse = GLKMatrix4 { m: [0.0; 16] };
    for row in 0..4 {
        for column in 0..4 {
            let value = augmented[row][column + 4] as f32;
            if !value.is_finite() {
                return (matrix4_identity(), false);
            }
            inverse.m[column * 4 + row] = value;
        }
    }
    (inverse, true)
}

/// Matrix product in GLKit's column-major layout.
fn multiply_matrix4(a: GLKMatrix4, b: GLKMatrix4) -> GLKMatrix4 {
    GLKMatrix4 {
        m: std::array::from_fn(|index| {
            let (column, row) = (index / 4, index % 4);
            (0..4)
                .map(|k| a.m[k * 4 + row] * b.m[column * 4 + k])
                .sum()
        }),
    }
}

fn multiply_matrix4_vector4(matrix: GLKMatrix4, vector: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|row| {
        (0..4)
            .map(|k| matrix.m[k * 4 + row] * vector[k])
            .sum()
    })
}

/// `GLKMathUnproject` takes window coordinates in the viewport's `(x, y,
/// width, height)` rectangle and returns object-space coordinates.
/// <https://developer.apple.com/documentation/glkit/glkmathunproject(_:_:_:_:_:)>
fn unproject_point(
    window: GLKVector3,
    model: GLKMatrix4,
    projection: GLKMatrix4,
    viewport: [i32; 4],
) -> Option<GLKVector3> {
    let (inverse, invertible) = invert_matrix4(multiply_matrix4(projection, model));
    if !invertible {
        return None;
    }
    let [x, y, width, height] = viewport;
    if width == 0 || height == 0 {
        return None;
    }
    let clip = [
        (window.x - x as f32) / width as f32 * 2.0 - 1.0,
        (window.y - y as f32) / height as f32 * 2.0 - 1.0,
        window.z * 2.0 - 1.0,
        1.0,
    ];
    let object = multiply_matrix4_vector4(inverse, clip);
    if object[3] == 0.0 {
        return None;
    }
    Some(GLKVector3 {
        x: object[0] / object[3],
        y: object[1] / object[3],
        z: object[2] / object[3],
    })
}

#[allow(non_snake_case)]
pub fn GLKMatrix4Invert(
    env: &mut Environment,
    matrix: GLKMatrix4,
    is_invertible: MutPtr<u8>,
) -> GLKMatrix4 {
    static TRACE_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let (inverse, invertible) = invert_matrix4(matrix);
    if crate::env_flag_cached!("TOUCHHLE_TRACE_GLK")
        && TRACE_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 8
    {
        log!(
            "GLKMatrix4Invert input={:?} invertible={} output={:?}",
            matrix.m,
            invertible,
            inverse.m
        );
    }
    if !is_invertible.is_null() {
        env.mem.write(is_invertible, u8::from(invertible));
    }
    inverse
}

/// Returns the object-space point, and reports through `success` whether the
/// model/projection pair could be inverted. Coordinates outside the viewport
/// are still unprojected, matching GLKit.
/// <https://developer.apple.com/documentation/glkit/glkmathunproject(_:_:_:_:_:)>
#[allow(non_snake_case)]
pub(super) fn GLKMathUnproject(
    env: &mut Environment,
    window: GLKVector3,
    model: GLKMatrix4,
    projection: GLKMatrix4,
    viewport: ConstPtr<i32>,
    success: MutPtr<u8>,
) -> GLKVector3 {
    let object = if viewport.is_null() {
        None
    } else {
        let viewport = [
            env.mem.read(viewport),
            env.mem.read(viewport + 1),
            env.mem.read(viewport + 2),
            env.mem.read(viewport + 3),
        ];
        unproject_point(window, model, projection, viewport)
    };
    if !success.is_null() {
        env.mem.write(success, u8::from(object.is_some()));
    }
    object.unwrap_or(GLKVector3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    })
}

pub const CONSTANTS: ConstantExports = &[(
    "_GLKMatrix4Identity",
    HostConstant::Custom(glk_matrix4_identity),
)];

pub const FUNCTIONS: FunctionExports = &[
    export_c_func!(GLKMatrix4Invert(_, _)),
    export_c_func!(GLKMathUnproject(_, _, _, _, _)),
];

/// `GLKViewDrawableColorFormat` etc. are plain integer enums in GLKit's
/// headers (`GLKView.h`); `GLKViewDrawableColorFormatRGBA8888` (the
/// documented default for `drawableColorFormat`) is 0, as are the
/// "FormatNone" defaults of the depth/stencil/multisample properties.
type GLKViewDrawableFormat = NSInteger;

struct GLKViewHostObject {
    superclass: UIViewHostObject,
    /// `EAGLContext*` — retained (documented as a strong property).
    context: id,
    /// `id<GLKViewDelegate>` — NOT retained. Apple documents the property
    /// as `unowned(unsafe)`.
    delegate: id,
    drawable_color_format: GLKViewDrawableFormat,
    drawable_depth_format: GLKViewDrawableFormat,
    drawable_stencil_format: GLKViewDrawableFormat,
    drawable_multisample: GLKViewDrawableFormat,
    enable_set_needs_display: bool,
}
impl_HostObject_with_superclass!(GLKViewHostObject);
impl Default for GLKViewHostObject {
    fn default() -> Self {
        GLKViewHostObject {
            superclass: Default::default(),
            context: nil,
            delegate: nil,
            // Defaults documented in GLKView.h: RGBA8888 color, no depth,
            // no stencil, no multisampling.
            drawable_color_format: 0,
            drawable_depth_format: 0,
            drawable_stencil_format: 0,
            drawable_multisample: 0,
            // Apple docs: "the default value is YES".
            enable_set_needs_display: true,
        }
    }
}

struct GLKViewControllerHostObject {
    superclass: UIViewControllerHostObject,
    /// `id<GLKViewControllerDelegate>` — NOT retained
    /// (`unowned(unsafe)` per Apple docs).
    delegate: id,
    preferred_frames_per_second: NSInteger,
    pause_on_will_resign_active: bool,
    resume_on_did_become_active: bool,
    paused: bool,
    frames_displayed: NSInteger,
}
impl_HostObject_with_superclass!(GLKViewControllerHostObject);
impl Default for GLKViewControllerHostObject {
    fn default() -> Self {
        GLKViewControllerHostObject {
            superclass: Default::default(),
            delegate: nil,
            // Apple docs: "The default value is 30" frames per second.
            preferred_frames_per_second: 30,
            // Apple docs: both pause/resume-on-app-state flags default
            // to YES.
            pause_on_will_resign_active: true,
            resume_on_did_become_active: true,
            paused: false,
            frames_displayed: 0,
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation GLKView: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<GLKViewHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame context:(id)context {
    let this: id = msg![env; this initWithFrame:frame];
    () = msg![env; this setContext:context];
    this
}

- (id)context {
    env.objc.borrow::<GLKViewHostObject>(this).context
}
- (())setContext:(id)context { // EAGLContext*
    let old = env.objc.borrow::<GLKViewHostObject>(this).context;
    if context != nil {
        retain(env, context);
    }
    env.objc.borrow_mut::<GLKViewHostObject>(this).context = context;
    if old != nil {
        release(env, old);
    }
}

- (id)delegate {
    env.objc.borrow::<GLKViewHostObject>(this).delegate
}
- (())setDelegate:(id)delegate {
    // Apple documents this property as unowned(unsafe): do not retain.
    env.objc.borrow_mut::<GLKViewHostObject>(this).delegate = delegate;
}

- (GLKViewDrawableFormat)drawableColorFormat {
    env.objc.borrow::<GLKViewHostObject>(this).drawable_color_format
}
- (())setDrawableColorFormat:(GLKViewDrawableFormat)format {
    env.objc.borrow_mut::<GLKViewHostObject>(this).drawable_color_format = format;
}
- (GLKViewDrawableFormat)drawableDepthFormat {
    env.objc.borrow::<GLKViewHostObject>(this).drawable_depth_format
}
- (())setDrawableDepthFormat:(GLKViewDrawableFormat)format {
    env.objc.borrow_mut::<GLKViewHostObject>(this).drawable_depth_format = format;
}
- (GLKViewDrawableFormat)drawableStencilFormat {
    env.objc.borrow::<GLKViewHostObject>(this).drawable_stencil_format
}
- (())setDrawableStencilFormat:(GLKViewDrawableFormat)format {
    env.objc.borrow_mut::<GLKViewHostObject>(this).drawable_stencil_format = format;
}
- (GLKViewDrawableFormat)drawableMultisample {
    env.objc.borrow::<GLKViewHostObject>(this).drawable_multisample
}
- (())setDrawableMultisample:(GLKViewDrawableFormat)format {
    env.objc.borrow_mut::<GLKViewHostObject>(this).drawable_multisample = format;
}

- (NSInteger)drawableWidth {
    let bounds: CGRect = msg![env; this bounds];
    let scale: CGFloat = msg![env; this contentScaleFactor];
    (bounds.size.width * scale) as NSInteger
}
- (NSInteger)drawableHeight {
    let bounds: CGRect = msg![env; this bounds];
    let scale: CGFloat = msg![env; this contentScaleFactor];
    (bounds.size.height * scale) as NSInteger
}

- (bool)enableSetNeedsDisplay {
    env.objc.borrow::<GLKViewHostObject>(this).enable_set_needs_display
}
- (())setEnableSetNeedsDisplay:(bool)value {
    env.objc.borrow_mut::<GLKViewHostObject>(this).enable_set_needs_display = value;
}

- (())bindDrawable {
    // Apple docs: binds the underlying framebuffer object so the app can
    // render into it. touchHLE's EAGL path renders straight into the
    // app's own framebuffer/renderbuffer objects, so there is no separate
    // GLKit-managed FBO to bind here.
    log_dbg!("[(GLKView*){:?} bindDrawable] (no-op)", this);
}

- (())deleteDrawable {
    log_dbg!("[(GLKView*){:?} deleteDrawable] (no-op)", this);
}

- (())display {
    // Apple docs: GLKView "uses the regular view drawing cycle…, calling
    // its drawRect: method whenever the contents of the view need to be
    // updated" and apps either override drawRect: in a subclass or
    // implement the delegate's glkView:drawInRect:.
    let bounds: CGRect = msg![env; this bounds];
    let delegate = env.objc.borrow::<GLKViewHostObject>(this).delegate;
    if delegate != nil {
        () = msg![env; delegate glkView:this drawInRect:bounds];
    } else {
        () = msg![env; this drawRect:bounds];
    }
}

- (())dealloc {
    let context = env.objc.borrow::<GLKViewHostObject>(this).context;
    if context != nil {
        release(env, context);
    }
    // delegate is unowned: nothing to release.
    env.objc.dealloc_object(this, &mut env.mem);
}

@end

@implementation GLKViewController: UIViewController

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<GLKViewControllerHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)delegate {
    env.objc.borrow::<GLKViewControllerHostObject>(this).delegate
}
- (())setDelegate:(id)delegate {
    // Apple documents this property as unowned(unsafe): do not retain.
    env.objc.borrow_mut::<GLKViewControllerHostObject>(this).delegate = delegate;
}

- (NSInteger)preferredFramesPerSecond {
    env.objc.borrow::<GLKViewControllerHostObject>(this).preferred_frames_per_second
}
- (())setPreferredFramesPerSecond:(NSInteger)fps {
    // Apple docs: setting 0 (or a negative value) means "use the default",
    // which is 30 fps.
    let fps = if fps <= 0 { 30 } else { fps };
    env.objc
        .borrow_mut::<GLKViewControllerHostObject>(this)
        .preferred_frames_per_second = fps;
}
- (NSInteger)framesPerSecond {
    // "The actual rate that the view controller attempts to call the view
    // to update its contents" — we don't decimate, so report the
    // preferred rate.
    env.objc.borrow::<GLKViewControllerHostObject>(this).preferred_frames_per_second
}

- (bool)isPaused {
    env.objc.borrow::<GLKViewControllerHostObject>(this).paused
}
- (())setPaused:(bool)paused {
    let host = env.objc.borrow_mut::<GLKViewControllerHostObject>(this);
    let was_paused = host.paused;
    host.paused = paused;
    if was_paused == paused {
        return;
    }
    // Apple docs (GLKViewControllerDelegate): the delegate receives
    // glkViewController:willPause: when the pause state changes.
    let delegate = env.objc.borrow::<GLKViewControllerHostObject>(this).delegate;
    if delegate != nil {
        if let Some(sel) = env.objc.lookup_selector("glkViewController:willPause:") {
            let responds: bool = msg![env; delegate respondsToSelector:sel];
            if responds {
                () = msg![env; delegate glkViewController:this willPause:paused];
            }
        }
    }
}

- (bool)pauseOnWillResignActive {
    env.objc.borrow::<GLKViewControllerHostObject>(this).pause_on_will_resign_active
}
- (())setPauseOnWillResignActive:(bool)value {
    env.objc
        .borrow_mut::<GLKViewControllerHostObject>(this)
        .pause_on_will_resign_active = value;
}
- (bool)resumeOnDidBecomeActive {
    env.objc.borrow::<GLKViewControllerHostObject>(this).resume_on_did_become_active
}
- (())setResumeOnDidBecomeActive:(bool)value {
    env.objc
        .borrow_mut::<GLKViewControllerHostObject>(this)
        .resume_on_did_become_active = value;
}

- (NSInteger)framesDisplayed {
    env.objc.borrow::<GLKViewControllerHostObject>(this).frames_displayed
}
- (NSTimeInterval)timeSinceLastUpdate {
    // We do not drive the GLKit animation loop yet; report a plausible
    // single-frame delta so dt-based game logic keeps advancing.
    let fps = env.objc.borrow::<GLKViewControllerHostObject>(this).preferred_frames_per_second;
    1.0 / (fps.max(1) as NSTimeInterval)
}
- (NSTimeInterval)timeSinceLastDraw {
    let fps = env.objc.borrow::<GLKViewControllerHostObject>(this).preferred_frames_per_second;
    1.0 / (fps.max(1) as NSTimeInterval)
}
- (NSTimeInterval)timeSinceFirstResume {
    let frames = env.objc.borrow::<GLKViewControllerHostObject>(this).frames_displayed;
    let fps = env.objc.borrow::<GLKViewControllerHostObject>(this).preferred_frames_per_second;
    (frames as NSTimeInterval) / (fps.max(1) as NSTimeInterval)
}
- (NSTimeInterval)timeSinceLastResume {
    msg![env; this timeSinceFirstResume]
}

@end

};

pub const DYLIB: crate::dyld::HostDylib = crate::dyld::HostDylib {
    path: "/System/Library/Frameworks/GLKit.framework/GLKit",
    aliases: &[],
    class_exports: &[CLASSES],
    constant_exports: &[CONSTANTS],
    function_exports: &[FUNCTIONS],
};

#[cfg(test)]
mod tests {
    use super::{invert_matrix4, matrix4_identity, unproject_point, GLKMatrix4, GLKVector3};

    #[test]
    fn glk_matrix4_invert_handles_identity_and_translation() {
        let matrix = GLKMatrix4 {
            m: [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 4.0, -5.0, 6.0, 1.0,
            ],
        };
        let (inverse, invertible) = invert_matrix4(matrix);
        assert!(invertible);
        assert_eq!(
            inverse.m,
            [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -4.0, 5.0, -6.0, 1.0,]
        );
    }

    #[test]
    fn glk_matrix4_invert_reports_singular_matrices() {
        let matrix = GLKMatrix4 { m: [0.0; 16] };
        let (inverse, invertible) = invert_matrix4(matrix);
        assert!(!invertible);
        assert_eq!(inverse, matrix4_identity());
    }

    /// With identity model and projection, window coordinates map linearly
    /// onto normalised device coordinates, with y growing downwards.
    #[test]
    fn glk_math_unproject_maps_window_coordinates_to_object_space() {
        let identity = matrix4_identity();
        let viewport = [0, 0, 100, 100];

        let centre = unproject_point(
            GLKVector3 {
                x: 50.0,
                y: 50.0,
                z: 0.5,
            },
            identity,
            identity,
            viewport,
        )
        .unwrap();
        assert_eq!(centre, GLKVector3 { x: 0.0, y: 0.0, z: 0.0 });

        let corner = unproject_point(
            GLKVector3 {
                x: 100.0,
                y: 100.0,
                z: 1.0,
            },
            identity,
            identity,
            viewport,
        )
        .unwrap();
        assert_eq!(corner, GLKVector3 { x: 1.0, y: 1.0, z: 1.0 });

        assert_eq!(
            unproject_point(
                GLKVector3 {
                    x: 0.0,
                    y: 0.0,
                    z: 0.0,
                },
                identity,
                GLKMatrix4 { m: [0.0; 16] },
                viewport,
            ),
            None
        );
    }

    /// The model transform is applied before the projection, so the inverse
    /// of `projection * model` is what unprojects; the opposite order gives a
    /// different point once the two matrices do not commute.
    #[test]
    fn glk_math_unproject_inverts_projection_times_model() {
        let mut projection = GLKMatrix4 { m: [0.0; 16] };
        projection.m[0] = 2.0;
        projection.m[5] = 2.0;
        projection.m[10] = 2.0;
        projection.m[15] = 1.0;
        let mut model = matrix4_identity();
        model.m[12] = 10.0;

        let object = unproject_point(
            GLKVector3 {
                x: 100.0,
                y: 50.0,
                z: 0.5,
            },
            model,
            projection,
            [0, 0, 100, 100],
        )
        .unwrap();
        // Inverse projection gives x = 1/2, then inverse translation subtracts 10.
        assert!((object.x + 9.5).abs() < 1e-5);
        assert_eq!(object.y, 0.0);
        assert_eq!(object.z, 0.0);
    }
}

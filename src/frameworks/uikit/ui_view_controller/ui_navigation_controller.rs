/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UINavigationController`.

use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::{ns_array, NSUInteger};
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes,
    release, retain, ClassExports, NSZonePtr,
};

#[derive(Default)]
pub(crate) struct UINavigationControllerHostObject {
    superclass: super::UIViewControllerHostObject,
    pub(crate) delegate: id,
    navigation_stack: Vec<id>,
    layout_running: bool,
    navigation_bar: id,
    toolbar: id,
    /// Whether the navigation bar is hidden.
    navigation_bar_hidden: bool,
    /// Whether the bottom toolbar is hidden (default true in UIKit).
    toolbar_hidden: bool,
    /// Whether an interactive pop gesture recogniser is enabled (iOS 7+).
    interactve_pop_gesture_enabled: bool,
    /// Whether we hide the navigation bar on keyboard show (default false).
    hides_bars_on_keyboard_appearance: bool,
    /// Whether we hide bars when swiping down (default false).
    hides_bars_on_swipe: bool,
    /// Whether we hide bars when the tap is in a readable content area (default
    //false).
    hides_bars_when_browsing: bool,
}
impl_HostObject_with_superclass!(UINavigationControllerHostObject);

impl UINavigationControllerHostObject {
    pub(crate) fn new() -> Self {
        Self {
            toolbar_hidden: true,
            interactve_pop_gesture_enabled: true,
            ..Default::default()
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UINavigationController: UIViewController

// =========================================================================
// MARK: - Allocation / init
// =========================================================================

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UINavigationControllerHostObject::new());
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithRootViewController:(id)root_vc {
    let this: id = msg![env; this init];
    let _: () = msg![env; this pushViewController:root_vc animated:false];
    this
}

- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];

    // A UINavigationController archived in a nib stores its view-controller
    // stack under "UIViewControllers" (and, for older archives, its already
    // built navigation bar under "UINavigationBar"). UIViewController's
    // -initWithCoder: doesn't know about these keys, so without decoding them
    // here the navigation stack stays empty: -topViewController is nil, the
    // root view controller's view is never added, and the app shows an empty
    // (white) navigation controller view. Decode them so the initial view
    // controller actually appears.
    let bar_key = get_static_str(env, "UINavigationBar");
    let bar: id = msg![env; coder decodeObjectForKey:bar_key];
    if bar != nil {
        retain(env, bar);
        let old = std::mem::replace(
            &mut env.objc.borrow_mut::<UINavigationControllerHostObject>(this).navigation_bar,
            bar,
        );
        if old != nil { release(env, old); }
    }

    let vcs_key = get_static_str(env, "UIViewControllers");
    let view_controllers: id = msg![env; coder decodeObjectForKey:vcs_key];
    if view_controllers != nil {
        let count: NSUInteger = msg![env; view_controllers count];
        if count != 0 {
            let _: () = msg![env; this setViewControllers:view_controllers];
        }
    }

    this
}

- (id)initWithNavigationBarClass:(id)_nav_bar_class
                    toolbarClass:(id)_toolbar_class {
    // Ignore custom classes — use standard UINavigationBar/UIToolbar.
    msg![env; this init]
}

// The container owns both its bar and its child layout. See Apple's
// View Controller Catalog, "Anatomy of a Navigation Interface".
// https://developer.apple.com/library/archive/documentation/WindowsViews/Conceptual/ViewControllerCatalog/Chapters/NavigationControllers.html
- (())loadView {
    let bounds = if env.window.is_some() {
        let screen: id = msg_class![env; UIScreen mainScreen];
        msg![env; screen bounds]
    } else { CGRect { origin: CGPoint::default(), size: CGSize { width: 320.0, height: 480.0 } } };
    let root: id = msg_class![env; _touchHLE_NavigationControllerView alloc];
    let root: id = msg![env; root initWithFrame:bounds];
    () = msg![env; this setView:root];
    release(env, root);
}
- (())viewDidLayoutSubviews {
    () = msg_super![env; this viewDidLayoutSubviews];
    () = msg![env; this _touchHLE_layoutNavigation];
}
- (())_touchHLE_layoutNavigation {
    let root = env.objc.borrow::<UINavigationControllerHostObject>(this).superclass.view;
    if root == nil { return; }
    let h = env.objc.borrow_mut::<UINavigationControllerHostObject>(this);
    if h.layout_running || h.navigation_stack.is_empty() { return; }
    h.layout_running = true;
    let stack = h.navigation_stack.clone();
    let bounds: CGRect = msg![env; root bounds];
    let top = *stack.last().unwrap();
    let bar: id = msg![env; this navigationBar];
    let hidden = env.objc.borrow::<UINavigationControllerHostObject>(this).navigation_bar_hidden;
    let item: id = msg![env; top navigationItem];
    let prompt: id = msg![env; item prompt];
    let height = if hidden { 0.0 } else {
        (if bounds.size.width > bounds.size.height { 32.0 } else { 44.0 }) + if prompt == nil { 0.0 } else { 30.0 }
    };
    let mut items = Vec::new();
    for &controller in &stack { let item: id = msg![env; controller navigationItem]; items.push(retain(env, item)); }
    let items = ns_array::from_vec(env, items);
    () = msg![env; bar setItems:items];
    release(env, items);
    let frame = CGRect { origin: CGPoint { x: bounds.origin.x, y: bounds.origin.y }, size: CGSize { width: bounds.size.width, height } };
    () = msg![env; bar setFrame:frame];
    () = msg![env; bar setHidden:hidden];
    () = msg![env; bar setDelegate:this];
    let content = CGRect { origin: CGPoint { x: bounds.origin.x, y: bounds.origin.y + height }, size: CGSize { width: bounds.size.width, height: (bounds.size.height - height).max(0.0) } };
    for controller in stack {
        let view: id = msg![env; controller view];
        () = msg![env; view setHidden:(controller != top)];
        if controller == top {
            () = msg![env; controller viewWillLayoutSubviews];
            () = msg![env; view setFrame:content];
            () = msg![env; view layoutSubviews];
            () = msg![env; controller viewDidLayoutSubviews];
        }
    }
    let parent: id = msg![env; bar superview];
    if parent != root { () = msg![env; root addSubview:bar]; }
    else { () = msg![env; root bringSubviewToFront:bar]; }
    () = msg![env; bar layoutSubviews];
    env.objc.borrow_mut::<UINavigationControllerHostObject>(this).layout_running = false;
}
- (bool)navigationBar:(id)_bar shouldPopItem:(id)_item {
    let _: id = msg![env; this popViewControllerAnimated:true];
    false // The controller has already synchronized the bar's item stack.
}

// =========================================================================
// MARK: - Delegate
// =========================================================================

- (id)delegate {
    env.objc.borrow::<UINavigationControllerHostObject>(this).delegate
}

- (())setDelegate:(id)delegate {
    log_dbg!("UINavigationController setDelegate:{:?}", delegate);
    env.objc.borrow_mut::<UINavigationControllerHostObject>(this).delegate = delegate;
}

// =========================================================================
// MARK: - Push / pop
// =========================================================================

- (())pushViewController:(id)view_controller animated:(bool)_animated {
    if view_controller == nil { return; }

    {
        let stack = &mut env
            .objc
            .borrow_mut::<UINavigationControllerHostObject>(this)
            .navigation_stack;
        if stack.contains(&view_controller) {
            log!("Warning: UINavigationController pushViewController: vc already on stack — ignoring");
            return;
        }
        stack.push(view_controller);
    }
    retain(env, view_controller);
    let _: () = msg![env; view_controller setNavigationController:this];
    // A navigation stack is a container relationship, including its root.
    // https://developer.apple.com/library/archive/featuredarticles/ViewControllerPGforiPhoneOS/ImplementingaContainerViewController.html
    let _: () = msg![env; view_controller setParentViewController:this];

    // Delegate: willShow. Use `register_host_selector` so we don't panic when
    // the app's delegate doesn't implement (and therefore never references)
    // this selector — some apps simply skip the optional UINavigationController
    // delegate callbacks.
    let delegate = env.objc.borrow::<UINavigationControllerHostObject>(this).delegate;
    if delegate != nil {
        let sel = env.objc.register_host_selector(
            "navigationController:willShowViewController:animated:".to_string(),
            &mut env.mem,
        );
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            let _: () = msg![env; delegate navigationController:this
                                       willShowViewController:view_controller
                                                     animated:false];
        }
    }

    // Add view. The navigation controller is responsible for sizing the
    // pushed view controller's view to fill its content area; views loaded
    // from a nib often arrive with a zero (or stale) frame, which would
    // otherwise leave the screen blank.
    let self_view: id = msg![env; this view];
    let vc_view:   id = msg![env; view_controller view];
    let bounds: CGRect = msg![env; self_view bounds];
    let _: () = msg![env; vc_view setFrame:bounds];
    let _: () = msg![env; view_controller viewWillAppear:false];
    let _: () = msg![env; self_view addSubview:vc_view];
    let _: () = msg![env; view_controller viewDidAppear:false];
    () = msg![env; this _touchHLE_layoutNavigation];

    // Delegate: didShow.
    let delegate = env.objc.borrow::<UINavigationControllerHostObject>(this).delegate;
    if delegate != nil {
        let sel = env.objc.register_host_selector(
            "navigationController:didShowViewController:animated:".to_string(),
            &mut env.mem,
        );
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            let _: () = msg![env; delegate navigationController:this
                                        didShowViewController:view_controller
                                                     animated:false];
        }
    }
}

- (id)popViewControllerAnimated:(bool)animated {
    let stack_len = env.objc
        .borrow::<UINavigationControllerHostObject>(this)
        .navigation_stack.len();
    if stack_len <= 1 {
        // Cannot pop the root view controller.
        return nil;
    }
    let popped = env.objc
        .borrow_mut::<UINavigationControllerHostObject>(this)
        .navigation_stack
        .pop()
        .unwrap_or(nil);

    if popped == nil { return nil; }

    // Remove view.
    let vc_view: id = msg![env; popped view];
    let _: () = msg![env; popped viewWillDisappear:animated];
    let _: () = msg![env; vc_view removeFromSuperview];
    let _: () = msg![env; popped viewDidDisappear:animated];

    let _: () = msg![env; popped setNavigationController:nil];
    let _: () = msg![env; popped setParentViewController:nil];

    // Show the new top VC.
    let new_top = env.objc
        .borrow::<UINavigationControllerHostObject>(this)
        .navigation_stack
        .last()
        .copied()
        .unwrap_or(nil);
    if new_top != nil {
        let _: () = msg![env; new_top viewWillAppear:animated];
        let _: () = msg![env; new_top viewDidAppear:animated];
        // Delegate: didShow new top.
        let delegate = env.objc.borrow::<UINavigationControllerHostObject>(this).delegate;
        if delegate != nil {
            let sel = env.objc.register_host_selector(
                "navigationController:didShowViewController:animated:".to_string(),
                &mut env.mem,
            );
            let responds: bool = msg![env; delegate respondsToSelector:sel];
            if responds {
                let _: () = msg![env; delegate navigationController:this
                                            didShowViewController:new_top
                                                         animated:animated];
            }
        }
    }

    () = msg![env; this _touchHLE_layoutNavigation];
    autorelease(env, popped)
}

- (id)popToViewController:(id)view_controller animated:(bool)animated {
    let idx = {
        let stack = &env.objc
            .borrow::<UINavigationControllerHostObject>(this)
            .navigation_stack;
        stack.iter().position(|&vc| vc == view_controller)
    };
    let Some(idx) = idx else {
        log!("Warning: UINavigationController popToViewController: vc not in stack");
        return nil;
    };

    // Collect everything above idx.
    let to_pop: Vec<id> = env.objc
        .borrow_mut::<UINavigationControllerHostObject>(this)
        .navigation_stack
        .drain((idx + 1)..)
        .collect();

    let popped_arr_vcs = to_pop.clone();
    for &controller in &popped_arr_vcs { retain(env, controller); }
    for &vc in &to_pop {
        let vc_view: id = msg![env; vc view];
        let _: () = msg![env; vc viewWillDisappear:animated];
        let _: () = msg![env; vc_view removeFromSuperview];
        let _: () = msg![env; vc viewDidDisappear:animated];
        let _: () = msg![env; vc setNavigationController:nil];
        let _: () = msg![env; vc setParentViewController:nil];
        release(env, vc);
    }

    if view_controller != nil {
        let _: () = msg![env; view_controller viewWillAppear:animated];
        let _: () = msg![env; view_controller viewDidAppear:animated];
    }

    // Build return array of popped VCs.
    let retained = popped_arr_vcs;
    let arr = ns_array::from_vec(env, retained);
    () = msg![env; this _touchHLE_layoutNavigation];
    autorelease(env, arr)
}

- (id)popToRootViewControllerAnimated:(bool)animated {
    let root = {
        let stack = &env.objc
            .borrow::<UINavigationControllerHostObject>(this)
            .navigation_stack;
        stack.first().copied().unwrap_or(nil)
    };
    if root == nil { return nil; }
    msg![env; this popToViewController:root animated:animated]
}

// =========================================================================
// MARK: - Appearance forwarding
// =========================================================================
// UINavigationController forwards appearance callbacks to the currently
// visible (top) child view controller. Without this, frameworks like
// IASKAppSettingsKit — which do all of their setup in -viewWillAppear: —
// never get called when the navigation controller itself is presented.

- (())viewWillAppear:(bool)animated {
    () = msg_super![env; this viewWillAppear:animated];
    let top: id = msg![env; this topViewController];
    if top != nil {
        () = msg![env; top viewWillAppear:animated];
    }
}

- (())viewDidAppear:(bool)animated {
    () = msg_super![env; this viewDidAppear:animated];
    let top: id = msg![env; this topViewController];
    if top != nil {
        () = msg![env; top viewDidAppear:animated];
    }
}

- (())viewWillDisappear:(bool)animated {
    let top: id = msg![env; this topViewController];
    if top != nil {
        () = msg![env; top viewWillDisappear:animated];
    }
    () = msg_super![env; this viewWillDisappear:animated];
}

- (())viewDidDisappear:(bool)animated {
    let top: id = msg![env; this topViewController];
    if top != nil {
        () = msg![env; top viewDidDisappear:animated];
    }
    () = msg_super![env; this viewDidDisappear:animated];
}

// =========================================================================
// MARK: - Stack accessors
// =========================================================================

- (id)topViewController {
    env.objc
        .borrow::<UINavigationControllerHostObject>(this)
        .navigation_stack
        .last()
        .copied()
        .unwrap_or(nil)
}

- (id)visibleViewController {
    // Same as topViewController in our stub (no modal on top of nav).
    msg![env; this topViewController]
}

- (id)viewControllers {
    let vcs = env.objc
        .borrow::<UINavigationControllerHostObject>(this)
        .navigation_stack
        .clone();
    for &vc in &vcs { retain(env, vc); }
    let arr = ns_array::from_vec(env, vcs);
    autorelease(env, arr)
}

- (())setViewControllers:(id)controllers {
    let _: () = msg![env; this setViewControllers:controllers animated:false];
}

- (())setViewControllers:(id)controllers animated:(bool)animated {
    let _self_view = env.objc
        .borrow::<UINavigationControllerHostObject>(this)
        .superclass
        .view;

    let old_stack = std::mem::take(
        &mut env.objc
            .borrow_mut::<UINavigationControllerHostObject>(this)
            .navigation_stack,
    );
    for vc in old_stack {
        let vc_view: id = msg![env; vc view];
        let _: () = msg![env; vc_view removeFromSuperview];
        let _: () = msg![env; vc setNavigationController:nil];
        let _: () = msg![env; vc setParentViewController:nil];
        release(env, vc);
    }

    let count: NSUInteger = msg![env; controllers count];
    if count == 0 { return; }

    let mut tmp: Vec<id> = Vec::new();
    for i in 0..(count - 1) {
        let vc: id = msg![env; controllers objectAtIndex:i];
        retain(env, vc);
        let _: () = msg![env; vc setNavigationController:this];
        let _: () = msg![env; vc setParentViewController:this];
        tmp.push(vc);
    }
    env.objc
        .borrow_mut::<UINavigationControllerHostObject>(this)
        .navigation_stack = tmp;

    let last_vc: id = msg![env; controllers objectAtIndex:(count - 1)];
    let _: () = msg![env; this pushViewController:last_vc animated:animated];
}

// =========================================================================
// MARK: - Navigation bar
// =========================================================================

- (id)navigationBar {
    let bar = env.objc.borrow::<UINavigationControllerHostObject>(this).navigation_bar;
    if bar != nil { return bar; }
    let bar: id = msg_class![env; UINavigationBar alloc];
    let bar: id = msg![env; bar init];
    env.objc.borrow_mut::<UINavigationControllerHostObject>(this).navigation_bar = bar;
    bar
}

- (bool)isNavigationBarHidden {
    env.objc.borrow::<UINavigationControllerHostObject>(this).navigation_bar_hidden
}

- (())setNavigationBarHidden:(bool)hidden {
    let _: () = msg![env; this setNavigationBarHidden:hidden animated:false];
}

- (())setNavigationBarHidden:(bool)hidden animated:(bool)_animated {
    log_dbg!("UINavigationController setNavigationBarHidden:{}", hidden);
    env.objc.borrow_mut::<UINavigationControllerHostObject>(this).navigation_bar_hidden = hidden;
    let bar = msg![env; this navigationBar];
    let _: () = msg![env; bar setHidden:hidden];
    () = msg![env; this _touchHLE_layoutNavigation];
}

// =========================================================================
// MARK: - Toolbar
// =========================================================================

- (id)toolbar {
    let tb = env.objc.borrow::<UINavigationControllerHostObject>(this).toolbar;
    if tb != nil { return tb; }
    let tb: id = msg_class![env; UIToolbar alloc];
    let tb: id = msg![env; tb init];
    env.objc.borrow_mut::<UINavigationControllerHostObject>(this).toolbar = tb;
    tb
}

- (bool)isToolbarHidden {
    env.objc.borrow::<UINavigationControllerHostObject>(this).toolbar_hidden
}

- (())setToolbarHidden:(bool)hidden {
    let _: () = msg![env; this setToolbarHidden:hidden animated:false];
}

- (())setToolbarHidden:(bool)hidden animated:(bool)_animated {
    log_dbg!("UINavigationController setToolbarHidden:{}", hidden);
    env.objc.borrow_mut::<UINavigationControllerHostObject>(this).toolbar_hidden = hidden;
    let tb = msg![env; this toolbar];
    let _: () = msg![env; tb setHidden:hidden];
}

// =========================================================================
// MARK: - iOS 7+ bar-hiding properties
// =========================================================================

- (bool)hidesBarsOnKeyboardAppearance {
    env.objc.borrow::<UINavigationControllerHostObject>(this).hides_bars_on_keyboard_appearance
}
- (())setHidesBarsOnKeyboardAppearance:(bool)v {
    env.objc.borrow_mut::<UINavigationControllerHostObject>(this)
        .hides_bars_on_keyboard_appearance = v;
}

- (bool)hidesBarsOnSwipe {
    env.objc.borrow::<UINavigationControllerHostObject>(this).hides_bars_on_swipe
}
- (())setHidesBarsOnSwipe:(bool)v {
    env.objc.borrow_mut::<UINavigationControllerHostObject>(this).hides_bars_on_swipe = v;
}

- (bool)hidesBarsWhenBrowsing {
    env.objc.borrow::<UINavigationControllerHostObject>(this).hides_bars_when_browsing
}
- (())setHidesBarsWhenBrowsing:(bool)v {
    env.objc.borrow_mut::<UINavigationControllerHostObject>(this).hides_bars_when_browsing = v;
}

// =========================================================================
// MARK: - Interactive pop gesture (iOS 7+)
// =========================================================================

// Returns a stub gesture recogniser id (nil is safe here — apps check it
// but rarely call methods on it in 32-bit era apps).
- (id)interactivePopGestureRecognizer {
    nil
}

// =========================================================================
// MARK: - Dealloc
// =========================================================================

- (())dealloc {
    let (stack, bar, tb) = {
        let h = env.objc.borrow::<UINavigationControllerHostObject>(this);
        (h.navigation_stack.clone(), h.navigation_bar, h.toolbar)
    };
    for vc in stack {
        let _: () = msg![env; vc setNavigationController:nil];
        let _: () = msg![env; vc setParentViewController:nil];
        release(env, vc);
    }
    release(env, bar);
    release(env, tb);
    () = msg_super![env; this dealloc];
}

// =========================================================================
// MARK: - Description
// =========================================================================

- (id)description {
    let depth = env.objc
        .borrow::<UINavigationControllerHostObject>(this)
        .navigation_stack.len();
    let s = format!("<UINavigationController: {:?}; stackDepth={}>", this, depth);
    let cstr = env.mem.alloc_and_write_cstr(s.as_bytes());
    msg_class![env; NSString stringWithUTF8String:cstr]
}

@end

@implementation _touchHLE_NavigationControllerView: UIView
- (())layoutSubviews {
    () = msg_super![env; this layoutSubviews];
    let controller: id = msg![env; this nextResponder];
    if controller != nil { () = msg![env; controller _touchHLE_layoutNavigation]; }
}
@end

};

#[cfg(test)]
mod navigation_container_tests {
    use super::*;

    #[test]
    fn navigation_controls_release_children_without_retaining_action_targets() {
        let mut environment = crate::environment::touch_test_environment();
        let env = &mut environment;
        let pool: id = msg_class![env; NSAutoreleasePool new];
        let target: id = msg_class![env; NSObject new];
        let selector = env
            .objc
            .register_host_selector("description".into(), &mut env.mem);
        let title = get_static_str(env, "Done");
        let item: id = msg_class![env; UIBarButtonItem alloc];
        let item: id =
            msg![env; item initWithTitle:title style:(0i32) target:target action:selector];
        let count: u32 = msg![env; target retainCount];
        assert_eq!(count, 1);
        release(env, item);
        let button: id = msg_class![env; UIButton new];
        let child: id = msg_class![env; UIView new];
        () = msg![env; button addSubview:child];
        release(env, button);
        let count: u32 = msg![env; child retainCount];
        assert_eq!(count, 1);
        release(env, child);
        release(env, target);
        () = msg![env; pool drain];
    }

    // The container must size its child after rotation and install the bar's
    // controls; pushing/popping updates both the view and item stacks.
    // https://developer.apple.com/library/archive/documentation/WindowsViews/Conceptual/ViewControllerCatalog/Chapters/NavigationControllers.html
    #[test]
    fn navigation_container_tracks_bounds_stack_and_bar_visibility() {
        let mut environment = crate::environment::touch_test_environment();
        let env = &mut environment;
        let pool: id = msg_class![env; NSAutoreleasePool new];
        let root_controller: id = msg_class![env; UIViewController new];
        let initial_title: id = msg![env; root_controller title];
        assert_eq!(initial_title, nil);
        let initial_item: id = msg![env; root_controller navigationItem];
        let initial_item_title: id = msg![env; initial_item title];
        assert_eq!(initial_item_title, nil);
        let root_view: id = msg_class![env; UIView new];
        () = msg![env; root_controller setView:root_view];
        let root_title = get_static_str(env, "Photos");
        () = msg![env; root_controller setTitle:root_title];
        let navigation: id = msg_class![env; UINavigationController alloc];
        let navigation: id = msg![env; navigation initWithRootViewController:root_controller];
        let view: id = msg![env; navigation view];
        let landscape = CGRect {
            origin: CGPoint::default(),
            size: CGSize {
                width: 480.0,
                height: 320.0,
            },
        };
        () = msg![env; view setFrame:landscape];
        () = msg![env; view layoutSubviews];
        let frame: CGRect = msg![env; root_view frame];
        assert_eq!(
            frame,
            CGRect {
                origin: CGPoint { x: 0.0, y: 32.0 },
                size: CGSize {
                    width: 480.0,
                    height: 288.0
                }
            }
        );
        let bar: id = msg![env; navigation navigationBar];
        let parent: id = msg![env; bar superview];
        assert_eq!(parent, view);
        let item: id = msg![env; root_controller navigationItem];
        let actual_item: id = msg![env; bar topItem];
        assert_eq!(actual_item, item);
        let parent: id = msg![env; root_controller parentViewController];
        assert_eq!(parent, navigation);
        let tab_item: id = msg![env; root_controller tabBarItem];
        let title = get_static_str(env, "Settings");
        () = msg![env; root_controller setTitle:title];
        let navigation_title: id = msg![env; item title];
        let tab_title: id = msg![env; tab_item title];
        assert_eq!(navigation_title, title);
        assert_eq!(tab_title, title);
        let second: id = msg_class![env; UIViewController new];
        let second_view: id = msg_class![env; UIView new];
        () = msg![env; second setView:second_view];
        () = msg![env; navigation pushViewController:second animated:false];
        let parent: id = msg![env; second parentViewController];
        assert_eq!(parent, navigation);
        assert!(msg![env; root_view isHidden]);
        assert!(!msg![env; second_view isHidden]);
        let returned: id = msg![env; navigation popViewControllerAnimated:false];
        assert_eq!(returned, second);
        let parent: id = msg![env; second parentViewController];
        assert_eq!(parent, nil);
        let top: id = msg![env; navigation topViewController];
        assert_eq!(top, root_controller);
        assert!(!msg![env; root_view isHidden]);
        () = msg![env; navigation setNavigationBarHidden:true];
        let frame: CGRect = msg![env; root_view frame];
        assert_eq!(frame, landscape);
        assert!(msg![env; bar isHidden]);
        for object in [navigation, root_controller, root_view, second, second_view] {
            release(env, object);
        }
        () = msg![env; pool drain];
    }
}

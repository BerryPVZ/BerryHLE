/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UINavigationBar`.

use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::NSInteger;
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::uikit::ui_view::UIViewHostObject;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, ClassExports, HostObject, NSZonePtr,
};

type UIBarStyle = NSInteger;
const UIBarStyleDefault: UIBarStyle = 0;
const UIBarStyleBlack: UIBarStyle = 1;
const UIBarStyleBlackOpaque: UIBarStyle = 2;
const UIBarStyleBlackTranslucent: UIBarStyle = 3;

#[derive(Default)]
struct UINavigationBarHostObject {
    /// Embedded `UIView` host state. `UINavigationBar` is a `UIView` subclass,
    /// so it must carry the superclass host object; without it every `UIView`
    /// method (frame, subviews, layout, ...) sent to a nav bar fails to borrow
    /// and falls back to a zeroed phantom.
    superclass: UIViewHostObject,
    /// UINavigationBarDelegate — weak reference
    delegate: id,
    bar_style: UIBarStyle,
    translucent: bool,
    tint_color: id,            // UIColor* — retained
    bar_tint_color: id,        // UIColor* — retained
    title_text_attributes: id, // NSDictionary* — retained
    /// NSMutableArray* of UINavigationItem* — retained
    items: id,
    /// UIImage* — retained
    shadow_image: id,
    /// UIImage* background — retained
    background_image: id,
    title_label: id,
    prompt_label: id,
    back_view: id,
}
impl_HostObject_with_superclass!(UINavigationBarHostObject);

// MARK: - UINavigationItem

#[derive(Default)]
struct UINavigationItemHostObject {
    title: id,        // NSString* — retained
    title_view: id,   // UIView* — retained
    prompt: id,       // NSString* — retained
    back_button: id,  // UIBarButtonItem* — retained
    left_button: id,  // UIBarButtonItem* — retained
    right_button: id, // UIBarButtonItem* — retained
    left_items: id,   // NSArray* — retained
    right_items: id,  // NSArray* — retained
    hides_back_button: bool,
    left_items_supplemented: bool,
}
impl HostObject for UINavigationItemHostObject {}

/// Item setters invalidate attached bars so late Done/title changes appear.
fn invalidate_item_bars(env: &mut crate::Environment, item: id) {
    let views = env.framework_state.uikit.ui_view.views.clone();
    let class: id = msg_class![env; UINavigationBar class];
    for view in views {
        if msg![env; view isKindOfClass:class] {
            let items = env.objc.borrow::<UINavigationBarHostObject>(view).items;
            if msg![env; items containsObject:item] {
                () = msg![env; view setNeedsLayout];
            }
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UINavigationBar: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let items = msg_class![env; NSMutableArray new];
    let host_object = Box::new(UINavigationBarHostObject {
        superclass: Default::default(),
        delegate: nil,
        bar_style: UIBarStyleDefault,
        translucent: true,
        tint_color: nil,
        bar_tint_color: nil,
        title_text_attributes: nil,
        items,
        shadow_image: nil,
        background_image: nil,
        title_label: nil,
        prompt_label: nil,
        back_view: nil,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)init {
    // Run UIView's initializer so the backing layer and view state are set up.
    msg_super![env; this init]
}

- (id)initWithFrame:(CGRect)frame {
    msg_super![env; this initWithFrame:frame]
}

- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let key = get_static_str(env, "UIBarStyle");
    let style: NSInteger = msg![env; coder decodeIntForKey:key];
    () = msg![env; this setBarStyle:style];
    let key = get_static_str(env, "UITranslucent");
    let has: bool = msg![env; coder containsValueForKey:key];
    if has {
        let value: bool = msg![env; coder decodeBoolForKey:key];
        () = msg![env; this setTranslucent:value];
    }
    let key = get_static_str(env, "UITintColor");
    let tint: id = msg![env; coder decodeObjectForKey:key];
    if tint != nil { () = msg![env; this setTintColor:tint]; }
    let key = get_static_str(env, "UIItems");
    let items: id = msg![env; coder decodeObjectForKey:key];
    if items != nil { () = msg![env; this setItems:items]; }
    () = msg![env; this layoutSubviews];
    this
}

- (())dealloc {
    let host = env.objc.borrow::<UINavigationBarHostObject>(this);
    let (tint_color, bar_tint_color, title_attrs, items, shadow, bg) = (
        host.tint_color,
        host.bar_tint_color,
        host.title_text_attributes,
        host.items,
        host.shadow_image,
        host.background_image,
    );
    release(env, tint_color);
    release(env, bar_tint_color);
    release(env, title_attrs);
    release(env, items);
    release(env, shadow);
    release(env, bg);
    let host = env.objc.borrow::<UINavigationBarHostObject>(this);
    let extra = [host.title_label, host.prompt_label, host.back_view];
    for object in extra { release(env, object); }
    // delegate is weak — no release; UIView tears down retained subviews/layer.
    msg_super![env; this dealloc]
}

// MARK: - Delegate

- (id)delegate {
    env.objc.borrow::<UINavigationBarHostObject>(this).delegate
}

- (())setDelegate:(id)delegate {
    // Weak reference — no retain.
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).delegate = delegate;
}

// MARK: - Style

- (UIBarStyle)barStyle {
    env.objc.borrow::<UINavigationBarHostObject>(this).bar_style
}

- (())setBarStyle:(UIBarStyle)style {
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).bar_style = style;
}

- (bool)isTranslucent {
    env.objc.borrow::<UINavigationBarHostObject>(this).translucent
}

- (())setTranslucent:(bool)value {
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).translucent = value;
}

// MARK: - Colors

- (id)tintColor {
    env.objc.borrow::<UINavigationBarHostObject>(this).tint_color
}

- (())setTintColor:(id)color {
    let old = env.objc.borrow::<UINavigationBarHostObject>(this).tint_color;
    release(env, old);
    retain(env, color);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).tint_color = color;
}

- (id)barTintColor {
    env.objc.borrow::<UINavigationBarHostObject>(this).bar_tint_color
}

- (())setBarTintColor:(id)color {
    let old = env.objc.borrow::<UINavigationBarHostObject>(this).bar_tint_color;
    release(env, old);
    retain(env, color);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).bar_tint_color = color;
}

// MARK: - Title attributes

- (id)titleTextAttributes {
    env.objc.borrow::<UINavigationBarHostObject>(this).title_text_attributes
}

- (())setTitleTextAttributes:(id)attrs {
    let old = env.objc.borrow::<UINavigationBarHostObject>(this).title_text_attributes;
    release(env, old);
    retain(env, attrs);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).title_text_attributes = attrs;
}

// MARK: - Background / shadow images

- (id)shadowImage {
    env.objc.borrow::<UINavigationBarHostObject>(this).shadow_image
}

- (())setShadowImage:(id)image {
    let old = env.objc.borrow::<UINavigationBarHostObject>(this).shadow_image;
    release(env, old);
    retain(env, image);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).shadow_image = image;
}

- (id)backgroundImageForBarMetrics:(NSInteger)_metrics {
    env.objc.borrow::<UINavigationBarHostObject>(this).background_image
}

- (())setBackgroundImage:(id)image forBarMetrics:(NSInteger)_metrics {
    let old = env.objc.borrow::<UINavigationBarHostObject>(this).background_image;
    release(env, old);
    retain(env, image);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).background_image = image;
}

- (())setBackgroundImage:(id)image
         forBarPosition:(NSInteger)_position
             barMetrics:(NSInteger)_metrics {
    () = msg![env; this setBackgroundImage:image forBarMetrics:_metrics];
}

// MARK: - Items stack

- (id)items {
    env.objc.borrow::<UINavigationBarHostObject>(this).items
}

- (())setItems:(id)items { // NSArray*
    let old_items = env.objc.borrow::<UINavigationBarHostObject>(this).items;
    let new_items: id = msg_class![env; NSMutableArray alloc];
    let new_items: id = msg![env; new_items initWithArray:items];
    release(env, old_items);
    env.objc.borrow_mut::<UINavigationBarHostObject>(this).items = new_items;
    () = msg![env; this setNeedsLayout];
}

- (())setItems:(id)items animated:(bool)_animated {
    () = msg![env; this setItems:items];
}

- (id)topItem {
    let items = env.objc.borrow::<UINavigationBarHostObject>(this).items;
    let count: u32 = msg![env; items count];
    if count == 0 { return nil; }
    msg![env; items objectAtIndex:(count - 1)]
}

- (id)backItem {
    let items = env.objc.borrow::<UINavigationBarHostObject>(this).items;
    let count: u32 = msg![env; items count];
    if count < 2 { return nil; }
    msg![env; items objectAtIndex:(count - 2)]
}

- (())pushNavigationItem:(id)item animated:(bool)_animated {
    let delegate = env.objc.borrow::<UINavigationBarHostObject>(this).delegate;
    // Ask delegate if we should push.
    if delegate != nil {
        let sel = env.objc.register_host_selector(
            "navigationBar:shouldPushItem:".to_string(), &mut env.mem,
        );
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            let should: bool = msg![env; delegate navigationBar:this shouldPushItem:item];
            if !should { return; }
        }
    }
    let items = env.objc.borrow::<UINavigationBarHostObject>(this).items;
    () = msg![env; items addObject:item];

    if delegate != nil {
        let sel = env.objc.register_host_selector(
            "navigationBar:didPushItem:".to_string(), &mut env.mem,
        );
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            () = msg![env; delegate navigationBar:this didPushItem:item];
        }
    }
}

- (id)popNavigationItemAnimated:(bool)_animated {
    let items = env.objc.borrow::<UINavigationBarHostObject>(this).items;
    let count: u32 = msg![env; items count];
    if count == 0 { return nil; }

    let top: id = msg![env; items objectAtIndex:(count - 1)];

    let delegate = env.objc.borrow::<UINavigationBarHostObject>(this).delegate;
    if delegate != nil {
        let sel = env.objc.register_host_selector(
            "navigationBar:shouldPopItem:".to_string(), &mut env.mem,
        );
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            let should: bool = msg![env; delegate navigationBar:this shouldPopItem:top];
            if !should { return nil; }
        }
    }

    retain(env, top);
    () = msg![env; items removeLastObject];
    () = msg![env; this setNeedsLayout];

    if delegate != nil {
        let sel = env.objc.register_host_selector(
            "navigationBar:didPopItem:".to_string(), &mut env.mem,
        );
        let responds: bool = msg![env; delegate respondsToSelector:sel];
        if responds {
            () = msg![env; delegate navigationBar:this didPopItem:top];
        }
    }
    crate::objc::autorelease(env, top)
}

// Navigation items supply the bar's visible title and controls, rather than
// being metadata-only. Use the existing UIBarButtonItem control renderer.
// https://developer.apple.com/library/archive/documentation/WindowsViews/Conceptual/ViewControllerCatalog/Chapters/NavigationControllers.html
- (())layoutSubviews {
    () = msg_super![env; this layoutSubviews];
    let item: id = msg![env; this topItem];
    let bounds: CGRect = msg![env; this bounds];
    if item == nil || bounds.size.width <= 0.0 { return; }
    let (mut title, mut prompt_label, mut back) = {
        let h = env.objc.borrow::<UINavigationBarHostObject>(this);
        (h.title_label, h.prompt_label, h.back_view)
    };
    if title == nil {
        title = msg_class![env; UILabel new];
        prompt_label = msg_class![env; UILabel new];
        back = msg_class![env; UIButton buttonWithType:(1i32)];
        retain(env, back);
        let selector = env.objc.register_host_selector("_touchHLE_navigationBack:".into(), &mut env.mem);
        () = msg![env; back addTarget:this action:selector forControlEvents:(64u32)];
        let h = env.objc.borrow_mut::<UINavigationBarHostObject>(this);
        h.title_label = title; h.prompt_label = prompt_label; h.back_view = back;
        let white: id = msg_class![env; UIColor whiteColor];
        let clear: id = msg_class![env; UIColor clearColor];
        let font: id = msg_class![env; UIFont boldSystemFontOfSize:(17.0f32)];
        for label in [title, prompt_label] {
            () = msg![env; label setBackgroundColor:clear];
            () = msg![env; label setTextColor:white];
            () = msg![env; label setTextAlignment:(1i32)];
            () = msg![env; label setFont:font];
        }
    }
    let color: id = if let Some(color) = {
        let h = env.objc.borrow::<UINavigationBarHostObject>(this);
        (h.bar_tint_color != nil).then_some(h.bar_tint_color)
    } { color } else {
        let style = env.objc.borrow::<UINavigationBarHostObject>(this).bar_style;
        if style == 0 { msg_class![env; UIColor colorWithRed:(0.35f32) green:(0.45f32) blue:(0.58f32) alpha:(1.0f32)] }
        else { msg_class![env; UIColor colorWithWhite:(0.12f32) alpha:(1.0f32)] }
    };
    () = msg![env; this setBackgroundColor:color];
    let prompt: id = msg![env; item prompt];
    let prompt_height = if prompt == nil { 0.0 } else { 30.0 };
    let row_height = (bounds.size.height - prompt_height).max(0.0);
    let custom_title: id = msg![env; item titleView];
    let left: id = msg![env; item leftBarButtonItem];
    let right: id = msg![env; item rightBarButtonItem];
    let back_item: id = msg![env; this backItem];
    let hide_back: bool = msg![env; item hidesBackButton];
    let left = if left != nil { left } else if back_item != nil && !hide_back {
        let back_title: id = msg![env; back_item title];
        let back_title = if back_title == nil { crate::frameworks::foundation::ns_string::get_static_str(env, "Back") } else { back_title };
        () = msg![env; back setTitle:back_title forState:(0u32)]; back
    } else { nil };
    let mut visible = vec![if custom_title == nil { title } else { custom_title }];
    if left != nil { visible.push(left); }
    if right != nil { visible.push(right); }
    if prompt != nil { visible.push(prompt_label); }
    let old: id = msg![env; this subviews];
    let count: u32 = msg![env; old count];
    for i in 0..count {
        let child: id = msg![env; old objectAtIndex:i];
        if !visible.contains(&child) { () = msg![env; child removeFromSuperview]; }
    }
    let left_width = if left == nil { 0.0 } else { 76.0 };
    let right_width = if right == nil { 0.0 } else { 76.0 };
    for (view, x, width) in [(left, 5.0, left_width), (right, bounds.size.width - right_width - 5.0, right_width)] {
        if view != nil {
            () = msg![env; view setFrame:(CGRect { origin: CGPoint { x, y: prompt_height }, size: CGSize { width, height: row_height } })];
            () = msg![env; view layoutSubviews];
        }
    }
    let title_text: id = msg![env; item title];
    () = msg![env; title setText:title_text];
    let center = if custom_title == nil { title } else { custom_title };
    let margin = left_width.max(right_width) + 5.0;
    () = msg![env; center setFrame:(CGRect { origin: CGPoint { x: margin, y: prompt_height }, size: CGSize { width: (bounds.size.width - margin * 2.0).max(0.0), height: row_height } })];
    () = msg![env; prompt_label setText:prompt];
    () = msg![env; prompt_label setFrame:(CGRect { origin: CGPoint::default(), size: CGSize { width: bounds.size.width, height: prompt_height } })];
    for view in visible {
        let parent: id = msg![env; view superview];
        if parent != this { () = msg![env; this addSubview:view]; }
    }
}
- (())_touchHLE_navigationBack:(id)_sender {
    let _: id = msg![env; this popNavigationItemAnimated:true];
}

// MARK: - Size

- (CGSize)sizeThatFits:(CGSize)_size {
    // Standard navigation bar height.
    CGSize { width: 320.0, height: 44.0 }
}

@end

@implementation UINavigationItem: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UINavigationItemHostObject {
        title: nil,
        title_view: nil,
        prompt: nil,
        back_button: nil,
        left_button: nil,
        right_button: nil,
        left_items: nil,
        right_items: nil,
        hides_back_button: false,
        left_items_supplemented: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithTitle:(id)title {
    retain(env, title);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).title = title;
    invalidate_item_bars(env, this);
    this
}

- (id)init {
    this
}

// --- ДОБАВЛЕНО: Поддержка загрузки из NIB-файла ---
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this init];
    // Archived navigation metadata must supply the same title and controls
    // as programmatic items. The bar's back-reference is weak, not owned.
    // https://developer.apple.com/documentation/uikit/uinavigationitem
    for (key, setter) in [
        ("UITitle", "setTitle:"), ("UIPrompt", "setPrompt:"),
        ("UITitleView", "setTitleView:"),
        ("UIBackBarButtonItem", "setBackBarButtonItem:"),
        ("UILeftBarButtonItem", "setLeftBarButtonItem:"),
        ("UIRightBarButtonItem", "setRightBarButtonItem:"),
    ] {
        let key = get_static_str(env, key);
        let value: id = msg![env; coder decodeObjectForKey:key];
        if value != nil {
            let selector = env.objc.lookup_selector(setter).unwrap();
            let _: () = crate::objc::msg_send_no_type_checking(env, (this, selector, value));
        }
    }
    let key = get_static_str(env, "UIHidesBackButton");
    let hidden: bool = msg![env; coder decodeBoolForKey:key];
    () = msg![env; this setHidesBackButton:hidden];
    this
}
// --------------------------------------------------

- (())dealloc {
    let host = env.objc.borrow::<UINavigationItemHostObject>(this);
    let (title, title_view, prompt, back, left, right, left_items, right_items) = (
        host.title, host.title_view, host.prompt,
        host.back_button, host.left_button, host.right_button,
        host.left_items, host.right_items,
    );
    release(env, title);
    release(env, title_view);
    release(env, prompt);
    release(env, back);
    release(env, left);
    release(env, right);
    release(env, left_items);
    release(env, right_items);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)title {
    env.objc.borrow::<UINavigationItemHostObject>(this).title
}

- (())setTitle:(id)title {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).title;
    release(env, old);
    retain(env, title);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).title = title;
    invalidate_item_bars(env, this);
}

- (id)titleView {
    env.objc.borrow::<UINavigationItemHostObject>(this).title_view
}

- (())setTitleView:(id)view {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).title_view;
    release(env, old);
    retain(env, view);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).title_view = view;
    invalidate_item_bars(env, this);
}

- (id)prompt {
    env.objc.borrow::<UINavigationItemHostObject>(this).prompt
}

- (())setPrompt:(id)prompt {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).prompt;
    release(env, old);
    retain(env, prompt);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).prompt = prompt;
    invalidate_item_bars(env, this);
}

- (id)backBarButtonItem {
    env.objc.borrow::<UINavigationItemHostObject>(this).back_button
}

- (())setBackBarButtonItem:(id)item {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).back_button;
    release(env, old);
    retain(env, item);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).back_button = item;
    invalidate_item_bars(env, this);
}

- (id)leftBarButtonItem {
    env.objc.borrow::<UINavigationItemHostObject>(this).left_button
}

- (())setLeftBarButtonItem:(id)item {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).left_button;
    release(env, old);
    retain(env, item);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).left_button = item;
    invalidate_item_bars(env, this);
}

- (())setLeftBarButtonItem:(id)item animated:(bool)_animated {
    () = msg![env; this setLeftBarButtonItem:item];
}

- (id)rightBarButtonItem {
    env.objc.borrow::<UINavigationItemHostObject>(this).right_button
}

- (())setRightBarButtonItem:(id)item {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).right_button;
    release(env, old);
    retain(env, item);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).right_button = item;
    invalidate_item_bars(env, this);
}

- (())setRightBarButtonItem:(id)item animated:(bool)_animated {
    () = msg![env; this setRightBarButtonItem:item];
}

- (id)leftBarButtonItems {
    env.objc.borrow::<UINavigationItemHostObject>(this).left_items
}

- (())setLeftBarButtonItems:(id)items {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).left_items;
    release(env, old);
    retain(env, items);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).left_items = items;
    // Sync single-item accessor to first element.
    if items != nil {
        let count: u32 = msg![env; items count];
        let first: id = if count > 0 { msg![env; items objectAtIndex:0u32] } else { nil };
        () = msg![env; this setLeftBarButtonItem:first];
    }
}

- (id)rightBarButtonItems {
    env.objc.borrow::<UINavigationItemHostObject>(this).right_items
}

- (())setRightBarButtonItems:(id)items {
    let old = env.objc.borrow::<UINavigationItemHostObject>(this).right_items;
    release(env, old);
    retain(env, items);
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).right_items = items;
    if items != nil {
        let count: u32 = msg![env; items count];
        let first: id = if count > 0 { msg![env; items objectAtIndex:0u32] } else { nil };
        () = msg![env; this setRightBarButtonItem:first];
    }
}

- (bool)hidesBackButton {
    env.objc.borrow::<UINavigationItemHostObject>(this).hides_back_button
}

- (())setHidesBackButton:(bool)value {
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).hides_back_button = value;
    invalidate_item_bars(env, this);
}

- (())setHidesBackButton:(bool)value animated:(bool)_animated {
    () = msg![env; this setHidesBackButton:value];
}

- (bool)leftItemsSupplementBackButton {
    env.objc.borrow::<UINavigationItemHostObject>(this).left_items_supplemented
}

- (())setLeftItemsSupplementBackButton:(bool)value {
    env.objc.borrow_mut::<UINavigationItemHostObject>(this).left_items_supplemented = value;
}

@end

};

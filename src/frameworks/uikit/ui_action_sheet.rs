/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UIActionSheet`.

use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::NSUInteger;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, ClassExports, NSZonePtr,
};
use crate::Environment;

#[derive(Default)]
struct UIActionSheetHostObject {
    superclass: super::ui_view::UIViewHostObject,
    delegate: id,
    title: id,
    /// NSMutableArray* of NSString* button titles
    button_titles: id,
    cancel_button_index: i32,
    destructive_button_index: i32,
    /// Tag for app use
    tag: i32,
    visible: bool,
    action_sheet_style: i32,
}
impl_HostObject_with_superclass!(UIActionSheetHostObject);

fn notify(env: &mut Environment, sheet: id, name: &str, index: Option<i32>) {
    let delegate = env.objc.borrow::<UIActionSheetHostObject>(sheet).delegate;
    let selector = env.objc.register_host_selector(name.into(), &mut env.mem);
    if delegate != nil && msg![env; delegate respondsToSelector:selector] {
        if let Some(index) = index {
            let _: () = crate::objc::msg_send(env, (delegate, selector, sheet, index));
        } else {
            let _: () = crate::objc::msg_send(env, (delegate, selector, sheet));
        }
    }
}

fn present(env: &mut Environment, sheet: id, view: id) {
    if env.objc.borrow::<UIActionSheetHostObject>(sheet).visible {
        return;
    }
    let app: id = msg_class![env; UIApplication sharedApplication];
    let window: id = msg![env; app keyWindow];
    let parent = if window != nil { window } else { view };
    if parent == nil {
        return;
    }
    let bounds: CGRect = msg![env; parent bounds];
    let _: () = msg![env; sheet setFrame:bounds];
    let dim: id = msg_class![env; UIColor colorWithWhite:0.0f32 alpha:0.55f32];
    let _: () = msg![env; sheet setBackgroundColor:dim];
    let (titles, title, cancel, destructive) = {
        let h = env.objc.borrow::<UIActionSheetHostObject>(sheet);
        (
            h.button_titles,
            h.title,
            h.cancel_button_index,
            h.destructive_button_index,
        )
    };
    let count: NSUInteger = msg![env; titles count];
    let row_height = 48.0;
    let title_height = if title == nil { 0.0 } else { 40.0 };
    let top = (bounds.size.height - count as f32 * row_height - title_height - 12.0).max(0.0);
    if title != nil {
        let label: id = msg_class![env; UILabel alloc];
        let frame = CGRect {
            origin: CGPoint { x: 8.0, y: top },
            size: CGSize {
                width: bounds.size.width - 16.0,
                height: title_height,
            },
        };
        let label: id = msg![env; label initWithFrame:frame];
        let white: id = msg_class![env; UIColor whiteColor];
        let _: () = msg![env; label setText:title];
        let _: () = msg![env; label setTextColor:white];
        let clear: id = msg_class![env; UIColor clearColor];
        let _: () = msg![env; label setBackgroundColor:clear];
        let _: () = msg![env; label setTextAlignment:1i32];
        let _: () = msg![env; label setNumberOfLines:2i32];
        let _: () = msg![env; sheet addSubview:label];
        release(env, label);
    }
    let action = env
        .objc
        .register_host_selector("_touchHLE_buttonTapped:".into(), &mut env.mem);
    for i in 0..count {
        let button: id = msg_class![env; UIButton buttonWithType:1i32];
        let frame = CGRect {
            origin: CGPoint {
                x: 8.0,
                y: top + title_height + i as f32 * row_height,
            },
            size: CGSize {
                width: bounds.size.width - 16.0,
                height: row_height - 4.0,
            },
        };
        let text: id = msg![env; titles objectAtIndex:i];
        let _: () = msg![env; button setFrame:frame];
        let _: () = msg![env; button setTitle:text forState:0u32];
        let _: () = msg![env; button setTag:(i as i32)];
        if i as i32 == destructive {
            let red: id = msg_class![env; UIColor redColor];
            let _: () = msg![env; button setTitleColor:red forState:0u32];
        } else if i as i32 == cancel {
            let black: id = msg_class![env; UIColor blackColor];
            let _: () = msg![env; button setTitleColor:black forState:0u32];
        }
        let _: () = msg![env; button addTarget:sheet action:action forControlEvents:64u32];
        let _: () = msg![env; sheet addSubview:button];
    }
    env.objc
        .borrow_mut::<UIActionSheetHostObject>(sheet)
        .visible = true;
    retain(env, sheet);
    notify(env, sheet, "willPresentActionSheet:", None);
    let _: () = msg![env; parent addSubview:sheet];
    notify(env, sheet, "didPresentActionSheet:", None);
    release(env, sheet);
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIActionSheet: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(UIActionSheetHostObject {
        superclass: Default::default(),
        delegate: nil,
        title: nil,
        button_titles: nil,
        cancel_button_index: -1,
        destructive_button_index: -1,
        tag: 0,
        visible: false,
        action_sheet_style: -1,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)init {
    let this: id = msg_super![env; this init];
    let buttons: id = msg_class![env; NSMutableArray new];
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).button_titles = buttons;
    this
}

// MARK: - Designated initializer

- (id)initWithTitle:(id)title               // NSString*
           delegate:(id)delegate
  cancelButtonTitle:(id)cancel_title        // NSString*
destructiveButtonTitle:(id)destructive_title // NSString*
  otherButtonTitles:(id)other_titles, ...args {
    let this: id = msg![env; this init];
    let buttons = env.objc.borrow::<UIActionSheetHostObject>(this).button_titles;

    // Set title.
    retain(env, title);
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).title = title;

    // Set delegate.
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).delegate = delegate;

    // Add destructive button first (index 0 when present), matching UIKit.
    if destructive_title != nil {
        let idx: NSUInteger = msg![env; buttons count];
        let _: () = msg![env; buttons addObject:destructive_title];
        env.objc.borrow_mut::<UIActionSheetHostObject>(this).destructive_button_index =
            idx as i32;
    }

    if other_titles != nil {
        let _: () = msg![env; buttons addObject:other_titles];
        let mut args = args.start();
        loop {
            let title: id = args.next(env);
            if title == nil { break; }
            let _: () = msg![env; buttons addObject:title];
        }
    }

    // Add cancel button last, matching UIKit behaviour.
    if cancel_title != nil {
        let idx: NSUInteger = msg![env; buttons count];
        let _: () = msg![env; buttons addObject:cancel_title];
        env.objc.borrow_mut::<UIActionSheetHostObject>(this).cancel_button_index =
            idx as i32;
    }

    this
}

- (())dealloc {
    let host = env.objc.borrow::<UIActionSheetHostObject>(this);
    let (title, button_titles) = (host.title, host.button_titles);
    release(env, title);
    release(env, button_titles);
    msg_super![env; this dealloc]
}

// MARK: - Delegate

- (id)delegate {
    env.objc.borrow::<UIActionSheetHostObject>(this).delegate
}

- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).delegate = delegate;
}

// MARK: - Title

- (id)title {
    env.objc.borrow::<UIActionSheetHostObject>(this).title
}

- (())setTitle:(id)title {
    let old = env.objc.borrow::<UIActionSheetHostObject>(this).title;
    release(env, old);
    retain(env, title);
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).title = title;
}

// MARK: - Tag

- (i32)tag {
    env.objc.borrow::<UIActionSheetHostObject>(this).tag
}

- (())setTag:(i32)tag {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).tag = tag;
}

// MARK: - Buttons

- (NSUInteger)addButtonWithTitle:(id)title { // NSString* -> NSInteger (button index)
    let buttons = env.objc.borrow::<UIActionSheetHostObject>(this).button_titles;
    let idx: NSUInteger = msg![env; buttons count];
    let _: () = msg![env; buttons addObject:title];
    idx
}

- (NSUInteger)numberOfButtons {
    let buttons = env.objc.borrow::<UIActionSheetHostObject>(this).button_titles;
    msg![env; buttons count]
}

- (i32)actionSheetStyle {
    env.objc.borrow::<UIActionSheetHostObject>(this).action_sheet_style
}

- (())setActionSheetStyle:(i32)style {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).action_sheet_style = style;
}

- (id)buttonTitleAtIndex:(NSUInteger)index { // NSString*
    let buttons = env.objc.borrow::<UIActionSheetHostObject>(this).button_titles;
    let count: NSUInteger = msg![env; buttons count];
    if index >= count {
        return nil;
    }
    msg![env; buttons objectAtIndex:index]
}

// MARK: - Special button indices

- (i32)cancelButtonIndex {
    env.objc.borrow::<UIActionSheetHostObject>(this).cancel_button_index
}

- (())setCancelButtonIndex:(i32)index {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).cancel_button_index = index;
}

- (i32)destructiveButtonIndex {
    env.objc.borrow::<UIActionSheetHostObject>(this).destructive_button_index
}

- (())setDestructiveButtonIndex:(i32)index {
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).destructive_button_index = index;
}

- (i32)firstOtherButtonIndex {
    // First button that is neither cancel nor destructive.
    let host = env.objc.borrow::<UIActionSheetHostObject>(this);
    let buttons = host.button_titles;
    let cancel = host.cancel_button_index;
    let destructive = host.destructive_button_index;
    let count: NSUInteger = msg![env; buttons count];
    for i in 0..count {
        let i_signed = i as i32;
        if i_signed != cancel && i_signed != destructive {
            return i_signed;
        }
    }
    -1
}

// MARK: - Visibility

- (bool)isVisible {
    env.objc.borrow::<UIActionSheetHostObject>(this).visible
}

// Presentation is asynchronous. UIKit calls the delegate only on selection.
// https://developer.apple.com/documentation/uikit/uiactionsheetdelegate
- (())showInView:(id)view { present(env, this, view); }
- (())showFromToolbar:(id)view { present(env, this, view); }
- (())showFromTabBar:(id)view { present(env, this, view); }
- (())showFromBarButtonItem:(id)_item animated:(bool)_animated { present(env, this, nil); }
- (())showFromRect:(CGRect)_rect inView:(id)view animated:(bool)_animated { present(env, this, view); }

- (())_touchHLE_buttonTapped:(id)sender {
    let index: i32 = msg![env; sender tag];
    let _: () = msg![env; this dismissWithClickedButtonIndex:index animated:true];
}

- (())dismissWithClickedButtonIndex:(i32)index animated:(bool)_animated {
    if !env.objc.borrow::<UIActionSheetHostObject>(this).visible { return; }
    retain(env, this);
    env.objc.borrow_mut::<UIActionSheetHostObject>(this).visible = false;
    notify(env, this, "actionSheet:clickedButtonAtIndex:", Some(index));
    notify(env, this, "actionSheet:willDismissWithButtonIndex:", Some(index));
    let _: () = msg![env; this removeFromSuperview];
    let subviews: id = msg![env; this subviews];
    let count: NSUInteger = msg![env; subviews count];
    for i in 0..count {
        let view: id = msg![env; subviews objectAtIndex:i];
        let _: () = msg![env; view removeFromSuperview];
    }
    notify(env, this, "actionSheet:didDismissWithButtonIndex:", Some(index));
    release(env, this);
}

@end

};

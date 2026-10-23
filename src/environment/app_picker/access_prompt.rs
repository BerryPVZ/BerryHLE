/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

use super::{
    make_alert_button, new_label, new_view, rect, ui_color, AppPickerDelegateHostObject,
};
use crate::frameworks::core_graphics::{CGFloat, CGRect};
use crate::frameworks::foundation::ns_run_loop::run_run_loop_single_iteration;
use crate::frameworks::foundation::{ns_string, NSInteger};
use crate::frameworks::uikit::ui_font::UITextAlignmentCenter;
use crate::frameworks::uikit::ui_view::ui_control::ui_button::UIButtonTypeCustom;
use crate::frameworks::uikit::ui_view::ui_control::UIControlEventTouchUpInside;
use crate::objc::{id, msg, msg_class, nil, release};
use crate::Environment;
use sha2::{Digest, Sha256};

pub(super) const FIELD_TAG: NSInteger = -7319;

const SALT: [u8; 16] = [
    0x17, 0x29, 0x44, 0xf0, 0x20, 0xdb, 0xd1, 0xe0, 0xc7, 0xa4, 0xfa, 0x65, 0xa5, 0x74, 0xc4,
    0x8d,
];
const EXPECTED: [u8; 32] = [
    0xe2, 0xfa, 0xf8, 0x8a, 0x88, 0xa1, 0x8c, 0x39, 0x80, 0xdb, 0xc7, 0xc1, 0xbf, 0x83, 0x17,
    0xf8, 0x81, 0x4b, 0x39, 0xe2, 0xde, 0x90, 0x95, 0x4f, 0xf3, 0xca, 0x40, 0x29, 0xce, 0xf7,
    0x69, 0x5d,
];

fn is_expected(value: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(SALT);
    hasher.update(value.as_bytes());
    let actual = hasher.finalize();
    let mut difference = 0u8;
    for (&actual, &expected) in actual.iter().zip(EXPECTED.iter()) {
        difference |= actual ^ expected;
    }
    difference == 0
}

/// Shows a deliberately unlabelled modal input. No result is displayed for a
/// rejected value, and no state is retained after the dialog closes.
pub(super) fn run_access_prompt(
    env: &mut Environment,
    run_loop: id,
    delegate: id,
    parent: id,
    app_frame: CGRect,
) -> bool {
    let width = app_frame.size.width;
    let status_offset = app_frame.origin.y;
    let full_height = app_frame.size.height + status_offset;
    let dim = ui_color(env, 0.0, 0.0, 0.0, 0.45);
    let root = new_view(
        env,
        rect(0.0, -status_offset, width, full_height),
        dim,
    );
    () = msg![env; parent addSubview:root];

    const PANEL_WIDTH: CGFloat = 270.0;
    const PANEL_HEIGHT: CGFloat = 126.0;
    let panel_width = PANEL_WIDTH.min(width - 24.0);
    let panel_x = (width - panel_width) / 2.0;
    let panel_y = ((full_height - PANEL_HEIGHT) * 0.30).max(status_offset + 8.0);
    let white: id = msg_class![env; UIColor whiteColor];
    let panel = new_view(
        env,
        rect(panel_x, panel_y, panel_width, PANEL_HEIGHT),
        white,
    );
    let panel_layer: id = msg![env; panel layer];
    () = msg![env; panel_layer setCornerRadius:(12.0 as CGFloat)];
    let border = ui_color(env, 0.70, 0.72, 0.75, 1.0);
    let border_cg: id = msg![env; border CGColor];
    () = msg![env; panel_layer setBorderColor:border_cg];
    () = msg![env; panel_layer setBorderWidth:(1.0 as CGFloat)];
    () = msg![env; root addSubview:panel];

    let field_frame = rect(16.0, 18.0, panel_width - 32.0, 38.0);
    let field: id = msg_class![env; UITextField alloc];
    let field: id = msg![env; field initWithFrame:field_frame];
    () = msg![env; field setTag:FIELD_TAG];
    () = msg![env; field setDelegate:delegate];
    () = msg![env; field setBackgroundColor:white];
    let clear: id = msg_class![env; UIColor clearColor];
    let black: id = msg_class![env; UIColor blackColor];
    // The real value is never drawn. A separate label renders only bullets.
    () = msg![env; field setTextColor:clear];
    () = msg![env; field setSecureTextEntry:true];
    () = msg![env; field setKeyboardType:(4 as NSInteger)];
    () = msg![env; field setReturnKeyType:(9 as NSInteger)];
    () = msg![env; field setAutocapitalizationType:(0 as NSInteger)];
    () = msg![env; field setAutocorrectionType:(1 as NSInteger)];
    let font: id = msg_class![env; UIFont systemFontOfSize:(22.0 as CGFloat)];
    () = msg![env; field setFont:font];
    let field_layer: id = msg![env; field layer];
    let field_border = ui_color(env, 0.62, 0.65, 0.70, 1.0);
    let field_border_cg: id = msg![env; field_border CGColor];
    () = msg![env; field_layer setBorderColor:field_border_cg];
    () = msg![env; field_layer setBorderWidth:(1.0 as CGFloat)];
    () = msg![env; field_layer setCornerRadius:(6.0 as CGFloat)];
    () = msg![env; panel addSubview:field];

    let mask = new_label(env, field_frame, "", 22.0, false, black);
    () = msg![env; mask setTextAlignment:UITextAlignmentCenter];
    () = msg![env; mask setUserInteractionEnabled:false];
    () = msg![env; panel addSubview:mask];

    let separator = new_view(
        env,
        rect(0.0, 70.0, panel_width, 1.0),
        ui_color(env, 0.82, 0.83, 0.85, 1.0),
    );
    () = msg![env; panel addSubview:separator];
    let vertical_separator = new_view(
        env,
        rect(panel_width / 2.0, 70.0, 1.0, 56.0),
        ui_color(env, 0.82, 0.83, 0.85, 1.0),
    );
    () = msg![env; panel addSubview:vertical_separator];
    let blue = ui_color(env, 0.0, 0.478, 1.0, 1.0);
    let cancel = make_alert_button(
        env,
        delegate,
        rect(0.0, 71.0, panel_width / 2.0, 55.0),
        "Cancel",
        blue,
        false,
        "accessPromptCancel",
    );
    () = msg![env; panel addSubview:cancel];
    let confirm = make_alert_button(
        env,
        delegate,
        rect(panel_width / 2.0, 71.0, panel_width / 2.0, 55.0),
        "OK",
        blue,
        true,
        "accessPromptConfirm",
    );
    () = msg![env; panel addSubview:confirm];

    {
        let host = env
            .objc
            .borrow_mut::<AppPickerDelegateHostObject>(delegate);
        host.access_prompt_confirm = false;
        host.access_prompt_cancel = false;
        host.access_prompt_submit = false;
    }

    let _: bool = msg![env; field becomeFirstResponder];
    let mut accepted = false;
    let mut finished = false;
    let mut masked_length = 0usize;
    while !finished {
        run_run_loop_single_iteration(env, run_loop);

        let text: id = msg![env; field text];
        let mut value = if text == nil {
            String::new()
        } else {
            ns_string::to_rust_string(env, text).into_owned()
        };
        let length = value.chars().count();
        if length != masked_length {
            masked_length = length;
            let bullets = ns_string::from_rust_string(env, "•".repeat(length.min(18)));
            () = msg![env; mask setText:bullets];
            release(env, bullets);
        }

        let (confirm, cancel, submit) = {
            let host = env
                .objc
                .borrow_mut::<AppPickerDelegateHostObject>(delegate);
            (
                std::mem::take(&mut host.access_prompt_confirm),
                std::mem::take(&mut host.access_prompt_cancel),
                std::mem::take(&mut host.access_prompt_submit),
            )
        };
        if cancel {
            finished = true;
        } else if confirm || submit {
            accepted = is_expected(&value);
            value.clear();
            let empty = ns_string::get_static_str(env, "");
            () = msg![env; field setText:empty];
            () = msg![env; mask setText:empty];
            masked_length = 0;
            if accepted {
                finished = true;
            } else {
                // Rejected input is cleared without an error or other feedback.
                let _: bool = msg![env; field becomeFirstResponder];
            }
        }
    }

    let _: bool = msg![env; field resignFirstResponder];
    let empty = ns_string::get_static_str(env, "");
    () = msg![env; field setText:empty];
    () = msg![env; mask setText:empty];
    () = msg![env; root removeFromSuperview];
    {
        let host = env
            .objc
            .borrow_mut::<AppPickerDelegateHostObject>(delegate);
        host.access_prompt_confirm = false;
        host.access_prompt_cancel = false;
        host.access_prompt_submit = false;
        host.icon_tapped = nil;
        host.icon_touch_down = nil;
        host.icon_touch_cancelled = false;
        host.swipe_left = false;
        host.swipe_right = false;
    }
    accepted
}

/// Adds the transparent status-bar target. It has no title, image, or other
/// visible affordance of its own.
pub(super) fn add_status_target(env: &mut Environment, delegate: id, bar: id, width: CGFloat) {
    let target: id = msg_class![env; UIButton buttonWithType:UIButtonTypeCustom];
    let bar_frame: CGRect = msg![env; bar frame];
    () = msg![env; target setFrame:(rect(
        width - 44.0,
        0.0,
        44.0,
        bar_frame.size.height
    ))];
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; target setBackgroundColor:clear];
    let selector = env.objc.lookup_selector("statusAccessoryTapped").unwrap();
    () = msg![env; target addTarget:delegate
                             action:selector
                   forControlEvents:UIControlEventTouchUpInside];
    () = msg![env; bar addSubview:target];
}

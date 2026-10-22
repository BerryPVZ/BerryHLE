/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Opt-in, in-game Globbert animation browser. No game assets are bundled.

use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::{from_rust_string, get_static_str, to_rust_string};
use crate::frameworks::foundation::NSInteger;
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports,
};
use crate::Environment;
use plist::Value;
use std::io::Cursor;
use std::time::{Duration, Instant};

const ROWS: usize = 10;

#[derive(Clone, Debug)]
struct Animation {
    name: String,
    last_frame: usize,
    flat: bool,
}

fn animation(name: &str, data: &[u8]) -> Option<Animation> {
    let root = Value::from_reader(Cursor::new(data)).ok()?;
    let dict = root.as_dictionary()?;
    let nested = dict.get("Frames").and_then(Value::as_dictionary);
    let frames = nested.unwrap_or(dict);
    let last_frame = frames
        .keys()
        .filter_map(|k| k.strip_prefix("Frame ")?.parse::<usize>().ok())
        .max()?;
    if last_frame == 0 || last_frame > 1_000_000 {
        return None;
    }
    Some(Animation {
        name: name.strip_suffix(".plist")?.to_owned(),
        last_frame,
        flat: nested.is_none(),
    })
}

fn is_ending(name: &str) -> bool {
    name.starts_with("DEX") || name == "TheEnd" || name == "Exit" || name.ends_with("-End")
}

#[derive(Clone, Copy)]
enum Request {
    Play(usize),
    Stop,
    Resume,
}

#[derive(Default)]
pub(super) struct State {
    root: id,
    panel: id,
    toggle: id,
    target: id,
    search: id,
    filter_button: id,
    page_label: id,
    status_label: id,
    row_buttons: Vec<id>,
    rows: usize,
    catalogue: Vec<Animation>,
    visible: Vec<usize>,
    query: String,
    endings: bool,
    page: usize,
    open: bool,
    size: (f32, f32),
    controller: id,
    opening_game: bool,
    request: Option<(Request, Instant)>,
}

fn text(env: &mut Environment, value: impl Into<String>) -> id {
    let value = from_rust_string(env, value.into());
    autorelease(env, value)
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> CGRect {
    CGRect {
        origin: CGPoint { x, y },
        size: CGSize { width, height },
    }
}

fn title(env: &mut Environment, button: id, value: impl Into<String>) {
    let value = text(env, value);
    () = msg![env; button setTitle:value forState:0u32];
}

fn label(env: &mut Environment, parent: id, frame: CGRect, size: f32) -> id {
    let label: id = msg_class![env; UILabel alloc];
    let label: id = msg![env; label initWithFrame:frame];
    let font: id = msg_class![env; UIFont systemFontOfSize:size];
    let black: id = msg_class![env; UIColor blackColor];
    () = msg![env; label setFont:font];
    () = msg![env; label setTextColor:black];
    () = msg![env; parent addSubview:label];
    release(env, label);
    label
}

fn button(
    env: &mut Environment,
    parent: id,
    target: id,
    action: &str,
    frame: CGRect,
    name: &str,
) -> id {
    let button: id = msg_class![env; UIButton buttonWithType:(1 as NSInteger)];
    () = msg![env; button setFrame:frame];
    let selector = env.objc.lookup_selector(action).unwrap();
    () = msg![env; button addTarget:target action:selector forControlEvents:0x40u32];
    title(env, button, name);
    let label: id = msg![env; button titleLabel];
    let font: id = msg_class![env; UIFont systemFontOfSize:(14.0 as CGFloat)];
    () = msg![env; label setFont:font];
    () = msg![env; parent addSubview:button];
    button
}

fn status(env: &mut Environment, value: impl Into<String>) {
    let label = env.framework_state.uikit.globbert_debug.status_label;
    let value = text(env, value);
    () = msg![env; label setText:value];
}

fn refresh(env: &mut Environment) {
    let state = &mut env.framework_state.uikit.globbert_debug;
    let query = state.query.to_lowercase();
    state.visible = state
        .catalogue
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            (!state.endings || is_ending(&a.name)) && a.name.to_lowercase().contains(&query)
        })
        .map(|(i, _)| i)
        .collect();
    let rows = state.rows.max(1);
    let pages = state.visible.len().div_ceil(rows).max(1);
    state.page = state.page.min(pages - 1);
    let page = state.page;
    let count = state.visible.len();
    let labels: Vec<_> = state
        .row_buttons
        .iter()
        .enumerate()
        .map(|(row, &button)| {
            let name = (row < rows)
                .then(|| state.visible.get(page * rows + row))
                .flatten()
                .map(|&i| state.catalogue[i].name.clone());
            (button, name)
        })
        .collect();
    let page_label = state.page_label;
    let filter = state.filter_button;
    let endings = state.endings;
    for (button, name) in labels {
        () = msg![env; button setHidden:(name.is_none())];
        if let Some(name) = name {
            title(env, button, name);
        }
    }
    title(
        env,
        filter,
        if endings {
            "Endings (tap for all animations)"
        } else {
            "All animations (tap for endings)"
        },
    );
    let value = text(env, format!("{} / {}  ({})", page + 1, pages, count));
    () = msg![env; page_label setText:value];
}

fn layout(env: &mut Environment, window: id) {
    let bounds: CGRect = msg![env; window bounds];
    let width = bounds.size.width;
    let height = bounds.size.height;
    let state = &mut env.framework_state.uikit.globbert_debug;
    let expanded_width = 360.0f32.min((width - 16.0).max(160.0));
    let rows = (((height - 250.0) / 32.0).floor() as usize).clamp(1, ROWS);
    state.rows = rows;
    state.size = (width, height);
    let open = state.open;
    let root = state.root;
    let panel = state.panel;
    let toggle = state.toggle;
    let row_buttons = state.row_buttons.clone();
    let w = if open { expanded_width } else { 150.0 };
    let h = if open {
        250.0 + rows as f32 * 32.0
    } else {
        34.0
    };
    () = msg![env; root setFrame:(rect(width - w - 8.0, 8.0, w, h))];
    () = msg![env; toggle setFrame:(rect(0.0, 0.0, w, 34.0))];
    title(
        env,
        toggle,
        if open {
            "Close Globbert debug"
        } else {
            "Globbert debug"
        },
    );
    () = msg![env; panel setHidden:(!open)];
    () = msg![env; panel setFrame:(rect(0.0, 38.0, expanded_width, h - 38.0))];
    let filter = env.framework_state.uikit.globbert_debug.filter_button;
    let search = env.framework_state.uikit.globbert_debug.search;
    () = msg![env; filter setFrame:(rect(8.0, 32.0, expanded_width - 16.0, 28.0))];
    () = msg![env; search setFrame:(rect(8.0, 64.0, expanded_width - 16.0, 30.0))];
    for (i, row) in row_buttons.iter().enumerate() {
        () = msg![env; (*row) setFrame:(rect(8.0, 100.0 + i as f32 * 32.0, expanded_width - 16.0, 30.0))];
    }
    // Footer views carry stable tags so they can be laid out after resizing.
    let footer_y = 100.0 + rows as f32 * 32.0;
    for (tag, frame) in [
        (21, rect(8.0, footer_y, 65.0, 28.0)),
        (22, rect(expanded_width - 73.0, footer_y, 65.0, 28.0)),
        (23, rect(75.0, footer_y, expanded_width - 150.0, 28.0)),
        (24, rect(8.0, footer_y + 34.0, 80.0, 28.0)),
        (
            25,
            rect(96.0, footer_y + 34.0, expanded_width - 104.0, 28.0),
        ),
        (26, rect(8.0, footer_y + 66.0, expanded_width - 16.0, 38.0)),
        (27, rect(8.0, 5.0, expanded_width - 16.0, 22.0)),
    ] {
        let view: id = msg![env; panel viewWithTag:(tag as NSInteger)];
        () = msg![env; view setFrame:frame];
    }
    refresh(env);
}

fn create(env: &mut Environment, window: id) {
    let path = env.bundle.bundle_path().to_owned();
    let files: Vec<String> = env
        .fs
        .enumerate(&path)
        .map(|i| {
            i.filter(|n| n.ends_with(".plist"))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    let mut catalogue: Vec<Animation> = files
        .iter()
        .filter_map(|name| animation(name, &env.fs.read(path.join(name)).ok()?))
        .collect();
    catalogue.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let root: id = msg_class![env; UIView new];
    let target: id = msg_class![env; _touchHLE_GlobbertDebug new];
    let panel: id = msg_class![env; UIView new];
    let background: id =
        msg_class![env; UIColor colorWithRed:0.10f32 green:0.12f32 blue:0.16f32 alpha:0.97f32];
    () = msg![env; panel setBackgroundColor:background];
    () = msg![env; root addSubview:panel];
    release(env, panel);
    let toggle = button(
        env,
        root,
        target,
        "toggle:",
        rect(0.0, 0.0, 150.0, 34.0),
        "Globbert debug",
    );
    let heading = label(env, panel, rect(8.0, 5.0, 320.0, 22.0), 16.0);
    () = msg![env; heading setTag:(27 as NSInteger)];
    let heading_text = text(env, "Choose an animation");
    () = msg![env; heading setText:heading_text];
    let filter = button(
        env,
        panel,
        target,
        "filter:",
        rect(8.0, 32.0, 320.0, 28.0),
        "All animations",
    );
    let search: id = msg_class![env; UITextField alloc];
    let search: id = msg![env; search initWithFrame:(rect(8.0, 64.0, 320.0, 30.0))];
    let placeholder = text(env, "Filter by animation name");
    let white: id = msg_class![env; UIColor whiteColor];
    () = msg![env; search setPlaceholder:placeholder];
    () = msg![env; search setBackgroundColor:white];
    () = msg![env; search setBorderStyle:(1 as NSInteger)];
    () = msg![env; panel addSubview:search];
    release(env, search);
    let mut row_buttons = Vec::new();
    for i in 0..ROWS {
        let row = button(
            env,
            panel,
            target,
            "playRow:",
            rect(8.0, 100.0 + i as f32 * 32.0, 320.0, 30.0),
            "",
        );
        () = msg![env; row setTag:(i as NSInteger)];
        row_buttons.push(row);
    }
    let previous = button(
        env,
        panel,
        target,
        "previous:",
        rect(0.0, 0.0, 0.0, 0.0),
        "Prev",
    );
    let next = button(
        env,
        panel,
        target,
        "next:",
        rect(0.0, 0.0, 0.0, 0.0),
        "Next",
    );
    let page_label = label(env, panel, rect(0.0, 0.0, 0.0, 0.0), 13.0);
    () = msg![env; page_label setTextAlignment:(1 as NSInteger)];
    let stop = button(
        env,
        panel,
        target,
        "stop:",
        rect(0.0, 0.0, 0.0, 0.0),
        "Stop",
    );
    let resume = button(
        env,
        panel,
        target,
        "resume:",
        rect(0.0, 0.0, 0.0, 0.0),
        "Return to game",
    );
    let status_label = label(env, panel, rect(0.0, 0.0, 0.0, 0.0), 12.0);
    () = msg![env; status_label setNumberOfLines:(2 as NSInteger)];
    for (view, tag) in [
        (previous, 21),
        (next, 22),
        (page_label, 23),
        (stop, 24),
        (resume, 25),
        (status_label, 26),
    ] {
        () = msg![env; view setTag:(tag as NSInteger)];
    }
    let state = &mut env.framework_state.uikit.globbert_debug;
    state.root = root;
    state.panel = panel;
    state.toggle = toggle;
    state.target = target;
    state.search = search;
    state.filter_button = filter;
    state.page_label = page_label;
    state.status_label = status_label;
    state.row_buttons = row_buttons;
    state.catalogue = catalogue;
    () = msg![env; window addSubview:root];
    layout(env, window);
    status(env, "Choose a clip; Stop interrupts playback.");
    log!(
        "Globbert debug menu enabled ({} animations)",
        env.framework_state.uikit.globbert_debug.catalogue.len()
    );
}

fn find_controller(env: &mut Environment) -> id {
    let current = env.framework_state.uikit.globbert_debug.controller;
    if current != nil {
        return current;
    }
    for view in env.framework_state.uikit.ui_view.views.clone() {
        let next: id = msg![env; view nextResponder];
        if next == nil {
            continue;
        }
        let class: id = msg![env; next class];
        if env.objc.get_class_name(class) == "GlobbertViewController" {
            retain(env, next);
            env.framework_state.uikit.globbert_debug.controller = next;
            return next;
        }
    }
    nil
}

fn stop_threads(env: &mut Environment, controller: id) -> bool {
    () = msg![env; controller setIsSquishing:false];
    () = msg![env; controller setStopAnimationThread:true];
    let a: id = msg![env; controller animationThread];
    let b: id = msg![env; controller animation2Thread];
    let c: id = msg![env; controller animation3Thread];
    let mut busy = false;
    for thread in [a, b, c] {
        if thread == nil {
            continue;
        }
        () = msg![env; thread cancel];
        let executing: bool = msg![env; thread isExecuting];
        busy |= executing;
    }
    busy
}

fn request(env: &mut Environment, request: Request) {
    env.framework_state.uikit.globbert_debug.request = Some((request, Instant::now()));
    let search = env.framework_state.uikit.globbert_debug.search;
    () = msg![env; search resignFirstResponder];
    status(env, "Stopping the previous animation...");
}

fn play(env: &mut Environment, controller: id, index: usize) {
    let Some(entry) = env
        .framework_state
        .uikit
        .globbert_debug
        .catalogue
        .get(index)
        .cloned()
    else {
        return;
    };
    let path = env
        .bundle
        .bundle_path()
        .join(format!("{}.plist", entry.name));
    let path = text(env, path.as_str());
    let original: id = msg_class![env; NSDictionary dictionaryWithContentsOfFile:path];
    if original == nil {
        status(env, "Could not load this animation.");
        return;
    }
    let dictionary = if entry.flat {
        let dictionary: id = msg_class![env; NSMutableDictionary dictionary];
        let frames = get_static_str(env, "Frames");
        () = msg![env; dictionary setObject:original forKey:frames];
        dictionary
    } else {
        original
    };
    let name = text(env, &entry.name);
    () = msg![env; controller setAnimationRef:name];
    () = msg![env; controller setAnimationPath:path];
    () = msg![env; controller setAnimationDictionary:dictionary];
    () = msg![env; controller setAnimationLastFrame:(entry.last_frame as NSInteger)];
    () = msg![env; controller setAnimationFrame:(0 as NSInteger)];
    () = msg![env; controller setStopAnimationThread:false];
    () = msg![env; controller setIsSquishing:false];
    () = msg![env; controller setHasSquished:false];
    () = msg![env; controller setHasKnockedSpeaker:false];
    () = msg![env; controller setHasSplattedSpeaker:false];
    // A debug selection can also be made from the welcome screen. Put the
    // gameplay view in front without removing the persistent debug overlay.
    let view: id = msg![env; controller view];
    let window = *env
        .framework_state
        .uikit
        .ui_view
        .ui_window
        .windows
        .last()
        .unwrap();
    () = msg![env; view setHidden:false];
    () = msg![env; window addSubview:view];
    let root = env.framework_state.uikit.globbert_debug.root;
    () = msg![env; window bringSubviewToFront:root];
    let selector = env.objc.lookup_selector("startAnimation").unwrap();
    let thread: id = msg_class![env; NSThread alloc];
    let thread: id = msg![env; thread initWithTarget:controller selector:selector object:nil];
    () = msg![env; controller setAnimationThread:thread];
    () = msg![env; thread start];
    release(env, thread);
    status(env, format!("Playing {}", entry.name));
    log!(
        "Globbert debug: playing {} ({} frames)",
        entry.name,
        entry.last_frame
    );
}

pub(super) fn tick(env: &mut Environment) {
    if !env.options.globbert_debug || env.bundle.bundle_identifier() != "com.andydev.globbert" {
        return;
    }
    let windows = env.framework_state.uikit.ui_view.ui_window.windows.clone();
    let window = windows.into_iter().rev().find(|&window| {
        let hidden: bool = msg![env; window isHidden];
        !hidden
    });
    let Some(window) = window else {
        return;
    };
    if env.framework_state.uikit.globbert_debug.root == nil {
        create(env, window);
    }
    let root = env.framework_state.uikit.globbert_debug.root;
    let parent: id = msg![env; root superview];
    if parent != window {
        () = msg![env; window addSubview:root];
    }
    let subviews: id = msg![env; window subviews];
    let last: id = msg![env; subviews lastObject];
    if last != root {
        () = msg![env; window bringSubviewToFront:root];
    }
    let bounds: CGRect = msg![env; window bounds];
    if env.framework_state.uikit.globbert_debug.size != (bounds.size.width, bounds.size.height) {
        layout(env, window);
    }
    if env.framework_state.uikit.globbert_debug.open {
        let search = env.framework_state.uikit.globbert_debug.search;
        let value: id = msg![env; search text];
        let query = if value == nil {
            String::new()
        } else {
            to_rust_string(env, value).into_owned()
        };
        if query != env.framework_state.uikit.globbert_debug.query {
            let state = &mut env.framework_state.uikit.globbert_debug;
            state.query = query;
            state.page = 0;
            refresh(env);
        }
    }
    let Some((requested, since)) = env.framework_state.uikit.globbert_debug.request else {
        return;
    };
    let controller = find_controller(env);
    if controller == nil {
        if matches!(requested, Request::Stop) {
            env.framework_state.uikit.globbert_debug.request = None;
            status(env, "Stopped.");
            return;
        }
        if !env.framework_state.uikit.globbert_debug.opening_game {
            for view in env.framework_state.uikit.ui_view.views.clone() {
                let welcome: id = msg![env; view nextResponder];
                if welcome == nil {
                    continue;
                }
                let class: id = msg![env; welcome class];
                if env.objc.get_class_name(class) == "WelcomeViewController" {
                    env.framework_state.uikit.globbert_debug.opening_game = true;
                    () = msg![env; welcome openApp];
                    break;
                }
            }
        }
        status(env, "Opening Globbert...");
        return;
    }
    if stop_threads(env, controller) {
        if since.elapsed() > Duration::from_secs(5) {
            status(env, "Waiting for the current animation to stop...");
        }
        return;
    }
    env.framework_state.uikit.globbert_debug.request = None;
    match requested {
        Request::Play(index) => play(env, controller, index),
        Request::Stop => status(env, "Stopped."),
        Request::Resume => {
            () = msg![env; controller setStopAnimationThread:false];
            () = msg![env; controller startGame];
            status(env, "Returned to normal gameplay.");
        }
    }
}

pub const CLASSES: ClassExports = objc_classes! {
(env, this, _cmd);
@implementation _touchHLE_GlobbertDebug: NSObject
- (())toggle:(id)_sender {
    let state = &mut env.framework_state.uikit.globbert_debug;
    state.open = !state.open;
    let root = state.root;
    let parent: id = msg![env; root superview];
    if !env.framework_state.uikit.globbert_debug.open {
        let search = env.framework_state.uikit.globbert_debug.search;
        () = msg![env; search resignFirstResponder];
    }
    if parent != nil { layout(env, parent); }
}
- (())filter:(id)_sender {
    let state = &mut env.framework_state.uikit.globbert_debug;
    state.endings = !state.endings; state.page = 0; refresh(env);
}
- (())previous:(id)_sender {
    let state = &mut env.framework_state.uikit.globbert_debug;
    state.page = state.page.saturating_sub(1); refresh(env);
}
- (())next:(id)_sender {
    env.framework_state.uikit.globbert_debug.page += 1; refresh(env);
}
- (())playRow:(id)sender {
    let row: NSInteger = msg![env; sender tag];
    let state = &env.framework_state.uikit.globbert_debug;
    if row < 0 || row as usize >= state.rows { return; }
    let index = state.visible.get(state.page * state.rows + row as usize).copied();
    if let Some(index) = index { request(env, Request::Play(index)); }
}
- (())stop:(id)_sender { request(env, Request::Stop); }
- (())resume:(id)_sender { request(env, Request::Resume); }
@end
};

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovers_nested_and_flat_sequences_without_including_metadata() {
        let nested = br#"<?xml version="1.0"?><plist version="1.0"><dict><key>Frames</key><dict><key>Frame 1</key><dict/><key>Frame 12</key><dict/></dict><key>Offscreen</key><true/></dict></plist>"#;
        let a = animation("DEX1.plist", nested).unwrap();
        assert_eq!(a.name, "DEX1");
        assert_eq!(a.last_frame, 12);
        assert!(!a.flat);
        let flat = br#"<?xml version="1.0"?><plist version="1.0"><dict><key>Frame 2</key><dict/></dict></plist>"#;
        assert!(animation("Headphones.plist", flat).unwrap().flat);
        assert!(animation("Info.plist", br#"<plist version="1.0"><dict><key>CFBundleName</key><string>Globbert</string></dict></plist>"#).is_none());
        assert!(animation("broken.plist", b"broken").is_none());
    }
}

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0.
 * If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//!
//! `UIBarButtonItem`.

use crate::abi::{GuestArg, GuestRet};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::{from_rust_string, get_static_str, to_rust_string};
use crate::frameworks::uikit::ui_font::UITextAlignmentCenter;
use crate::frameworks::uikit::ui_view::ui_control::{UIControlEventTouchUpInside, UIControlEvents};
use crate::objc::{
    autorelease, id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes,
    release, retain, ClassExports, NSZonePtr, SEL,
};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UIBarButtonItemStyle {
    Plain,
    Bordered,
    Done,
}

impl GuestArg for UIBarButtonItemStyle {
    const REG_COUNT: usize = 1;

    fn from_regs(regs: &[u32]) -> Self {
        UIBarButtonItemStyle::try_from(regs[0] as i32).unwrap_or(UIBarButtonItemStyle::Done)
    }

    fn to_regs(self, regs: &mut [u32]) {
        regs[0] = self as i32 as u32;
    }
}

impl GuestRet for UIBarButtonItemStyle {
    fn from_regs(regs: &[u32]) -> Self {
        UIBarButtonItemStyle::try_from(regs[0] as i32).unwrap_or(UIBarButtonItemStyle::Done)
    }

    fn to_regs(self, regs: &mut [u32]) {
        regs[0] = self as i32 as u32;
    }
}

impl TryFrom<i32> for UIBarButtonItemStyle {
    type Error = &'static str;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(UIBarButtonItemStyle::Plain),
            1 => Ok(UIBarButtonItemStyle::Bordered),
            2 => Ok(UIBarButtonItemStyle::Done),
            _ => Err("Invalid UIBarButtonItemStyle"),
        }
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UIBarButtonSystemItem {
    Done,
    Cancel,
    Edit,
    Save,
    Add,
    FlexibleSpace,
    FixedSpace,
    Compose,
    Reply,
    Action,
    Organize,
    Bookmarks,
    Search,
    Refresh,
    Stop,
    Camera,
    Trash,
    Play,
    Pause,
    Rewind,
    FastForward,
    Undo,
    Redo,
}

impl GuestArg for UIBarButtonSystemItem {
    const REG_COUNT: usize = 1;

    fn from_regs(regs: &[u32]) -> Self {
        UIBarButtonSystemItem::try_from(regs[0] as i32).unwrap_or(UIBarButtonSystemItem::Done)
    }

    fn to_regs(self, regs: &mut [u32]) {
        regs[0] = self as i32 as u32;
    }
}

impl GuestRet for UIBarButtonSystemItem {
    fn from_regs(regs: &[u32]) -> Self {
        UIBarButtonSystemItem::try_from(regs[0] as i32).unwrap_or(UIBarButtonSystemItem::Done)
    }

    fn to_regs(self, regs: &mut [u32]) {
        regs[0] = self as i32 as u32;
    }
}

impl TryFrom<i32> for UIBarButtonSystemItem {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(UIBarButtonSystemItem::Done),
            1 => Ok(UIBarButtonSystemItem::Cancel),
            2 => Ok(UIBarButtonSystemItem::Edit),
            3 => Ok(UIBarButtonSystemItem::Save),
            4 => Ok(UIBarButtonSystemItem::Add),
            5 => Ok(UIBarButtonSystemItem::FlexibleSpace),
            6 => Ok(UIBarButtonSystemItem::FixedSpace),
            7 => Ok(UIBarButtonSystemItem::Compose),
            8 => Ok(UIBarButtonSystemItem::Reply),
            9 => Ok(UIBarButtonSystemItem::Action),
            10 => Ok(UIBarButtonSystemItem::Organize),
            11 => Ok(UIBarButtonSystemItem::Bookmarks),
            12 => Ok(UIBarButtonSystemItem::Search),
            13 => Ok(UIBarButtonSystemItem::Refresh),
            14 => Ok(UIBarButtonSystemItem::Stop),
            15 => Ok(UIBarButtonSystemItem::Camera),
            16 => Ok(UIBarButtonSystemItem::Trash),
            17 => Ok(UIBarButtonSystemItem::Play),
            18 => Ok(UIBarButtonSystemItem::Pause),
            19 => Ok(UIBarButtonSystemItem::Rewind),
            20 => Ok(UIBarButtonSystemItem::FastForward),
            21 => Ok(UIBarButtonSystemItem::Undo),
            22 => Ok(UIBarButtonSystemItem::Redo),
            _ => Err(()),
        }
    }
}

impl std::fmt::Display for UIBarButtonSystemItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Done => "Done",
            Self::Cancel => "Cancel",
            Self::Edit => "Edit",
            Self::Save => "Save",
            Self::Add => "Add",
            Self::FlexibleSpace => "FlexibleSpace",
            Self::FixedSpace => "FixedSpace",
            Self::Compose => "Compose",
            Self::Reply => "Reply",
            Self::Action => "Action",
            Self::Organize => "Organize",
            Self::Bookmarks => "Bookmarks",
            Self::Search => "Search",
            Self::Refresh => "Refresh",
            Self::Stop => "Stop",
            Self::Camera => "Camera",
            Self::Trash => "Trash",
            Self::Play => "Play",
            Self::Pause => "Pause",
            Self::Rewind => "Rewind",
            Self::FastForward => "FastForward",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
        };
        write!(f, "{name}")
    }
}

struct UIBarButtonItemHostObject {
    superclass: super::UIControlHostObject,
    title: id,
    pub image: id,
    style: UIBarButtonItemStyle,
    target: id,
    action: Option<SEL>,
    system_item: UIBarButtonSystemItem,
    pub label: id,
    custom_view: id, // <-- Добавлено хранилище для customView
    width: CGFloat,
}

impl_HostObject_with_superclass!(UIBarButtonItemHostObject);

impl Default for UIBarButtonItemHostObject {
    fn default() -> Self {
        Self {
            superclass: Default::default(),
            title: nil,
            image: nil,
            style: UIBarButtonItemStyle::Plain,
            target: nil,
            action: None,
            system_item: UIBarButtonSystemItem::Done,
            label: nil,
            custom_view: nil, // <-- Инициализируем нулем
            width: 0.0,
        }
    }
}

/// Create the label that draws a titled bar button item.
fn make_title_label(env: &mut crate::Environment, title: id) -> id {
    let font: id = msg_class![env; UIFont systemFontOfSize:17_f32];
    let title_color: id = msg_class![env; UIColor blackColor];
    let item_bg_color: id = msg_class![env; UIColor whiteColor];
    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 0.0,
            height: 44.0,
        },
    };
    let title_label: id = msg_class![env; UILabel new];
    let title_label: id = msg![env; title_label initWithFrame:frame];
    () = msg![env; title_label setTextAlignment:UITextAlignmentCenter];
    () = msg![env; title_label setText:title];
    () = msg![env; title_label setTextColor:title_color];
    () = msg![env; title_label setFont:font];
    () = msg![env; title_label setBackgroundColor:item_bg_color];
    let layer: id = msg![env; title_label layer];
    () = msg![env; layer setCornerRadius:(10.0 as CGFloat)];
    title_label
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation UIBarButtonItem: UIControl

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UIBarButtonItemHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithTitle:(id)title
                style:(UIBarButtonItemStyle)style
                target:(id)target
                action:(SEL)action
{
    log_dbg!(
        "[(UIBarButtonItem*){:?} initWithTitle:{:?} style:{:?} target:{:?} action:{:?}]",
        this,
        to_rust_string(env, title),
        style,
        target,
        action
    );

    let font: id = msg_class![env; UIFont systemFontOfSize:17_f32];
    let title_color: id = msg_class![env; UIColor blackColor];
    let item_bg_color: id = msg_class![env; UIColor whiteColor];

    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 0.0, // UIToolbar will determine size based on number of items
            height: 44.0,
        },
    };

    let title_label: id = msg_class![env; UILabel new];
    let title_label: id = msg![env; title_label initWithFrame:frame];
    () = msg![env; title_label setTextAlignment:UITextAlignmentCenter];
    () = msg![env; title_label setText:title];
    () = msg![env; title_label setTextColor:title_color];
    () = msg![env; title_label setFont:font];
    () = msg![env; title_label setBackgroundColor:item_bg_color];

    let layer: id = msg![env; title_label layer];
    () = msg![env; layer setCornerRadius:(10.0 as CGFloat)];

    let host = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
    host.title = title;
    host.style = style;
    host.target = target;
    host.action = Some(action);
    host.system_item = UIBarButtonSystemItem::Done;
    host.label = title_label;

    if title != nil {
        retain(env, title);
    }
    retain(env, target);

    if target != nil {
        () = msg![env; this addTarget:target action:action forControlEvents:UIControlEventTouchUpInside];
    }

    let this: id = msg_super![env; this initWithFrame:frame];
    if title_label != nil {
        () = msg![env; this addSubview:title_label];
    }
    this
}

- (id)initWithImage:(id)image
              style:(UIBarButtonItemStyle)style
             target:(id)target
             action:(SEL)action
{
    log_dbg!(
        "[(UIBarButtonItem*){:?} initWithImage:{:?} style:{:?} target:{:?} action:{:?}]",
        this,
        image,
        style,
        target,
        action
    );

    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize { width: 0.0, height: 44.0 },
    };

    // Честно создаем UIImageView для рендера картинки на кнопке
    let image_view: id = if image != nil {
        let iv: id = msg_class![env; UIImageView alloc];
        let iv: id = msg![env; iv initWithImage:image];
        autorelease(env, iv);
        iv
    } else {
        nil
    };

    let host = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
    host.image = image;
    host.style = style;
    host.target = target;
    host.action = Some(action);
    host.system_item = UIBarButtonSystemItem::Done;

    // ХАК АРХИТЕКТУРЫ: Пишем image_view в custom_view.
    // Твои методы sizeThatFits: и layoutSubviews: автоматически
    // подхватят этот виджет, рассчитают размер и отрисуют его!
    host.custom_view = image_view;

    if image != nil {
        retain(env, image);
    }
    if image_view != nil {
        retain(env, image_view);
    }
    if target != nil {
        retain(env, target);
        () = msg![env; this addTarget:target action:action forControlEvents:UIControlEventTouchUpInside];
    }

    let this: id = msg_super![env; this initWithFrame:frame];

    if image_view != nil {
        () = msg![env; this addSubview:image_view];
    }
    this
}

- (id)initWithBarButtonSystemItem:(UIBarButtonSystemItem)system_item
                            target:(id)target
                            action:(SEL)action
{
    log_dbg!(
        "[(UIBarButtonItem*){:?} initWithBarButtonSystemItem:{} target:{:?} action:{:?}]",
        this,
        system_item,
        target,
        action
    );

    let frame = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: 0.0,
            height: 44.0,
        },
    };

    let label = match system_item {
        UIBarButtonSystemItem::FlexibleSpace |
        UIBarButtonSystemItem::FixedSpace => nil,
        _ => {
            let font: id = msg_class![env; UIFont systemFontOfSize:17_f32];
            let title_color: id = msg_class![env; UIColor blackColor];
            let item_bg_color: id = msg_class![env; UIColor whiteColor];

            let title_label: id = msg_class![env; UILabel new];
            let title_label: id = msg![env; title_label initWithFrame:frame];
            () = msg![env; title_label setTextAlignment:UITextAlignmentCenter];

            let title = from_rust_string(env, system_item.to_string());
            () = msg![env; title_label setText:title];
            () = msg![env; title_label setTextColor:title_color];
            () = msg![env; title_label setFont:font];
            () = msg![env; title_label setBackgroundColor:item_bg_color];

            let layer: id = msg![env; title_label layer];
            () = msg![env; layer setCornerRadius:(10.0 as CGFloat)];
            autorelease(env, title);
            title_label
        }
    };

    let host = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
    host.system_item = system_item;
    host.target = target;
    host.action = Some(action);
    host.label = label;

    if target != nil {
        retain(env, target);
        () = msg![env; this addTarget:target action:action forControlEvents:UIControlEventTouchUpInside];
    }

    let this: id = msg_super![env; this initWithFrame:frame];
    if label != nil {
        () = msg![env; this addSubview:label];
    }
    this
}

// Bar button items archived in a nib (e.g. the items of a UIToolbar).
- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];

    let title_key = get_static_str(env, "UITitle");
    let title: id = msg![env; coder decodeObjectForKey:title_key];
    let image_key = get_static_str(env, "UIImage");
    let image: id = msg![env; coder decodeObjectForKey:image_key];
    let custom_view_key = get_static_str(env, "UICustomView");
    let custom_view: id = msg![env; coder decodeObjectForKey:custom_view_key];
    let style_key = get_static_str(env, "UIStyle");
    let style: i32 = msg![env; coder decodeIntForKey:style_key];
    let width_key = get_static_str(env, "UIWidth");
    let width: f32 = msg![env; coder decodeFloatForKey:width_key];
    let is_system_key = get_static_str(env, "UIIsSystemItem");
    let is_system: bool = msg![env; coder decodeBoolForKey:is_system_key];
    let system_item_key = get_static_str(env, "UISystemItemIdentifier");
    let system_item: i32 = msg![env; coder decodeIntForKey:system_item_key];
    let enabled_key = get_static_str(env, "UIEnabled");
    let has_enabled: bool = msg![env; coder containsValueForKey:enabled_key];
    let enabled: bool = !has_enabled || msg![env; coder decodeBoolForKey:enabled_key];

    let system_item = if is_system {
        UIBarButtonSystemItem::try_from(system_item).unwrap_or(UIBarButtonSystemItem::Done)
    } else {
        UIBarButtonSystemItem::Done
    };
    let is_space = is_system
        && matches!(
            system_item,
            UIBarButtonSystemItem::FlexibleSpace | UIBarButtonSystemItem::FixedSpace
        );

    // Build the same subviews as the programmatic initialisers.
    let (label, image_view) = if custom_view != nil || is_space {
        (nil, nil)
    } else if image != nil {
        let iv: id = msg_class![env; UIImageView alloc];
        let iv: id = msg![env; iv initWithImage:image];
        (nil, iv)
    } else if title != nil {
        (make_title_label(env, title), nil)
    } else if is_system {
        let system_title = from_rust_string(env, system_item.to_string());
        autorelease(env, system_title);
        (make_title_label(env, system_title), nil)
    } else {
        (nil, nil)
    };

    retain(env, title);
    retain(env, image);
    retain(env, custom_view);
    {
        let host = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
        host.title = title;
        host.image = image;
        host.style = UIBarButtonItemStyle::try_from(style).unwrap_or(UIBarButtonItemStyle::Plain);
        host.system_item = system_item;
        host.width = width;
        host.label = label;
        host.custom_view = if custom_view != nil { custom_view } else { image_view };
    }
    if label != nil {
        () = msg![env; this addSubview:label];
    }
    if image_view != nil {
        () = msg![env; this addSubview:image_view];
    }
    if custom_view != nil {
        () = msg![env; this addSubview:custom_view];
    }
    () = msg![env; this setEnabled:enabled];
    this
}

// Nib connections give a bar button item its target and action without an
// event mask: the action fires when the item is tapped.
- (())addTarget:(id)target
         action:(SEL)action
forControlEvents:(UIControlEvents)events {
    let events = if events == 0 {
        if target != nil {
            retain(env, target);
        }
        let old_target = env.objc.borrow::<UIBarButtonItemHostObject>(this).target;
        release(env, old_target);
        let host = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
        host.target = target;
        host.action = Some(action);
        UIControlEventTouchUpInside
    } else {
        events
    };
    msg_super![env; this addTarget:target action:action forControlEvents:events]
}

// MARK: - НОВЫЙ МЕТОД: initWithCustomView:

- (id)initWithCustomView:(id)custom_view {
    log_dbg!(
        "[(UIBarButtonItem*){:?} initWithCustomView:{:?}]",
        this,
        custom_view
    );

    // Определяем размеры кнопки по размеру переданного view (если он есть)
    let frame = if custom_view != nil {
        let f: CGRect = msg![env; custom_view frame];
        f
    } else {
        CGRect {
            origin: CGPoint { x: 0.0, y: 0.0 },
            size: CGSize { width: 0.0, height: 44.0 },
        }
    };

    let host = env.objc.borrow_mut::<UIBarButtonItemHostObject>(this);
    host.custom_view = custom_view;

    if custom_view != nil {
        retain(env, custom_view);
    }

    // Инициализируем саму кнопку
    let this: id = msg_super![env; this initWithFrame:frame];

    // Добавляем кастомный виджет как subview, чтобы он рендерился
    if custom_view != nil {
        () = msg![env; this addSubview:custom_view];
    }

    this
}

// Геттер и сеттер для customView на всякий случай

- (id)customView {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).custom_view
}

- (())setCustomView:(id)custom_view {
    let old = env.objc.borrow::<UIBarButtonItemHostObject>(this).custom_view;
    release(env, old);
    retain(env, custom_view);
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).custom_view = custom_view;
}

- (id)label {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).label
}

- (id)title {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).title
}

- (id)image {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).image
}

- (())setImage:(id)image {
    let old = env.objc.borrow::<UIBarButtonItemHostObject>(this).image;
    release(env, old);
    retain(env, image);
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).image = image;
}

- (())setTitle:(id)title {
    let old = env.objc.borrow::<UIBarButtonItemHostObject>(this).title;
    retain(env, title);
    release(env, old);
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).title = title;
    // Apps rename items on the fly (e.g. "Clear" becoming "OK"); the label is
    // what's drawn, so update it too.
    let label = env.objc.borrow::<UIBarButtonItemHostObject>(this).label;
    if label != nil {
        () = msg![env; label setText:title];
        () = msg![env; this layoutSubviews];
    }
}

- (UIBarButtonItemStyle)style {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).style
}

- (())setStyle:(UIBarButtonItemStyle)style {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).style = style;
}

- (id)target {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).target
}

- (())setTarget:(id)target {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).target = target;
}

- (CGFloat)width {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).width
}

- (())setWidth:(CGFloat)width {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).width = width;
}

- (CGSize)sizeThatFits:(CGSize)size {
    // 1. Сначала извлекаем нужные переменные в отдельной области видимости
    //    чтобы сбросить неизменяемое заимствование (immutable borrow).
    let (custom_view, label) = {
        let host = env.objc.borrow::<UIBarButtonItemHostObject>(this);
        (host.custom_view, host.label)
    };

    // Если есть custom_view, берём размеры от него
    if custom_view != nil {
        // Like UIKit, a custom view keeps its own width (e.g. a text field
        // from a nib); only size it to fit if it has none.
        let cv_frame: CGRect = msg![env; custom_view frame];
        if cv_frame.size.width <= 0.0 {
            () = msg![env; custom_view sizeToFit];
        }
        let cv_frame: CGRect = msg![env; custom_view frame];
        CGSize {
            width: cv_frame.size.width + 16.0,
            height: size.height,
        }
    } else if label != nil {
        () = msg![env; label sizeToFit];
        let label_frame: CGRect = msg![env; label frame];
        CGSize {
            width: label_frame.size.width + 16.0, // horizontal padding
            height: size.height,
        }
    } else {
        CGSize { width: 0.0, height: size.height }
    }
}

- (())layoutSubviews {
    // 1. Опять же, ограничиваем scope неизменяемого заимствования.
    let (custom_view, label) = {
        let host = env.objc.borrow::<UIBarButtonItemHostObject>(this);
        (host.custom_view, host.label)
    };

    // 2. Теперь мы можем безопасно вызывать msg!, потому что
    //    borrow::<UIBarButtonItemHostObject> уже завершен.
    let bounds: CGRect = msg![env; this bounds];

    // Устанавливаем фрейм для custom_view, если он есть
    if custom_view != nil {
        // Centre the custom view at its own size (the item is wider by the
        // padding added in -sizeThatFits:, so filling it would grow the view
        // on every layout pass).
        let cv_frame: CGRect = msg![env; custom_view frame];
        let width = cv_frame.size.width.min(bounds.size.width);
        let height = if cv_frame.size.height > 0.0 {
            cv_frame.size.height.min(bounds.size.height)
        } else {
            bounds.size.height
        };
        let frame = CGRect {
            origin: CGPoint {
                x: ((bounds.size.width - width) / 2.0).round(),
                y: ((bounds.size.height - height) / 2.0).round(),
            },
            size: CGSize { width, height },
        };
        () = msg![env; custom_view setFrame:frame];
    } else if label != nil {
        () = msg![env; label setFrame:bounds];
    }
}

- (SEL)action {
    match env.objc.borrow::<UIBarButtonItemHostObject>(this).action {
        Some(sel) => sel,
        None => {
            log!("Warning: UIBarButtonItem has no action set!");
            env.objc.lookup_selector("undefinedSelector").unwrap()
        }
    }
}

- (())setAction:(SEL)action {
    env.objc.borrow_mut::<UIBarButtonItemHostObject>(this).action = Some(action);
}

- (UIBarButtonSystemItem)systemItem {
    env.objc.borrow::<UIBarButtonItemHostObject>(this).system_item
}

- (())dealloc {
    // Деструктурируем объект, чтобы извлечь custom_view
    let UIBarButtonItemHostObject {
        superclass: _,
        title,
        image,
        style: _,
        target,
        action: _,
        system_item: _,
        label,
        custom_view,
        width: _,
    } = std::mem::take(env.objc.borrow_mut(this));

    log_dbg!("dealloc [(UIBarButtonItem*){:?} title {:?}, target {:?}, label {:?}, custom_view {:?}]", this, title, target, label, custom_view);

    release(env, title);
    release(env, image);
    release(env, target);
    release(env, label);
    // Не забываем очистить кастомный виджет
    release(env, custom_view);

    msg_super![env; this dealloc]
}

@end

};

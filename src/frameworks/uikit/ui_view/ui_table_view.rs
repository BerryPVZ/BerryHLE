/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `UITableView`, `UITableViewCell`.

use crate::frameworks::core_graphics::{CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_string::get_static_str;
use crate::frameworks::foundation::{NSInteger, NSUInteger};
use crate::frameworks::uikit::ui_view;
use crate::frameworks::uikit::ui_view::ui_scroll_view::UIScrollViewHostObject;
use crate::frameworks::uikit::ui_view_controller::UIViewControllerHostObject;
use crate::objc::{
    id, impl_HostObject_with_superclass, msg, msg_class, msg_super, nil, objc_classes, release,
    retain, ClassExports, NSZonePtr,
};

/// How many cells `-reloadData` keeps for reuse. A table only ever shows a
/// screenful at a time, so a small pool covers every visible row.
const REUSE_POOL_LIMIT: usize = 32;

#[derive(Default)]
struct UITableViewControllerHostObject {
    superclass: UIViewControllerHostObject,
    /// `UITableViewStyle` passed to `-initWithStyle:` (0 = plain, 1 = grouped),
    /// used by `-loadView` when it lazily creates the managed table view.
    style: i32,
}
impl_HostObject_with_superclass!(UITableViewControllerHostObject);

pub struct UITableViewCellHostObject {
    superclass: ui_view::UIViewHostObject, // <-- Убираем super::
    content_view: id,
    /// `UITableViewCellStyle` passed to `-initWithStyle:reuseIdentifier:`
    /// (0 = default, 1 = value1, 2 = value2, 3 = subtitle). Selects the
    /// built-in labels' fonts, colours and layout.
    style: i32,
    /// Highlight drawn behind the content view while the row is selected,
    /// retained. `nil` until `-selectedBackgroundView` is first asked for.
    selected_background_view: id,
    /// Lazily-created default subviews (UIKit's built-in cell labels/image),
    /// each retained while it lives here. `nil` until first requested.
    text_label: id,
    detail_text_label: id,
    image_view: id,
/// Whether the row is selected. Kept separately from the highlight
/// view because `UITableViewCellSelectionStyleNone` still reports a
/// selection even though nothing is drawn.
selected: bool,
/// `UITableViewCellSelectionStyle` (0 = none, 1 = blue, 2 = gray).
selection_style: i32,
/// `UITableViewCellAccessoryType` (0 = none, 1 = disclosure indicator,
/// 2 = disclosure button, 3 = checkmark).
accessory_type: i32,
/// Custom accessory view, retained. Takes precedence over
/// `accessory_type`.
accessory_view: id,
/// Label drawing a system accessory glyph, retained. Created on
/// demand by `layout_cell_contents`.
accessory_label: id,
/// Reuse identifier from `-initWithStyle:reuseIdentifier:`, retained.
reuse_identifier: id,
}
impl_HostObject_with_superclass!(UITableViewCellHostObject);
impl Default for UITableViewCellHostObject {
    fn default() -> Self {
        UITableViewCellHostObject {
            superclass: Default::default(),
            content_view: nil,
            style: 0,
            selected_background_view: nil,
            text_label: nil,
            detail_text_label: nil,
            image_view: nil,
selected: false,
selection_style: 1,
accessory_type: 0,
accessory_view: nil,
accessory_label: nil,
reuse_identifier: nil,
        }
    }
}

pub struct UITableViewHostObject {
    /// Inherits all UIScrollView (and therefore UIView) state. Without this
    /// field the table view's host object would not be borrow-compatible with
    /// the superclass methods, which would route through the use-after-free
    /// fallback and corrupt rendering.
    superclass: UIScrollViewHostObject,
    /// Non-retained delegate (`UITableViewDelegate`).
    table_delegate: id,
    /// Non-retained data source (`UITableViewDataSource`).
    data_source: id,
    /// `UITableViewStyle` passed to `-initWithFrame:style:` (0 = plain,
    /// 1 = grouped). Grouped tables inset their cells and draw the section
    /// header titles the data source reports.
    style: i32,
    /// Section header labels created by `-reloadData`, retained while they
    /// are subviews. Removed and released on the next reload.
    section_views: Vec<id>,
    /// Header title per section, as reported by the data source.
    section_titles: Vec<String>,
    /// Index paths of the selected rows, each retained. Empty when
    /// nothing is selected. More than one entry only when
    /// `-setAllowsMultipleSelection:YES` was used.
    selected_paths: Vec<id>,
    /// Whether taps add to the selection instead of replacing it.
    allows_multiple_selection: bool,
    /// Cells handed back by `-reloadData`, keyed by reuse identifier and
    /// retained. `-dequeueReusableCellWithIdentifier:` takes one out.
    reuse_pool: Vec<(String, id)>,
    /// Whether taps select rows. `-setAllowsSelection:` writes this.
    allows_selection: bool,
    /// Touch that started on a row, with its start point, so a drag can be
    /// told apart from a tap.
    selection_touch: Option<(id, CGPoint)>,
    /// Width the rows were last laid out at. A grouped table's cell width
    /// depends on it, so a resize has to lay the rows out again.
    layout_width: f32,
    /// Cells currently laid out, top-to-bottom in the order generated by
    /// `reloadData`. Each cell is retained while it is in this list.
    cells: Vec<id>,
    /// Index paths matching `cells` 1:1.
    cell_index_paths: Vec<id>,
    row_height: f32,
    section_header_height: f32,
    section_footer_height: f32,
    separator_style: NSInteger,
    separator_color: id,
    table_header_view: id,
    table_footer_view: id,
    /// Whether we have performed the implicit first `reloadData`. UIKit
    /// reloads a table the first time it lays out with a data source; touchHLE
    /// has no automatic layout pass, so we trigger that reload ourselves the
    /// first time the table has both a data source and a non-zero size.
    auto_reloaded: bool,
    /// Whether the table has had its first `layoutSubviews`. UIKit's initial
    /// load happens then, after the owning view controller's `-viewDidLoad`
    /// has prepared the data, so the table is reloaded again at that point.
    laid_out: bool,
}
impl Default for UITableViewHostObject {
    fn default() -> Self {
        UITableViewHostObject {
            superclass: Default::default(),
            table_delegate: nil,
            data_source: nil,
            style: 0,
            section_views: Vec::new(),
            section_titles: Vec::new(),
            selected_paths: Vec::new(),
            allows_multiple_selection: false,
            reuse_pool: Vec::new(),
            allows_selection: true,
            selection_touch: None,
            layout_width: 0.0,
            cells: Vec::new(),
            cell_index_paths: Vec::new(),
            row_height: 44.0,
            section_header_height: 22.0,
            section_footer_height: 0.0,
            separator_style: 1, // UITableViewCellSeparatorStyleSingleLine
            separator_color: nil,
            table_header_view: nil,
            table_footer_view: nil,
            auto_reloaded: false,
            laid_out: false,
        }
    }
}
impl_HostObject_with_superclass!(UITableViewHostObject);

/// Perform the implicit first `reloadData` that UIKit does during a table's
/// first layout. Called whenever the data source or size changes; only fires
/// once, and only once both a data source and a usable size are available.
fn maybe_auto_reload(env: &mut crate::Environment, this: id) {
    let bounds: CGRect = msg![env; this bounds];
    let (bw, bh) = (bounds.size.width, bounds.size.height);
    {
        let host = env.objc.borrow::<UITableViewHostObject>(this);
        if host.data_source == nil {
            return;
        }
        // A grouped table's cell width depends on the table's width, so a
        // resize (rotation, split view) has to lay the rows out again.
        if host.auto_reloaded && host.layout_width == bw {
            return;
        }
    }
    if bw <= 0.0 || bh <= 0.0 {
        return;
    }
    {
        let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
        host.auto_reloaded = true;
        host.layout_width = bw;
    }
    () = msg![env; this reloadData];
}

/// Drop the cell list and (optionally) remove the cell views from the table.
fn clear_cells(env: &mut crate::Environment, this: id, remove_subviews: bool) {
    // Take the vectors out so we can release without nested borrows.
    let (cells, paths, decorations) = {
        let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
        (
            std::mem::take(&mut host.cells),
            std::mem::take(&mut host.cell_index_paths),
            std::mem::take(&mut host.section_views),
        )
    };
    for view in decorations {
        if remove_subviews {
            let _: () = msg![env; view removeFromSuperview];
        }
        release(env, view);
    }
    for cell in cells {
        if remove_subviews {
            let _: () = msg![env; cell removeFromSuperview];
        }
        // A cell that carries a reuse identifier goes back to the pool
        // so the next reload can hand it out again instead of making the
        // data source allocate a new one.
        let identifier: id = msg![env; cell reuseIdentifier];
        if identifier != nil {
            let key = crate::frameworks::foundation::ns_string::to_rust_string(env, identifier)
                .into_owned();
            let pooled = env.objc.borrow::<UITableViewHostObject>(this).reuse_pool.len();
            if pooled < REUSE_POOL_LIMIT {
                env.objc
                    .borrow_mut::<UITableViewHostObject>(this)
                    .reuse_pool
                    .push((key, cell));
                continue;
            }
        }
        release(env, cell);
    }
    for path in paths {
        release(env, path);
    }
}

fn make_rect(x: f32, y: f32, w: f32, h: f32) -> CGRect {
    CGRect {
        origin: CGPoint { x, y },
        size: CGSize {
            width: w,
            height: h,
        },
    }
}

/// Position a `UITableViewCell`'s built-in content view, image view and text
/// labels, mirroring UIKit's default cell layout closely enough to be
/// recognisable: the optional image sits on the left and the text label fills
/// the remaining width. touchHLE has no automatic layout pass, so this is
/// invoked whenever the cell's frame changes or a default subview is created.
fn layout_cell_contents(env: &mut crate::Environment, this: id) {
    let bounds: CGRect = msg![env; this bounds];
    let (w, h) = (bounds.size.width, bounds.size.height);
    let (content_view, image_view, text_label, detail_label) = {
        let host = env.objc.borrow::<UITableViewCellHostObject>(this);
        (
            host.content_view,
            host.image_view,
            host.text_label,
            host.detail_text_label,
        )
    };

    if content_view != nil {
        () = msg![env; content_view setFrame:(make_rect(0.0, 0.0, w, h))];
    }

    // The highlight sits behind the content, so it has to track the
    // cell's size as well.
    let selected_background_view =
        env.objc.borrow::<UITableViewCellHostObject>(this).selected_background_view;
    if selected_background_view != nil {
        () = msg![env; selected_background_view setFrame:(make_rect(0.0, 0.0, w, h))];
    }

    let mut text_x = 10.0;
    if image_view != nil {
        let image: id = msg![env; image_view image];
        if image != nil && h > 0.0 {
            () = msg![env; image_view setFrame:(make_rect(0.0, 0.0, h, h))];
            text_x = h + 10.0;
        } else {
            () = msg![env; image_view setFrame:(make_rect(0.0, 0.0, 0.0, 0.0))];
        }
    }

    // A custom accessory view takes the right edge; a system accessory
    // (disclosure indicator or checkmark) is drawn as a glyph in a
    // narrower slot. Either way the text has to stop before it.
    let (accessory_view, accessory_type) = {
        let host = env.objc.borrow::<UITableViewCellHostObject>(this);
        (host.accessory_view, host.accessory_type)
    };
    let mut accessory_w = 0.0;
    if accessory_view != nil {
        accessory_w = 44.0f32.min(w * 0.5);
        () = msg![env; accessory_view setFrame:(make_rect(w - accessory_w, 0.0, accessory_w, h))];
        // A custom view replaces the built-in glyph rather than
        // drawing on top of it.
        let label = env.objc.borrow::<UITableViewCellHostObject>(this).accessory_label;
        if label != nil {
            () = msg![env; label setHidden:true];
        }
    } else if accessory_type != 0 {
        accessory_w = 22.0;
        let label = ensure_cell_accessory_label(env, this);
        let glyph = if accessory_type == 3 { "\u{2713}" } else { "\u{203a}" };
        let text = get_static_str(env, glyph);
        () = msg![env; label setText:text];
        () = msg![env; label setHidden:false];
        () = msg![env; label setFrame:(make_rect(w - accessory_w, 0.0, accessory_w, h))];
    } else {
        let label = env.objc.borrow::<UITableViewCellHostObject>(this).accessory_label;
        if label != nil {
            () = msg![env; label setHidden:true];
        }
    }

    let text_w = (w - text_x - 10.0 - accessory_w).max(0.0);
    let style = env.objc.borrow::<UITableViewCellHostObject>(this).style;
    if text_label != nil && detail_label != nil && (style == 1 || style == 2) {
        // Value cells put the title and the detail on one line, the detail
        // right-aligned. Value2 gives the title a fixed narrow column.
        let detail_size: CGSize = msg![env; detail_label sizeThatFits:(CGSize { width: text_w, height: h })];
        let detail_w = if style == 2 {
            (text_w - 86.0).max(0.0)
        } else {
            detail_size.width.min(text_w * 0.6).max(0.0)
        };
        let title_w = if style == 2 {
            80.0f32.min(text_w)
        } else {
            (text_w - detail_w - 10.0).max(0.0)
        };
        () = msg![env; text_label setFrame:(make_rect(text_x, 0.0, title_w, h))];
        () = msg![env; detail_label setFrame:(make_rect(w - 10.0 - detail_w, 0.0, detail_w, h))];
    } else if text_label != nil && detail_label != nil && style == 3 {
        // Subtitle: the title sits over the detail, both centred vertically.
        let title: CGSize = msg![env; text_label sizeThatFits:(CGSize { width: text_w, height: h })];
        let detail: CGSize = msg![env; detail_label sizeThatFits:(CGSize { width: text_w, height: h })];
        let title_h = title.height.min(h * 0.6).max(0.0);
        let detail_h = detail.height.min((h - title_h).max(0.0)).max(0.0);
        let y = ((h - title_h - detail_h) / 2.0).max(0.0);
        () = msg![env; text_label setFrame:(make_rect(text_x, y, text_w, title_h))];
        () = msg![env; detail_label setFrame:(make_rect(text_x, y + title_h, text_w, detail_h))];
    } else if text_label != nil && detail_label != nil {
        // Two-line cell: stack the title over the detail text.
        () = msg![env; text_label setFrame:(make_rect(text_x, 0.0, text_w, h * 0.6))];
        () = msg![env; detail_label setFrame:(make_rect(text_x, h * 0.6, text_w, h * 0.4))];
    } else if text_label != nil {
        () = msg![env; text_label setFrame:(make_rect(text_x, 0.0, text_w, h))];
    }
}

/// Lazily create the label that draws a system accessory glyph.
fn ensure_cell_accessory_label(env: &mut crate::Environment, this: id) -> id {
    let existing = env.objc.borrow::<UITableViewCellHostObject>(this).accessory_label;
    if existing != nil {
        return existing;
    }
    let label = ensure_cell_subview(env, this, "UILabel");
    let font: id = msg_class![env; UIFont boldSystemFontOfSize:(20.0f32)];
    () = msg![env; label setFont:font];
    () = msg![env; label setTextAlignment:(1i32)];
    let gray: id = msg_class![env; UIColor grayColor];
    () = msg![env; label setTextColor:gray];
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).accessory_label = label;
    label
}

/// Lazily create one of the cell's default subviews (a `UILabel` or
/// `UIImageView`) with a transparent background so the cell shows through,
/// add it to the content view, and store it in `slot`.
fn ensure_cell_subview(env: &mut crate::Environment, this: id, class_name: &str) -> id {
    let content_view = env
        .objc
        .borrow::<UITableViewCellHostObject>(this)
        .content_view;
    let class = env.objc.get_known_class(class_name, &mut env.mem);
    let view: id = msg![env; class alloc];
    let view: id = msg![env; view init];
    let clear: id = msg_class![env; UIColor clearColor];
    () = msg![env; view setBackgroundColor:clear];
    retain(env, view);
    if content_view != nil {
        () = msg![env; content_view addSubview:view];
    }
    view
}

/// Whether a touch that started at `start` and ended at `end` is a tap rather
/// than a drag. Ten points is the slop UIKit uses before a touch becomes a
/// scroll, and a scroll must never select the row under the finger.
fn is_tap(start: CGPoint, end: CGPoint) -> bool {
    (start.x - end.x).hypot(start.y - end.y) < 10.0
}

/// Turn a completed tap into a selection: ask the delegate whether the row
/// may be selected, then select it and report the selection. A drag of ten
/// points or more, or a touch that ends on a different row, is a scroll.
fn select_tapped_row(env: &mut crate::Environment, table: id, start: CGPoint, end: CGPoint) {
    if !env.objc.borrow::<UITableViewHostObject>(table).allows_selection {
        return;
    }
    if !is_tap(start, end) {
        return;
    }
    let first: id = msg![env; table indexPathForRowAtPoint:start];
    let last: id = msg![env; table indexPathForRowAtPoint:end];
    if first == nil || last == nil {
        return;
    }
    let same: bool = msg![env; first isEqual:last];
    if !same {
        return;
    }
    retain(env, last);
    let delegate = env.objc.borrow::<UITableViewHostObject>(table).table_delegate;
    if delegate != nil {
        retain(env, delegate);
    }
    let selector = env
        .objc
        .register_host_selector("tableView:willSelectRowAtIndexPath:".to_string(), &mut env.mem);
    let path: id = if delegate != nil && msg![env; delegate respondsToSelector:selector] {
        msg![env; delegate tableView:table willSelectRowAtIndexPath:last]
    } else {
        last
    };
    if path != nil {
        retain(env, path);
    }
    release(env, last);
    if path != nil {
        // With multiple selection a tap toggles the row, which is how
        // mail-style lists behave.
        let multiple = env
            .objc
            .borrow::<UITableViewHostObject>(table)
            .allows_multiple_selection;
        let already = if multiple {
            let selected = env
                .objc
                .borrow::<UITableViewHostObject>(table)
                .selected_paths
                .clone();
            let mut found = false;
            for other in selected {
                let equal: bool = msg![env; other isEqual:path];
                if equal {
                    found = true;
                    break;
                }
            }
            found
        } else {
            false
        };
        if already {
            () = msg![env; table deselectRowAtIndexPath:path animated:false];
        } else {
            () = msg![env; table selectRowAtIndexPath:path animated:false scrollPosition:(0i32)];
        }
        // A toggle that turned the row off reports a deselection, so the
        // delegate sees the same pair of calls UIKit would send.
        let name = if already {
            "tableView:didDeselectRowAtIndexPath:"
        } else {
            "tableView:didSelectRowAtIndexPath:"
        };
        let selector = env
            .objc
            .register_host_selector(name.to_string(), &mut env.mem);
        if delegate != nil && msg![env; delegate respondsToSelector:selector] {
            if already {
                () = msg![env; delegate tableView:table didDeselectRowAtIndexPath:path];
            } else {
                () = msg![env; delegate tableView:table didSelectRowAtIndexPath:path];
            }
        }
        release(env, path);
    }
    if delegate != nil {
        release(env, delegate);
    }
}

/// Vertical content offset that brings a row into view for a given
/// `UITableViewScrollPosition` (0 = none, 1 = top, 2 = middle, 3 = bottom),
/// clamped to the scrollable range.
fn scroll_offset_for(
    position: i32,
    row_top: f32,
    row_height: f32,
    view_height: f32,
    content_height: f32,
    current: f32,
) -> f32 {
    let target = match position {
        1 => row_top,
        2 => row_top - (view_height - row_height) * 0.5,
        3 => row_top + row_height - view_height,
        _ => current,
    };
    let max_y = (content_height - view_height).max(0.0);
    target.clamp(0.0, max_y)
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// Добавляем приватный класс, на который ругается NIB-декодер
@implementation UITableViewCellContentView: UIView

- (id)initWithFrame:(CGRect)frame {
    msg_super![env; this initWithFrame:frame]
}

- (id)initWithCoder:(id)coder {
    msg_super![env; this initWithCoder:coder]
}

@end

@implementation UITableViewCell: UIView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITableViewCellHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame {
    let this: id = msg_super![env; this initWithFrame:frame];

    // Честно создаем внутренний content_view.
    // Передаем имя класса напрямую в макрос msg_class!
    let content_view: id = msg_class![env; UITableViewCellContentView alloc];
    let content_view: id = msg![env; content_view initWithFrame:frame];

    // Добавляем его как subview (как в iOS)
    () = msg![env; this addSubview:content_view];

    env.objc.borrow_mut::<UITableViewCellHostObject>(this).content_view = content_view;
    this
}

- (id)initWithStyle:(i32)style reuseIdentifier:(id)identifier {
        env.objc.borrow_mut::<UITableViewCellHostObject>(this).style = style;
        if identifier != nil {
            retain(env, identifier);
            let replaced = {
                let host = env.objc.borrow_mut::<UITableViewCellHostObject>(this);
                std::mem::replace(&mut host.reuse_identifier, identifier)
            };
            if replaced != nil {
                release(env, replaced);
            }
        }
        let frame = <CGRect as Default>::default();
        msg![env; this initWithFrame:frame]
    }

// `-[UITableViewCell initWithFrame:reuseIdentifier:]` is the iOS 2.x
// designated initializer, deprecated in iOS 3.0 in favour of
// `-initWithStyle:reuseIdentifier:`. The header (UIKit's
// `UITableViewCell.h`) still ships the symbol, and several legacy 32-bit
// games (e.g. Cooking Fever, Dungeon Shooter) call it. Apple's reference
// implementation routes it through the frame-only initializer and
// records the reuse identifier — the reuse identifier itself is held by
// the cell so that `+[UITableView dequeueReusableCellWithIdentifier:]`
// can later match it. <https://developer.apple.com/documentation/uikit/uitableviewcell/1623200-initwithframe>
- (id)initWithFrame:(CGRect)frame reuseIdentifier:(id)identifier {
        let this: id = msg![env; this initWithFrame:frame];
        if identifier != nil {
            retain(env, identifier);
            let replaced = {
                let host = env.objc.borrow_mut::<UITableViewCellHostObject>(this);
                std::mem::replace(&mut host.reuse_identifier, identifier)
            };
            if replaced != nil {
                release(env, replaced);
            }
        }
        this
    }

- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    // При загрузке из NIB contentView обычно уже внутри (как под-вьюха).
    // Мы можем просто инициализировать поле, если NIB его создал,
    // но для безопасности создадим пустой, если его нет.
    let frame = <CGRect as Default>::default();

    // Исправлено здесь: убран get_known_class и переменная, имя класса передано
    // напрямую
    let content_view: id = msg_class![env; UITableViewCellContentView alloc];
    let content_view: id = msg![env; content_view initWithFrame:frame];

    () = msg![env; this addSubview:content_view];
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).content_view = content_view;
    this
}

- (())dealloc {
let (
    content_view,
    text_label,
    detail_label,
    image_view,
    selected_background_view,
    accessory_view,
    accessory_label,
    reuse_identifier,
) = {
    let host = env.objc.borrow::<UITableViewCellHostObject>(this);
    (
        host.content_view,
        host.text_label,
        host.detail_text_label,
        host.image_view,
        host.selected_background_view,
        host.accessory_view,
        host.accessory_label,
        host.reuse_identifier,
    )
};
    release(env, content_view);
    if text_label != nil { release(env, text_label); }
    if detail_label != nil { release(env, detail_label); }
    if image_view != nil { release(env, image_view); }
    if selected_background_view != nil { release(env, selected_background_view); }
if accessory_view != nil { release(env, accessory_view); }
if accessory_label != nil { release(env, accessory_label); }
if reuse_identifier != nil { release(env, reuse_identifier); }
    
    msg_super![env; this dealloc]
}

- (id)reuseIdentifier {
    env.objc.borrow::<UITableViewCellHostObject>(this).reuse_identifier
}

- (id)textLabel {
    let existing = env.objc.borrow::<UITableViewCellHostObject>(this).text_label;
    if existing != nil {
        return existing;
    }
    let label = ensure_cell_subview(env, this, "UILabel");
    let style = env.objc.borrow::<UITableViewCellHostObject>(this).style;
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).text_label = label;
    // Value2 puts a small blue title in a fixed left column.
    let font: id = if style == 2 {
        msg_class![env; UIFont systemFontOfSize:(12.0f32)]
    } else {
        msg_class![env; UIFont boldSystemFontOfSize:(17.0f32)]
    };
    () = msg![env; label setFont:font];
    if style == 2 {
        let blue: id = msg_class![env; UIColor colorWithRed:(0.22f32) green:(0.33f32) blue:(0.53f32) alpha:(1.0f32)];
        () = msg![env; label setTextColor:blue];
        () = msg![env; label setTextAlignment:(2i32)];
    }
    layout_cell_contents(env, this);
    label
}

- (id)detailTextLabel {
    let style = env.objc.borrow::<UITableViewCellHostObject>(this).style;
    // The default style has no detail label at all.
    if style == 0 {
        return nil;
    }
    let existing = env.objc.borrow::<UITableViewCellHostObject>(this).detail_text_label;
    if existing != nil {
        return existing;
    }
    let label = ensure_cell_subview(env, this, "UILabel");
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).detail_text_label = label;
    let size = if style == 3 {
        14.0f32
    } else if style == 2 {
        15.0f32
    } else {
        17.0f32
    };
    let font: id = msg_class![env; UIFont systemFontOfSize:size];
    () = msg![env; label setFont:font];
    if style == 1 {
        let blue: id = msg_class![env; UIColor colorWithRed:(0.22f32) green:(0.33f32) blue:(0.53f32) alpha:(1.0f32)];
        () = msg![env; label setTextColor:blue];
        () = msg![env; label setTextAlignment:(2i32)];
    } else if style == 3 {
        let gray: id = msg_class![env; UIColor grayColor];
        () = msg![env; label setTextColor:gray];
    }
    layout_cell_contents(env, this);
    label
}

// ---- Deprecated iOS 2.x cell text/image API (replaced by textLabel / imageView
// in iOS 3.0). Forwarded to the default subviews. ----

- (id)text {
    let label = env.objc.borrow::<UITableViewCellHostObject>(this).text_label;
    if label == nil { return nil; }
    msg![env; label text]
}
- (())setText:(id)text {
    let label: id = msg![env; this textLabel];
    () = msg![env; label setText:text];
}

- (id)font {
    let label = env.objc.borrow::<UITableViewCellHostObject>(this).text_label;
    if label == nil { return nil; }
    msg![env; label font]
}
- (())setFont:(id)font {
    let label: id = msg![env; this textLabel];
    () = msg![env; label setFont:font];
}

- (id)textColor {
    let label = env.objc.borrow::<UITableViewCellHostObject>(this).text_label;
    if label == nil { return nil; }
    msg![env; label textColor]
}
- (())setTextColor:(id)color {
    let label: id = msg![env; this textLabel];
    () = msg![env; label setTextColor:color];
}

// Old name for UILabel's highlightedTextColor.
- (id)selectedTextColor {
    let label = env.objc.borrow::<UITableViewCellHostObject>(this).text_label;
    if label == nil { return nil; }
    msg![env; label highlightedTextColor]
}
- (())setSelectedTextColor:(id)color {
    let label: id = msg![env; this textLabel];
    () = msg![env; label setHighlightedTextColor:color];
}

- (i32)textAlignment {
    let label = env.objc.borrow::<UITableViewCellHostObject>(this).text_label;
    if label == nil { return 0; } // UITextAlignmentLeft
    msg![env; label textAlignment]
}
- (())setTextAlignment:(i32)alignment {
    let label: id = msg![env; this textLabel];
    () = msg![env; label setTextAlignment:alignment];
}

- (i32)lineBreakMode {
    let label = env.objc.borrow::<UITableViewCellHostObject>(this).text_label;
    if label == nil { return 4; } // UILineBreakModeTailTruncation (UILabel default)
    msg![env; label lineBreakMode]
}
- (())setLineBreakMode:(i32)mode {
    let label: id = msg![env; this textLabel];
    () = msg![env; label setLineBreakMode:mode];
}

- (id)image {
    let iv = env.objc.borrow::<UITableViewCellHostObject>(this).image_view;
    if iv == nil { return nil; }
    msg![env; iv image]
}
- (())setImage:(id)image {
    let iv: id = msg![env; this imageView];
    () = msg![env; iv setImage:image];
    // The text label's x offset depends on whether an image is present.
    layout_cell_contents(env, this);
}

- (id)selectedImage {
    let iv = env.objc.borrow::<UITableViewCellHostObject>(this).image_view;
    if iv == nil { return nil; }
    msg![env; iv highlightedImage]
}
- (())setSelectedImage:(id)image {
    let iv: id = msg![env; this imageView];
    () = msg![env; iv setHighlightedImage:image];
}

- (id)imageView {
    let existing = env.objc.borrow::<UITableViewCellHostObject>(this).image_view;
    if existing != nil {
        return existing;
    }
    let iv = ensure_cell_subview(env, this, "UIImageView");
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).image_view = iv;
    layout_cell_contents(env, this);
    iv
}

- (())setFrame:(CGRect)frame {
    () = msg_super![env; this setFrame:frame];
    layout_cell_contents(env, this);
}

- (())layoutSubviews {
    () = msg_super![env; this layoutSubviews];
    layout_cell_contents(env, this);
}

// Теперь возвращаем реальный contentView, а не this
- (id)contentView {
    env.objc.borrow::<UITableViewCellHostObject>(this).content_view
}

- (id)accessoryView {
    env.objc.borrow::<UITableViewCellHostObject>(this).accessory_view
}
- (())setAccessoryView:(id)view {
        if view != nil {
            retain(env, view);
        }
        let old = {
            let host = env.objc.borrow_mut::<UITableViewCellHostObject>(this);
            std::mem::replace(&mut host.accessory_view, view)
        };
        if old != nil {
            () = msg![env; old removeFromSuperview];
            release(env, old);
        }
        if view != nil {
            () = msg![env; this addSubview:view];
        }
        layout_cell_contents(env, this);
    }
- (i32)accessoryType {
    env.objc.borrow::<UITableViewCellHostObject>(this).accessory_type
}
- (())setAccessoryType:(i32)accessory_type {
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).accessory_type = accessory_type;
    layout_cell_contents(env, this);
}
- (i32)selectionStyle {
    env.objc.borrow::<UITableViewCellHostObject>(this).selection_style
}
- (())setSelectionStyle:(i32)style {
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).selection_style = style;
    // Turning the highlight off while the row is selected has to remove
    // the highlight that is already showing.
    if style == 0 {
        let view = env.objc.borrow::<UITableViewCellHostObject>(this).selected_background_view;
        if view != nil {
            () = msg![env; view setHidden:true];
        }
    }
}
- (bool)isSelected {
    env.objc.borrow::<UITableViewCellHostObject>(this).selected
}
- (())setSelected:(bool)selected {
    () = msg![env; this setSelected:selected animated:false];
}
- (())setSelected:(bool)selected animated:(bool)_animated {
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).selected = selected;
    // `UITableViewCellSelectionStyleNone` keeps the state but draws no
    // highlight, which is how a cell with a custom background opts out.
    let style = env.objc.borrow::<UITableViewCellHostObject>(this).selection_style;
    if style == 0 {
        return;
    }
    let view: id = msg![env; this selectedBackgroundView];
    let hidden = !selected;
    () = msg![env; view setHidden:hidden];
}
- (bool)isHighlighted { false }
- (())setHighlighted:(bool)_highlighted {}
- (())setHighlighted:(bool)_highlighted animated:(bool)_animated {}
- (())prepareForReuse {
    // UIKit resets the per-row state here, because the cell is about to be
    // handed to a different row. Without this a recycled cell keeps the
    let view = {
        let host = env.objc.borrow_mut::<UITableViewCellHostObject>(this);
        host.selected = false;
        host.selected_background_view
    };
    if view != nil {
        () = msg![env; view setHidden:true];
    }
}
- (id)backgroundView { nil }
- (())setBackgroundView:(id)_view {}
- (id)selectedBackgroundView {
    let existing = env.objc.borrow::<UITableViewCellHostObject>(this).selected_background_view;
    if existing != nil {
        return existing;
    }
    let view: id = msg_class![env; UIView alloc];
    let frame: CGRect = msg![env; this bounds];
    let view: id = msg![env; view initWithFrame:frame];
    let gray: id = msg_class![env; UIColor colorWithWhite:(0.85f32) alpha:(1.0f32)];
    () = msg![env; view setBackgroundColor:gray];
    () = msg![env; view setHidden:true];
    () = msg![env; this insertSubview:view atIndex:(0i32)];
    retain(env, view);
    env.objc.borrow_mut::<UITableViewCellHostObject>(this).selected_background_view = view;
    view
}
- (())setSelectedBackgroundView:(id)view {
    if view != nil {
        retain(env, view);
        () = msg![env; this insertSubview:view atIndex:(0i32)];
    }
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<UITableViewCellHostObject>(this).selected_background_view,
        view,
    );
    if old != nil {
        () = msg![env; old removeFromSuperview];
        release(env, old);
    }
}
- (())setEditing:(bool)_editing animated:(bool)_animated {}
- (bool)isEditing { false }
- (i32)editingStyle { 0 }
- (bool)showingDeleteConfirmation { false }
- (())setIndentationLevel:(i32)_level {}
- (i32)indentationLevel { 0 }

@end

@implementation UIProgressView: UIView

- (id)initWithFrame:(CGRect)frame {
    msg_super![env; this initWithFrame:frame]
}

- (id)initWithProgressViewStyle:(i32)_style {
    msg_super![env; this init]
}

- (f32)progress { 0.0 }
- (())setProgress:(f32)_progress {}
- (())setProgress:(f32)_progress animated:(bool)_animated {}
- (i32)progressViewStyle { 0 }
- (())setProgressViewStyle:(i32)_style {}
- (id)progressTintColor { nil }
- (())setProgressTintColor:(id)_color {}
- (id)trackTintColor { nil }
- (())setTrackTintColor:(id)_color {}
- (id)progressImage { nil }
- (())setProgressImage:(id)_image {}
- (id)trackImage { nil }
- (())setTrackImage:(id)_image {}

@end

@implementation UITableView: UIScrollView

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITableViewHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithFrame:(CGRect)frame style:(i32)style {
    let this: id = msg_super![env; this initWithFrame:frame];
    env.objc.borrow_mut::<UITableViewHostObject>(this).style = style;
    this
}

- (i32)style {
    env.objc.borrow::<UITableViewHostObject>(this).style
}

- (id)initWithCoder:(id)coder {
    let this: id = msg_super![env; this initWithCoder:coder];
    let style_key = get_static_str(env, "UIStyle");
    let style: i32 = msg![env; coder decodeIntForKey:style_key];
    env.objc.borrow_mut::<UITableViewHostObject>(this).style = style;
    let row_height_key = get_static_str(env, "UIRowHeight");
    let has_row_height: bool = msg![env; coder containsValueForKey:row_height_key];
    if has_row_height {
        let row_height: f32 = msg![env; coder decodeFloatForKey:row_height_key];
        if row_height > 0.0 {
            env.objc.borrow_mut::<UITableViewHostObject>(this).row_height = row_height;
        }
    }
    let separator_style_key = get_static_str(env, "UISeparatorStyle");
    let has_separator_style: bool = msg![env; coder containsValueForKey:separator_style_key];
    if has_separator_style {
        let separator_style: NSInteger = msg![env; coder decodeIntegerForKey:separator_style_key];
        env.objc.borrow_mut::<UITableViewHostObject>(this).separator_style = separator_style;
    }
    this
}

- (())layoutSubviews {
    () = msg_super![env; this layoutSubviews];
    let (laid_out, data_source) = {
        let host = env.objc.borrow::<UITableViewHostObject>(this);
        (host.laid_out, host.data_source)
    };
    if !laid_out && data_source != nil {
        env.objc.borrow_mut::<UITableViewHostObject>(this).laid_out = true;
        env.objc.borrow_mut::<UITableViewHostObject>(this).auto_reloaded = true;
        () = msg![env; this reloadData];
    }
}

- (())dealloc {
    clear_cells(env, this, false);
    let header = env.objc.borrow::<UITableViewHostObject>(this).table_header_view;
    let footer = env.objc.borrow::<UITableViewHostObject>(this).table_footer_view;
    let separator_color = env.objc.borrow::<UITableViewHostObject>(this).separator_color;
    if header != nil { release(env, header); }
    if footer != nil { release(env, footer); }
    if separator_color != nil { release(env, separator_color); }
        let selected = std::mem::take(
            &mut env.objc.borrow_mut::<UITableViewHostObject>(this).selected_paths,
        );
        for path in selected {
            release(env, path);
        }
        let pooled = std::mem::take(
            &mut env.objc.borrow_mut::<UITableViewHostObject>(this).reuse_pool,
        );
        for (_, cell) in pooled {
            release(env, cell);
        }
    msg_super![env; this dealloc]
}

- (id)visibleCells {
    // We don't track visibility separately from layout, so report every cell
    // currently materialised by -reloadData.
    let cells = env.objc.borrow::<UITableViewHostObject>(this).cells.clone();
    let array: id = msg_class![env; NSMutableArray array];
    for cell in cells {
        () = msg![env; array addObject:cell];
    }
    array
}

- (id)delegate {
    env.objc.borrow::<UITableViewHostObject>(this).table_delegate
}
- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<UITableViewHostObject>(this).table_delegate = delegate;
}
- (id)dataSource {
    env.objc.borrow::<UITableViewHostObject>(this).data_source
}
- (())setDataSource:(id)data_source {
    env.objc.borrow_mut::<UITableViewHostObject>(this).data_source = data_source;
    maybe_auto_reload(env, this);
}

- (())setFrame:(CGRect)frame {
    () = msg_super![env; this setFrame:frame];
    maybe_auto_reload(env, this);
}

- (())setBounds:(CGRect)bounds {
    () = msg_super![env; this setBounds:bounds];
    maybe_auto_reload(env, this);
}

- (())reloadData {
    // Drop existing cells before recreating them.
    clear_cells(env, this, true);

    let data_source = env.objc.borrow::<UITableViewHostObject>(this).data_source;
    if data_source == nil {
        return;
    }

    let bounds: CGRect = msg![env; this bounds];
    let width = bounds.size.width;
    let (row_height, grouped) = {
        let host = env.objc.borrow::<UITableViewHostObject>(this);
        (host.row_height, host.style != 0)
    };
    // Grouped tables inset their cells and draw a header per section.
    let cell_width = if grouped { (width - 20.0).max(0.0) } else { width };
    let inset = (width - cell_width) / 2.0;

    // Section count defaults to 1 if the data source does not implement
    // -numberOfSectionsInTableView:.
    let sel_num_sections = env.objc.lookup_selector("numberOfSectionsInTableView:");
    let mut num_sections: NSInteger = 1;
    if let Some(sel) = sel_num_sections {
        let responds: bool = msg![env; data_source respondsToSelector:sel];
        if responds {
            num_sections = msg![env; data_source numberOfSectionsInTableView:this];
        }
    }

    let mut y: f32 = 0.0;
    let header = env.objc.borrow::<UITableViewHostObject>(this).table_header_view;
    if header != nil {
        let mut frame: CGRect = msg![env; header frame];
        let h = frame.size.height.max(0.0);
        frame.origin = CGPoint { x: 0.0, y };
        frame.size.width = width;
        () = msg![env; header setFrame:frame];
        () = msg![env; this addSubview:header];
        y += h;
    }

    let mut new_cells: Vec<id> = Vec::new();
    let mut new_paths: Vec<id> = Vec::new();
    let mut section_views: Vec<id> = Vec::new();
    let mut section_titles: Vec<String> = Vec::new();

    // Rows can have individual heights (-tableView:heightForRowAtIndexPath:).
    let delegate = env.objc.borrow::<UITableViewHostObject>(this).table_delegate;
    let delegate_has_row_heights = delegate != nil && {
        let sel = env.objc.lookup_selector("tableView:heightForRowAtIndexPath:");
        match sel {
            Some(sel) => msg![env; delegate respondsToSelector:sel],
            None => false,
        }
    };
    let sel_header_title = env.objc.lookup_selector("tableView:titleForHeaderInSection:");

    for section in 0..num_sections {
        let title: id = match sel_header_title {
            Some(sel) => {
                let responds: bool = msg![env; data_source respondsToSelector:sel];
                if responds {
                    msg![env; data_source tableView:this titleForHeaderInSection:section]
                } else {
                    nil
                }
            }
            None => nil,
        };
        section_titles.push(if title == nil {
            String::new()
        } else {
            crate::frameworks::foundation::ns_string::to_rust_string(env, title).into_owned()
        });
        if grouped {
            y += 8.5;
        }
        if title != nil {
            let label: id = msg_class![env; UILabel new];
            () = msg![env; label setText:title];
            let font: id = msg_class![env; UIFont boldSystemFontOfSize:(17.0f32)];
            () = msg![env; label setFont:font];
            let clear: id = msg_class![env; UIColor clearColor];
            () = msg![env; label setBackgroundColor:clear];
            let frame = make_rect(inset + 10.0, y, (cell_width - 20.0).max(0.0), 28.0);
            () = msg![env; label setFrame:frame];
            () = msg![env; this addSubview:label];
            section_views.push(label);
            y += 28.0;
        }

        let row_count: NSInteger = msg![env; data_source
            tableView:this
            numberOfRowsInSection:section];
        for row in 0..row_count {
            let index_path: id = msg_class![env; NSIndexPath
                indexPathForRow:(row as NSUInteger)
                inSection:(section as NSUInteger)];
            retain(env, index_path);

            let cell: id = msg![env; data_source
                tableView:this
                cellForRowAtIndexPath:index_path];
            if cell == nil {
                release(env, index_path);
                continue;
            }
            retain(env, cell);

            let this_row_height: f32 = if delegate_has_row_heights {
                msg![env; delegate tableView:this heightForRowAtIndexPath:index_path]
            } else {
                row_height
            };
            let frame = CGRect {
                origin: CGPoint { x: inset, y },
                size: CGSize { width: cell_width, height: this_row_height },
            };
            () = msg![env; cell setFrame:frame];
            () = msg![env; this addSubview:cell];

            y += this_row_height;
            new_cells.push(cell);
            new_paths.push(index_path);
        }
    }
    if grouped {
        y += 12.0;
    }

    let footer = env.objc.borrow::<UITableViewHostObject>(this).table_footer_view;
    if footer != nil {
        let mut frame: CGRect = msg![env; footer frame];
        let h = frame.size.height.max(0.0);
        frame.origin = CGPoint { x: 0.0, y };
        frame.size.width = width;
        () = msg![env; footer setFrame:frame];
        () = msg![env; this addSubview:footer];
        y += h;
    }

    {
        let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
        host.cells = new_cells;
        host.cell_index_paths = new_paths;
        host.section_views = section_views;
        host.section_titles = section_titles;
    }

    // A reload throws the cell views away, so put the highlight back on
    // every row that is still selected.
    let selected = env.objc.borrow::<UITableViewHostObject>(this).selected_paths.clone();
    for path in selected {
        let cell: id = msg![env; this cellForRowAtIndexPath:path];
        if cell != nil {
            () = msg![env; cell setSelected:true animated:false];
        }
    }

    let content_size = CGSize { width, height: y };
    () = msg![env; this setContentSize:content_size];
}

- (())reloadSections:(id)_sections withRowAnimation:(i32)_animation {
    () = msg![env; this reloadData];
}
- (())reloadRowsAtIndexPaths:(id)_paths withRowAnimation:(i32)_animation {
    () = msg![env; this reloadData];
}
- (i32)numberOfSections {
    let data_source = env.objc.borrow::<UITableViewHostObject>(this).data_source;
    if data_source == nil { return 0; }
    let sel = match env.objc.lookup_selector("numberOfSectionsInTableView:") {
        Some(sel) => sel,
        None => return 1,
    };
    let responds: bool = msg![env; data_source respondsToSelector:sel];
    if !responds { return 1; }
    msg![env; data_source numberOfSectionsInTableView:this]
}
- (i32)numberOfRowsInSection:(i32)section {
    let data_source = env.objc.borrow::<UITableViewHostObject>(this).data_source;
    if data_source == nil { return 0; }
    msg![env; data_source tableView:this numberOfRowsInSection:section]
}
- (id)cellForRowAtIndexPath:(id)path {
    if path == nil { return nil; }
    let cell_paths = env.objc.borrow::<UITableViewHostObject>(this).cell_index_paths.clone();
    for (i, other) in cell_paths.into_iter().enumerate() {
        let equal: bool = msg![env; path isEqual:other];
        if equal {
            return env.objc.borrow::<UITableViewHostObject>(this).cells[i];
        }
    }
    nil
}
- (id)dequeueReusableCellWithIdentifier:(id)identifier {
        if identifier == nil {
            return nil;
        }
        let key = crate::frameworks::foundation::ns_string::to_rust_string(env, identifier)
            .into_owned();
        let found = {
            let host = env.objc.borrow::<UITableViewHostObject>(this);
            host.reuse_pool.iter().position(|(k, _)| *k == key)
        };
        let Some(index) = found else {
            return nil;
        };
        let cell = env
            .objc
            .borrow_mut::<UITableViewHostObject>(this)
            .reuse_pool
            .remove(index)
            .1;
        () = msg![env; cell prepareForReuse];
        // The pool's reference is handed to the caller, which returns the
        // cell from `tableView:cellForRowAtIndexPath:`; `-reloadData`
        // retains it there. Autoreleasing keeps it alive until then.
        () = msg![env; cell autorelease];
        cell
    }
- (id)indexPathForCell:(id)cell {
    if cell == nil { return nil; }
    let host = env.objc.borrow::<UITableViewHostObject>(this);
    for (i, &c) in host.cells.iter().enumerate() {
        if c == cell {
            return host.cell_index_paths[i];
        }
    }
    nil
}
- (id)indexPathForSelectedRow {
        let host = env.objc.borrow::<UITableViewHostObject>(this);
        host.selected_paths.first().copied().unwrap_or(nil)
    }
    - (id)indexPathsForSelectedRows {
        let selected = env.objc.borrow::<UITableViewHostObject>(this).selected_paths.clone();
        if selected.is_empty() {
            return nil;
        }
        let array: id = msg_class![env; NSMutableArray array];
        for path in selected {
            () = msg![env; array addObject:path];
        }
        array
    }
- (())selectRowAtIndexPath:(id)path animated:(bool)_animated scrollPosition:(i32)scroll_position {
        let multiple = env.objc.borrow::<UITableViewHostObject>(this).allows_multiple_selection;
        if path == nil {
            let old = std::mem::take(
                &mut env.objc.borrow_mut::<UITableViewHostObject>(this).selected_paths,
            );
            for other in old {
                let cell: id = msg![env; this cellForRowAtIndexPath:other];
                if cell != nil {
                    () = msg![env; cell setSelected:false animated:false];
                }
                release(env, other);
            }
            return;
        }
        let already = {
            let selected = env.objc.borrow::<UITableViewHostObject>(this).selected_paths.clone();
            let mut found = false;
            for other in selected {
                let equal: bool = msg![env; other isEqual:path];
                if equal {
                    found = true;
                    break;
                }
            }
            found
        };
        if already {
            // Already selected: nothing to add, but the caller may still
            // have asked for the row to be scrolled into view.
            if scroll_position != 0 {
                () = msg![env; this scrollToRowAtIndexPath:path atScrollPosition:scroll_position animated:false];
            }
            return;
        }
        retain(env, path);
        if !multiple {
            let old = std::mem::take(
                &mut env.objc.borrow_mut::<UITableViewHostObject>(this).selected_paths,
            );
            for other in old {
                let cell: id = msg![env; this cellForRowAtIndexPath:other];
                if cell != nil {
                    () = msg![env; cell setSelected:false animated:false];
                }
                release(env, other);
            }
        }
        env.objc
            .borrow_mut::<UITableViewHostObject>(this)
            .selected_paths
            .push(path);
        let cell: id = msg![env; this cellForRowAtIndexPath:path];
        if cell != nil {
            () = msg![env; cell setSelected:true animated:false];
        }
        if scroll_position != 0 {
            () = msg![env; this scrollToRowAtIndexPath:path atScrollPosition:scroll_position animated:false];
        }
    }
- (())deselectRowAtIndexPath:(id)path animated:(bool)_animated {
        if path == nil {
            return;
        }
        let selected = env.objc.borrow::<UITableViewHostObject>(this).selected_paths.clone();
        let mut index = None;
        for (i, other) in selected.iter().enumerate() {
            let other_path: id = *other;
            let equal: bool = msg![env; other_path isEqual:path];
            if equal {
                index = Some(i);
                break;
            }
        }
        let Some(index) = index else {
            return;
        };
        let old = env
            .objc
            .borrow_mut::<UITableViewHostObject>(this)
            .selected_paths
            .remove(index);
        let cell: id = msg![env; this cellForRowAtIndexPath:old];
        if cell != nil {
            () = msg![env; cell setSelected:false animated:false];
        }
        release(env, old);
    }
- (id)indexPathForRowAtPoint:(CGPoint)point {
    let (cells, paths) = {
        let host = env.objc.borrow::<UITableViewHostObject>(this);
        (host.cells.clone(), host.cell_index_paths.clone())
    };
    for (i, cell) in cells.into_iter().enumerate() {
        let frame: CGRect = msg![env; cell frame];
        if point.x >= frame.origin.x
            && point.x < frame.origin.x + frame.size.width
            && point.y >= frame.origin.y
            && point.y < frame.origin.y + frame.size.height
        {
            return paths[i];
        }
    }
    nil
}
- (())scrollToRowAtIndexPath:(id)path atScrollPosition:(i32)position animated:(bool)_animated {
        let cell: id = msg![env; this cellForRowAtIndexPath:path];
        if cell == nil {
            return;
        }
        let frame: CGRect = msg![env; cell frame];
        let bounds: CGRect = msg![env; this bounds];
        let content: CGSize = msg![env; this contentSize];
        let view_h = bounds.size.height;
        let current = bounds.origin.y;
        // UITableViewScrollPosition: 0 = none, 1 = top, 2 = middle,
        // 3 = bottom.
        let y = scroll_offset_for(
            position,
            frame.origin.y,
            frame.size.height,
            view_h,
            content.height,
            current,
        );
        let offset = CGPoint {
            x: bounds.origin.x,
            y,
        };
        () = msg![env; this setContentOffset:offset];
    }
- (())setEditing:(bool)_editing animated:(bool)_animated {}
- (bool)isEditing { false }
- (())setAllowsSelection:(bool)allows {
    env.objc.borrow_mut::<UITableViewHostObject>(this).allows_selection = allows;
}
- (bool)allowsSelection {
    env.objc.borrow::<UITableViewHostObject>(this).allows_selection
}
- (())setAllowsMultipleSelection:(bool)allows {
        env.objc
            .borrow_mut::<UITableViewHostObject>(this)
            .allows_multiple_selection = allows;
        if !allows {
            // Dropping back to single selection keeps the first row.
            let extra = {
                let host = env.objc.borrow_mut::<UITableViewHostObject>(this);
                if host.selected_paths.len() > 1 {
                    host.selected_paths.split_off(1)
                } else {
                    Vec::new()
                }
            };
            for path in extra {
                let cell: id = msg![env; this cellForRowAtIndexPath:path];
                if cell != nil {
                    () = msg![env; cell setSelected:false animated:false];
                }
                release(env, path);
            }
        }
    }
    - (bool)allowsMultipleSelection {
        env.objc
            .borrow::<UITableViewHostObject>(this)
            .allows_multiple_selection
    }
- (())touchesBegan:(id)touches withEvent:(id)event {
    let touch: id = msg![env; touches anyObject];
    let point: CGPoint = msg![env; touch locationInView:this];
    env.objc.borrow_mut::<UITableViewHostObject>(this).selection_touch = Some((touch, point));
    msg_super![env; this touchesBegan:touches withEvent:event]
}
- (())touchesEnded:(id)touches withEvent:(id)event {
    // Read the drag state before the scroll view clears it.
    let dragging: bool = msg![env; this isDragging];
    let tracked = env
        .objc
        .borrow_mut::<UITableViewHostObject>(this)
        .selection_touch
        .take();
    () = msg_super![env; this touchesEnded:touches withEvent:event];
    if let Some((touch, start)) = tracked {
        let ended: id = msg![env; touches anyObject];
        if touch == ended && !dragging {
            let end: CGPoint = msg![env; ended locationInView:this];
            select_tapped_row(env, this, start, end);
        }
    }
}
- (())touchesCancelled:(id)touches withEvent:(id)event {
    env.objc.borrow_mut::<UITableViewHostObject>(this).selection_touch = None;
    msg_super![env; this touchesCancelled:touches withEvent:event]
}
- (())setRowHeight:(f32)height {
    env.objc.borrow_mut::<UITableViewHostObject>(this).row_height = height;
}
- (f32)rowHeight {
    env.objc.borrow::<UITableViewHostObject>(this).row_height
}
- (())setSectionHeaderHeight:(f32)height {
    env.objc.borrow_mut::<UITableViewHostObject>(this).section_header_height = height;
}
- (())setSectionFooterHeight:(f32)height {
    env.objc.borrow_mut::<UITableViewHostObject>(this).section_footer_height = height;
}
- (id)tableHeaderView {
    env.objc.borrow::<UITableViewHostObject>(this).table_header_view
}
- (())setTableHeaderView:(id)view {
    if view != nil { retain(env, view); }
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<UITableViewHostObject>(this).table_header_view,
        view,
    );
    if old != nil { release(env, old); }
}
- (id)tableFooterView {
    env.objc.borrow::<UITableViewHostObject>(this).table_footer_view
}
- (())setTableFooterView:(id)view {
    if view != nil { retain(env, view); }
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<UITableViewHostObject>(this).table_footer_view,
        view,
    );
    if old != nil { release(env, old); }
}
- (())setSeparatorStyle:(i32)style {
    env.objc.borrow_mut::<UITableViewHostObject>(this).separator_style = style as NSInteger;
}
- (i32)separatorStyle {
    env.objc.borrow::<UITableViewHostObject>(this).separator_style
}
- (())setSeparatorColor:(id)color {
    if color != nil { retain(env, color); }
    let old = std::mem::replace(
        &mut env.objc.borrow_mut::<UITableViewHostObject>(this).separator_color,
        color,
    );
    if old != nil { release(env, old); }
}
- (id)separatorColor {
    env.objc.borrow::<UITableViewHostObject>(this).separator_color
}
- (())registerClass:(id)_class forCellReuseIdentifier:(id)_identifier {}
- (())registerNib:(id)_nib forCellReuseIdentifier:(id)_identifier {}
- (())beginUpdates {}
- (())endUpdates {}
- (())insertRowsAtIndexPaths:(id)_paths withRowAnimation:(i32)_animation {
    () = msg![env; this reloadData];
}
- (())deleteRowsAtIndexPaths:(id)_paths withRowAnimation:(i32)_animation {
    () = msg![env; this reloadData];
}

@end

@implementation UITableViewController: UIViewController

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<UITableViewControllerHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithStyle:(i32)style {
    let this: id = msg_super![env; this init];
    env.objc.borrow_mut::<UITableViewControllerHostObject>(this).style = style;
    this
}

// Честный iOS-подход: переопределяем создание вьюхи по умолчанию.
// Если NIB'а нет, мы принудительно создаем UITableView, а не UIView.
- (())loadView {
    let frame = <CGRect as Default>::default();
    let style = env.objc.borrow::<UITableViewControllerHostObject>(this).style;
    let table_view: id = msg_class![env; UITableView alloc];
    let table_view: id = msg![env; table_view initWithFrame:frame style:style];

    // UITableViewController owns its table view and, per UIKit, makes itself
    // both the data source and the delegate. Without this the table never asks
    // the controller for any rows and renders empty (a blank white screen).
    () = msg![env; table_view setDataSource:this];
    () = msg![env; table_view setDelegate:this];

    // Устанавливаем таблицу как главную view этого контроллера
    () = msg![env; this setView:table_view];
    release(env, table_view);
}

// Теперь tableView ссылается на реальный UITableView
- (id)tableView {
    msg![env; this view]
}

- (())setTableView:(id)view {
    () = msg![env; this setView:view];
}

- (bool)clearsSelectionOnViewWillAppear { true }
- (())setClearsSelectionOnViewWillAppear:(bool)_clears {}

@end

};


#[cfg(test)]
mod tests {
    use super::{is_tap, scroll_offset_for};
    use crate::frameworks::core_graphics::CGPoint;

    fn point(x: f32, y: f32) -> CGPoint {
        CGPoint { x, y }
    }

    #[test]
    fn a_touch_that_barely_moves_is_a_tap() {
        assert!(is_tap(point(10.0, 10.0), point(10.0, 10.0)));
        assert!(is_tap(point(10.0, 10.0), point(13.0, 14.0)));
    }

    #[test]
    fn a_touch_that_moves_ten_points_is_a_drag() {
        assert!(!is_tap(point(10.0, 10.0), point(10.0, 20.0)));
        assert!(!is_tap(point(10.0, 10.0), point(16.0, 18.0)));
    }

    #[test]
    fn scroll_position_top_puts_the_row_at_the_top() {
        assert_eq!(scroll_offset_for(1, 300.0, 44.0, 480.0, 2000.0, 0.0), 300.0);
    }

    #[test]
    fn scroll_position_middle_centres_the_row() {
        // 480pt view, 44pt row: the row's centre lands on the view's centre.
        assert_eq!(scroll_offset_for(2, 300.0, 44.0, 480.0, 2000.0, 0.0), 82.0);
    }

    #[test]
    fn scroll_position_bottom_puts_the_row_at_the_bottom() {
        assert_eq!(scroll_offset_for(3, 1000.0, 44.0, 480.0, 2000.0, 0.0), 564.0);
    }

    #[test]
    fn scroll_position_none_keeps_the_current_offset() {
        assert_eq!(scroll_offset_for(0, 300.0, 44.0, 480.0, 2000.0, 120.0), 120.0);
    }

    #[test]
    fn scroll_offsets_are_clamped_to_the_scrollable_range() {
        // A row near the top cannot scroll above the content start.
        assert_eq!(scroll_offset_for(1, -50.0, 44.0, 480.0, 2000.0, 0.0), 0.0);
        // A row near the bottom cannot scroll past the last screenful.
        assert_eq!(scroll_offset_for(1, 1900.0, 44.0, 480.0, 2000.0, 0.0), 1520.0);
        // Content shorter than the view has no scrollable range at all.
        assert_eq!(scroll_offset_for(1, 100.0, 44.0, 480.0, 200.0, 0.0), 0.0);
    }
}
/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! A small, host-side App Store-style browser for the Internet Archive's
//! `ipaarchive` collection. Network and file I/O run on worker threads; the
//! emulated UIKit views are only touched from the picker/run-loop thread.

use super::{new_label, new_view, rect, ui_color, AppPickerDelegateHostObject};
use crate::frameworks::core_graphics::{CGFloat, CGPoint, CGRect, CGSize};
use crate::frameworks::foundation::ns_run_loop::run_run_loop_single_iteration;
use crate::frameworks::foundation::{ns_string, NSInteger};
use crate::frameworks::uikit::ui_font::{UITextAlignmentCenter, UITextAlignmentLeft};
use crate::frameworks::uikit::ui_view::ui_control::ui_button::UIButtonTypeCustom;
use crate::frameworks::uikit::ui_view::ui_control::{
    UIControlEventTouchUpInside, UIControlStateNormal,
};
use crate::objc::{id, msg, msg_class, nil, release};
use crate::Environment;
use serde_json::Value;
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

const COLLECTION_ID: &str = "ipaarchive";
const CATALOG_ROWS: usize = 25;
const ROW_HEIGHT: CGFloat = 62.0;
const MAX_JSON_RESPONSE_SIZE: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug)]
struct CatalogItem {
    identifier: String,
    title: String,
    version: Option<String>,
    date: Option<String>,
    creator: Option<String>,
}

#[derive(Debug)]
struct CatalogPage {
    items: Vec<CatalogItem>,
    num_found: u64,
    page: usize,
}

#[derive(Clone, Debug)]
struct ArchiveFile {
    name: String,
    size: Option<u64>,
    version: Option<String>,
}

#[derive(Debug)]
struct ItemDetails {
    identifier: String,
    title: String,
    files: Vec<ArchiveFile>,
}

#[derive(Debug)]
struct DownloadOutcome {
    path: PathBuf,
    already_present: bool,
}

enum DownloadEvent {
    Progress {
        downloaded: u64,
        total: Option<u64>,
    },
    Finished {
        result: Result<DownloadOutcome, String>,
    },
}

enum StoreMessage {
    Catalog {
        generation: u64,
        result: Result<CatalogPage, String>,
    },
    Details {
        generation: u64,
        result: Result<ItemDetails, String>,
    },
    Download {
        generation: u64,
        event: DownloadEvent,
    },
}

struct ActiveDownload {
    generation: u64,
    file_index: usize,
    identifier: String,
    key: String,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct StoreActions {
    close: bool,
    search: bool,
    previous: bool,
    next: bool,
    back: bool,
    retry: bool,
    cancel: bool,
    item: Option<NSInteger>,
    download: Option<NSInteger>,
}

fn take_actions(env: &mut Environment, delegate: id) -> StoreActions {
    let host = env
        .objc
        .borrow_mut::<AppPickerDelegateHostObject>(delegate);
    StoreActions {
        close: std::mem::take(&mut host.archive_store_close),
        search: std::mem::take(&mut host.archive_store_search),
        previous: std::mem::take(&mut host.archive_store_previous),
        next: std::mem::take(&mut host.archive_store_next),
        back: std::mem::take(&mut host.archive_store_back),
        retry: std::mem::take(&mut host.archive_store_retry),
        cancel: std::mem::take(&mut host.archive_store_cancel),
        item: std::mem::take(&mut host.archive_store_item),
        download: std::mem::take(&mut host.archive_store_download),
    }
}

struct StoreViews {
    root: id,
    delegate: id,
    nav_title: id,
    back_button: id,
    search_field: id,
    search_button: id,
    status_label: id,
    scroll_view: id,
    previous_button: id,
    next_button: id,
    page_label: id,
    retry_button: id,
    progress_label: id,
    progress_fill: id,
    progress_width: CGFloat,
    cancel_button: id,
    rows: Vec<id>,
    download_buttons: Vec<(usize, id)>,
}

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Catalog,
    Details,
}

struct CatalogState {
    query: String,
    page: usize,
    total: u64,
    loading: bool,
    generation: u64,
    current: Option<CatalogPage>,
    error: Option<String>,
}

impl CatalogState {
    fn new() -> Self {
        Self {
            query: String::new(),
            page: 1,
            total: 0,
            loading: false,
            generation: 0,
            current: None,
            error: None,
        }
    }

    fn load(
        &mut self,
        env: &mut Environment,
        views: &mut StoreViews,
        sender: &Sender<StoreMessage>,
        page: usize,
    ) {
        self.page = page.max(1);
        self.loading = true;
        self.error = None;
        self.generation += 1;
        clear_rows(env, views);
        () = msg![env; (views.retry_button) setHidden:true];
        let status = if self.query.is_empty() {
            "Loading IPA archives…".to_string()
        } else {
            format!("Searching for “{}”…", self.query)
        };
        set_label(env, views.status_label, &status);
        update_catalog_footer(env, views, self);
        request_catalog(
            sender,
            self.generation,
            self.query.clone(),
            self.page,
            CATALOG_ROWS,
        );
    }

    fn show(&self, env: &mut Environment, views: &mut StoreViews) {
        show_catalog_chrome(env, views);
        clear_rows(env, views);
        if let Some(page) = &self.current {
            rebuild_catalog_rows(env, views, &page.items);
            let status = if self.query.is_empty() {
                format!(
                    "{} archives • title A–Z • tap an entry to view IPA files",
                    self.total
                )
            } else {
                format!("{} results for “{}”", self.total, self.query)
            };
            set_label(env, views.status_label, &status);
            () = msg![env; (views.retry_button) setHidden:true];
        } else if let Some(error) = &self.error {
            set_label(env, views.status_label, error);
            () = msg![env; (views.retry_button) setHidden:false];
        } else if self.loading {
            set_label(env, views.status_label, "Loading IPA archives…");
        }
        update_catalog_footer(env, views, self);
    }
}

struct DetailsState {
    item: Option<CatalogItem>,
    details: Option<ItemDetails>,
    generation: u64,
}

impl DetailsState {
    fn new() -> Self {
        Self {
            item: None,
            details: None,
            generation: 0,
        }
    }

    fn load(
        &mut self,
        env: &mut Environment,
        views: &mut StoreViews,
        sender: &Sender<StoreMessage>,
        item: CatalogItem,
    ) {
        self.generation += 1;
        self.details = None;
        let identifier = item.identifier.clone();
        let title = item.title.clone();
        self.item = Some(item);
        show_details_chrome(env, views, &title);
        clear_rows(env, views);
        set_label(env, views.status_label, "Looking for downloadable IPA files…");
        () = msg![env; (views.retry_button) setHidden:true];
        request_details(sender, self.generation, identifier);
    }

    fn show_error(&mut self, env: &mut Environment, views: &mut StoreViews, error: String) {
        self.details = None;
        clear_rows(env, views);
        set_label(env, views.status_label, &error);
        () = msg![env; (views.retry_button) setHidden:false];
    }
}

/// Open the store as a modal picker screen. The return value tells the home
/// screen whether it should re-enumerate the apps directory.
pub(super) fn run_archive_store(
    env: &mut Environment,
    run_loop: id,
    delegate: id,
    parent: id,
    app_frame: CGRect,
    apps_dir: &Path,
) -> bool {
    let mut views = build_store_views(env, delegate, parent, app_frame);
    let (sender, receiver) = mpsc::channel();
    let mut screen = Screen::Catalog;
    let mut catalog = CatalogState::new();
    let mut details = DetailsState::new();
    let mut active_download: Option<ActiveDownload> = None;
    let mut download_generation = 0u64;
    let mut downloaded_keys = HashSet::new();
    let mut downloaded_any = false;
    let mut close_after_download = false;
    let mut finished = false;

    catalog.load(env, &mut views, &sender, 1);

    while !finished {
        run_run_loop_single_iteration(env, run_loop);
        drain_messages(
            env,
            &receiver,
            &mut views,
            &mut screen,
            &mut catalog,
            &mut details,
            &mut active_download,
            &mut downloaded_keys,
            &mut downloaded_any,
            &mut close_after_download,
            &mut finished,
        );

        if finished {
            break;
        }
        let actions = take_actions(env, delegate);

        if actions.close {
            if let Some(active) = &active_download {
                active.cancel.store(true, Ordering::Release);
                close_after_download = true;
                set_label(env, views.progress_label, "Cancelling download…");
                set_button_enabled(env, views.cancel_button, false);
            } else {
                finished = true;
            }
        }
        if close_after_download || finished {
            continue;
        }

        if actions.cancel {
            if let Some(active) = &active_download {
                active.cancel.store(true, Ordering::Release);
                set_label(env, views.progress_label, "Cancelling download…");
                set_button_enabled(env, views.cancel_button, false);
            }
        }

        if actions.search {
            let text: id = msg![env; (views.search_field) text];
            let query = if text == nil {
                String::new()
            } else {
                ns_string::to_rust_string(env, text).into_owned()
            };
            let _: bool = msg![env; (views.search_field) resignFirstResponder];
            catalog.query = query.trim().to_string();
            screen = Screen::Catalog;
            details.generation += 1;
            catalog.load(env, &mut views, &sender, 1);
        } else if actions.previous && screen == Screen::Catalog {
            if !catalog.loading && catalog.page > 1 {
                catalog.load(env, &mut views, &sender, catalog.page - 1);
            }
        } else if actions.next && screen == Screen::Catalog {
            if !catalog.loading && catalog_page_count(&catalog) > catalog.page {
                catalog.load(env, &mut views, &sender, catalog.page + 1);
            }
        } else if actions.back && screen == Screen::Details {
            let _: bool = msg![env; (views.search_field) resignFirstResponder];
            details.generation += 1;
            screen = Screen::Catalog;
            catalog.show(env, &mut views);
        } else if actions.retry {
            match screen {
                Screen::Catalog => {
                    let page = catalog.page;
                    catalog.load(env, &mut views, &sender, page);
                }
                Screen::Details => {
                    if let Some(item) = details.item.clone() {
                        details.load(env, &mut views, &sender, item);
                    }
                }
            }
        } else if let Some(index) = actions.item {
            if screen == Screen::Catalog && !catalog.loading {
                let item = catalog
                    .current
                    .as_ref()
                    .and_then(|page| page.items.get(index.max(0) as usize))
                    .cloned();
                if let Some(item) = item {
                    screen = Screen::Details;
                    details.load(env, &mut views, &sender, item);
                }
            }
        } else if let Some(index) = actions.download {
            if screen == Screen::Details {
                if active_download.is_some() {
                    set_label(
                        env,
                        views.progress_label,
                        "Please wait for the current download to finish.",
                    );
                } else if let Some(item_details) = &details.details {
                    let file = item_details.files.get(index.max(0) as usize).cloned();
                    if let Some(file) = file {
                        download_generation += 1;
                        let key = download_key(&item_details.identifier, &file.name);
                        let cancel = Arc::new(AtomicBool::new(false));
                        active_download = Some(ActiveDownload {
                            generation: download_generation,
                            file_index: index.max(0) as usize,
                            identifier: item_details.identifier.clone(),
                            key,
                            cancel: cancel.clone(),
                        });
                        () = msg![env; (views.cancel_button) setHidden:false];
                        set_button_enabled(env, views.cancel_button, true);
                        set_download_progress(env, &views, 0, file.size);
                        set_label(
                            env,
                            views.progress_label,
                            &format!("Starting {}…", display_file_name(&file.name)),
                        );
                        request_download(
                            &sender,
                            download_generation,
                            apps_dir.to_path_buf(),
                            item_details.identifier.clone(),
                            file,
                            cancel,
                        );
                    }
                }
            }
        }
    }

    let _: bool = msg![env; (views.search_field) resignFirstResponder];
    () = msg![env; (views.root) removeFromSuperview];
    let host = env
        .objc
        .borrow_mut::<AppPickerDelegateHostObject>(delegate);
    host.archive_store_close = false;
    host.archive_store_search = false;
    host.archive_store_previous = false;
    host.archive_store_next = false;
    host.archive_store_back = false;
    host.archive_store_retry = false;
    host.archive_store_cancel = false;
    host.archive_store_item = None;
    host.archive_store_download = None;
    downloaded_any
}

#[allow(clippy::too_many_arguments)]
fn drain_messages(
    env: &mut Environment,
    receiver: &Receiver<StoreMessage>,
    views: &mut StoreViews,
    screen: &mut Screen,
    catalog: &mut CatalogState,
    details: &mut DetailsState,
    active_download: &mut Option<ActiveDownload>,
    downloaded_keys: &mut HashSet<String>,
    downloaded_any: &mut bool,
    close_after_download: &mut bool,
    finished: &mut bool,
) {
    while let Ok(message) = receiver.try_recv() {
        match message {
            StoreMessage::Catalog {
                generation,
                result,
            } => {
                if generation != catalog.generation {
                    continue;
                }
                catalog.loading = false;
                match result {
                    Ok(page) => {
                        catalog.total = page.num_found;
                        catalog.page = page.page;
                        catalog.current = Some(page);
                        catalog.error = None;
                        if *screen == Screen::Catalog {
                            catalog.show(env, views);
                        }
                    }
                    Err(error) => {
                        catalog.current = None;
                        catalog.error = Some(error);
                        if *screen == Screen::Catalog {
                            catalog.show(env, views);
                        }
                    }
                }
            }
            StoreMessage::Details {
                generation,
                result,
            } => {
                if generation != details.generation {
                    continue;
                }
                match result {
                    Ok(item_details) => {
                        set_label(
                            env,
                            views.nav_title,
                            &item_details.title,
                        );
                        let status = if item_details.files.is_empty() {
                            format!(
                                "No direct .ipa files in {}. Try another archive entry.",
                                item_details.identifier
                            )
                        } else {
                            format!(
                                "{} IPA file{} • choose GET to download",
                                item_details.files.len(),
                                if item_details.files.len() == 1 { "" } else { "s" }
                            )
                        };
                        set_label(env, views.status_label, &status);
                        rebuild_detail_rows(env, views, &item_details, downloaded_keys);
                        details.details = Some(item_details);
                        () = msg![env; (views.retry_button) setHidden:true];
                    }
                    Err(error) => {
                        details.show_error(env, views, error);
                    }
                }
            }
            StoreMessage::Download { generation, event } => {
                if active_download
                    .as_ref()
                    .is_none_or(|active| active.generation != generation)
                {
                    continue;
                }
                match event {
                    DownloadEvent::Progress { downloaded, total } => {
                        set_download_progress(env, views, downloaded, total);
                    }
                    DownloadEvent::Finished { result } => {
                        let active = active_download.take().unwrap();
                        () = msg![env; (views.cancel_button) setHidden:true];
                        match result {
                            Ok(outcome) => {
                                *downloaded_any = true;
                                downloaded_keys.insert(active.key);
                                let file_name = outcome
                                    .path
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy();
                                let text = if outcome.already_present {
                                    format!("Already saved: {file_name}")
                                } else {
                                    format!("Saved to touchHLE_apps: {file_name}")
                                };
                                set_label(env, views.progress_label, &text);
                                set_progress_fraction(env, views, 1.0);
                                let showing_downloaded_item = details
                                    .details
                                    .as_ref()
                                    .is_some_and(|item| item.identifier == active.identifier);
                                if showing_downloaded_item {
                                    for (index, button) in &views.download_buttons {
                                        if *index == active.file_index {
                                            set_button_title(env, *button, "Saved");
                                            set_button_enabled(env, *button, false);
                                        }
                                    }
                                }
                            }
                            Err(error) => {
                                set_progress_fraction(env, views, 0.0);
                                if error == "Download cancelled." {
                                    set_label(env, views.progress_label, "Download cancelled.");
                                } else {
                                    set_label(env, views.progress_label, "Download failed.");
                                    set_label(env, views.status_label, &error);
                                }
                            }
                        }
                        if *close_after_download {
                            *close_after_download = false;
                            *finished = true;
                        }
                    }
                }
            }
        }
    }
}

fn build_store_views(
    env: &mut Environment,
    delegate: id,
    parent: id,
    app_frame: CGRect,
) -> StoreViews {
    let width = app_frame.size.width;
    let status_offset = app_frame.origin.y;
    let full_height = app_frame.size.height + status_offset;
    let background = ui_color(env, 0.90, 0.92, 0.95, 1.0);
    let root = new_view(
        env,
        rect(0.0, -status_offset, width, full_height),
        background,
    );
    () = msg![env; parent addSubview:root];

    let nav_height = 44.0 + status_offset;
    let nav_color = ui_color(env, 0.12, 0.29, 0.48, 1.0);
    let nav = new_view(env, rect(0.0, 0.0, width, nav_height), nav_color);
    () = msg![env; root addSubview:nav];
    let white: id = msg_class![env; UIColor whiteColor];
    let clear: id = msg_class![env; UIColor clearColor];
    let nav_title = new_label(
        env,
        rect(66.0, status_offset, width - 132.0, 44.0),
        "IPA Archive",
        16.0,
        true,
        white,
    );
    () = msg![env; nav_title setTextAlignment:UITextAlignmentCenter];
    () = msg![env; root addSubview:nav_title];

    let button_blue = ui_color(env, 0.0, 0.40, 0.82, 1.0);
    let back_button = make_button(
        env,
        delegate,
        root,
        rect(6.0, status_offset + 7.0, 58.0, 30.0),
        "< Back",
        "archiveStoreBack",
        0,
        clear,
        white,
    );
    () = msg![env; back_button setHidden:true];
    let _done_button = make_button(
        env,
        delegate,
        root,
        rect(width - 64.0, status_offset + 7.0, 58.0, 30.0),
        "Done",
        "archiveStoreClose",
        0,
        clear,
        white,
    );

    let search_y = nav_height + 7.0;
    let search_field: id = msg_class![env; UITextField alloc];
    let search_field: id = msg![env; search_field initWithFrame:(rect(
        8.0,
        search_y,
        width - 88.0,
        32.0
    ))];
    let field_font: id = msg_class![env; UIFont systemFontOfSize:(13.0 as CGFloat)];
    () = msg![env; search_field setFont:field_font];
    () = msg![env; search_field setDelegate:delegate];
    () = msg![env; search_field setReturnKeyType:(6 as NSInteger)]; // Search
    () = msg![env; search_field setAutocapitalizationType:(0 as NSInteger)];
    () = msg![env; search_field setAutocorrectionType:(1 as NSInteger)];
    let placeholder = ns_string::from_rust_string(env, "Search titles and identifiers".to_string());
    () = msg![env; search_field setPlaceholder:placeholder];
    release(env, placeholder);
    let field_layer: id = msg![env; search_field layer];
    let field_border = ui_color(env, 0.65, 0.68, 0.72, 1.0);
    let field_border_cg: id = msg![env; field_border CGColor];
    () = msg![env; field_layer setBorderColor:field_border_cg];
    () = msg![env; field_layer setBorderWidth:(1.0 as CGFloat)];
    () = msg![env; field_layer setCornerRadius:(5.0 as CGFloat)];
    () = msg![env; root addSubview:search_field];
    let search_button = make_button(
        env,
        delegate,
        root,
        rect(width - 76.0, search_y, 68.0, 32.0),
        "Search",
        "archiveStoreSearch",
        0,
        button_blue,
        white,
    );

    let gray = ui_color(env, 0.30, 0.33, 0.38, 1.0);
    let status_label = new_label(
        env,
        rect(10.0, search_y + 36.0, width - 20.0, 30.0),
        "",
        11.0,
        false,
        gray,
    );
    () = msg![env; status_label setNumberOfLines:2];
    () = msg![env; status_label setTextAlignment:UITextAlignmentLeft];
    () = msg![env; root addSubview:status_label];

    let page_y = full_height - 64.0;
    let scroll_y = search_y + 69.0;
    let scroll_height = (page_y - scroll_y - 3.0).max(40.0);
    let scroll_view: id = msg_class![env; UIScrollView alloc];
    let scroll_view: id = msg![env; scroll_view initWithFrame:(rect(
        0.0,
        scroll_y,
        width,
        scroll_height
    ))];
    () = msg![env; scroll_view setBackgroundColor:white];
    () = msg![env; scroll_view setClipsToBounds:true];
    () = msg![env; scroll_view setShowsHorizontalScrollIndicator:false];
    () = msg![env; root addSubview:scroll_view];

    let previous_button = make_button(
        env,
        delegate,
        root,
        rect(8.0, page_y, 62.0, 30.0),
        "< Prev",
        "archiveStorePrevious",
        0,
        button_blue,
        white,
    );
    let next_button = make_button(
        env,
        delegate,
        root,
        rect(width - 70.0, page_y, 62.0, 30.0),
        "Next >",
        "archiveStoreNext",
        0,
        button_blue,
        white,
    );
    let page_label = new_label(
        env,
        rect(74.0, page_y, width - 148.0, 30.0),
        "Page 1",
        12.0,
        false,
        gray,
    );
    () = msg![env; page_label setTextAlignment:UITextAlignmentCenter];
    () = msg![env; root addSubview:page_label];

    let retry_button = make_button(
        env,
        delegate,
        root,
        rect(
            (width - 120.0) / 2.0,
            scroll_y + (scroll_height - 34.0) / 2.0,
            120.0,
            34.0,
        ),
        "Retry",
        "archiveStoreRetry",
        0,
        button_blue,
        white,
    );
    () = msg![env; retry_button setHidden:true];

    let progress_label = new_label(
        env,
        rect(8.0, full_height - 30.0, width - 82.0, 17.0),
        "Downloads are saved in touchHLE_apps",
        10.0,
        false,
        gray,
    );
    () = msg![env; root addSubview:progress_label];
    let cancel_red = ui_color(env, 0.78, 0.18, 0.20, 1.0);
    let cancel_button = make_button(
        env,
        delegate,
        root,
        rect(width - 70.0, full_height - 32.0, 62.0, 22.0),
        "Cancel",
        "archiveStoreCancel",
        0,
        cancel_red,
        white,
    );
    () = msg![env; cancel_button setHidden:true];

    let progress_width = width - 16.0;
    let track_color = ui_color(env, 0.72, 0.75, 0.79, 1.0);
    let track = new_view(
        env,
        rect(8.0, full_height - 10.0, progress_width, 4.0),
        track_color,
    );
    () = msg![env; root addSubview:track];
    let progress_fill = new_view(
        env,
        rect(8.0, full_height - 10.0, 0.0, 4.0),
        button_blue,
    );
    () = msg![env; root addSubview:progress_fill];

    StoreViews {
        root,
        delegate,
        nav_title,
        back_button,
        search_field,
        search_button,
        status_label,
        scroll_view,
        previous_button,
        next_button,
        page_label,
        retry_button,
        progress_label,
        progress_fill,
        progress_width,
        cancel_button,
        rows: Vec::new(),
        download_buttons: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn make_button(
    env: &mut Environment,
    delegate: id,
    parent: id,
    frame: CGRect,
    title: &str,
    action: &str,
    tag: NSInteger,
    background: id,
    title_color: id,
) -> id {
    let button: id = msg_class![env; UIButton buttonWithType:UIButtonTypeCustom];
    () = msg![env; button setFrame:frame];
    () = msg![env; button setBackgroundColor:background];
    let title = ns_string::from_rust_string(env, title.to_string());
    () = msg![env; button setTitle:title forState:UIControlStateNormal];
    release(env, title);
    () = msg![env; button setTitleColor:title_color forState:UIControlStateNormal];
    () = msg![env; button setTag:tag];
    () = msg![env; button layoutSubviews];
    let label: id = msg![env; button titleLabel];
    let font: id = msg_class![env; UIFont boldSystemFontOfSize:(12.0 as CGFloat)];
    () = msg![env; label setFont:font];
    let layer: id = msg![env; button layer];
    () = msg![env; layer setCornerRadius:(5.0 as CGFloat)];
    let selector = env.objc.lookup_selector(action).unwrap();
    () = msg![env; button addTarget:delegate
                              action:selector
                    forControlEvents:UIControlEventTouchUpInside];
    () = msg![env; parent addSubview:button];
    button
}

fn set_label(env: &mut Environment, label: id, text: &str) {
    let text = ns_string::from_rust_string(env, text.to_string());
    () = msg![env; label setText:text];
    release(env, text);
}

fn set_button_title(env: &mut Environment, button: id, title: &str) {
    let title = ns_string::from_rust_string(env, title.to_string());
    () = msg![env; button setTitle:title forState:UIControlStateNormal];
    release(env, title);
}

fn set_button_enabled(env: &mut Environment, button: id, enabled: bool) {
    () = msg![env; button setEnabled:enabled];
    let alpha: CGFloat = if enabled { 1.0 } else { 0.42 };
    () = msg![env; button setAlpha:alpha];
}

fn show_catalog_chrome(env: &mut Environment, views: &StoreViews) {
    set_label(env, views.nav_title, "IPA Archive");
    () = msg![env; (views.back_button) setHidden:true];
    () = msg![env; (views.search_field) setHidden:false];
    () = msg![env; (views.search_button) setHidden:false];
    () = msg![env; (views.previous_button) setHidden:false];
    () = msg![env; (views.next_button) setHidden:false];
    () = msg![env; (views.page_label) setHidden:false];
}

fn show_details_chrome(env: &mut Environment, views: &StoreViews, title: &str) {
    set_label(env, views.nav_title, title);
    () = msg![env; (views.back_button) setHidden:false];
    () = msg![env; (views.search_field) setHidden:true];
    () = msg![env; (views.search_button) setHidden:true];
    () = msg![env; (views.previous_button) setHidden:true];
    () = msg![env; (views.next_button) setHidden:true];
    () = msg![env; (views.page_label) setHidden:true];
}

fn catalog_page_count(catalog: &CatalogState) -> usize {
    ((catalog.total as usize + CATALOG_ROWS - 1) / CATALOG_ROWS).max(1)
}

fn update_catalog_footer(env: &mut Environment, views: &StoreViews, catalog: &CatalogState) {
    let pages = catalog_page_count(catalog);
    set_label(
        env,
        views.page_label,
        &format!("Page {} / {}", catalog.page.min(pages), pages),
    );
    set_button_enabled(
        env,
        views.previous_button,
        !catalog.loading && catalog.page > 1,
    );
    set_button_enabled(
        env,
        views.next_button,
        !catalog.loading && catalog.page < pages,
    );
}

fn clear_rows(env: &mut Environment, views: &mut StoreViews) {
    for row in views.rows.drain(..) {
        () = msg![env; row removeFromSuperview];
    }
    views.download_buttons.clear();
}

fn reset_scroll(env: &mut Environment, views: &StoreViews, content_height: CGFloat) {
    let frame: CGRect = msg![env; (views.scroll_view) frame];
    () = msg![env; (views.scroll_view) setContentSize:(CGSize {
        width: frame.size.width,
        height: content_height.max(frame.size.height),
    })];
    () = msg![env; (views.scroll_view) setContentOffset:(CGPoint { x: 0.0, y: 0.0 })];
}

fn rebuild_catalog_rows(env: &mut Environment, views: &mut StoreViews, items: &[CatalogItem]) {
    clear_rows(env, views);
    let scroll_frame: CGRect = msg![env; (views.scroll_view) frame];
    let width = scroll_frame.size.width;
    let dark: id = msg_class![env; UIColor blackColor];
    let gray = ui_color(env, 0.38, 0.40, 0.44, 1.0);
    for (index, item) in items.iter().enumerate() {
        let row: id = msg_class![env; UIButton buttonWithType:UIButtonTypeCustom];
        () = msg![env; row setFrame:(rect(
            0.0,
            index as CGFloat * ROW_HEIGHT,
            width,
            ROW_HEIGHT
        ))];
        let row_color = if index % 2 == 0 {
            ui_color(env, 1.0, 1.0, 1.0, 1.0)
        } else {
            ui_color(env, 0.96, 0.97, 0.98, 1.0)
        };
        () = msg![env; row setBackgroundColor:row_color];
        () = msg![env; row setTag:(index as NSInteger)];
        let selector = env.objc.lookup_selector("archiveStoreItem:").unwrap();
        // The delegate is attached when the row is created below. Keeping all
        // row subviews inside the button makes the entire entry tappable.
        () = msg![env; (views.scroll_view) addSubview:row];

        let title = new_label(
            env,
            rect(10.0, 5.0, width - 20.0, 34.0),
            &item.title,
            14.0,
            true,
            dark,
        );
        () = msg![env; title setNumberOfLines:2];
        () = msg![env; row addSubview:title];
        let subtitle = new_label(
            env,
            rect(10.0, 41.0, width - 20.0, 15.0),
            &catalog_subtitle(item),
            11.0,
            false,
            gray,
        );
        () = msg![env; row addSubview:subtitle];
        let separator_color = ui_color(env, 0.82, 0.84, 0.87, 1.0);
        let separator = new_view(
            env,
            rect(8.0, ROW_HEIGHT - 1.0, width - 16.0, 1.0),
            separator_color,
        );
        () = msg![env; row addSubview:separator];

        // Store the delegate on the button after all visual subviews exist.
        // `make_button` cannot be used here because a catalog row has a custom
        // title layout rather than a centered UIButton title.
        let delegate = views.delegate;
        () = msg![env; row addTarget:delegate
                              action:selector
                    forControlEvents:UIControlEventTouchUpInside];
        views.rows.push(row);
    }
    reset_scroll(env, views, items.len() as CGFloat * ROW_HEIGHT);
}

fn rebuild_detail_rows(
    env: &mut Environment,
    views: &mut StoreViews,
    details: &ItemDetails,
    downloaded_keys: &HashSet<String>,
) {
    clear_rows(env, views);
    let scroll_frame: CGRect = msg![env; (views.scroll_view) frame];
    let width = scroll_frame.size.width;
    let dark: id = msg_class![env; UIColor blackColor];
    let white: id = msg_class![env; UIColor whiteColor];
    let gray = ui_color(env, 0.38, 0.40, 0.44, 1.0);
    let blue = ui_color(env, 0.0, 0.40, 0.82, 1.0);
    let delegate = views.delegate;
    for (index, file) in details.files.iter().enumerate() {
        let row_color = if index % 2 == 0 {
            ui_color(env, 1.0, 1.0, 1.0, 1.0)
        } else {
            ui_color(env, 0.96, 0.97, 0.98, 1.0)
        };
        let row = new_view(
            env,
            rect(0.0, index as CGFloat * ROW_HEIGHT, width, ROW_HEIGHT),
            row_color,
        );
        () = msg![env; (views.scroll_view) addSubview:row];
        let title = new_label(
            env,
            rect(10.0, 5.0, width - 94.0, 34.0),
            &file.name,
            12.0,
            true,
            dark,
        );
        () = msg![env; title setNumberOfLines:2];
        () = msg![env; row addSubview:title];
        let subtitle = new_label(
            env,
            rect(10.0, 41.0, width - 94.0, 15.0),
            &file_subtitle(file),
            11.0,
            false,
            gray,
        );
        () = msg![env; row addSubview:subtitle];

        let button: id = msg_class![env; UIButton buttonWithType:UIButtonTypeCustom];
        () = msg![env; button setFrame:(rect(width - 76.0, 16.0, 66.0, 30.0))];
        () = msg![env; button setBackgroundColor:blue];
        let already_downloaded = downloaded_keys.contains(&download_key(
            &details.identifier,
            &file.name,
        ));
        set_button_title(env, button, if already_downloaded { "Saved" } else { "GET" });
        () = msg![env; button setTitleColor:white forState:UIControlStateNormal];
        () = msg![env; button setTag:(index as NSInteger)];
        () = msg![env; button layoutSubviews];
        let button_label: id = msg![env; button titleLabel];
        let font: id = msg_class![env; UIFont boldSystemFontOfSize:(12.0 as CGFloat)];
        () = msg![env; button_label setFont:font];
        let layer: id = msg![env; button layer];
        () = msg![env; layer setCornerRadius:(5.0 as CGFloat)];
        let selector = env.objc.lookup_selector("archiveStoreDownload:").unwrap();
        () = msg![env; button addTarget:delegate
                                  action:selector
                        forControlEvents:UIControlEventTouchUpInside];
        () = msg![env; row addSubview:button];
        set_button_enabled(env, button, !already_downloaded);
        views.download_buttons.push((index, button));
        views.rows.push(row);
    }
    reset_scroll(env, views, details.files.len() as CGFloat * ROW_HEIGHT);
}

fn catalog_subtitle(item: &CatalogItem) -> String {
    let mut parts = Vec::new();
    if let Some(version) = &item.version {
        parts.push(format!("Version {version}"));
    }
    if let Some(date) = &item.date {
        parts.push(date.clone());
    }
    if let Some(creator) = &item.creator {
        parts.push(creator.clone());
    }
    if parts.is_empty() {
        item.identifier.clone()
    } else {
        parts.join(" • ")
    }
}

fn file_subtitle(file: &ArchiveFile) -> String {
    let mut parts = Vec::new();
    if let Some(version) = &file.version {
        parts.push(format!("Version {version}"));
    }
    if let Some(size) = file.size {
        parts.push(format_size(size));
    }
    if parts.is_empty() {
        "IPA archive".to_string()
    } else {
        parts.join(" • ")
    }
}

fn set_download_progress(env: &mut Environment, views: &StoreViews, downloaded: u64, total: Option<u64>) {
    let fraction = total
        .filter(|total| *total > 0)
        .map(|total| downloaded as f64 / total as f64)
        .unwrap_or(0.0);
    set_progress_fraction(env, views, fraction);
    let text = if let Some(total) = total.filter(|total| *total > 0) {
        format!(
            "{} / {} • {:.0}%",
            format_size(downloaded),
            format_size(total),
            fraction.clamp(0.0, 1.0) * 100.0
        )
    } else {
        format!("{} downloaded", format_size(downloaded))
    };
    set_label(env, views.progress_label, &text);
}

fn set_progress_fraction(env: &mut Environment, views: &StoreViews, fraction: f64) {
    let mut frame: CGRect = msg![env; (views.progress_fill) frame];
    frame.size.width = views.progress_width * fraction.clamp(0.0, 1.0) as CGFloat;
    () = msg![env; (views.progress_fill) setFrame:frame];
}

fn format_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes_f = bytes as f64;
    if bytes_f >= GIB {
        format!("{:.2} GB", bytes_f / GIB)
    } else if bytes_f >= MIB {
        format!("{:.1} MB", bytes_f / MIB)
    } else if bytes_f >= KIB {
        format!("{:.0} KB", bytes_f / KIB)
    } else {
        format!("{bytes} B")
    }
}

fn download_key(identifier: &str, file_name: &str) -> String {
    format!("{identifier}\n{file_name}")
}

fn display_file_name(name: &str) -> &str {
    name.rsplit(|character| character == '/' || character == '\\')
        .next()
        .unwrap_or(name)
}

fn request_catalog(
    sender: &Sender<StoreMessage>,
    generation: u64,
    query: String,
    page: usize,
    rows: usize,
) {
    let worker_sender = sender.clone();
    let spawn = std::thread::Builder::new()
        .name("HyperHLE archive catalog".to_string())
        .spawn(move || {
            let result = fetch_catalog(&query, page, rows);
            let _ = worker_sender.send(StoreMessage::Catalog { generation, result });
        });
    if let Err(error) = spawn {
        let _ = sender.send(StoreMessage::Catalog {
            generation,
            result: Err(format!("Could not start the catalog request: {error}")),
        });
    }
}

fn request_details(sender: &Sender<StoreMessage>, generation: u64, identifier: String) {
    let worker_sender = sender.clone();
    let spawn = std::thread::Builder::new()
        .name("HyperHLE archive metadata".to_string())
        .spawn(move || {
            let result = fetch_details(&identifier);
            let _ = worker_sender.send(StoreMessage::Details { generation, result });
        });
    if let Err(error) = spawn {
        let _ = sender.send(StoreMessage::Details {
            generation,
            result: Err(format!("Could not start the metadata request: {error}")),
        });
    }
}

fn request_download(
    sender: &Sender<StoreMessage>,
    generation: u64,
    apps_dir: PathBuf,
    identifier: String,
    file: ArchiveFile,
    cancel: Arc<AtomicBool>,
) {
    let worker_sender = sender.clone();
    let progress_sender = sender.clone();
    let spawn = std::thread::Builder::new()
        .name("HyperHLE IPA download".to_string())
        .spawn(move || {
            let result = download_ipa(
                &apps_dir,
                &identifier,
                &file,
                &cancel,
                &progress_sender,
                generation,
            );
            let _ = worker_sender.send(StoreMessage::Download {
                generation,
                event: DownloadEvent::Finished { result },
            });
        });
    if let Err(error) = spawn {
        let _ = sender.send(StoreMessage::Download {
            generation,
            event: DownloadEvent::Finished {
                result: Err(format!("Could not start the download worker: {error}")),
            },
        });
    }
}

fn archive_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(20))
        .timeout_read(Duration::from_secs(60))
        .timeout_write(Duration::from_secs(60))
        .redirects(10)
        .build()
}

fn check_response(result: Result<ureq::Response, ureq::Error>) -> Result<ureq::Response, String> {
    match result {
        Ok(response) => Ok(response),
        Err(ureq::Error::Status(status, response)) => Err(format!(
            "Internet Archive returned HTTP {} {}.",
            status,
            response.status_text()
        )),
        Err(ureq::Error::Transport(error)) => Err(format!(
            "Could not connect to Internet Archive: {error}. Check the network connection and try again."
        )),
    }
}

fn fetch_json(url: &str) -> Result<Value, String> {
    let response = archive_agent()
        .get(url)
        .set("Accept", "application/json")
        .set(
            "User-Agent",
            concat!("HyperHLE-Reborn/", env!("CARGO_PKG_VERSION")),
        )
        .call();
    let response = check_response(response)?;
    let mut body = Vec::new();
    response
        .into_reader()
        .take((MAX_JSON_RESPONSE_SIZE + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|error| format!("Could not read Internet Archive's response: {error}"))?;
    if body.len() > MAX_JSON_RESPONSE_SIZE {
        return Err("Internet Archive's metadata response is too large.".to_string());
    }
    serde_json::from_slice(&body)
        .map_err(|error| format!("Internet Archive returned invalid JSON: {error}"))
}

fn fetch_catalog(query: &str, page: usize, rows: usize) -> Result<CatalogPage, String> {
    let page = page.max(1);
    let rows = rows.clamp(1, 50);
    let url = catalog_url(query, page, rows);
    let json = fetch_json(&url)?;
    parse_catalog(&json, page)
}

fn catalog_url(query: &str, page: usize, rows: usize) -> String {
    let mut archive_query = format!("collection:{COLLECTION_ID}");
    for term in query.split_whitespace().take(8) {
        let term: String = term.chars().take(80).collect();
        let term = escape_lucene_term(&term);
        if !term.is_empty() {
            archive_query.push_str(&format!(
                " AND (title:{term}* OR identifier:{term}*)"
            ));
        }
    }
    let fields = [
        "identifier",
        "title",
        "date",
        "year",
        "creator",
        "version",
    ];
    let mut url = format!(
        "https://archive.org/advancedsearch.php?q={}",
        percent_encode(&archive_query)
    );
    for field in fields {
        url.push_str("&fl%5B%5D=");
        url.push_str(field);
    }
    url.push_str(&format!(
        "&sort%5B%5D={}&rows={rows}&page={page}&output=json",
        percent_encode("title asc")
    ));
    url
}

fn parse_catalog(json: &Value, requested_page: usize) -> Result<CatalogPage, String> {
    let response = json
        .get("response")
        .ok_or_else(|| "The catalog response has no response object.".to_string())?;
    let num_found = response
        .get("numFound")
        .and_then(json_u64)
        .ok_or_else(|| "The catalog response has no result count.".to_string())?;
    let documents = response
        .get("docs")
        .and_then(Value::as_array)
        .ok_or_else(|| "The catalog response has no document list.".to_string())?;
    let mut items = Vec::new();
    for document in documents {
        let Some(identifier) = document.get("identifier").and_then(json_text) else {
            continue;
        };
        if identifier.is_empty() {
            continue;
        }
        let title = document
            .get("title")
            .and_then(json_text)
            .unwrap_or_else(|| identifier.clone());
        let explicit_version = document.get("version").and_then(json_text);
        let version = infer_version(&title, &identifier, explicit_version);
        let date = document
            .get("date")
            .and_then(json_text)
            .or_else(|| document.get("year").and_then(json_text))
            .map(|date| short_date(&date));
        let creator = document.get("creator").and_then(json_text);
        items.push(CatalogItem {
            identifier,
            title,
            version,
            date,
            creator,
        });
    }
    Ok(CatalogPage {
        items,
        num_found,
        page: requested_page,
    })
}

fn fetch_details(identifier: &str) -> Result<ItemDetails, String> {
    let url = format!(
        "https://archive.org/metadata/{}",
        percent_encode(identifier)
    );
    let json = fetch_json(&url)?;
    parse_details(&json, identifier)
}

fn parse_details(json: &Value, requested_identifier: &str) -> Result<ItemDetails, String> {
    let metadata = json.get("metadata");
    let identifier = metadata
        .and_then(|metadata| metadata.get("identifier"))
        .and_then(json_text)
        .unwrap_or_else(|| requested_identifier.to_string());
    let title = metadata
        .and_then(|metadata| metadata.get("title"))
        .and_then(json_text)
        .unwrap_or_else(|| identifier.clone());
    let item_version = metadata
        .and_then(|metadata| metadata.get("version"))
        .and_then(json_text)
        .or_else(|| infer_version(&title, &identifier, None));
    let empty_files = Vec::new();
    let files = json
        .get("files")
        .and_then(Value::as_array)
        .unwrap_or(&empty_files);
    let mut ipa_files = Vec::new();
    for file in files {
        let Some(name) = file.get("name").and_then(json_text) else {
            continue;
        };
        if !name.to_ascii_lowercase().ends_with(".ipa") || json_is_private(file.get("private")) {
            continue;
        }
        let size = file.get("size").and_then(json_u64);
        // A multi-version item often has a generic title but precise file
        // names, so the file's own version wins over the item-level guess.
        let file_version = extract_version(&name).or_else(|| item_version.clone());
        let version = infer_version(&name, &identifier, file_version);
        ipa_files.push(ArchiveFile { name, size, version });
    }
    ipa_files.sort_by_key(|file| file.name.to_ascii_lowercase());
    Ok(ItemDetails {
        identifier,
        title,
        files: ipa_files,
    })
}

fn download_ipa(
    apps_dir: &Path,
    identifier: &str,
    file: &ArchiveFile,
    cancel: &AtomicBool,
    sender: &Sender<StoreMessage>,
    generation: u64,
) -> Result<DownloadOutcome, String> {
    std::fs::create_dir_all(apps_dir)
        .map_err(|error| format!("Could not create the apps directory: {error}"))?;
    let (destination, already_present) =
        choose_destination(apps_dir, identifier, &file.name, file.size);
    if already_present {
        return Ok(DownloadOutcome {
            path: destination,
            already_present: true,
        });
    }

    let partial = destination.with_extension("ipa.part");
    if partial.exists() {
        std::fs::remove_file(&partial)
            .map_err(|error| format!("Could not remove an old partial download: {error}"))?;
    }

    let result = (|| -> Result<(), String> {
        if cancel.load(Ordering::Acquire) {
            return Err("Download cancelled.".to_string());
        }
        let url = download_url(identifier, &file.name);
        let response = archive_agent()
            .get(&url)
            .set(
                "User-Agent",
                concat!("HyperHLE-Reborn/", env!("CARGO_PKG_VERSION")),
            )
            .call();
        let response = check_response(response)?;
        let total = response
            .header("Content-Length")
            .and_then(|length| length.parse::<u64>().ok())
            .or(file.size);
        let mut reader = response.into_reader();
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|error| format!("Could not create {}: {error}", partial.display()))?;
        let mut buffer = [0u8; 64 * 1024];
        let mut downloaded = 0u64;
        let mut last_report = Instant::now();
        let mut last_reported_bytes = 0u64;
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err("Download cancelled.".to_string());
            }
            let count = reader
                .read(&mut buffer)
                .map_err(|error| format!("The download was interrupted: {error}"))?;
            if count == 0 {
                break;
            }
            output
                .write_all(&buffer[..count])
                .map_err(|error| format!("Could not save the IPA: {error}"))?;
            downloaded += count as u64;
            if downloaded - last_reported_bytes >= 1024 * 1024
                || last_report.elapsed() >= Duration::from_millis(150)
            {
                let _ = sender.send(StoreMessage::Download {
                    generation,
                    event: DownloadEvent::Progress {
                        downloaded,
                        total,
                    },
                });
                last_report = Instant::now();
                last_reported_bytes = downloaded;
            }
        }
        output
            .sync_all()
            .map_err(|error| format!("Could not finish saving the IPA: {error}"))?;
        drop(output);
        if let Some(expected) = file.size.filter(|size| *size > 0) {
            if downloaded != expected {
                return Err(format!(
                    "The download is incomplete: expected {expected} bytes, received {downloaded}."
                ));
            }
        }
        // An IPA is a ZIP package. Checking its central directory keeps HTTP
        // error pages and truncated files out of the picker even when the
        // archive metadata did not include a size.
        let package = std::fs::File::open(&partial)
            .map_err(|error| format!("Could not verify the downloaded IPA: {error}"))?;
        drop(zip::ZipArchive::new(package).map_err(|error| {
            format!("The downloaded file is not a valid IPA/ZIP archive: {error}")
        })?);
        let _ = sender.send(StoreMessage::Download {
            generation,
            event: DownloadEvent::Progress {
                downloaded,
                total: Some(downloaded),
            },
        });
        if cancel.load(Ordering::Acquire) {
            return Err("Download cancelled.".to_string());
        }
        std::fs::rename(&partial, &destination).map_err(|error| {
            format!(
                "Could not move the completed IPA into {}: {error}",
                apps_dir.display()
            )
        })?;
        Ok(())
    })();

    if let Err(error) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(error);
    }
    Ok(DownloadOutcome {
        path: destination,
        already_present: false,
    })
}

fn choose_destination(
    apps_dir: &Path,
    identifier: &str,
    archive_name: &str,
    expected_size: Option<u64>,
) -> (PathBuf, bool) {
    let original_name = safe_file_component(display_file_name(archive_name));
    let original = apps_dir.join(&original_name);
    if existing_file_matches(&original, expected_size) {
        return (original, true);
    }
    if !original.exists() {
        return (original, false);
    }

    let stem = original_name
        .get(..original_name.len().saturating_sub(4))
        .unwrap_or("app");
    let suffix = safe_file_component(identifier);
    for counter in 1..1000usize {
        let suffix = if counter == 1 {
            suffix.clone()
        } else {
            format!("{suffix}-{counter}")
        };
        let name: String = format!(
            "{} [{}].ipa",
            stem.chars().take(100).collect::<String>(),
            suffix.chars().take(48).collect::<String>()
        );
        let candidate = apps_dir.join(name);
        if existing_file_matches(&candidate, expected_size) {
            return (candidate, true);
        }
        if !candidate.exists() {
            return (candidate, false);
        }
    }
    (original, false)
}

fn existing_file_matches(path: &Path, expected_size: Option<u64>) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    metadata.is_file()
        && expected_size
            .filter(|size| *size > 0)
            .is_none_or(|size| metadata.len() == size)
}

fn safe_file_component(name: &str) -> String {
    let mut result: String = name
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                '_'
            } else {
                character
            }
        })
        .take(160)
        .collect();
    result = result.trim().trim_matches('.').to_string();
    if result.is_empty() || result == "." || result == ".." {
        "app.ipa".to_string()
    } else {
        result
    }
}

fn download_url(identifier: &str, file_name: &str) -> String {
    let encoded_path = file_name
        .split('/')
        .map(percent_encode)
        .collect::<Vec<_>>()
        .join("/");
    format!(
        "https://archive.org/download/{}/{}",
        percent_encode(identifier),
        encoded_path
    )
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push('%');
            encoded.push_str(&format!("{byte:02X}"));
        }
    }
    encoded
}

fn escape_lucene_term(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if "+-=&|><!(){}[]^\"~*?:\\/".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn json_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let text = text.trim();
            (!text.is_empty()).then_some(text.to_string())
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Array(values) => {
            let text = values.iter().filter_map(json_text).collect::<Vec<_>>().join(", ");
            (!text.is_empty()).then_some(text)
        }
        _ => None,
    }
}

fn json_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

fn json_is_private(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::String(value)) => value.eq_ignore_ascii_case("true") || value == "1",
        Some(Value::Number(value)) => value.as_u64() == Some(1),
        _ => false,
    }
}

fn short_date(value: &str) -> String {
    value.chars().take(10).collect()
}

fn infer_version(title: &str, identifier: &str, explicit: Option<String>) -> Option<String> {
    explicit
        .and_then(|version| {
            let version = version.trim().trim_start_matches(['v', 'V']).to_string();
            (!version.is_empty()).then_some(version)
        })
        .or_else(|| extract_version(title))
        .or_else(|| extract_version(identifier))
}

fn extract_version(text: &str) -> Option<String> {
    // Do not mistake the package extension for another version component.
    let text = text
        .get(text.len().saturating_sub(4)..)
        .filter(|extension| extension.eq_ignore_ascii_case(".ipa"))
        .map(|_| &text[..text.len() - 4])
        .unwrap_or(text);
    for token in text.split(|character: char| {
        character.is_whitespace() || "(),[]{}<>".contains(character)
    }) {
        let token = token.trim_matches(|character: char| {
            matches!(character, ':' | ';' | '!' | '?' | '"' | '\'')
        });
        let explicit_v = token.as_bytes().get(..2).is_some_and(|prefix| {
            matches!(prefix[0], b'v' | b'V') && prefix[1].is_ascii_digit()
        });
        for (start, character) in token.char_indices() {
            if !character.is_ascii_digit() {
                continue;
            }
            let mut end = start + character.len_utf8();
            for (index, next) in token.char_indices().skip_while(|(index, _)| *index <= start) {
                if next.is_ascii_digit() || next == '.' {
                    end = index + next.len_utf8();
                } else {
                    break;
                }
            }
            let candidate = token[start..end].trim_end_matches('.');
            let dotted = candidate.contains('.');
            if !dotted && !(explicit_v && start == 1) {
                continue;
            }
            if candidate.len() <= 40
                && candidate
                    .split('.')
                    .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn catalog_keeps_every_collection_media_type_and_reads_versions() {
        let json = json!({"response": {"numFound": 2, "docs": [
            {"identifier": "example-game", "title": "Example Game 1.2.3", "mediatype": "movies"},
            {"identifier": "other", "title": "Other", "version": "v4.5"}
        ]}});
        let page = parse_catalog(&json, 3).unwrap();
        assert_eq!(page.num_found, 2);
        assert_eq!(page.page, 3);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].version.as_deref(), Some("1.2.3"));
        assert_eq!(page.items[1].version.as_deref(), Some("4.5"));
    }

    #[test]
    fn details_only_expose_public_ipa_files() {
        let json = json!({
            "metadata": {"identifier": "item", "title": "Game 2.0"},
            "files": [
                {"name": "Game 2.0.ipa", "size": "1024"},
                {"name": "Other.IPA", "size": 2048},
                {"name": "notes.txt"},
                {"name": "secret.ipa", "private": "true"}
            ]
        });
        let details = parse_details(&json, "item").unwrap();
        assert_eq!(details.files.len(), 2);
        assert_eq!(details.files[0].size, Some(1024));
        assert_eq!(details.files[1].size, Some(2048));
        assert_eq!(details.files[0].version.as_deref(), Some("2.0"));
    }

    #[test]
    fn version_inference_handles_common_archive_titles() {
        assert_eq!(extract_version("Video Star 11.2.8 1675756235"), Some("11.2.8".to_string()));
        assert_eq!(extract_version("[ Temple Run ][ 1.6.1 ]"), Some("1.6.1".to_string()));
        assert_eq!(extract_version("Talking Tom v2.7 IPA"), Some("2.7".to_string()));
        assert_eq!(extract_version("Game 2.0.ipa"), Some("2.0".to_string()));
        assert_eq!(extract_version("Game-1.2.3.ipa"), Some("1.2.3".to_string()));
        assert_eq!(extract_version("iPhone4-1.2.3.ipa"), Some("1.2.3".to_string()));
        assert_eq!(extract_version("Minecraft Pocket Edition"), None);
        assert_eq!(extract_version("é1"), None);
    }

    #[test]
    fn archive_urls_are_encoded_without_flattening_file_paths() {
        assert_eq!(
            download_url("item id", "folder/Game & More.ipa"),
            "https://archive.org/download/item%20id/folder/Game%20%26%20More.ipa"
        );
        let url = catalog_url("", 1, 25);
        assert!(url.contains("collection%3Aipaarchive"));
        assert!(!url.contains("mediatype"));
    }

    #[test]
    fn file_names_cannot_escape_the_apps_directory() {
        assert_eq!(
            safe_file_component(display_file_name("../Game.ipa")),
            "Game.ipa"
        );
        assert_eq!(safe_file_component(".."), "app.ipa");
        assert_eq!(display_file_name("folder/subfolder/Game.ipa"), "Game.ipa");
    }

    #[test]
    fn search_terms_are_literal_and_paginated() {
        let url = catalog_url("need:for speed*", 4, 25);
        assert!(url.contains("page=4"));
        assert!(url.contains("rows=25"));
        assert!(url.contains("need%5C%3Afor"));
        assert!(url.contains("speed%5C%2A"));
    }
}

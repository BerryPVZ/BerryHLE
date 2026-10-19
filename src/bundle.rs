/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Utilities for working with bundles. So far we are only interested in the
//! application bundle.
//!
//! Relevant Apple documentation:
//! * [Bundle Programming Guide](https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/Introduction/Introduction.html)
//!   * [Anatomy of an iOS Application Bundle](https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/BundleTypes/BundleTypes.html)
//! * [Bundle Resources](https://developer.apple.com/documentation/bundleresources?language=objc)

use crate::fs::{BundleData, Fs, GuestPath, GuestPathBuf};
use crate::image::Image;
use crate::window::{DeviceFamily, DeviceOrientation};
use plist::dictionary::Dictionary;
use plist::Value;
use std::io::Cursor;

#[derive(Debug)]
pub struct Bundle {
    path: GuestPathBuf,
    plist: Dictionary,
}

impl Bundle {
    /// See [Fs::new] for meaning of `read_only_mode`.
    pub fn new_bundle_and_fs_from_host_path(
        mut bundle_data: BundleData,
        read_only_mode: bool,
    ) -> Result<(Bundle, Fs), String> {
        let plist_bytes = bundle_data.read_plist()?;

        let plist = Value::from_reader(Cursor::new(plist_bytes))
            .map_err(|_| "Could not deserialize plist data".to_string())?;

        let plist = plist
            .into_dictionary()
            .ok_or_else(|| "plist root value is not a dictionary".to_string())?;

        let bundle_name = format!(
            "{}.app",
            if let Some(canonical) = plist.get("CFBundleName") {
                canonical.as_string().unwrap()
            } else {
                bundle_data.bundle_name()
            }
        );
        let bundle_id = plist["CFBundleIdentifier"].as_string().unwrap();

        let (fs, guest_path) = Fs::new(bundle_data, bundle_name, bundle_id, read_only_mode);

        let bundle = Bundle {
            path: guest_path,
            plist,
        };

        Ok((bundle, fs))
    }

    /// Create a fake bundle (see [crate::Environment::new_without_app]).
    pub fn new_fake_bundle() -> Bundle {
        Bundle {
            path: GuestPathBuf::from(String::new()),
            plist: Dictionary::new(),
        }
    }

    pub fn bundle_path(&self) -> &GuestPath {
        &self.path
    }

    pub fn bundle_identifier(&self) -> &str {
        self.plist
            .get("CFBundleIdentifier")
            .and_then(|v| v.as_string())
            .unwrap_or("org.touchhle.app-picker")
    }

    pub fn bundle_version(&self) -> &str {
        self.plist
            .get("CFBundleVersion")
            .or_else(|| self.plist.get("CFBundleShortVersionString"))
            .and_then(|v| v.as_string())
            .unwrap_or("0")
    }

    pub fn bundle_localizations(&self) -> &[Value] {
        static EMPTY_VAL: Vec<Value> = Vec::new();
        self.plist
            .get("CFBundleLocalizations")
            .and_then(|v| v.as_array())
            .unwrap_or(&EMPTY_VAL)
    }

    /// Canonical name for the bundle according to Info.plist
    pub fn canonical_bundle_name(&self) -> Option<&str> {
        self.plist
            .get("CFBundleName")
            .map(|name| name.as_string().unwrap())
    }

    /// Name for the bundle, either the canonical name or, if there isn't one,
    /// the name this bundle has in the filesystem.
    pub fn bundle_name(&self) -> &str {
        self.path.file_name().unwrap().strip_suffix(".app").unwrap()
    }

    pub fn display_name(&self) -> &str {
        if let Some(display_name) = self.plist.get("CFBundleDisplayName") {
            display_name.as_string().unwrap()
        } else {
            ""
        }
    }

    pub fn minimum_os_version(&self) -> Option<String> {
        // `MinimumOSVersion` is documented as a string (e.g. "3.0"), but some
        // bundles — particularly 64-bit App Store builds — encode it as a
        // real number (3.0) or integer (3) in their binary Info.plist.
        // `as_string().unwrap()` panicked on those, taking down the whole
        // emulator. Accept any of those encodings and normalise to a string.
        self.plist.get("MinimumOSVersion").and_then(|v| {
            if let Some(s) = v.as_string() {
                Some(s.to_string())
            } else if let Some(i) = v.as_signed_integer() {
                Some(i.to_string())
            } else if let Some(u) = v.as_unsigned_integer() {
                Some(u.to_string())
            } else {
                v.as_real().map(|r| {
                    // Format e.g. 3.0 as "3", 6.1 as "6.1" — the consumer in
                    // lib.rs parses major/minor numerically and tolerates a
                    // missing minor component.
                    if r.fract() == 0.0 {
                        format!("{}", r as i64)
                    } else {
                        format!("{}", r)
                    }
                })
            }
        })
    }

    pub fn required_device_capabilities(&self) -> Vec<&str> {
        self.plist
            .get("UIRequiredDeviceCapabilities")
            .map(|v| {
                if let Some(dict) = v.as_dictionary() {
                    // TODO: support undesired capabilities
                    assert!(dict.values().all(|x| x.as_boolean().unwrap()));
                    dict.keys().map(|o| o.as_str()).collect()
                } else {
                    v.as_array()
                        .unwrap()
                        .iter()
                        .map(|o| o.as_string().unwrap())
                        .collect()
                }
            })
            .unwrap_or_default()
    }

    pub fn executable_path(&self) -> GuestPathBuf {
        // FIXME: Is this key optional? All iPhone apps seem to have it.
        self.path
            .join(self.plist["CFBundleExecutable"].as_string().unwrap())
    }

    /// Find the launch image for the given device family.
    ///
    /// Returns [None] if the bundle has no launch image. The returned path uses
    /// the spelling stored in the bundle: app bundles were built on
    /// case-insensitive volumes, so a plist entry like `Default@2x.png` may
    /// match a file actually named `default@2x.png`. Returning the spelling
    /// from the plist for such a file made the caller's exact-match check fail
    /// even though the log said the image was being used.
    pub fn launch_image_path(
        &self,
        fs: &Fs,
        device_family: DeviceFamily,
    ) -> Option<GuestPathBuf> {
        // Check if there's a custom base name in plist
        let base_name = self
            .plist
            .get("UILaunchImageFile")
            .map(|v| v.as_string().unwrap())
            .unwrap_or("Default");

        // Try device-specific variants first, then fallback to base name
        let candidates = if device_family.is_ipad() {
            if device_family.is_retina() {
                vec![
                    format!("{}@2x~ipad.png", base_name), // iPad Retina
                    format!("{}~ipad.png", base_name),    // iPad non-Retina
                    format!("{}@2x.png", base_name),      // Fallback Retina
                    format!("{}.png", base_name),         // Fallback
                ]
            } else {
                vec![
                    format!("{}~ipad.png", base_name),    // iPad non-Retina
                    format!("{}@2x~ipad.png", base_name), // iPad Retina
                    format!("{}.png", base_name),         // Fallback
                ]
            }
        } else if device_family.is_phone_568() {
            vec![
                format!("{}-568h@2x.png", base_name), // iPhone 5 (4-inch)
                format!("{}@2x.png", base_name),      // iPhone Retina
                format!("{}.png", base_name),         // iPhone non-Retina
            ]
        } else if device_family.is_retina() {
            vec![
                format!("{}@2x.png", base_name), // iPhone Retina
                format!("{}.png", base_name),    // iPhone non-Retina
            ]
        } else {
            vec![
                format!("{}@2x.png", base_name), // iPhone Retina (fallback)
                format!("{}.png", base_name),    // iPhone non-Retina
            ]
        };

        if let Some(path) = self.first_existing_file(fs, candidates.iter().map(String::as_str)) {
            log!("Using launch image: {}", path.as_str());
            return Some(path);
        }

        // Final fallback: a plain `Default.png` in the bundle, whatever the
        // plist said.
        if let Some(path) = self.first_existing_file(fs, ["Default.png"].into_iter()) {
            log!("Using launch image: {}", path.as_str());
            return Some(path);
        }

        log!("Warning: No launch image found");
        None
    }

    /// The first of `names` that exists in the bundle root, with the spelling
    /// that is actually stored there.
    fn first_existing_file<'a>(
        &self,
        fs: &Fs,
        names: impl Iterator<Item = &'a str>,
    ) -> Option<GuestPathBuf> {
        for name in names {
            let path = self.path.join(name);
            // Resolve the case first: `fs.is_file()` matches the VFS spelling
            // exactly, while the plist may use another case.
            let Some(resolved) = fs.resolve_case_insensitive_path(&path) else {
                continue;
            };
            if fs.is_file(&resolved) {
                return Some(resolved);
            }
        }
        None
    }

    pub fn status_bar_hidden(&self) -> bool {
        self.plist
            .get("UIStatusBarHidden")
            .and_then(|v| v.as_boolean())
            .unwrap_or(false)
    }

    /// Resolve all candidate icon files declared by the bundle's
    /// Info.plist, in priority order. Returns paths from most-specific
    /// to least-specific so callers can fall back when a file is
    /// missing on disk.
    ///
    /// Apple's documented lookup order (see "App Icons on iPhone, iPod
    /// touch, and iPad", `CFBundleIcons` reference, and historical
    /// `CFBundleIconFile`):
    ///
    /// 1. `CFBundleIcons` (iOS 5+) → `CFBundlePrimaryIcon` →
    ///    `CFBundleIconFiles[]` (an array of stem names; iOS picks the
    ///    one whose suffix matches the device class — `@2x.png`,
    ///    `@2x~ipad.png`, …; we just try the literal name and the
    ///    explicit "@2x.png"/".png" decorations).
    /// 2. `CFBundleIconFile` (legacy single-string key, iOS 2/3/4).
    /// 3. Hard-coded defaults: `Icon.png`, `icon.png`,
    ///    `Icon@2x.png`.
    fn icon_path_candidates(&self) -> Vec<GuestPathBuf> {
        let mut candidates: Vec<String> = Vec::new();

        let push = |candidates: &mut Vec<String>, name: &str| {
            let lower = name.to_lowercase();
            if lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                candidates.push(name.to_string());
            } else {
                candidates.push(format!("{name}.png"));
                candidates.push(format!("{name}@2x.png"));
                candidates.push(format!("{name}.jpg"));
            }
        };

        // 1. CFBundleIcons → CFBundlePrimaryIcon → CFBundleIconFiles (iOS 5+).
        if let Some(icons) = self.plist.get("CFBundleIcons") {
            if let Some(dict) = icons.as_dictionary() {
                if let Some(primary) = dict.get("CFBundlePrimaryIcon") {
                    if let Some(primary_dict) = primary.as_dictionary() {
                        if let Some(files) = primary_dict.get("CFBundleIconFiles") {
                            if let Some(arr) = files.as_array() {
                                for entry in arr {
                                    if let Some(s) = entry.as_string() {
                                        push(&mut candidates, s);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        // The iPad-specific variant (`CFBundleIcons~ipad`) lives at the
        // top level too.
        if let Some(icons) = self.plist.get("CFBundleIcons~ipad") {
            if let Some(dict) = icons.as_dictionary() {
                if let Some(primary) = dict.get("CFBundlePrimaryIcon") {
                    if let Some(primary_dict) = primary.as_dictionary() {
                        if let Some(files) = primary_dict.get("CFBundleIconFiles") {
                            if let Some(arr) = files.as_array() {
                                for entry in arr {
                                    if let Some(s) = entry.as_string() {
                                        push(&mut candidates, s);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 2. CFBundleIconFiles[] at the top level (rare; iOS 3.2+ aux key).
        if let Some(files) = self.plist.get("CFBundleIconFiles") {
            if let Some(arr) = files.as_array() {
                for entry in arr {
                    if let Some(s) = entry.as_string() {
                        push(&mut candidates, s);
                    }
                }
            }
        }

        // 3. Legacy CFBundleIconFile (iOS 2/3/4).
        if let Some(filename) = self.plist.get("CFBundleIconFile") {
            if let Some(s) = filename.as_string() {
                push(&mut candidates, s);
            }
        }

        // 4. Hard-coded defaults documented in
        //    "Icon Files" of the iPhone Application Programming Guide.
        for default in [
            "Icon.png",
            "icon.png",
            "Icon@2x.png",
            "Icon-72.png",
            "Icon-Small.png",
        ] {
            candidates.push(default.to_string());
        }

        // Deduplicate while preserving order.
        let mut seen = std::collections::HashSet::new();
        candidates.retain(|c| seen.insert(c.clone()));

        candidates
            .into_iter()
            .map(|name| self.path.join(name))
            .collect()
    }

    fn icon_path(&self) -> GuestPathBuf {
        // Backwards-compatible single-path accessor: return the first
        // candidate produced by the full lookup. The new `load_icon`
        // path tries all of them in order before giving up.
        self.icon_path_candidates()
            .into_iter()
            .next()
            .unwrap_or_else(|| self.path.join("Icon.png"))
    }

    /// Every image file in the bundle that looks like an app icon, whatever
    /// the `Info.plist` says. Old games often ship a large `iTunesArtwork` or
    /// `Icon-72@2x.png` that the plist never mentions, while the plist points
    /// at a tiny 57×57 `Icon.png`.
    fn stray_icon_path_candidates(&self, fs: &Fs) -> Vec<GuestPathBuf> {
        let Ok(names) = fs.enumerate(&self.path) else {
            return Vec::new();
        };
        let mut candidates: Vec<GuestPathBuf> = names
            .filter(|name| is_icon_file_name(name))
            .map(|name| self.path.join(name))
            .collect();
        // `enumerate` iterates a hash map, so sort for a deterministic order
        // (used as the tie-break between equally large images).
        candidates.sort();
        candidates
    }

    /// Load icon and round off its corners (and add sheen if needed) for
    /// display.
    ///
    /// The plist names are only a hint: old bundles often ship several icons
    /// (`Icon.png` 57×57, `Icon@2x.png` 114×114, `Icon-72@2x.png` 144×144, …),
    /// and some ship placeholder images that are larger than the real icon.
    /// So instead of taking the first file that exists, this looks at *every*
    /// icon-like file in the bundle, ranks them by resolution (PNG and JPEG
    /// headers are enough, so nothing is decoded twice), and decodes them
    /// largest-first until a usable image is found. A large icon is what keeps
    /// the picker sharp: it is drawn at roughly 113 physical pixels on a
    /// 2× display, so a 57×57 icon would be noticeably blurry.
    pub fn load_icon(&self, fs: &Fs) -> Result<Image, String> {
        // Gather unique candidate paths. The plist spelling must be resolved
        // case-insensitively: the bundle was built on a case-insensitive
        // volume, so e.g. "Icon@2x.png" in the plist may be stored as
        // "icon@2x.png" in the VFS.
        let mut paths: Vec<GuestPathBuf> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for path in self.icon_path_candidates() {
            let resolved = fs
                .resolve_case_insensitive_path(&path)
                .unwrap_or(path);
            if seen.insert(resolved.clone()) {
                paths.push(resolved);
            }
        }
        for path in self.stray_icon_path_candidates(fs) {
            if seen.insert(path.clone()) {
                paths.push(path);
            }
        }

        // Rank the files by resolution using their headers, then keep only the
        // paths: the image data is re-read in ranked order below, so a bundle
        // with many icons doesn't hold all of them in memory at once.
        let mut ranked: Vec<(u64, usize, GuestPathBuf)> = Vec::new();
        for path in paths {
            let Ok(bytes) = fs.read(&path) else {
                continue;
            };
            let area = image_area(&bytes);
            let order = ranked.len();
            ranked.push((area, order, path));
        }
        // Largest first; `sort_by_key` is stable, so equal sizes keep the
        // plist-first order.
        ranked.sort_by_key(|entry| std::cmp::Reverse(entry.0));

        // Decode largest-first. The first image that isn't a blank/flat
        // placeholder wins; the largest placeholder is kept as a last resort.
        let mut best: Option<Image> = None;
        let mut best_area: u64 = 0;
        let mut last_err: Option<String> = None;
        for (_, _, path) in &ranked {
            let Ok(bytes) = fs.read(path) else {
                continue;
            };
            match Image::from_bytes(&bytes) {
                Ok(image) => {
                    if !image.is_blank_or_solid_black() {
                        best = Some(image);
                        break;
                    }
                    let area = image.area();
                    if area > best_area {
                        best_area = area;
                        best = Some(image);
                    }
                }
                Err(e) => last_err = Some(format!("Could not parse icon image: {e}")),
            }
        }

        let Some(mut image) = best else {
            // Mirror the historical phrasing so any tooling that scrapes
            // the warning still recognises it.
            return Err(last_err.unwrap_or_else(|| "Could not read icon file".to_string()));
        };
        // UIPrerenderedIcon is used to avoid iOS applying a sheen effect,
        // should be boolean, but some apps use a string, so we check both.
        // See https://developer.apple.com/library/archive/qa/qa1614/_index.html
        // Default if it does not exist is NO/false.
        let add_sheen = !self
            .plist
            .get("UIPrerenderedIcon")
            .and_then(|v| v.as_boolean().or(v.as_string().map(|s| s == "YES")))
            .unwrap_or(false);
        // iPhone OS icons are 57px by 57px and the OS always applies a
        // rounded corner (see e.g. documentation of UIPrerenderedIcon). The
        // radius scales with the icon's resolution, so `@2x`/`@3x` icons get
        // proportionally rounded corners rather than nearly square ones.
        image.round_icon_corners(add_sheen);
        Ok(image)
    }

    pub fn main_nib_filename(&self, device_family: Option<DeviceFamily>) -> Option<&str> {
        // TODO: extend this logic for all device-specific keys
        if let Some(device_family) = device_family {
            if device_family.is_ipad() && self.plist.get("NSMainNibFile~ipad").is_some() {
                return self
                    .plist
                    .get("NSMainNibFile~ipad")
                    .map(|v| v.as_string().unwrap());
            }
        }
        self.plist
            .get("NSMainNibFile")
            .map(|v| v.as_string().unwrap())
    }

    /// The value of `UIMainStoryboardFile` (or its `~ipad` device-specific
    /// variant) from `Info.plist`, if any. This is the modern (iOS 5+)
    /// replacement for `NSMainNibFile`: the value is the base name (without
    /// extension) of a compiled storyboard found in the bundle resources
    /// (e.g. `Main` resolves to `Base.lproj/Main~iphone.storyboardc/` on
    /// iPhone).
    pub fn main_storyboard_filename(&self, device_family: Option<DeviceFamily>) -> Option<&str> {
        if let Some(device_family) = device_family {
            if device_family.is_ipad() && self.plist.get("UIMainStoryboardFile~ipad").is_some() {
                return self
                    .plist
                    .get("UIMainStoryboardFile~ipad")
                    .map(|v| v.as_string().unwrap());
            }
        }
        self.plist
            .get("UIMainStoryboardFile")
            .map(|v| v.as_string().unwrap())
    }

    pub fn supported_interface_orientations(&self) -> Vec<&str> {
        // Apple's Bundle Resources documentation
        // (https://developer.apple.com/documentation/bundleresources/information-property-list/uisupportedinterfaceorientations)
        // says UISupportedInterfaceOrientations (iOS 3.2+) is an Array of
        // Strings, and UIInterfaceOrientation (iPhone OS 2.0+) is a single
        // String. UISupportedInterfaceOrientations takes precedence.
        //
        // In practice some pre-3.2 apps (e.g. Tiny Zoo Friends, plist
        // observed in the wild) store UISupportedInterfaceOrientations as
        // a single String — sometimes even a comma-separated list — which
        // is technically malformed but iOS accepts it. We mirror that
        // tolerance instead of crashing the host emulator.
        if let Some(v) = self.plist.get("UISupportedInterfaceOrientations") {
            if let Some(arr) = v.as_array() {
                return arr.iter().filter_map(|o| o.as_string()).collect();
            }
            if let Some(s) = v.as_string() {
                if s.contains(',') {
                    log!(
                        "UISupportedInterfaceOrientations is a comma-separated string ({}); splitting.",
                        s
                    );
                }
                return s
                    .split(',')
                    .map(|p| p.trim())
                    .filter(|p| !p.is_empty())
                    .collect();
            }
            log!(
                "UISupportedInterfaceOrientations has unexpected plist type {:?}; ignoring.",
                v
            );
        }

        if let Some(v) = self.plist.get("UIInterfaceOrientation") {
            let str = v.as_string().unwrap_or("UIInterfaceOrientationPortrait");
            if str.contains(',') {
                log!(
                    "UIInterfaceOrientation is a comma separated list of strings ({}), splitting!",
                    str
                );
            }
            return str
                .split(',')
                .map(|p| p.trim())
                .filter(|p| !p.is_empty())
                .collect();
        }

        vec!["UIInterfaceOrientationPortrait"]
    }

    pub fn device_family_array(&self) -> Vec<DeviceFamily> {
        // Apple docs: UIDeviceFamily is an Array of Numbers (1=iPhone, 2=iPad).
        // https://developer.apple.com/documentation/bundleresources/information-property-list/uidevicefamily
        // We additionally accept strings ("1", "2", "iphone", "ipad") because
        // a few real-world plists serialise the values that way; ignore
        // unknown entries instead of panicking the emulator.
        let Some(v) = self.plist.get("UIDeviceFamily") else {
            return vec![DeviceFamily::iPhone];
        };
        let Some(arr) = v.as_array() else {
            log!(
                "UIDeviceFamily has unexpected plist type {:?}; defaulting to iPhone.",
                v
            );
            return vec![DeviceFamily::iPhone];
        };
        let mut families = Vec::new();
        for o in arr {
            let parsed = match o {
                Value::Integer(i) => i.as_unsigned().and_then(|n| DeviceFamily::try_from(n).ok()),
                Value::String(s) => {
                    // First try numeric; fall back to symbolic name.
                    s.parse::<u64>()
                        .ok()
                        .and_then(|n| DeviceFamily::try_from(n).ok())
                        .or_else(|| DeviceFamily::try_from(s.as_str()).ok())
                }
                _ => None,
            };
            match parsed {
                Some(family) => families.push(family),
                None => log!(
                    "UIDeviceFamily entry {:?} is not a recognised device family; ignoring.",
                    o
                ),
            }
        }
        if families.is_empty() {
            vec![DeviceFamily::iPhone]
        } else {
            families
        }
    }
}

/// Pixel area of a PNG or JPEG image, read from its header, or 0 if the
/// format isn't recognised. Only used to rank icon candidates, so a cheap
/// header parse is enough and large icons are never decoded just to compare
/// their sizes.
fn image_area(bytes: &[u8]) -> u64 {
    let png = png_area(bytes);
    if png > 0 {
        return png;
    }
    jpeg_area(bytes)
}

/// Area of a JPEG image, found by walking its marker segments until a
/// start-of-frame marker (SOF0/SOF1/SOF2, …) with the frame size. Returns 0
/// for anything that is not a JPEG or has no readable frame header.
fn jpeg_area(bytes: &[u8]) -> u64 {
    // SOI marker.
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return 0;
    }
    let mut i = 2;
    // Every marker needs at least two length bytes plus five header bytes for
    // SOF, hence the +9.
    while i + 9 <= bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        // Standalone markers (TEM, RSTn, SOI, EOI) have no length field.
        if marker == 0x01 || (0xD0..=0xD9).contains(&marker) {
            i += 2;
            continue;
        }
        let length = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if length < 2 {
            return 0;
        }
        // SOF markers carry the frame dimensions; C4 (DHT), C8 (JPG) and
        // CC (DAC) share the C0..CF range but are not frame headers.
        let is_sof =
            (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC;
        if is_sof {
            let height = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u64;
            let width = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u64;
            return width * height;
        }
        i += 2 + length;
    }
    0
}

/// Width times height of a PNG file, read from its header without decoding
/// the image. Returns 0 for other formats or malformed files.
fn png_area(bytes: &[u8]) -> u64 {
    const PNG_MAGIC: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    // The IHDR chunk must come first: 8-byte magic, 4-byte length, "IHDR",
    // then the width and height as big-endian u32.
    if bytes.len() < 24 || bytes[..8] != PNG_MAGIC || &bytes[12..16] != b"IHDR" {
        return 0;
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    u64::from(width) * u64::from(height)
}

/// Whether a file name in an app bundle looks like an app icon:
/// `Icon.png`, `Icon@2x.png`, `Icon-72.png`, `Icon-Small@2x.png`,
/// `AppIcon60x60@2x.png`, `iTunesArtwork`, and so on.
fn is_icon_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // Strip the extension and any @2x/@3x and ~ipad/~iphone decorations.
    let stem = lower.split('.').next().unwrap_or("");
    let stem = stem.split('@').next().unwrap_or(stem);
    let stem = stem.split('~').next().unwrap_or(stem);
    // `iTunesArtwork`/`iTunesArtwork@2x` have no file extension, but they are
    // usually the largest (and sharpest) artwork in the bundle.
    if !lower.contains('.') && stem == "itunesartwork" {
        return true;
    }
    let is_image =
        lower.ends_with(".png") || lower.ends_with(".jpg") || lower.ends_with(".jpeg");
    is_image && (stem.starts_with("icon") || stem.starts_with("appicon"))
}

#[cfg(test)]
mod icon_area_tests {
    use super::{image_area, jpeg_area};

    #[test]
    fn jpeg_area_reads_the_frame_size() {
        let mut bytes = vec![0xFF, 0xD8]; // SOI
        // APP0 segment: marker, length (2 bytes length field + payload).
        bytes.extend_from_slice(&[0xFF, 0xE0, 0x00, 0x04, 0xAA, 0xBB]);
        // SOF0: length 17, precision 8, height 512, width 432, 3 components.
        bytes.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        bytes.extend_from_slice(&512u16.to_be_bytes());
        bytes.extend_from_slice(&432u16.to_be_bytes());
        bytes.push(0x03);
        assert_eq!(jpeg_area(&bytes), 512 * 432);
        assert_eq!(image_area(&bytes), 512 * 432);
        // Not a JPEG at all.
        assert_eq!(jpeg_area(b"\x89PNG\r\n\x1a\n"), 0);
    }
}

#[cfg(test)]
mod icon_file_name_tests {
    use super::is_icon_file_name;

    #[test]
    fn recognises_common_icon_names() {
        for name in [
            "Icon.png",
            "icon.png",
            "Icon@2x.png",
            "Icon-72@2x.png",
            "Icon-Small-50.png",
            "AppIcon60x60@2x.png",
            "iTunesArtwork",
            "iTunesArtwork@2x",
            "AppIcon.jpg",
        ] {
            assert!(is_icon_file_name(name), "{name} should be an icon file");
        }
    }

    #[test]
    fn ignores_other_bundle_files() {
        for name in [
            "Default.png",
            "Default@2x.png",
            "Info.plist",
            "game.dat",
            "IconSheet.png.plist",
            "pic.png",
        ] {
            assert!(!is_icon_file_name(name), "{name} should not be an icon");
        }
    }
}

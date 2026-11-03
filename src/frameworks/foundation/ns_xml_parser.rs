/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! NSXMLParser.

use super::ns_string::from_rust_string;
use super::NSUInteger;
use crate::environment::Environment;
use crate::mem::ConstVoidPtr;
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, todo_objc_setter,
    ClassExports, HostObject, NSZonePtr, SEL,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

/// Maximum consecutive errors before giving up on recovery.
const MAX_CONSECUTIVE_ERRORS: usize = 50;

#[derive(Default)]
struct NSXMLParserHostObject {
    data: id,
    delegate: id,
    error: id, // NSError*
    should_resolve_external_entities: bool,
}
impl HostObject for NSXMLParserHostObject {}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSXMLParser: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSXMLParserHostObject {
        data: nil,
        delegate: nil,
        error: nil,
        should_resolve_external_entities: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithContentsOfURL:(id)url { // NSURL *
    let data: id = msg_class![env; NSData dataWithContentsOfURL:url];
    msg![env; this initWithData:data]
}

- (id)initWithData:(id)data { // NSData *
    if data == nil {
        release(env, this);
        return nil;
    }
    retain(env, data);
    env.objc.borrow_mut::<NSXMLParserHostObject>(this).data = data;
    this
}

- (())setDelegate:(id)delegate {
    env.objc.borrow_mut::<NSXMLParserHostObject>(this).delegate = delegate;
}
- (id)delegate {
    env.objc.borrow::<NSXMLParserHostObject>(this).delegate
}

- (())setShouldResolveExternalEntities:(bool)should_resolve {
        env.objc.borrow_mut::<NSXMLParserHostObject>(this).should_resolve_external_entities = should_resolve;
}
- (())setShouldProcessNamespaces:(bool)should {
    todo_objc_setter!(this, should);
}
- (())setShouldReportNamespacePrefixes:(bool)should {
    todo_objc_setter!(this, should);
}

- (id)parserError {
    env.objc.borrow::<NSXMLParserHostObject>(this).error
}

- (bool)parse {
    let data = env.objc.borrow::<NSXMLParserHostObject>(this).data;

    if data == nil {
        log!("NSXMLParser: data is nil, cannot parse!");
        let error = make_parse_error(env, 0, "Data is nil".to_string());
        env.objc.borrow_mut::<NSXMLParserHostObject>(this).error = error;
        return false;
    }

    let bytes: ConstVoidPtr = msg![env; data bytes];
    let length: NSUInteger = msg![env; data length];

    if length == 0 {
        let error = make_parse_error(env, 0, "Data is empty".to_string());
        env.objc.borrow_mut::<NSXMLParserHostObject>(this).error = error;
        return false;
    }

    log_dbg!("Parsing XML data ({} bytes)...", length);
    let bytes: &[u8] = env.mem.bytes_at_mut(bytes.cast().cast_mut(), length);

    // ── Phase 1: Collect events with lenient parsing ──────────────────
    let mut reader = Reader::from_reader(bytes);

    // KEY FIX: Disable strict end-tag matching. Many iOS game XML files
    // have mismatched tags (e.g. <Object> closed with </Resources>).
    // Apple's NSXMLParser is also lenient about this in practice.
    reader.config_mut().check_end_names = false;
    // Trim whitespace in closing tag names for extra robustness.
    reader.config_mut().trim_markup_names_in_closing_tags = true;

    let mut events: Vec<Event<'_>> = Vec::new();
    let mut had_error = false;
    let mut error_position: i32 = 0;
    let mut error_message: String = String::new();
    let mut consecutive_errors: usize = 0;

    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(e) => {
                events.push(e.into_owned());
                consecutive_errors = 0;
            }
            Err(e) => {
                let pos = reader.error_position();
                log!(
                    "XML Parse Error at position {}: {:?} (attempting recovery, event count so far: {})",
                    pos,
                    e,
                    events.len()
                );

                // Remember the first error for reporting
                if !had_error {
                    had_error = true;
                    error_position = pos as i32;
                    error_message = format!("{:?}", e);
                }

                consecutive_errors += 1;
                if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                    log!(
                        "XML Parse: {} consecutive errors, stopping recovery at position {}",
                        consecutive_errors,
                        pos
                    );
                    break;
                }

                // For MismatchedEndTag and similar structural errors,
                // quick_xml has already consumed the problematic token,
                // so continuing is safe. For other errors (e.g. invalid
                // bytes), the reader may re-try the same position, which
                // is why we limit consecutive errors above.
                continue;
            }
        }
    }

    log_dbg!(
        "XML event collection complete: {} events, had_error={}",
        events.len(),
        had_error
    );

    // ── Phase 2: Deliver events to delegate ───────────────────────────
    let delegate = env.objc.borrow::<NSXMLParserHostObject>(this).delegate;

    let sel_did_start: SEL = env.objc.register_host_selector(
        "parserDidStartDocument:".to_string(),
        &mut env.mem,
    );
    if delegate != nil && msg![env; delegate respondsToSelector:sel_did_start] {
        () = msg![env; delegate parserDidStartDocument:this];
    }

    // The delegate can change inside any callback (SMXMLDocument hands off
    // to child elements this way). Resolve it again for every notification.
    // Swift Foundation XMLParser.swift likewise reads parser.delegate at
    // each SAX callback: https://github.com/swiftlang/swift-corelibs-foundation/blob/main/Sources/FoundationXML/XMLParser.swift
    for event in &events {
        let delegate = env.objc.borrow::<NSXMLParserHostObject>(this).delegate;
        match event {
            Event::Empty(e) => {
                let name = safe_local_name(e.local_name().as_ref());
                let name_id: id = from_rust_string(env, name);
                let name_id = autorelease(env, name_id);

                let sel_start: SEL = env.objc.register_host_selector(
                    "parser:didStartElement:namespaceURI:qualifiedName:attributes:".to_string(),
                    &mut env.mem,
                );
                if delegate != nil && msg![env; delegate respondsToSelector:sel_start] {
                    let dict = build_attributes_dict(env, e);
                    () = msg![env; delegate parser:this
                                   didStartElement:name_id
                                      namespaceURI:nil
                                     qualifiedName:name_id
                                        attributes:dict];
                }

                let delegate = env.objc.borrow::<NSXMLParserHostObject>(this).delegate;
                let sel_end: SEL = env.objc.register_host_selector(
                    "parser:didEndElement:namespaceURI:qualifiedName:".to_string(),
                    &mut env.mem,
                );
                if delegate != nil && msg![env; delegate respondsToSelector:sel_end] {
                    () = msg![env; delegate parser:this
                                     didEndElement:name_id
                                      namespaceURI:nil
                                     qualifiedName:name_id];
                }
            }
            Event::Text(e) => {
                if let Some(text) = safe_decode_text(e) {
                    if !text.is_empty() && text != "\0" {
                        let sel_chars: SEL = env.objc.register_host_selector(
                            "parser:foundCharacters:".to_string(),
                            &mut env.mem,
                        );
                        if delegate != nil && msg![env; delegate respondsToSelector:sel_chars] {
                            let chars = from_rust_string(env, text);
                            let chars = autorelease(env, chars);
                            () = msg![env; delegate parser:this foundCharacters:chars];
                        }
                    }
                }
            }
            Event::Start(e) => {
                let name = safe_local_name(e.local_name().as_ref());
                let name_id: id = from_rust_string(env, name);
                let name_id = autorelease(env, name_id);

                let sel_start: SEL = env.objc.register_host_selector(
                    "parser:didStartElement:namespaceURI:qualifiedName:attributes:".to_string(),
                    &mut env.mem,
                );
                if delegate != nil && msg![env; delegate respondsToSelector:sel_start] {
                    let dict = build_attributes_dict(env, e);
                    () = msg![env; delegate parser:this
                                   didStartElement:name_id
                                      namespaceURI:nil
                                     qualifiedName:name_id
                                        attributes:dict];
                }
            }
            Event::End(e) => {
                let name = safe_local_name(e.local_name().as_ref());
                let name_id: id = from_rust_string(env, name);
                let name_id = autorelease(env, name_id);

                let sel_end: SEL = env.objc.register_host_selector(
                    "parser:didEndElement:namespaceURI:qualifiedName:".to_string(),
                    &mut env.mem,
                );
                if delegate != nil && msg![env; delegate respondsToSelector:sel_end] {
                    () = msg![env; delegate parser:this
                                     didEndElement:name_id
                                      namespaceURI:nil
                                     qualifiedName:name_id];
                }
            }
            Event::CData(e) => {
                if let Ok(text) = e.decode() {
                    let text = text.to_string();
                    if !text.is_empty() {
                        let sel_chars: SEL = env.objc.register_host_selector(
                            "parser:foundCharacters:".to_string(),
                            &mut env.mem,
                        );
                        if delegate != nil && msg![env; delegate respondsToSelector:sel_chars] {
                            let text_id = from_rust_string(env, text);
                            let text_id = autorelease(env, text_id);
                            () = msg![env; delegate parser:this foundCharacters:text_id];
                        }
                    }
                }
            }
            Event::Comment(e) => {
                if let Ok(comment) = e.decode() {
                    let comment = comment.to_string();
                    let sel_comment: SEL = env.objc.register_host_selector(
                        "parser:foundComment:".to_string(),
                        &mut env.mem,
                    );
                    if delegate != nil && msg![env; delegate respondsToSelector:sel_comment] {
                        let comment_id = from_rust_string(env, comment);
                        let comment_id = autorelease(env, comment_id);
                        () = msg![env; delegate parser:this foundComment:comment_id];
                    }
                }
            }
            _ => {}
        }
    }

    // ── Phase 3: Finalize ─────────────────────────────────────────────
    let delegate = env.objc.borrow::<NSXMLParserHostObject>(this).delegate;
    let sel_did_end: SEL = env.objc.register_host_selector(
        "parserDidEndDocument:".to_string(),
        &mut env.mem,
    );
    if delegate != nil && msg![env; delegate respondsToSelector:sel_did_end] {
        () = msg![env; delegate parserDidEndDocument:this];
    }

    // If there was an error, report it to the delegate but still return
    // true if we managed to collect events. This matches the behaviour
    // where the game gets its data AND is informed about the problem.
    if had_error {
        let error = make_parse_error(env, error_position, error_message);
        env.objc.borrow_mut::<NSXMLParserHostObject>(this).error = error;

        let delegate = env.objc.borrow::<NSXMLParserHostObject>(this).delegate;
        let sel_err: SEL = env.objc.register_host_selector(
            "parser:parseErrorOccurred:".to_string(),
            &mut env.mem,
        );
        if delegate != nil && msg![env; delegate respondsToSelector:sel_err] {
            let err = env.objc.borrow::<NSXMLParserHostObject>(this).error;
            () = msg![env; delegate parser:this parseErrorOccurred:err];
        }

        // Return true if we collected events — the game can use partial data.
        // Return false only if we collected nothing at all.
        if events.is_empty() {
            return false;
        }
    }

    true
}

- (bool)shouldResolveExternalEntities {
        env.objc.borrow::<NSXMLParserHostObject>(this).should_resolve_external_entities
}

- (())dealloc {
    let &NSXMLParserHostObject { data, error, .. } = env.objc.borrow(this);
    release(env, data);
    release(env, error);
    env.objc.dealloc_object(this, &mut env.mem);
}

@end

};

// =========================================================================
// MARK: - Helper functions
// =========================================================================

/// Safely extract the local name from a BytesStart/BytesEnd event,
/// using lossy UTF-8 conversion (replaces invalid bytes with '').
fn safe_local_name(name_bytes: &[u8]) -> String {
    String::from_utf8_lossy(name_bytes).into_owned()
}

/// Safely decode text from a Text event, returning None on failure.
fn safe_decode_text(e: &quick_xml::events::BytesText<'_>) -> Option<String> {
    e.decode().ok().map(|cow| cow.to_string())
}

/// Build an NSDictionary from XML element attributes, gracefully skipping
/// any attributes that fail to parse (bad UTF-8, unescape errors, etc.).
fn build_attributes_dict(env: &mut Environment, e: &BytesStart) -> id {
    let dict: id = msg_class![env; NSMutableDictionary new];

    for attr_result in e.attributes() {
        let attr = match attr_result {
            Ok(a) => a,
            Err(_) => {
                log_dbg!("XML: skipping malformed attribute");
                continue;
            }
        };

        let key_str = String::from_utf8_lossy(attr.key.local_name().as_ref()).into_owned();
        let val_str = match attr.unescape_value() {
            Ok(v) => v.to_string(),
            Err(_) => {
                // Fallback: use raw bytes with lossy conversion
                String::from_utf8_lossy(attr.value.as_ref()).into_owned()
            }
        };

        let key = from_rust_string(env, key_str);
        let val = from_rust_string(env, val_str);
        () = msg![env; dict setObject:val forKey:key];
        release(env, key);
        release(env, val);
    }

    autorelease(env, dict)
}

fn make_parse_error(env: &mut Environment, code: i32, message: String) -> id {
    let domain = from_rust_string(env, "NSXMLParserErrorDomain".to_string());
    let domain = autorelease(env, domain);

    let desc_key = from_rust_string(env, "NSLocalizedDescription".to_string());
    let desc_key = autorelease(env, desc_key);
    let desc_val = from_rust_string(env, message);
    let desc_val = autorelease(env, desc_val);

    let user_info: id = msg_class![env; NSMutableDictionary new];
    let user_info = autorelease(env, user_info);
    () = msg![env; user_info setObject:desc_val forKey:desc_key];

    let error: id = msg_class![env; NSError errorWithDomain:domain
                                                        code:code
                                                    userInfo:user_info];
    retain(env, error);
    error
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use super::super::ns_string::to_rust_string;

    #[derive(Default)]
    struct Delegate {
        events: Vec<String>,
        on_document: id,
        on_start: id,
        on_end: id,
        clear_on_start: bool,
    }
    impl HostObject for Delegate {}

    pub const CLASSES: ClassExports = objc_classes! {
        (env, this, _cmd);
        @implementation _BerryXMLDelegateTest: NSObject
        + (id)allocWithZone:(NSZonePtr)_zone {
            env.objc.alloc_object(this, Box::<Delegate>::default(), &mut env.mem)
        }
        - (())parserDidStartDocument:(id)parser {
            let host = env.objc.borrow_mut::<Delegate>(this);
            host.events.push("document-start".into());
            let next = host.on_document;
            if next != nil { () = msg![env; parser setDelegate:next]; }
        }
        - (())parser:(id)parser didStartElement:(id)name namespaceURI:(id)_uri
                       qualifiedName:(id)_qname attributes:(id)_attributes {
            let name = to_rust_string(env, name).to_string();
            let host = env.objc.borrow_mut::<Delegate>(this);
            host.events.push(format!("start:{name}"));
            let (next, clear) = (host.on_start, host.clear_on_start);
            if next != nil || clear { () = msg![env; parser setDelegate:next]; }
        }
        - (())parser:(id)parser didEndElement:(id)name namespaceURI:(id)_uri qualifiedName:(id)_qname {
            let name = to_rust_string(env, name).to_string();
            let host = env.objc.borrow_mut::<Delegate>(this);
            host.events.push(format!("end:{name}"));
            let next = host.on_end;
            if next != nil { () = msg![env; parser setDelegate:next]; }
        }
        - (())parser:(id)_parser foundCharacters:(id)text {
            let text = to_rust_string(env, text).to_string();
            env.objc.borrow_mut::<Delegate>(this).events.push(format!("text:{text}"));
        }
        - (())parserDidEndDocument:(id)_parser {
            env.objc.borrow_mut::<Delegate>(this).events.push("document-end".into());
        }
        @end
    };

    fn parser(env: &mut Environment) -> id {
        let xml = b"<root><leaf/>payload</root>";
        let bytes = env.mem.alloc(xml.len() as u32);
        env.mem.bytes_at_mut(bytes.cast(), xml.len() as u32).copy_from_slice(xml);
        let data: id = msg_class![env; NSData dataWithBytesNoCopy:bytes
            length:(xml.len() as NSUInteger) freeWhenDone:true];
        let parser: id = msg_class![env; NSXMLParser alloc];
        msg![env; parser initWithData:data]
    }

    #[test]
    fn callbacks_follow_delegate_handoffs_including_empty_elements() {
        let _guard = crate::environment::FRAMEWORK_TEST_LOCK.lock().unwrap();
        let env = &mut Environment::new_for_framework_tests().unwrap();
        let parser = parser(env);
        let a: id = msg_class![env; _BerryXMLDelegateTest new];
        let b: id = msg_class![env; _BerryXMLDelegateTest new];
        let c: id = msg_class![env; _BerryXMLDelegateTest new];
        let d: id = msg_class![env; _BerryXMLDelegateTest new];
        env.objc.borrow_mut::<Delegate>(a).on_document = b;
        env.objc.borrow_mut::<Delegate>(b).on_start = c;
        env.objc.borrow_mut::<Delegate>(c).on_start = d;
        env.objc.borrow_mut::<Delegate>(c).on_end = b;
        env.objc.borrow_mut::<Delegate>(d).on_end = c;
        () = msg![env; parser setDelegate:a];
        let parsed: bool = msg![env; parser parse];
        assert!(parsed);
        assert_eq!(env.objc.borrow::<Delegate>(a).events, ["document-start"]);
        assert_eq!(env.objc.borrow::<Delegate>(b).events, ["start:root", "document-end"]);
        assert_eq!(env.objc.borrow::<Delegate>(c).events, ["start:leaf", "text:payload", "end:root"]);
        assert_eq!(env.objc.borrow::<Delegate>(d).events, ["end:leaf"]);
        for object in [parser, a, b, c, d] { release(env, object); }
    }

    #[test]
    fn clearing_delegate_stops_subsequent_callbacks() {
        let _guard = crate::environment::FRAMEWORK_TEST_LOCK.lock().unwrap();
        let env = &mut Environment::new_for_framework_tests().unwrap();
        let parser = parser(env);
        let delegate: id = msg_class![env; _BerryXMLDelegateTest new];
        env.objc.borrow_mut::<Delegate>(delegate).clear_on_start = true;
        () = msg![env; parser setDelegate:delegate];
        let parsed: bool = msg![env; parser parse];
        assert!(parsed);
        assert_eq!(env.objc.borrow::<Delegate>(delegate).events, ["document-start", "start:root"]);
        release(env, parser);
        release(env, delegate);
    }
}

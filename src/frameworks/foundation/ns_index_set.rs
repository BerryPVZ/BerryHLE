/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSIndexSet`.
//!
//! An immutable collection of unique, sorted, non-negative integer indexes.
//! We back it with a sorted, de-duplicated `Vec<NSUInteger>`, which keeps the
//! semantics exact for the accessor methods apps commonly use.
//!
//! Apple reference:
//! <https://developer.apple.com/documentation/foundation/nsindexset>

use super::ns_string::from_rust_string;
use super::{NSInteger, NSRange, NSUInteger};
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, retain, ClassExports, HostObject, NSZonePtr,
};

/// `NSNotFound` is `NSIntegerMax` (per Apple's `NSObjCRuntime.h`).
const NS_NOT_FOUND: NSUInteger = NSInteger::MAX as NSUInteger;

#[derive(Default)]
struct NSIndexSetHostObject {
    /// Sorted, de-duplicated list of indexes contained in the set.
    indexes: Vec<NSUInteger>,
}
impl HostObject for NSIndexSetHostObject {}

fn set_range(host: &mut NSIndexSetHostObject, location: NSUInteger, length: NSUInteger) {
    host.indexes.clear();
    if length == 0 {
        return;
    }
    // Guard against overflow when location + length would exceed the range.
    let end = (location as u64) + (length as u64);
    let mut i = location as u64;
    while i < end {
        host.indexes.push(i as NSUInteger);
        i += 1;
    }
}

/// Adds one index, keeping the list sorted and de-duplicated.
fn insert_index(host: &mut NSIndexSetHostObject, index: NSUInteger) {
    if let Err(pos) = host.indexes.binary_search(&index) {
        host.indexes.insert(pos, index);
    }
}

/// Adds every index in `location..location + length`, merging with the
/// existing sorted list in one pass.
fn add_range(host: &mut NSIndexSetHostObject, location: NSUInteger, length: NSUInteger) {
    if length == 0 {
        return;
    }
    let start = location as u64;
    let end = start.saturating_add(length as u64);
    let old = std::mem::take(&mut host.indexes);
    let mut merged = Vec::with_capacity(old.len().saturating_add(length as usize));
    let mut next_new = start;
    for index in old {
        let index64 = index as u64;
        while next_new < end && next_new < index64 {
            merged.push(next_new as NSUInteger);
            next_new += 1;
        }
        merged.push(index);
        if index64 >= next_new {
            next_new = index64 + 1;
        }
    }
    while next_new < end {
        merged.push(next_new as NSUInteger);
        next_new += 1;
    }
    host.indexes = merged;
}

/// Removes every index in `location..location + length`.
fn remove_range(host: &mut NSIndexSetHostObject, location: NSUInteger, length: NSUInteger) {
    let start = location as u64;
    let end = start.saturating_add(length as u64);
    host.indexes
        .retain(|&index| (index as u64) < start || (index as u64) >= end);
}

/// Moves every index at or above `index` by `delta`. A negative shift can
/// push an index below zero (Apple drops those) or past an index that stayed
/// put, so the result is sorted again.
fn shift_indexes(host: &mut NSIndexSetHostObject, index: NSUInteger, delta: NSInteger) {
    let mut shifted = Vec::with_capacity(host.indexes.len());
    for &i in &host.indexes {
        if i < index {
            shifted.push(i);
            continue;
        }
        let moved = i as i64 + delta as i64;
        if moved >= 0 {
            shifted.push(moved as NSUInteger);
        }
    }
    shifted.sort_unstable();
    shifted.dedup();
    host.indexes = shifted;
}

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

@implementation NSIndexSet: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<NSIndexSetHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

// MARK: - Constructors

+ (id)indexSet {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new init];
    autorelease(env, new)
}

+ (id)indexSetWithIndex:(NSUInteger)index {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new initWithIndex:index];
    autorelease(env, new)
}

+ (id)indexSetWithIndexesInRange:(NSRange)range {
    let new: id = msg![env; this alloc];
    let new: id = msg![env; new init];
    {
        let host = env.objc.borrow_mut::<NSIndexSetHostObject>(new);
        set_range(host, range.location, range.length);
    }
    autorelease(env, new)
}

// MARK: - Init

- (id)init {
    env.objc.borrow_mut::<NSIndexSetHostObject>(this).indexes = Vec::new();
    this
}

- (id)initWithIndex:(NSUInteger)index {
    env.objc.borrow_mut::<NSIndexSetHostObject>(this).indexes = vec![index];
    this
}

- (id)initWithIndexesInRange:(NSRange)range {
    let host = env.objc.borrow_mut::<NSIndexSetHostObject>(this);
    set_range(host, range.location, range.length);
    this
}

- (id)initWithIndexSet:(id)other { // NSIndexSet*
    let indexes = if other == nil {
        Vec::new()
    } else {
        env.objc.borrow::<NSIndexSetHostObject>(other).indexes.clone()
    };
    env.objc.borrow_mut::<NSIndexSetHostObject>(this).indexes = indexes;
    this
}

// MARK: - Dealloc

- (())dealloc {
    env.objc.dealloc_object(this, &mut env.mem)
}

// MARK: - Count / membership

- (NSUInteger)count {
    env.objc.borrow::<NSIndexSetHostObject>(this).indexes.len() as NSUInteger
}

- (bool)containsIndex:(NSUInteger)index {
    env.objc.borrow::<NSIndexSetHostObject>(this).indexes.contains(&index)
}

- (NSUInteger)countOfIndexesInRange:(NSRange)range {
    let end = (range.location as u64) + (range.length as u64);
    env.objc
        .borrow::<NSIndexSetHostObject>(this)
        .indexes
        .iter()
        .filter(|&&i| (i as u64) >= (range.location as u64) && (i as u64) < end)
        .count() as NSUInteger
}

// MARK: - Boundaries

- (NSUInteger)firstIndex {
    let host = env.objc.borrow::<NSIndexSetHostObject>(this);
    *host.indexes.first().unwrap_or(&NS_NOT_FOUND)
}

- (NSUInteger)lastIndex {
    let host = env.objc.borrow::<NSIndexSetHostObject>(this);
    *host.indexes.last().unwrap_or(&NS_NOT_FOUND)
}

- (NSUInteger)indexGreaterThanIndex:(NSUInteger)index {
    let host = env.objc.borrow::<NSIndexSetHostObject>(this);
    host.indexes
        .iter()
        .copied()
        .find(|&i| i > index)
        .unwrap_or(NS_NOT_FOUND)
}

- (NSUInteger)indexLessThanIndex:(NSUInteger)index {
    let host = env.objc.borrow::<NSIndexSetHostObject>(this);
    host.indexes
        .iter()
        .copied()
        .rev()
        .find(|&i| i < index)
        .unwrap_or(NS_NOT_FOUND)
}

- (NSUInteger)indexGreaterThanOrEqualToIndex:(NSUInteger)index {
    let host = env.objc.borrow::<NSIndexSetHostObject>(this);
    host.indexes
        .iter()
        .copied()
        .find(|&i| i >= index)
        .unwrap_or(NS_NOT_FOUND)
}

- (NSUInteger)indexLessThanOrEqualToIndex:(NSUInteger)index {
    let host = env.objc.borrow::<NSIndexSetHostObject>(this);
    host.indexes
        .iter()
        .copied()
        .rev()
        .find(|&i| i <= index)
        .unwrap_or(NS_NOT_FOUND)
}

// MARK: - Equality

- (bool)isEqualToIndexSet:(id)other { // NSIndexSet*
    if this == other { return true; }
    if other == nil  { return false; }
    let a = env.objc.borrow::<NSIndexSetHostObject>(this).indexes.clone();
    let b = env.objc.borrow::<NSIndexSetHostObject>(other);
    a == b.indexes
}

- (bool)isEqual:(id)other {
    if this == other { return true; }
    if other == nil  { return false; }
    let cls: id = msg_class![env; NSIndexSet class];
    let is_kind: bool = msg![env; other isKindOfClass:cls];
    if !is_kind {
        return false;
    }
    let a = env.objc.borrow::<NSIndexSetHostObject>(this).indexes.clone();
    let b = env.objc.borrow::<NSIndexSetHostObject>(other);
    a == b.indexes
}

- (NSUInteger)hash {
    env.objc.borrow::<NSIndexSetHostObject>(this).indexes.len() as NSUInteger
}

// MARK: - NSCopying

- (id)copyWithZone:(NSZonePtr)_zone {
    // NSIndexSet is immutable — return self retained.
    retain(env, this)
}

- (id)mutableCopyWithZone:(NSZonePtr)_zone {
    let new: id = msg_class![env; NSMutableIndexSet alloc];
    let idxs = env.objc.borrow::<NSIndexSetHostObject>(this).indexes.clone();
    env.objc.borrow_mut::<NSIndexSetHostObject>(new).indexes = idxs;
    autorelease(env, new)
}

// MARK: - NSCoding

- (id)initWithCoder:(id)coder {
    let key = crate::frameworks::foundation::ns_string::get_static_str(env, "NSIndexSetData");
    let data: id = msg![env; coder decodeObjectForKey:key];
    if data != nil {
        let len: NSUInteger = msg![env; data length];
        let count = len as usize / std::mem::size_of::<NSUInteger>();
        let bytes_ptr: crate::mem::ConstVoidPtr = msg![env; data bytes];
        let mut indexes = Vec::with_capacity(count);
        for i in 0..count as u32 {
            indexes.push(env.mem.read(bytes_ptr.cast::<NSUInteger>() + i));
        }
        indexes.sort_unstable();
        indexes.dedup();
        env.objc.borrow_mut::<NSIndexSetHostObject>(this).indexes = indexes;
    }
    this
}

- (())encodeWithCoder:(id)coder {
    let host = env.objc.borrow::<NSIndexSetHostObject>(this);
    let byte_len = (host.indexes.len() * std::mem::size_of::<NSUInteger>()) as NSUInteger;
    let buf: crate::mem::MutVoidPtr = env.mem.alloc(byte_len as u32).cast();
    for (i, &index) in host.indexes.iter().enumerate() {
        env.mem.write(buf.cast::<NSUInteger>() + i as u32, index);
    }
    let data: id = msg_class![env; NSData dataWithBytesNoCopy:buf length:byte_len];
    let key = crate::frameworks::foundation::ns_string::get_static_str(env, "NSIndexSetData");
    () = msg![env; coder encodeObject:data forKey:key];
}

// MARK: - Description

- (id)description {
    let host = env.objc.borrow::<NSIndexSetHostObject>(this);
    let parts: Vec<String> = host.indexes.iter().map(|i| i.to_string()).collect();
    let s = format!("<NSIndexSet: {} index(es) [{}]>", parts.len(), parts.join(", "));
    let ns = from_rust_string(env, s);
    autorelease(env, ns)
}

@end

// =========================================================================
// MARK: - NSMutableIndexSet
// =========================================================================

@implementation NSMutableIndexSet: NSIndexSet

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::<NSIndexSetHostObject>::default();
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

// MARK: - Mutation

- (())addIndex:(NSUInteger)index {
    let host = env.objc.borrow_mut::<NSIndexSetHostObject>(this);
    insert_index(host, index);
}

- (())addIndexesInRange:(NSRange)range {
    let host = env.objc.borrow_mut::<NSIndexSetHostObject>(this);
    add_range(host, range.location, range.length);
}

- (())addIndexes:(id)other { // NSIndexSet*
    if other == nil {
        return;
    }
    let indexes = {
        let other = env.objc.borrow::<NSIndexSetHostObject>(other);
        other.indexes.clone()
    };
    let host = env.objc.borrow_mut::<NSIndexSetHostObject>(this);
    host.indexes.extend_from_slice(&indexes);
    host.indexes.sort_unstable();
    host.indexes.dedup();
}

- (())removeIndex:(NSUInteger)index {
    let host = env.objc.borrow_mut::<NSIndexSetHostObject>(this);
    if let Ok(pos) = host.indexes.binary_search(&index) {
        host.indexes.remove(pos);
    }
}

- (())removeIndexesInRange:(NSRange)range {
    let host = env.objc.borrow_mut::<NSIndexSetHostObject>(this);
    remove_range(host, range.location, range.length);
}

- (())removeIndexes:(id)other { // NSIndexSet*
    if other == nil {
        return;
    }
    let indexes = {
        let other = env.objc.borrow::<NSIndexSetHostObject>(other);
        other.indexes.clone()
    };
    let host = env.objc.borrow_mut::<NSIndexSetHostObject>(this);
    host.indexes
        .retain(|index| indexes.binary_search(index).is_err());
}

- (())removeAllIndexes {
    env.objc.borrow_mut::<NSIndexSetHostObject>(this).indexes.clear();
}

- (())shiftIndexesStartingAtIndex:(NSUInteger)index by:(NSInteger)delta {
    let host = env.objc.borrow_mut::<NSIndexSetHostObject>(this);
    shift_indexes(host, index, delta);
}

// MARK: - NSCopying

- (id)copyWithZone:(NSZonePtr)_zone {
    let indexes = env.objc.borrow::<NSIndexSetHostObject>(this).indexes.clone();
    let new: id = msg_class![env; NSIndexSet alloc];
    env.objc.borrow_mut::<NSIndexSetHostObject>(new).indexes = indexes;
    autorelease(env, new)
}

- (id)mutableCopyWithZone:(NSZonePtr)_zone {
    retain(env, this)
}

@end

};

#[cfg(test)]
mod tests {
    use super::*;

    fn host(indexes: Vec<NSUInteger>) -> NSIndexSetHostObject {
        NSIndexSetHostObject { indexes }
    }

    #[test]
    fn insert_index_keeps_the_list_sorted_and_unique() {
        let mut set = host(vec![1, 4]);
        insert_index(&mut set, 3);
        insert_index(&mut set, 4);
        assert_eq!(set.indexes, vec![1, 3, 4]);
    }

    #[test]
    fn add_range_merges_with_the_existing_indexes() {
        let mut set = host(vec![1, 4]);
        add_range(&mut set, 2, 3);
        assert_eq!(set.indexes, vec![1, 2, 3, 4]);
        add_range(&mut set, 4, 2);
        assert_eq!(set.indexes, vec![1, 2, 3, 4, 5]);
        add_range(&mut set, 0, 0);
        assert_eq!(set.indexes, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn shift_moves_indexes_at_or_above_the_boundary() {
        let mut set = host(vec![1, 2, 5]);
        shift_indexes(&mut set, 2, 3);
        assert_eq!(set.indexes, vec![1, 5, 8]);
        shift_indexes(&mut set, 5, -4);
        assert_eq!(set.indexes, vec![1, 4]);
        shift_indexes(&mut set, 0, -5);
        assert!(set.indexes.is_empty());
    }

    #[test]
    fn remove_range_removes_only_the_indexes_inside_it() {
        let mut set = host(vec![0, 1, 2, 3, 4]);
        remove_range(&mut set, 1, 3);
        assert_eq!(set.indexes, vec![0, 4]);
        remove_range(&mut set, 0, 0);
        assert_eq!(set.indexes, vec![0, 4]);
    }

    #[test]
    fn keyed_archive_round_trips_a_mutable_index_set() {
        let _lock = crate::environment::ENVIRONMENT_TEST_LOCK.lock().unwrap();
        let mut environment = crate::environment::touch_test_environment();
        let env = &mut environment;
        let pool: id = msg_class![env; NSAutoreleasePool new];
        let set: id = msg_class![env; NSMutableIndexSet indexSet];
        () = msg![env; set addIndex:1];
        () = msg![env; set addIndex:7];
        let range = NSRange { location: 3, length: 2 };
        () = msg![env; set addIndexesInRange:range];
        let archived: id = msg_class![env; NSKeyedArchiver archivedDataWithRootObject:set];
        assert_ne!(archived, nil);
        let restored: id = msg_class![env; NSKeyedUnarchiver unarchiveObjectWithData:archived];
        assert_ne!(restored, nil);
        let equal: bool = msg![env; restored isEqualToIndexSet:set];
        assert!(equal);
        let count: NSUInteger = msg![env; restored count];
        assert_eq!(count, 4);

        let empty: id = msg_class![env; NSMutableIndexSet indexSet];
        let archived_empty: id = msg_class![env; NSKeyedArchiver archivedDataWithRootObject:empty];
        let restored_empty: id =
            msg_class![env; NSKeyedUnarchiver unarchiveObjectWithData:archived_empty];
        assert_ne!(restored_empty, nil);
        let empty_count: NSUInteger = msg![env; restored_empty count];
        assert_eq!(empty_count, 0);
        () = msg![env; pool drain];
    }
}

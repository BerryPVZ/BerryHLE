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
    fn remove_range_removes_only_the_indexes_inside_it() {
        let mut set = host(vec![0, 1, 2, 3, 4]);
        remove_range(&mut set, 1, 3);
        assert_eq!(set.indexes, vec![0, 4]);
        remove_range(&mut set, 0, 0);
        assert_eq!(set.indexes, vec![0, 4]);
    }
}

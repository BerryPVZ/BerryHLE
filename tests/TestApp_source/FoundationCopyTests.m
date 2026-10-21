/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
#ifdef DEFINE_ME_WHEN_BUILDING_ON_MACOS
#import <Foundation/Foundation.h>
#else
#import "system_headers.h"
@interface NSObject (FoundationCopyTests)
- (id)copy;
@end
@interface NSHashTable : NSObject
- (id)initWithOptions:(NSUInteger)options capacity:(NSUInteger)capacity;
- (void)addObject:(id)object;
- (void)removeObject:(id)object;
- (void)removeAllObjects;
- (NSUInteger)count;
- (id)member:(id)object;
@end
#endif
#include <string.h>

static int copy_members_freed;
@interface HashTableCopyMember : NSObject
@end
@implementation HashTableCopyMember
- (void)dealloc {
  copy_members_freed++;
  [super dealloc];
}
@end

int test_NSHashTable_copy_is_independent(void) {
  NSAutoreleasePool *pool = [[NSAutoreleasePool alloc] init];
  NSHashTable *table = [[NSHashTable alloc] initWithOptions:0 capacity:2];
  id first = [[HashTableCopyMember alloc] init];
  id second = [[NSObject alloc] init];
  [table addObject:first];
  NSHashTable *copy = [table copy];
  int result = 0;
  if (copy == table) result = -1;
  else if ([copy count] != 1 || [copy member:first] != first) result = -2;
  [table addObject:second];
  if (!result && ([table count] != 2 || [copy count] != 1)) result = -3;
  [copy removeObject:first];
  if (!result && ([table count] != 2 || [copy count] != 0)) result = -4;
  [copy release];
  [table release];
  [first release];
  [second release];
  [pool drain];
  return result;
}

int test_NSHashTable_copy_owns_members(void) {
  NSAutoreleasePool *pool = [[NSAutoreleasePool alloc] init];
  NSHashTable *table = [[NSHashTable alloc] initWithOptions:0 capacity:1];
  id member = [[HashTableCopyMember alloc] init];
  copy_members_freed = 0;
  [table addObject:member];
  NSAutoreleasePool *inner = [[NSAutoreleasePool alloc] init];
  NSHashTable *copy = [table copy];
  [table removeAllObjects];
  [member release];
  [table release];
  [inner drain];
  int result = 0;
  if (copy_members_freed != 0) result = -1;
  else if ([copy count] != 1 || [copy member:member] != member) result = -2;
  [copy release];
  if (!result && copy_members_freed != 1) result = -3;
  [pool drain];
  return result;
}

int test_NSHashTable_copy_empty(void) {
  NSAutoreleasePool *pool = [[NSAutoreleasePool alloc] init];
  NSHashTable *table = [[NSHashTable alloc] initWithOptions:0 capacity:0];
  NSHashTable *copy = [table copy];
  id member = [[NSObject alloc] init];
  [copy addObject:member];
  int result = copy != table && [table count] == 0 && [copy count] == 1 ? 0 : -1;
  [copy release];
  [table release];
  [member release];
  [pool drain];
  return result;
}


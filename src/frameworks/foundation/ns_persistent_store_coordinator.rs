/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! `NSPersistentStoreCoordinator` and related Core Data classes.
//! Attribute models with SQLite and in-memory persistence.

#[path = "core_data_runtime.rs"]
mod runtime;

use crate::dyld::{ConstantExports, HostConstant};
use crate::frameworks::foundation::ns_string;
use crate::frameworks::foundation::{NSInteger, NSUInteger};
use crate::mem::MutPtr;
use crate::objc::{
    autorelease, id, msg, msg_class, nil, objc_classes, release, retain, ClassExports, HostObject,
    NSZonePtr,
};

// MARK: - NSPersistentStoreCoordinator

#[derive(Default)]
struct NSPersistentStoreCoordinatorHostObject {
    /// `NSManagedObjectModel*`
    managed_object_model: id,
    /// `NSArray<NSPersistentStore*>*`
    persistent_stores: id,
}
impl HostObject for NSPersistentStoreCoordinatorHostObject {}

// MARK: - NSPersistentStore

#[derive(Default)]
struct NSPersistentStoreHostObject {
    /// `NSString*`
    type_: id,
    /// `NSURL*`
    url: id,
    /// `NSDictionary*`
    options: id,
    storage: Option<super::core_data_store::Store>,
    /// `NSPersistentStoreCoordinator*` — weak
    coordinator: id,
}
impl HostObject for NSPersistentStoreHostObject {}

// MARK: - NSManagedObjectModel

#[derive(Default)]
struct NSManagedObjectModelHostObject {
    /// `NSArray<NSEntityDescription*>*`
    entities: id,
}
impl HostObject for NSManagedObjectModelHostObject {}

// MARK: - NSManagedObjectContext

#[derive(Default)]
struct NSManagedObjectContextHostObject {
    /// `NSPersistentStoreCoordinator*`
    persistent_store_coordinator: id,
    /// `NSManagedObjectContext*` — parent context (weak)
    parent_context: id,
    undo_manager: id,
    merge_policy: id,
    objects: Vec<id>,
}
impl HostObject for NSManagedObjectContextHostObject {}

/// For failure paths only: on success, Cocoa leaves `*error` alone and
/// callers may pass an uninitialized pointer.
fn clear_error_out_parameter(env: &mut crate::Environment, error: MutPtr<id>) {
    if !error.is_null() {
        env.mem.write(error, nil);
    }
}

// MARK: - NSManagedObject

#[derive(Default)]
struct NSManagedObjectHostObject {
    /// `NSEntityDescription*`
    entity: id,
    /// `NSManagedObjectContext*` — weak
    managed_object_context: id,
    values: std::collections::HashMap<String,id>,
    row_id: i64,
    inserted: bool,
    dirty: bool,
    deleted: bool,
}
impl HostObject for NSManagedObjectHostObject {}

// MARK: - NSEntityDescription

#[derive(Default)]
struct NSEntityDescriptionHostObject {
    /// `NSString*`
    name: id,
    /// `NSString*`
    managed_object_class_name: id,
    /// `NSDictionary<NSString*, NSPropertyDescription*>*`
    properties_by_name: id,
}
impl HostObject for NSEntityDescriptionHostObject {}

// MARK: - NSFetchRequest

#[derive(Default)]
struct NSFetchRequestHostObject {
    /// `NSEntityDescription*`
    entity: id,
    /// `NSString*`
    entity_name: id,
    /// `NSPredicate*`
    predicate: id,
    /// `NSArray<NSSortDescriptor*>*`
    sort_descriptors: id,
    fetch_limit: u64,
    fetch_offset: u64,
    fetch_batch_size: u64,
    returns_objects_as_faults: bool,
    includes_subentities: bool,
    result_type: NSUInteger,
    unsupported_options: bool,
}
impl HostObject for NSFetchRequestHostObject {}

// MARK: - NSPropertyDescription

#[derive(Default)]
struct NSPropertyDescriptionHostObject {
    /// `NSString*`
    name: id,
    optional: bool,
    transient: bool,
    /// `NSDictionary*`
    user_info: id,
    entity: id,
}
impl HostObject for NSPropertyDescriptionHostObject {}

// MARK: - NSAttributeDescription

#[derive(Default)]
struct NSAttributeDescriptionHostObject {
    superclass: NSPropertyDescriptionHostObject,
    /// NSAttributeType (integer enum)
    attribute_type: NSInteger,
    /// `id` default value
    default_value: id,
    allows_external_binary_data_storage: bool,
}
crate::impl_HostObject_with_superclass!(NSAttributeDescriptionHostObject);

// Apple Core Data — exported `NSString * const` identifiers used as
// store-type tags, error-domain names, store-option keys, notification
// names, and `userInfo` dictionary keys. The literal values must match
// the system's, because apps compare them with `isEqualToString:` and
// `CFStringCompare()` (e.g. `[type isEqualToString:NSSQLiteStoreType]`
// to pick a SQLite store, or `[error.domain
// isEqualToString:NSSQLiteErrorDomain]` to filter SQL errors).
//
// Reference: Apple Core Data Programming Guide,
// `NSPersistentStoreCoordinator.h` and `NSManagedObjectContext.h`.
pub const CONSTANTS: ConstantExports = &[
    // Store-type identifiers
    ("_NSSQLiteStoreType", HostConstant::NSString("SQLite")),
    ("_NSXMLStoreType", HostConstant::NSString("XML")),
    ("_NSBinaryStoreType", HostConstant::NSString("Binary")),
    ("_NSInMemoryStoreType", HostConstant::NSString("InMemory")),
    // Error domain
    (
        "_NSSQLiteErrorDomain",
        HostConstant::NSString("NSSQLiteErrorDomain"),
    ),
    // NSError userInfo keys
    (
        "_NSDetailedErrorsKey",
        HostConstant::NSString("NSDetailedErrorsKey"),
    ),
    (
        "_NSValidationObjectErrorKey",
        HostConstant::NSString("NSValidationObjectErrorKey"),
    ),
    (
        "_NSValidationKeyErrorKey",
        HostConstant::NSString("NSValidationKeyErrorKey"),
    ),
    (
        "_NSValidationPredicateErrorKey",
        HostConstant::NSString("NSValidationPredicateErrorKey"),
    ),
    (
        "_NSValidationValueErrorKey",
        HostConstant::NSString("NSValidationValueErrorKey"),
    ),
    (
        "_NSAffectedStoresErrorKey",
        HostConstant::NSString("NSAffectedStoresErrorKey"),
    ),
    (
        "_NSAffectedObjectsErrorKey",
        HostConstant::NSString("NSAffectedObjectsErrorKey"),
    ),
    (
        "_NSPersistentStoreSaveConflictsErrorKey",
        HostConstant::NSString("NSPersistentStoreSaveConflictsErrorKey"),
    ),
    // Persistent-store option keys
    (
        "_NSReadOnlyPersistentStoreOption",
        HostConstant::NSString("NSReadOnlyPersistentStoreOption"),
    ),
    (
        "_NSPersistentStoreTimeoutOption",
        HostConstant::NSString("NSPersistentStoreTimeoutOption"),
    ),
    (
        "_NSSQLitePragmasOption",
        HostConstant::NSString("NSSQLitePragmasOption"),
    ),
    (
        "_NSSQLiteAnalyzeOption",
        HostConstant::NSString("NSSQLiteAnalyzeOption"),
    ),
    (
        "_NSSQLiteManualVacuumOption",
        HostConstant::NSString("NSSQLiteManualVacuumOption"),
    ),
    (
        "_NSIgnorePersistentStoreVersioningOption",
        HostConstant::NSString("NSIgnorePersistentStoreVersioningOption"),
    ),
    (
        "_NSMigratePersistentStoresAutomaticallyOption",
        HostConstant::NSString("NSMigratePersistentStoresAutomaticallyOption"),
    ),
    (
        "_NSInferMappingModelAutomaticallyOption",
        HostConstant::NSString("NSInferMappingModelAutomaticallyOption"),
    ),
    (
        "_NSStoreUUIDInMetadataKey",
        HostConstant::NSString("NSStoreUUID"),
    ),
    ("_NSStoreTypeKey", HostConstant::NSString("NSStoreType")),
    (
        "_NSStoreModelVersionHashesKey",
        HostConstant::NSString("NSStoreModelVersionHashes"),
    ),
    (
        "_NSStoreModelVersionIdentifiersKey",
        HostConstant::NSString("NSStoreModelVersionIdentifiers"),
    ),
    (
        "_NSPersistentStoreOSCompatibility",
        HostConstant::NSString("NSPersistentStoreOSCompatibility"),
    ),
    // Merge-policy singletons. On real iOS these are `id`
    // (NSMergePolicy*) constants. We expose unique non-NULL slots so
    // identity comparisons (`policy == NSOverwriteMergePolicy`) work.
    // The stub NSManagedObjectContext accepts any non-NULL id as the
    // mergePolicy.
    ("_NSErrorMergePolicy", HostConstant::NullPtr),
    (
        "_NSMergeByPropertyStoreTrumpMergePolicy",
        HostConstant::NullPtr,
    ),
    (
        "_NSMergeByPropertyObjectTrumpMergePolicy",
        HostConstant::NullPtr,
    ),
    ("_NSOverwriteMergePolicy", HostConstant::NullPtr),
    ("_NSRollbackMergePolicy", HostConstant::NullPtr),
    // Notification names
    (
        "_NSManagedObjectContextDidSaveNotification",
        HostConstant::NSString("NSManagedObjectContextDidSaveNotification"),
    ),
    (
        "_NSManagedObjectContextWillSaveNotification",
        HostConstant::NSString("NSManagedObjectContextWillSaveNotification"),
    ),
    (
        "_NSManagedObjectContextObjectsDidChangeNotification",
        HostConstant::NSString("NSManagedObjectContextObjectsDidChangeNotification"),
    ),
    (
        "_NSPersistentStoreCoordinatorStoresDidChangeNotification",
        HostConstant::NSString("NSPersistentStoreCoordinatorStoresDidChangeNotification"),
    ),
    (
        "_NSPersistentStoreCoordinatorStoresWillChangeNotification",
        HostConstant::NSString("NSPersistentStoreCoordinatorStoresWillChangeNotification"),
    ),
    (
        "_NSPersistentStoreCoordinatorWillRemoveStoreNotification",
        HostConstant::NSString("NSPersistentStoreCoordinatorWillRemoveStoreNotification"),
    ),
    (
        "_NSPersistentStoreDidImportUbiquitousContentChangesNotification",
        HostConstant::NSString("NSPersistentStoreDidImportUbiquitousContentChangesNotification"),
    ),
    // Notification userInfo keys
    ("_NSInsertedObjectsKey", HostConstant::NSString("inserted")),
    ("_NSUpdatedObjectsKey", HostConstant::NSString("updated")),
    ("_NSDeletedObjectsKey", HostConstant::NSString("deleted")),
    (
        "_NSRefreshedObjectsKey",
        HostConstant::NSString("refreshed"),
    ),
    (
        "_NSInvalidatedObjectsKey",
        HostConstant::NSString("invalidated"),
    ),
    (
        "_NSInvalidatedAllObjectsKey",
        HostConstant::NSString("invalidatedAll"),
    ),
    (
        "_NSAddedPersistentStoresKey",
        HostConstant::NSString("NSAddedPersistentStoresKey"),
    ),
    (
        "_NSRemovedPersistentStoresKey",
        HostConstant::NSString("NSRemovedPersistentStoresKey"),
    ),
    (
        "_NSUUIDChangedPersistentStoresKey",
        HostConstant::NSString("NSUUIDChangedPersistentStoresKey"),
    ),
    // Ubiquitous (iCloud) Core Data option keys
    (
        "_NSPersistentStoreUbiquitousContentNameKey",
        HostConstant::NSString("NSPersistentStoreUbiquitousContentNameKey"),
    ),
    (
        "_NSPersistentStoreUbiquitousContentURLKey",
        HostConstant::NSString("NSPersistentStoreUbiquitousContentURLKey"),
    ),
    (
        "_NSPersistentStoreUbiquitousPeerTokenOption",
        HostConstant::NSString("NSPersistentStoreUbiquitousPeerTokenOption"),
    ),
    (
        "_NSPersistentStoreRemoveUbiquitousMetadataOption",
        HostConstant::NSString("NSPersistentStoreRemoveUbiquitousMetadataOption"),
    ),
    (
        "_NSPersistentStoreRebuildFromUbiquitousContentOption",
        HostConstant::NSString("NSPersistentStoreRebuildFromUbiquitousContentOption"),
    ),
];

pub const CLASSES: ClassExports = objc_classes! {

(env, this, _cmd);

// =========================================================================
// NSPersistentStoreCoordinator
// =========================================================================

@implementation NSPersistentStoreCoordinator: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let stores = msg_class![env; NSMutableArray new];
    let host_object = Box::new(NSPersistentStoreCoordinatorHostObject {
        managed_object_model: nil,
        persistent_stores: stores,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)metadataForPersistentStoreOfType:(id)_store_type // NSString*
                                   URL:(id)_url        // NSURL*
                                 error:(MutPtr<id>)_error {
    clear_error_out_parameter(env, _error);    // NSError**
    log!("NSPersistentStoreCoordinator metadataForPersistentStoreOfType:URL:error: stubbed");
    nil
}

+ (bool)setMetadata:(id)_metadata               // NSDictionary*
    forPersistentStoreOfType:(id)_store_type    // NSString*
                         URL:(id)_url           // NSURL*
                       error:(MutPtr<id>)_error {
    clear_error_out_parameter(env, _error);       // NSError**
    log!("NSPersistentStoreCoordinator setMetadata:forPersistentStoreOfType:URL:error: stubbed");
    false
}

- (id)initWithManagedObjectModel:(id)model { // NSManagedObjectModel*
    retain(env, model);
    env.objc.borrow_mut::<NSPersistentStoreCoordinatorHostObject>(this)
        .managed_object_model = model;
    this
}

- (())dealloc {
    let host = env.objc.borrow::<NSPersistentStoreCoordinatorHostObject>(this);
    let (model, stores) = (host.managed_object_model, host.persistent_stores);
    release(env, model);
    release(env, stores);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)managedObjectModel {
    env.objc.borrow::<NSPersistentStoreCoordinatorHostObject>(this).managed_object_model
}

- (id)persistentStores {
    env.objc.borrow::<NSPersistentStoreCoordinatorHostObject>(this).persistent_stores
}

- (id)persistentStoreForURL:(id)url { // NSURL*
    let stores = env.objc.borrow::<NSPersistentStoreCoordinatorHostObject>(this).persistent_stores;
    let count: u32 = msg![env; stores count];
    for i in 0..count {
        let store: id = msg![env; stores objectAtIndex:i];
        let store_url: id = msg![env; store URL];
        let eq: bool = msg![env; store_url isEqual:url];
        if eq { return store; }
    }
    nil
}

- (id)URLForPersistentStore:(id)store { // NSPersistentStore*
    msg![env; store URL]
}

- (bool)setURL:(id)_url                   // NSURL*
    forPersistentStore:(id)_store {       // NSPersistentStore*
    log!("NSPersistentStoreCoordinator setURL:forPersistentStore: stubbed -> false");
    false
}

- (id)addPersistentStoreWithType:(id)store_type     // NSString*
                   configuration:(id)_configuration // NSString*
                              URL:(id)url            // NSURL*
                          options:(id)options        // NSDictionary*
                            error:(MutPtr<id>)_error {
    runtime::add_store(env,this,store_type,url,options,_error)
}

- (bool)removePersistentStore:(id)store  // NSPersistentStore*
                        error:(MutPtr<id>)_error {
    let stores = env.objc.borrow::<NSPersistentStoreCoordinatorHostObject>(this).persistent_stores;
    () = msg![env; stores removeObject:store];
    true
}

- (bool)migratePersistentStore:(id)_store  // NSPersistentStore*
                         toURL:(id)_url    // NSURL*
                       options:(id)_opts  // NSDictionary*
                       withType:(id)_type // NSString*
                          error:(MutPtr<id>)_err {
    clear_error_out_parameter(env, _err); // NSError**
    log!("NSPersistentStoreCoordinator migratePersistentStore: stubbed -> false");
    false
}

- (id)metadataForPersistentStore:(id)_store { // NSPersistentStore*
    nil
}

- (())setMetadata:(id)_metadata                 // NSDictionary*
    forPersistentStore:(id)_store {             // NSPersistentStore*
    log!("NSPersistentStoreCoordinator setMetadata:forPersistentStore: stubbed");
}

- (())lock {
    log_dbg!("NSPersistentStoreCoordinator lock: stubbed");
}

- (())unlock {
    log_dbg!("NSPersistentStoreCoordinator unlock: stubbed");
}

- (bool)tryLock {
    true
}

- (id)executeRequest:(id)_request          // NSPersistentStoreRequest*
         withContext:(id)_context          // NSManagedObjectContext*
               error:(MutPtr<id>)_error {
    clear_error_out_parameter(env, _error);         // NSError**
    log!("NSPersistentStoreCoordinator executeRequest:withContext:error: stubbed -> nil");
    nil
}

@end

// =========================================================================
// NSPersistentStore
// =========================================================================

@implementation NSPersistentStore: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSPersistentStoreHostObject {
        type_: nil,
        url: nil,
        options: nil,
        storage: None,
        coordinator: nil,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithPersistentStoreCoordinator:(id)coordinator // NSPersistentStoreCoordinator*
                       configurationName:(id)_config     // NSString*
                                     URL:(id)url         // NSURL*
                                 options:(id)options {   // NSDictionary*
    // coordinator is weak — no retain.
    env.objc.borrow_mut::<NSPersistentStoreHostObject>(this).coordinator = coordinator;
    retain(env, url);
    retain(env, options);
    env.objc.borrow_mut::<NSPersistentStoreHostObject>(this).url     = url;
    env.objc.borrow_mut::<NSPersistentStoreHostObject>(this).options = options;
    this
}

- (())dealloc {
    let host = env.objc.borrow::<NSPersistentStoreHostObject>(this);
    let (type_, url, options) = (host.type_, host.url, host.options);
    release(env, type_);
    release(env, url);
    release(env, options);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)URL {
    env.objc.borrow::<NSPersistentStoreHostObject>(this).url
}

- (id)options {
    env.objc.borrow::<NSPersistentStoreHostObject>(this).options
}

- (id)type {
    env.objc.borrow::<NSPersistentStoreHostObject>(this).type_
}

- (id)persistentStoreCoordinator {
    env.objc.borrow::<NSPersistentStoreHostObject>(this).coordinator
}

- (id)identifier {
    nil
}

- (id)configurationName {
    nil
}

- (bool)isReadOnly {
    false
}

- (())setReadOnly:(bool)_value {
    // Stub.
}

- (id)metadata {
    nil
}

- (())setMetadata:(id)_metadata {
    // Stub.
}

- (bool)didAddToPersistentStoreCoordinator:(id)_coordinator {
    true
}

- (())willRemoveFromPersistentStoreCoordinator:(id)_coordinator {
    // Stub.
}

@end

// =========================================================================
// NSManagedObjectModel
// =========================================================================

@implementation NSManagedObjectModel: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let entities = msg_class![env; NSMutableArray new];
    let host_object = Box::new(NSManagedObjectModelHostObject {
        entities,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)mergedModelFromBundles:(id)_bundles { // NSArray<NSBundle*>*
    runtime::merge_bundles(env,_bundles)
}

+ (id)modelByMergingModels:(id)_models { // NSArray<NSManagedObjectModel*>*
    log!("NSManagedObjectModel modelByMergingModels: stubbed -> empty model");
    let model: id = msg_class![env; NSManagedObjectModel new];
    autorelease(env, model)
}

- (id)init {
    this
}

- (id)initWithContentsOfURL:(id)_url { // NSURL*
    let path:id=msg![env; _url path];
    let path=ns_string::to_rust_string(env,path).into_owned();
    let result=env.fs.read(crate::fs::GuestPath::new(&path)).map_err(|_|"Cannot read managed model".to_string())
        .and_then(|bytes|runtime::load_model(env,this,&bytes));
    if let Err(e)=result{log!("Core Data model: {}",e);release(env,this);nil}else{this}
}

- (())dealloc {
    let entities = env.objc.borrow::<NSManagedObjectModelHostObject>(this).entities;
    release(env, entities);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)entities {
    env.objc.borrow::<NSManagedObjectModelHostObject>(this).entities
}

- (())setEntities:(id)entities {
    let old = env.objc.borrow::<NSManagedObjectModelHostObject>(this).entities;
    release(env, old);
    retain(env, entities);
    env.objc.borrow_mut::<NSManagedObjectModelHostObject>(this).entities = entities;
}

- (id)entitiesByName { runtime::entities_by_name(env,this) }

- (id)configurations {
    msg_class![env; NSArray new]
}

- (id)entitiesForConfiguration:(id)_configuration {
    msg_class![env; NSArray new]
}

- (bool)isConfiguration:(id)_configuration
    compatibleWithStoreMetadata:(id)_metadata {
    true
}

@end

// =========================================================================
// NSManagedObjectContext
// =========================================================================

@implementation NSManagedObjectContext: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSManagedObjectContextHostObject {
        persistent_store_coordinator: nil,
        parent_context: nil,
        undo_manager: nil,
        merge_policy: nil,
        objects: Vec::new(),
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)init {
    this
}

- (id)initWithConcurrencyType:(NSInteger)_concurrency_type {
    this
}

- (())dealloc {
    runtime::reset(env,this);
    let host = env.objc.borrow::<NSManagedObjectContextHostObject>(this);
    let (psc, undo) = (host.persistent_store_coordinator, host.undo_manager);
    release(env, psc);
    release(env, undo);
    // parent_context and merge_policy are weak / unretained
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)persistentStoreCoordinator {
    env.objc.borrow::<NSManagedObjectContextHostObject>(this).persistent_store_coordinator
}

- (())setPersistentStoreCoordinator:(id)coordinator {
    let old = env.objc.borrow::<NSManagedObjectContextHostObject>(this)
        .persistent_store_coordinator;
    release(env, old);
    retain(env, coordinator);
    env.objc.borrow_mut::<NSManagedObjectContextHostObject>(this)
        .persistent_store_coordinator = coordinator;
}

- (id)parentContext {
    env.objc.borrow::<NSManagedObjectContextHostObject>(this).parent_context
}

- (())setParentContext:(id)ctx {
    // Weak — no retain.
    env.objc.borrow_mut::<NSManagedObjectContextHostObject>(this).parent_context = ctx;
}

- (id)undoManager {
    env.objc.borrow::<NSManagedObjectContextHostObject>(this).undo_manager
}

- (())setUndoManager:(id)undo_manager {
    let old = env.objc.borrow::<NSManagedObjectContextHostObject>(this).undo_manager;
    release(env, old);
    retain(env, undo_manager);
    env.objc.borrow_mut::<NSManagedObjectContextHostObject>(this).undo_manager = undo_manager;
}

- (id)mergePolicy {
    env.objc.borrow::<NSManagedObjectContextHostObject>(this).merge_policy
}

- (())setMergePolicy:(id)policy {
    // Weak — no retain.
    env.objc.borrow_mut::<NSManagedObjectContextHostObject>(this).merge_policy = policy;
}

- (bool)hasChanges {
    env.objc.borrow::<NSManagedObjectContextHostObject>(this).objects.iter().any(|object|{
        let o=env.objc.borrow::<NSManagedObjectHostObject>(*object); o.inserted || o.dirty || o.deleted
    })
}

- (bool)save:(MutPtr<id>)_error {
    runtime::save(env,this,_error)
}

- (())reset {
    runtime::reset(env,this);
}

- (())rollback {
    runtime::rollback(env,this);
}

- (())undo {
    log!("NSManagedObjectContext undo: stubbed");
}

- (())redo {
    log!("NSManagedObjectContext redo: stubbed");
}

- (id)executeFetchRequest:(id)_request // NSFetchRequest*
                    error:(MutPtr<id>)_error {
    runtime::fetch(env,this,_request,_error)
}

- (NSUInteger)countForFetchRequest:(id)_request // NSFetchRequest*
                              error:(MutPtr<id>)_error {
    let results=runtime::fetch(env,this,_request,_error);
    if results==nil{NSUInteger::MAX}else{msg![env; results count]}
}

- (id)existingObjectWithID:(id)_object_id // NSManagedObjectID*
                     error:(MutPtr<id>)_error {
    clear_error_out_parameter(env, _error);   // NSError**
    nil
}

- (id)objectWithID:(id)_object_id { // NSManagedObjectID*
    nil
}

- (id)objectRegisteredForID:(id)_object_id { // NSManagedObjectID*
    nil
}

- (())insertObject:(id)_object { // NSManagedObject*
    let objects=&env.objc.borrow::<NSManagedObjectContextHostObject>(this).objects;
    if !objects.contains(&_object){
        retain(env,_object);
        env.objc.borrow_mut::<NSManagedObjectContextHostObject>(this).objects.push(_object);
        let host=env.objc.borrow_mut::<NSManagedObjectHostObject>(_object);
        host.managed_object_context=this;host.inserted=true;host.dirty=true;
        ()=msg![env; _object awakeFromInsert];
    }
}

- (())deleteObject:(id)_object { // NSManagedObject*
    let o=env.objc.borrow_mut::<NSManagedObjectHostObject>(_object);o.deleted=true;o.dirty=true;
}

- (())refreshObject:(id)_object mergeChanges:(bool)_merge {
    log!("NSManagedObjectContext refreshObject:mergeChanges: stubbed");
}

- (())processPendingChanges {
    log_dbg!("NSManagedObjectContext processPendingChanges: stubbed");
}

- (())mergeChangesFromContextDidSaveNotification:(id)_notification {
    log!("NSManagedObjectContext mergeChangesFromContextDidSaveNotification: stubbed");
}

- (())setMergeChangesFromContextDidSaveNotificationOnMain:(bool)_value {
    // Stub.
}

- (())performBlock:(id)_block {
    log!("NSManagedObjectContext performBlock: stubbed (block not called)");
}

- (())performBlockAndWait:(id)_block {
    log!("NSManagedObjectContext performBlockAndWait: stubbed (block not called)");
}

- (id)registeredObjects {
    msg_class![env; NSSet new]
}

- (id)insertedObjects {
    msg_class![env; NSSet new]
}

- (id)updatedObjects {
    msg_class![env; NSSet new]
}

- (id)deletedObjects {
    msg_class![env; NSSet new]
}

@end

// =========================================================================
// NSManagedObject
// =========================================================================

@implementation NSManagedObject: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSManagedObjectHostObject {
        entity: nil,
        managed_object_context: nil,
        ..Default::default()
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (id)initWithEntity:(id)entity                        // NSEntityDescription*
insertIntoManagedObjectContext:(id)context {           // NSManagedObjectContext*
    retain(env, entity);
    env.objc.borrow_mut::<NSManagedObjectHostObject>(this).entity = entity;
    // context is weak.
    env.objc.borrow_mut::<NSManagedObjectHostObject>(this).managed_object_context = context;
    runtime::apply_defaults(env,this);
    if context != nil {
        () = msg![env; context insertObject:this];
    }
    this
}

- (())dealloc {
    let values=std::mem::take(&mut env.objc.borrow_mut::<NSManagedObjectHostObject>(this).values);
    for value in values.into_values(){release(env,value);}
    let entity = env.objc.borrow::<NSManagedObjectHostObject>(this).entity;
    release(env, entity);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)entity {
    env.objc.borrow::<NSManagedObjectHostObject>(this).entity
}

- (id)managedObjectContext {
    env.objc.borrow::<NSManagedObjectHostObject>(this).managed_object_context
}

- (id)objectID {
    nil
}

- (bool)isInserted {env.objc.borrow::<NSManagedObjectHostObject>(this).inserted}
- (bool)isUpdated {let o=env.objc.borrow::<NSManagedObjectHostObject>(this);o.dirty && !o.inserted}
- (bool)isDeleted {env.objc.borrow::<NSManagedObjectHostObject>(this).deleted}
- (bool)isFault      { false }
- (bool)hasChanges {let o=env.objc.borrow::<NSManagedObjectHostObject>(this);o.dirty || o.inserted || o.deleted}

- (id)valueForKey:(id)_key {
    let key=ns_string::to_rust_string(env,_key).into_owned();
    env.objc.borrow::<NSManagedObjectHostObject>(this).values.get(&key).copied().unwrap_or(nil)
}

- (())setValue:(id)_value forKey:(id)_key {
    let key=ns_string::to_rust_string(env,_key).into_owned();
    runtime::set_value(env,this,&key,_value);
}

- (())willAccessValueForKey:(id)_key  {}
- (())didAccessValueForKey:(id)_key   {}
- (())willChangeValueForKey:(id)_key  {}
- (())didChangeValueForKey:(id)_key   {}

- (id)primitiveValueForKey:(id)_key {msg![env; this valueForKey:_key]}
- (())setPrimitiveValue:(id)_value forKey:(id)_key {()=msg![env; this setValue:_value forKey:_key];}

- (id)changedValues         { msg_class![env; NSDictionary new] }
- (id)changedValuesForCurrentEvent { msg_class![env; NSDictionary new] }
- (id)committedValuesForKeys:(id)_keys { msg_class![env; NSDictionary new] }

- (())validateForInsert:(id)_error {}
- (())validateForUpdate:(id)_error {}
- (())validateForDelete:(id)_error {}
- (())awakeFromInsert {}
- (())awakeFromFetch  {}
- (())prepareForDeletion {}
- (())willSave  {}
- (())didSave   {}
- (())willTurnIntoFault {}
- (())didTurnIntoFault  {}

@end

// =========================================================================
// NSEntityDescription
// =========================================================================

@implementation NSEntityDescription: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let props = msg_class![env; NSMutableDictionary new];
    let host_object = Box::new(NSEntityDescriptionHostObject {
        name: nil,
        managed_object_class_name: nil,
        properties_by_name: props,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)entityForName:(id)_name                       // NSString*
    inManagedObjectContext:(id)_context {            // NSManagedObjectContext*
    runtime::entity(env,_context,_name)
}

+ (id)insertNewObjectForEntityForName:(id)_name     // NSString*
             inManagedObjectContext:(id)_context {  // NSManagedObjectContext*
    let entity=runtime::entity(env,_context,_name);
    runtime::insert_new(env,entity,_context)
}

- (())dealloc {
    let host = env.objc.borrow::<NSEntityDescriptionHostObject>(this);
    let (name, class_name, props) = (
        host.name, host.managed_object_class_name, host.properties_by_name
    );
    release(env, name);
    release(env, class_name);
    release(env, props);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)name {
    env.objc.borrow::<NSEntityDescriptionHostObject>(this).name
}

- (())setName:(id)name {
    let old = env.objc.borrow::<NSEntityDescriptionHostObject>(this).name;
    release(env, old);
    retain(env, name);
    env.objc.borrow_mut::<NSEntityDescriptionHostObject>(this).name = name;
}

- (id)managedObjectClassName {
    env.objc.borrow::<NSEntityDescriptionHostObject>(this).managed_object_class_name
}

- (())setManagedObjectClassName:(id)name {
    let old = env.objc.borrow::<NSEntityDescriptionHostObject>(this).managed_object_class_name;
    release(env, old);
    retain(env, name);
    env.objc.borrow_mut::<NSEntityDescriptionHostObject>(this).managed_object_class_name = name;
}

- (id)propertiesByName {
    env.objc.borrow::<NSEntityDescriptionHostObject>(this).properties_by_name
}

- (id)properties {
    let props = env.objc.borrow::<NSEntityDescriptionHostObject>(this).properties_by_name;
    msg![env; props allValues]
}

- (())setProperties:(id)_properties {
    runtime::set_properties(env,this,_properties);
}

- (id)subentities   { msg_class![env; NSArray new] }
- (id)subentitiesByName { msg_class![env; NSDictionary new] }
- (id)superentity   { nil }
- (bool)isAbstract  { false }
- (id)userInfo      { nil }

@end

// =========================================================================
// NSFetchRequest
// =========================================================================

@implementation NSFetchRequest: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSFetchRequestHostObject {
        entity: nil,
        entity_name: nil,
        predicate: nil,
        sort_descriptors: nil,
        fetch_limit: 0,
        fetch_offset: 0,
        fetch_batch_size: 0,
        returns_objects_as_faults: true,
        includes_subentities: true,
        result_type: 0,
        unsupported_options: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

+ (id)fetchRequestWithEntityName:(id)name { // NSString*
    let req: id = msg_class![env; NSFetchRequest alloc];
    let req: id = msg![env; req initWithEntityName:name];
    autorelease(env, req)
}

- (id)initWithEntityName:(id)name { // NSString*
    retain(env, name);
    env.objc.borrow_mut::<NSFetchRequestHostObject>(this).entity_name = name;
    this
}

- (())dealloc {
    let host = env.objc.borrow::<NSFetchRequestHostObject>(this);
    let (entity, entity_name, predicate, sort) = (
        host.entity, host.entity_name, host.predicate, host.sort_descriptors
    );
    release(env, entity);
    release(env, entity_name);
    release(env, predicate);
    release(env, sort);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (NSUInteger)resultType {env.objc.borrow::<NSFetchRequestHostObject>(this).result_type}
- (())setResultType:(NSUInteger)value {env.objc.borrow_mut::<NSFetchRequestHostObject>(this).result_type=value;}

- (id)entity {
    env.objc.borrow::<NSFetchRequestHostObject>(this).entity
}

- (())setEntity:(id)entity {
    let old = env.objc.borrow::<NSFetchRequestHostObject>(this).entity;
    release(env, old);
    retain(env, entity);
    env.objc.borrow_mut::<NSFetchRequestHostObject>(this).entity = entity;
}

- (id)entityName {
    env.objc.borrow::<NSFetchRequestHostObject>(this).entity_name
}

- (id)predicate {
    env.objc.borrow::<NSFetchRequestHostObject>(this).predicate
}

- (())setPredicate:(id)predicate {
    let old = env.objc.borrow::<NSFetchRequestHostObject>(this).predicate;
    release(env, old);
    retain(env, predicate);
    env.objc.borrow_mut::<NSFetchRequestHostObject>(this).predicate = predicate;
}

- (id)sortDescriptors {
    env.objc.borrow::<NSFetchRequestHostObject>(this).sort_descriptors
}

- (())setSortDescriptors:(id)descs {
    let old = env.objc.borrow::<NSFetchRequestHostObject>(this).sort_descriptors;
    release(env, old);
    retain(env, descs);
    env.objc.borrow_mut::<NSFetchRequestHostObject>(this).sort_descriptors = descs;
}

- (NSUInteger)fetchLimit {
    env.objc.borrow::<NSFetchRequestHostObject>(this).fetch_limit as NSUInteger
}

- (())setFetchLimit:(NSUInteger)limit {
    env.objc.borrow_mut::<NSFetchRequestHostObject>(this).fetch_limit = limit as u64;
}

- (NSUInteger)fetchOffset {
    env.objc.borrow::<NSFetchRequestHostObject>(this).fetch_offset as NSUInteger
}

- (())setFetchOffset:(NSUInteger)offset {
    env.objc.borrow_mut::<NSFetchRequestHostObject>(this).fetch_offset = offset as u64;
}

- (NSUInteger)fetchBatchSize {
    env.objc.borrow::<NSFetchRequestHostObject>(this).fetch_batch_size as NSUInteger
}

- (())setFetchBatchSize:(NSUInteger)size {
    env.objc.borrow_mut::<NSFetchRequestHostObject>(this).fetch_batch_size = size as u64;
}

- (bool)returnsObjectsAsFaults {
    env.objc.borrow::<NSFetchRequestHostObject>(this).returns_objects_as_faults
}

- (())setReturnsObjectsAsFaults:(bool)value {
    env.objc.borrow_mut::<NSFetchRequestHostObject>(this).returns_objects_as_faults = value;
}

- (bool)includesSubentities {
    env.objc.borrow::<NSFetchRequestHostObject>(this).includes_subentities
}

- (())setIncludesSubentities:(bool)value {
    env.objc.borrow_mut::<NSFetchRequestHostObject>(this).includes_subentities = value;
}

- (bool)includesPendingChanges    { true  }
- (())setIncludesPendingChanges:(bool)_v {if !_v {env.objc.borrow_mut::<NSFetchRequestHostObject>(this).unsupported_options=true;}}

- (bool)returnsDistinctResults    { false }
- (())setReturnsDistinctResults:(bool)_v {if _v {env.objc.borrow_mut::<NSFetchRequestHostObject>(this).unsupported_options=true;}}

- (id)propertiesToFetch           { nil }
- (())setPropertiesToFetch:(id)_v {if _v!=nil {env.objc.borrow_mut::<NSFetchRequestHostObject>(this).unsupported_options=true;}}

- (id)propertiesToGroupBy         { nil }
- (())setPropertiesToGroupBy:(id)_v {if _v!=nil {env.objc.borrow_mut::<NSFetchRequestHostObject>(this).unsupported_options=true;}}

- (id)havingPredicate             { nil }
- (())setHavingPredicate:(id)_v {if _v!=nil {env.objc.borrow_mut::<NSFetchRequestHostObject>(this).unsupported_options=true;}}



@end

// =========================================================================
// NSPropertyDescription (abstract base for NSAttributeDescription,
// NSRelationshipDescription, NSFetchedPropertyDescription)
// =========================================================================

@implementation NSPropertyDescription: NSObject

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSPropertyDescriptionHostObject {
        name: nil,
        optional: false,
        transient: false,
        user_info: nil,
        entity: nil,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())dealloc {
    let host = env.objc.borrow::<NSPropertyDescriptionHostObject>(this);
    let (name, user_info) = (host.name, host.user_info);
    release(env, name);
    release(env, user_info);
    env.objc.dealloc_object(this, &mut env.mem)
}

- (id)name {
    env.objc.borrow::<NSPropertyDescriptionHostObject>(this).name
}

- (())setName:(id)name {
    let old = env.objc.borrow::<NSPropertyDescriptionHostObject>(this).name;
    release(env, old);
    retain(env, name);
    env.objc.borrow_mut::<NSPropertyDescriptionHostObject>(this).name = name;
}

- (bool)isOptional {
    env.objc.borrow::<NSPropertyDescriptionHostObject>(this).optional
}

- (())setOptional:(bool)optional {
    env.objc.borrow_mut::<NSPropertyDescriptionHostObject>(this).optional = optional;
}

- (bool)isTransient {
    env.objc.borrow::<NSPropertyDescriptionHostObject>(this).transient
}

- (())setTransient:(bool)transient {
    env.objc.borrow_mut::<NSPropertyDescriptionHostObject>(this).transient = transient;
}

- (id)userInfo {
    env.objc.borrow::<NSPropertyDescriptionHostObject>(this).user_info
}

- (())setUserInfo:(id)info {
    let old = env.objc.borrow::<NSPropertyDescriptionHostObject>(this).user_info;
    release(env, old);
    retain(env, info);
    env.objc.borrow_mut::<NSPropertyDescriptionHostObject>(this).user_info = info;
}

- (id)entity {env.objc.borrow::<NSPropertyDescriptionHostObject>(this).entity}
- (id)validationPredicates { msg_class![env; NSArray new] }
- (id)validationWarnings { msg_class![env; NSArray new] }
- (bool)isIndexed { false }
- (())setIndexed:(bool)_v {}
- (bool)isIndexedBySpotlight { false }
- (())setIndexedBySpotlight:(bool)_v {}
- (bool)isStoredInExternalRecord { false }
- (())setStoredInExternalRecord:(bool)_v {}

@end

// =========================================================================
// NSAttributeDescription
// Per Apple: <https://developer.apple.com/documentation/coredata/nsattributedescription>
// Describes a Core Data entity attribute (its type, default value, etc.).
// =========================================================================

@implementation NSAttributeDescription: NSPropertyDescription

+ (id)allocWithZone:(NSZonePtr)_zone {
    let host_object = Box::new(NSAttributeDescriptionHostObject {
        superclass: NSPropertyDescriptionHostObject::default(),
        attribute_type: 0, // NSUndefinedAttributeType
        default_value: nil,
        allows_external_binary_data_storage: false,
    });
    env.objc.alloc_object(this, host_object, &mut env.mem)
}

- (())dealloc {
    let default_value = env.objc.borrow::<NSAttributeDescriptionHostObject>(this).default_value;
    release(env, default_value);
    ()=crate::objc::msg_super![env; this dealloc];
}

- (NSInteger)attributeType {
    env.objc.borrow::<NSAttributeDescriptionHostObject>(this).attribute_type
}

- (())setAttributeType:(NSInteger)attr_type {
    env.objc.borrow_mut::<NSAttributeDescriptionHostObject>(this).attribute_type = attr_type;
}

- (id)defaultValue {
    env.objc.borrow::<NSAttributeDescriptionHostObject>(this).default_value
}

- (())setDefaultValue:(id)value {
    let old = env.objc.borrow::<NSAttributeDescriptionHostObject>(this).default_value;
    release(env, old);
    retain(env, value);
    env.objc.borrow_mut::<NSAttributeDescriptionHostObject>(this).default_value = value;
}

- (id)attributeValueClassName { nil }
- (())setAttributeValueClassName:(id)_name {}

- (id)valueTransformerName { nil }
- (())setValueTransformerName:(id)_name {}

- (bool)allowsExternalBinaryDataStorage {
    env.objc.borrow::<NSAttributeDescriptionHostObject>(this).allows_external_binary_data_storage
}

- (())setAllowsExternalBinaryDataStorage:(bool)value {
    env.objc.borrow_mut::<NSAttributeDescriptionHostObject>(this).allows_external_binary_data_storage = value;
}

@end

// =========================================================================
// NSRelationshipDescription
// =========================================================================

@implementation NSRelationshipDescription: NSPropertyDescription

- (id)destinationEntity { nil }
- (())setDestinationEntity:(id)_entity {}
- (id)inverseRelationship { nil }
- (())setInverseRelationship:(id)_rel {}
- (NSInteger)deleteRule { 0 } // NSNoActionDeleteRule
- (())setDeleteRule:(NSInteger)_rule {}
- (bool)isToMany { false }
- (NSUInteger)maxCount { 0 }
- (())setMaxCount:(NSUInteger)_count {}
- (NSUInteger)minCount { 0 }
- (())setMinCount:(NSUInteger)_count {}
- (bool)isOrdered { false }
- (())setOrdered:(bool)_v {}

@end

};

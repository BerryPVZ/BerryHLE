/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Supported Core Data model/context operations. No success is reported until
//! model validation and the SQLite transaction have actually succeeded.
//!
//! Relationships are stored in a join table, so a to-many is just multiple
//! rows. `NSNullifyDeleteRule` and `NSCascadeDeleteRule` are honoured;
//! `NSDenyDeleteRule` and `NSNoActionDeleteRule` behave as nullify because
//! there is no validation pass that could report a deny before the graph is
//! already modified. Dotted key paths (`spouse.name`) do not traverse, and
//! `ANY`/`@count` in a predicate see the relationship as a plain set.
use super::*;
use crate::frameworks::foundation::{
    core_data_model as model, core_data_store as storage, ns_data, ns_date, ns_predicate,
};
use crate::fs::GuestPath;
use crate::Environment;
use plist::Value;
use std::collections::{HashMap, HashSet};

pub(super) fn fail(env: &mut Environment, error: MutPtr<id>, message: &str) {
    log!("Core Data: {}", message);
    if !error.is_null() {
        let domain = ns_string::get_static_str(env, "NSCocoaErrorDomain");
        let key = ns_string::get_static_str(env, "NSLocalizedDescription");
        let text = text(env, message.to_owned());
        let info: id = msg_class![env; NSDictionary dictionaryWithObject:text forKey:key];
        let value: id = msg_class![env; NSError errorWithDomain:domain code:(134060 as NSInteger) userInfo:info];
        env.mem.write(error, value);
    }
}
fn string(env: &mut Environment, value: id) -> String {
    ns_string::to_rust_string(env, value).into_owned()
}
fn text(env: &mut Environment, value: String) -> id {
    let object = ns_string::from_rust_string(env, value);
    autorelease(env, object)
}
fn items(env: &mut Environment, arr: id) -> Vec<id> {
    let count: NSUInteger = msg![env; arr count];
    (0..count).map(|i| msg![env; arr objectAtIndex:i]).collect()
}
fn array(env: &mut Environment, values: Vec<id>) -> id {
    let arr: id = msg_class![env; NSMutableArray new];
    for value in values {
        () = msg![env; arr addObject:value];
    }
    autorelease(env, arr)
}
pub(super) fn entities_by_name(env: &mut Environment, model: id) -> id {
    let entities = env
        .objc
        .borrow::<NSManagedObjectModelHostObject>(model)
        .entities;
    let entities = items(env, entities);
    let dict: id = msg_class![env; NSMutableDictionary new];
    for entity in entities {
        let name = env
            .objc
            .borrow::<NSEntityDescriptionHostObject>(entity)
            .name;
        if name != nil {
            () = msg![env; dict setObject:entity forKey:name];
        }
    }
    autorelease(env, dict)
}
pub(super) fn entity(env: &mut Environment, context: id, name: id) -> id {
    let psc = env
        .objc
        .borrow::<NSManagedObjectContextHostObject>(context)
        .persistent_store_coordinator;
    let model = env
        .objc
        .borrow::<NSPersistentStoreCoordinatorHostObject>(psc)
        .managed_object_model;
    let dict = entities_by_name(env, model);
    msg![env; dict objectForKey:name]
}
pub(super) fn set_properties(env: &mut Environment, entity: id, properties: id) {
    let dict: id = msg_class![env; NSMutableDictionary new];
    for property in items(env, properties) {
        let name: id = msg![env; property name];
        if name != nil {
            () = msg![env; dict setObject:property forKey:name];
        }
        env.objc
            .borrow_mut::<NSPropertyDescriptionHostObject>(property)
            .entity = entity;
    }
    let old = env
        .objc
        .borrow::<NSEntityDescriptionHostObject>(entity)
        .properties_by_name;
    env.objc
        .borrow_mut::<NSEntityDescriptionHostObject>(entity)
        .properties_by_name = dict;
    release(env, old);
}
fn value_to_object(env: &mut Environment, value: &Value, kind: i32) -> Result<id, String> {
    Ok(match (kind, value) {
        (700, Value::String(s)) => text(env, s.clone()),
        (900, Value::Real(v)) => ns_date::date_from_reference_interval(env, *v),
        (100 | 200 | 300, Value::Integer(v)) => {
            let n = v.as_signed().ok_or("Integer outside signed 64-bit range")?;
            msg_class![env; NSNumber numberWithLongLong:n]
        }
        (500 | 600, Value::Real(n)) => msg_class![env; NSNumber numberWithDouble:(*n)],
        (800, Value::Boolean(b)) => msg_class![env; NSNumber numberWithBool:(*b)],
        (1000, Value::Data(bytes)) => {
            let buffer = env.mem.alloc(bytes.len() as u32);
            env.mem
                .bytes_at_mut(buffer.cast(), bytes.len() as u32)
                .copy_from_slice(bytes);
            let data: id = msg_class![env; NSData dataWithBytesNoCopy:buffer length:(bytes.len() as NSUInteger)];
            data
        }
        _ => return Err(format!("Stored attribute does not match model type {kind}")),
    })
}
fn properties(env: &mut Environment, entity: id) -> Vec<(String, id)> {
    let props = env
        .objc
        .borrow::<NSEntityDescriptionHostObject>(entity)
        .properties_by_name;
    let keys: id = msg![env; props allKeys];
    items(env, keys)
        .into_iter()
        .map(|key| {
            let name = string(env, key);
            let attr: id = msg![env; props objectForKey:key];
            (name, attr)
        })
        .collect()
}
fn attributes(env: &mut Environment, entity: id) -> Vec<(String, id)> {
    properties(env, entity)
        .into_iter()
        .filter(|(_, property)| {
            env.objc
                .get_host_object(*property)
                .is_some_and(|h| h.as_any().is::<NSAttributeDescriptionHostObject>())
        })
        .collect()
}
fn relationships(env: &mut Environment, entity: id) -> Vec<(String, id)> {
    properties(env, entity)
        .into_iter()
        .filter(|(_, property)| {
            env.objc
                .get_host_object(*property)
                .is_some_and(|h| h.as_any().is::<NSRelationshipDescriptionHostObject>())
        })
        .collect()
}
/// The objects a to-many relationship currently holds, or the single object of
/// a to-one relationship. Empty when the value is nil.
fn related_objects(env: &mut Environment, value: id, to_many: bool) -> Vec<id> {
    if value == nil {
        return Vec::new();
    }
    if to_many {
        let all: id = msg![env; value allObjects];
        items(env, all)
    } else {
        vec![value]
    }
}

fn model_fingerprint(env: &mut Environment, object_model: id) -> Result<String, String> {
    let entities = env
        .objc
        .borrow::<NSManagedObjectModelHostObject>(object_model)
        .entities;
    let mut descriptions = Vec::new();
    for entity in items(env, entities) {
        let e = env.objc.borrow::<NSEntityDescriptionHostObject>(entity);
        let (name, class) = (e.name, e.managed_object_class_name);
        let class_name = if class == nil {
            "NSManagedObject".to_owned()
        } else {
            string(env, class)
        };
        let class_id = env
            .objc
            .try_get_known_class(&class_name, &mut env.mem)
            .ok_or("Unknown managed object class")?;
        let mut attrs = Vec::new();
        let mut rels = Vec::new();
        for (key, property) in properties(env, entity) {
            if env
                .objc
                .declared_property_attributes(class_id, &key, &env.mem)
                .is_some_and(|attrs| !attrs.starts_with("T@"))
            {
                return Err(
                    "Scalar dynamic attributes are not implemented; use boxed KVC attributes"
                        .into(),
                );
            }
            let (transient, optional) = {
                let p = env.objc.borrow::<NSPropertyDescriptionHostObject>(property);
                (p.transient, p.optional)
            };
            if transient {
                return Err("Transient properties are not implemented".into());
            }
            let attribute = env.objc.get_host_object(property).and_then(|h| {
                h.as_any()
                    .downcast_ref::<NSAttributeDescriptionHostObject>()
            });
            if let Some(a) = attribute {
                if !model::supported_type(a.attribute_type) {
                    return Err("Unsupported attribute type".into());
                }
                let kind = a.attribute_type;
                let default = a.default_value;
                let default = if default == nil {
                    None
                } else {
                    Some(attribute_value(env, default, kind, &key)?)
                };
                attrs.push(format!("{key}:{kind}:{optional}:{default:?}"));
                continue;
            }
            let relationship = env.objc.get_host_object(property).and_then(|h| {
                h.as_any()
                    .downcast_ref::<NSRelationshipDescriptionHostObject>()
            });
            let Some(r) = relationship else {
                return Err("Unsupported property description".into());
            };
            let (destination, inverse, to_many, delete_rule) = (
                r.destination_entity,
                r.inverse_relationship,
                (r.max_count != 1),
                r.delete_rule,
            );
            let destination = if destination == nil {
                String::new()
            } else {
                let name = env
                    .objc
                    .borrow::<NSEntityDescriptionHostObject>(destination)
                    .name;
                string(env, name)
            };
            let inverse = if inverse == nil {
                String::new()
            } else {
                let name = env
                    .objc
                    .borrow::<NSPropertyDescriptionHostObject>(inverse)
                    .name;
                string(env, name)
            };
            rels.push(format!(
                "{key}:{destination}:{to_many}:{optional}:{delete_rule}:{inverse}"
            ));
        }
        attrs.sort();
        rels.sort();
        descriptions.push(format!(
            "{}:{}:{attrs:?}:{rels:?}",
            string(env, name),
            string(env, class)
        ));
    }
    descriptions.sort();
    Ok(format!("VetusCoreData2:{descriptions:?}"))
}
pub(super) fn load_model(env: &mut Environment, this: id, bytes: &[u8]) -> Result<(), String> {
    let entities = model::parse(bytes)?;
    let mut result = Vec::new();
    for description in &entities {
        let entity: id = msg_class![env; NSEntityDescription new];
        let name = text(env, description.name.clone());
        let class = text(env, description.class.clone());
        () = msg![env; entity setName:name];
        () = msg![env; entity setManagedObjectClassName:class];
        let mut props = Vec::new();
        for attribute in &description.attributes {
            let property: id = msg_class![env; NSAttributeDescription new];
            let name = text(env, attribute.name.clone());
            () = msg![env; property setName:name];
            () = msg![env; property setAttributeType:(attribute.kind)];
            () = msg![env; property setOptional:(attribute.optional)];
            if let Some(value) = &attribute.default {
                let object = value_to_object(env, value, attribute.kind)?;
                () = msg![env; property setDefaultValue:object];
            }
            props.push(property);
        }
        for relationship in &description.relationships {
            let property: id = msg_class![env; NSRelationshipDescription new];
            let name = text(env, relationship.name.clone());
            () = msg![env; property setName:name];
            () = msg![env; property setOptional:(relationship.optional)];
            () = msg![env; property setDeleteRule:(relationship.delete_rule)];
            // maxCount 1 is to-one; 0 is unbounded, which is to-many.
            let max_count: NSUInteger = if relationship.to_many { 0 } else { 1 };
            () = msg![env; property setMaxCount:max_count];
            props.push(property);
        }
        let attributes = array(env, props.clone());
        set_properties(env, entity, attributes);
        for attr in props {
            release(env, attr);
        }
        result.push(entity);
    }
    // Second pass: destinations and inverses can only be linked once every
    // entity exists.
    for (description, entity) in entities.iter().zip(result.iter()) {
        for relationship in &description.relationships {
            let name = text(env, relationship.name.clone());
            let props = env
                .objc
                .borrow::<NSEntityDescriptionHostObject>(*entity)
                .properties_by_name;
            let property: id = msg![env; props objectForKey:name];
            if property == nil {
                continue;
            }
            let destination = entities
                .iter()
                .position(|e| e.name == relationship.destination)
                .map(|index| result[index]);
            let Some(destination) = destination else {
                continue;
            };
            () = msg![env; property setDestinationEntity:destination];
            let Some(inverse) = &relationship.inverse else {
                continue;
            };
            let inverse_name = text(env, inverse.clone());
            let props = env
                .objc
                .borrow::<NSEntityDescriptionHostObject>(destination)
                .properties_by_name;
            let inverse_property: id = msg![env; props objectForKey:inverse_name];
            if inverse_property != nil {
                () = msg![env; property setInverseRelationship:inverse_property];
            }
        }
    }
    let entities = array(env, result.clone());
    () = msg![env; this setEntities:entities];
    for entity in result {
        release(env, entity);
    }
    Ok(())
}
pub(super) fn merge_bundles(env: &mut Environment, bundles: id) -> id {
    let bundles = if bundles == nil {
        let b: id = msg_class![env; NSBundle mainBundle];
        vec![b]
    } else {
        items(env, bundles)
    };
    let model: id = msg_class![env; NSManagedObjectModel new];
    let mut all = Vec::new();
    let mut names = HashSet::new();
    for bundle in bundles {
        let path: id = msg![env; bundle bundlePath];
        let path = string(env, path);
        let members = match env.fs.enumerate_recursive(GuestPath::new(&path)) {
            Ok(m) => m,
            Err(_) => {
                release(env, model);
                return nil;
            }
        };
        for member in members.into_iter().filter(|p| p.as_str().ends_with(".mom")) {
            let path = GuestPath::new(&path).join(member.as_str());
            let bytes = match env.fs.read(&path) {
                Ok(b) => b,
                Err(_) => {
                    release(env, model);
                    return nil;
                }
            };
            let partial: id = msg_class![env; NSManagedObjectModel new];
            if let Err(e) = load_model(env, partial, &bytes) {
                log!("Core Data model: {}", e);
                release(env, partial);
                release(env, model);
                return nil;
            }
            let entities = env
                .objc
                .borrow::<NSManagedObjectModelHostObject>(partial)
                .entities;
            for entity in items(env, entities) {
                let name = env
                    .objc
                    .borrow::<NSEntityDescriptionHostObject>(entity)
                    .name;
                if !names.insert(string(env, name)) {
                    release(env, partial);
                    release(env, model);
                    return nil;
                }
                retain(env, entity);
                all.push(entity);
            }
            release(env, partial);
        }
    }
    if all.is_empty() {
        release(env, model);
        return nil;
    }
    let entities = array(env, all.clone());
    () = msg![env; model setEntities:entities];
    for entity in all {
        release(env, entity);
    }
    autorelease(env, model)
}
fn dynamic_get(env: &mut Environment, object: id, cmd: crate::objc::SEL) -> id {
    let key = cmd.as_str(&env.mem).to_owned();
    env.objc
        .borrow::<NSManagedObjectHostObject>(object)
        .values
        .get(&key)
        .copied()
        .unwrap_or(nil)
}
fn dynamic_set(env: &mut Environment, object: id, cmd: crate::objc::SEL, value: id) {
    let selector = cmd.as_str(&env.mem).to_owned();
    let entity = env.objc.borrow::<NSManagedObjectHostObject>(object).entity;
    for (key, _) in properties(env, entity) {
        if format!("set{}{}:", key[..1].to_ascii_uppercase(), &key[1..]) == selector {
            set_value(env, object, &key, value);
            return;
        }
    }
}
// A to-many relationship is an empty set rather than nil, so callers can add
// to it without a nil check.
fn ensure_relationship_sets(env: &mut Environment, object: id, entity: id) {
    for (key, relationship) in relationships(env, entity) {
        if env
            .objc
            .borrow::<NSRelationshipDescriptionHostObject>(relationship)
            .max_count
            == 1
        {
            continue;
        }
        if env
            .objc
            .borrow::<NSManagedObjectHostObject>(object)
            .values
            .contains_key(&key)
        {
            continue;
        }
        let set: id = msg_class![env; NSMutableSet new];
        retain(env, set);
        env.objc
            .borrow_mut::<NSManagedObjectHostObject>(object)
            .values
            .insert(key, set);
    }
}

pub(super) fn apply_defaults(env: &mut Environment, object: id) {
    let entity = env.objc.borrow::<NSManagedObjectHostObject>(object).entity;
    for (key, attr) in attributes(env, entity) {
        let value = env
            .objc
            .borrow::<NSAttributeDescriptionHostObject>(attr)
            .default_value;
        if value != nil {
            set_value(env, object, &key, value);
        }
    }
    ensure_relationship_sets(env, object, entity);
}
pub(super) fn insert_new(env: &mut Environment, entity: id, context: id) -> id {
    if entity == nil {
        return nil;
    }
    let class = env
        .objc
        .borrow::<NSEntityDescriptionHostObject>(entity)
        .managed_object_class_name;
    let class = if class == nil {
        "NSManagedObject".into()
    } else {
        string(env, class)
    };
    let Some(class) = env.objc.try_get_known_class(&class, &mut env.mem) else {
        return nil;
    };
    let base = env.objc.get_known_class("NSManagedObject", &mut env.mem);
    let mut ancestor = class;
    while ancestor != nil && ancestor != base {
        ancestor = env.objc.get_superclass(ancestor);
    }
    if ancestor != base {
        return nil;
    }
    for (key, _) in properties(env, entity) {
        if !key.is_ascii() || key.is_empty() {
            return nil;
        }
        if env
            .objc
            .declared_property_attributes(class, &key, &env.mem)
            .is_some_and(|attrs| !attrs.starts_with("T@"))
        {
            log!(
                "Core Data: scalar dynamic property {} is not implemented",
                key
            );
            return nil;
        }
        let setter = format!("set{}{}:", key[..1].to_ascii_uppercase(), &key[1..]);
        env.objc.add_host_method_if_absent(
            class,
            &key,
            &(dynamic_get as fn(&mut Environment, id, crate::objc::SEL) -> id),
            &mut env.mem,
        );
        env.objc.add_host_method_if_absent(
            class,
            &setter,
            &(dynamic_set as fn(&mut Environment, id, crate::objc::SEL, id)),
            &mut env.mem,
        );
    }
    let object: id = msg![env; class alloc];
    let object: id = msg![env; object initWithEntity:entity insertIntoManagedObjectContext:context];
    autorelease(env, object)
}
pub(super) fn set_value(env: &mut Environment, object: id, key: &str, value: id) {
    let entity = env.objc.borrow::<NSManagedObjectHostObject>(object).entity;
    let props = env
        .objc
        .borrow::<NSEntityDescriptionHostObject>(entity)
        .properties_by_name;
    let k = text(env, key.to_owned());
    let property: id = msg![env; props objectForKey:k];
    if property == nil {
        log!("Core Data: unknown property {}", key);
        return;
    }
    retain(env, value);
    let host = env.objc.borrow_mut::<NSManagedObjectHostObject>(object);
    let old = if value == nil {
        host.values.remove(key)
    } else {
        host.values.insert(key.into(), value)
    };
    host.dirty = true;
    if let Some(old) = old {
        release(env, old);
    }
}
fn store_for_context(env: &mut Environment, context: id) -> Result<id, String> {
    let h = env.objc.borrow::<NSManagedObjectContextHostObject>(context);
    if h.parent_context != nil {
        return Err("Parent contexts are not implemented".into());
    }
    let psc = h.persistent_store_coordinator;
    if psc == nil {
        return Err("Context has no persistent store coordinator".into());
    }
    let stores = env
        .objc
        .borrow::<NSPersistentStoreCoordinatorHostObject>(psc)
        .persistent_stores;
    let stores = items(env, stores);
    if stores.len() != 1 {
        return Err("Exactly one persistent store is currently supported".into());
    }
    Ok(stores[0])
}
pub(super) fn add_store(
    env: &mut Environment,
    psc: id,
    type_: id,
    url: id,
    options: id,
    error: MutPtr<id>,
) -> id {
    let kind = string(env, type_);
    let model = env
        .objc
        .borrow::<NSPersistentStoreCoordinatorHostObject>(psc)
        .managed_object_model;
    let stores = env
        .objc
        .borrow::<NSPersistentStoreCoordinatorHostObject>(psc)
        .persistent_stores;
    let attempt = (|| -> Result<storage::Store, String> {
        if !items(env, stores).is_empty() {
            return Err("Multiple persistent stores are not implemented".into());
        }
        if options != nil {
            let keys: id = msg![env; options allKeys];
            for key in items(env, keys) {
                let name = string(env, key);
                if name == "NSReadOnlyPersistentStoreOption" {
                    let v: id = msg![env; options objectForKey:key];
                    let yes: bool = msg![env; v boolValue];
                    if yes {
                        return Err("Read-only persistent stores are not implemented".into());
                    }
                }
            }
        }
        let fingerprint = model_fingerprint(env, model)?;
        let path = match kind.as_str() {
            "InMemory" => None,
            "SQLite" => {
                if url == nil {
                    return Err("SQLite store requires a URL".into());
                }
                let is_file: bool = msg![env; url isFileURL];
                if !is_file {
                    return Err("SQLite store requires a file URL".into());
                }
                let path: id = msg![env; url path];
                let path = string(env, path);
                Some(
                    env.fs
                        .writable_host_path(GuestPath::new(&path))
                        .map_err(|_| "Persistent store URL is not writable".to_owned())?,
                )
            }
            _ => return Err(format!("Unsupported persistent store type {kind}")),
        };
        storage::Store::open(path.as_deref(), &fingerprint)
    })();
    let storage = match attempt {
        Ok(s) => s,
        Err(e) => {
            fail(env, error, &e);
            return nil;
        }
    };
    let store: id = msg_class![env; NSPersistentStore alloc];
    let store: id = msg![env; store initWithPersistentStoreCoordinator:psc configurationName:nil URL:url options:options];
    retain(env, type_);
    let host = env.objc.borrow_mut::<NSPersistentStoreHostObject>(store);
    host.type_ = type_;
    host.storage = Some(storage);
    () = msg![env; stores addObject:store];
    release(env, store);
    store
}
fn replace_values(
    env: &mut Environment,
    object: id,
    record: &storage::Record,
) -> Result<(), String> {
    let entity = env.objc.borrow::<NSManagedObjectHostObject>(object).entity;
    let mut values = HashMap::new();
    for (key, attr) in attributes(env, entity) {
        if let Some(value) = record.attributes.get(&key) {
            let kind = env
                .objc
                .borrow::<NSAttributeDescriptionHostObject>(attr)
                .attribute_type;
            let value = value_to_object(env, value, kind)?;
            retain(env, value);
            values.insert(key, value);
        }
    }
    let host = env.objc.borrow_mut::<NSManagedObjectHostObject>(object);
    let old = std::mem::replace(&mut host.values, values);
    host.row_id = record.id;
    host.inserted = false;
    host.deleted = false;
    host.dirty = false;
    for value in old.into_values() {
        release(env, value);
    }
    ensure_relationship_sets(env, object, entity);
    Ok(())
}
fn apply_stored_relationships(
    env: &mut Environment,
    context: id,
    store: id,
    skip_dirty: bool,
) -> Result<(), String> {
    let rows = env
        .objc
        .borrow::<NSPersistentStoreHostObject>(store)
        .storage
        .as_ref()
        .ok_or("Store is not open")?
        .relationships()?;
    if rows.is_empty() {
        return Ok(());
    }
    let objects = env
        .objc
        .borrow::<NSManagedObjectContextHostObject>(context)
        .objects
        .clone();
    let mut by_row = HashMap::new();
    for object in &objects {
        let row_id = env.objc.borrow::<NSManagedObjectHostObject>(*object).row_id;
        if row_id > 0 {
            by_row.insert(row_id, *object);
        }
    }
    for (source_id, name, target_id) in rows {
        let (Some(source), Some(target)) = (
            by_row.get(&source_id).copied(),
            by_row.get(&target_id).copied(),
        ) else {
            continue;
        };
        // An object with unsaved changes keeps its in-memory relationships;
        // reloading them here would silently undo the guest's edits.
        if skip_dirty && env.objc.borrow::<NSManagedObjectHostObject>(source).dirty {
            continue;
        }
        let entity = env.objc.borrow::<NSManagedObjectHostObject>(source).entity;
        let Some((_, property)) = relationships(env, entity)
            .into_iter()
            .find(|(key, _)| *key == name)
        else {
            continue;
        };
        let to_many = env
            .objc
            .borrow::<NSRelationshipDescriptionHostObject>(property)
            .max_count
            != 1;
        if to_many {
            let set = {
                let existing = env
                    .objc
                    .borrow::<NSManagedObjectHostObject>(source)
                    .values
                    .get(&name)
                    .copied()
                    .unwrap_or(nil);
                if existing != nil {
                    existing
                } else {
                    let set: id = msg_class![env; NSMutableSet new];
                    retain(env, set);
                    env.objc
                        .borrow_mut::<NSManagedObjectHostObject>(source)
                        .values
                        .insert(name.clone(), set);
                    set
                }
            };
            () = msg![env; set addObject:target];
        } else {
            retain(env, target);
            let old = env
                .objc
                .borrow_mut::<NSManagedObjectHostObject>(source)
                .values
                .insert(name.clone(), target);
            if let Some(old) = old {
                release(env, old);
            }
        }
    }
    Ok(())
}

fn load_registered(env: &mut Environment, context: id) -> Result<(), String> {
    let store = store_for_context(env, context)?;
    let records = env
        .objc
        .borrow::<NSPersistentStoreHostObject>(store)
        .storage
        .as_ref()
        .ok_or("Store is not open")?
        .records()?;
    let live: HashSet<_> = records.iter().map(|record| record.id).collect();
    let registered = env
        .objc
        .borrow::<NSManagedObjectContextHostObject>(context)
        .objects
        .clone();
    for object in registered {
        let host = env.objc.borrow::<NSManagedObjectHostObject>(object);
        if host.row_id > 0 && !host.dirty && !live.contains(&host.row_id) {
            env.objc
                .borrow_mut::<NSManagedObjectContextHostObject>(context)
                .objects
                .retain(|o| *o != object);
            env.objc
                .borrow_mut::<NSManagedObjectHostObject>(object)
                .managed_object_context = nil;
            release(env, object);
        }
    }
    for record in records {
        let existing = env
            .objc
            .borrow::<NSManagedObjectContextHostObject>(context)
            .objects
            .clone()
            .into_iter()
            .find(|o| env.objc.borrow::<NSManagedObjectHostObject>(*o).row_id == record.id);
        if let Some(object) = existing {
            if !env.objc.borrow::<NSManagedObjectHostObject>(object).dirty {
                replace_values(env, object, &record)?;
            }
        } else {
            let name = text(env, record.entity.clone());
            let entity = entity(env, context, name);
            if entity == nil {
                return Err("Stored entity is missing from model".into());
            }
            let object = insert_new(env, entity, nil);
            if object == nil {
                return Err("Could not create managed class".into());
            }
            retain(env, object);
            env.objc
                .borrow_mut::<NSManagedObjectContextHostObject>(context)
                .objects
                .push(object);
            env.objc
                .borrow_mut::<NSManagedObjectHostObject>(object)
                .managed_object_context = context;
            replace_values(env, object, &record)?;
            () = msg![env; object awakeFromFetch];
        }
    }
    apply_stored_relationships(env, context, store, true)?;
    Ok(())
}
pub(super) fn fetch(env: &mut Environment, context: id, request: id, error: MutPtr<id>) -> id {
    if let Err(e) = load_registered(env, context) {
        fail(env, error, &e);
        return nil;
    }
    let req = env.objc.borrow::<NSFetchRequestHostObject>(request);
    let (mut entity_id, name, predicate, sort, limit, offset, result_type) = (
        req.entity,
        req.entity_name,
        req.predicate,
        req.sort_descriptors,
        req.fetch_limit,
        req.fetch_offset,
        req.result_type,
    );
    if entity_id == nil && name != nil {
        entity_id = entity(env, context, name);
    }
    if env
        .objc
        .borrow::<NSFetchRequestHostObject>(request)
        .unsupported_options
        || entity_id == nil
        || result_type != 0
        || (predicate != nil && !ns_predicate::is_supported(env, predicate))
    {
        fail(
            env,
            error,
            "Unsupported fetch result type, predicate, or missing entity",
        );
        return nil;
    }
    let mut objects = env
        .objc
        .borrow::<NSManagedObjectContextHostObject>(context)
        .objects
        .clone();
    objects.retain(|object| {
        let o = env.objc.borrow::<NSManagedObjectHostObject>(*object);
        o.entity == entity_id && !o.deleted
    });
    if predicate != nil {
        objects.retain(|object| {
            let result: bool = msg![env; predicate evaluateWithObject:(*object)];
            result
        });
    }
    let descriptors = items(env, sort);
    objects.sort_by(|a, b| {
        for desc in &descriptors {
            let r: i32 = msg![env; (*desc) compareObject:(*a) toObject:(*b)];
            if r != 0 {
                return r.cmp(&0);
            }
        }
        std::cmp::Ordering::Equal
    });
    let result = objects
        .into_iter()
        .skip(offset as usize)
        .take(if limit == 0 {
            usize::MAX
        } else {
            limit as usize
        })
        .collect();
    array(env, result)
}
fn attribute_value(
    env: &mut Environment,
    value: id,
    kind: i32,
    key: &str,
) -> Result<Value, String> {
    let class = match kind {
        700 => "NSString",
        900 => "NSDate",
        1000 => "NSData",
        _ => "NSNumber",
    };
    let class = env.objc.get_known_class(class, &mut env.mem);
    let valid: bool = msg![env; value isKindOfClass:class];
    if !valid {
        return Err(format!("Attribute {key} has the wrong value class"));
    }
    let stored = match kind {
        700 => Value::String(string(env, value)),
        900 => Value::Real(ns_date::get_time_interval(env, value)),
        1000 => Value::Data(ns_data::to_rust_slice(env, value).to_vec()),
        100 | 200 | 300 => {
            let n: i64 = msg![env; value longLongValue];
            Value::Integer(n.into())
        }
        800 => {
            let n: bool = msg![env; value boolValue];
            Value::Boolean(n)
        }
        500 | 600 => {
            let n: f64 = msg![env; value doubleValue];
            Value::Real(n)
        }
        _ => return Err(format!("Unsupported attribute type {kind}")),
    };
    Ok(stored)
}
fn serialize_object(env: &mut Environment, object: id) -> Result<storage::Record, String> {
    let o = env.objc.borrow::<NSManagedObjectHostObject>(object);
    let (entity, row_id, values) = (o.entity, o.row_id, o.values.clone());
    let name = env
        .objc
        .borrow::<NSEntityDescriptionHostObject>(entity)
        .name;
    let name = string(env, name);
    let mut stored_attributes = plist::Dictionary::new();
    for (key, attribute) in attributes(env, entity) {
        let a = env
            .objc
            .borrow::<NSAttributeDescriptionHostObject>(attribute);
        let kind = a.attribute_type;
        let optional = env
            .objc
            .borrow::<NSPropertyDescriptionHostObject>(attribute)
            .optional;
        let value = values.get(&key).copied().unwrap_or(nil);
        if value == nil {
            if optional {
                continue;
            }
            return Err(format!("Required attribute {key} is missing"));
        }
        let stored = attribute_value(env, value, kind, &key)?;
        stored_attributes.insert(key, stored);
    }
    Ok(storage::Record {
        id: row_id,
        entity: name,
        attributes: stored_attributes,
    })
}
pub(super) fn save(env: &mut Environment, context: id, error: MutPtr<id>) -> bool {
    let attempt = (|| -> Result<(), String> {
        let store = store_for_context(env, context)?;
        let objects = env
            .objc
            .borrow::<NSManagedObjectContextHostObject>(context)
            .objects
            .clone();
        let mut changed = Vec::new();
        let mut records = Vec::new();
        let mut deleted = Vec::new();
        for object in &objects {
            let o = env.objc.borrow::<NSManagedObjectHostObject>(*object);
            if o.deleted {
                if o.row_id > 0 {
                    deleted.push(o.row_id);
                }
            } else if o.dirty || o.inserted {
                changed.push(*object);
                records.push(serialize_object(env, *object)?);
            }
        }
        let ids = env
            .objc
            .borrow_mut::<NSPersistentStoreHostObject>(store)
            .storage
            .as_mut()
            .ok_or("Store is not open")?
            .save(&records, &deleted)?;
        for (object, id) in changed.iter().zip(ids) {
            let o = env.objc.borrow_mut::<NSManagedObjectHostObject>(*object);
            o.row_id = id;
            o.inserted = false;
        }
        // Row ids exist only now, so relationship rows are written afterwards.
        let mut relationship_rows = Vec::new();
        let mut relationship_sources = Vec::new();
        for object in &changed {
            let (entity, row_id, values) = {
                let o = env.objc.borrow::<NSManagedObjectHostObject>(*object);
                (o.entity, o.row_id, o.values.clone())
            };
            if row_id <= 0 {
                continue;
            }
            relationship_sources.push(row_id);
            for (key, property) in relationships(env, entity) {
                let to_many = env
                    .objc
                    .borrow::<NSRelationshipDescriptionHostObject>(property)
                    .max_count != 1;
                let value = values.get(&key).copied().unwrap_or(nil);
                for target in related_objects(env, value, to_many) {
                    let (target_id, target_deleted) = {
                        let t = env.objc.borrow::<NSManagedObjectHostObject>(target);
                        (t.row_id, t.deleted)
                    };
                    // A target deleted in this same save has already had its
                    // rows removed, so re-adding one would leave a dangling id.
                    if target_id > 0 && !target_deleted {
                        relationship_rows.push((row_id, key.clone(), target_id));
                    }
                }
                }
            }
        env.objc
            .borrow_mut::<NSPersistentStoreHostObject>(store)
            .storage
            .as_mut()
            .ok_or("Store is not open")?
            .save_relationships(&relationship_sources, &relationship_rows)?;
        // Only clear the dirty flags once the relationship rows are stored
        // too, so a failure here can be retried by saving again.
        for object in &changed {
            env.objc
                .borrow_mut::<NSManagedObjectHostObject>(*object)
                .dirty = false;
        }
        for object in objects {
            if env.objc.borrow::<NSManagedObjectHostObject>(object).deleted {
                let h = env
                    .objc
                    .borrow_mut::<NSManagedObjectContextHostObject>(context);
                h.objects.retain(|o| *o != object);
                env.objc
                    .borrow_mut::<NSManagedObjectHostObject>(object)
                    .managed_object_context = nil;
                release(env, object);
            }
        }
        Ok(())
    })();
    match attempt {
        Ok(()) => true,
        Err(e) => {
            fail(env, error, &e);
            false
        }
    }
}
/// Mark `object` deleted and apply its relationships' delete rules.
///
/// `NSNullifyDeleteRule` (1) is the default. `NSCascadeDeleteRule` (2) deletes
/// the related objects too. `NSDenyDeleteRule` (3) and `NSNoActionDeleteRule`
/// (0) both behave as nullify here: this store has no validation pass that
/// could report a deny without first corrupting the graph.
pub(super) fn delete_object(env: &mut Environment, object: id) {
    if object == nil {
        return;
    }

    if env.objc.borrow::<NSManagedObjectHostObject>(object).deleted {
        return;
    }
// Mark first: a cyclic relationship would otherwise recurse forever.
{
let host = env.objc.borrow_mut::<NSManagedObjectHostObject>(object);
host.deleted = true;
host.dirty = true;
}
    let entity = env.objc.borrow::<NSManagedObjectHostObject>(object).entity;
    for (key, property) in relationships(env, entity) {
        let (delete_rule, inverse, to_many) = {
            let r = env.objc.borrow::<NSRelationshipDescriptionHostObject>(property);
            (r.delete_rule, r.inverse_relationship, r.max_count != 1)
        };
        let value = env
            .objc
            .borrow::<NSManagedObjectHostObject>(object)
            .values
            .get(&key)
            .copied()
            .unwrap_or(nil);
        for target in related_objects(env, value, to_many) {
            if target == nil {
                continue;
            }
            if delete_rule == 2 {
                delete_object(env, target);
                continue;
            }
            if inverse == nil {
                continue;
            }
            let inverse_name = string(
                env,
                env.objc
                    .borrow::<NSPropertyDescriptionHostObject>(inverse)
                    .name,
            );
            let target_entity = env.objc.borrow::<NSManagedObjectHostObject>(target).entity;
            let inverse_to_many = relationships(env, target_entity)
                .into_iter()
                .find(|(name, _)| *name == inverse_name)
                .is_some_and(|(_, property)| {
                    env.objc
                        .borrow::<NSRelationshipDescriptionHostObject>(property)
                        .max_count != 1
                });
            if inverse_to_many {
                let set = env
                    .objc
                    .borrow::<NSManagedObjectHostObject>(target)
                    .values
                    .get(&inverse_name)
                    .copied()
                    .unwrap_or(nil);
                if set != nil {
                    () = msg![env; set removeObject:object];
                }
            } else {
                set_value(env, target, &inverse_name, nil);
            }
        }
    }
}

pub(super) fn reset(env: &mut Environment, context: id) {
    let objects = std::mem::take(
        &mut env
            .objc
            .borrow_mut::<NSManagedObjectContextHostObject>(context)
            .objects,
    );
    for object in objects {
        env.objc
            .borrow_mut::<NSManagedObjectHostObject>(object)
            .managed_object_context = nil;
        release(env, object);
    }
}
pub(super) fn rollback(env: &mut Environment, context: id) {
    let attempt = (|| -> Result<(), String> {
        let store = store_for_context(env, context)?;
        let records = env
            .objc
            .borrow::<NSPersistentStoreHostObject>(store)
            .storage
            .as_ref()
            .ok_or("Store is not open")?
            .records()?;
        let records: HashMap<_, _> = records.into_iter().map(|r| (r.id, r)).collect();
        let objects = env
            .objc
            .borrow::<NSManagedObjectContextHostObject>(context)
            .objects
            .clone();
        for object in objects {
            let row = env.objc.borrow::<NSManagedObjectHostObject>(object).row_id;
            if let Some(record) = records.get(&row) {
                replace_values(env, object, record)?;
            } else {
                env.objc
                    .borrow_mut::<NSManagedObjectContextHostObject>(context)
                    .objects
                    .retain(|o| *o != object);
                env.objc
                    .borrow_mut::<NSManagedObjectHostObject>(object)
                    .managed_object_context = nil;
                release(env, object);
            }
        }
        apply_stored_relationships(env, context, store, false)?;
        Ok(())
    })();
    if let Err(e) = attempt {
        log!("Core Data rollback failed: {}", e);
    }
}

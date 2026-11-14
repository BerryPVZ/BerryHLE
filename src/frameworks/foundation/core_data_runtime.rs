/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! Supported Core Data model/context operations. No success is reported until
//! model validation and the SQLite transaction have actually succeeded.
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
        for (key, attr) in properties(env, entity) {
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
            let actual = env.objc.get_host_object(attr).and_then(|h| {
                h.as_any()
                    .downcast_ref::<NSAttributeDescriptionHostObject>()
            });
            let a = actual.ok_or("Relationships are not implemented")?;
            if !model::supported_type(a.attribute_type) {
                return Err("Unsupported attribute type".into());
            }
            let kind = a.attribute_type;
            let default = a.default_value;
            let p = env.objc.borrow::<NSPropertyDescriptionHostObject>(attr);
            if p.transient {
                return Err("Transient attributes are not implemented".into());
            }
            let optional = p.optional;
            let default = if default == nil {
                None
            } else {
                Some(attribute_value(env, default, kind, &key)?)
            };
            attrs.push(format!("{key}:{kind}:{optional}:{default:?}"));
        }
        attrs.sort();
        descriptions.push(format!(
            "{}:{}:{attrs:?}",
            string(env, name),
            string(env, class)
        ));
    }
    descriptions.sort();
    Ok(format!("VetusCoreData1:{descriptions:?}"))
}
pub(super) fn load_model(env: &mut Environment, this: id, bytes: &[u8]) -> Result<(), String> {
    let entities = model::parse(bytes)?;
    let mut result = Vec::new();
    for description in entities {
        let entity: id = msg_class![env; NSEntityDescription new];
        let name = text(env, description.name);
        let class = text(env, description.class);
        () = msg![env; entity setName:name];
        () = msg![env; entity setManagedObjectClassName:class];
        let mut props = Vec::new();
        for attribute in description.attributes {
            let property: id = msg_class![env; NSAttributeDescription new];
            let name = text(env, attribute.name);
            () = msg![env; property setName:name];
            () = msg![env; property setAttributeType:(attribute.kind)];
            () = msg![env; property setOptional:(attribute.optional)];
            if let Some(value) = attribute.default {
                let object = value_to_object(env, &value, attribute.kind)?;
                () = msg![env; property setDefaultValue:object];
            }
            props.push(property);
        }
        let attributes = array(env, props.clone());
        set_properties(env, entity, attributes);
        for attr in props {
            release(env, attr);
        }
        result.push(entity);
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
pub(super) fn apply_defaults(env: &mut Environment, object: id) {
    let entity = env.objc.borrow::<NSManagedObjectHostObject>(object).entity;
    for (key, attr) in properties(env, entity) {
        let value = env
            .objc
            .borrow::<NSAttributeDescriptionHostObject>(attr)
            .default_value;
        if value != nil {
            set_value(env, object, &key, value);
        }
    }
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
    for (key, attr) in properties(env, entity) {
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
    let mut attributes = plist::Dictionary::new();
    for (key, attribute) in properties(env, entity) {
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
        attributes.insert(key, stored);
    }
    Ok(storage::Record {
        id: row_id,
        entity: name,
        attributes,
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
        for (object, id) in changed.into_iter().zip(ids) {
            let o = env.objc.borrow_mut::<NSManagedObjectHostObject>(object);
            o.row_id = id;
            o.inserted = false;
            o.dirty = false;
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
        Ok(())
    })();
    if let Err(e) = attempt {
        log!("Core Data rollback failed: {}", e);
    }
}

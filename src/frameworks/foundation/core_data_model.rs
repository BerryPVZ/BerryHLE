/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! The attribute-only portion of the NSKeyedArchiver compiled model format.
//! Unsupported relationships/inheritance fail explicitly rather than silently
//! creating an empty model.
use plist::Value;
trait ValueFields {
    fn get(&self, key: &str) -> Option<&Value>;
}
impl ValueFields for Value {
    fn get(&self, key: &str) -> Option<&Value> {
        self.as_dictionary()?.get(key)
    }
}
fn resolve<'a>(objects: &'a [Value], v: &'a Value) -> Result<&'a Value, String> {
    match v.as_uid() {
        Some(uid) => objects
            .get(uid.get() as usize)
            .ok_or_else(|| "Invalid model reference".into()),
        None => Ok(v),
    }
}
fn field<'a>(objects: &'a [Value], v: &'a Value, key: &str) -> Result<&'a Value, String> {
    resolve(
        objects,
        v.get(key)
            .ok_or_else(|| format!("Missing model field {key}"))?,
    )
}

#[derive(Debug, Clone)]
pub(super) struct Attribute {
    pub name: String,
    pub kind: i32,
    pub optional: bool,
    pub default: Option<Value>,
}
#[derive(Debug, Clone)]
pub(super) struct Entity {
    pub name: String,
    pub class: String,
    pub attributes: Vec<Attribute>,
}

pub(super) fn supported_type(kind: i32) -> bool {
    matches!(kind, 100 | 200 | 300 | 500 | 600 | 700 | 800 | 900 | 1000)
}

pub(super) fn parse(bytes: &[u8]) -> Result<Vec<Entity>, String> {
    let archive = Value::from_reader(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let objects = archive
        .get("$objects")
        .and_then(Value::as_array)
        .ok_or("Missing archived objects")?;
    let root_ref = archive
        .get("$top")
        .and_then(|v| v.get("root"))
        .ok_or("Missing model root")?;
    let model = resolve(objects, root_ref)?;
    let entities = field(objects, model, "NSEntities")?;
    let items = entities
        .get("NS.objects")
        .and_then(Value::as_array)
        .ok_or("Missing entities")?;
    let mut out = Vec::new();
    for item in items {
        let entity = resolve(objects, item)?;
        if entity
            .get("NSSuperentity")
            .and_then(Value::as_uid)
            .is_some_and(|v| v.get() != 0)
        {
            return Err("Entity inheritance is not implemented".into());
        }
        let name = field(objects, entity, "NSEntityName")?
            .as_string()
            .ok_or("Invalid entity name")?
            .to_owned();
        let class = field(objects, entity, "NSClassNameForEntity")?
            .as_string()
            .unwrap_or("NSManagedObject")
            .to_owned();
        let properties = field(objects, entity, "NSProperties")?;
        let properties = properties
            .get("NS.objects")
            .and_then(Value::as_array)
            .ok_or("Invalid entity properties")?;
        let mut attributes = Vec::new();
        for property in properties {
            let property = resolve(objects, property)?;
            let class_desc = field(objects, property, "$class")?;
            if class_desc.get("$classname").and_then(Value::as_string)
                != Some("NSAttributeDescription")
            {
                return Err("Relationships and fetched properties are not implemented".into());
            }
            let kind = property
                .get("NSAttributeType")
                .and_then(Value::as_signed_integer)
                .ok_or("Missing attribute type")? as i32;
            if !supported_type(kind) {
                return Err(format!("Unsupported attribute type {kind}"));
            }
            let name = field(objects, property, "NSPropertyName")?
                .as_string()
                .ok_or("Invalid property name")?
                .to_owned();
            if property.get("NSIsTransient").and_then(Value::as_boolean)==Some(true) {
                return Err("Transient attributes are not implemented".into());
            }
            let optional = property
                .get("NSIsOptional")
                .and_then(Value::as_boolean)
                .unwrap_or(false);
            let default = match property.get("NSDefaultValue") {
                None => None,
                Some(v) if v.as_uid().is_some_and(|v| v.get() == 0) => None,
                Some(v) => {
                    let value = resolve(objects, v)?;
                    if !matches!(
                        value,
                        Value::String(_)
                            | Value::Integer(_)
                            | Value::Real(_)
                            | Value::Boolean(_)
                            | Value::Data(_)
                    ) {
                        return Err("Unsupported archived default value".into());
                    }
                    Some(value.clone())
                }
            };
            attributes.push(Attribute {
                name,
                kind,
                optional,
                default,
            });
        }
        out.push(Entity {
            name,
            class,
            attributes,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use plist::{Dictionary, Uid};

    fn reference(index: u64) -> Value {
        Value::Uid(Uid::new(index))
    }
    fn dictionary(values: &[(&str, Value)]) -> Value {
        Value::Dictionary(
            values
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        )
    }

    // An ordinary keyed archive graph, including the dictionary indirection,
    // class descriptors and backreferences emitted by the model compiler.
    // This is intentionally independent of any particular app's model.
    fn archive(relationship: bool) -> Vec<u8> {
        let mut objects = vec![
            Value::String("$null".into()),
            dictionary(&[("NSEntities", reference(2)), ("$class", reference(13))]),
            dictionary(&[
                ("NS.keys", Value::Array(vec![reference(3)])),
                ("NS.objects", Value::Array(vec![reference(4)])),
            ]),
            Value::String("Entry".into()),
            dictionary(&[
                ("NSEntityName", reference(3)),
                ("NSClassNameForEntity", reference(5)),
                ("NSSuperentity", reference(0)),
                ("NSProperties", reference(6)),
                ("$class", reference(14)),
            ]),
            Value::String("NSManagedObject".into()),
            dictionary(&[
                ("NS.keys", Value::Array(vec![reference(7), reference(8)])),
                (
                    "NS.objects",
                    Value::Array(vec![reference(9), reference(10)]),
                ),
            ]),
            Value::String("timestamp".into()),
            Value::String("title".into()),
            dictionary(&[
                ("NSPropertyName", reference(7)),
                ("NSAttributeType", Value::Integer(900i64.into())),
                ("NSEntity", reference(4)),
                ("NSDefaultValue", reference(0)),
                ("$class", reference(11)),
            ]),
            dictionary(&[
                ("NSPropertyName", reference(8)),
                ("NSAttributeType", Value::Integer(700i64.into())),
                ("NSEntity", reference(4)),
                ("NSIsOptional", Value::Boolean(true)),
                ("NSDefaultValue", reference(12)),
                ("$class", reference(11)),
            ]),
            dictionary(&[
                ("$classname", Value::String("NSAttributeDescription".into())),
                (
                    "$classes",
                    Value::Array(vec![
                        Value::String("NSAttributeDescription".into()),
                        Value::String("NSPropertyDescription".into()),
                        Value::String("NSObject".into()),
                    ]),
                ),
            ]),
            Value::String("untitled".into()),
            dictionary(&[("$classname", Value::String("NSManagedObjectModel".into()))]),
            dictionary(&[("$classname", Value::String("NSEntityDescription".into()))]),
        ];
        if relationship {
            objects.push(dictionary(&[
                (
                    "$classname",
                    Value::String("NSRelationshipDescription".into()),
                ),
                (
                    "$classes",
                    Value::Array(vec![
                        Value::String("NSRelationshipDescription".into()),
                        Value::String("NSPropertyDescription".into()),
                        Value::String("NSObject".into()),
                    ]),
                ),
            ]));
            objects[10] = dictionary(&[
                ("NSPropertyName", reference(8)),
                ("NSEntity", reference(4)),
                ("NSDestinationEntity", reference(4)),
                ("NSIsOptional", Value::Boolean(true)),
                ("NSIsToMany", Value::Boolean(false)),
                ("$class", reference(15)),
            ]);
        }
        let mut archive = Dictionary::new();
        archive.insert("$version".into(), Value::Integer(100000i64.into()));
        archive.insert("$archiver".into(), Value::String("NSKeyedArchiver".into()));
        archive.insert("$objects".into(), Value::Array(objects));
        archive.insert("$top".into(), dictionary(&[("root", reference(1))]));
        let mut bytes = Vec::new();
        Value::Dictionary(archive)
            .to_writer_binary(&mut bytes)
            .unwrap();
        bytes
    }

    #[test]
    fn compiled_model_resolves_entity_and_date_string_attributes() {
        let entities = parse(&archive(false)).unwrap();
        assert_eq!(entities.len(), 1);
        let entity = &entities[0];
        assert_eq!(entity.name, "Entry");
        assert_eq!(entity.class, "NSManagedObject");
        assert_eq!(entity.attributes.len(), 2);
        let date = &entity.attributes[0];
        assert_eq!(
            (date.name.as_str(), date.kind, date.optional),
            ("timestamp", 900, false)
        );
        assert!(date.default.is_none());
        let title = &entity.attributes[1];
        assert_eq!(
            (title.name.as_str(), title.kind, title.optional),
            ("title", 700, true)
        );
        assert_eq!(
            title.default.as_ref().and_then(Value::as_string),
            Some("untitled")
        );
    }

    #[test]
    fn compiled_model_rejects_relationship_instead_of_dropping_it() {
        let error = parse(&archive(true)).unwrap_err();
        assert!(error.contains("Relationships"), "{error}");
    }
}

/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
//! A private SQLite representation for supported Core Data attribute models.
//! Apple explicitly documents that its SQLite representation is private:
//! https://developer.apple.com/library/archive/documentation/Cocoa/Conceptual/CoreData/PersistentStoreFeatures.html
//! Never overwrite/import an unrecognized store or claim migration succeeded.
use plist::{Dictionary, Value};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

#[derive(Clone, Debug)]
pub(super) struct Record {
    pub id: i64,
    pub entity: String,
    pub attributes: Dictionary,
}

pub(super) struct Store {
    connection: Connection,
}
impl Store {
    pub fn open(path: Option<&Path>, model: &str) -> Result<Self, String> {
        let mut connection = match path {
            Some(path) => Connection::open(path),
            None => Connection::open_in_memory(),
        }
        .map_err(|e| e.to_string())?;
        let tables: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if tables != 0 {
            let saved: String = connection
                .query_row(
                    "SELECT model FROM vetus_coredata_metadata WHERE version=1",
                    [],
                    |r| r.get(0),
                )
                .map_err(|_| "Unrecognized or unsupported Core Data store".to_string())?;
            if saved != model {
                return Err(
                    "Persistent store model is incompatible; migration is not implemented".into(),
                );
            }
        } else {
            let tx = connection.transaction().map_err(|e| e.to_string())?;
            tx.execute_batch("CREATE TABLE vetus_coredata_metadata(version INTEGER PRIMARY KEY,model TEXT NOT NULL); CREATE TABLE vetus_coredata_objects(id INTEGER PRIMARY KEY AUTOINCREMENT,entity TEXT NOT NULL,attributes BLOB NOT NULL);")
                .map_err(|e|e.to_string())?;
            tx.execute(
                "INSERT INTO vetus_coredata_metadata(version,model) VALUES(1,?)",
                [model],
            )
            .map_err(|e| e.to_string())?;
            tx.commit().map_err(|e| e.to_string())?;
        }
        Ok(Self { connection })
    }
    pub fn records(&self) -> Result<Vec<Record>, String> {
        let mut stmt = self
            .connection
            .prepare("SELECT id,entity,attributes FROM vetus_coredata_objects ORDER BY id")
            .map_err(|e| e.to_string())?;
        let values = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        values
            .map(|v| {
                let (id, entity, bytes) = v.map_err(|e| e.to_string())?;
                let attributes = Value::from_reader(std::io::Cursor::new(bytes))
                    .map_err(|e| e.to_string())?
                    .into_dictionary()
                    .ok_or("Invalid stored attributes")?;
                Ok(Record {
                    id,
                    entity,
                    attributes,
                })
            })
            .collect()
    }
    /// Commit all object changes together. Return permanent IDs only after
    /// COMMIT succeeds; failed saves leave context changes pending.
    pub fn save(&mut self, records: &[Record], deleted: &[i64]) -> Result<Vec<i64>, String> {
        let tx = self.connection.transaction().map_err(|e| e.to_string())?;
        for id in deleted {
            tx.execute("DELETE FROM vetus_coredata_objects WHERE id=?", [id])
                .map_err(|e| e.to_string())?;
        }
        let mut ids = Vec::new();
        for record in records {
            let mut payload = Vec::new();
            Value::Dictionary(record.attributes.clone())
                .to_writer_binary(&mut payload)
                .map_err(|e| e.to_string())?;
            let id = if record.id == 0 {
                tx.execute(
                    "INSERT INTO vetus_coredata_objects(entity,attributes) VALUES(?,?)",
                    params![record.entity, payload],
                )
                .map_err(|e| e.to_string())?;
                tx.last_insert_rowid()
            } else {
                let exists = tx
                    .query_row(
                        "SELECT id FROM vetus_coredata_objects WHERE id=?",
                        [record.id],
                        |r| r.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                if exists.is_none() {
                    return Err("Object was removed by another context".into());
                }
                tx.execute(
                    "UPDATE vetus_coredata_objects SET attributes=? WHERE id=?",
                    params![payload, record.id],
                )
                .map_err(|e| e.to_string())?;
                record.id
            };
            ids.push(id);
        }
        tx.commit().map_err(|e| e.to_string())?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sqlite_roundtrip_update_delete_and_model_validation() {
        let path = std::env::temp_dir().join(format!(
            "vetus-coredata-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut values = Dictionary::new();
        values.insert("title".into(), Value::String("first".into()));
        let mut record = Record {
            id: 0,
            entity: "Entry".into(),
            attributes: values,
        };
        let mut store = Store::open(Some(&path), "model1").unwrap();
        record.id = store.save(&[record.clone()], &[]).unwrap()[0];
        drop(store);
        let mut store = Store::open(Some(&path), "model1").unwrap();
        assert_eq!(
            store.records().unwrap()[0].attributes["title"].as_string(),
            Some("first")
        );
        record
            .attributes
            .insert("title".into(), Value::String("updated".into()));
        store.save(&[record.clone()], &[]).unwrap();
        assert_eq!(
            store.records().unwrap()[0].attributes["title"].as_string(),
            Some("updated")
        );
        store.save(&[], &[record.id]).unwrap();
        assert!(store.records().unwrap().is_empty());
        drop(store);
        assert!(Store::open(Some(&path), "model2").is_err());
        std::fs::remove_file(path).unwrap();
    }
}

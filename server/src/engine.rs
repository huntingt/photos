//! Driver Logic for Album Fragments
//!
//! Album fragments are the pieces that form the structural metadata of an album that is sent to
//! the end user. Since this metadata can be rather large, fragments allow it to be broken up so
//! that only the necessary data is sent to the user. Additionally, they are immutable to make
//! caching easier.
//!
//! Fragments are either `Section`s or `Top`s, and are identified by a `fragment_id` that is scoped
//! to the album that they are located in. Each album has a single `Top` fragment that lists all of
//! its component sections and their respective `fragment_id`s. Each section then contains a list
//! of resident files.

use crate::common::File;
use crate::error::ApiError;
use chrono::{offset::Utc, TimeZone};
use serde::{
    de::{Deserializer, SeqAccess, Visitor},
    Deserialize,
};
use serde::{
    ser::{SerializeSeq, Serializer},
    Serialize,
};
use sled::transaction::{ConflictableTransactionResult, TransactionalTree};
use std::collections::BTreeMap;
use std::fmt;
use wire::Album;

#[derive(PartialEq, PartialOrd, Eq, Ord, Debug)]
struct FileKey {
    time_stamp: i64,
    file_id: String,
}

#[derive(PartialEq, Eq, Debug)]
struct FileDetails {
    width: i32,
    height: i32,
}

#[derive(PartialEq, Eq, Debug)]
struct SectionDetails {
    fragment_id: u64,
    length: usize,
}

#[derive(PartialEq, Eq, Debug)]
struct Section(BTreeMap<FileKey, FileDetails>);

#[derive(PartialEq, Eq, Debug)]
struct Top(BTreeMap<i64, SectionDetails>);

impl Serialize for Section {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for (key, details) in &self.0 {
            seq.serialize_element(&(key.time_stamp, &key.file_id, details.width, details.height))?;
        }
        seq.end()
    }
}

struct SectionVisitor;

impl<'de> Visitor<'de> for SectionVisitor {
    type Value = Section;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "a fragment")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut btree = BTreeMap::new();

        while let Some((time_stamp, file_id, width, height)) = seq.next_element()? {
            btree.insert(
                FileKey {
                    time_stamp,
                    file_id,
                },
                FileDetails { width, height },
            );
        }

        Ok(Section(btree))
    }
}

impl<'de> Deserialize<'de> for Section {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(SectionVisitor)
    }
}

impl Serialize for Top {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(self.0.len()))?;
        for (ts, details) in &self.0 {
            seq.serialize_element(&(ts, details.fragment_id, details.length))?;
        }
        seq.end()
    }
}

struct TopVisitor;

impl<'de> Visitor<'de> for TopVisitor {
    type Value = Top;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "top listing of section entries")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut btree = BTreeMap::new();

        while let Some((ts, fragment_id, length)) = seq.next_element()? {
            btree.insert(
                ts,
                SectionDetails {
                    fragment_id,
                    length,
                },
            );
        }

        Ok(Top(btree))
    }
}

impl<'de> Deserialize<'de> for Top {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(TopVisitor)
    }
}

pub struct Engine<'a, 'b, 'c, 'd, 'e> {
    album_id: &'a str,
    album: &'b mut Album<'c, 'd>,
    fragments: &'e TransactionalTree,
    /// Cache of `Section`s and their original `fragment_id`s if the `Section` already existed in
    /// the database. Amortizes serialization and `fragment_id` allocation by batching changes.
    cache: BTreeMap<i64, (Option<u64>, Section)>,
    top: Top,
}

type EngineResult<T> = ConflictableTransactionResult<T, ApiError>;

impl<'a, 'b, 'c, 'd, 'e> Engine<'a, 'b, 'c, 'd, 'e> {
    /// # Example
    /// ```rust
    /// # use mod::test::dummy_db;
    /// # let db = dummy_db();
    /// db.transaction(|fragments| {
    ///     /// Set up album ...
    ///     engine::empty("album_id", fragments)?;
    ///     Ok(())
    /// })?;
    /// # Ok::<_, ApiError>(());
    /// ```
    pub fn empty(album_id: &str, fragments: &TransactionalTree) -> EngineResult<u64> {
        let id = Engine::get_id(album_id, 0);
        let json = serde_json::to_string(&Top(BTreeMap::new())).unwrap();
        fragments.insert(id, json.as_bytes())?;
        Ok(0)
    }

    /// # Example
    /// ```rust
    /// # use mod::test::{dummy_db, dummy_album, dummy_file};
    /// # let db = dummy_db();
    /// # let album = dummy_album();
    /// # let file = dummy_file();
    /// album = db.transaction(|fragments| {
    ///     /// Need to transactionally load the album for consistency.
    ///     let local_album = album.clone();
    ///
    ///     /// Open a new engine
    ///     let mut e = engine::new("album_id", &mut album, fragments)?;
    ///
    ///     /// Mutate it
    ///     e.add("file_id", &file)?;
    ///
    ///     /// Commit the changes to the database
    ///     e.commit()?;
    ///     Ok(local_album)
    /// })?;
    /// # Ok::<_, ApiError>(());
    /// ```
    pub fn new(
        album_id: &'a str,
        album: &'b mut Album<'c, 'd>,
        fragments: &'e TransactionalTree,
    ) -> EngineResult<Self> {
        let top_id = Self::get_id(album_id, album.fragment_head);
        let top_bytes = fragments.get(top_id)?.unwrap();
        let top = serde_json::from_slice(&top_bytes).unwrap();

        Ok(Engine {
            album_id,
            album,
            fragments,
            cache: BTreeMap::new(),
            top,
        })
    }

    pub fn commit(mut self) -> EngineResult<()> {
        // Exit if no mutations are necessary.
        if self.cache.len() == 0 {
            return Ok(());
        }

        // Otherwise delete the current top
        self.delete(self.album.fragment_head)?;

        for (ts, (maybe_id, section)) in &self.cache {
            // If the section already exists, delete it.
            if let Some(id) = maybe_id {
                self.delete(*id)?;
            };

            let length = section.0.len();

            let prev_length = if length > 0 {
                // Write the section into the next available fragment_id, then update the top.
                self.album.fragment_head += 1;
                self.write(section)?;
                self.top.0.insert(
                    *ts,
                    SectionDetails {
                        fragment_id: self.album.fragment_head,
                        length,
                    },
                )
            } else {
                // Delete the section since it is empty.
                self.top.0.remove(ts)
            }
            .map(|v| v.length)
            .unwrap_or(0);

            self.album.length = self.album.length + length - prev_length;
        }

        self.album.fragment_head += 1;
        self.write(&self.top)?;

        self.album.last_update = Utc::now().timestamp();

        let min = self.top.0.iter().next();
        let max = self.top.0.iter().next_back();
        self.album.date_range = match (min, max) {
            (Some((min, _)), Some((max, _))) => Some((*min, *max)),
            _ => None,
        };

        Ok(())
    }

    pub fn add(&mut self, file_id: &str, file: &File) -> EngineResult<()> {
        let key = FileKey {
            time_stamp: file.metadata.last_modified,
            file_id: file_id.to_owned(),
        };

        let details = FileDetails {
            width: file.width,
            height: file.height,
        };

        self.modify_section(key.time_stamp, |ref mut section| {
            section.0.insert(key, details);
        })?;

        Ok(())
    }

    pub fn remove(&mut self, file_id: &str, file: &File) -> EngineResult<()> {
        let key = FileKey {
            time_stamp: file.metadata.last_modified,
            file_id: file_id.to_owned(),
        };

        self.modify_section(key.time_stamp, |ref mut section| {
            section.0.remove(&key);
        })?;

        Ok(())
    }

    /// Open the section on the day of timestamp `ts` and mutate by `f`.
    fn modify_section<F>(&mut self, ts: i64, f: F) -> EngineResult<()>
    where
        F: FnOnce(&mut Section),
    {
        // Truncate the timestamp to the date in the album's time zone.
        let ts = self
            .album
            .description
            .time_zone
            .timestamp(ts, 0)
            .date()
            .and_hms(0, 0, 0)
            .timestamp();

        if let Some((_, ref mut section)) = self.cache.get_mut(&ts) {
            // Section is already cached.
            f(section);
        } else if let Some(details) = self.top.0.get(&ts) {
            // Section exists, but needs to be deserialized.
            let mut section = self.read(details.fragment_id)?;
            f(&mut section);
            self.cache.insert(ts, (Some(details.fragment_id), section));
        } else {
            // Section needs to be created.
            let mut section = Section(BTreeMap::new());
            f(&mut section);
            self.cache.insert(ts, (None, section));
        }

        Ok(())
    }

    fn read(&self, id: u64) -> EngineResult<Section> {
        let id = Self::get_id(self.album_id, id);
        let bytes = self.fragments.get(id)?.unwrap();
        let section = serde_json::from_slice(&bytes).unwrap();
        Ok(section)
    }

    fn write<T: Serialize>(&self, fragment: &T) -> EngineResult<()> {
        let id = Self::get_id(self.album_id, self.album.fragment_head);
        let json = serde_json::to_string(fragment).unwrap();
        self.fragments.insert(id, json.as_bytes())?;
        Ok(())
    }

    fn delete(&self, id: u64) -> EngineResult<()> {
        let id = Self::get_id(self.album_id, id);
        self.fragments.remove(id)?.unwrap();
        Ok(())
    }

    pub fn get_id(album_id: &str, fragment_id: u64) -> Vec<u8> {
        [album_id.as_bytes(), b".", &fragment_id.to_be_bytes()].concat()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::wire::{AlbumSettings, FileMetadata};
    use std::borrow::Cow;

    #[test]
    fn ser_de_section() {
        let mut s = Section(BTreeMap::new());

        s.0.insert(
            FileKey {
                time_stamp: 0,
                file_id: "a".to_string(),
            },
            FileDetails {
                width: 1,
                height: 2,
            },
        );

        s.0.insert(
            FileKey {
                time_stamp: 3,
                file_id: "b".to_string(),
            },
            FileDetails {
                width: 4,
                height: 5,
            },
        );

        let json = serde_json::to_string(&s).unwrap();
        assert_eq!("[[0,\"a\",1,2],[3,\"b\",4,5]]", &json);

        let s_de = serde_json::from_slice(json.as_bytes()).unwrap();
        assert_eq!(s, s_de);
    }

    #[test]
    fn ser_de_top() {
        let mut t = Top(BTreeMap::new());

        t.0.insert(
            0,
            SectionDetails {
                fragment_id: 4,
                length: 8,
            },
        );
        t.0.insert(
            1,
            SectionDetails {
                fragment_id: 5,
                length: 9,
            },
        );
        t.0.insert(
            2,
            SectionDetails {
                fragment_id: 6,
                length: 10,
            },
        );

        let json = serde_json::to_string(&t).unwrap();
        assert_eq!("[[0,4,8],[1,5,9],[2,6,10]]", &json);

        let t_de: Top = serde_json::from_slice(json.as_bytes()).unwrap();

        assert_eq!(t, t_de);
    }

    fn dummy_file(num: i32, ts: i64) -> File<'static, 'static, 'static> {
        File {
            owner_id: "u0",
            width: 40 + 2 * num,
            height: 41 + 2 * num,
            metadata: FileMetadata {
                last_modified: ts,
                name: Cow::from("name"),
                mime: Cow::from("*/*"),
            },
        }
    }

    fn dummy_album() -> Album<'static, 'static> {
        Album {
            owner_id: Cow::from("u0"),
            fragment_head: 0,
            description: AlbumSettings {
                name: Cow::from("album_name"),
                time_zone: chrono_tz::Asia::Kolkata,
            },
            length: 0,
            last_update: 0,
            date_range: None,
        }
    }

    fn dummy_db() -> sled::Db {
        let db = sled::Config::new().temporary(true).open().unwrap();

        db.transaction(|t| {
            Engine::empty("a", t)?;
            Ok(())
        })
        .unwrap();

        db
    }

    #[test]
    fn engine_add_remove() {
        let db = dummy_db();

        assert_eq!(db.len(), 1);

        let id_0 = dummy_file(0, 0);
        let id_1 = dummy_file(1, 0);
        let mut album = dummy_album();

        album = db
            .transaction(|t| {
                // Sled retries on transaction conflicts, so the album needs to
                // be restarted on every iteration so that it also acts
                // transactionally.
                let mut local_album = album.clone();
                let mut e = Engine::new("a", &mut local_album, t)?;

                // Try to double insert values. Should fail
                e.add("id_0", &id_0)?;
                e.add("id_0", &id_0)?;
                e.add("id_1", &id_1)?;
                e.commit()?;

                // The local copy of the album is assigned back to the main copy
                Ok(local_album)
            })
            .unwrap();

        assert_eq!(db.len(), 2);
        let bytes = db.get(Engine::get_id("a", 1)).unwrap().unwrap();
        assert_eq!(&bytes, b"[[0,\"id_0\",40,41],[0,\"id_1\",42,43]]");

        album = db
            .transaction(|t| {
                let mut local_album = album.clone();
                let mut e = Engine::new("a", &mut local_album, t)?;

                // Remove a single copy and check that only the other is left
                e.remove("id_0", &id_0)?;
                e.commit()?;
                Ok(local_album)
            })
            .unwrap();

        assert_eq!(db.len(), 2);
        let bytes = db.get(Engine::get_id("a", 3)).unwrap().unwrap();
        assert_eq!(&bytes, b"[[0,\"id_1\",42,43]]");

        db.transaction(|t| {
            let mut local_album = album.clone();
            let mut e = Engine::new("a", &mut local_album, t)?;

            // Try to remove a copy that is no longer there, and remove the
            // last file in the section to check that the section is deleted
            e.remove("id_0", &id_0)?;
            e.remove("id_1", &id_1)?;
            e.commit()?;
            Ok(local_album)
        })
        .unwrap();

        assert_eq!(db.len(), 1);
        let bytes = db.get(Engine::get_id("a", 5)).unwrap().unwrap();
        assert_eq!(&bytes, b"[]");
    }

    #[test]
    fn engine_empty_transaction() {
        let db = dummy_db();
        let mut album = dummy_album();

        let first_update = album.last_update;

        album = db
            .transaction(|t| {
                let mut local_album = album.clone();
                let e = Engine::new("a", &mut local_album, t)?;
                e.commit()?;
                Ok(local_album)
            })
            .unwrap();

        assert_eq!(album.fragment_head, 0);
        assert_eq!(album.length, 0);
        assert_eq!(album.last_update, first_update);
    }
}

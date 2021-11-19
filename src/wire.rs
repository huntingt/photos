use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserDetails<'a, 'b> {
    #[serde(borrow)]
    pub email: Cow<'a, str>,
    #[serde(borrow)]
    pub password: Cow<'b, str>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Key<'a> {
    #[serde(borrow)]
    pub key: Cow<'a, str>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionList<'a> {
    #[serde(borrow)]
    pub key_prefixes: Vec<Cow<'a, str>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileMetadata<'a, 'b> {
    pub last_modified: i64,

    #[serde(borrow)]
    pub name: Cow<'a, str>,

    #[serde(borrow)]
    pub mime: Cow<'b, str>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ListRequest<'a> {
    pub prefix: Option<Cow<'a, str>>,
    pub skip: Option<usize>,
    pub length: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileList<'a, 'b> {
    #[serde(borrow)]
    pub files: Vec<(Cow<'a, str>, Cow<'b, str>)>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AlbumSettings<'a> {
    pub name: Cow<'a, str>,
    pub time_zone: chrono_tz::Tz,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NewResource<'a> {
    #[serde(borrow)]
    pub id: Cow<'a, str>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IdList<'a> {
    #[serde(borrow)]
    pub ids: Vec<Cow<'a, str>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Album<'a, 'b> {
    #[serde(rename = "owner")]
    #[serde(borrow)]
    pub owner_id: Cow<'a, str>,
    #[serde(borrow)]
    pub description: AlbumSettings<'b>,
    pub fragment_head: u64,
    pub length: usize,
    pub last_update: i64,
    pub date_range: Option<(i64, i64)>,
}

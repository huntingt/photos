use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct UserDetails<'a> {
    pub email: &'a str,
    pub password: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Key<'a> {
    pub key: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SessionsList {
    pub key_prefixes: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Metadata<'a> {
    pub last_modified: i64,
    pub name: &'a str,
    pub mime: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListParams<'a> {
    pub start: Option<&'a str>,
    pub end: Option<&'a str>,
    pub skip: Option<usize>,
    pub length: Option<usize>,
}

#[derive(Serialize, Debug)]
pub struct FileList<'a> {
    pub files: Vec<(&'a str, &'a str)>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UploadDetails<'a> {
    pub file_id: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AlbumDescription<'a> {
    pub name: &'a str,
    pub time_zone: chrono_tz::Tz,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewAlbumDetails<'a> {
    pub album_id: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileIdList {
    pub file_ids: Vec<String>,
}

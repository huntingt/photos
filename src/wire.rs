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

#[derive(Serialize, Deserialize, Debug)]
pub struct Metadata<'a> {
    pub last_modified: u64,
    pub name: &'a str,
    pub mime: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ListParams<'a> {
    pub start: Option<&'a str>,
    pub length: Option<usize>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FileList {
    pub files: Vec<(String, String)>,
}

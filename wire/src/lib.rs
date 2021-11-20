use serde::{Deserialize, Serialize};
use std::borrow::Cow;

pub trait IntoOwned {
    type Owned;
    fn into_owned(self) -> Self::Owned;
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UserDetails<'a, 'b> {
    #[serde(borrow)]
    pub email: Cow<'a, str>,
    #[serde(borrow)]
    pub password: Cow<'b, str>,
}

impl<'a, 'b> IntoOwned for UserDetails<'a, 'b> {
    type Owned = UserDetails<'static, 'static>;

    fn into_owned(self) -> Self::Owned {
        UserDetails {
            email: Cow::Owned(self.email.into_owned()),
            password: Cow::Owned(self.password.into_owned()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Key<'a> {
    #[serde(borrow)]
    pub key: Cow<'a, str>,
}

impl<'a> IntoOwned for Key<'a> {
    type Owned = Key<'static>;

    fn into_owned(self) -> Self::Owned {
        Key {
            key: Cow::Owned(self.key.into_owned()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionList<'a> {
    #[serde(borrow)]
    pub key_prefixes: Vec<Cow<'a, str>>,
}

impl<'a> IntoOwned for SessionList<'a> {
    type Owned = SessionList<'static>;

    fn into_owned(self) -> Self::Owned {
        SessionList {
            key_prefixes: self.key_prefixes
                .iter()
                .map(|e| Cow::Owned(e.to_string()))
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileMetadata<'a, 'b> {
    pub last_modified: i64,

    #[serde(borrow)]
    pub name: Cow<'a, str>,

    #[serde(borrow)]
    pub mime: Cow<'b, str>,
}

impl<'a, 'b> IntoOwned for FileMetadata<'a, 'b> {
    type Owned = FileMetadata<'static, 'static>;

    fn into_owned(self) -> Self::Owned {
        FileMetadata {
            last_modified: self.last_modified,
            name: Cow::Owned(self.name.into_owned()),
            mime: Cow::Owned(self.mime.into_owned()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ListRequest<'a> {
    pub prefix: Option<Cow<'a, str>>,
    pub skip: Option<usize>,
    pub length: Option<usize>,
}

impl<'a> IntoOwned for ListRequest<'a> {
    type Owned = ListRequest<'static>;

    fn into_owned(self) -> Self::Owned {
        let Self { skip, length, prefix } = self;
        
        let prefix = match prefix {
            Some(e) => Some(Cow::Owned(e.into_owned())),
            None => None,
        };

        ListRequest {
            skip,
            length,
            prefix,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileList<'a, 'b> {
    #[serde(borrow)]
    pub files: Vec<(Cow<'a, str>, Cow<'b, str>)>,
}

impl<'a, 'b> IntoOwned for FileList<'a, 'b> {
    type Owned = FileList<'static, 'static>;

    fn into_owned(self) -> Self::Owned {
        FileList {
            files: self.files
                .iter()
                .map(|(a, b)| (
                        Cow::Owned(a.to_string()),
                        Cow::Owned(b.to_string())
                    ))
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AlbumSettings<'a> {
    pub name: Cow<'a, str>,
    pub time_zone: chrono_tz::Tz,
}

impl<'a> IntoOwned for AlbumSettings<'a> {
    type Owned = AlbumSettings<'static>;

    fn into_owned(self) -> Self::Owned {
        AlbumSettings {
            time_zone: self.time_zone,
            name: Cow::Owned(self.name.into_owned()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NewResource<'a> {
    #[serde(borrow)]
    pub id: Cow<'a, str>,
}

impl<'a> IntoOwned for NewResource<'a> {
    type Owned = NewResource<'static>;

    fn into_owned(self) -> Self::Owned {
        NewResource {
            id: Cow::Owned(self.id.into_owned()),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct IdList<'a> {
    #[serde(borrow)]
    pub ids: Vec<Cow<'a, str>>,
}

impl<'a> IntoOwned for IdList<'a> {
    type Owned = IdList<'static>;

    fn into_owned(self) -> Self::Owned {
        IdList {
            ids: self.ids
                .iter()
                .map(|e| Cow::Owned(e.to_string()))
                .collect(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Album<'a, 'b> {
    #[serde(borrow)]
    pub owner_id: Cow<'a, str>,
    #[serde(borrow)]
    pub description: AlbumSettings<'b>,
    pub fragment_head: u64,
    pub length: usize,
    pub last_update: i64,
    pub date_range: Option<(i64, i64)>,
}

impl<'a, 'b> IntoOwned for Album<'a, 'b> {
    type Owned = Album<'static, 'static>;

    fn into_owned(self) -> Self::Owned {
        Album {
            fragment_head: self.fragment_head,
            length: self.length,
            last_update: self.last_update,
            date_range: self.date_range,
            owner_id: Cow::Owned(self.owner_id.into_owned()),
            description: self.description.into_owned(),
        }
    }
}

#[test]
fn return_cow() {
    fn helper() -> UserDetails<'static, 'static> {
        let email = "email";
        let password = ["pass", "word"].concat();

        UserDetails {
            email: Cow::from(email),
            password: Cow::from(&password),
        }.into_owned()
    }

    let user = helper();

    assert_eq!(&user.email, "email");
    assert_eq!(&user.password, "password");
}

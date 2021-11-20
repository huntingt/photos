mod error;

use crate::error::{Result, ResponseErrorExt};
use reqwest::{Url, Body};
use std::time::UNIX_EPOCH;
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::io::{self, AsyncReadExt};
use bytes::{Bytes, BytesMut};
use async_stream::try_stream;
use futures::stream::Stream;
use wire::*;
use std::borrow::Cow;
use clap::{Arg, App, SubCommand, crate_version, crate_name};
use std::io::Write;
use console::style;
use std::collections::HashSet;

fn file_stream(mut file: fs::File, chunk_size: usize) -> impl Stream<Item = io::Result<Bytes>> {
    try_stream! {
        loop {
            let mut buffer = BytesMut::with_capacity(chunk_size);
            file.read_buf(&mut buffer).await?;

            if buffer.is_empty() {
                break;
            }

            yield buffer.into();
        }
    }
}

const UPLOAD_METADATA: &'static str = "upload-metadata";
/*
impl Context {

    async fn create_album(&self, name: &str) -> Result<String> {
        let json = format!("{{\"name\":\"{}\",\"time_zone\":\"EST\"}}", name);
        let bytes = self.client
            .post(self.build_url("album/create"))
            .body(json)
            .send().await?
            .check_status().await?
            .bytes().await?;
        let json: NewResource = serde_json::from_slice(&bytes)?;
        Ok(json.id.to_string())
    }

    async fn add_to_album(&self, album_id: &str, file_ids: Vec<String>) -> Result<()> {
        self.client
            .post(self.build_url(&format!("album/add/{}", album_id)))
            .json(&IdList { ids: file_ids.iter().map(|e| Cow::from(e)).collect() })
            .send().await?
            .check_status().await?;
        Ok(())
    }

    async fn remove_from_album(&self, album_id: &str, file_ids: Vec<String>) -> Result<()> {
        self.client
            .delete(self.build_url(&format!("album/remove/{}", album_id)))
            .json(&IdList { ids: file_ids.iter().map(|e| Cow::from(e)).collect() })
            .send().await?
            .check_status().await?;
        Ok(())
    }
}
*/

fn prompt_line(prompt: &str) -> String {
    print!("{}", prompt);
    std::io::stdout().flush().unwrap();

    let mut string = String::new();
    std::io::stdin().read_line(&mut string).unwrap();

    string.trim().to_string()
}

pub struct Client {
    pub client: reqwest::Client,
    pub db: sled::Db,
}

impl Client {
    fn new(db_path: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            db: sled::open(db_path).unwrap(),
        }
    }

    fn temp() -> Self {
        Self {
            client: reqwest::Client::new(),
            db: sled::Config::new().temporary(true).open().unwrap(),
        }
    }

    fn get_key(&self) -> Option<String> {
        if let Some(bytes) = self.db.get(b"key").unwrap() {
            let string = std::str::from_utf8(&bytes).unwrap();
            Some(string.to_owned())
        } else {
            None
        }
    }

    fn set_key(&self, key: &str) {
        self.db.insert(b"key", key.as_bytes()).unwrap();
    }

    fn get_url(&self) -> Option<Url> {
        if let Some(bytes) = self.db.get(b"url").unwrap() {
            let string = std::str::from_utf8(&bytes).unwrap();
            Some(Url::parse(string).unwrap())
        } else {
            None
        }
    }

    fn set_url(&self, url: &Url) {
        self.db.insert(b"url", url.to_string().as_bytes()).unwrap();
    }

    fn get_prompt_url(&self) -> Url {
        if let Some(url) = self.get_url() {
            return url;
        }

        loop {
            let string = prompt_line("url: ");
            
            match Url::parse(&string) {
                Ok(url) => {
                    self.set_url(&url);
                    return url;
                },
                Err(err) => eprintln!("{:?}", err),
            };
        }
    }

    fn prompt_user_details(&self) -> UserDetails<'static, 'static> {
        let email = prompt_line("email: ");
        let password = rpassword::prompt_password_stdout("password: ").unwrap();

        UserDetails {
            email: Cow::from(email),
            password: Cow::from(password),
        }
    }

    async fn get_prompt_key(&self) -> String {
        if let Some(key) = self.get_key() {
            return key;
        }

        let user = self.prompt_user_details();
        let key = self.login(&user).await.unwrap();

        key.key.into_owned()
    }
    
    fn build_url(&self, path: &str) -> Url {
        self.get_prompt_url().join(path).unwrap()
    }
    
    async fn build_auth_url(&self, path: &str) -> Url {
        let mut url = self.build_url(path);

        let key = self.get_prompt_key().await;
        url.query_pairs_mut().append_pair("key", &key);

        url
    }

    async fn create_user<'a, 'b>(&self, user: &UserDetails<'a, 'b>) -> Result<()> {
        self.client
            .post(self.build_url("user/create"))
            .json(user)
            .send().await?
            .check_status().await?;

        Ok(())
    }

    async fn login<'a, 'b>(&self, user: &UserDetails<'a, 'b>) -> Result<Key<'static>> {
        let bytes = self.client
            .post(self.build_url("user/login"))
            .json(user)
            .send().await?
            .check_status().await?
            .bytes().await?;
        let json: Key = serde_json::from_slice(&bytes)?;

        self.set_key(&json.key);

        Ok(json.into_owned())
    }

    async fn sessions(&self) -> Result<SessionList<'static>> {
        let bytes = self.client
            .get(self.build_auth_url("user/sessions").await)
            .send().await?
            .check_status().await?
            .bytes().await?;
        let json: SessionList = serde_json::from_slice(&bytes)?;
        Ok(json.into_owned())
    }

    async fn logout(&self, prefix: Option<&str>) -> Result<()> {
        let db_key = self.get_key();
        let key = match (&db_key, prefix) {
            (None, None) => return Ok(()),
            (_, Some(k)) => k,
            (Some(k), None) => k,
        };

        self.client
            .delete(self.build_auth_url("user/logout").await)
            .json(&Key { key: Cow::from(key) })
            .send().await?
            .check_status().await?;

        if let Some(current_key) = &self.get_key() {
            if let (Some((_,ck)), Some((_,k))) = (current_key.split_once('.'), key.split_once('.')) {
                if ck.starts_with(k) {
                    self.db.remove(b"key").unwrap();
                }
            }
        }

        Ok(())
    }

    async fn file_list<'a>(&self, req: &ListRequest<'a>) -> Result<FileList<'static, 'static>> {
        let bytes = self.client
            .get(self.build_auth_url("file/list").await)
            .json(req)
            .send().await?
            .check_status().await?
            .bytes().await?;
        let json: FileList = serde_json::from_slice(&bytes)?;
        Ok(json.into_owned())
    }

    async fn upload(&self, path: &Path, json: Option<&Path>) -> Result<NewResource<'static>> {
        let mime = mime_guess::from_path(path).first_or_octet_stream();

        let o_time_stamp = json.map(|json_path| {
            let file = std::fs::File::open(json_path).ok()?;
            let value: serde_json::Value = serde_json::from_reader(file).ok()?;
            value.get("creationTime")?.get("timestamp")?.as_str()?.parse::<i64>().ok()
        }).flatten();

        let time_stamp = if let Some(ts) = o_time_stamp {
            ts
        } else {
            let modified = fs::metadata(path).await?.modified().unwrap();
            modified.duration_since(UNIX_EPOCH)
                .expect("This timestamp doesn't make sense")
                .as_secs() as i64
        };        

        let name = path.file_name().unwrap().to_str()
            .expect("Only support unicode file names");

        let metadata = serde_json::to_string(&FileMetadata {
            last_modified: time_stamp,
            name: Cow::from(name),
            mime: Cow::from(mime.essence_str()),
        }).unwrap();
        let metadata_header = base64::encode_config(metadata.as_bytes(), base64::URL_SAFE);

        let file = fs::File::open(path).await.unwrap();
        let body = Body::wrap_stream(file_stream(file, 1024 * 8));

        let bytes = self.client
            .post(self.build_auth_url("file/upload").await)
            .header(UPLOAD_METADATA, metadata_header)
            .body(body)
            .send().await?
            .check_status().await?
            .bytes().await?;
        let json: NewResource = serde_json::from_slice(&bytes)?;

        Ok(json.into_owned())
    }

    async fn upload_dir(&self, dir: &Path) -> Result<Vec<String>> {
        let mut iter = fs::read_dir(dir).await?;
        let mut file_paths = HashSet::new();

        println!("Uploading {:?}...", dir);

        while let Some(entry) = iter.next_entry().await? {
            if entry.file_type().await?.is_file() {
                file_paths.insert(entry.path());
            }
        }

        let extended: Vec<_> = file_paths
            .iter()
            .filter(|p| !p.to_str().unwrap().ends_with(".json"))
            .map(|p| {
                let mut os_string = p.clone().into_os_string();
                os_string.push(".json");
                let json = PathBuf::from(os_string);
                if file_paths.contains(&json) {
                    (p, Some(json))
                } else {
                    (p, None)
                }
            })
            .collect();

        let bar = indicatif::ProgressBar::new(extended.len() as u64);
        let mut file_ids = vec![];
        let mut errors = vec![];

        for (path, json) in extended.iter() {
            match self.upload(&path, json.as_ref().map(|p| p.as_path())).await {
                Ok(new) => {
                    file_ids.push(new.id.into_owned());
                },
                Err(_) => {
                    errors.push(path);
                },
            }
            bar.inc(1);
        }
        bar.finish();

        for path in errors.iter()  {
            eprintln!("Couldn't upload: {:?}", path);
        }

        Ok(file_ids)
    }

    async fn create_album<'a>(&self, settings: &AlbumSettings<'a>) -> Result<String> {
        let bytes = self.client
            .post(self.build_auth_url("album/create").await)
            .json(settings)
            .send().await?
            .check_status().await?
            .bytes().await?;
        let json: NewResource = serde_json::from_slice(&bytes)?;
        Ok(json.id.to_string())
    }

    async fn add_to_album(&self, album_id: &str, file_ids: &Vec<String>) -> Result<()> {
        self.client
            .post(self.build_auth_url(&format!("album/add/{}", album_id)).await)
            .json(&IdList { ids: file_ids.iter().map(|e| Cow::from(e)).collect() })
            .send().await?
            .check_status().await?;
        Ok(())
    }

    async fn remove_from_album(&self, album_id: &str, file_ids: &Vec<String>) -> Result<()> {
        self.client
            .delete(self.build_auth_url(&format!("album/remove/{}", album_id)).await)
            .json(&IdList { ids: file_ids.iter().map(|e| Cow::from(e)).collect() })
            .send().await?
            .check_status().await?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let matches = App::new(crate_name!())
        .version(crate_version!())
        .arg(Arg::with_name("database")
            .short("d")
            .takes_value(true))
        .arg(Arg::with_name("temp")
            .short("t")
            .long("temp"))
        .arg(Arg::with_name("url")
            .takes_value(true))
        .subcommand(SubCommand::with_name("create"))
        .subcommand(SubCommand::with_name("login"))
        .subcommand(SubCommand::with_name("sessions"))
        .subcommand(SubCommand::with_name("logout")
            .arg(Arg::with_name("prefix")
                .index(1)
                .takes_value(true)))
        .subcommand(SubCommand::with_name("upload")
            .arg(Arg::with_name("add")
                .short("a")
                .long("add")
                .takes_value(true))
            .arg(Arg::with_name("path")
                .required(true)
                .index(1)))
        .subcommand(SubCommand::with_name("list")
            .arg(Arg::with_name("prefix")
                .index(1)
                .takes_value(true))
            .arg(Arg::with_name("add")
                .short("a")
                .long("add")
                .takes_value(true))
            .arg(Arg::with_name("remove")
                .short("r")
                .long("remove")
                .takes_value(true))
            .arg(Arg::with_name("skip")
                .short("s")
                .takes_value(true))
            .arg(Arg::with_name("length")
                .short("l")
                .takes_value(true)))
        .subcommand(SubCommand::with_name("album")
            .subcommand(SubCommand::with_name("create")
                .arg(Arg::with_name("name")
                    .index(1)
                    .required(true)
                    .takes_value(true))
                .arg(Arg::with_name("timezone")
                    .short("tz")
                    .takes_value(true))))
        .get_matches();

    let client = if matches.value_of("temp").is_none() {
        let db_path = matches.value_of("database").unwrap_or(".sync");
        Client::new(db_path)
    } else {
        Client::temp()
    };

    if let Some(url) = matches.value_of("url") {
        client.set_url(&Url::parse(url).unwrap());
    }

    if let Some(_) = matches.subcommand_matches("create") {
        let user = client.prompt_user_details();
        client.create_user(&user).await?;
        println!("Created user");
        client.login(&user).await?;
        println!("Logged in");
    } else if let Some(_) = matches.subcommand_matches("login") {
        let user = client.prompt_user_details();
        client.login(&user).await?;
        println!("Logged in");
    } else if let Some(_) = matches.subcommand_matches("sessions") {
        for (i, key) in client.sessions().await?.key_prefixes.iter().enumerate() {
            let (start, end) = key.split_once('.').unwrap();
            print!("{}\t{}.{}", style(i).bold().dim(), style(start).dim(), end);

            if let Some(my_key) = client.get_key() {
                let key: &str = &key;
                if my_key.starts_with(key) {
                    println!(" [*]");
                    continue;
                }
            }
            println!("");
        }
    } else if let Some(matches) = matches.subcommand_matches("logout") {
        client.logout(matches.value_of("prefix")).await?;
    } else if let Some(matches) = matches.subcommand_matches("upload") {
        let path = Path::new(matches.value_of("path").unwrap());
        
        let file_ids = if path.is_file() {
            vec![client.upload(path, None).await?.id.to_string()]
        } else {
            client.upload_dir(path).await?
        };


        if let Some(album) = matches.value_of("add") {
            client.add_to_album(&album, &file_ids).await?;
        }
    } else if let Some(matches) = matches.subcommand_matches("list") {
        let request = ListRequest {
            prefix: matches.value_of("prefix").map(|e| Cow::from(e)),
            skip: matches.value_of("skip").map(|e| e.parse().ok()).flatten(),
            length: matches.value_of("length").map(|e| e.parse().ok()).flatten(),
        };

        let json = client.file_list(&request).await?;

        let mut file_ids = vec![];

        for (i, (name, id)) in json.files.iter().enumerate() {
            let i = i + request.skip.unwrap_or(0);
            print!("{}", style(i).bold().dim());
            println!("\t{: <40} {}", name, style(id).dim());

            file_ids.push(id.to_string());
        }

        if let Some(album) = matches.value_of("add") {
            client.add_to_album(&album, &file_ids).await?;
        }

        if let Some(album) = matches.value_of("remove") {
            client.remove_from_album(&album, &file_ids).await?;
        }
    } else if let Some(matches) = matches.subcommand_matches("album") {
        if let Some(matches) = matches.subcommand_matches("create") {
            let settings = AlbumSettings {
                name: Cow::from(matches.value_of("name").unwrap()),
                time_zone: matches.value_of("timezone").unwrap_or("EST").parse().unwrap(),
            };

            let id = client.create_album(&settings).await?;
            println!("Created album id={}", id);
        }
    }
    
    Ok(())
}

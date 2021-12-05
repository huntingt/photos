use serde::{Serialize, Deserialize};
use crate::{
    error::{ApiResult},
    common::{File, AppState, User},
    album::engine::Engine,
};
use wire::Album;
use sled::Transactional;

#[derive(Serialize, Deserialize, Debug)]
pub enum Command<'a> {
    Album(&'a str),
    File(&'a str, File<'a, 'a, 'a>),
    User(&'a str),
}

impl<'a> Command<'a> {
    pub fn run(&self, state: &AppState) -> ApiResult<()> {
        let cmd_id = state.db.generate_id()?.to_be_bytes();
        let cmd_bytes = bincode::serialize(self).unwrap();

        state.delete.insert(&cmd_id, cmd_bytes)?;

        self.finish(state, &cmd_id)
    }

    fn finish(&self, state: &AppState, cmd_id: &[u8]) -> ApiResult<()> {
        use Command::*;

        match self {
            Album(album_id) => delete_album(state, album_id),
            File(file_id, file) => delete_file(state, file_id, file),
            User(user_id) => delete_user(state, user_id),
        }?;

        state.delete.remove(cmd_id)?;

        Ok(())
    }

    pub fn restore(state: &AppState) -> ApiResult<()> {
        for entry in state.delete.iter() {
            let (key, value) = entry?;

            let cmd: Command = bincode::deserialize(&value).unwrap();
            cmd.finish(state, &key)?;
        }

        Ok(())
    }
}

fn delete_album(state: &AppState, album_id: &str) -> ApiResult<()> {
    let AppState {
        ref albums,
        ref album_to_user,
        ref user_to_album,
        ref inclusions,
        ref fragments,
        ..
    } = state;

    albums.remove(album_id)?;

    let prefix = [album_id, "."].concat();

    // I chose to first remove access to the album before deleting all of the
    // resources. Resource fetch can still fail, however, in certain race conditions.
    for entry in album_to_user.scan_prefix(&prefix) {
        let (key, _) = entry?;

        let (_, user_id) = std::str::from_utf8(&key)
            .unwrap()
            .split_once(".")
            .unwrap();

        (album_to_user, user_to_album).transaction(|(album_to_user, user_to_album)| {
            album_to_user.remove(key.clone())?;
            user_to_album.remove([user_id, ".", album_id].concat().as_bytes())?;

            Ok(())
        })?;
    }

    for entry in fragments.scan_prefix(&prefix) {
        let (key, _) = entry?;
        fragments.remove(key)?;
    }

    for entry in inclusions.scan_prefix(&prefix) {
        let (key, _) = entry?;
        inclusions.remove(key)?;
    }

    Ok(())
}

fn delete_file(state: &AppState, file_id: &str, file: &File) -> ApiResult<()> {
    let AppState {
        ref files,
        ref file_names,
        ref albums,
        ref fragments,
        ref inclusions,
        ref upload_path,
        ref medium_path,
        ref small_path,
        ..
    } = state;

    (files, file_names).transaction(|(files, file_names)| {
        files.remove(file_id)?;
        file_names.remove([file.owner_id, ".", &file.metadata.name].concat().as_bytes())?;

        Ok(())
    })?;

    for entry in inclusions.scan_prefix([file_id, "."].concat()) {
        let (key, _) = entry?;
        let (_, album_id) = std::str::from_utf8(&key)
            .unwrap()
            .split_once(".")
            .unwrap();

        (albums, fragments, inclusions).transaction(|(albums, fragments, inclusions)| {
            if let Some(album_bytes) = albums.get(album_id)? {
                let mut album: Album = bincode::deserialize(&album_bytes).unwrap();

                let mut e = Engine::new(album_id, &mut album, fragments)?;
                e.remove(file_id, file)?;
                e.commit()?;

                let album_bytes = bincode::serialize(&album).unwrap();
                albums.insert(album_id, album_bytes)?;

                // Ok that we didn't check to see if the file was still in the album
                // because the remove operation is idempotent.
                inclusions.remove(key.clone())?;
            }

            Ok(())
        })?;
    }

    let upload_path = upload_path.join(file_id);
    let medium_path = medium_path.join(file_id);
    let small_path = small_path.join(file_id);

    let _ = std::fs::remove_file(upload_path);
    let _ = std::fs::remove_file(medium_path);
    let _ = std::fs::remove_file(small_path);
    
    Ok(())
}

fn delete_user(state: &AppState, user_id: &str) -> ApiResult<()> {
    let AppState {
        ref users,
        ref emails,
        ref sessions,
        ref inclusions,
        ref files,
        ref user_to_album,
        ..
    } = state;

    (users, emails).transaction(|(users, emails)| {
        if let Some(user_bytes) = users.remove(user_id)? {
            let user: User = bincode::deserialize(&user_bytes).unwrap();
            emails.remove(user.email)?;
        }

        Ok(())
    })?;

    for entry in sessions.scan_prefix([user_id, "."].concat()) {
        let (key, _) = entry?;
        sessions.remove(key)?;
    }

    // Delete albums first because this will reduce the number of recalculations
    // that individual file removals will cause.
    for entry in user_to_album.scan_prefix([user_id, "."].concat()) {
        let (key, _) = entry?;
        let (_, album_id) = std::str::from_utf8(&key)
            .unwrap()
            .split_once(".")
            .unwrap();

        Command::Album(album_id).run(state)?;
    }

    // TODO: consider just unsharing the user from each of the albums that they are
    // in to make this even more efficient
    for entry in inclusions.scan_prefix([user_id, "."].concat()) {
        let (_, value) = entry?;
        let file_id = std::str::from_utf8(&value).unwrap();
        if let Some(file_bytes) = files.get(file_id)? {
            let file: File = bincode::deserialize(&file_bytes).unwrap();

            // Need to preserve the file so that it can be tracked down in
            // the album using the timestamp.
            Command::File(file_id, file).run(state)?;
        }
    }

    Ok(())
}

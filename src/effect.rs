//! Effects: the IO the reducer asks for. The reducer stays pure; it pushes an `Effect` onto
//! `App.effects`, and only `run` (the loop in `lib.rs`) drains them and spawns the task that does
//! the request and sends `Action`s back on the one channel (the same shape as `event.rs`).

use crate::action::Action;
use crate::api::{ApiError, Client};
use crate::data::{RoomId, RoomKind};
use tokio::sync::mpsc::Sender;

/// The text of a message being sent or saved as an edit, exactly as typed. Its `Debug` prints only
/// the byte length, so the derived `Debug` of `Effect` can never print message text (D-14, T-04-01).
#[derive(Clone, PartialEq)]
pub struct SendText(String);

impl SendText {
    pub fn new(text: String) -> Self {
        SendText(text)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for SendText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SendText(<{}> bytes)", self.0.len())
    }
}

/// A reaction's shortcode (without colons) on its way to the server. Like `SendText`, its `Debug`
/// prints only a byte length, so the derived `Debug` of `Effect` never prints a reaction name.
#[derive(Clone, PartialEq)]
pub struct EmojiName(String);

impl EmojiName {
    pub fn new(name: String) -> Self {
        EmojiName(name)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for EmojiName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EmojiName(<{}> bytes)", self.0.len())
    }
}

/// Which custom emoji image to fetch: its name, the `etag` the server lists for it and the file
/// extension. All three are server text, so like `EmojiName` its `Debug` prints only lengths and
/// the derived `Debug` of `Effect` and `Action` can never print them.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct EmojiRef {
    pub name: String,
    pub etag: String,
    pub ext: String,
}

impl std::fmt::Debug for EmojiRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EmojiRef(<{}> bytes)", self.name.len())
    }
}

/// One IO intent: a fetch, or one send attempt.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// Load the joined rooms and the sidebar folders.
    LoadRooms,
    /// Load the user's own username with `GET /api/v1/me`, once per connection (D-17): at startup
    /// and on `:reconnect`, never per send. Best effort: a failure reports nothing.
    LoadMe,
    /// Load a page of a room's history: the newest `count` messages, or, with `before`, the page
    /// strictly older than that raw server timestamp.
    LoadHistory {
        room: RoomId,
        kind: RoomKind,
        private: bool,
        count: u16,
        before: Option<String>,
    },
    /// One page of the replies of thread `parent_id` of `room` (`Client::load_thread`), `count` of
    /// them from `offset`; with `fetch_parent` the parent message is fetched too (`chat.getMessage`),
    /// for a thread whose parent the main flow does not hold. Answers `Action::ThreadLoaded` or
    /// `Action::ThreadLoadFailed`, addressed by `room` and `parent_id`.
    LoadThread {
        room: RoomId,
        parent_id: String,
        offset: u32,
        count: u16,
        fetch_parent: bool,
    },
    /// One page of the thread list of `room` (`Client::list_threads`): `count` threads from
    /// `offset` under `filter` and the search text `query`. Answers `Action::ThreadListPage`,
    /// addressed by `room` and `generation`. The text is a `SendText`, so `Debug` never prints it.
    ListThreads {
        room: RoomId,
        filter: crate::api::ThreadFilter,
        query: SendText,
        generation: u64,
        offset: u32,
        count: u16,
    },
    /// The parent message of a thread chosen in the thread list (`chat.getMessage`), for a thread
    /// whose parent the main flow does not hold. Answers `Action::ThreadListParent`.
    FetchThreadParent { room: RoomId, parent_id: String },
    /// One page of the server search of `room`: `GET chat.search`, `count` results from `offset`.
    /// Answers `Action::SearchPage`, addressed by `room` and `search_id`. The query is a
    /// `SendText`, so `Debug` never prints it.
    SearchRoom {
        room: RoomId,
        query: SendText,
        search_id: u64,
        offset: u32,
        count: u16,
    },
    /// The history around message `id` of `room` (`Client::load_context`): `before` messages
    /// before it and `after` after it, `raw_ts` being its raw server timestamp. Answers
    /// `Action::ContextLoaded` or `Action::ContextFailed`, addressed by `room` and `context_id`.
    LoadContext {
        room: RoomId,
        kind: RoomKind,
        private: bool,
        id: String,
        raw_ts: String,
        context_id: u64,
        before: u16,
        after: u16,
    },
    /// The next page of messages after `raw_ts` for a detached room (`Client::load_newer`), `n` of
    /// them. Answers `Action::NewerLoaded` or `Action::NewerFailed`, addressed by `room` and
    /// `newer_id`.
    LoadNewer {
        room: RoomId,
        kind: RoomKind,
        private: bool,
        raw_ts: String,
        n: u16,
        newer_id: u64,
    },
    /// One attempt to send one pending message to `room`: `POST chat.sendMessage` with the client
    /// `id` as the message `_id` (D-07, D-15). With `reconcile` (a retry of an `Unconfirmed`
    /// message) the server is asked first with `GET chat.getMessage`: a 200 is the answer and no
    /// POST is sent; after a failing POST it is asked once more, and a 200 still counts as sent. The
    /// answer comes back as `Action::MessageSent` or `Action::SendFailed`, both addressed by `room`
    /// and `id`.
    SendMessage {
        room: RoomId,
        id: String,
        text: SendText,
        reconcile: bool,
        /// The parent of the thread this message replies to, when it is a thread reply.
        tmid: Option<String>,
        /// A thread reply that is also shown in the channel.
        tshow: bool,
    },
    /// Replace the text of message `id` of `room` with `POST chat.update`. Not a send: it uses no
    /// queue and no `SendState`. Answers `Action::EditDone` or `Action::EditFailed`, addressed by
    /// `room` and `id`.
    EditMessage {
        room: RoomId,
        id: String,
        text: SendText,
    },
    /// Delete message `id` of `room` with `POST chat.delete`. Answers `Action::DeleteDone` or
    /// `Action::DeleteFailed`, addressed by `room` and `id`.
    DeleteMessage { room: RoomId, id: String },
    /// Add (`add`) or take back the user's reaction `emoji` to message `id` with
    /// `POST chat.react`. Answers nothing on success (the server streams the reacted message back,
    /// which is what changes the row) and `Action::ReactFailed` on failure.
    React {
        id: String,
        emoji: EmojiName,
        add: bool,
    },
    /// Follow (`follow`) or unfollow thread `parent_id` of `room` with `POST chat.followMessage` or
    /// `chat.unfollowMessage`. Answers `Action::FollowDone` or `Action::FollowFailed`.
    FollowThread {
        room: RoomId,
        parent_id: String,
        follow: bool,
    },
    /// Fetch the server's custom emojis with `GET emoji-custom.list`. Answers
    /// `Action::CustomEmojisLoaded` or `Action::CustomEmojisFailed`.
    LoadCustomEmojis,
    /// Read the emoji box's state file (`emoji-recents` under the state directory). Answers
    /// `Action::RecentsLoaded`, with nothing in it when the file is missing or unusable. Needs no
    /// server.
    LoadRecents,
    /// Write the emoji box's state file. Answers nothing: a file that cannot be written leaves the
    /// picks of the session working in memory. Needs no server.
    SaveRecents(crate::app::emoji_recents::Recents),
    /// Get the image of one custom emoji: from the disk cache when it holds this `etag`, else with
    /// `GET <server>/emoji-custom/<name>.<ext>`, then decode it. Answers
    /// `Action::EmojiImageReady` or `Action::EmojiImageFailed`.
    FetchEmojiImage(EmojiRef),
    /// Mark `room` read with `POST subscriptions.read`. Answers `Action::MarkReadDone`; a failure
    /// answers nothing, the room just stays unread.
    MarkRead { room: RoomId },
    /// Mark thread `parent_id` of `room` read with `POST chat.readThread`. Answers
    /// `Action::MarkThreadReadDone`; a failure answers nothing, the thread just stays unread.
    MarkThreadRead { room: RoomId, parent_id: String },
}

/// Perform `effect` and return the Actions that report the outcome. `client` is `Err` when the
/// HTTP client could not be built, which fails every effect with that error.
pub async fn run(client: Result<Client, ApiError>, effect: Effect) -> Vec<Action> {
    // The state file needs no server, so these two run even when the client could not be built.
    match effect {
        Effect::LoadRecents => {
            let dir = state_dir_from_env();
            let recents = tokio::task::spawn_blocking(move || load_recents(dir.as_deref()))
                .await
                .unwrap_or_default();
            return vec![Action::RecentsLoaded(recents)];
        }
        Effect::SaveRecents(recents) => {
            let dir = state_dir_from_env();
            let _ =
                tokio::task::spawn_blocking(move || save_recents(dir.as_deref(), &recents)).await;
            return Vec::new();
        }
        _ => {}
    }
    let client = match client {
        Ok(client) => client,
        Err(error) => {
            return vec![match effect {
                Effect::LoadRooms => Action::LoadFailed(error),
                // LoadRooms already reports the client error; the username is best effort.
                Effect::LoadMe => return Vec::new(),
                // Best effort too: the room stays unread.
                Effect::MarkRead { .. } | Effect::MarkThreadRead { .. } => return Vec::new(),
                Effect::LoadHistory { room, before, .. } => Action::HistoryFailed {
                    room,
                    older: before.is_some(),
                    error,
                },
                Effect::LoadThread {
                    room,
                    parent_id,
                    offset,
                    ..
                } => Action::ThreadLoadFailed {
                    room,
                    parent_id,
                    offset,
                    error,
                },
                Effect::ListThreads {
                    room,
                    generation,
                    offset,
                    ..
                } => Action::ThreadListPage {
                    room,
                    generation,
                    offset,
                    result: Err(error),
                },
                Effect::FetchThreadParent { room, parent_id } => Action::ThreadListParent {
                    room,
                    parent_id,
                    result: Err(error),
                },
                Effect::SearchRoom {
                    room,
                    search_id,
                    offset,
                    ..
                } => Action::SearchPage {
                    room,
                    search_id,
                    offset,
                    result: Err(error),
                },
                Effect::LoadContext {
                    room, context_id, ..
                } => Action::ContextFailed {
                    room,
                    context_id,
                    error,
                },
                Effect::LoadNewer { room, newer_id, .. } => Action::NewerFailed {
                    room,
                    newer_id,
                    error,
                },
                Effect::SendMessage { room, id, .. } => Action::SendFailed { room, id, error },
                Effect::EditMessage { room, id, .. } => Action::EditFailed { room, id, error },
                Effect::DeleteMessage { room, id } => Action::DeleteFailed { room, id, error },
                Effect::React { .. } => Action::ReactFailed { error },
                Effect::FollowThread { follow, .. } => Action::FollowFailed { follow, error },
                Effect::LoadCustomEmojis => Action::CustomEmojisFailed,
                Effect::FetchEmojiImage(emoji) => Action::EmojiImageFailed(emoji),
                Effect::LoadRecents | Effect::SaveRecents(_) => return Vec::new(),
            }];
        }
    };
    match effect {
        Effect::LoadRooms => match client.load_rooms().await {
            Ok(list) => vec![
                Action::FoldersLoaded(list.folders),
                Action::RoomsLoaded(list.rooms),
            ],
            Err(error) => vec![Action::LoadFailed(error)],
        },
        Effect::LoadMe => match client.me_with_id().await {
            Ok(Some((name, id))) => std::iter::once(Action::MeLoaded(name))
                .chain(id.map(Action::OwnIdLoaded))
                .collect(),
            Ok(None) | Err(_) => Vec::new(),
        },
        Effect::LoadHistory {
            room,
            kind,
            private,
            count,
            before,
        } => {
            let older = before.is_some();
            match client
                .load_history(&room, kind, private, count, before.as_deref())
                .await
            {
                Ok(page) => vec![Action::HistoryLoaded {
                    room,
                    messages: page.messages,
                    oldest_ts: page.oldest_ts,
                    older,
                }],
                Err(error) => vec![Action::HistoryFailed { room, older, error }],
            }
        }
        Effect::LoadThread {
            room,
            parent_id,
            offset,
            count,
            fetch_parent,
        } => {
            let replies = client.load_thread(&room, &parent_id, offset, count);
            let parent = async {
                if fetch_parent {
                    client.get_message(&room, &parent_id).await.map(Some)
                } else {
                    Ok(None)
                }
            };
            match tokio::join!(replies, parent) {
                (Ok(page), Ok(parent)) => vec![Action::ThreadLoaded {
                    room,
                    parent_id,
                    offset,
                    parent,
                    page,
                }],
                (Err(error), _) | (_, Err(error)) => vec![Action::ThreadLoadFailed {
                    room,
                    parent_id,
                    offset,
                    error,
                }],
            }
        }
        Effect::ListThreads {
            room,
            filter,
            query,
            generation,
            offset,
            count,
        } => {
            let result = client
                .list_threads(&room, filter, query.as_str(), offset, count)
                .await;
            vec![Action::ThreadListPage {
                room,
                generation,
                offset,
                result,
            }]
        }
        Effect::FetchThreadParent { room, parent_id } => {
            let result = client.get_message(&room, &parent_id).await;
            vec![Action::ThreadListParent {
                room,
                parent_id,
                result,
            }]
        }
        Effect::SearchRoom {
            room,
            query,
            search_id,
            offset,
            count,
        } => {
            let result = client
                .search_room(&room, query.as_str(), offset, count)
                .await;
            vec![Action::SearchPage {
                room,
                search_id,
                offset,
                result,
            }]
        }
        Effect::LoadContext {
            room,
            kind,
            private,
            id,
            raw_ts,
            context_id,
            before,
            after,
        } => {
            match client
                .load_context(&room, kind, private, &id, &raw_ts, before, after)
                .await
            {
                Ok(window) => vec![Action::ContextLoaded {
                    room,
                    context_id,
                    window,
                }],
                Err(error) => vec![Action::ContextFailed {
                    room,
                    context_id,
                    error,
                }],
            }
        }
        Effect::LoadNewer {
            room,
            kind,
            private,
            raw_ts,
            n,
            newer_id,
        } => match client.load_newer(&room, kind, private, &raw_ts, n).await {
            Ok(page) => vec![Action::NewerLoaded {
                room,
                newer_id,
                page,
            }],
            Err(error) => vec![Action::NewerFailed {
                room,
                newer_id,
                error,
            }],
        },
        Effect::SendMessage {
            room,
            id,
            text,
            reconcile,
            tmid,
            tshow,
        } => {
            // A retry of an Unconfirmed message first asks the server whether it already holds the
            // message (D-15): a lost response must never become a duplicate in the room. Any
            // answer but a 200 means "not stored", and the POST follows.
            if reconcile && let Ok(message) = client.get_message(&room, &id).await {
                return vec![Action::MessageSent { room, id, message }];
            }
            let thread = tmid.as_deref().map(|parent_id| crate::api::ThreadTarget {
                parent_id,
                also_in_channel: tshow,
            });
            match client.send_message(&room, &id, text.as_str(), thread).await {
                Ok(message) => vec![Action::MessageSent { room, id, message }],
                Err(error) => {
                    // The attempt may have landed meanwhile, or a duplicate `_id` was answered
                    // with an error (probe P2): ask once more, and a 200 still counts as sent.
                    if reconcile && let Ok(message) = client.get_message(&room, &id).await {
                        return vec![Action::MessageSent { room, id, message }];
                    }
                    vec![Action::SendFailed { room, id, error }]
                }
            }
        }
        Effect::EditMessage { room, id, text } => {
            match client.edit_message(&room, &id, text.as_str()).await {
                Ok(message) => vec![Action::EditDone { room, id, message }],
                Err(error) => vec![Action::EditFailed { room, id, error }],
            }
        }
        Effect::DeleteMessage { room, id } => match client.delete_message(&room, &id).await {
            Ok(()) => vec![Action::DeleteDone { room, id }],
            Err(error) => vec![Action::DeleteFailed { room, id, error }],
        },
        Effect::React { id, emoji, add } => match client.react(&id, emoji.as_str(), add).await {
            Ok(()) => Vec::new(),
            Err(error) => vec![Action::ReactFailed { error }],
        },
        Effect::FollowThread {
            room,
            parent_id,
            follow,
        } => {
            let done = if follow {
                client.follow_thread(&parent_id).await
            } else {
                client.unfollow_thread(&parent_id).await
            };
            match done {
                Ok(()) => vec![Action::FollowDone {
                    room,
                    parent_id,
                    follow,
                }],
                Err(error) => vec![Action::FollowFailed { follow, error }],
            }
        }
        Effect::MarkRead { room } => match client.mark_read(&room).await {
            Ok(()) => vec![Action::MarkReadDone { room }],
            Err(_) => Vec::new(),
        },
        Effect::MarkThreadRead { room, parent_id } => {
            match client.mark_thread_read(&parent_id).await {
                Ok(()) => vec![Action::MarkThreadReadDone { room, parent_id }],
                Err(_) => Vec::new(),
            }
        }
        Effect::LoadCustomEmojis => match client.custom_emojis().await {
            Ok(list) => vec![Action::CustomEmojisLoaded(list)],
            Err(_) => vec![Action::CustomEmojisFailed],
        },
        Effect::FetchEmojiImage(emoji) => {
            vec![fetch_emoji_image(&client, cache_dir_from_env().as_deref(), emoji).await]
        }
        // Handled before the client was looked at.
        Effect::LoadRecents | Effect::SaveRecents(_) => Vec::new(),
    }
}

/// The state directory from the environment, `None` when there is no usable one.
fn state_dir_from_env() -> Option<std::path::PathBuf> {
    crate::config::paths::state_dir(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

/// The file the emoji box's state lives in.
const RECENTS_FILE: &str = "emoji-recents";

/// Read the state file in `dir`. A missing, unreadable, oversized, not-UTF-8 or malformed file is
/// an empty state, never an error, and nothing from it is logged. A file that is not a regular
/// file is never opened.
pub fn load_recents(dir: Option<&std::path::Path>) -> crate::app::emoji_recents::Recents {
    use crate::app::emoji_recents::{MAX_FILE_BYTES, Recents};
    use std::io::Read;
    let Some(path) = dir.map(|d| d.join(RECENTS_FILE)) else {
        return Recents::default();
    };
    let Ok(meta) = std::fs::metadata(&path) else {
        return Recents::default();
    };
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return Recents::default();
    }
    let mut text = String::new();
    // `take` as well: the file may have grown since the metadata was read.
    let read = std::fs::File::open(&path)
        .and_then(|file| file.take(MAX_FILE_BYTES + 1).read_to_string(&mut text));
    if read.is_err() || text.len() as u64 > MAX_FILE_BYTES {
        return Recents::default();
    }
    Recents::parse(&text)
}

/// Write the state file in `dir`: to a temporary file readable by its owner only, then renamed over
/// the real one, so a crash leaves the old file or the new one. Returns whether it was written; the
/// caller ignores a failure.
pub fn save_recents(
    dir: Option<&std::path::Path>,
    recents: &crate::app::emoji_recents::Recents,
) -> bool {
    use std::io::Write;
    let Some(dir) = dir else {
        return false;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let target = dir.join(RECENTS_FILE);
    let temp = dir.join(format!(".{RECENTS_FILE}.{}.tmp", std::process::id()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let written = options
        .open(&temp)
        .and_then(|mut file| file.write_all(recents.format().as_bytes()));
    if written.is_err() || std::fs::rename(&temp, &target).is_err() {
        let _ = std::fs::remove_file(&temp);
        return false;
    }
    true
}

/// The emoji cache directory from the environment, `None` when there is no usable one.
fn cache_dir_from_env() -> Option<std::path::PathBuf> {
    crate::emoji_image::cache_dir(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME"))
}

/// Get and decode the image `emoji` names: from `cache` when it holds this etag, else from the
/// server (and then into the cache, if it can be written). A failure of any step, including a
/// cached file that no longer decodes, is `EmojiImageFailed`; the cause is not kept.
pub async fn fetch_emoji_image(
    client: &Client,
    cache: Option<&std::path::Path>,
    emoji: EmojiRef,
) -> Action {
    use crate::api::MAX_EMOJI_BYTES;
    use crate::emoji_image::{decode, read_cached, write_cached};
    let cached = cache
        .and_then(|dir| read_cached(dir, &emoji.name, &emoji.etag, &emoji.ext, MAX_EMOJI_BYTES));
    let (bytes, from_cache) = match cached {
        Some(bytes) => (bytes, true),
        None => match client
            .emoji_image(&emoji.name, &emoji.ext, &emoji.etag)
            .await
        {
            Ok(bytes) => (bytes, false),
            Err(_) => return Action::EmojiImageFailed(emoji),
        },
    };
    // Decoding is CPU work on bytes from the server: off the async threads.
    let decoded = tokio::task::spawn_blocking(move || decode(&bytes).map(|image| (image, bytes)))
        .await
        .ok()
        .and_then(Result::ok);
    match decoded {
        Some((image, bytes)) => {
            if !from_cache && let Some(dir) = cache {
                // Only bytes that decoded are kept. A cache that cannot be written is not an error.
                write_cached(dir, &emoji.name, &emoji.etag, &emoji.ext, &bytes);
            }
            Action::EmojiImageReady { emoji, image }
        }
        None => Action::EmojiImageFailed(emoji),
    }
}

/// Run `effect` on its own task and send each resulting Action on `tx`. The only place an effect
/// starts IO.
pub fn spawn(client: Result<Client, ApiError>, tx: Sender<Action>, effect: Effect) {
    tokio::spawn(async move {
        for action in run(client, effect).await {
            if tx.send(action).await.is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::connection::{ConnEnv, EnvValue, Secret};
    use crate::config::{Sources, load_from_sources};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const HISTORY: &str = include_str!("api/fixtures/history.json");

    fn client_for(server: &MockServer) -> Client {
        let env = |v: &str| EnvValue::Set(Secret::new(v.to_string()));
        let sources = Sources {
            env: ConnEnv {
                server_url: env(&server.uri()),
                user_id: env("u1"),
                token: env("SENTINEL-TOKEN-9c41"),
            },
            require_connection: true,
            ..Sources::default()
        };
        let config = load_from_sources(&sources)
            .unwrap_or_else(|report| panic!("unexpected config errors: {report}"));
        Client::new(&config.connection.expect("connection")).expect("client")
    }

    fn recents_dir(tag: &str) -> std::path::PathBuf {
        crate::test_util::temp_config_dir(tag, &[])
    }

    #[test]
    fn recents_survive_a_save_and_a_load() {
        use crate::app::emoji_recents::Recents;
        let dir = recents_dir("recents-roundtrip");
        let mut recents = Recents::default();
        recents.record("tada");
        recents.record("tada");
        recents.tone = 2;
        // The directory is created when it is missing.
        let inner = dir.join("state").join("rockettui");
        assert!(save_recents(Some(&inner), &recents));
        assert_eq!(load_recents(Some(&inner)), recents);
        // No temporary file is left behind.
        let names: Vec<_> = std::fs::read_dir(&inner)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, [std::ffi::OsString::from("emoji-recents")]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_state_file_is_readable_by_its_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = recents_dir("recents-mode");
        assert!(save_recents(Some(&dir), &Default::default()));
        let mode = std::fs::metadata(dir.join("emoji-recents"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_oversized_or_odd_state_file_reads_as_empty() {
        use crate::app::emoji_recents::{MAX_FILE_BYTES, Recents};
        let dir = recents_dir("recents-bad");
        // Missing, and no directory at all.
        assert_eq!(load_recents(Some(&dir)), Recents::default());
        assert_eq!(load_recents(None), Recents::default());
        // Larger than 64 KiB: not read, even though every line is good.
        let big: String = (0..=(MAX_FILE_BYTES / 8))
            .map(|n| format!("e{n} 1 1\n"))
            .collect();
        assert!(big.len() as u64 > MAX_FILE_BYTES);
        std::fs::write(dir.join("emoji-recents"), big).unwrap();
        assert_eq!(load_recents(Some(&dir)), Recents::default());
        // Not UTF-8.
        std::fs::write(dir.join("emoji-recents"), [0xff, 0xfe, 0x00]).unwrap();
        assert_eq!(load_recents(Some(&dir)), Recents::default());
        // A directory where the file should be.
        std::fs::remove_file(dir.join("emoji-recents")).unwrap();
        std::fs::create_dir(dir.join("emoji-recents")).unwrap();
        assert_eq!(load_recents(Some(&dir)), Recents::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn a_state_directory_that_cannot_be_written_is_not_an_error() {
        use std::os::unix::fs::PermissionsExt;
        let dir = recents_dir("recents-readonly");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let inner = dir.join("rockettui");
        // Running as root ignores the mode, so only the answer of a refusing directory is checked.
        let saved = save_recents(Some(&inner), &Default::default());
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        if !saved {
            assert!(!inner.join("emoji-recents").exists());
        }
        assert!(!save_recents(None, &Default::default()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn load_history_effect() -> Effect {
        Effect::LoadHistory {
            room: RoomId("r-devteam".to_string()),
            kind: RoomKind::PrivateGroup,
            private: true,
            count: 50,
            before: None,
        }
    }

    #[tokio::test]
    async fn load_history_actions() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/groups.history"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(HISTORY.to_string(), "application/json"),
            )
            .mount(&server)
            .await;
        let actions = run(Ok(client_for(&server)), load_history_effect()).await;
        assert_eq!(actions.len(), 1, "{actions:?}");
        match &actions[0] {
            Action::HistoryLoaded {
                room,
                messages,
                oldest_ts,
                older,
            } => {
                assert_eq!(room.0, "r-devteam");
                assert_eq!(messages.len(), 4);
                assert_eq!(oldest_ts.as_deref(), Some("2026-11-16T09:59:00.000Z"));
                assert!(!older);
            }
            other => panic!("expected HistoryLoaded, got {other:?}"),
        }

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(401).set_body_string("{\"success\":false}"))
            .mount(&server)
            .await;
        let actions = run(Ok(client_for(&server)), load_history_effect()).await;
        assert_eq!(
            actions,
            [Action::HistoryFailed {
                room: RoomId("r-devteam".to_string()),
                older: false,
                error: ApiError::Unauthorized,
            }]
        );
    }

    #[tokio::test]
    async fn load_me_actions() {
        async fn serve(status: u16, body: &str) -> MockServer {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/v1/me"))
                .respond_with(
                    ResponseTemplate::new(status)
                        .set_body_raw(body.to_string(), "application/json"),
                )
                .mount(&server)
                .await;
            server
        }
        let server = serve(
            200,
            "{\"_id\":\"u1\",\"username\":\"smoke\",\"success\":true}",
        )
        .await;
        let actions = run(Ok(client_for(&server)), Effect::LoadMe).await;
        assert_eq!(
            actions,
            [
                Action::MeLoaded("smoke".to_string()),
                Action::OwnIdLoaded("u1".to_string())
            ]
        );

        let server = serve(401, "{\"success\":false}").await;
        assert_eq!(run(Ok(client_for(&server)), Effect::LoadMe).await, []);

        let server = serve(200, "{\"username\":\"\"}").await;
        assert_eq!(run(Ok(client_for(&server)), Effect::LoadMe).await, []);

        assert_eq!(run(Err(ApiError::Client), Effect::LoadMe).await, []);
    }

    #[tokio::test]
    async fn load_history_without_client_fails_closed() {
        let actions = run(Err(ApiError::Client), load_history_effect()).await;
        assert_eq!(
            actions,
            [Action::HistoryFailed {
                room: RoomId("r-devteam".to_string()),
                older: false,
                error: ApiError::Client,
            }]
        );
    }

    const SENTINEL: &str = "SENTINEL-BODY-7f3a";

    fn general() -> RoomId {
        RoomId("general".to_string())
    }

    fn send_effect() -> Effect {
        Effect::SendMessage {
            room: general(),
            id: "rt-test-1".to_string(),
            text: SendText::new(SENTINEL.to_string()),
            reconcile: false,
            tmid: None,
            tshow: false,
        }
    }

    /// The server's answer to a send of `text` under `id`: the stored message.
    fn answer(id: &str, text: &str) -> String {
        serde_json::json!({
            "message": {
                "_id": id, "rid": "general", "msg": text,
                "ts": "2026-11-17T10:00:00.123Z",
                "u": {"_id": "u1", "username": "smoke"}
            },
            "success": true
        })
        .to_string()
    }

    async fn mount_send_ok(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.sendMessage"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(answer("rt-test-1", SENTINEL), "application/json"),
            )
            .mount(server)
            .await;
    }

    async fn mount_send_status(server: &MockServer, status: u16) {
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(status).set_body_string("{\"success\":false}"))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn send_message_actions() {
        let server = MockServer::start().await;
        mount_send_ok(&server).await;
        let actions = run(Ok(client_for(&server)), send_effect()).await;
        assert_eq!(actions.len(), 1, "{actions:?}");
        match &actions[0] {
            Action::MessageSent { room, id, message } => {
                assert_eq!(*room, general());
                assert_eq!(id, "rt-test-1");
                assert_eq!(message.id, "rt-test-1");
                assert_eq!(message.author, "smoke");
                assert_eq!(message.body, SENTINEL);
            }
            other => panic!("expected MessageSent, got {other:?}"),
        }

        let server = MockServer::start().await;
        mount_send_status(&server, 401).await;
        let actions = run(Ok(client_for(&server)), send_effect()).await;
        assert_eq!(
            actions,
            [Action::SendFailed {
                room: general(),
                id: "rt-test-1".to_string(),
                error: ApiError::Unauthorized,
            }]
        );
    }

    #[tokio::test]
    async fn send_without_client_fails_closed() {
        let actions = run(Err(ApiError::Client), send_effect()).await;
        assert_eq!(
            actions,
            [Action::SendFailed {
                room: general(),
                id: "rt-test-1".to_string(),
                error: ApiError::Client,
            }]
        );
    }

    fn reconcile_effect(reconcile: bool) -> Effect {
        Effect::SendMessage {
            room: general(),
            id: "rt-test-1".to_string(),
            text: SendText::new(SENTINEL.to_string()),
            reconcile,
            tmid: None,
            tshow: false,
        }
    }

    /// Mount `GET chat.getMessage` answering `status`; a 200 carries the stored message. `times`
    /// limits how many requests the mock answers, so a later mock can answer the next ones.
    async fn mount_get_message(server: &MockServer, status: u16, times: Option<u64>) {
        let template = if status == 200 {
            ResponseTemplate::new(200)
                .set_body_raw(answer("rt-test-1", SENTINEL), "application/json")
        } else {
            ResponseTemplate::new(status).set_body_string("{\"success\":false}")
        };
        let mock = Mock::given(method("GET"))
            .and(path("/api/v1/chat.getMessage"))
            .respond_with(template);
        match times {
            Some(n) => mock.up_to_n_times(n).mount(server).await,
            None => mock.mount(server).await,
        }
    }

    /// The (method, path) of every request the server received, in order.
    async fn seen(server: &MockServer) -> Vec<(String, String)> {
        server
            .received_requests()
            .await
            .expect("recorded")
            .iter()
            .map(|r| (r.method.to_string(), r.url.path().to_string()))
            .collect()
    }

    fn get_message_call() -> (String, String) {
        ("GET".to_string(), "/api/v1/chat.getMessage".to_string())
    }

    fn post_call() -> (String, String) {
        ("POST".to_string(), "/api/v1/chat.sendMessage".to_string())
    }

    fn is_sent(actions: &[Action]) -> bool {
        matches!(actions, [Action::MessageSent { .. }])
    }

    #[tokio::test]
    async fn reconcile_found_skips_post() {
        let server = MockServer::start().await;
        mount_get_message(&server, 200, None).await;
        mount_send_ok(&server).await;
        let actions = run(Ok(client_for(&server)), reconcile_effect(true)).await;
        assert!(is_sent(&actions), "{actions:?}");
        if let [Action::MessageSent { id, message, .. }] = &actions[..] {
            assert_eq!(id, "rt-test-1");
            assert_eq!(message.author, "smoke");
        }
        assert_eq!(seen(&server).await, [get_message_call()], "no POST");
    }

    #[tokio::test]
    async fn reconcile_absent_posts() {
        let server = MockServer::start().await;
        mount_get_message(&server, 400, None).await;
        mount_send_ok(&server).await;
        let actions = run(Ok(client_for(&server)), reconcile_effect(true)).await;
        assert!(is_sent(&actions), "{actions:?}");
        assert_eq!(seen(&server).await, [get_message_call(), post_call()]);
    }

    #[tokio::test]
    async fn reconcile_post_error_rechecks() {
        // The first check says absent, the POST fails, the second check finds the message: sent.
        let server = MockServer::start().await;
        mount_get_message(&server, 400, Some(1)).await;
        mount_get_message(&server, 200, None).await;
        mount_send_status(&server, 400).await;
        let actions = run(Ok(client_for(&server)), reconcile_effect(true)).await;
        assert!(is_sent(&actions), "{actions:?}");
        assert_eq!(
            seen(&server).await,
            [get_message_call(), post_call(), get_message_call()]
        );

        // Both checks say absent: the POST's own error is reported.
        let server = MockServer::start().await;
        mount_get_message(&server, 400, None).await;
        mount_send_status(&server, 400).await;
        let actions = run(Ok(client_for(&server)), reconcile_effect(true)).await;
        assert_eq!(
            actions,
            [Action::SendFailed {
                room: general(),
                id: "rt-test-1".to_string(),
                error: ApiError::Rejected,
            }]
        );
        assert_eq!(
            seen(&server).await,
            [get_message_call(), post_call(), get_message_call()]
        );
    }

    #[tokio::test]
    async fn no_reconcile_never_gets() {
        let server = MockServer::start().await;
        mount_get_message(&server, 200, None).await;
        mount_send_ok(&server).await;
        let actions = run(Ok(client_for(&server)), reconcile_effect(false)).await;
        assert!(is_sent(&actions), "{actions:?}");
        assert_eq!(seen(&server).await, [post_call()]);

        // A failing POST of a plain send is reported without any check.
        let server = MockServer::start().await;
        mount_get_message(&server, 200, None).await;
        mount_send_status(&server, 500).await;
        let actions = run(Ok(client_for(&server)), reconcile_effect(false)).await;
        assert_eq!(
            actions,
            [Action::SendFailed {
                room: general(),
                id: "rt-test-1".to_string(),
                error: ApiError::Server(500),
            }]
        );
        assert_eq!(seen(&server).await, [post_call()]);
    }

    fn edit_effect() -> Effect {
        Effect::EditMessage {
            room: general(),
            id: "m1".to_string(),
            text: SendText::new(SENTINEL.to_string()),
        }
    }

    fn delete_effect() -> Effect {
        Effect::DeleteMessage {
            room: general(),
            id: "m1".to_string(),
        }
    }

    #[tokio::test]
    async fn mark_read_answers_done_on_success_and_nothing_on_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/subscriptions.read"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw("{\"success\":true}", "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let effect = Effect::MarkRead { room: general() };
        let actions = run(Ok(client_for(&server)), effect.clone()).await;
        assert_eq!(actions, [Action::MarkReadDone { room: general() }]);

        let failing = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&failing)
            .await;
        assert!(
            run(Ok(client_for(&failing)), effect.clone())
                .await
                .is_empty()
        );
        // No client at all fails silently too.
        assert!(run(Err(ApiError::Unauthorized), effect).await.is_empty());
    }

    #[tokio::test]
    async fn mark_thread_read_answers_done_on_success_and_nothing_on_failure() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.readThread"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw("{\"success\":true}", "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let effect = Effect::MarkThreadRead {
            room: general(),
            parent_id: "p1".to_string(),
        };
        let actions = run(Ok(client_for(&server)), effect.clone()).await;
        assert_eq!(
            actions,
            [Action::MarkThreadReadDone {
                room: general(),
                parent_id: "p1".to_string()
            }]
        );

        let failing = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&failing)
            .await;
        assert!(
            run(Ok(client_for(&failing)), effect.clone())
                .await
                .is_empty()
        );
        assert!(run(Err(ApiError::Unauthorized), effect).await.is_empty());
    }

    #[tokio::test]
    async fn edit_message_actions() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.update"))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(answer("m1", SENTINEL), "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let actions = run(Ok(client_for(&server)), edit_effect()).await;
        assert_eq!(actions.len(), 1, "{actions:?}");
        match &actions[0] {
            Action::EditDone { room, id, message } => {
                assert_eq!(*room, general());
                assert_eq!(id, "m1");
                assert_eq!(message.body, SENTINEL);
            }
            other => panic!("expected EditDone, got {other:?}"),
        }

        let server = MockServer::start().await;
        mount_send_status(&server, 400).await;
        assert_eq!(
            run(Ok(client_for(&server)), edit_effect()).await,
            [Action::EditFailed {
                room: general(),
                id: "m1".to_string(),
                error: ApiError::Rejected,
            }]
        );
        assert_eq!(
            run(Err(ApiError::Client), edit_effect()).await,
            [Action::EditFailed {
                room: general(),
                id: "m1".to_string(),
                error: ApiError::Client,
            }]
        );
        // Not a send: an edit never asks `chat.getMessage` and never posts a message.
        let requests = server.received_requests().await.expect("recorded");
        assert!(
            requests
                .iter()
                .all(|r| r.url.path() == "/api/v1/chat.update"),
            "{requests:?}"
        );
    }

    fn react_effect(add: bool) -> Effect {
        Effect::React {
            id: "m1".to_string(),
            emoji: EmojiName::new("tada".to_string()),
            add,
        }
    }

    #[tokio::test]
    async fn react_sends_the_request_and_answers_nothing_on_success() {
        for add in [true, false] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/v1/chat.react"))
                .and(wiremock::matchers::body_json(serde_json::json!({
                    "messageId": "m1",
                    "emoji": ":tada:",
                    "shouldReact": add,
                })))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_raw("{\"success\":true}", "application/json"),
                )
                .expect(1)
                .mount(&server)
                .await;
            // The streamed message is what changes the row, so success reports no action.
            let actions = run(Ok(client_for(&server)), react_effect(add)).await;
            assert!(actions.is_empty(), "{actions:?}");
        }
    }

    #[tokio::test]
    async fn react_failures_become_one_action() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.react"))
            .respond_with(ResponseTemplate::new(400).set_body_raw(
                "{\"success\":false,\"error\":\"not valid\"}",
                "application/json",
            ))
            .mount(&server)
            .await;
        assert_eq!(
            run(Ok(client_for(&server)), react_effect(true)).await,
            [Action::ReactFailed {
                error: ApiError::Rejected
            }]
        );
        assert_eq!(
            run(Err(ApiError::Client), react_effect(true)).await,
            [Action::ReactFailed {
                error: ApiError::Client
            }]
        );
    }

    #[tokio::test]
    async fn custom_emojis_effect_actions() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/emoji-custom.list"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                include_str!("api/fixtures/docker/emoji_custom_list.json"),
                "application/json",
            ))
            .expect(1)
            .mount(&server)
            .await;
        let actions = run(Ok(client_for(&server)), Effect::LoadCustomEmojis).await;
        match actions.as_slice() {
            [Action::CustomEmojisLoaded(list)] => {
                let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
                assert_eq!(names, ["rtshipit", "rtplain"]);
            }
            other => panic!("expected CustomEmojisLoaded, got {other:?}"),
        }

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/emoji-custom.list"))
            .respond_with(ResponseTemplate::new(503).set_body_raw("{}", "application/json"))
            .mount(&server)
            .await;
        assert_eq!(
            run(Ok(client_for(&server)), Effect::LoadCustomEmojis).await,
            [Action::CustomEmojisFailed]
        );
        assert_eq!(
            run(Err(ApiError::Client), Effect::LoadCustomEmojis).await,
            [Action::CustomEmojisFailed]
        );
    }

    #[tokio::test]
    async fn delete_message_actions() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/chat.delete"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw("{\"_id\":\"m1\",\"success\":true}", "application/json"),
            )
            .expect(1)
            .mount(&server)
            .await;
        assert_eq!(
            run(Ok(client_for(&server)), delete_effect()).await,
            [Action::DeleteDone {
                room: general(),
                id: "m1".to_string(),
            }]
        );
        let server = MockServer::start().await;
        mount_send_status(&server, 400).await;
        assert_eq!(
            run(Ok(client_for(&server)), delete_effect()).await,
            [Action::DeleteFailed {
                room: general(),
                id: "m1".to_string(),
                error: ApiError::Rejected,
            }]
        );
        assert_eq!(
            run(Err(ApiError::Client), delete_effect()).await,
            [Action::DeleteFailed {
                room: general(),
                id: "m1".to_string(),
                error: ApiError::Client,
            }]
        );
    }

    #[test]
    fn edit_effect_debug_redacts() {
        let shown = format!("{:?}", edit_effect());
        assert!(!shown.contains(SENTINEL), "{shown}");
        assert!(shown.contains("m1"), "{shown}");
    }

    #[test]
    fn send_text_debug_redacts() {
        let text = SendText::new(SENTINEL.to_string());
        assert_eq!(text.as_str(), SENTINEL);
        let shown = format!("{text:?}");
        assert_eq!(shown, format!("SendText(<{}> bytes)", SENTINEL.len()));
        assert!(!shown.contains(SENTINEL));
        let effect = format!("{:?}", send_effect());
        assert!(!effect.contains(SENTINEL), "{effect}");
        assert!(effect.contains("rt-test-1"), "{effect}");
    }

    #[tokio::test]
    async fn send_round_trip_swaps_in_place() {
        use crate::app::update::update;
        use crate::data::{Message, MessageKind, SendState};
        use crate::test_util::app_80x24;

        let pending = Message {
            edited: false,
            plain_text: true,
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
            refs: None,
            reactions: Vec::new(),
            id: "rt-test-1".to_string(),
            room_id: general(),
            author: "me".to_string(),
            ts: 0,
            body: SENTINEL.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sending,
        };
        let seeded = || {
            let mut app = app_80x24();
            app.open_room(general());
            app.messages
                .get_mut(&general())
                .expect("general has messages")
                .push(pending.clone());
            app
        };

        // 200: the server's copy replaces the pending message in place; no history reload.
        let server = MockServer::start().await;
        mount_send_ok(&server).await;
        let mut app = seeded();
        let len = app.messages[&general()].len();
        let k = len - 1;
        for action in run(Ok(client_for(&server)), send_effect()).await {
            update(&mut app, action);
        }
        assert_eq!(app.messages[&general()].len(), len);
        let swapped = &app.messages[&general()][k];
        assert_eq!(swapped.id, "rt-test-1");
        assert_eq!(swapped.author, "smoke");
        assert_eq!(swapped.ts, 1_794_909_600);
        assert!(
            !app.effects
                .iter()
                .any(|e| matches!(e, Effect::LoadHistory { .. })),
            "{:?}",
            app.effects
        );
        let requests = server.received_requests().await.expect("recorded");
        assert_eq!(requests.len(), 1, "one send is one request");

        // 401: the message stays in place, now marked Failed, and the status bar shows the fixed
        // reason.
        let server = MockServer::start().await;
        mount_send_status(&server, 401).await;
        let mut app = seeded();
        let mut before = app.messages[&general()].clone();
        for action in run(Ok(client_for(&server)), send_effect()).await {
            update(&mut app, action);
        }
        before[k].send = SendState::Failed;
        assert_eq!(app.messages[&general()], before);
        assert_eq!(app.messages[&general()][k].author, "me");
        let Some(crate::app::StatusMsg::Error(text)) = &app.status_msg else {
            panic!("no error status");
        };
        assert!(text.starts_with("message not sent: "), "{text}");
        assert!(!text.contains(SENTINEL), "{text}");
    }

    // ---- custom emoji images

    fn png_bytes() -> Vec<u8> {
        use image::ImageEncoder;
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(&[9, 8, 7, 255], 1, 1, image::ExtendedColorType::Rgba8)
            .unwrap();
        out
    }

    fn emoji_ref() -> EmojiRef {
        EmojiRef {
            name: "rtshipit".to_string(),
            etag: "46961c".to_string(),
            ext: "png".to_string(),
        }
    }

    fn cache_tmp(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rockettui-test-fx-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    async fn serve_image(server: &MockServer, bytes: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path("/emoji-custom/rtshipit.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn an_image_is_fetched_decoded_and_then_cached() {
        let server = MockServer::start().await;
        serve_image(&server, png_bytes()).await;
        let dir = cache_tmp("fetch");
        let action = fetch_emoji_image(&client_for(&server), Some(&dir), emoji_ref()).await;
        match action {
            Action::EmojiImageReady { emoji, image } => {
                assert_eq!(emoji, emoji_ref());
                assert_eq!((image.width, image.height), (1, 1));
                assert_eq!(&image.pixels[..], &[9, 8, 7, 255]);
            }
            other => panic!("expected the image, got {other:?}"),
        }
        // The next time (a later session) it is read from disk and the server is not asked.
        let second = MockServer::start().await;
        let action = fetch_emoji_image(&client_for(&second), Some(&dir), emoji_ref()).await;
        assert!(
            matches!(action, Action::EmojiImageReady { .. }),
            "{action:?}"
        );
        assert!(second.received_requests().await.unwrap().is_empty());
        // A changed etag is not a hit: the server is asked.
        serve_image(&second, png_bytes()).await;
        let mut newer = emoji_ref();
        newer.etag = "other".to_string();
        let action = fetch_emoji_image(&client_for(&second), Some(&dir), newer).await;
        assert!(matches!(action, Action::EmojiImageReady { .. }));
        assert_eq!(second.received_requests().await.unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn without_a_usable_cache_the_image_still_loads() {
        let server = MockServer::start().await;
        serve_image(&server, png_bytes()).await;
        let action = fetch_emoji_image(&client_for(&server), None, emoji_ref()).await;
        assert!(matches!(action, Action::EmojiImageReady { .. }));
        // A cache directory that cannot be written.
        let dir = cache_tmp("blocked");
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("emoji");
        std::fs::write(&blocker, b"x").unwrap();
        let action = fetch_emoji_image(&client_for(&server), Some(&blocker), emoji_ref()).await;
        assert!(matches!(action, Action::EmojiImageReady { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_failing_image_reports_the_failure_and_caches_nothing() {
        let dir = cache_tmp("fail");
        // Not an image: nothing is cached.
        let server = MockServer::start().await;
        serve_image(&server, b"<html>not an image</html>".to_vec()).await;
        let action = fetch_emoji_image(&client_for(&server), Some(&dir), emoji_ref()).await;
        assert_eq!(action, Action::EmojiImageFailed(emoji_ref()));
        assert!(!dir.exists() || std::fs::read_dir(&dir).unwrap().next().is_none());
        // Not found.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        assert_eq!(
            fetch_emoji_image(&client_for(&server), Some(&dir), emoji_ref()).await,
            Action::EmojiImageFailed(emoji_ref())
        );
        // The effect with no client at all.
        assert_eq!(
            run(Err(ApiError::Client), Effect::FetchEmojiImage(emoji_ref())).await,
            [Action::EmojiImageFailed(emoji_ref())]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}

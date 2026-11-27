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

/// Which attachment image to fetch: the server path of the file (`/file-upload/...`), the size the
/// server reports for it and the pixel size it reports. All are server text or numbers about a
/// user's file, so its `Debug` prints only the length of the path.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct AttachmentRef {
    pub path: String,
    pub size: Option<u64>,
    pub dims: Option<(u32, u32)>,
    /// Whether to keep the frames of an animated image (the `animate_images` setting).
    pub animate: bool,
    /// A path to fetch instead of `path`, for an animated image whose `path` is a still thumbnail
    /// the server made: the original file. `path` stays the key the answer is filed under.
    pub source: Option<String>,
}

impl std::fmt::Debug for AttachmentRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AttachmentRef(<{}> bytes)", self.path.len())
    }
}

/// Which attachment to download to be opened: the server path of the file (`/file-upload/...`),
/// its media type and the size the server reports. All are server text or numbers about a user's
/// file, so its `Debug` prints only lengths.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DownloadRef {
    pub path: String,
    pub mime: String,
    pub size: Option<u64>,
}

impl std::fmt::Debug for DownloadRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DownloadRef(<{}> bytes, <{}> bytes)",
            self.path.len(),
            self.mime.len()
        )
    }
}

/// Which video to make a poster of: the server path of the file (`/file-upload/...`), its media
/// type, the size the server reports and the `ffmpeg` program to use. The first three are server
/// text or numbers about a user's file, so its `Debug` prints only lengths.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct VideoRef {
    pub path: String,
    pub mime: String,
    pub size: Option<u64>,
    pub ffmpeg: String,
}

impl std::fmt::Debug for VideoRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "VideoRef(<{}> bytes, <{}> bytes)",
            self.path.len(),
            self.mime.len()
        )
    }
}

/// What starts one playback run of a video: which video, how to decode it and where the run
/// begins. Server text and numbers about a user's file, so its `Debug` prints only lengths.
#[derive(Clone, PartialEq, Eq)]
pub struct VideoStart {
    pub video: VideoRef,
    /// The `ffprobe` program, run only when `probe` is set.
    pub ffprobe: String,
    /// Seconds into the video the run starts at.
    pub start_secs: u64,
    /// The frame size, from the poster (or its fullscreen scaling).
    pub size: (u32, u32),
    /// The most a side of the frame may be: `MAX_SIDE`, or `FULLSCREEN_SIDE` for fullscreen.
    pub max_side: u32,
    /// The run's number; every action of the run carries it.
    pub generation: u64,
    /// Start paused: the first frame is shown and the run holds.
    pub paused: bool,
    /// Read the length of the video (the first run of a player; a seek already knows it).
    pub probe: bool,
}

impl std::fmt::Debug for VideoStart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "VideoStart({:?}, generation {}, start {}, {}x{})",
            self.video, self.generation, self.start_secs, self.size.0, self.size.1
        )
    }
}

/// Why the external program did not run: it could not be started, or it exited with an error right
/// away. Nothing of the program's output or arguments is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenError {
    CannotStart,
    ExitedWithError,
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
    /// Get one attachment image: from the disk cache when it holds this path and size, else with
    /// `GET <server><path>` (a `/file-upload/` path only), then decode it for its inline box.
    /// Answers `Action::AttachmentImageReady` or `Action::AttachmentImageFailed`.
    FetchAttachmentImage(AttachmentRef),
    /// Read one attachment image from the disk cache and decode it for the full-size view (up to
    /// `VIEW_SHRINK_TO` pixels). Makes no request and needs no server. Answers
    /// `Action::AttachmentViewReady` or `Action::AttachmentViewFailed`.
    LoadAttachmentView(AttachmentRef),
    /// Download an attachment to the downloads cache under a name the client builds (a hash of
    /// its path and an extension from its media type), unless that file is already there, so a
    /// second open makes no request. Only `open_attachment` starts it; the one other download is
    /// inside `LoadVideoPoster`, for a video within the inline limit or one the user asked for. Answers `Action::AttachmentDownloaded` or
    /// `Action::AttachmentDownloadFailed`.
    DownloadAttachment(DownloadRef),
    /// Make the poster of a video: from the poster cache when it holds one, else download the
    /// video to the downloads cache (unless it is there), run `ffmpeg` for one frame about a
    /// second in, decode it with the defensive decoder and keep it in the cache. Answers
    /// `Action::VideoPosterReady` or `Action::VideoPosterFailed`.
    LoadVideoPoster(VideoRef),
    /// Start `ffmpeg` on the downloaded video and a thread that paces its frames (see
    /// `video::spawn_playback`), after reading the length when asked to. Answers
    /// `Action::VideoStarted` (the frames then come as `Action::VideoFrame`, and the end as
    /// `Action::VideoEnded`) or `Action::VideoStartFailed`. Needs no server.
    StartVideo(VideoStart),
    /// Run an external program on a downloaded file: `command` is the program and its leading
    /// arguments, the file is added as the last one, never through a shell, with the standard
    /// streams detached. Answers `Action::AttachmentOpened`. Needs no server.
    OpenFile {
        command: Vec<String>,
        file: std::path::PathBuf,
    },
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
    // A file that is already downloaded needs no server, so it is opened even offline.
    if let Effect::DownloadAttachment(download) = &effect
        && let Some(file) = downloaded_file(download)
    {
        return vec![Action::AttachmentDownloaded {
            download: download.clone(),
            file,
        }];
    }
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
        Effect::OpenFile { command, file } => return vec![open_file(command, file).await],
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
                Effect::FetchAttachmentImage(image) => Action::AttachmentImageFailed(image),
                Effect::LoadVideoPoster(video) => Action::VideoPosterFailed {
                    video,
                    error: crate::video::VideoError::CannotShow,
                },
                Effect::LoadRecents | Effect::SaveRecents(_) | Effect::StartVideo(_) => {
                    return Vec::new();
                }
                Effect::LoadAttachmentView(image) => Action::AttachmentViewFailed(image),
                Effect::DownloadAttachment(download) => Action::AttachmentDownloadFailed {
                    download,
                    error: crate::api::DownloadError::Failed,
                },
                // Handled before the client was looked at.
                Effect::OpenFile { .. } => return Vec::new(),
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
        Effect::FetchAttachmentImage(image) => {
            vec![
                fetch_attachment_image(&client, attachment_cache_dir_from_env().as_deref(), image)
                    .await,
            ]
        }
        // Handled before the client was looked at.
        Effect::LoadAttachmentView(image) => vec![
            load_attachment_view(&client, attachment_cache_dir_from_env().as_deref(), image).await,
        ],
        Effect::DownloadAttachment(download) => {
            vec![download_attachment(&client, download).await]
        }
        Effect::LoadVideoPoster(video) => vec![
            load_video_poster(
                &client,
                attachment_cache_dir_from_env().as_deref(),
                downloads_dir_from_env().as_deref(),
                video,
            )
            .await,
        ],
        // Started by `spawn`, which has the channel the frames go to.
        Effect::LoadRecents
        | Effect::SaveRecents(_)
        | Effect::OpenFile { .. }
        | Effect::StartVideo(_) => Vec::new(),
    }
}

/// Start one playback run (see `Effect::StartVideo`) with the downloaded file looked for in
/// `downloads`. Blocking: the probe and the start wait on a program. The one answer is sent on
/// `tx`; the frames are sent by the reader thread.
pub fn start_video_blocking(
    downloads: Option<&std::path::Path>,
    tx: &Sender<Action>,
    start: VideoStart,
) {
    use crate::video::{Playback, PlaybackToken, VideoError, probe_duration, spawn_playback};
    let generation = start.generation;
    let started = (|| {
        let dir = downloads.ok_or(VideoError::CannotShow)?;
        let download = DownloadRef {
            path: start.video.path.clone(),
            mime: start.video.mime.clone(),
            size: start.video.size,
        };
        let file = downloaded_file_in(dir, &download).ok_or(VideoError::CannotShow)?;
        let duration = if start.probe {
            probe_duration(&start.ffprobe, &file)
        } else {
            None
        };
        let handle = spawn_playback(
            Playback {
                ffmpeg: start.video.ffmpeg.clone(),
                file,
                start_secs: start.start_secs,
                width: start.size.0,
                height: start.size.1,
                max_side: start.max_side,
                generation,
                paused: start.paused,
            },
            tx.clone(),
        )?;
        Ok((duration, PlaybackToken::new(handle)))
    })();
    let action = match started {
        Ok((duration, handle)) => Action::VideoStarted {
            generation,
            duration,
            handle,
        },
        Err(error) => Action::VideoStartFailed { generation, error },
    };
    let _ = tx.blocking_send(action);
}

/// The key a video's poster is cached under: the video's server path with a marker, so it never
/// collides with the file of an image at the same path.
fn poster_cache_key(path: &str) -> String {
    format!("{path}#poster")
}

/// Make the poster `video` names (see `Effect::LoadVideoPoster`). The poster cache is looked at
/// first and needs neither the server nor `ffmpeg`. A cached poster that no longer decodes is a
/// miss, not an error. Every failure is `VideoPosterFailed` with a fixed kind; nothing about the
/// file or the program's output is kept.
pub async fn load_video_poster(
    client: &Client,
    cache: Option<&std::path::Path>,
    downloads: Option<&std::path::Path>,
    video: VideoRef,
) -> Action {
    use crate::api::{DownloadError, MAX_ATTACHMENT_BYTES};
    use crate::emoji_image::{
        INLINE_SHRINK_TO, attachment, decode, read_attachment_cached, write_attachment_cached,
    };
    use crate::video::{VideoError, poster_png};
    let failed = |video, error| Action::VideoPosterFailed { video, error };
    let key = poster_cache_key(&video.path);
    let limits = attachment(INLINE_SHRINK_TO, None);
    if let Some(bytes) =
        cache.and_then(|dir| read_attachment_cached(dir, &key, video.size, MAX_ATTACHMENT_BYTES))
        && let Ok(decoded) = decode(&bytes, &limits)
        && crate::video::frame_len(decoded.width, decoded.height, crate::video::MAX_SIDE).is_some()
    {
        return Action::VideoPosterReady { video, decoded };
    }
    let download = DownloadRef {
        path: video.path.clone(),
        mime: video.mime.clone(),
        size: video.size,
    };
    let file = match download_attachment_into(client, downloads, download).await {
        Action::AttachmentDownloaded { file, .. } => file,
        Action::AttachmentDownloadFailed {
            error: DownloadError::TooLarge,
            ..
        } => return failed(video, VideoError::TooLarge),
        _ => return failed(video, VideoError::CannotShow),
    };
    let ffmpeg = video.ffmpeg.clone();
    // The program and the decoder both work on untrusted bytes: off the async threads.
    let made = tokio::task::spawn_blocking(move || {
        let png = poster_png(&ffmpeg, &file)?;
        let decoded = decode(&png, &limits).map_err(|_| VideoError::CannotShow)?;
        // The playback frame is this size, so it must be within the frame limit.
        crate::video::frame_len(decoded.width, decoded.height, crate::video::MAX_SIDE)
            .ok_or(VideoError::CannotShow)?;
        Ok::<_, VideoError>((decoded, png))
    })
    .await
    .unwrap_or(Err(VideoError::CannotShow));
    match made {
        Ok((decoded, png)) => {
            if let Some(dir) = cache {
                // A cache that cannot be written is not an error: the poster stays in memory.
                write_attachment_cached(dir, &key, video.size, &png);
            }
            Action::VideoPosterReady { video, decoded }
        }
        Err(error) => failed(video, error),
    }
}

/// The downloads directory from the environment, `None` when there is no usable one.
fn downloads_dir_from_env() -> Option<std::path::PathBuf> {
    crate::download::downloads_dir(std::env::var_os("XDG_CACHE_HOME"), std::env::var_os("HOME"))
}

/// The file of `download` when it is already in `dir`: a regular file under the name the client
/// builds. `None` for a media type outside the list, which is never opened.
fn downloaded_file_in(dir: &std::path::Path, download: &DownloadRef) -> Option<std::path::PathBuf> {
    let file = dir.join(crate::download::file_name(&download.path, &download.mime)?);
    std::fs::metadata(&file)
        .ok()
        .filter(std::fs::Metadata::is_file)
        .map(|_| file)
}

fn downloaded_file(download: &DownloadRef) -> Option<std::path::PathBuf> {
    downloaded_file_in(&downloads_dir_from_env()?, download)
}

/// Download `download` into `dir` (see `Effect::DownloadAttachment`). A media type outside the
/// list is refused before anything is requested.
pub async fn download_attachment_into(
    client: &Client,
    dir: Option<&std::path::Path>,
    download: DownloadRef,
) -> Action {
    use crate::api::{DownloadError, MAX_DOWNLOAD_BYTES};
    let failed = |download, error| Action::AttachmentDownloadFailed { download, error };
    let Some(name) = crate::download::file_name(&download.path, &download.mime) else {
        return failed(download, DownloadError::Refused);
    };
    let Some(dir) = dir else {
        return failed(download, DownloadError::Save);
    };
    if let Some(file) = downloaded_file_in(dir, &download) {
        return Action::AttachmentDownloaded { download, file };
    }
    if download.size.is_some_and(|n| n > MAX_DOWNLOAD_BYTES as u64) {
        return failed(download, DownloadError::TooLarge);
    }
    match client
        .download_attachment(&download.path, dir, &name, MAX_DOWNLOAD_BYTES)
        .await
    {
        Ok(file) => Action::AttachmentDownloaded { download, file },
        Err(error) => failed(download, error),
    }
}

async fn download_attachment(client: &Client, download: DownloadRef) -> Action {
    download_attachment_into(client, downloads_dir_from_env().as_deref(), download).await
}

/// How long a program that was just started is watched for an immediate failure.
const EARLY_EXIT_WATCH: std::time::Duration = std::time::Duration::from_millis(600);

/// Start `command` (the program and its leading arguments) with `file` as the last argument. No
/// shell: every entry is one argument as written. The standard streams are detached so the program
/// cannot draw over the interface. The program is watched for a moment, so one that fails at
/// once is reported; after that it is left to run, and a background task reaps it when it ends so
/// it never lingers as a zombie. The client never waits for it and never stops it.
pub async fn open_file(command: Vec<String>, file: std::path::PathBuf) -> Action {
    use std::process::{Command, Stdio};
    let Some((program, args)) = command.split_first() else {
        return Action::AttachmentOpened(Err(OpenError::CannotStart));
    };
    let spawned = Command::new(program)
        .args(args)
        .arg(&file)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = spawned else {
        return Action::AttachmentOpened(Err(OpenError::CannotStart));
    };
    let deadline = tokio::time::Instant::now() + EARLY_EXIT_WATCH;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Action::AttachmentOpened(Ok(())),
            Ok(Some(_)) => return Action::AttachmentOpened(Err(OpenError::ExitedWithError)),
            Ok(None) if tokio::time::Instant::now() < deadline => {
                tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            }
            // Still running, or it cannot be asked: it started.
            Ok(None) | Err(_) => break,
        }
    }
    // Reap it when it ends, off the async threads.
    drop(tokio::task::spawn_blocking(move || {
        let _ = child.wait();
    }));
    Action::AttachmentOpened(Ok(()))
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

/// The attachment image cache directory from the environment, `None` when there is none.
fn attachment_cache_dir_from_env() -> Option<std::path::PathBuf> {
    crate::emoji_image::attachment_cache_dir(
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
    )
}

/// Get and decode the attachment image `image` names, for its inline box: from `cache` when it
/// holds this path and size, else from the server (and then into the cache, if it can be written).
/// A failure of any step, including a cached file that no longer decodes, is
/// `AttachmentImageFailed`; the cause is not kept. A path the client may not request is a failure
/// without a request, and so is a reported size over the cap.
pub async fn fetch_attachment_image(
    client: &Client,
    cache: Option<&std::path::Path>,
    image: AttachmentRef,
) -> Action {
    use crate::api::MAX_ATTACHMENT_BYTES;
    use crate::emoji_image::{
        INLINE_ANIM_SHRINK_TO, INLINE_SHRINK_TO, attachment, decode, read_attachment_cached,
        write_attachment_cached,
    };
    let source = image.source.clone().unwrap_or_else(|| image.path.clone());
    let cached = cache
        .and_then(|dir| read_attachment_cached(dir, &source, image.size, MAX_ATTACHMENT_BYTES));
    let (bytes, from_cache) = match cached {
        Some(bytes) => (bytes, true),
        None => {
            if image.size.is_some_and(|n| n > MAX_ATTACHMENT_BYTES as u64) {
                return Action::AttachmentImageFailed(image);
            }
            match client.attachment_image(&source).await {
                Ok(bytes) => (bytes, false),
                Err(_) => return Action::AttachmentImageFailed(image),
            }
        }
    };
    // Decoding is CPU work on bytes from the server: off the async threads.
    let animated_side = image.animate.then_some(INLINE_ANIM_SHRINK_TO);
    let decoded = tokio::task::spawn_blocking(move || {
        decode(&bytes, &attachment(INLINE_SHRINK_TO, animated_side)).map(|decoded| (decoded, bytes))
    })
    .await
    .ok()
    .and_then(Result::ok);
    match decoded {
        Some((decoded, bytes)) => {
            if !from_cache && let Some(dir) = cache {
                // Only bytes that decoded are kept. A cache that cannot be written is not an error.
                write_attachment_cached(dir, &source, image.size, &bytes);
            }
            Action::AttachmentImageReady { image, decoded }
        }
        None => Action::AttachmentImageFailed(image),
    }
}

/// Get the original file of the attachment image `image` names and decode it for the full-size
/// view (up to `VIEW_SHRINK_TO` pixels): from `cache` when it holds this path and size, else from
/// the server under `MAX_VIEW_BYTES` (and then into the cache). A failure of any step is
/// `AttachmentViewFailed`, and the view then keeps the smaller image it already holds.
pub async fn load_attachment_view(
    client: &Client,
    cache: Option<&std::path::Path>,
    image: AttachmentRef,
) -> Action {
    use crate::api::MAX_VIEW_BYTES;
    use crate::emoji_image::{
        VIEW_ANIM_SHRINK_TO, VIEW_SHRINK_TO, attachment, decode, read_attachment_cached,
        write_attachment_cached,
    };
    let cached =
        cache.and_then(|dir| read_attachment_cached(dir, &image.path, image.size, MAX_VIEW_BYTES));
    let (bytes, from_cache) = match cached {
        Some(bytes) => (bytes, true),
        None => match client.attachment_bytes(&image.path, MAX_VIEW_BYTES).await {
            Ok(bytes) => (bytes, false),
            Err(_) => return Action::AttachmentViewFailed(image),
        },
    };
    let animated_side = image.animate.then_some(VIEW_ANIM_SHRINK_TO);
    let decoded = tokio::task::spawn_blocking(move || {
        decode(&bytes, &attachment(VIEW_SHRINK_TO, animated_side)).map(|decoded| (decoded, bytes))
    })
    .await
    .ok()
    .and_then(Result::ok);
    match decoded {
        Some((decoded, bytes)) => {
            if !from_cache && let Some(dir) = cache {
                write_attachment_cached(dir, &image.path, image.size, &bytes);
            }
            Action::AttachmentViewReady { image, decoded }
        }
        None => Action::AttachmentViewFailed(image),
    }
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
    let decoded = tokio::task::spawn_blocking(move || {
        decode(&bytes, &crate::emoji_image::EMOJI).map(|image| (image, bytes))
    })
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
    // A playback needs the channel for its frames, and waits on a program: its own blocking task.
    if let Effect::StartVideo(start) = effect {
        let downloads = downloads_dir_from_env();
        drop(tokio::task::spawn_blocking(move || {
            start_video_blocking(downloads.as_deref(), &tx, start);
        }));
        return;
    }
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
            attachments: Vec::new(),
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

    fn attachment_ref() -> AttachmentRef {
        AttachmentRef {
            path: "/file-upload/f1/photo.png".to_string(),
            size: Some(100),
            dims: Some((1, 1)),
            animate: false,
            source: None,
        }
    }

    async fn serve_attachment(server: &MockServer, bytes: Vec<u8>) {
        Mock::given(method("GET"))
            .and(path("/file-upload/f1/photo.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(server)
            .await;
    }

    fn download_ref() -> DownloadRef {
        DownloadRef {
            path: "/file-upload/v1/clip.mp4".to_string(),
            mime: "video/mp4".to_string(),
            size: Some(100),
        }
    }

    #[tokio::test]
    async fn a_download_is_saved_under_a_client_built_name_and_not_asked_for_twice() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file-upload/v1/clip.mp4"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"video bytes".to_vec()))
            .mount(&server)
            .await;
        let dir = cache_tmp("download-ok");
        let action =
            download_attachment_into(&client_for(&server), Some(&dir), download_ref()).await;
        let Action::AttachmentDownloaded { download, file } = action else {
            panic!("expected the file, got {action:?}");
        };
        assert_eq!(download, download_ref());
        assert_eq!(file.parent(), Some(dir.as_path()));
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.ends_with(".mp4") && !name.contains("clip"), "{name}");
        assert_eq!(std::fs::read(&file).unwrap(), b"video bytes");
        // Asking again opens the same file with no request.
        let second = MockServer::start().await;
        let again =
            download_attachment_into(&client_for(&second), Some(&dir), download_ref()).await;
        assert_eq!(
            again,
            Action::AttachmentDownloaded {
                download: download_ref(),
                file: file.clone()
            }
        );
        assert!(second.received_requests().await.unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_download_of_an_unlisted_type_or_a_hostile_name_is_refused_before_any_request() {
        let server = MockServer::start().await;
        let dir = cache_tmp("download-refused");
        let mut pdf = download_ref();
        pdf.mime = "application/pdf".to_string();
        let action = download_attachment_into(&client_for(&server), Some(&dir), pdf.clone()).await;
        assert_eq!(
            action,
            Action::AttachmentDownloadFailed {
                download: pdf,
                error: crate::api::DownloadError::Refused
            }
        );
        let mut absolute = download_ref();
        absolute.path = "https://elsewhere.example/file-upload/a/b.mp4".to_string();
        let action =
            download_attachment_into(&client_for(&server), Some(&dir), absolute.clone()).await;
        assert_eq!(
            action,
            Action::AttachmentDownloadFailed {
                download: absolute,
                error: crate::api::DownloadError::Refused
            }
        );
        assert!(server.received_requests().await.unwrap().is_empty());
        assert!(!dir.exists());
    }

    #[tokio::test]
    async fn a_download_over_the_reported_size_makes_no_request_and_a_404_fails() {
        let server = MockServer::start().await;
        let dir = cache_tmp("download-fail");
        let mut big = download_ref();
        big.size = Some(crate::api::MAX_DOWNLOAD_BYTES as u64 + 1);
        let action = download_attachment_into(&client_for(&server), Some(&dir), big.clone()).await;
        assert_eq!(
            action,
            Action::AttachmentDownloadFailed {
                download: big,
                error: crate::api::DownloadError::TooLarge
            }
        );
        assert!(server.received_requests().await.unwrap().is_empty());
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        let action =
            download_attachment_into(&client_for(&server), Some(&dir), download_ref()).await;
        assert!(matches!(
            action,
            Action::AttachmentDownloadFailed {
                error: crate::api::DownloadError::Failed,
                ..
            }
        ));
        // No directory to save in: the file could not be saved.
        let action = download_attachment_into(&client_for(&server), None, download_ref()).await;
        assert!(matches!(
            action,
            Action::AttachmentDownloadFailed {
                error: crate::api::DownloadError::Save,
                ..
            }
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A shell script in a fresh temporary directory, run as a program.
    #[cfg(unix)]
    fn script(name: &str, body: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        use std::os::unix::fs::PermissionsExt;
        let dir = cache_tmp(name);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("player.sh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        (dir, path)
    }

    #[cfg(unix)]
    async fn wait_for(path: &std::path::Path) -> String {
        for _ in 0..100 {
            if let Ok(text) = std::fs::read_to_string(path)
                && !text.is_empty()
            {
                return text;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("the program did not write its file");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn the_program_gets_its_arguments_then_the_file_as_one_last_argument() {
        // The script writes what it was given to a file named in its own text.
        let out = cache_tmp("open-args-out").with_extension("txt");
        let (dir2, program2) = script(
            "open-args2",
            &format!(
                "printf '%s\\n' \"$#\" \"$1\" \"$2\" \"$3\" > '{}'",
                out.display()
            ),
        );
        let file = dir2.join("my clip; rm -rf $HOME.mp4");
        let action = open_file(
            vec![
                program2.display().to_string(),
                "--force-window".to_string(),
                "a b".to_string(),
            ],
            file.clone(),
        )
        .await;
        assert_eq!(action, Action::AttachmentOpened(Ok(())));
        let seen = wait_for(&out).await;
        let lines: Vec<&str> = seen.lines().collect();
        assert_eq!(lines[0], "3", "no shell splitting: {seen}");
        assert_eq!(lines[1], "--force-window");
        assert_eq!(lines[2], "a b");
        assert_eq!(
            lines[3],
            file.display().to_string(),
            "the file, as one argument"
        );
        let _ = std::fs::remove_dir_all(&dir2);
        let _ = std::fs::remove_file(&out);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_program_that_is_missing_or_fails_at_once_is_reported() {
        let action = open_file(
            vec!["/nonexistent/definitely-not-a-player".to_string()],
            "/tmp/x.mp4".into(),
        )
        .await;
        assert_eq!(
            action,
            Action::AttachmentOpened(Err(OpenError::CannotStart))
        );
        assert_eq!(
            open_file(Vec::new(), "/tmp/x.mp4".into()).await,
            Action::AttachmentOpened(Err(OpenError::CannotStart))
        );
        let (dir, failing) = script("open-fail", "exit 3");
        let action = open_file(vec![failing.display().to_string()], "/tmp/x.mp4".into()).await;
        assert_eq!(
            action,
            Action::AttachmentOpened(Err(OpenError::ExitedWithError))
        );
        let (dir_ok, quick) = script("open-quick", "exit 0");
        let action = open_file(vec![quick.display().to_string()], "/tmp/x.mp4".into()).await;
        assert_eq!(action, Action::AttachmentOpened(Ok(())));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir_ok);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_program_that_keeps_running_does_not_hold_the_client() {
        let (dir, slow) = script("open-slow", "sleep 2");
        let started = std::time::Instant::now();
        let action = open_file(vec![slow.display().to_string()], "/tmp/x.mp4".into()).await;
        assert_eq!(action, Action::AttachmentOpened(Ok(())));
        assert!(started.elapsed() < std::time::Duration::from_secs(3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn an_animated_image_is_fetched_from_its_source_not_its_thumbnail() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file-upload/id/orig.gif"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(png_bytes()))
            .mount(&server)
            .await;
        let mut r = attachment_ref();
        r.path = "/file-upload/id/thumb-orig.gif".to_string();
        r.source = Some("/file-upload/id/orig.gif".to_string());
        let action = fetch_attachment_image(&client_for(&server), None, r.clone()).await;
        // The answer is filed under the thumbnail path the box looks up.
        assert!(
            matches!(&action, Action::AttachmentImageReady { image, .. } if *image == r),
            "{action:?}"
        );
        let seen = server.received_requests().await.unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].url.path(), "/file-upload/id/orig.gif");
    }

    #[tokio::test]
    async fn an_attachment_image_is_fetched_decoded_and_then_cached() {
        let server = MockServer::start().await;
        serve_attachment(&server, png_bytes()).await;
        let dir = cache_tmp("attach-fetch");
        let action =
            fetch_attachment_image(&client_for(&server), Some(&dir), attachment_ref()).await;
        match action {
            Action::AttachmentImageReady { image, decoded } => {
                assert_eq!(image, attachment_ref());
                assert_eq!((decoded.width, decoded.height), (1, 1));
                assert_eq!(&decoded.pixels[..], &[9, 8, 7, 255]);
            }
            other => panic!("expected the image, got {other:?}"),
        }
        // A later session reads it from disk and the server is not asked.
        let second = MockServer::start().await;
        let action =
            fetch_attachment_image(&client_for(&second), Some(&dir), attachment_ref()).await;
        assert!(
            matches!(action, Action::AttachmentImageReady { .. }),
            "{action:?}"
        );
        assert!(second.received_requests().await.unwrap().is_empty());
        // A different size is a different file: the server is asked.
        serve_attachment(&second, png_bytes()).await;
        let mut changed = attachment_ref();
        changed.size = Some(101);
        let action = fetch_attachment_image(&client_for(&second), Some(&dir), changed).await;
        assert!(matches!(action, Action::AttachmentImageReady { .. }));
        assert_eq!(second.received_requests().await.unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn without_a_usable_cache_the_attachment_image_still_loads() {
        let server = MockServer::start().await;
        serve_attachment(&server, png_bytes()).await;
        let action = fetch_attachment_image(&client_for(&server), None, attachment_ref()).await;
        assert!(matches!(action, Action::AttachmentImageReady { .. }));
        let dir = cache_tmp("attach-blocked");
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("attachments");
        std::fs::write(&blocker, b"x").unwrap();
        let action =
            fetch_attachment_image(&client_for(&server), Some(&blocker), attachment_ref()).await;
        assert!(matches!(action, Action::AttachmentImageReady { .. }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_failing_attachment_image_reports_the_failure_and_caches_nothing() {
        let dir = cache_tmp("attach-fail");
        // Not an image, whatever the type said: nothing is cached.
        let server = MockServer::start().await;
        serve_attachment(&server, b"<html>login</html>".to_vec()).await;
        let action =
            fetch_attachment_image(&client_for(&server), Some(&dir), attachment_ref()).await;
        assert_eq!(action, Action::AttachmentImageFailed(attachment_ref()));
        assert!(!dir.exists() || std::fs::read_dir(&dir).unwrap().next().is_none());
        // An image whose header reports too many pixels.
        let server = MockServer::start().await;
        let mut huge = Vec::new();
        {
            use image::ImageEncoder;
            let pixels = vec![0u8; 5000 * 4];
            image::codecs::png::PngEncoder::new(&mut huge)
                .write_image(&pixels, 5000, 1, image::ExtendedColorType::Rgba8)
                .unwrap();
        }
        serve_attachment(&server, huge).await;
        assert_eq!(
            fetch_attachment_image(&client_for(&server), Some(&dir), attachment_ref()).await,
            Action::AttachmentImageFailed(attachment_ref())
        );
        // Not found.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;
        assert_eq!(
            fetch_attachment_image(&client_for(&server), Some(&dir), attachment_ref()).await,
            Action::AttachmentImageFailed(attachment_ref())
        );
        // Over the cap while reading.
        let server = MockServer::start().await;
        serve_attachment(&server, vec![0u8; crate::api::MAX_ATTACHMENT_BYTES + 1]).await;
        assert_eq!(
            fetch_attachment_image(&client_for(&server), Some(&dir), attachment_ref()).await,
            Action::AttachmentImageFailed(attachment_ref())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_reported_size_over_the_cap_or_a_foreign_path_makes_no_request() {
        let server = MockServer::start().await;
        serve_attachment(&server, png_bytes()).await;
        let mut big = attachment_ref();
        big.size = Some(crate::api::MAX_ATTACHMENT_BYTES as u64 + 1);
        assert_eq!(
            fetch_attachment_image(&client_for(&server), None, big.clone()).await,
            Action::AttachmentImageFailed(big)
        );
        let mut foreign = attachment_ref();
        foreign.path = "https://elsewhere.example/file-upload/f1/photo.png".to_string();
        assert_eq!(
            fetch_attachment_image(&client_for(&server), None, foreign.clone()).await,
            Action::AttachmentImageFailed(foreign)
        );
        assert!(server.received_requests().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn the_attachment_effect_with_no_client_fails_the_image() {
        assert_eq!(
            run(
                Err(ApiError::Client),
                Effect::FetchAttachmentImage(attachment_ref())
            )
            .await,
            [Action::AttachmentImageFailed(attachment_ref())]
        );
    }

    #[tokio::test]
    async fn the_full_size_view_loads_the_original_and_caches_it() {
        use image::ImageEncoder;
        let mut big = Vec::new();
        image::codecs::png::PngEncoder::new(&mut big)
            .write_image(
                &vec![7u8; 2000 * 1000 * 4],
                2000,
                1000,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        let dir = cache_tmp("view");
        let r = AttachmentRef {
            path: "/file-upload/orig/photo.png".to_string(),
            size: None,
            dims: None,
            animate: false,
            source: None,
        };
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/file-upload/orig/photo.png"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(big))
            .expect(1)
            .mount(&server)
            .await;
        match load_attachment_view(&client_for(&server), Some(&dir), r.clone()).await {
            Action::AttachmentViewReady { image, decoded } => {
                assert_eq!(image, r);
                // Larger than the inline decode, within the view's limit.
                assert_eq!(decoded.width, crate::emoji_image::VIEW_SHRINK_TO);
                assert!(decoded.width > crate::emoji_image::INLINE_SHRINK_TO);
            }
            other => panic!("expected the image, got {other:?}"),
        }
        // The next time it comes from the cache: the server is not asked again.
        let again = load_attachment_view(&client_for(&server), Some(&dir), r.clone()).await;
        assert!(matches!(again, Action::AttachmentViewReady { .. }));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_full_size_load_that_fails_reports_it() {
        let r = AttachmentRef {
            path: "/file-upload/orig/photo.png".to_string(),
            size: None,
            dims: None,
            animate: false,
            source: None,
        };
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"<html>".to_vec()))
            .mount(&server)
            .await;
        assert_eq!(
            load_attachment_view(&client_for(&server), None, r.clone()).await,
            Action::AttachmentViewFailed(r.clone())
        );
        let foreign = AttachmentRef {
            path: "https://elsewhere.example/file-upload/a/b.png".to_string(),
            size: None,
            dims: None,
            animate: false,
            source: None,
        };
        assert_eq!(
            load_attachment_view(&client_for(&server), None, foreign.clone()).await,
            Action::AttachmentViewFailed(foreign)
        );
        // With no client at all the effect still answers.
        assert!(matches!(
            run(Err(ApiError::Client), Effect::LoadAttachmentView(r))
                .await
                .as_slice(),
            [Action::AttachmentViewFailed(_)]
        ));
    }
    // ---- video posters ---------------------------------------------------------------------

    fn png_of(width: u32, height: u32) -> Vec<u8> {
        use image::ImageEncoder;
        let mut out = Vec::new();
        let pixels = vec![200u8; (width * height * 4) as usize];
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(&pixels, width, height, image::ExtendedColorType::Rgba8)
            .unwrap();
        out
    }

    fn video_ref(ffmpeg: &str) -> VideoRef {
        VideoRef {
            path: "/file-upload/v1/clip.mp4".to_string(),
            mime: "video/mp4".to_string(),
            size: Some(100),
            ffmpeg: ffmpeg.to_string(),
        }
    }

    async fn serve_video(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/file-upload/v1/clip.mp4"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"video bytes".to_vec()))
            .mount(server)
            .await;
    }

    /// A fake `ffmpeg` that records each run and writes `poster` to stdout.
    fn fake_poster_program(dir: &std::path::Path, poster: &[u8]) -> String {
        std::fs::write(dir.join("poster.png"), poster).unwrap();
        crate::video::fake::script(
            dir,
            "ffmpeg",
            "echo run >> \"$(dirname \"$0\")/runs.log\"\ncat \"$(dirname \"$0\")/poster.png\"",
        )
    }

    #[tokio::test]
    async fn a_poster_is_made_cached_and_then_read_from_the_cache_without_running_anything() {
        let server = MockServer::start().await;
        serve_video(&server).await;
        let fake = crate::video::fake::temp_dir("poster-fx");
        let program = fake_poster_program(&fake, &png_of(16, 9));
        let downloads = cache_tmp("poster-downloads");
        let cache = cache_tmp("poster-cache");
        let action = load_video_poster(
            &client_for(&server),
            Some(&cache),
            Some(&downloads),
            video_ref(&program),
        )
        .await;
        let Action::VideoPosterReady { video, decoded } = action else {
            panic!("expected a poster, got {action:?}");
        };
        assert_eq!(video, video_ref(&program));
        assert_eq!((decoded.width, decoded.height), (16, 9));
        assert_eq!(
            std::fs::read_to_string(fake.join("runs.log"))
                .unwrap()
                .lines()
                .count(),
            1
        );
        assert_eq!(
            std::fs::read_dir(&cache).unwrap().count(),
            1,
            "the poster is cached"
        );
        // A later session: no server, and not even a program that exists.
        let second = MockServer::start().await;
        let nowhere = fake.join("gone").to_string_lossy().into_owned();
        let again = load_video_poster(
            &client_for(&second),
            Some(&cache),
            Some(&cache_tmp("poster-downloads-2")),
            video_ref(&nowhere),
        )
        .await;
        assert!(
            matches!(again, Action::VideoPosterReady { .. }),
            "{again:?}"
        );
        assert!(second.received_requests().await.unwrap().is_empty());
        assert_eq!(
            std::fs::read_to_string(fake.join("runs.log"))
                .unwrap()
                .lines()
                .count(),
            1,
            "ffmpeg did not run again"
        );
        let _ = std::fs::remove_dir_all(&cache);
        let _ = std::fs::remove_dir_all(&downloads);
    }

    #[tokio::test]
    async fn a_poster_that_cannot_be_made_fails_with_a_fixed_kind_and_caches_nothing() {
        use crate::video::VideoError;
        let server = MockServer::start().await;
        serve_video(&server).await;
        let fake = crate::video::fake::temp_dir("poster-fail");
        let downloads = cache_tmp("poster-fail-downloads");
        let cache = cache_tmp("poster-fail-cache");
        let run = |program: String, size: Option<u64>| {
            let (server, downloads, cache) = (&server, &downloads, &cache);
            async move {
                let mut video = video_ref(&program);
                video.size = size;
                load_video_poster(&client_for(server), Some(cache), Some(downloads), video).await
            }
        };
        let kind = |action: Action| match action {
            Action::VideoPosterFailed { error, .. } => error,
            other => panic!("expected a failure, got {other:?}"),
        };
        // ffmpeg is not installed.
        let missing = fake.join("none").to_string_lossy().into_owned();
        assert_eq!(
            kind(run(missing, Some(100)).await),
            VideoError::ProgramMissing
        );
        // ffmpeg fails.
        let failing = crate::video::fake::script(&fake, "fails", "exit 1");
        assert_eq!(kind(run(failing, Some(100)).await), VideoError::CannotShow);
        // ffmpeg writes something that is not an image.
        let junk = fake_poster_program(&fake, b"<html>not a png</html>");
        assert_eq!(kind(run(junk, Some(100)).await), VideoError::CannotShow);
        // A picture over the frame limit is refused, whatever the program says.
        let huge = fake_poster_program(&fake, &png_of(481, 10));
        assert_eq!(kind(run(huge, Some(100)).await), VideoError::CannotShow);
        assert!(
            !cache.exists() || std::fs::read_dir(&cache).unwrap().next().is_none(),
            "nothing cached"
        );
        // The file is over the download limit: nothing is requested.
        let quiet = MockServer::start().await;
        let action = load_video_poster(
            &client_for(&quiet),
            Some(&cache),
            Some(&cache_tmp("poster-big")),
            VideoRef {
                size: Some(crate::api::MAX_DOWNLOAD_BYTES as u64 + 1),
                ..video_ref("ffmpeg")
            },
        )
        .await;
        assert_eq!(kind(action), VideoError::TooLarge);
        assert!(quiet.received_requests().await.unwrap().is_empty());
        // A media type that is not a video type is refused before any request.
        let action = load_video_poster(
            &client_for(&quiet),
            Some(&cache),
            Some(&cache_tmp("poster-pdf")),
            VideoRef {
                mime: "application/pdf".to_string(),
                ..video_ref("ffmpeg")
            },
        )
        .await;
        assert_eq!(kind(action), VideoError::CannotShow);
        assert!(quiet.received_requests().await.unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&cache);
        let _ = std::fs::remove_dir_all(&downloads);
    }

    #[test]
    fn video_effects_and_actions_print_no_path_name_or_program_output() {
        let video = VideoRef {
            path: "/file-upload/SENTINEL-ROOM/SECRET-NAME.mp4".to_string(),
            mime: "video/mp4".to_string(),
            size: Some(5),
            ffmpeg: "/opt/SENTINEL-FFMPEG/bin/ffmpeg".to_string(),
        };
        let printed = [
            format!("{video:?}"),
            format!("{:?}", Effect::LoadVideoPoster(video.clone())),
            format!(
                "{:?}",
                Action::VideoPosterFailed {
                    video: video.clone(),
                    error: crate::video::VideoError::CannotShow
                }
            ),
            format!(
                "{:?}",
                Action::VideoPosterReady {
                    video,
                    decoded: crate::emoji_image::Rgba::default()
                }
            ),
            format!(
                "{:?}",
                Action::VideoFrame {
                    generation: 1,
                    index: 2,
                    pixels: vec![0xAB; 64].into()
                }
            ),
            format!("{:?}", Action::VideoEnded { generation: 1 }),
        ];
        for text in &printed {
            for secret in ["SENTINEL", "SECRET", "file-upload", "ffmpeg", "171"] {
                assert!(!text.contains(secret), "{secret} in {text}");
            }
        }
        assert!(printed[4].contains("<64> bytes"), "{}", printed[4]);
    }
}

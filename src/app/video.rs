//! Inline video: which attachments get a box, their posters and (below) the player.
//!
//! Pure: no I/O. The poster is an `Effect`, its answer an `Action`. The poster of a video is kept
//! as the `ImageState` of its path in `AttachmentImages`, so uploading, placing, the
//! near-the-viewport rule and freeing all work as for an image. Every notice is fixed wording:
//! no file name, path, size or program output is ever shown or logged.

use crate::action::Action;
use crate::api::{MAX_ATTACHMENT_BYTES, MAX_DOWNLOAD_BYTES};
use crate::app::attachments::ImageState;
use crate::app::emoji::Graphics;
use crate::app::{App, Mode, StatusMsg, image_view};
use crate::data::RoomId;
use crate::data::{Attachment, AttachmentKind, MessageKind};
use crate::effect::{Effect, VideoRef, VideoStart};
use crate::emoji_image::Rgba;
use crate::input::actions::name_of;
use crate::input::key::Chord;
use crate::input::key::format_keys;
use crate::input::keymap::Keymap;
use crate::input::trie::{Lookup, Trie};
use crate::video::PlaybackToken;
use std::sync::Arc;
use std::time::Duration;

/// The shape a video box is reserved in, as "reported" pixels (16:9): the server tells no video
/// size, so the box depends on the pane width and `image_max_rows` only.
pub const BOX_DIMS: (u32, u32) = (1280, 720);

/// True when `attachment` is a video the client may fetch and draw a box for: a video kind with a
/// plain `/file-upload/` path, a media type of the download table and a reported size, if any,
/// within the download limit. Anything else is drawn as its caption only.
pub fn is_fetchable_video(attachment: &Attachment) -> bool {
    attachment.kind == AttachmentKind::Video
        && attachment.path.is_some()
        && attachment
            .mime
            .as_deref()
            .is_some_and(|mime| crate::download::extension(mime).is_some())
        && attachment
            .size
            .is_none_or(|size| size <= MAX_DOWNLOAD_BYTES as u64)
}

/// True when the video is small enough for its poster to load without a key: its reported size is
/// within the inline image download limit. A larger one, or one with no reported size, waits for
/// `play_video`.
pub fn is_small(attachment: &Attachment) -> bool {
    attachment
        .size
        .is_some_and(|size| size <= MAX_ATTACHMENT_BYTES as u64)
}

/// The request for the poster of `attachment`, `None` when it is not a fetchable video.
pub fn reference(attachment: &Attachment, ffmpeg: &str) -> Option<VideoRef> {
    if !is_fetchable_video(attachment) {
        return None;
    }
    Some(VideoRef {
        path: attachment.path.clone()?,
        mime: attachment.mime.clone()?,
        size: attachment.size,
        ffmpeg: ffmpeg.to_string(),
    })
}

/// The first Normal-mode key bound to `play_video`, for the placeholder's hint. `None` when it
/// has none.
pub fn play_key(keymap: &Keymap) -> Option<String> {
    let mut keys: Vec<String> = keymap
        .bindings(Mode::Normal)
        .into_iter()
        .filter(|(_, action)| name_of(action) == Some("play_video"))
        .map(|(chords, _)| format_keys(&chords))
        .collect();
    keys.sort();
    keys.into_iter().next()
}

/// The first Normal-mode key bound to `play_video_fullscreen`, for the status row. `None` when
/// it has none.
pub fn fullscreen_key(keymap: &Keymap) -> Option<String> {
    let mut keys: Vec<String> = keymap
        .bindings(Mode::Normal)
        .into_iter()
        .filter(|(_, action)| **action == Action::PlayVideoFullscreen)
        .map(|(chords, _)| format_keys(&chords))
        .collect();
    keys.sort();
    keys.into_iter().next()
}

/// The first key bound to the player action `action` in the `[video]` table, as the status row
/// names it (`Space`, `Esc`, `q`, `C-c`). `None` when the action has no key.
pub fn video_key(keymap: &Keymap, action: &Action) -> Option<String> {
    let mut keys: Vec<String> = keymap
        .video_bindings()
        .into_iter()
        .filter(|(_, bound)| *bound == action)
        .map(|(chords, _)| format_keys(&chords))
        .collect();
    // The shortest spelling is the friendliest to read (`q` before `Esc`), then alphabetical.
    keys.sort_by(|a, b| (a.len(), a).cmp(&(b.len(), b)));
    keys.into_iter().next().map(|key| {
        key.strip_prefix('<')
            .and_then(|k| k.strip_suffix('>'))
            .filter(|k| !k.is_empty())
            .map_or(key.clone(), str::to_string)
    })
}

fn info(app: &mut App, text: &str) {
    app.status_msg = Some(StatusMsg::Info(text.to_string()));
}

fn error(app: &mut App, text: &str) {
    app.status_msg = Some(StatusMsg::Error(text.to_string()));
}

/// True when video boxes are drawn and loaded at all: the terminal can show images and the user
/// did not turn video off.
pub fn enabled(app: &App) -> bool {
    app.graphics == Graphics::Supported && app.settings.inline_video
}

/// Ask for the posters of the videos the open room draws that are small enough and were not asked
/// for before. Nothing is asked for unless video is enabled, and not while `ffmpeg` is known to be
/// missing (a `play_video` press tries again). Each path is asked for once per session and the
/// requests share the images' concurrency limit.
pub fn request_posters(app: &mut App) {
    if !enabled(app) || app.attachment_images.video_missing() {
        return;
    }
    let Some(room) = app.current_room.as_ref() else {
        return;
    };
    let mut wanted: Vec<VideoRef> = Vec::new();
    for message in app.messages.get(room).map_or(&[][..], Vec::as_slice) {
        if message.kind != MessageKind::Normal || !message.in_main_flow() {
            continue;
        }
        wanted.extend(
            message
                .attachments
                .iter()
                .filter(|a| is_small(a))
                .filter_map(|a| reference(a, &app.settings.ffmpeg)),
        );
    }
    for video in wanted {
        app.attachment_images.queue_video(video, false);
    }
}

/// `play_video`: step the video of the message under the cursor. A placeholder asks for its
/// poster; every notice is fixed wording and anything but the messages pane in Normal mode is a
/// silent no-op.
pub fn play(app: &mut App) {
    // The key that started a video also stops it (the player takes the keys, see `key`).
    if app.video.is_some() {
        stop(app);
        return;
    }
    open_video(app, false);
}

/// `play_video_fullscreen`: play the video of the message under the cursor in the overlay. A
/// player that is already running is expanded (inline) or closed (fullscreen).
pub fn play_fullscreen(app: &mut App) {
    if app.video.is_some() {
        toggle_fullscreen(app);
        return;
    }
    open_video(app, true);
}

/// The common start of `play` and `play_fullscreen`: check the message and terminal, then start
/// from the ready poster, or ask for it.
fn open_video(app: &mut App, fullscreen: bool) {
    let Some((room, message)) = image_view::message_under_cursor(app) else {
        return;
    };
    let message_id = message.id.clone();
    let mut videos = message
        .attachments
        .iter()
        .filter(|a| a.kind == AttachmentKind::Video);
    let first = (message.kind == MessageKind::Normal)
        .then(|| videos.next())
        .flatten();
    let Some(attachment) = first else {
        info(app, "no video in this message");
        return;
    };
    if app.graphics != Graphics::Supported {
        info(app, "this terminal cannot show images");
        return;
    }
    if !app.settings.inline_video {
        info(app, "inline video is turned off");
        return;
    }
    let Some(video) = reference(attachment, &app.settings.ffmpeg) else {
        error(app, "this video cannot be shown");
        return;
    };
    match app.attachment_images.state(&video.path) {
        Some(ImageState::Failed) => error(app, "this video could not be shown"),
        Some(ImageState::Ready { image, .. }) => {
            let size = (image.width, image.height);
            start(app, room, message_id, video, size, fullscreen);
        }
        None if app.attachment_images.is_requested(&video.path) => {
            info(app, "loading the video...");
            if fullscreen {
                app.attachment_images
                    .want_fullscreen(&video.path, room, message_id);
            }
        }
        None => {
            info(app, "loading the video...");
            if fullscreen {
                app.attachment_images
                    .want_fullscreen(&video.path, room, message_id);
            }
            app.attachment_images.queue_video(video, true);
        }
    }
    crate::app::attachments::pump(app);
}

/// What the next press of `play_video` does to the video of the message under the cursor, in the
/// wording the context popup lists: load it (the box is a placeholder), play it (it shows its
/// poster) or stop it (it plays or is paused). `None` for a message with no video the client may
/// fetch, and for one whose poster failed (there is nothing to press).
pub fn next_step(app: &App) -> Option<&'static str> {
    let (_, message) = image_view::message_under_cursor(app)?;
    if message.kind != MessageKind::Normal {
        return None;
    }
    let video = message.attachments.iter().find(|a| is_fetchable_video(a))?;
    let path = video.path.as_deref()?;
    if app.video.as_ref().is_some_and(|p| p.video.path == path) {
        return Some("stop the video");
    }
    match app.attachment_images.state(path) {
        Some(ImageState::Failed) => None,
        Some(ImageState::Ready { .. }) => Some("play the video"),
        None => Some("load the video"),
    }
}

/// True when the context popup lists `play_video_fullscreen`: the message under the cursor has a
/// video the client may fetch whose poster did not fail.
pub fn can_play_fullscreen(app: &App) -> bool {
    next_step(app).is_some()
}

/// A poster arrived, decoded.
pub fn on_poster_ready(app: &mut App, video: VideoRef, decoded: Rgba) {
    let images = &mut app.attachment_images;
    images.video_poster_done(&video.path);
    images.set_video_missing(false);
    let wanted = images.take_fullscreen(&video.path);
    // A play button on the still, so it reads as a video; playing frames replace it.
    let badged = crate::video::with_play_badge(&decoded);
    let size = (badged.width, badged.height);
    let path = video.path.clone();
    images.store_ready(path, badged, None);
    // A `play_video_fullscreen` press waited for this poster: open the overlay now, if the room
    // is still the one it was pressed in and nothing else plays.
    if let Some((room, message_id)) = wanted
        && enabled(app)
        && app.video.is_none()
        && app.current_room.as_ref() == Some(&room)
    {
        start(app, room, message_id, video, size, true);
    }
    crate::app::attachments::pump(app);
}

/// A poster could not be made. A missing program leaves the box a placeholder and stops the
/// automatic requests; any other failure is final for the session and the box goes away. A press
/// of `play_video` is answered in fixed words; an automatic request says nothing except that the
/// program is missing, once.
pub fn on_poster_failed(app: &mut App, video: VideoRef, why: crate::video::VideoError) {
    use crate::video::VideoError;
    let images = &mut app.attachment_images;
    let explicit = images.video_poster_done(&video.path);
    images.take_fullscreen(&video.path);
    let notice = match why {
        VideoError::ProgramMissing => {
            let first = !images.video_missing();
            images.set_video_missing(true);
            // The box stays a placeholder and a later press may try again.
            images.forget_request(&video.path);
            (explicit || first).then_some("ffmpeg was not found: video needs it")
        }
        VideoError::CannotShow => {
            images.store_failed(video.path);
            explicit.then_some("this video could not be shown")
        }
        VideoError::TooLarge => {
            images.store_failed(video.path);
            explicit.then_some("the file is too large to download")
        }
    };
    if let Some(text) = notice {
        error(app, text);
    }
    crate::app::attachments::pump(app);
}

// ---------------------------------------------------------------------------------------------
// The player
// ---------------------------------------------------------------------------------------------

/// The video that plays or is paused. One at a time. It holds no frame beyond the newest one that
/// has not been sent to the terminal yet.
pub struct Player {
    pub room: RoomId,
    pub message_id: String,
    /// The video, as the poster was asked for (its path keys the poster and the box).
    pub video: VideoRef,
    /// Whether the player is paused (also while the first run is still starting).
    pub paused: bool,
    /// The number of the current run. A seek starts a new one; frames and ends of an older run
    /// are ignored.
    pub generation: u64,
    /// Seconds into the video the current run started at.
    pub start_secs: u64,
    /// The number of the newest frame of the current run that arrived.
    pub index: u64,
    /// The length of the video, when the probe could read it.
    pub duration: Option<Duration>,
    /// The frame size: the poster's inline, its fullscreen scaling in the overlay.
    pub frame_size: (u32, u32),
    /// True once the current run reached the end of the video; only the overlay stays open then.
    pub ended: bool,
    /// True while the video plays in the fullscreen overlay instead of its inline box.
    pub fullscreen: bool,
    /// The cells the shown id was last placed in, so a changed overlay (a resize) places it again.
    placed: Option<(usize, usize)>,
    /// The two terminal image ids the frames alternate between.
    pub ids: [u32; 2],
    /// Which of the two ids the cells show now; `None` until the first frame was sent (the poster
    /// shows until then).
    pub shown: Option<usize>,
    /// The newest frame that was not sent to the terminal yet.
    pub pending: Option<Arc<[u8]>>,
    /// Keys of a sequence typed so far (`gv`).
    keys: Vec<Chord>,
    /// The run in progress; `None` while the first run (or a seek's) is still starting.
    handle: Option<PlaybackToken>,
}

impl Player {
    /// Where the video is now: the start of the run plus its newest frame, never past the end.
    pub fn position(&self) -> Duration {
        let at = crate::video::position(self.start_secs, self.index);
        self.duration.map_or(at, |d| at.min(d))
    }

    /// The most a side of this player's frames may be.
    pub fn max_side(&self) -> u32 {
        if self.fullscreen {
            crate::video::FULLSCREEN_SIDE
        } else {
            crate::video::MAX_SIDE
        }
    }

    /// True while a run is starting: no process answered yet.
    pub fn starting(&self) -> bool {
        self.handle.is_none()
    }
}

/// The keys a player takes: its own table, and the Normal-mode keys of `play_video`, so the key
/// that started a video also stops it.
fn capture_trie(keymap: &Keymap) -> Trie<Action> {
    let mut trie = Trie::new();
    for (chords, action) in keymap.bindings(Mode::Normal) {
        if matches!(action, Action::PlayVideo | Action::PlayVideoFullscreen) {
            trie.insert(&chords, action.clone());
        }
    }
    for (chords, action) in keymap.video_bindings() {
        trie.insert(&chords, action.clone());
    }
    trie
}

/// A key while a video plays or is paused: matched against the player's keys alone, so everything
/// else does nothing. A sequence in progress that nothing continues is dropped.
pub fn key(app: &mut App, chord: Chord) {
    let trie = capture_trie(&app.keymap);
    let Some(player) = app.video.as_mut() else {
        return;
    };
    player.keys.push(chord);
    let found = match trie.lookup(&player.keys) {
        Lookup::Exact(action) | Lookup::ExactAndPrefix(action) => Some(action.clone()),
        Lookup::Prefix => return,
        Lookup::None => None,
    };
    player.keys.clear();
    if let Some(action) = found {
        apply(app, action);
    }
}

/// Do what a player key says. A no-op without a player.
pub fn apply(app: &mut App, action: Action) {
    if app.video.is_none() {
        return;
    }
    match action {
        Action::VideoPause => pause(app),
        Action::VideoSeekBack => seek(app, -(crate::video::SEEK_SECS as i64)),
        Action::VideoSeekForward => seek(app, crate::video::SEEK_SECS as i64),
        Action::VideoSeekBackFar => seek(app, -(crate::video::SEEK_FAR_SECS as i64)),
        Action::VideoSeekForwardFar => seek(app, crate::video::SEEK_FAR_SECS as i64),
        Action::VideoStop | Action::PlayVideo => stop(app),
        Action::PlayVideoFullscreen => toggle_fullscreen(app),
        _ => {}
    }
}

/// Stop the video: end its process, free both of its images in the terminal and give the keys
/// back. The box shows its poster again. A no-op without a player.
pub fn stop(app: &mut App) {
    let Some(mut player) = app.video.take() else {
        return;
    };
    if let Some(handle) = player.handle.take() {
        handle.stop();
    }
    for id in player.ids {
        app.attachment_images.queue_delete(id);
    }
}

/// Begin playing the poster that is ready for `video`, in the message `message_id` of `room`:
/// another video that plays is stopped first. The notices are fixed words.
fn start(
    app: &mut App,
    room: RoomId,
    message_id: String,
    video: VideoRef,
    poster: (u32, u32),
    fullscreen: bool,
) {
    stop(app);
    let (size, max_side) = if fullscreen {
        (
            crate::video::fullscreen_size(poster, fullscreen_target(app)),
            crate::video::FULLSCREEN_SIDE,
        )
    } else {
        (poster, crate::video::MAX_SIDE)
    };
    let Some(ids) = app.attachment_images.take_video_ids() else {
        error(app, "this video could not be shown");
        return;
    };
    let generation = app.attachment_images.next_video_run();
    app.effects.push(Effect::StartVideo(VideoStart {
        video: video.clone(),
        ffprobe: app.settings.ffprobe.clone(),
        start_secs: 0,
        size,
        max_side,
        generation,
        paused: false,
        probe: true,
    }));
    app.video = Some(Player {
        room,
        message_id,
        video,
        paused: false,
        generation,
        start_secs: 0,
        index: 0,
        duration: None,
        frame_size: size,
        ended: false,
        fullscreen,
        placed: None,
        ids,
        shown: None,
        pending: None,
        keys: Vec::new(),
        handle: None,
    });
}

/// The pixels the overlay's picture can use: its inside in cells times the terminal's cell size.
fn fullscreen_target(app: &App) -> (u32, u32) {
    let inner = crate::ui::image_view::inner(app);
    let (cw, ch) = crate::ui::attachment::cell_px();
    (u32::from(inner.width) * cw, u32::from(inner.height) * ch)
}

/// `play_video_fullscreen` while a player exists: a fullscreen player closes, an inline one
/// expands. The expanded run continues from the same position at the larger size (a seek by
/// nothing), keeps its paused state, and the overlay says it is loading until the first new frame.
fn toggle_fullscreen(app: &mut App) {
    let target = fullscreen_target(app);
    let Some(player) = app.video.as_mut() else {
        return;
    };
    if player.fullscreen {
        stop(app);
        return;
    }
    player.fullscreen = true;
    let size = crate::video::fullscreen_size(player.frame_size, target);
    player.frame_size = size;
    // The frames of the inline size are not shown in the overlay.
    player.shown = None;
    player.pending = None;
    player.placed = None;
    seek(app, 0);
}

/// Pause or resume. A run that is still starting is told when it arrives.
fn pause(app: &mut App) {
    let Some(player) = app.video.as_mut() else {
        return;
    };
    player.paused = !player.paused;
    if let Some(handle) = &player.handle {
        handle.set_paused(player.paused);
    }
}

/// Seek `delta` seconds (negative is back) from where the video is: a new run starts there and
/// the old one ends; the frame on screen stays until the new run's first frame arrives. A seek
/// to the end of the video stops it, since nothing is left to play.
fn seek(app: &mut App, delta: i64) {
    let ffprobe = app.settings.ffprobe.clone();
    let Some(player) = app.video.as_mut() else {
        return;
    };
    let target = crate::video::seek_target(player.position(), delta, player.duration);
    let end = player.duration.map_or(crate::video::MAX_PLAY_SECS, |d| {
        d.as_secs().min(crate::video::MAX_PLAY_SECS)
    });
    if target >= end {
        stop(app);
        return;
    }
    if let Some(old) = player.handle.take() {
        old.stop();
    }
    let generation = app.attachment_images.next_video_run();
    let Some(player) = app.video.as_mut() else {
        return;
    };
    player.generation = generation;
    player.start_secs = target;
    player.index = 0;
    player.ended = false;
    app.effects.push(Effect::StartVideo(VideoStart {
        video: player.video.clone(),
        ffprobe,
        start_secs: target,
        size: player.frame_size,
        max_side: player.max_side(),
        generation,
        paused: player.paused,
        probe: false,
    }));
}

/// A run was started. A run of an older player or an older seek is ended at once.
pub fn on_started(
    app: &mut App,
    generation: u64,
    duration: Option<Duration>,
    handle: PlaybackToken,
) {
    match app.video.as_mut() {
        Some(player) if player.generation == generation => {
            handle.set_paused(player.paused);
            player.handle = Some(handle);
            if duration.is_some() {
                player.duration = duration;
            }
        }
        _ => handle.stop(),
    }
}

/// A run could not be started: the player goes away and the notice says why, in fixed words.
pub fn on_start_failed(app: &mut App, generation: u64, why: crate::video::VideoError) {
    if app
        .video
        .as_ref()
        .is_none_or(|p| p.generation != generation)
    {
        return;
    }
    stop(app);
    use crate::video::VideoError;
    error(
        app,
        match why {
            VideoError::ProgramMissing => "ffmpeg was not found: video needs it",
            VideoError::CannotShow | VideoError::TooLarge => "this video could not be shown",
        },
    );
}

/// A frame arrived: it replaces any frame not yet sent. A frame of another run is dropped.
pub fn on_frame(app: &mut App, generation: u64, index: u64, pixels: Arc<[u8]>) {
    let Some(player) = app.video.as_mut() else {
        return;
    };
    if player.generation != generation {
        return;
    }
    player.index = index;
    player.pending = Some(pixels);
}

/// The run ended: the video is over. Inline, the box shows its poster again. In the overlay the
/// last frame stays up (marked ended) until a stop key closes it or a seek plays again.
pub fn on_ended(app: &mut App, generation: u64) {
    match app.video.as_mut() {
        Some(player) if player.generation == generation && player.fullscreen => {
            player.ended = true;
        }
        Some(player) if player.generation == generation => stop(app),
        _ => {}
    }
}

/// Stop the player when it can no longer be shown: another room is open, its message left the
/// room, video is not allowed any more, or its box fell far from the viewport (new messages
/// push it, and a video nobody sees must not keep decoding).
pub fn sync(app: &mut App) {
    let Some(player) = app.video.as_ref() else {
        return;
    };
    // The overlay covers the room, so a video nobody could hide stays; it only needs its message.
    let keep = enabled(app)
        && app.current_room.as_ref() == Some(&player.room)
        && near_viewport(app, player).is_some_and(|near| near || player.fullscreen);
    if !keep {
        stop(app);
    }
}

/// True when the player's message is still in its room, has the video, and its rows are within a
/// screen of the viewport. `None` when the message is gone.
fn near_viewport(app: &App, player: &Player) -> Option<bool> {
    let msgs = app.messages.get(&player.room)?;
    let at = msgs.iter().position(|m| m.id == player.message_id)?;
    let has_video = msgs[at]
        .attachments
        .iter()
        .any(|a| a.path.as_deref() == Some(player.video.path.as_str()) && is_fetchable_video(a));
    if msgs[at].kind != MessageKind::Normal || !has_video {
        return None;
    }
    let (width, height) = crate::app::messages::viewport(app);
    let ctx = app.pane_row_ctx(&player.room, msgs);
    let starts = crate::app::messages::row_starts_of(msgs, width, &ctx);
    let top = crate::app::messages::view(app, &player.room).top_row;
    let height = usize::from(height);
    let (from, to) = (top.saturating_sub(height), top + 2 * height);
    Some(starts[at + 1] > from && starts[at] < to)
}

/// The cells (columns, rows) the player's frames are shown in: the box of its message, with the
/// frame fitted inside. `None` when the message is not found.
fn player_cells(app: &App, player: &Player) -> Option<(usize, usize)> {
    if player.fullscreen {
        return crate::ui::image_view::placement(app, player.frame_size);
    }
    let msgs = app.messages.get(&player.room)?;
    let message = msgs.iter().find(|m| m.id == player.message_id)?;
    let attachment = message
        .attachments
        .iter()
        .find(|a| a.path.as_deref() == Some(player.video.path.as_str()))?;
    let (width, _) = crate::app::messages::viewport(app);
    let ctx = app.pane_row_ctx(&player.room, msgs);
    let body = crate::ui::wrap::body_width(message, usize::from(width), &ctx);
    let boxed = crate::ui::attachment::box_of(attachment, body, app.settings.image_max_rows);
    (boxed.0 > 0).then(|| crate::ui::attachment::fitted(boxed, player.frame_size))
}

/// The terminal side of the player, written before the next frame is drawn: the newest frame that
/// was not sent yet, transmitted under the one of the two ids that is not shown, placed in the
/// box's cells, and then the cells switch to that id. The frame is dropped once sent, and each is
/// sent once. Only the colour of the cells changes, so no row moves.
pub fn prepare_player(app: &mut App, out: &mut Vec<String>) {
    let Some(player) = app.video.as_ref() else {
        return;
    };
    let cells = player_cells(app, player);
    // No new frame: a shown picture whose cells changed (the overlay was resized while paused) is
    // placed again, so it refits without a new transmit.
    if player.pending.is_none() {
        if let (Some(shown), Some(cells)) = (player.shown, cells)
            && player.fullscreen
            && player.placed != Some(cells)
        {
            let id = player.ids[shown];
            out.push(crate::kitty::place(id, cells.0, cells.1));
            if let Some(player) = app.video.as_mut() {
                player.placed = Some(cells);
            }
        }
        return;
    }
    let Some(player) = app.video.as_mut() else {
        return;
    };
    let Some(pixels) = player.pending.take() else {
        return;
    };
    let Some(cells) = cells else {
        return;
    };
    let hidden = player.shown.map_or(0, |shown| 1 - shown);
    let id = player.ids[hidden];
    out.push(crate::kitty::transmit(
        id,
        player.frame_size.0,
        player.frame_size.1,
        &pixels,
    ));
    // A transmit may drop what was placed under the id, so the placement goes with every frame.
    out.push(crate::kitty::place(id, cells.0, cells.1));
    player.shown = Some(hidden);
    player.placed = Some(cells);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::Focus;
    use crate::app::update::update;
    use crate::data::Message;
    use crate::effect::Effect;
    use crate::test_util::{app_80x24, press, sample_message};
    use crate::video::VideoError;

    pub(crate) const SMALL: &str = "/file-upload/v1/small.mp4";
    pub(crate) const BIG: &str = "/file-upload/v2/big.mp4";

    pub(crate) fn video_attachment(path: &str, size: Option<u64>) -> Attachment {
        Attachment {
            kind: AttachmentKind::Video,
            name: "SENTINEL-NAME.mp4".to_string(),
            path: Some(path.to_string()),
            full_path: None,
            mime: Some("video/mp4".to_string()),
            size,
            dims: None,
        }
    }

    /// An app on its first room, graphics on, the messages pane focused, one message per list of
    /// attachments.
    pub(crate) fn room_app(lists: Vec<Vec<Attachment>>) -> App {
        let mut app = app_80x24();
        app.graphics = Graphics::Supported;
        let room = app.rooms[0].id.clone();
        let msgs = lists
            .into_iter()
            .enumerate()
            .map(|(i, attachments)| Message {
                room_id: room.clone(),
                body: format!("see {i}"),
                attachments,
                plain_text: false,
                ..sample_message(&format!("m{i}"))
            })
            .collect();
        app.messages.insert(room.clone(), msgs);
        app.open_room(room);
        app.focus = Focus::Messages;
        app.effects.clear();
        app
    }

    pub(crate) fn posters(app: &App) -> Vec<String> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::LoadVideoPoster(v) => Some(v.path.clone()),
                _ => None,
            })
            .collect()
    }

    fn tick(app: &mut App) {
        update(app, Action::Clock(0));
    }

    fn status(app: &App) -> Option<String> {
        match app.status_msg.as_ref()? {
            StatusMsg::Info(t) | StatusMsg::Error(t) => Some(t.clone()),
        }
    }

    fn video_ref(path: &str) -> VideoRef {
        VideoRef {
            path: path.to_string(),
            mime: "video/mp4".to_string(),
            size: Some(1000),
            ffmpeg: "ffmpeg".to_string(),
        }
    }

    pub(crate) fn pixel() -> Rgba {
        Rgba {
            width: 2,
            height: 2,
            pixels: vec![9u8; 16].into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_small_video_gets_its_poster_without_a_key_and_a_large_one_waits() {
        let mut app = room_app(vec![
            vec![video_attachment(SMALL, Some(1_500_000))],
            vec![video_attachment(BIG, Some(40_000_000))],
            vec![video_attachment("/file-upload/v3/none.mp4", None)],
        ]);
        tick(&mut app);
        assert_eq!(posters(&app), [SMALL]);
        // The limit is the inline image limit, inclusive.
        let mut app = room_app(vec![
            vec![video_attachment(SMALL, Some(MAX_ATTACHMENT_BYTES as u64))],
            vec![video_attachment(BIG, Some(MAX_ATTACHMENT_BYTES as u64 + 1))],
        ]);
        tick(&mut app);
        assert_eq!(posters(&app), [SMALL]);
    }

    #[test]
    fn a_poster_is_asked_for_once_per_path() {
        let mut app = room_app(vec![
            vec![video_attachment(SMALL, Some(1000))],
            vec![video_attachment(SMALL, Some(1000))],
        ]);
        tick(&mut app);
        assert_eq!(posters(&app), [SMALL]);
        app.effects.clear();
        tick(&mut app);
        tick(&mut app);
        assert!(posters(&app).is_empty());
    }

    #[test]
    fn nothing_is_asked_without_images_or_with_the_setting_off() {
        let mut app = room_app(vec![vec![video_attachment(SMALL, Some(1000))]]);
        app.graphics = Graphics::Unsupported;
        tick(&mut app);
        assert!(app.effects.is_empty());
        app.graphics = Graphics::Supported;
        app.settings.inline_video = false;
        tick(&mut app);
        assert!(app.effects.is_empty());
        // `inline_images` is the images' setting, not the video's.
        app.settings.inline_video = true;
        app.settings.inline_images = false;
        tick(&mut app);
        assert_eq!(posters(&app), [SMALL]);
    }

    #[test]
    fn a_room_with_an_audio_file_and_a_large_video_requests_nothing() {
        let audio = Attachment {
            kind: AttachmentKind::Audio,
            mime: Some("audio/ogg".to_string()),
            ..video_attachment("/file-upload/a/v.ogg", Some(1000))
        };
        let mut app = room_app(vec![
            vec![audio],
            vec![video_attachment(BIG, Some(40_000_000))],
        ]);
        tick(&mut app);
        tick(&mut app);
        assert!(app.effects.is_empty(), "{:?}", app.effects);
        assert_eq!(app.attachment_images.requested_count(), 0);
    }

    #[test]
    fn a_video_that_cannot_be_fetched_is_never_asked_for() {
        let mut no_path = video_attachment(SMALL, Some(1000));
        no_path.path = None;
        let mut odd_type = video_attachment("/file-upload/v/x.bin", Some(1000));
        odd_type.mime = Some("application/octet-stream".to_string());
        let huge = video_attachment("/file-upload/v/h.mp4", Some(MAX_DOWNLOAD_BYTES as u64 + 1));
        let mut app = room_app(vec![vec![no_path], vec![odd_type], vec![huge]]);
        tick(&mut app);
        assert!(app.effects.is_empty());
        press(&mut app, "gv");
        assert_eq!(status(&app).as_deref(), Some("this video cannot be shown"));
        assert!(app.effects.is_empty());
    }

    #[test]
    fn the_key_loads_a_large_video_once() {
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        tick(&mut app);
        assert!(app.effects.is_empty());
        press(&mut app, "gv");
        assert_eq!(posters(&app), [BIG]);
        assert_eq!(status(&app).as_deref(), Some("loading the video..."));
        app.effects.clear();
        press(&mut app, "gv");
        assert!(
            posters(&app).is_empty(),
            "a second press waits for the first"
        );
    }

    #[test]
    fn a_press_on_a_message_with_no_video_or_outside_the_pane() {
        let mut app = room_app(vec![Vec::new()]);
        press(&mut app, "gv");
        assert_eq!(status(&app).as_deref(), Some("no video in this message"));
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        app.focus = Focus::Sidebar;
        press(&mut app, "gv");
        assert!(app.effects.is_empty());
        assert_eq!(status(&app), None, "nothing happens outside the pane");
    }

    #[test]
    fn the_press_says_so_when_images_cannot_be_shown_or_video_is_off() {
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        app.graphics = Graphics::Unsupported;
        press(&mut app, "gv");
        assert_eq!(
            status(&app).as_deref(),
            Some("this terminal cannot show images")
        );
        app.graphics = Graphics::Supported;
        app.settings.inline_video = false;
        press(&mut app, "gv");
        assert_eq!(status(&app).as_deref(), Some("inline video is turned off"));
        assert!(app.effects.is_empty());
    }

    #[test]
    fn the_poster_is_stored_under_the_video_path_and_a_failed_one_stays_failed() {
        let mut app = room_app(vec![
            vec![video_attachment(SMALL, Some(1000))],
            vec![video_attachment(BIG, Some(2000))],
        ]);
        tick(&mut app);
        assert_eq!(posters(&app), [SMALL, BIG]);
        update(
            &mut app,
            Action::VideoPosterReady {
                video: video_ref(SMALL),
                decoded: pixel(),
            },
        );
        assert!(matches!(
            app.attachment_images.state(SMALL),
            Some(ImageState::Ready {
                uploaded: false,
                ..
            })
        ));
        update(
            &mut app,
            Action::VideoPosterFailed {
                video: video_ref(BIG),
                error: VideoError::CannotShow,
            },
        );
        assert_eq!(app.attachment_images.state(BIG), Some(&ImageState::Failed));
        assert_eq!(app.attachment_images.in_flight(), 0);
        // Final for the session: no new request, and a press says so.
        app.effects.clear();
        tick(&mut app);
        assert!(app.effects.is_empty());
        press(&mut app, "gv");
        assert!(app.effects.is_empty());
    }

    #[test]
    fn posters_share_the_concurrency_limit_and_a_key_goes_first() {
        let lists = (0..5)
            .map(|i| {
                vec![video_attachment(
                    &format!("/file-upload/s{i}/c.mp4"),
                    Some(1000),
                )]
            })
            .chain([vec![video_attachment(BIG, Some(40_000_000))]])
            .collect();
        let mut app = room_app(lists);
        tick(&mut app);
        assert_eq!(posters(&app).len(), crate::app::attachments::MAX_IN_FLIGHT);
        assert_eq!(
            app.attachment_images.queued(),
            0,
            "images have their own queue"
        );
        // The press for the big one waits for a slot, then goes before the other posters.
        app.effects.clear();
        press(&mut app, "gv");
        assert!(posters(&app).is_empty());
        let first = "/file-upload/s0/c.mp4";
        update(
            &mut app,
            Action::VideoPosterReady {
                video: video_ref(first),
                decoded: pixel(),
            },
        );
        assert_eq!(posters(&app), [BIG], "{:?}", posters(&app));
    }

    #[test]
    fn images_and_posters_share_the_slots() {
        let mut app = room_app(vec![
            vec![crate::app::attachments::tests::image_attachment(
                "/file-upload/i/a.png",
            )],
            vec![video_attachment(SMALL, Some(1000))],
        ]);
        tick(&mut app);
        assert_eq!(app.attachment_images.in_flight(), 2);
        assert_eq!(app.effects.len(), 2);
    }

    #[test]
    fn a_missing_program_leaves_the_placeholder_and_stops_the_automatic_requests() {
        let mut app = room_app(vec![vec![video_attachment(SMALL, Some(1000))]]);
        tick(&mut app);
        update(
            &mut app,
            Action::VideoPosterFailed {
                video: video_ref(SMALL),
                error: VideoError::ProgramMissing,
            },
        );
        assert_eq!(
            status(&app).as_deref(),
            Some("ffmpeg was not found: video needs it")
        );
        assert_eq!(
            app.attachment_images.state(SMALL),
            None,
            "the box stays a placeholder"
        );
        // No automatic retry loop.
        app.effects.clear();
        tick(&mut app);
        tick(&mut app);
        assert!(posters(&app).is_empty(), "{:?}", app.effects);
        // A press tries again and says so again when it still fails.
        press(&mut app, "gv");
        assert_eq!(posters(&app), [SMALL]);
        update(
            &mut app,
            Action::VideoPosterFailed {
                video: video_ref(SMALL),
                error: VideoError::ProgramMissing,
            },
        );
        assert_eq!(
            status(&app).as_deref(),
            Some("ffmpeg was not found: video needs it")
        );
        // Once a poster is made the automatic requests go on: a new video is asked for again.
        press(&mut app, "gv");
        assert!(app.attachment_images.video_missing());
        update(
            &mut app,
            Action::VideoPosterReady {
                video: video_ref(SMALL),
                decoded: pixel(),
            },
        );
        assert!(!app.attachment_images.video_missing());
        let room = app.current_room.clone().unwrap();
        let more = Message {
            room_id: room.clone(),
            attachments: vec![video_attachment("/file-upload/v9/other.mp4", Some(1000))],
            ..sample_message("m-new")
        };
        app.messages.get_mut(&room).unwrap().push(more);
        app.effects.clear();
        tick(&mut app);
        assert_eq!(posters(&app), ["/file-upload/v9/other.mp4"]);
    }

    #[test]
    fn failure_notices_are_fixed_words_without_server_text() {
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        press(&mut app, "gv");
        update(
            &mut app,
            Action::VideoPosterFailed {
                video: video_ref(BIG),
                error: VideoError::CannotShow,
            },
        );
        assert_eq!(
            status(&app).as_deref(),
            Some("this video could not be shown")
        );
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        press(&mut app, "gv");
        update(
            &mut app,
            Action::VideoPosterFailed {
                video: video_ref(BIG),
                error: VideoError::TooLarge,
            },
        );
        assert_eq!(
            status(&app).as_deref(),
            Some("the file is too large to download")
        );
        // An automatic poster that fails says nothing.
        let mut app = room_app(vec![vec![video_attachment(SMALL, Some(1000))]]);
        tick(&mut app);
        update(
            &mut app,
            Action::VideoPosterFailed {
                video: video_ref(SMALL),
                error: VideoError::CannotShow,
            },
        );
        assert_eq!(status(&app), None);
    }
    #[test]
    fn a_ready_poster_is_sent_to_the_terminal_once_and_placed_in_its_box() {
        let mut app = room_app(vec![vec![video_attachment(SMALL, Some(1000))]]);
        tick(&mut app);
        update(
            &mut app,
            Action::VideoPosterReady {
                video: video_ref(SMALL),
                decoded: pixel(),
            },
        );
        let first = crate::app::attachments::prepare_frame(&mut app);
        let Some(ImageState::Ready {
            id,
            uploaded,
            placed,
            ..
        }) = app.attachment_images.state(SMALL).cloned()
        else {
            panic!("the poster is ready");
        };
        assert!(uploaded && placed.is_some());
        assert!(
            first.iter().any(|c| c.contains(&format!("i={id}"))),
            "{first:?}"
        );
        assert_eq!(
            first.len(),
            2,
            "one transmit and one placement: {} bytes",
            first.iter().map(String::len).sum::<usize>()
        );
        // Nothing again until something changes.
        assert!(crate::app::attachments::prepare_frame(&mut app).is_empty());
        // With video off nothing is written at all.
        let mut off = room_app(vec![vec![video_attachment(SMALL, Some(1000))]]);
        off.settings.inline_video = false;
        off.settings.inline_images = false;
        assert!(crate::app::attachments::prepare_frame(&mut off).is_empty());
    }
    // ---- the player ------------------------------------------------------------------------

    use crate::video::fake::{gone, temp_dir, token};
    use std::path::PathBuf;

    fn poster(w: u32, h: u32) -> Rgba {
        Rgba {
            width: w,
            height: h,
            pixels: vec![7u8; (w * h * 4) as usize].into(),
            ..Default::default()
        }
    }

    /// An app whose newest message holds a small video with its poster ready (16 by 9 pixels),
    /// after a plain message. The messages pane is focused, the cursor on the video.
    pub(crate) fn ready_app() -> App {
        let mut app = room_app(vec![Vec::new(), vec![video_attachment(SMALL, Some(1000))]]);
        tick(&mut app);
        update(
            &mut app,
            Action::VideoPosterReady {
                video: video_ref(SMALL),
                decoded: poster(16, 9),
            },
        );
        app.effects.clear();
        app
    }

    fn starts(app: &mut App) -> Vec<VideoStart> {
        let all: Vec<Effect> = std::mem::take(&mut app.effects);
        all.into_iter()
            .filter_map(|e| match e {
                Effect::StartVideo(start) => Some(start),
                _ => None,
            })
            .collect()
    }

    /// Press the key on the poster and answer the start: the player runs a fake process.
    pub(crate) fn playing(tag: &str) -> (App, PathBuf, u32) {
        let mut app = ready_app();
        press(&mut app, "gv");
        let start = starts(&mut app).pop().expect("a run was asked for");
        let dir = temp_dir(tag);
        let (handle, pid) = token(&dir, "run1");
        update(
            &mut app,
            Action::VideoStarted {
                generation: start.generation,
                duration: Some(Duration::from_secs(48)),
                handle,
            },
        );
        (app, dir, pid)
    }

    pub(crate) fn frame(app: &mut App, index: u64) {
        let generation = app.video.as_ref().unwrap().generation;
        update(
            app,
            Action::VideoFrame {
                generation,
                index,
                pixels: vec![index as u8; 16 * 9 * 4].into(),
            },
        );
    }

    fn cursor(app: &App) -> usize {
        let room = app.current_room.clone().unwrap();
        crate::app::messages::view(app, &room).cursor
    }

    #[test]
    fn the_key_on_a_poster_starts_playback_and_takes_the_keys() {
        let mut app = ready_app();
        assert!(app.video.is_none());
        press(&mut app, "gv");
        let player = app.video.as_ref().expect("a player");
        assert_eq!(player.video.path, SMALL);
        assert_eq!(player.frame_size, (16, 9), "the poster's size");
        assert!(player.starting() && !player.paused);
        let runs = starts(&mut app);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].start_secs, 0);
        assert!(runs[0].probe && !runs[0].paused);
        assert_eq!(runs[0].size, (16, 9));
        assert_eq!(runs[0].video.path, SMALL);
        assert_eq!(runs[0].ffprobe, "ffprobe");
        assert_eq!(runs[0].generation, app.video.as_ref().unwrap().generation);
    }

    #[test]
    fn the_run_started_makes_the_player_play_and_a_stale_run_is_ended() {
        let (app, dir, pid) = playing("started");
        let player = app.video.as_ref().unwrap();
        assert!(!player.starting());
        assert_eq!(player.duration, Some(Duration::from_secs(48)));
        assert!(!gone(pid) || !cfg!(target_os = "linux"), "the process runs");
        // A run of no player at all is ended at once.
        let mut other = ready_app();
        let (stale, stale_pid) = token(&dir, "stale");
        update(
            &mut other,
            Action::VideoStarted {
                generation: 99,
                duration: None,
                handle: stale,
            },
        );
        assert!(gone(stale_pid), "the run nobody wants was killed");
        drop(app);
        assert!(gone(pid), "dropping the app ends the run");
    }

    #[test]
    fn the_key_again_stops_the_video_and_shows_the_poster() {
        let (mut app, _dir, pid) = playing("gv-again");
        press(&mut app, "gv");
        assert!(app.video.is_none());
        assert!(gone(pid));
        assert!(matches!(
            app.attachment_images.state(SMALL),
            Some(ImageState::Ready { .. })
        ));
        // And the keys are the Normal mode's again.
        press(&mut app, "k");
        assert_eq!(cursor(&app), 0);
    }

    #[test]
    fn another_video_takes_over_and_the_first_is_stopped() {
        let (mut app, dir, pid) = playing("takeover");
        let old = app.video.as_ref().unwrap().generation;
        let ids = app.video.as_ref().unwrap().ids;
        let other = VideoRef {
            path: BIG.to_string(),
            ..video_ref(BIG)
        };
        let room = app.current_room.clone().unwrap();
        // A newer message holds the other video.
        let newer = Message {
            room_id: room.clone(),
            attachments: vec![video_attachment(BIG, Some(1000))],
            ..sample_message("m-newer")
        };
        app.messages.get_mut(&room).unwrap().push(newer);
        start(&mut app, room, "m-newer".to_string(), other, (4, 4), false);
        assert!(gone(pid), "the first process was ended");
        let player = app.video.as_ref().unwrap();
        assert_ne!(player.generation, old, "its own run number");
        assert_eq!(player.video.path, BIG);
        assert_eq!(player.ids, ids, "the two ids are reused");
        assert_eq!(starts(&mut app).len(), 1);
        // The old run's late frames and its end are not the new player's.
        update(
            &mut app,
            Action::VideoFrame {
                generation: old,
                index: 3,
                pixels: vec![0; 16].into(),
            },
        );
        update(&mut app, Action::VideoEnded { generation: old });
        let player = app.video.as_ref().expect("still the new player");
        assert!(player.pending.is_none());
        drop(dir);
    }

    #[test]
    fn a_failed_start_says_so_in_fixed_words_and_gives_the_keys_back() {
        for (error, text) in [
            (
                VideoError::ProgramMissing,
                "ffmpeg was not found: video needs it",
            ),
            (VideoError::CannotShow, "this video could not be shown"),
        ] {
            let mut app = ready_app();
            press(&mut app, "gv");
            let generation = app.video.as_ref().unwrap().generation;
            // A failure of an older run says nothing.
            update(
                &mut app,
                Action::VideoStartFailed {
                    generation: generation + 7,
                    error,
                },
            );
            assert!(app.video.is_some());
            update(&mut app, Action::VideoStartFailed { generation, error });
            assert!(app.video.is_none());
            assert_eq!(status(&app).as_deref(), Some(text));
            assert!(
                !text.contains("SENTINEL") && !text.contains("small") && !text.contains('/'),
                "{text}"
            );
        }
    }

    #[test]
    fn a_missing_downloaded_file_fails_the_run_without_a_process() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(4);
        let dir = temp_dir("no-file");
        crate::effect::start_video_blocking(
            Some(&dir),
            &tx,
            VideoStart {
                video: video_ref(SMALL),
                ffprobe: "ffprobe".to_string(),
                start_secs: 0,
                size: (16, 9),
                max_side: crate::video::MAX_SIDE,
                generation: 5,
                paused: false,
                probe: true,
            },
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            Action::VideoStartFailed {
                generation: 5,
                error: VideoError::CannotShow
            }
        );
        crate::effect::start_video_blocking(
            None,
            &tx,
            VideoStart {
                video: video_ref(SMALL),
                ffprobe: "ffprobe".to_string(),
                start_secs: 0,
                size: (16, 9),
                max_side: crate::video::MAX_SIDE,
                generation: 6,
                paused: false,
                probe: true,
            },
        );
        assert!(matches!(
            rx.try_recv().unwrap(),
            Action::VideoStartFailed { generation: 6, .. }
        ));
    }

    #[test]
    fn a_run_is_started_on_the_downloaded_file_and_the_length_is_probed() {
        let dir = temp_dir("start-ok");
        let downloads = dir.join("downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let name = crate::download::file_name(SMALL, "video/mp4").unwrap();
        std::fs::write(downloads.join(name), b"video bytes").unwrap();
        let ffmpeg = crate::video::fake::frames_script(&dir, "ffmpeg", 2, 16 * 9 * 4, 0);
        let ffprobe = crate::video::fake::script(&dir, "ffprobe", "echo 12.5");
        let (tx, mut rx) = tokio::sync::mpsc::channel(8);
        let mut video = video_ref(SMALL);
        video.ffmpeg = ffmpeg;
        crate::effect::start_video_blocking(
            Some(&downloads),
            &tx,
            VideoStart {
                video,
                ffprobe,
                start_secs: 0,
                size: (16, 9),
                max_side: crate::video::MAX_SIDE,
                generation: 3,
                paused: false,
                probe: true,
            },
        );
        let mut started = None;
        let mut frames = 0;
        let mut ended = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !ended && std::time::Instant::now() < deadline {
            match rx.try_recv() {
                Ok(Action::VideoStarted {
                    generation,
                    duration,
                    handle,
                }) => started = Some((generation, duration, handle)),
                Ok(Action::VideoFrame { generation: 3, .. }) => frames += 1,
                Ok(Action::VideoEnded { generation: 3 }) => ended = true,
                Ok(other) => panic!("{other:?}"),
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        let (generation, duration, _handle) = started.expect("started");
        assert_eq!(
            (generation, duration),
            (3, Some(Duration::from_millis(12_500)))
        );
        assert_eq!(frames, 2);
        assert!(ended);
        // The program was given the file of the cache, by its client-built name.
        let runs = std::fs::read_to_string(dir.join("runs.log")).unwrap();
        assert!(
            runs.contains(&downloads.to_string_lossy().into_owned()),
            "{runs}"
        );
    }

    // ---- frames to the terminal ------------------------------------------------------------

    fn escapes(app: &mut App) -> Vec<String> {
        crate::app::attachments::prepare_frame(app)
    }

    #[test]
    fn a_frame_is_sent_once_under_the_hidden_id_and_the_ids_alternate() {
        let (mut app, _dir, _pid) = playing("alternate");
        let ids = app.video.as_ref().unwrap().ids;
        // The poster goes up first; with no frame yet nothing else does, and the poster shows.
        let poster_upload = escapes(&mut app);
        assert!(
            poster_upload
                .iter()
                .all(|c| !c.contains(&format!("i={},", ids[0])))
        );
        let again = escapes(&mut app);
        assert!(
            again.is_empty(),
            "no frame yet: the poster shows: {again:?}"
        );
        let mut used = Vec::new();
        for n in 0..4u64 {
            frame(&mut app, n);
            let sent = escapes(&mut app);
            assert_eq!(sent.len(), 2, "one transmit and one placement: {sent:?}");
            let shown = app.video.as_ref().unwrap().shown.unwrap();
            used.push(shown);
            let id = ids[shown];
            assert!(
                sent[0].contains(&format!("i={id},")) && sent[0].contains("a=t"),
                "{:?}",
                &sent[0][..60]
            );
            assert!(
                sent[1].contains(&format!("i={id},")) && sent[1].contains("a=p"),
                "{}",
                sent[1]
            );
            // Not sent twice.
            assert!(escapes(&mut app).is_empty());
            assert!(
                app.video.as_ref().unwrap().pending.is_none(),
                "dropped once sent"
            );
        }
        assert_eq!(used, [0, 1, 0, 1], "alternating");
    }

    #[test]
    fn only_the_newest_waiting_frame_is_sent() {
        let (mut app, _dir, _pid) = playing("newest");
        escapes(&mut app); // the poster
        frame(&mut app, 1);
        frame(&mut app, 2);
        frame(&mut app, 3);
        let sent = escapes(&mut app);
        assert_eq!(sent.len(), 2, "one frame");
        assert_eq!(app.video.as_ref().unwrap().index, 3);
        assert_eq!(
            app.video.as_ref().unwrap().position(),
            crate::video::position(0, 3)
        );
    }

    #[test]
    fn a_frame_of_another_run_is_dropped_and_none_means_nothing_without_a_player() {
        let (mut app, _dir, _pid) = playing("stale-frame");
        let generation = app.video.as_ref().unwrap().generation;
        update(
            &mut app,
            Action::VideoFrame {
                generation: generation + 1,
                index: 9,
                pixels: vec![1; 16 * 9 * 4].into(),
            },
        );
        assert!(app.video.as_ref().unwrap().pending.is_none());
        let mut idle = ready_app();
        escapes(&mut idle); // the poster
        update(
            &mut idle,
            Action::VideoFrame {
                generation: 1,
                index: 0,
                pixels: vec![1; 4].into(),
            },
        );
        assert!(escapes(&mut idle).is_empty());
    }

    type Rgb = Option<(u8, u8, u8)>;

    fn colour_and_text(app: &App) -> (Vec<Rgb>, Vec<String>) {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
        terminal.draw(|f| crate::ui::view(f, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut colours = Vec::new();
        let mut text = Vec::new();
        for y in 0..buffer.area.height {
            let mut line = String::new();
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                line.push_str(cell.symbol());
                if cell.symbol().starts_with(crate::kitty::PLACEHOLDER)
                    && let ratatui::style::Color::Rgb(r, g, b) = cell.fg
                {
                    colours.push(Some((r, g, b)));
                }
            }
            text.push(line);
        }
        (colours, text)
    }

    #[test]
    fn the_cells_only_change_colour_when_a_frame_arrives_and_when_it_stops() {
        // What the room looks like before anything plays, with the poster uploaded so it draws.
        let mut idle = ready_app();
        escapes(&mut idle);
        let (poster_colours, poster_text) = colour_and_text(&idle);
        let poster_id = match idle.attachment_images.state(SMALL) {
            Some(ImageState::Ready { id, .. }) => *id,
            _ => panic!("poster"),
        };
        let (mut app, _dir, _pid) = playing("colour");
        escapes(&mut app);
        assert!(!poster_colours.is_empty());
        assert!(
            poster_colours
                .iter()
                .all(|c| *c == Some(crate::kitty::id_color(poster_id)))
        );
        let ids = app.video.as_ref().unwrap().ids;
        for (n, want) in [(0u64, ids[0]), (1, ids[1]), (2, ids[0])] {
            frame(&mut app, n);
            escapes(&mut app);
            let (colours, text) = colour_and_text(&app);
            assert_eq!(colours.len(), poster_colours.len(), "same cells");
            assert!(
                colours
                    .iter()
                    .all(|c| *c == Some(crate::kitty::id_color(want))),
                "frame {n}"
            );
            // The status row changes (the position moves); every other row is the same.
            let changed: Vec<usize> = (0..text.len())
                .filter(|i| text[*i] != poster_text[*i])
                .collect();
            assert!(changed.len() <= 1, "{changed:?}");
        }
        press(&mut app, "q");
        escapes(&mut app);
        let (colours, text) = colour_and_text(&app);
        assert!(
            colours
                .iter()
                .all(|c| *c == Some(crate::kitty::id_color(poster_id)))
        );
        assert_eq!(text, poster_text, "back to the poster, no row moved");
    }

    #[test]
    fn stopping_deletes_both_ids_and_the_rest_stays() {
        let (mut app, _dir, pid) = playing("deletes");
        let ids = app.video.as_ref().unwrap().ids;
        frame(&mut app, 0);
        escapes(&mut app);
        press(&mut app, "<Esc>");
        assert!(app.video.is_none());
        assert!(gone(pid));
        let out = escapes(&mut app);
        for id in ids {
            assert!(out.contains(&crate::kitty::delete(id)), "{out:?}");
        }
        assert!(
            !out.iter().any(|c| c.contains("a=t") || c.contains("a=p")),
            "nothing else is sent"
        );
        assert!(escapes(&mut app).is_empty(), "deleted once");
        // The poster is still in the terminal: its image is not among the deletes.
        let poster_id = match app.attachment_images.state(SMALL) {
            Some(ImageState::Ready { id, .. }) => *id,
            _ => panic!("poster"),
        };
        assert!(!ids.contains(&poster_id));
    }

    // ---- controls --------------------------------------------------------------------------

    #[test]
    fn every_default_key_does_its_job_and_other_keys_do_nothing() {
        let (mut app, _dir, _pid) = playing("keys");
        let before = cursor(&app);
        for other in [
            "j", "k", "x", "gg", "G", "<C-d>", ":", "i", "dd", "?", "gp", "<CR>",
        ] {
            press(&mut app, other);
            assert!(app.video.is_some(), "{other} must not stop the video");
            assert_eq!(cursor(&app), before, "{other}");
            assert_eq!(app.mode, Mode::Normal);
            assert!(app.effects.is_empty(), "{other}: {:?}", app.effects);
        }
        // Space pauses and resumes.
        press(&mut app, "<Space>");
        assert!(app.video.as_ref().unwrap().paused);
        press(&mut app, "<Space>");
        assert!(!app.video.as_ref().unwrap().paused);
        // The stop keys.
        for stop in ["<Esc>", "q", "<C-c>"] {
            let (mut app, _dir, pid) = playing(&format!("stop-{}", stop.len()));
            press(&mut app, stop);
            assert!(app.video.is_none(), "{stop}");
            assert!(gone(pid));
            assert_eq!(app.mode, Mode::Normal);
            press(&mut app, "k");
            assert_eq!(cursor(&app), 0, "the cursor moves again after {stop}");
        }
    }

    #[test]
    fn a_rebound_key_works_and_an_unbound_one_does_nothing() {
        let mut app = ready_app();
        app.keymap = crate::test_util::app_with_config(
            crate::test_util::config_from_keymap(
                "[video]\n\",\" = \"video_seek_back\"\n\".\" = \"video_seek_forward\"\n\"h\" = \"none\"\n\"s\" = \"video_pause\"\n\"<Space>\" = \"none\"\n",
            ),
            80,
            24,
        )
        .keymap;
        press(&mut app, "gv");
        let start = starts(&mut app).pop().unwrap();
        let (handle, _pid) = token(&temp_dir("rebound"), "run1");
        update(
            &mut app,
            Action::VideoStarted {
                generation: start.generation,
                duration: Some(Duration::from_secs(48)),
                handle,
            },
        );
        press(&mut app, "<Space>");
        assert!(!app.video.as_ref().unwrap().paused, "Space is unbound");
        press(&mut app, "s");
        assert!(app.video.as_ref().unwrap().paused);
        press(&mut app, "h");
        assert!(starts(&mut app).is_empty(), "h is unbound");
        press(&mut app, ".");
        assert_eq!(starts(&mut app).len(), 1, "the new forward key seeks");
        press(&mut app, ",");
        assert_eq!(starts(&mut app).len(), 1, "and the new back key too");
    }

    #[test]
    fn a_player_with_the_play_key_unbound_still_stops_with_its_own_keys() {
        let mut app = ready_app();
        app.keymap = crate::test_util::app_with_config(
            crate::test_util::config_from_keymap(
                "[normal]\n\"gv\" = \"none\"\n\"V\" = \"play_video\"\n",
            ),
            80,
            24,
        )
        .keymap;
        press(&mut app, "V");
        assert!(app.video.is_some());
        press(&mut app, "gv");
        assert!(app.video.is_some(), "gv is not bound any more");
        press(&mut app, "V");
        assert!(app.video.is_none(), "the key that started it stops it");
    }

    // ---- seeking ---------------------------------------------------------------------------

    fn at(app: &mut App, start_secs: u64) {
        let player = app.video.as_mut().unwrap();
        player.start_secs = start_secs;
        player.index = 0;
    }

    #[test]
    fn a_seek_starts_a_new_run_at_the_new_time_and_ends_the_old_one() {
        for (keys, from, to) in [
            ("l", 12, 17),
            ("h", 12, 7),
            ("L", 12, 42),
            ("H", 40, 10),
            ("h", 2, 0),
            ("H", 20, 0),
        ] {
            let (mut app, dir, pid) = playing("seek");
            let old = app.video.as_ref().unwrap().generation;
            at(&mut app, from);
            frame(&mut app, 0);
            escapes(&mut app);
            press(&mut app, keys);
            let player = app.video.as_ref().expect("still playing");
            assert_ne!(player.generation, old, "{keys}");
            assert_eq!(
                (player.start_secs, player.index),
                (to, 0),
                "{keys} from {from}"
            );
            assert!(player.starting(), "the new run is on its way");
            assert!(gone(pid), "the old process was killed");
            let runs = starts(&mut app);
            assert_eq!(runs.len(), 1, "{keys}");
            assert_eq!(runs[0].start_secs, to);
            let generation = app.video.as_ref().unwrap().generation;
            assert_eq!(runs[0].generation, generation);
            assert!(!runs[0].probe, "the length is known already");
            assert!(!runs[0].paused);
            drop(dir);
        }
    }

    #[test]
    fn the_old_picture_stays_until_the_new_runs_first_frame_and_old_frames_are_dropped() {
        let (mut app, _dir, _pid) = playing("seek-frames");
        at(&mut app, 12);
        frame(&mut app, 0);
        escapes(&mut app);
        let shown = app.video.as_ref().unwrap().shown;
        let old = app.video.as_ref().unwrap().generation;
        press(&mut app, "l");
        assert_eq!(
            app.video.as_ref().unwrap().shown,
            shown,
            "the old picture stays"
        );
        assert!(escapes(&mut app).is_empty());
        // A frame of the old run still in the channel is dropped.
        update(
            &mut app,
            Action::VideoFrame {
                generation: old,
                index: 40,
                pixels: vec![9; 16 * 9 * 4].into(),
            },
        );
        assert!(app.video.as_ref().unwrap().pending.is_none());
        assert_eq!(
            app.video.as_ref().unwrap().position(),
            Duration::from_secs(17)
        );
        // The new run's first frame is shown, and the position follows it.
        let generation = app.video.as_ref().unwrap().generation;
        update(
            &mut app,
            Action::VideoFrame {
                generation,
                index: 0,
                pixels: vec![5; 16 * 9 * 4].into(),
            },
        );
        assert_eq!(escapes(&mut app).len(), 2);
        assert_ne!(app.video.as_ref().unwrap().shown, shown, "the other id");
    }

    #[test]
    fn seeking_while_paused_shows_the_new_frame_and_stays_paused() {
        let (mut app, _dir, _pid) = playing("seek-paused");
        at(&mut app, 12);
        press(&mut app, "<Space>");
        assert!(app.video.as_ref().unwrap().paused);
        press(&mut app, "l");
        let runs = starts(&mut app);
        assert_eq!(runs.len(), 1);
        assert!(runs[0].paused, "the new run holds after its first frame");
        assert!(app.video.as_ref().unwrap().paused);
        // The run answers: it is told to stay paused.
        let (handle, _pid) = token(&temp_dir("seek-paused-2"), "run2");
        update(
            &mut app,
            Action::VideoStarted {
                generation: runs[0].generation,
                duration: None,
                handle,
            },
        );
        assert_eq!(
            app.video.as_ref().unwrap().duration,
            Some(Duration::from_secs(48)),
            "kept"
        );
        assert!(app.video.as_ref().unwrap().paused);
    }

    #[test]
    fn a_seek_to_the_end_stops_the_video_and_a_run_that_ended_does_too() {
        let (mut app, _dir, pid) = playing("seek-end");
        at(&mut app, 45);
        press(&mut app, "l");
        assert!(
            app.video.is_none(),
            "within 5 s of the end, forward goes to the end and stops"
        );
        assert!(gone(pid));
        assert!(starts(&mut app).is_empty());
        // Far forward from the middle also lands on the end.
        let (mut app, _dir, _pid) = playing("seek-end-far");
        at(&mut app, 30);
        press(&mut app, "L");
        assert!(app.video.is_none());
        // The end of the stream returns to the poster and gives the keys back.
        let (mut app, _dir, pid) = playing("ended");
        let generation = app.video.as_ref().unwrap().generation;
        update(
            &mut app,
            Action::VideoEnded {
                generation: generation + 1,
            },
        );
        assert!(app.video.is_some(), "a stale end is ignored");
        update(&mut app, Action::VideoEnded { generation });
        assert!(app.video.is_none());
        assert!(gone(pid));
        press(&mut app, "k");
        assert_eq!(cursor(&app), 0);
    }

    #[test]
    fn a_seek_while_the_first_run_is_still_starting_is_safe() {
        let mut app = ready_app();
        press(&mut app, "gv");
        let first = starts(&mut app).pop().unwrap();
        press(&mut app, "L");
        let runs = starts(&mut app);
        assert_eq!(runs.len(), 1);
        assert_ne!(runs[0].generation, first.generation);
        // The first run answers late: it is ended, not adopted.
        let (late, late_pid) = token(&temp_dir("late"), "late");
        update(
            &mut app,
            Action::VideoStarted {
                generation: first.generation,
                duration: Some(Duration::from_secs(48)),
                handle: late,
            },
        );
        assert!(gone(late_pid));
        assert!(app.video.as_ref().unwrap().starting());
    }

    #[test]
    fn a_seek_in_a_video_of_unknown_length_is_bounded_by_the_first_ten_minutes() {
        let mut app = ready_app();
        press(&mut app, "gv");
        let start = starts(&mut app).pop().unwrap();
        let (handle, _pid) = token(&temp_dir("unknown"), "run1");
        update(
            &mut app,
            Action::VideoStarted {
                generation: start.generation,
                duration: None,
                handle,
            },
        );
        assert_eq!(app.video.as_ref().unwrap().duration, None);
        at(&mut app, 100);
        press(&mut app, "L");
        assert_eq!(app.video.as_ref().unwrap().start_secs, 130);
        at(&mut app, 590);
        press(&mut app, "L");
        assert!(app.video.is_none(), "ten minutes is the end");
    }
    // ---- the context popup -----------------------------------------------------------------

    fn play_row(app: &App) -> Option<(String, String)> {
        crate::app::context::lines(app)
            .rows
            .into_iter()
            .find(|r| r.desc.ends_with(" the video"))
            .map(|r| (r.keys, r.desc))
    }

    #[test]
    fn the_popup_names_the_next_step_of_the_video_with_its_live_key() {
        // A placeholder (a large video, nothing asked): load it.
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        assert_eq!(
            play_row(&app),
            Some(("gv".to_string(), "load the video".to_string()))
        );
        // The poster is ready: play it.
        update(
            &mut app,
            Action::VideoPosterReady {
                video: video_ref(BIG),
                decoded: pixel(),
            },
        );
        assert_eq!(
            play_row(&app).map(|r| r.1).as_deref(),
            Some("play the video")
        );
        // Playing or paused: stop it.
        press(&mut app, "gv");
        assert!(app.video.is_some());
        assert_eq!(
            play_row(&app).map(|r| r.1).as_deref(),
            Some("stop the video")
        );
        app.video.as_mut().unwrap().paused = true;
        assert_eq!(
            play_row(&app).map(|r| r.1).as_deref(),
            Some("stop the video")
        );
        stop(&mut app);
        assert_eq!(
            play_row(&app).map(|r| r.1).as_deref(),
            Some("play the video")
        );
    }

    #[test]
    fn the_popup_lists_no_video_row_without_a_fetchable_video_or_a_poster_that_failed() {
        let app = room_app(vec![Vec::new()]);
        assert_eq!(play_row(&app), None, "no video");
        let mut no_path = video_attachment(SMALL, Some(1000));
        no_path.path = None;
        let app = room_app(vec![vec![no_path]]);
        assert_eq!(play_row(&app), None, "a video the client may not fetch");
        let mut app = room_app(vec![vec![video_attachment(SMALL, Some(1000))]]);
        tick(&mut app);
        update(
            &mut app,
            Action::VideoPosterFailed {
                video: video_ref(SMALL),
                error: VideoError::CannotShow,
            },
        );
        assert_eq!(play_row(&app), None, "nothing to press once it failed");
        // Not on the sidebar or the input.
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        app.focus = Focus::Input;
        assert_eq!(play_row(&app), None);
    }

    #[test]
    fn the_popup_row_follows_the_live_keymap() {
        let row = |src: &str| {
            let configured = crate::test_util::app_with_config(
                crate::test_util::config_from_keymap(src),
                80,
                24,
            );
            let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
            app.keymap = configured.keymap;
            play_row(&app).map(|r| r.0)
        };
        assert_eq!(
            row("[normal]\n\"V\" = \"play_video\"\n").as_deref(),
            Some("V gv"),
            "every key, sorted"
        );
        assert_eq!(
            row("[normal]\n\"gv\" = \"none\"\n"),
            None,
            "unbound: left out"
        );
        assert!(crate::input::actions::ACTION_NAMES.contains(&"play_video"));
    }
    // ---- lifecycle -------------------------------------------------------------------------

    fn assert_stopped(app: &mut App, pid: u32, why: &str) {
        assert!(app.video.is_none(), "{why}: the player is gone");
        assert!(gone(pid), "{why}: the process is gone");
        // Both of its images are freed in the terminal.
        let out = escapes(app);
        assert!(
            out.iter().filter(|c| c.contains("a=d")).count() >= 2,
            "{why}: {out:?}"
        );
    }

    #[test]
    fn changing_room_stops_the_video() {
        let (mut app, _dir, pid) = playing("room-change");
        let other = app.rooms[1].id.clone();
        app.open_room(other);
        tick(&mut app);
        assert_stopped(&mut app, pid, "room change");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn a_deleted_message_stops_the_video_and_releases_the_keys() {
        let (mut app, _dir, pid) = playing("deleted");
        let room = app.current_room.clone().unwrap();
        update(
            &mut app,
            Action::RtDeleted {
                room,
                id: "m1".to_string(),
            },
        );
        assert_stopped(&mut app, pid, "message deleted");
        press(&mut app, "k");
        assert_eq!(cursor(&app), 0, "the cursor moves again");
    }

    #[test]
    fn a_terminal_or_setting_that_no_longer_allows_video_stops_it() {
        let (mut app, _dir, pid) = playing("unsupported");
        app.graphics = Graphics::Unsupported;
        tick(&mut app);
        assert_stopped(&mut app, pid, "graphics gone");
        let (mut app, _dir, pid) = playing("setting-off");
        app.settings.inline_video = false;
        tick(&mut app);
        assert_stopped(&mut app, pid, "inline_video off");
    }

    #[test]
    fn a_video_pushed_far_out_of_view_stops() {
        let (mut app, _dir, pid) = playing("pushed");
        let room = app.current_room.clone().unwrap();
        // The newest messages arrive; the pane follows the bottom, so the video moves up.
        for n in 0..80 {
            let msg = Message {
                room_id: room.clone(),
                body: format!("later {n}"),
                ts: 1_000 + n,
                ..sample_message(&format!("later-{n}"))
            };
            update(&mut app, Action::RtMessage(msg));
            if app.video.is_none() {
                break;
            }
        }
        assert_stopped(&mut app, pid, "pushed out of view");
    }

    #[test]
    fn a_video_that_stays_in_view_keeps_playing_while_other_messages_arrive() {
        let (mut app, _dir, _pid) = playing("stays");
        let room = app.current_room.clone().unwrap();
        let msg = Message {
            room_id: room,
            body: "one more".to_string(),
            ts: 2_000,
            ..sample_message("one-more")
        };
        update(&mut app, Action::RtMessage(msg));
        assert!(app.video.is_some());
    }

    #[test]
    fn quitting_stops_the_video() {
        let (mut app, _dir, pid) = playing("quit");
        update(&mut app, Action::Quit);
        assert!(app.should_quit);
        assert_stopped(&mut app, pid, "quit");
    }

    #[test]
    fn nothing_is_left_after_stop_and_after_the_app_is_dropped() {
        let (mut app, _dir, pid) = playing("no-leftovers");
        press(&mut app, "q");
        assert!(gone(pid), "after stop");
        let (app, _dir, pid) = playing("no-leftovers-drop");
        assert!(
            !gone(pid) || !cfg!(target_os = "linux"),
            "it runs while the app lives"
        );
        drop(app);
        assert!(gone(pid), "after the app is dropped");
        // A player that never got its run answered owns no process.
        let mut app = ready_app();
        press(&mut app, "gv");
        press(&mut app, "q");
        assert!(app.video.is_none());
    }

    // ---- fullscreen --------------------------------------------------------------------------

    /// Press `gV` on the poster and answer the start: a fullscreen player on a fake process.
    pub(crate) fn playing_fullscreen(tag: &str) -> (App, PathBuf, u32) {
        let mut app = ready_app();
        press(&mut app, "gV");
        let start = starts(&mut app).pop().expect("a run was asked for");
        let dir = temp_dir(tag);
        let (handle, pid) = token(&dir, "run1");
        update(
            &mut app,
            Action::VideoStarted {
                generation: start.generation,
                duration: Some(Duration::from_secs(48)),
                handle,
            },
        );
        (app, dir, pid)
    }

    #[test]
    fn the_fullscreen_key_on_a_poster_starts_a_larger_run_from_the_start() {
        let mut app = ready_app();
        press(&mut app, "gV");
        let player = app.video.as_ref().expect("a player");
        assert!(player.fullscreen && player.starting() && !player.paused);
        // The poster is 16 by 9 and the overlay's inside is 78 by 20 cells of 8 by 16 pixels: the
        // frame is the poster's shape fitted in 624 by 320 pixels.
        assert_eq!(player.frame_size, (568, 320));
        let runs = starts(&mut app);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].size, (568, 320));
        assert_eq!(runs[0].max_side, crate::video::FULLSCREEN_SIDE);
        assert_eq!((runs[0].start_secs, runs[0].probe), (0, true));
        // The mode, focus and cursor stay as they were.
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, Focus::Messages);
    }

    #[test]
    fn the_inline_run_keeps_its_own_size_and_limit() {
        let mut app = ready_app();
        press(&mut app, "gv");
        let runs = starts(&mut app);
        assert_eq!(runs[0].size, (16, 9));
        assert_eq!(runs[0].max_side, crate::video::MAX_SIDE);
        assert!(!app.video.as_ref().unwrap().fullscreen);
    }

    #[test]
    fn the_fullscreen_key_on_a_placeholder_downloads_once_and_then_opens_the_overlay() {
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        press(&mut app, "gV");
        assert_eq!(status(&app).as_deref(), Some("loading the video..."));
        assert_eq!(posters(&app), [BIG.to_string()]);
        assert!(app.video.is_none(), "nothing plays before the poster");
        // Pressing again while it loads asks for nothing more.
        app.effects.clear();
        press(&mut app, "gV");
        assert!(posters(&app).is_empty());
        update(
            &mut app,
            Action::VideoPosterReady {
                video: video_ref(BIG),
                decoded: poster(16, 9),
            },
        );
        let player = app.video.as_ref().expect("the overlay opened");
        assert!(player.fullscreen);
        let runs = starts(&mut app);
        assert_eq!(runs.len(), 1, "one run, no second download");
        assert_eq!(runs[0].size, (568, 320));
        assert!(posters(&app).is_empty());
    }

    #[test]
    fn a_plain_poster_wait_does_not_open_the_overlay_and_a_failure_forgets_the_wish() {
        // `gv` on a placeholder only loads the poster, as before.
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        press(&mut app, "gv");
        update(
            &mut app,
            Action::VideoPosterReady {
                video: video_ref(BIG),
                decoded: poster(16, 9),
            },
        );
        assert!(app.video.is_none());
        // A failed download says so and leaves no overlay behind.
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        press(&mut app, "gV");
        update(
            &mut app,
            Action::VideoPosterFailed {
                video: video_ref(BIG),
                error: VideoError::CannotShow,
            },
        );
        assert!(app.video.is_none());
        assert_eq!(
            status(&app).as_deref(),
            Some("this video could not be shown")
        );
    }

    #[test]
    fn a_waiting_fullscreen_press_is_dropped_when_the_room_changed() {
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        press(&mut app, "gV");
        let other = app.rooms[1].id.clone();
        app.open_room(other);
        tick(&mut app);
        update(
            &mut app,
            Action::VideoPosterReady {
                video: video_ref(BIG),
                decoded: poster(16, 9),
            },
        );
        assert!(app.video.is_none());
    }

    #[test]
    fn the_fullscreen_key_says_so_when_there_is_nothing_to_show() {
        let mut app = room_app(vec![vec![]]);
        press(&mut app, "gV");
        assert_eq!(status(&app).as_deref(), Some("no video in this message"));
        let mut app = ready_app();
        app.graphics = Graphics::Unsupported;
        press(&mut app, "gV");
        assert_eq!(
            status(&app).as_deref(),
            Some("this terminal cannot show images")
        );
        let mut app = ready_app();
        app.settings.inline_video = false;
        press(&mut app, "gV");
        assert_eq!(status(&app).as_deref(), Some("inline video is turned off"));
        assert!(app.video.is_none());
        // Outside the messages pane it does nothing.
        let mut app = ready_app();
        app.focus = Focus::Sidebar;
        press(&mut app, "gV");
        assert!(app.video.is_none());
    }

    #[test]
    fn a_poster_that_failed_cannot_be_played_fullscreen() {
        let mut app = room_app(vec![vec![video_attachment(SMALL, Some(1000))]]);
        tick(&mut app);
        update(
            &mut app,
            Action::VideoPosterFailed {
                video: video_ref(SMALL),
                error: VideoError::CannotShow,
            },
        );
        app.effects.clear();
        press(&mut app, "gV");
        assert!(app.video.is_none());
        assert!(starts(&mut app).is_empty());
    }

    #[test]
    fn the_fullscreen_key_expands_a_playing_inline_run_at_the_same_position() {
        let (mut app, _dir, pid) = playing("expand-playing");
        at(&mut app, 12);
        let old = app.video.as_ref().unwrap().generation;
        press(&mut app, "gV");
        let player = app.video.as_ref().expect("still one player");
        assert!(player.fullscreen && !player.paused);
        assert_ne!(player.generation, old, "a new run");
        assert_eq!(player.frame_size, (568, 320));
        assert!(
            player.shown.is_none() && player.pending.is_none(),
            "the small picture is not shown in the overlay"
        );
        let runs = starts(&mut app);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].start_secs, 12);
        assert_eq!(runs[0].size, (568, 320));
        assert_eq!(runs[0].max_side, crate::video::FULLSCREEN_SIDE);
        assert!(!runs[0].probe && !runs[0].paused);
        assert!(gone(pid), "the inline run's process is ended");
    }

    #[test]
    fn expanding_a_paused_run_keeps_it_paused() {
        let (mut app, _dir, _pid) = playing("expand-paused");
        at(&mut app, 20);
        press(&mut app, "<Space>");
        assert!(app.video.as_ref().unwrap().paused);
        press(&mut app, "gV");
        assert!(app.video.as_ref().unwrap().paused);
        let runs = starts(&mut app);
        assert_eq!((runs[0].start_secs, runs[0].paused), (20, true));
    }

    #[test]
    fn the_stop_keys_and_the_play_keys_close_the_overlay() {
        for (tag, keys) in [
            ("fs-q", "q"),
            ("fs-esc", "<Esc>"),
            ("fs-gv", "gv"),
            ("fs-gV", "gV"),
        ] {
            let (mut app, _dir, pid) = playing_fullscreen(tag);
            press(&mut app, keys);
            assert_stopped(&mut app, pid, keys);
            // The room is as before: the cursor moves again.
            press(&mut app, "k");
            assert_eq!(cursor(&app), 0, "{keys}");
        }
    }

    #[test]
    fn a_seek_in_the_overlay_keeps_the_size_and_the_limit() {
        let (mut app, _dir, _pid) = playing_fullscreen("fs-seek");
        at(&mut app, 10);
        press(&mut app, "l");
        let runs = starts(&mut app);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].start_secs, 15);
        assert_eq!(runs[0].size, (568, 320));
        assert_eq!(runs[0].max_side, crate::video::FULLSCREEN_SIDE);
        assert!(app.video.as_ref().unwrap().fullscreen);
    }

    #[test]
    fn a_rebound_fullscreen_key_works_and_an_unbound_one_does_nothing() {
        let mut app = ready_app();
        app.keymap = crate::test_util::app_with_config(
            crate::test_util::config_from_keymap("[normal]\n\"F\" = \"play_video_fullscreen\"\n"),
            80,
            24,
        )
        .keymap;
        press(&mut app, "F");
        assert!(app.video.as_ref().is_some_and(|p| p.fullscreen));
        press(&mut app, "F");
        assert!(app.video.is_none(), "the rebound key closes it too");
        // Unbound: nothing opens, and an open overlay cannot be closed by it (the stop keys do).
        let mut app = ready_app();
        app.keymap = crate::test_util::app_with_config(
            crate::test_util::config_from_keymap("[normal]\n\"gV\" = \"none\"\n"),
            80,
            24,
        )
        .keymap;
        press(&mut app, "gV");
        assert!(app.video.is_none());
        press(&mut app, "gv");
        assert!(app.video.is_some());
        press(&mut app, "gV");
        assert!(app.video.is_some(), "an unbound key does nothing");
        press(&mut app, "q");
        assert!(app.video.is_none());
    }

    // ---- fullscreen: the terminal side -------------------------------------------------------

    fn overlay_cells(app: &App) -> (usize, usize) {
        crate::ui::image_view::placement(app, app.video.as_ref().unwrap().frame_size).unwrap()
    }

    fn fs_frame(app: &mut App, index: u64) {
        let generation = app.video.as_ref().unwrap().generation;
        update(
            app,
            Action::VideoFrame {
                generation,
                index,
                pixels: vec![index as u8; 568 * 320 * 4].into(),
            },
        );
    }

    #[test]
    fn a_fullscreen_frame_is_placed_in_the_overlays_cells_and_the_ids_alternate() {
        let (mut app, _dir, _pid) = playing_fullscreen("fs-place");
        let ids = app.video.as_ref().unwrap().ids;
        escapes(&mut app);
        let (cols, rows) = overlay_cells(&app);
        let mut used = Vec::new();
        for n in 0..4u64 {
            fs_frame(&mut app, n);
            let sent = escapes(&mut app);
            assert_eq!(sent.len(), 2, "{n}");
            let shown = app.video.as_ref().unwrap().shown.unwrap();
            used.push(shown);
            let id = ids[shown];
            assert!(sent[0].contains(&format!("i={id},")) && sent[0].contains("a=t"));
            assert!(sent[0].contains("s=568,v=320"), "{}", &sent[0][..50]);
            assert_eq!(sent[1], crate::kitty::place(id, cols, rows));
        }
        assert_eq!(used, [0, 1, 0, 1]);
        // The overlay is the whole screen: it is bigger than the inline box was.
        assert!(cols > 16 && rows >= 8, "{cols}x{rows}");
    }

    #[test]
    fn a_resize_while_paused_places_the_shown_picture_again_without_a_new_frame() {
        let (mut app, _dir, _pid) = playing_fullscreen("fs-resize-paused");
        escapes(&mut app);
        fs_frame(&mut app, 0);
        escapes(&mut app);
        press(&mut app, "<Space>");
        assert!(escapes(&mut app).is_empty(), "nothing changed");
        let before = overlay_cells(&app);
        app.size = ratatui::layout::Size::new(50, 14);
        let after = overlay_cells(&app);
        assert_ne!(before, after);
        let id = {
            let player = app.video.as_ref().unwrap();
            player.ids[player.shown.unwrap()]
        };
        // The poster in the room moves with the resize too; only the player's id is looked at.
        let ours = |sent: Vec<String>| -> Vec<String> {
            sent.into_iter()
                .filter(|c| c.contains(&format!("i={id},")))
                .collect()
        };
        assert_eq!(
            ours(escapes(&mut app)),
            [crate::kitty::place(id, after.0, after.1)]
        );
        assert!(ours(escapes(&mut app)).is_empty(), "once per change");
    }

    #[test]
    fn a_resize_while_playing_is_placed_with_the_next_frame() {
        let (mut app, _dir, _pid) = playing_fullscreen("fs-resize-playing");
        escapes(&mut app);
        fs_frame(&mut app, 0);
        escapes(&mut app);
        app.size = ratatui::layout::Size::new(50, 14);
        let after = overlay_cells(&app);
        fs_frame(&mut app, 1);
        let id = app.video.as_ref().unwrap().ids[1];
        let sent = escapes(&mut app);
        assert!(
            sent.contains(&crate::kitty::place(id, after.0, after.1)),
            "{sent:?}"
        );
    }

    #[test]
    fn closing_the_overlay_deletes_both_ids() {
        let (mut app, _dir, pid) = playing_fullscreen("fs-delete");
        let ids = app.video.as_ref().unwrap().ids;
        escapes(&mut app);
        fs_frame(&mut app, 0);
        escapes(&mut app);
        press(&mut app, "q");
        assert!(gone(pid));
        let out = escapes(&mut app);
        for id in ids {
            assert!(
                out.iter()
                    .any(|c| c.contains("a=d") && c.contains(&format!("i={id},"))),
                "{id}: {out:?}"
            );
        }
    }

    // ---- fullscreen: lifecycle ---------------------------------------------------------------

    #[test]
    fn the_overlay_closes_for_each_reason_the_inline_player_does() {
        let (mut app, _dir, pid) = playing_fullscreen("fs-room");
        let other = app.rooms[1].id.clone();
        app.open_room(other);
        tick(&mut app);
        assert_stopped(&mut app, pid, "room change");

        let (mut app, _dir, pid) = playing_fullscreen("fs-deleted");
        let room = app.current_room.clone().unwrap();
        update(
            &mut app,
            Action::RtDeleted {
                room,
                id: "m1".to_string(),
            },
        );
        assert_stopped(&mut app, pid, "message deleted");
        press(&mut app, "k");
        assert_eq!(cursor(&app), 0, "the keys are released");

        let (mut app, _dir, pid) = playing_fullscreen("fs-graphics");
        app.graphics = Graphics::Unsupported;
        tick(&mut app);
        assert_stopped(&mut app, pid, "graphics gone");

        let (mut app, _dir, pid) = playing_fullscreen("fs-setting");
        app.settings.inline_video = false;
        tick(&mut app);
        assert_stopped(&mut app, pid, "inline_video off");

        let (mut app, _dir, pid) = playing_fullscreen("fs-quit");
        update(&mut app, Action::Quit);
        assert_stopped(&mut app, pid, "quit");
    }

    #[test]
    fn the_overlay_stays_open_when_the_video_ends_and_a_seek_plays_again() {
        let (mut app, _dir, _pid) = playing_fullscreen("fs-ended");
        let generation = app.video.as_ref().unwrap().generation;
        update(&mut app, Action::VideoEnded { generation });
        let player = app.video.as_ref().expect("still open");
        assert!(player.fullscreen && player.ended);
        // A seek back starts a new run and clears the mark.
        at(&mut app, 40);
        press(&mut app, "h");
        let runs = starts(&mut app);
        assert_eq!(runs.len(), 1);
        assert!(!app.video.as_ref().unwrap().ended);
        // Ending again, then a stop key closes it.
        let generation = app.video.as_ref().unwrap().generation;
        update(&mut app, Action::VideoEnded { generation });
        assert!(app.video.is_some());
        press(&mut app, "q");
        assert!(app.video.is_none());
    }

    #[test]
    fn new_messages_do_not_close_the_overlay_however_far_they_push_the_video() {
        let (mut app, _dir, _pid) = playing_fullscreen("fs-pushed");
        let room = app.current_room.clone().unwrap();
        for n in 0..80 {
            let msg = Message {
                room_id: room.clone(),
                body: format!("later {n}"),
                ts: 1_000 + n,
                ..sample_message(&format!("later-{n}"))
            };
            update(&mut app, Action::RtMessage(msg));
        }
        assert!(app.video.as_ref().is_some_and(|p| p.fullscreen));
    }

    #[test]
    fn no_process_is_left_after_the_overlay_closes_or_the_app_is_dropped() {
        let (mut app, _dir, pid) = playing_fullscreen("fs-no-leftovers");
        press(&mut app, "gV");
        assert!(gone(pid), "after close");
        let (app, _dir, pid) = playing_fullscreen("fs-no-leftovers-drop");
        assert!(!gone(pid) || !cfg!(target_os = "linux"));
        drop(app);
        assert!(gone(pid), "after the app is dropped");
    }

    // ---- fullscreen: the popup ---------------------------------------------------------------

    fn fullscreen_row(app: &App) -> Option<(String, String)> {
        crate::app::context::lines(app)
            .rows
            .into_iter()
            .find(|r| r.desc == "watch the video fullscreen")
            .map(|r| (r.keys, r.desc))
    }

    #[test]
    fn the_popup_lists_watching_fullscreen_with_the_live_keys() {
        // Listed on a placeholder, a poster and a playing video.
        let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
        assert_eq!(fullscreen_row(&app).map(|r| r.0).as_deref(), Some("gV"));
        update(
            &mut app,
            Action::VideoPosterReady {
                video: video_ref(BIG),
                decoded: pixel(),
            },
        );
        assert!(fullscreen_row(&app).is_some());
        press(&mut app, "gv");
        assert!(fullscreen_row(&app).is_some());
        // Not on a message with no video, nor one whose poster failed.
        let app = room_app(vec![vec![]]);
        assert!(fullscreen_row(&app).is_none());
        let mut app = room_app(vec![vec![video_attachment(SMALL, Some(1000))]]);
        tick(&mut app);
        update(
            &mut app,
            Action::VideoPosterFailed {
                video: video_ref(SMALL),
                error: VideoError::CannotShow,
            },
        );
        assert!(fullscreen_row(&app).is_none());
    }

    #[test]
    fn the_popup_row_follows_a_rebound_or_unbound_fullscreen_key() {
        let row = |src: &str| {
            let mut app = room_app(vec![vec![video_attachment(BIG, Some(40_000_000))]]);
            app.keymap = crate::test_util::app_with_config(
                crate::test_util::config_from_keymap(src),
                80,
                24,
            )
            .keymap;
            fullscreen_row(&app).map(|r| r.0)
        };
        assert_eq!(
            row("[normal]\n\"F\" = \"play_video_fullscreen\"\n\"gV\" = \"none\"\n").as_deref(),
            Some("F")
        );
        assert_eq!(row("[normal]\n\"gV\" = \"none\"\n"), None);
    }
}

//! The images of message attachments, owned by the session: asked for when their message is drawn
//! in the open room, kept decoded in memory, and handed to the terminal by `tui`.
//!
//! Pure: no I/O. The request is an `Effect`, the answer an `Action`. The rules mirror the custom
//! emoji ones (`app::emoji`): once per path per session, a small concurrency limit, nothing unless
//! the terminal can show images. Nothing here formats a name, a path or a size.

use crate::api::MAX_ATTACHMENT_BYTES;
use crate::app::App;
use crate::app::emoji::Graphics;
use crate::data::{Attachment, AttachmentKind, MessageKind};
use crate::effect::{AttachmentRef, Effect, VideoRef};
use crate::emoji_image::Rgba;
use crate::kitty::{IMAGE_ID_FIRST, IMAGE_ID_LAST};
use std::collections::{HashMap, HashSet, VecDeque};

/// At most this many attachment fetches run at once; the rest wait in a queue.
pub const MAX_IN_FLIGHT: usize = 3;

/// Where the image of one attachment stands. An attachment not in the map is unknown, queued or
/// on its way.
#[derive(Clone, Debug, PartialEq)]
pub enum ImageState {
    /// Fetched and decoded for its inline box. `id` is the terminal image id it is transmitted as;
    /// `uploaded` is set once the terminal has the pixels, and only then are its cells drawn;
    /// `placed` is the cells (columns, rows) the terminal was last told it covers.
    Ready {
        image: Rgba,
        /// The pixel size the server reported, which the box was reserved from.
        dims: Option<(u32, u32)>,
        id: u32,
        uploaded: bool,
        placed: Option<(usize, usize)>,
    },
    /// Could not be fetched or decoded: final for the session, and the box goes away.
    Failed,
}

/// The attachment images of the session: what was asked for, what is on its way and what came
/// back. One entry per file path, never per message.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AttachmentImages {
    /// Every path already asked for, so one is asked for once per session.
    requested: HashSet<String>,
    /// Asked for and waiting for a free slot.
    queue: VecDeque<AttachmentRef>,
    /// Fetches running now.
    in_flight: usize,
    /// The outcome per path.
    state: HashMap<String, ImageState>,
    /// Posters of videos waiting for a free slot (they share `in_flight` with the images).
    video_queue: VecDeque<VideoRef>,
    /// The videos whose poster a `play_video` press asked for, so a failure can say so.
    video_explicit: HashSet<String>,
    /// Videos whose download a `play_video_fullscreen` press asked for: the room and message to
    /// open the overlay for once the poster is ready, by path.
    video_fullscreen: HashMap<String, (crate::data::RoomId, String)>,
    /// True once `ffmpeg` was not found: no poster is asked for by itself until one is made.
    video_missing: bool,
    /// The two terminal image ids the player alternates between, taken on first use and kept.
    video_ids: Option<[u32; 2]>,
    /// The number of the last playback run started: every run of every player has its own.
    video_runs: u64,
    /// The last terminal image id handed out; ids start at `IMAGE_ID_FIRST`.
    last_id: u32,
    /// The id the full-size view uses, taken on first use and kept: one image at a time is shown.
    view_id: Option<u32>,
    /// Terminal images to delete before the next frame (the view's, once it closed).
    pending_deletes: Vec<u32>,
}

impl AttachmentImages {
    /// The state of the image at `path`, `None` while it is unknown, queued or on its way.
    pub fn state(&self, path: &str) -> Option<&ImageState> {
        self.state.get(path)
    }

    /// Put `state` for `path`, for tests that draw with an image in a given state.
    #[cfg(test)]
    pub fn set_state(&mut self, path: &str, state: ImageState) {
        self.state.insert(path.to_string(), state);
    }

    /// True when `path` was already asked for this session.
    pub fn is_requested(&self, path: &str) -> bool {
        self.requested.contains(path)
    }

    /// True when `ffmpeg` was not found the last time it was needed.
    pub fn video_missing(&self) -> bool {
        self.video_missing
    }

    pub(crate) fn set_video_missing(&mut self, missing: bool) {
        self.video_missing = missing;
    }

    /// Queue the poster of `video` unless its path was asked for already. `explicit` marks a
    /// request that a key press made. True when it was queued.
    pub(crate) fn queue_video(&mut self, video: VideoRef, explicit: bool) -> bool {
        if !self.requested.insert(video.path.clone()) {
            return false;
        }
        if explicit {
            self.video_explicit.insert(video.path.clone());
            self.video_queue.push_front(video);
        } else {
            self.video_queue.push_back(video);
        }
        true
    }

    /// A poster request ended: free its slot. True when a key press had asked for it.
    pub(crate) fn video_poster_done(&mut self, path: &str) -> bool {
        self.in_flight = self.in_flight.saturating_sub(1);
        self.video_explicit.remove(path)
    }

    /// Remember that the poster of `path` was asked for to play it fullscreen in `message_id`.
    pub(crate) fn want_fullscreen(
        &mut self,
        path: &str,
        room: crate::data::RoomId,
        message_id: String,
    ) {
        self.video_fullscreen
            .insert(path.to_string(), (room, message_id));
    }

    /// Take the fullscreen intent of `path`, if a press left one.
    pub(crate) fn take_fullscreen(&mut self, path: &str) -> Option<(crate::data::RoomId, String)> {
        self.video_fullscreen.remove(path)
    }

    /// Forget that `path` was asked for, so it can be asked for again.
    pub(crate) fn forget_request(&mut self, path: &str) {
        self.requested.remove(path);
    }

    /// Keep `decoded` as the ready image of `path` (a still: one terminal image id). Failed when
    /// the id range is used up.
    pub(crate) fn store_ready(&mut self, path: String, decoded: Rgba, dims: Option<(u32, u32)>) {
        let state = match self.next_ids(decoded.frame_count()) {
            Some(id) => ImageState::Ready {
                image: decoded,
                dims,
                id,
                uploaded: false,
                placed: None,
            },
            None => ImageState::Failed,
        };
        self.state.insert(path, state);
    }

    pub(crate) fn store_failed(&mut self, path: String) {
        self.state.insert(path, ImageState::Failed);
    }

    /// The player's two terminal image ids, taken the first time they are asked for. `None` when
    /// the id range is used up.
    pub(crate) fn take_video_ids(&mut self) -> Option<[u32; 2]> {
        if self.video_ids.is_none() {
            let first = self.next_ids(2)?;
            self.video_ids = Some([first, first + 1]);
        }
        self.video_ids
    }

    /// A new run number, never used before.
    pub(crate) fn next_video_run(&mut self) -> u64 {
        self.video_runs += 1;
        self.video_runs
    }

    /// The number of fetches running now.
    pub fn in_flight(&self) -> usize {
        self.in_flight
    }

    /// The number of fetches waiting for a slot.
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// How many paths were asked for so far, for tests.
    #[cfg(test)]
    pub fn requested_count(&self) -> usize {
        self.requested.len()
    }

    /// The id of the full-size view's image, taken the first time it is asked for. `None` before
    /// the view was ever shown, or when the id range is used up.
    pub fn view_id(&self) -> Option<u32> {
        self.view_id
    }

    /// The view's first id, taken now if it was not yet. The view reserves a block of
    /// `MAX_FRAMES` ids, one per frame of the image it shows.
    pub(crate) fn take_view_id(&mut self) -> Option<u32> {
        if self.view_id.is_none() {
            self.view_id = self.next_ids(crate::emoji_image::MAX_FRAMES as u32);
        }
        self.view_id
    }

    /// Delete terminal image `id` before the next frame.
    pub(crate) fn queue_delete(&mut self, id: u32) {
        self.pending_deletes.push(id);
    }

    /// The next free terminal image id, `None` when the range is used up.
    #[cfg(test)]
    pub(crate) fn next_id(&mut self) -> Option<u32> {
        self.next_ids(1)
    }

    /// The first of `count` consecutive free terminal image ids (a frame is an image of its own),
    /// `None` when the range cannot hold them.
    pub(crate) fn next_ids(&mut self, count: u32) -> Option<u32> {
        let first = self.last_id.max(IMAGE_ID_FIRST - 1) + 1;
        let last = first.checked_add(count.max(1) - 1)?;
        (last <= IMAGE_ID_LAST).then(|| {
            self.last_id = last;
            first
        })
    }

    /// True when an animated image is among the cells drawn: `on_screen` is the ids the placeholder
    /// cells of the last frame carried (`ui::attachment::ids_on_screen`). Only then does the screen
    /// need redrawing on a timer. The view's frames count while the view is open.
    pub fn animating_among(&self, on_screen: &HashSet<u32>, view: Option<&Rgba>) -> bool {
        let shown = |first: u32, image: &Rgba| {
            image.is_animated()
                && (first..first + image.frame_count()).any(|id| on_screen.contains(&id))
        };
        view.zip(self.view_id).is_some_and(|(image, id)| shown(id, image))
            || self.state.values().any(|state| {
                matches!(state, ImageState::Ready { image, id, uploaded: true, .. } if shown(*id, image))
            })
    }
}

/// True when `attachment` is an image the client may fetch and draw inline: an image kind with a
/// plain `/file-upload/` path and a reported size, if any, within the download cap. Anything else
/// is drawn as its caption only.
pub fn is_fetchable_image(attachment: &Attachment) -> bool {
    attachment.kind == AttachmentKind::Image
        && attachment.path.is_some()
        // With a thumbnail under `path` the reported size is the original's, not the thumbnail's.
        && (attachment.full_path.is_some()
            || attachment
                .size
                .is_none_or(|size| size <= MAX_ATTACHMENT_BYTES as u64))
}

/// The request for `attachment`, `None` when it is not a fetchable image.
fn reference(attachment: &Attachment, animate: bool) -> Option<AttachmentRef> {
    if !is_fetchable_image(attachment) {
        return None;
    }
    Some(AttachmentRef {
        path: attachment.path.clone()?,
        size: attachment.size,
        dims: attachment.dims,
        animate,
        source: animated_original(attachment, animate),
    })
}

/// The original file of a gif or webp whose inline path is a still thumbnail, when animation is on
/// and the original is within the inline download cap: the thumbnail would never play.
fn animated_original(attachment: &Attachment, animate: bool) -> Option<String> {
    let animated_type = matches!(attachment.mime.as_deref(), Some("image/gif" | "image/webp"));
    (animate
        && animated_type
        && attachment
            .size
            .is_none_or(|size| size <= MAX_ATTACHMENT_BYTES as u64))
    .then(|| attachment.full_path.clone())?
}

/// Ask for the images of the attachments the open room draws and that were not asked for before.
/// Nothing is asked for unless the terminal can show images and `inline_images` is on. Each path is
/// asked for once per session, at most `MAX_IN_FLIGHT` run at a time, and the rest wait in order.
pub fn request_images(app: &mut App) {
    crate::app::video::request_posters(app);
    if app.graphics != Graphics::Supported || !app.settings.inline_images {
        pump(app);
        return;
    }
    let Some(room) = app.current_room.as_ref() else {
        pump(app);
        return;
    };
    let animate = app.settings.animate_images;
    let mut wanted: Vec<AttachmentRef> = Vec::new();
    for message in app.messages.get(room).map_or(&[][..], Vec::as_slice) {
        if message.kind != MessageKind::Normal || !message.in_main_flow() {
            continue;
        }
        wanted.extend(
            message
                .attachments
                .iter()
                .filter_map(|a| reference(a, animate)),
        );
    }
    for image in wanted {
        if app.attachment_images.requested.insert(image.path.clone()) {
            app.attachment_images.queue.push_back(image);
        }
    }
    pump(app);
}

/// Start as many queued fetches as there are free slots: the images first, then the video posters
/// (which a key press put in front of the other posters).
pub(crate) fn pump(app: &mut App) {
    while app.attachment_images.in_flight < MAX_IN_FLIGHT {
        if let Some(next) = app.attachment_images.queue.pop_front() {
            app.attachment_images.in_flight += 1;
            app.effects.push(Effect::FetchAttachmentImage(next));
        } else if let Some(next) = app.attachment_images.video_queue.pop_front() {
            app.attachment_images.in_flight += 1;
            app.effects.push(Effect::LoadVideoPoster(next));
        } else {
            break;
        }
    }
}

/// How many attachment images may sit in the terminal at once. Past it, the lowest-numbered ones
/// that are not near the viewport are deleted there (their pixels stay in memory and go up again
/// when the message comes back into view).
pub const MAX_UPLOADED: usize = 24;

/// The terminal side of the attachment images, to be written before the next frame is drawn: the
/// escape sequences that transmit the images near the viewport, place each in the cells its box
/// draws, and delete the oldest ones far from it past `MAX_UPLOADED`. Marks every image it
/// transmits as uploaded, so the frame draws its cells, and remembers the cells it placed so a
/// changed box (a resize) places again. Empty unless images are drawn at all. Nothing is drawn
/// from here: the escape sequences are the caller's to write.
pub fn prepare_frame(app: &mut App) -> Vec<String> {
    // Images to delete go out first, whatever else is true: a stopped video frees its two.
    let mut out: Vec<String> = std::mem::take(&mut app.attachment_images.pending_deletes)
        .into_iter()
        .map(crate::kitty::delete)
        .collect();
    if app.graphics != Graphics::Supported
        || !(app.settings.inline_images || app.settings.inline_video)
    {
        return out;
    }
    let Some(room) = app.current_room.clone() else {
        return out;
    };
    let wanted = wanted_placements(app, &room);
    let view_cells = app.image_view.as_ref().and_then(|view| {
        let pixels = view.pixels.as_ref()?;
        crate::ui::image_view::placement(app, (pixels.image.width, pixels.image.height))
    });
    // The newest frame of a playing video goes up before the cells that show it.
    crate::app::video::prepare_player(app, &mut out);
    let images = &mut app.attachment_images;
    // The full-size view: its own id, transmitted once per image and placed in the overlay.
    if let (Some(view), Some(cells)) = (app.image_view.as_mut(), view_cells)
        && let Some(pixels) = view.pixels.as_mut()
        && let Some(id) = images.take_view_id()
    {
        if !pixels.uploaded {
            transmit_frames(&mut out, id, &pixels.image);
            pixels.uploaded = true;
            pixels.placed = None;
        }
        if pixels.placed != Some(cells) {
            place_frames(&mut out, id, &pixels.image, cells);
            pixels.placed = Some(cells);
        }
    }
    for (path, cells) in &wanted {
        let Some(ImageState::Ready {
            image,
            id,
            uploaded,
            placed,
            ..
        }) = images.state.get_mut(path)
        else {
            continue;
        };
        if !*uploaded {
            transmit_frames(&mut out, *id, image);
            *uploaded = true;
            *placed = None;
        }
        if *placed != Some(*cells) {
            place_frames(&mut out, *id, image, *cells);
            *placed = Some(*cells);
        }
    }
    let near: HashSet<&String> = wanted.iter().map(|(path, _)| path).collect();
    // An animation that is no longer near the viewport gives up all its frames at once: they are
    // many, and nothing is drawn from them.
    for (path, state) in images.state.iter_mut() {
        if let ImageState::Ready {
            image,
            id,
            uploaded,
            placed,
            ..
        } = state
            && *uploaded
            && image.is_animated()
            && !near.contains(path)
        {
            delete_frames(&mut out, *id, image);
            *uploaded = false;
            *placed = None;
        }
    }
    // Past the bound, free the oldest stills that are not near the viewport.
    let mut uploaded: Vec<(u32, &String)> = images
        .state
        .iter()
        .filter_map(|(path, state)| match state {
            ImageState::Ready {
                id,
                image,
                uploaded: true,
                ..
            } if !image.is_animated() => Some((*id, path)),
            _ => None,
        })
        .collect();
    if uploaded.len() > MAX_UPLOADED {
        uploaded.sort_unstable();
        let excess = uploaded.len() - MAX_UPLOADED;
        let doomed: Vec<String> = uploaded
            .into_iter()
            .filter(|(_, path)| !near.contains(path))
            .take(excess)
            .map(|(_, path)| path.clone())
            .collect();
        for path in doomed {
            if let Some(ImageState::Ready {
                id,
                uploaded,
                placed,
                ..
            }) = images.state.get_mut(&path)
            {
                out.push(crate::kitty::delete(*id));
                *uploaded = false;
                *placed = None;
            }
        }
    }
    out
}

/// Transmit every frame of `image` as the images `first_id`, `first_id + 1`, and so on.
pub(crate) fn transmit_frames(out: &mut Vec<String>, first_id: u32, image: &Rgba) {
    for n in 0..image.frame_count() {
        if let Some(pixels) = image.frame_pixels(n) {
            out.push(crate::kitty::transmit(
                first_id + n,
                image.width,
                image.height,
                pixels,
            ));
        }
    }
}

/// Give every frame of `image` the same placement of `cells` (columns, rows).
pub(crate) fn place_frames(
    out: &mut Vec<String>,
    first_id: u32,
    image: &Rgba,
    cells: (usize, usize),
) {
    for n in 0..image.frame_count() {
        out.push(crate::kitty::place(first_id + n, cells.0, cells.1));
    }
}

/// Delete every frame of `image` from the terminal.
pub(crate) fn delete_frames(out: &mut Vec<String>, first_id: u32, image: &Rgba) {
    for n in 0..image.frame_count() {
        out.push(crate::kitty::delete(first_id + n));
    }
}

/// The ready images of the messages within a screen of the viewport of `room`, each with the cells
/// (columns, rows) its box draws them in, first use of a path winning. The box is worked out the
/// way the rows are (`wrap::body_width`, `attachment::box_size`, `attachment::fitted`), so the
/// placement and the drawn cells always agree.
fn wanted_placements(app: &App, room: &crate::data::RoomId) -> Vec<(String, (usize, usize))> {
    let msgs = app.messages.get(room).map_or(&[][..], Vec::as_slice);
    if msgs.iter().all(|m| m.attachments.is_empty()) {
        return Vec::new();
    }
    let (width, height) = crate::app::messages::viewport(app);
    let ctx = app.pane_row_ctx(room, msgs);
    let starts = crate::app::messages::row_starts_of(msgs, width, &ctx);
    let top = crate::app::messages::view(app, room).top_row;
    let height = usize::from(height);
    let (from, to) = (top.saturating_sub(height), top + 2 * height);
    let mut out: Vec<(String, (usize, usize))> = Vec::new();
    for (i, msg) in msgs.iter().enumerate() {
        if starts[i + 1] <= from || starts[i] >= to || msg.kind != MessageKind::Normal {
            continue;
        }
        let body = crate::ui::wrap::body_width(msg, usize::from(width), &ctx);
        let images_on = app.settings.inline_images;
        let video_on = app.settings.inline_video;
        for attachment in msg.attachments.iter().filter(|a| {
            (images_on && is_fetchable_image(a))
                || (video_on && crate::app::video::is_fetchable_video(a))
        }) {
            let Some(path) = attachment.path.as_ref() else {
                continue;
            };
            let Some(ImageState::Ready { image, .. }) = app.attachment_images.state(path) else {
                continue;
            };
            if out.iter().any(|(p, _)| p == path) {
                continue;
            }
            let boxed =
                crate::ui::attachment::box_of(attachment, body, app.settings.image_max_rows);
            if boxed.0 == 0 {
                continue;
            }
            let cells = crate::ui::attachment::fitted(boxed, (image.width, image.height));
            out.push((path.clone(), cells));
        }
    }
    out
}

/// An image arrived, decoded.
pub fn on_image_ready(app: &mut App, image: AttachmentRef, decoded: Rgba) {
    let images = &mut app.attachment_images;
    images.in_flight = images.in_flight.saturating_sub(1);
    let state = match images.next_ids(decoded.frame_count()) {
        Some(id) => ImageState::Ready {
            image: decoded,
            dims: image.dims,
            id,
            uploaded: false,
            placed: None,
        },
        None => ImageState::Failed,
    };
    images.state.insert(image.path, state);
    pump(app);
}

/// An image could not be fetched or decoded: final for the session, so its box is removed and
/// only the caption stays.
pub fn on_image_failed(app: &mut App, image: AttachmentRef) {
    let images = &mut app.attachment_images;
    images.in_flight = images.in_flight.saturating_sub(1);
    images.state.insert(image.path, ImageState::Failed);
    pump(app);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::update::update;
    use crate::data::Message;
    use crate::test_util::{app_80x24, sample_message};

    pub(crate) fn image_attachment(path: &str) -> Attachment {
        Attachment {
            kind: AttachmentKind::Image,
            name: "photo.png".to_string(),
            path: Some(path.to_string()),
            full_path: None,
            mime: Some("image/png".to_string()),
            size: Some(1000),
            dims: Some((800, 600)),
        }
    }

    /// An app on its first room, with images allowed, holding one message per list of attachments.
    fn room_app(messages: Vec<Vec<Attachment>>) -> App {
        let mut app = app_80x24();
        app.graphics = Graphics::Supported;
        let room = app.rooms[0].id.clone();
        let msgs = messages
            .into_iter()
            .enumerate()
            .map(|(i, attachments)| Message {
                room_id: room.clone(),
                attachments,
                ..sample_message(&format!("m{i}"))
            })
            .collect();
        app.messages.insert(room.clone(), msgs);
        app.current_room = Some(room);
        app.effects.clear();
        app
    }

    fn fetches(app: &App) -> Vec<String> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::FetchAttachmentImage(r) => Some(r.path.clone()),
                _ => None,
            })
            .collect()
    }

    fn tick(app: &mut App) {
        update(app, Action::Clock(0));
    }

    fn done(path: &str) -> AttachmentRef {
        AttachmentRef {
            path: path.to_string(),
            size: Some(1000),
            dims: Some((800, 600)),
            animate: false,
            source: None,
        }
    }

    fn pixel() -> Rgba {
        Rgba {
            width: 1,
            height: 1,
            pixels: vec![1, 2, 3, 255].into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_drawn_image_is_asked_for_once_per_path() {
        let mut app = room_app(vec![
            vec![image_attachment("/file-upload/a/one.png")],
            vec![image_attachment("/file-upload/b/two.png")],
            // The same file shown twice is still one request.
            vec![image_attachment("/file-upload/a/one.png")],
        ]);
        tick(&mut app);
        assert_eq!(
            fetches(&app),
            ["/file-upload/a/one.png", "/file-upload/b/two.png"]
        );
        app.effects.clear();
        tick(&mut app);
        tick(&mut app);
        assert!(fetches(&app).is_empty(), "nothing is asked for twice");
        assert_eq!(app.attachment_images.requested_count(), 2);
    }

    #[test]
    fn nothing_is_asked_for_on_a_terminal_without_images() {
        let mut app = room_app(vec![vec![image_attachment("/file-upload/a/one.png")]]);
        app.graphics = Graphics::Unsupported;
        tick(&mut app);
        assert!(fetches(&app).is_empty());
        assert_eq!(app.attachment_images.requested_count(), 0);
    }

    #[test]
    fn nothing_is_asked_for_when_inline_images_is_off() {
        let mut app = room_app(vec![vec![image_attachment("/file-upload/a/one.png")]]);
        app.settings.inline_images = false;
        tick(&mut app);
        assert!(fetches(&app).is_empty());
        // Turning it back on asks, as the setting is only read, not remembered.
        app.settings.inline_images = true;
        tick(&mut app);
        assert_eq!(fetches(&app).len(), 1);
    }

    #[test]
    fn only_images_with_a_fetchable_path_are_asked_for() {
        let mut other = image_attachment("/file-upload/a/x.pdf");
        other.kind = AttachmentKind::Other;
        let mut video = image_attachment("/file-upload/a/v.mp4");
        video.kind = AttachmentKind::Video;
        let mut no_path = image_attachment("/x");
        no_path.path = None;
        let mut app = room_app(vec![vec![other, video, no_path]]);
        tick(&mut app);
        assert!(fetches(&app).is_empty());
    }

    #[test]
    fn a_reported_size_over_the_cap_is_never_requested() {
        let mut big = image_attachment("/file-upload/a/big.png");
        big.size = Some(MAX_ATTACHMENT_BYTES as u64 + 1);
        let mut exact = image_attachment("/file-upload/a/exact.png");
        exact.size = Some(MAX_ATTACHMENT_BYTES as u64);
        let mut unknown = image_attachment("/file-upload/a/unknown.png");
        unknown.size = None;
        assert!(!is_fetchable_image(&big));
        let mut app = room_app(vec![vec![big, exact, unknown]]);
        tick(&mut app);
        assert_eq!(
            fetches(&app),
            ["/file-upload/a/exact.png", "/file-upload/a/unknown.png"]
        );
    }

    #[test]
    fn a_big_original_does_not_block_its_thumbnail() {
        let mut thumb = image_attachment("/file-upload/t/thumb-x.png");
        thumb.size = Some(MAX_ATTACHMENT_BYTES as u64 * 4);
        thumb.full_path = Some("/file-upload/o/x.png".to_string());
        assert!(is_fetchable_image(&thumb));
    }

    #[test]
    fn only_the_open_room_is_asked_for() {
        let mut app = room_app(vec![vec![image_attachment("/file-upload/a/one.png")]]);
        let other = app.rooms[1].id.clone();
        let msg = Message {
            room_id: other.clone(),
            attachments: vec![image_attachment("/file-upload/z/other.png")],
            ..sample_message("o1")
        };
        app.messages.insert(other, vec![msg]);
        tick(&mut app);
        assert_eq!(fetches(&app), ["/file-upload/a/one.png"]);
        // No room open: nothing.
        let mut app = room_app(vec![vec![image_attachment("/file-upload/a/one.png")]]);
        app.current_room = None;
        tick(&mut app);
        assert!(fetches(&app).is_empty());
    }

    #[test]
    fn a_thread_reply_not_in_the_main_flow_is_not_asked_for() {
        let mut app = room_app(vec![vec![image_attachment("/file-upload/a/one.png")]]);
        let room = app.current_room.clone().expect("room");
        let reply = app.messages.get_mut(&room).expect("messages");
        reply[0].tmid = Some("parent".to_string());
        tick(&mut app);
        assert!(fetches(&app).is_empty());
    }

    #[test]
    fn at_most_the_limit_run_at_once_and_none_is_dropped() {
        let paths: Vec<String> = (0..MAX_IN_FLIGHT + 3)
            .map(|i| format!("/file-upload/{i}/p.png"))
            .collect();
        let mut app = room_app(vec![paths.iter().map(|p| image_attachment(p)).collect()]);
        tick(&mut app);
        assert_eq!(fetches(&app).len(), MAX_IN_FLIGHT);
        assert_eq!(app.attachment_images.in_flight(), MAX_IN_FLIGHT);
        assert_eq!(app.attachment_images.queued(), 3);
        // Each answer frees a slot that starts the next one, in order, until all were asked.
        app.effects.clear();
        for (i, path) in paths.iter().enumerate() {
            let answer = if i % 2 == 0 {
                Action::AttachmentImageReady {
                    image: done(path),
                    decoded: pixel(),
                }
            } else {
                Action::AttachmentImageFailed(done(path))
            };
            update(&mut app, answer);
        }
        let asked: Vec<String> = paths[MAX_IN_FLIGHT..].to_vec();
        assert_eq!(fetches(&app), asked);
        assert_eq!(app.attachment_images.queued(), 0);
    }

    #[test]
    fn a_ready_image_is_kept_with_its_own_id_and_is_not_uploaded_yet() {
        let mut app = room_app(vec![vec![
            image_attachment("/file-upload/a/one.png"),
            image_attachment("/file-upload/b/two.png"),
        ]]);
        tick(&mut app);
        for path in ["/file-upload/a/one.png", "/file-upload/b/two.png"] {
            update(
                &mut app,
                Action::AttachmentImageReady {
                    image: done(path),
                    decoded: pixel(),
                },
            );
        }
        let id_of = |app: &App, path: &str| match app.attachment_images.state(path) {
            Some(ImageState::Ready { id, uploaded, .. }) => {
                assert!(!uploaded, "the terminal does not have it yet");
                *id
            }
            other => panic!("expected a ready image, got {other:?}"),
        };
        let (a, b) = (
            id_of(&app, "/file-upload/a/one.png"),
            id_of(&app, "/file-upload/b/two.png"),
        );
        assert_ne!(a, b);
        assert!(a >= IMAGE_ID_FIRST && b >= IMAGE_ID_FIRST);
        assert!(a > crate::app::emoji::MAX_IMAGE_ID);
        assert_eq!(app.attachment_images.in_flight(), 0);
    }

    #[test]
    fn a_failed_image_is_final_and_is_not_asked_for_again() {
        let mut app = room_app(vec![vec![image_attachment("/file-upload/a/one.png")]]);
        tick(&mut app);
        update(
            &mut app,
            Action::AttachmentImageFailed(done("/file-upload/a/one.png")),
        );
        assert_eq!(
            app.attachment_images.state("/file-upload/a/one.png"),
            Some(&ImageState::Failed)
        );
        app.effects.clear();
        tick(&mut app);
        assert!(fetches(&app).is_empty());
        assert!(app.status_msg.is_none(), "a failed image shows nothing");
    }

    #[test]
    fn running_out_of_ids_fails_the_image_instead_of_reusing_one() {
        let mut images = AttachmentImages {
            last_id: IMAGE_ID_LAST,
            ..Default::default()
        };
        assert_eq!(images.next_id(), None);
        images.last_id = IMAGE_ID_LAST - 1;
        assert_eq!(images.next_id(), Some(IMAGE_ID_LAST));
        assert_eq!(images.next_id(), None);
    }

    // ---- the terminal side

    fn ready_in(app: &mut App, path: &str, w: u32, h: u32) {
        let decoded = Rgba {
            width: w,
            height: h,
            pixels: vec![5u8; (w * h * 4) as usize].into(),
            ..Default::default()
        };
        update(
            app,
            Action::AttachmentImageReady {
                image: done(path),
                decoded,
            },
        );
    }

    fn count(commands: &[String], what: &str) -> usize {
        commands.iter().filter(|c| c.contains(what)).count()
    }

    #[test]
    fn a_ready_image_is_transmitted_once_and_placed_in_its_box() {
        let path = "/file-upload/a/one.png";
        let mut app = room_app(vec![vec![image_attachment(path)]]);
        tick(&mut app);
        assert!(prepare_frame(&mut app).is_empty(), "nothing is ready yet");
        ready_in(&mut app, path, 640, 480);
        let commands = prepare_frame(&mut app);
        assert_eq!(count(&commands, "a=t,"), 1, "{commands:?}");
        assert_eq!(count(&commands, "a=p,"), 1);
        // The transmit comes first, then the placement of the cells the box draws.
        assert!(commands[0].contains("a=t,") && commands[1].contains("a=p,"));
        let Some(ImageState::Ready {
            id,
            uploaded,
            placed,
            ..
        }) = app.attachment_images.state(path).cloned()
        else {
            panic!("ready");
        };
        assert!(uploaded);
        let (cols, rows) = placed.expect("placed");
        assert!(
            commands[1].contains(&format!("i={id},c={cols},r={rows}")),
            "{}",
            commands[1]
        );
        // Nothing is sent again while nothing changed.
        assert!(prepare_frame(&mut app).is_empty());
    }

    #[test]
    fn a_resize_places_the_image_again_without_transmitting_it() {
        let path = "/file-upload/a/one.png";
        let mut app = room_app(vec![vec![image_attachment(path)]]);
        tick(&mut app);
        ready_in(&mut app, path, 640, 480);
        prepare_frame(&mut app);
        let placed = |app: &App| match app.attachment_images.state(path) {
            Some(ImageState::Ready { placed, .. }) => *placed,
            _ => None,
        };
        let before = placed(&app);
        update(&mut app, Action::Resize(50, 24));
        let commands = prepare_frame(&mut app);
        assert_eq!(count(&commands, "a=t,"), 0, "the pixels are already there");
        assert_eq!(count(&commands, "a=p,"), 1);
        assert_ne!(placed(&app), before, "a narrower pane, a narrower box");
    }

    #[test]
    fn the_placement_is_the_box_the_rows_drew() {
        // Same cells as the drawing: compare with what the pane draws.
        let path = "/file-upload/a/one.png";
        let mut app = room_app(vec![vec![image_attachment(path)]]);
        tick(&mut app);
        ready_in(&mut app, path, 640, 480);
        prepare_frame(&mut app);
        let Some(ImageState::Ready {
            placed: Some((cols, rows)),
            ..
        }) = app.attachment_images.state(path).cloned()
        else {
            panic!("placed");
        };
        let buffer = crate::test_util::render_buffer(&app, 80, 24);
        let drawn = buffer
            .content()
            .iter()
            .filter(|c| c.symbol().starts_with(crate::kitty::PLACEHOLDER))
            .count();
        assert_eq!(drawn, cols * rows);
    }

    #[test]
    fn nothing_is_sent_without_graphics_or_with_the_setting_off() {
        let path = "/file-upload/a/one.png";
        for tweak in [0, 1] {
            let mut app = room_app(vec![vec![image_attachment(path)]]);
            tick(&mut app);
            ready_in(&mut app, path, 64, 48);
            if tweak == 0 {
                app.graphics = Graphics::Unsupported;
            } else {
                app.settings.inline_images = false;
            }
            assert!(prepare_frame(&mut app).is_empty());
        }
    }

    /// An app whose room holds `n` small images, one per message, all ready.
    fn many(n: usize) -> (App, Vec<String>) {
        let paths: Vec<String> = (0..n).map(|i| format!("/file-upload/{i}/p.png")).collect();
        let mut app = room_app(paths.iter().map(|p| vec![image_attachment(p)]).collect());
        // Small images so each message is a few rows and many fit in a few screens.
        for m in app.messages.values_mut().flatten() {
            for a in &mut m.attachments {
                a.dims = Some((16, 16));
            }
        }
        tick(&mut app);
        for p in &paths {
            ready_in(&mut app, p, 16, 16);
        }
        (app, paths)
    }

    fn uploaded_count(app: &App, paths: &[String]) -> usize {
        paths
            .iter()
            .filter(|p| {
                matches!(
                    app.attachment_images.state(p),
                    Some(ImageState::Ready { uploaded: true, .. })
                )
            })
            .count()
    }

    #[test]
    fn only_images_near_the_viewport_go_up() {
        let (mut app, paths) = many(60);
        let commands = prepare_frame(&mut app);
        let sent = count(&commands, "a=t,");
        assert!(sent > 0 && sent < paths.len(), "{sent} of {}", paths.len());
        assert_eq!(uploaded_count(&app, &paths), sent);
        // The newest message is in view (the pane starts at the bottom).
        assert!(matches!(
            app.attachment_images.state(paths.last().unwrap()),
            Some(ImageState::Ready { uploaded: true, .. })
        ));
        assert!(matches!(
            app.attachment_images.state(&paths[0]),
            Some(ImageState::Ready {
                uploaded: false,
                ..
            })
        ));
    }

    #[test]
    fn past_the_bound_the_oldest_far_images_are_deleted_and_the_near_ones_stay() {
        let (mut app, paths) = many(80);
        prepare_frame(&mut app);
        // Scroll up through the room a screen at a time: uploads grow, then the bound bites.
        let room = app.current_room.clone().unwrap();
        let mut deleted_any = false;
        for step in (0..80).rev().step_by(6) {
            app.msg_views.insert(
                room.clone(),
                crate::app::messages::MsgView {
                    cursor: step,
                    top_row: 0,
                },
            );
            // Let the view settle on the cursor.
            let view = crate::app::messages::view(&app, &room);
            app.msg_views.insert(room.clone(), view);
            let commands = prepare_frame(&mut app);
            // A delete frees one image by id, never all of them.
            assert_eq!(count(&commands, "d=A"), 0);
            deleted_any |= count(&commands, "a=d,") > 0;
            assert!(
                uploaded_count(&app, &paths) <= MAX_UPLOADED.max(near_count(&app, &room)),
                "step {step}"
            );
            // Every image the viewport shows is up.
            for p in near_paths(&app, &room) {
                assert!(
                    matches!(
                        app.attachment_images.state(&p),
                        Some(ImageState::Ready { uploaded: true, .. })
                    ),
                    "{step}"
                );
            }
        }
        assert!(
            deleted_any,
            "the bound was reached and something was deleted"
        );
    }

    fn near_paths(app: &App, room: &crate::data::RoomId) -> Vec<String> {
        wanted_placements(app, room)
            .into_iter()
            .map(|(p, _)| p)
            .collect()
    }

    fn near_count(app: &App, room: &crate::data::RoomId) -> usize {
        near_paths(app, room).len()
    }

    #[test]
    fn a_deleted_image_is_not_sent_again_until_it_is_near_again() {
        let (mut app, paths) = many(80);
        prepare_frame(&mut app);
        let room = app.current_room.clone().unwrap();
        // Walk to the top so the early ones go up and the late ones are freed.
        for step in (0..80).rev().step_by(4) {
            app.msg_views.insert(
                room.clone(),
                crate::app::messages::MsgView {
                    cursor: step,
                    top_row: 0,
                },
            );
            let view = crate::app::messages::view(&app, &room);
            app.msg_views.insert(room.clone(), view);
            prepare_frame(&mut app);
        }
        // Nothing changes on the next call at rest: no transmit, no delete, no place.
        assert!(prepare_frame(&mut app).is_empty());
        assert!(uploaded_count(&app, &paths) <= MAX_UPLOADED + near_count(&app, &room));
        // A freed image keeps its pixels and goes up again when it comes back.
        let freed = paths
            .iter()
            .find(|p| {
                matches!(
                    app.attachment_images.state(p),
                    Some(ImageState::Ready { uploaded: false, image, .. }) if image.width == 16
                )
            })
            .cloned()
            .expect("a freed image");
        let index = paths.iter().position(|p| p == &freed).unwrap();
        app.msg_views.insert(
            room.clone(),
            crate::app::messages::MsgView {
                cursor: index,
                top_row: 0,
            },
        );
        let view = crate::app::messages::view(&app, &room);
        app.msg_views.insert(room.clone(), view);
        let commands = prepare_frame(&mut app);
        assert!(count(&commands, "a=t,") >= 1, "{}", commands.len());
        assert!(matches!(
            app.attachment_images.state(&freed),
            Some(ImageState::Ready { uploaded: true, .. })
        ));
    }

    // ---- animation

    /// An animated image of `frames` frames, each 100 ms, `w` by `h` pixels.
    pub(crate) fn animated(w: u32, h: u32, frames: usize) -> Rgba {
        let frame = |n: usize| crate::emoji_image::Frame {
            pixels: vec![n as u8; (w * h * 4) as usize].into(),
            delay_ms: 100,
        };
        Rgba {
            width: w,
            height: h,
            pixels: vec![0u8; (w * h * 4) as usize].into(),
            delay_ms: 100,
            later: (1..frames).map(frame).collect::<Vec<_>>().into(),
        }
    }

    fn ready_animated(app: &mut App, path: &str, frames: usize) {
        update(
            app,
            Action::AttachmentImageReady {
                image: done(path),
                decoded: animated(16, 16, frames),
            },
        );
    }

    fn first_id(app: &App, path: &str) -> u32 {
        match app.attachment_images.state(path) {
            Some(ImageState::Ready { id, .. }) => *id,
            other => panic!("expected a ready image, got {other:?}"),
        }
    }

    #[test]
    fn an_animated_image_takes_an_id_for_each_frame() {
        let (a, b, c) = (
            "/file-upload/a/one.gif",
            "/file-upload/b/two.png",
            "/file-upload/c/three.gif",
        );
        let mut app = room_app(vec![vec![
            image_attachment(a),
            image_attachment(b),
            image_attachment(c),
        ]]);
        tick(&mut app);
        ready_animated(&mut app, a, 5);
        ready_in(&mut app, b, 16, 16);
        ready_animated(&mut app, c, 2);
        let (ia, ib, ic) = (first_id(&app, a), first_id(&app, b), first_id(&app, c));
        assert_eq!(ib, ia + 5, "five frames, five ids");
        assert_eq!(ic, ib + 1, "a still takes one");
        assert!(ia >= IMAGE_ID_FIRST);
    }

    #[test]
    fn an_animation_that_does_not_fit_the_id_range_fails() {
        let mut images = AttachmentImages {
            last_id: IMAGE_ID_LAST - 2,
            ..Default::default()
        };
        assert_eq!(images.next_ids(4), None);
        assert_eq!(images.next_ids(2), Some(IMAGE_ID_LAST - 1));
    }

    #[test]
    fn a_fetch_carries_the_animation_setting() {
        let animate_of = |on: bool| {
            let mut app = room_app(vec![vec![image_attachment("/file-upload/a/one.gif")]]);
            app.settings.animate_images = on;
            tick(&mut app);
            app.effects
                .iter()
                .find_map(|e| match e {
                    Effect::FetchAttachmentImage(r) => Some(r.animate),
                    _ => None,
                })
                .expect("a fetch")
        };
        assert!(animate_of(true));
        assert!(!animate_of(false));
    }

    #[test]
    fn an_animated_thumbnail_is_fetched_from_the_original_only_when_animation_is_on() {
        let source_of = |mime: &str, animate: bool, size: u64| {
            let mut a = image_attachment("/file-upload/id/thumb-x.gif");
            a.full_path = Some("/file-upload/id/x.gif".to_string());
            a.mime = Some(mime.to_string());
            a.size = Some(size);
            let mut app = room_app(vec![vec![a]]);
            app.settings.animate_images = animate;
            tick(&mut app);
            app.effects.iter().find_map(|e| match e {
                Effect::FetchAttachmentImage(r) => Some(r.source.clone()),
                _ => None,
            })
        };
        let original = Some(Some("/file-upload/id/x.gif".to_string()));
        assert_eq!(source_of("image/gif", true, 1000), original);
        assert_eq!(source_of("image/webp", true, 1000), original);
        assert_eq!(source_of("image/gif", false, 1000), Some(None));
        assert_eq!(source_of("image/png", true, 1000), Some(None));
        // An original over the inline cap stays a thumbnail.
        let big = MAX_ATTACHMENT_BYTES as u64 + 1;
        assert_eq!(source_of("image/gif", true, big), Some(None));
    }

    #[test]
    fn every_frame_of_an_animation_is_transmitted_and_placed_once() {
        let path = "/file-upload/a/one.gif";
        let mut app = room_app(vec![vec![image_attachment(path)]]);
        tick(&mut app);
        ready_animated(&mut app, path, 4);
        let id = first_id(&app, path);
        let commands = prepare_frame(&mut app);
        assert_eq!(count(&commands, "a=t,"), 4, "{}", commands.len());
        assert_eq!(count(&commands, "a=p,"), 4);
        for n in 0..4 {
            assert!(
                commands
                    .iter()
                    .any(|c| c.contains("a=t,") && c.contains(&format!("i={}", id + n))),
                "frame {n}"
            );
            assert!(
                commands
                    .iter()
                    .any(|c| c.contains("a=p,") && c.contains(&format!("i={},", id + n))),
                "frame {n}"
            );
        }
        // Nothing again while nothing changed; a resize places every frame, transmits none.
        assert!(prepare_frame(&mut app).is_empty());
        update(&mut app, Action::Resize(50, 24));
        let commands = prepare_frame(&mut app);
        assert_eq!(count(&commands, "a=t,"), 0);
        assert_eq!(count(&commands, "a=p,"), 4);
    }

    /// An app whose room holds an animated image first, then `n` small still ones.
    fn gif_then_stills(n: usize) -> (App, String) {
        let gif = "/file-upload/g/anim.gif".to_string();
        let mut paths = vec![gif.clone()];
        paths.extend((0..n).map(|i| format!("/file-upload/{i}/p.png")));
        let mut app = room_app(paths.iter().map(|p| vec![image_attachment(p)]).collect());
        for m in app.messages.values_mut().flatten() {
            for a in &mut m.attachments {
                a.dims = Some((16, 16));
            }
        }
        tick(&mut app);
        ready_animated(&mut app, &gif, 3);
        for p in &paths[1..] {
            ready_in(&mut app, p, 16, 16);
        }
        (app, gif)
    }

    fn look_at(app: &mut App, cursor: usize) {
        let room = app.current_room.clone().unwrap();
        app.msg_views.insert(
            room.clone(),
            crate::app::messages::MsgView { cursor, top_row: 0 },
        );
        let view = crate::app::messages::view(app, &room);
        app.msg_views.insert(room, view);
    }

    #[test]
    fn the_frames_of_an_animation_that_leaves_the_viewport_are_freed_at_once() {
        let (mut app, gif) = gif_then_stills(60);
        let id = first_id(&app, &gif);
        // At the bottom the animation is far away: none of its frames are in the terminal.
        let commands = prepare_frame(&mut app);
        assert!(commands.iter().all(|c| !c.contains(&format!("i={id}"))));
        // At the top it is near: all three frames go up.
        look_at(&mut app, 0);
        let commands = prepare_frame(&mut app);
        assert_eq!(
            commands
                .iter()
                .filter(
                    |c| c.contains("a=t,") && (id..id + 3).any(|i| c.contains(&format!("i={i},")))
                )
                .count(),
            3
        );
        // Back at the bottom they are all deleted, one delete per frame, nothing else of it.
        look_at(&mut app, 60);
        let commands = prepare_frame(&mut app);
        for n in 0..3 {
            assert!(
                commands
                    .iter()
                    .any(|c| c.contains("a=d,") && c.contains(&format!("i={},", id + n))),
                "frame {n} freed"
            );
        }
        assert!(matches!(
            app.attachment_images.state(&gif),
            Some(ImageState::Ready {
                uploaded: false,
                ..
            })
        ));
        // It does not count against the bound of stills, and comes back when near again.
        look_at(&mut app, 0);
        let commands = prepare_frame(&mut app);
        assert!(count(&commands, "a=t,") >= 3);
    }

    #[test]
    fn a_timer_is_needed_only_while_an_animated_image_is_on_screen() {
        let (mut app, gif) = gif_then_stills(1);
        let id = first_id(&app, &gif);
        let still = first_id(&app, "/file-upload/0/p.png");
        prepare_frame(&mut app);
        let images = &app.attachment_images;
        let ids = |v: &[u32]| v.iter().copied().collect::<HashSet<u32>>();
        assert!(
            !images.animating_among(&ids(&[]), None),
            "nothing on screen"
        );
        assert!(
            !images.animating_among(&ids(&[still]), None),
            "a still image"
        );
        assert!(images.animating_among(&ids(&[id]), None));
        assert!(
            images.animating_among(&ids(&[id + 2]), None),
            "any frame of it"
        );
        assert!(
            !images.animating_among(&ids(&[id + 3]), None),
            "not its frame"
        );
    }

    #[test]
    fn an_animation_not_in_the_terminal_needs_no_timer() {
        let (app, gif) = gif_then_stills(60);
        let id = first_id(&app, &gif);
        // Never sent (it is far away): even if its id were on screen, nothing is playing.
        let on_screen: HashSet<u32> = [id].into_iter().collect();
        assert!(!app.attachment_images.animating_among(&on_screen, None));
    }
}

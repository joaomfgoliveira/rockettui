//! The full-size image view (`show_image`, `gp`): the first image of the message under the cursor,
//! drawn as large as the terminal allows over the whole screen.
//!
//! State, not a mode: opening it leaves the mode badge, the focus and the cursor alone, so closing
//! returns to the message flow exactly as it was. While it is open the reducer hands it every key
//! (`h` and `l` change image, `Esc`, `q` and `Ctrl-c` close, nothing else does anything). The view
//! names its message by room and id and an image number, and holds no copy of the message, so a
//! live change that removes the message or its images closes it (`close_if_stale`).
//!
//! The pixels are those of the original file (`Effect::LoadAttachmentView`: the disk cache, else
//! one request for the file's own `/file-upload/` path), decoded larger than the inline box. The
//! inline box may show only a thumbnail the server made, so the view needs the original; if it
//! cannot be had the view keeps the smaller image already in memory. The image goes to the terminal under an id of its own, which is deleted when the view
//! closes (`app::attachments::prepare_frame`).
//!
//! Pure: no I/O and no `Frame`; `ui::image_view` draws it. Text goes through the sanitizer here, so
//! the drawer can measure what it prints.

use crate::app::attachments::{ImageState, is_fetchable_image};
use crate::app::emoji::Graphics;
use crate::app::{App, Focus, Mode, StatusMsg, messages};
use crate::data::{Attachment, Message, MessageKind, RoomId};
use crate::effect::{AttachmentRef, Effect};
use crate::emoji_image::Rgba;
use crate::input::key::Chord;
use crate::ui::sanitize::sanitize_line;
use crossterm::event::{KeyCode, KeyModifiers};

/// The decoded pixels the view shows, for the image at `path`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pixels {
    pub path: String,
    pub image: Rgba,
    /// Set once the terminal has the pixels (under the view's own id).
    pub uploaded: bool,
    /// The cells (columns, rows) the terminal was last told the image covers.
    pub placed: Option<(usize, usize)>,
}

/// The message the view was opened on, by room and id, the image shown (a position among the
/// message's images) and what is known of its pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageView {
    pub room: RoomId,
    pub id: String,
    pub index: usize,
    pub pixels: Option<Pixels>,
    /// The path whose full-size load is on its way, so it is asked for once.
    pub requested: Option<String>,
}

/// What the view shows now.
#[derive(Clone, Debug, PartialEq)]
pub struct Content {
    /// The file name, sanitized.
    pub name: String,
    /// The 1-based position of the image and how many images the message has.
    pub position: (usize, usize),
    pub body: Body,
}

/// The middle of the view.
#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    /// The image is not on screen yet: still loading.
    Loading,
    /// The image failed and will not come.
    Failed,
    /// The image, as these pixels.
    Image { image: Rgba, uploaded: bool },
}

/// The fetchable images of a message, in the order the server lists them.
pub fn images_of(message: &Message) -> Vec<&Attachment> {
    if message.kind != MessageKind::Normal {
        return Vec::new();
    }
    message
        .attachments
        .iter()
        .filter(|a| is_fetchable_image(a))
        .collect()
}

/// The room and the message under the cursor when the messages pane is focused: what `show_image`
/// acts on, and what the context popup checks before it lists the action. `None` for any other
/// pane or no open room.
fn message_under_cursor(app: &App) -> Option<(RoomId, &Message)> {
    if app.mode != Mode::Normal || app.focus != Focus::Messages {
        return None;
    }
    let room = app.current_room.clone()?;
    let message = app
        .messages
        .get(&room)?
        .get(messages::view(app, &room).cursor)?;
    Some((room, message))
}

/// True when the message under the cursor has an image `show_image` could open: what the context
/// popup checks before it lists the action.
pub fn has_image_under_cursor(app: &App) -> bool {
    message_under_cursor(app).is_some_and(|(_, m)| !images_of(m).is_empty())
}

fn notice(app: &mut App, text: &str) {
    app.status_msg = Some(StatusMsg::Info(text.to_string()));
}

/// `show_image`: open the view on the first image of the message under the cursor. A message with
/// no image, an image that is not ready, an image that failed or a terminal that cannot show
/// images gets a status notice and nothing else changes. Anything but the messages pane in
/// Normal mode is a silent no-op.
pub fn open(app: &mut App) {
    let Some((room, message)) = message_under_cursor(app) else {
        return;
    };
    let id = message.id.clone();
    let first = images_of(message).first().and_then(|a| a.path.clone());
    let Some(path) = first else {
        notice(app, "no image in this message");
        return;
    };
    if app.graphics != Graphics::Supported {
        notice(app, "images cannot be shown in this terminal");
        return;
    }
    if !app.settings.inline_images {
        notice(app, "images are turned off (inline_images)");
        return;
    }
    match app.attachment_images.state(&path) {
        Some(ImageState::Ready { .. }) => {}
        Some(ImageState::Failed) => {
            notice(app, "the image could not be loaded");
            return;
        }
        None => {
            notice(app, "the image is still loading");
            return;
        }
    }
    app.matcher.reset();
    app.image_view = Some(ImageView {
        room,
        id,
        index: 0,
        pixels: None,
        requested: None,
    });
    sync(app);
}

/// The message the view is on, and its images, `None` when the view is closed or the message is
/// gone.
fn current<'a>(app: &'a App, view: &ImageView) -> Option<Vec<&'a Attachment>> {
    let message = app
        .messages
        .get(&view.room)?
        .iter()
        .find(|m| m.id == view.id)?;
    let images = images_of(message);
    (!images.is_empty()).then_some(images)
}

/// The view's content now, `None` when it is closed or its message is gone or has no image.
pub fn content(app: &App) -> Option<Content> {
    let view = app.image_view.as_ref()?;
    let images = current(app, view)?;
    let index = view.index.min(images.len() - 1);
    let attachment = images[index];
    let path = attachment.path.as_deref()?;
    let body = match app.attachment_images.state(path) {
        Some(ImageState::Failed) => Body::Failed,
        Some(ImageState::Ready { .. }) => match view.pixels.as_ref() {
            Some(p) if p.path == path => Body::Image {
                image: p.image.clone(),
                uploaded: p.uploaded,
            },
            // While the full-size pixels load the view says so, rather than flash the smaller
            // image first.
            _ => Body::Loading,
        },
        None => Body::Loading,
    };
    Some(Content {
        name: sanitize_line(&attachment.name).into_owned(),
        position: (index + 1, images.len()),
        body,
    })
}

/// What the view loads for `attachment`: the original file when the server shows a thumbnail
/// under the inline path (its size is not the reported one, so none is given), else the inline
/// path itself. `None` for an attachment with no path.
fn view_ref(attachment: &Attachment, animate: bool) -> Option<AttachmentRef> {
    match attachment.full_path.as_ref() {
        Some(full) => Some(AttachmentRef {
            path: full.clone(),
            size: None,
            dims: None,
            animate,
            source: None,
        }),
        None => Some(AttachmentRef {
            path: attachment.path.clone()?,
            size: attachment.size,
            dims: attachment.dims,
            animate,
            source: None,
        }),
    }
}

/// The inline path of the image the view is on when `loaded` is what the view asked for.
fn inline_path_of(app: &App, loaded: &str) -> Option<String> {
    let view = app.image_view.as_ref()?;
    let images = current(app, view)?;
    let attachment = images.get(view.index.min(images.len() - 1))?;
    (view_ref(attachment, false)?.path == loaded).then(|| attachment.path.clone())?
}

/// Ask for the full-size pixels of the image the view is on when they are not held or on their
/// way, and the smaller image is ready to stand in for them. The reducer calls this after every
/// action, so an image that finishes loading while the view is open is picked up.
pub fn sync(app: &mut App) {
    let Some(view) = app.image_view.as_ref() else {
        return;
    };
    let Some(images) = current(app, view) else {
        return;
    };
    let index = view.index.min(images.len() - 1);
    let Some(path) = images[index].path.clone() else {
        return;
    };
    let animate = app.settings.animate_images;
    let wanted = view_ref(images[index], animate).unwrap_or(AttachmentRef {
        path: path.clone(),
        size: images[index].size,
        dims: images[index].dims,
        animate,
        source: None,
    });
    let held = view.pixels.as_ref().is_some_and(|p| p.path == path);
    let asked = view.requested.as_deref() == Some(path.as_str());
    if held
        || asked
        || !matches!(
            app.attachment_images.state(&path),
            Some(ImageState::Ready { .. })
        )
    {
        return;
    }
    if let Some(view) = app.image_view.as_mut() {
        view.requested = Some(path);
    }
    app.effects.push(Effect::LoadAttachmentView(wanted));
}

/// The full-size pixels arrived for `image`. An answer for an image the view is no longer on is
/// dropped.
pub fn on_view_ready(app: &mut App, image: AttachmentRef, decoded: Rgba) {
    if let Some(inline) = inline_path_of(app, &image.path) {
        set_pixels(app, &inline, decoded);
    }
}

/// The full-size pixels could not be had: the view keeps the smaller image already in memory.
pub fn on_view_failed(app: &mut App, image: AttachmentRef) {
    let Some(inline) = inline_path_of(app, &image.path) else {
        return;
    };
    let fallback = match app.attachment_images.state(&inline) {
        Some(ImageState::Ready { image, .. }) => Some(image.clone()),
        _ => None,
    };
    if let Some(small) = fallback {
        set_pixels(app, &inline, small);
    }
}

fn set_pixels(app: &mut App, path: &str, image: Rgba) {
    let on_it = app.image_view.as_ref().is_some_and(|view| {
        current(app, view).is_some_and(|images| {
            images
                .get(view.index.min(images.len().saturating_sub(1)))
                .and_then(|a| a.path.as_deref())
                == Some(path)
        })
    });
    if !on_it {
        return;
    }
    let old = app.image_view.as_mut().and_then(|view| {
        view.pixels.replace(Pixels {
            path: path.to_string(),
            image,
            uploaded: false,
            placed: None,
        })
    });
    // The view's ids are one block: frames the new image does not overwrite would linger.
    if let Some(old) = old {
        free_pixels(app, &old);
    }
}

/// Delete the terminal's copy of the view's image, every frame of it, before the next frame.
fn free_pixels(app: &mut App, pixels: &Pixels) {
    if pixels.uploaded
        && let Some(id) = app.attachment_images.view_id()
    {
        for n in 0..pixels.image.frame_count() {
            app.attachment_images.queue_delete(id + n);
        }
    }
}

/// Close the view. The terminal's copy of the image, all its frames if it plays, is deleted
/// before the next frame.
pub fn close(app: &mut App) {
    if let Some(view) = app.image_view.take()
        && let Some(pixels) = view.pixels
    {
        free_pixels(app, &pixels);
    }
}

/// Close the view when it has nothing left to show: its message left the room or lost its images.
/// The reducer calls this after every action, so a live change closes it at once.
pub fn close_if_stale(app: &mut App) {
    let stale = app
        .image_view
        .as_ref()
        .is_some_and(|view| current(app, view).is_none());
    if stale {
        close(app);
    }
}

/// Move to the next (`forward`) or previous image of the message, stopping at either end.
fn step(app: &mut App, forward: bool) {
    let Some(view) = app.image_view.as_ref() else {
        return;
    };
    let Some(images) = current(app, view) else {
        return;
    };
    let last = images.len() - 1;
    let index = view.index.min(last);
    let next = if forward {
        (index + 1).min(last)
    } else {
        index.saturating_sub(1)
    };
    if next == index {
        return;
    }
    let old = app.image_view.as_mut().and_then(|view| {
        view.index = next;
        view.requested = None;
        view.pixels.take()
    });
    if let Some(old) = old {
        free_pixels(app, &old);
    }
    sync(app);
}

/// A key while the view is open: it never reaches the keymap. `Esc`, `q` and `Ctrl-c` close it,
/// `l` shows the next image and `h` the previous one. Nothing else does anything.
pub fn key(app: &mut App, chord: Chord) {
    let plain = |c: char| chord.code == KeyCode::Char(c) && chord.mods.is_empty();
    let ctrl_c = chord.code == KeyCode::Char('c') && chord.mods == KeyModifiers::CONTROL;
    if ctrl_c || plain('q') || (chord.code == KeyCode::Esc && chord.mods.is_empty()) {
        close(app);
    } else if plain('l') {
        step(app, true);
    } else if plain('h') {
        step(app, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::attachments::MAX_IN_FLIGHT;
    use crate::app::update::update;
    use crate::data::{AttachmentKind, Message};
    use crate::test_util::{app_80x24, press, render_buffer, sample_message};

    const A: &str = "/file-upload/a/one.png";
    const B: &str = "/file-upload/b/two.png";
    const C: &str = "/file-upload/c/three.png";

    fn att(path: &str, name: &str) -> Attachment {
        Attachment {
            kind: AttachmentKind::Image,
            name: name.to_string(),
            path: Some(path.to_string()),
            full_path: None,
            mime: Some("image/png".to_string()),
            size: Some(1000),
            dims: Some((160, 160)),
        }
    }

    fn pixels(w: u32, h: u32) -> Rgba {
        Rgba {
            width: w,
            height: h,
            pixels: vec![3u8; (w * h * 4) as usize].into(),
            ..Default::default()
        }
    }

    fn refer(path: &str) -> AttachmentRef {
        AttachmentRef {
            path: path.to_string(),
            size: Some(1000),
            dims: Some((160, 160)),
            animate: false,
            source: None,
        }
    }

    /// An app with the cursor on a message holding `attachments` (the last message of the room).
    fn app_with(attachments: Vec<Attachment>) -> App {
        let mut app = app_80x24();
        app.graphics = Graphics::Supported;
        let room = app.rooms[0].id.clone();
        let msg = Message {
            room_id: room.clone(),
            body: "look".to_string(),
            attachments,
            plain_text: false,
            ..sample_message("m-img")
        };
        app.messages.insert(room.clone(), vec![msg]);
        app.open_room(room);
        app.focus = Focus::Messages;
        app.effects.clear();
        app
    }

    fn ready(app: &mut App, path: &str) {
        update(
            app,
            Action::AttachmentImageReady {
                image: refer(path),
                decoded: pixels(16, 16),
            },
        );
    }

    /// The text of the status line, `None` when there is none.
    fn status(app: &App) -> Option<String> {
        match app.status_msg.as_ref()? {
            StatusMsg::Info(text) | StatusMsg::Error(text) => Some(text.clone()),
        }
    }

    fn view_loads(app: &App) -> Vec<String> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::LoadAttachmentView(r) => Some(r.path.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn gp_opens_the_first_ready_image_and_asks_for_the_full_size_from_the_cache() {
        let mut app = app_with(vec![att(A, "one.png"), att(B, "two.png")]);
        update(&mut app, Action::Clock(0));
        ready(&mut app, A);
        app.effects.clear();
        press(&mut app, "gp");
        let view = app.image_view.as_ref().expect("the view is open");
        assert_eq!(view.index, 0);
        assert_eq!(view.id, "m-img");
        assert_eq!(view_loads(&app), [A]);
        // No request for any image: it reads the cache.
        assert!(
            !app.effects
                .iter()
                .any(|e| matches!(e, Effect::FetchAttachmentImage(_)))
        );
        // The mode, focus and status are untouched.
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, Focus::Messages);
        assert!(app.status_msg.is_none());
        // Asking again while the load is out does not ask twice.
        update(&mut app, Action::Clock(0));
        assert_eq!(view_loads(&app).len(), 1);
    }

    #[test]
    fn the_full_size_pixels_replace_the_loading_note() {
        let mut app = app_with(vec![att(A, "one.png")]);
        update(&mut app, Action::Clock(0));
        ready(&mut app, A);
        press(&mut app, "gp");
        assert_eq!(content(&app).expect("content").body, Body::Loading);
        update(
            &mut app,
            Action::AttachmentViewReady {
                image: refer(A),
                decoded: pixels(100, 100),
            },
        );
        match content(&app).expect("content").body {
            Body::Image { image, uploaded } => {
                assert_eq!(image.width, 100);
                assert!(!uploaded);
            }
            other => panic!("{other:?}"),
        }
        // An answer for another image is dropped.
        update(
            &mut app,
            Action::AttachmentViewReady {
                image: refer(B),
                decoded: pixels(7, 7),
            },
        );
        let Body::Image { image, .. } = content(&app).unwrap().body else {
            panic!("image");
        };
        assert_eq!(image.width, 100);
    }

    #[test]
    fn a_failed_full_size_load_falls_back_to_the_image_in_memory() {
        let mut app = app_with(vec![att(A, "one.png")]);
        update(&mut app, Action::Clock(0));
        ready(&mut app, A);
        press(&mut app, "gp");
        update(&mut app, Action::AttachmentViewFailed(refer(A)));
        let Body::Image { image, .. } = content(&app).expect("content").body else {
            panic!("the small image stands in");
        };
        assert_eq!((image.width, image.height), (16, 16));
    }

    #[test]
    fn l_and_h_change_image_and_stop_at_the_ends() {
        let mut app = app_with(vec![
            att(A, "one.png"),
            att(B, "two.png"),
            att(C, "three.png"),
        ]);
        update(&mut app, Action::Clock(0));
        for p in [A, B, C] {
            ready(&mut app, p);
        }
        press(&mut app, "gp");
        let at = |app: &App| app.image_view.as_ref().map(|v| v.index);
        assert_eq!(content(&app).unwrap().position, (1, 3));
        press(&mut app, "h");
        assert_eq!(at(&app), Some(0), "stops at the first");
        app.effects.clear();
        press(&mut app, "l");
        assert_eq!(at(&app), Some(1));
        assert_eq!(content(&app).unwrap().position, (2, 3));
        assert_eq!(
            view_loads(&app),
            [B],
            "the next image's pixels are asked for"
        );
        press(&mut app, "l");
        press(&mut app, "l");
        assert_eq!(at(&app), Some(2), "stops at the last");
        press(&mut app, "h");
        assert_eq!(at(&app), Some(1));
        // Going back: the first image's pixels are asked for again from the cache, not the server.
        assert!(
            !app.effects
                .iter()
                .any(|e| matches!(e, Effect::FetchAttachmentImage(_)))
        );
    }

    #[test]
    fn esc_q_and_ctrl_c_close_it_and_the_cursor_stays() {
        for close_key in ["<Esc>", "q", "<C-c>"] {
            let mut app = app_with(vec![att(A, "one.png")]);
            update(&mut app, Action::Clock(0));
            ready(&mut app, A);
            let before = (
                app.msg_views.get(&app.rooms[0].id).copied(),
                app.mode,
                app.focus,
                messages::view(&app, &app.rooms[0].id),
            );
            press(&mut app, "gp");
            assert!(app.image_view.is_some());
            press(&mut app, close_key);
            assert!(app.image_view.is_none(), "{close_key}");
            let after = (
                app.msg_views.get(&app.rooms[0].id).copied(),
                app.mode,
                app.focus,
                messages::view(&app, &app.rooms[0].id),
            );
            assert_eq!(before, after, "{close_key}");
        }
    }

    #[test]
    fn other_keys_do_nothing_while_it_is_open() {
        let mut app = app_with(vec![att(A, "one.png")]);
        update(&mut app, Action::Clock(0));
        ready(&mut app, A);
        press(&mut app, "gp");
        for keys in ["j", "k", "i", ":", "gg", "G", "<C-k>", "gp"] {
            press(&mut app, keys);
            assert!(app.image_view.is_some(), "{keys}");
            assert_eq!(app.mode, Mode::Normal, "{keys}");
        }
    }

    #[test]
    fn a_message_without_an_image_says_so_and_changes_nothing() {
        let mut app = app_with(Vec::new());
        press(&mut app, "gp");
        assert!(app.image_view.is_none());
        assert_eq!(status(&app).as_deref(), Some("no image in this message"));
        // A file that is not an image is not an image either.
        let mut pdf = att("/file-upload/x/a.pdf", "a.pdf");
        pdf.kind = AttachmentKind::Other;
        let mut app = app_with(vec![pdf]);
        press(&mut app, "gp");
        assert!(app.image_view.is_none());
        assert!(app.status_msg.is_some());
    }

    #[test]
    fn an_image_that_is_loading_or_failed_says_so() {
        let mut app = app_with(vec![att(A, "one.png")]);
        update(&mut app, Action::Clock(0));
        press(&mut app, "gp");
        assert!(app.image_view.is_none());
        assert_eq!(status(&app).as_deref(), Some("the image is still loading"));
        update(&mut app, Action::AttachmentImageFailed(refer(A)));
        press(&mut app, "gp");
        assert!(app.image_view.is_none());
        assert_eq!(
            status(&app).as_deref(),
            Some("the image could not be loaded")
        );
        let _ = MAX_IN_FLIGHT;
    }

    #[test]
    fn a_terminal_without_images_says_so() {
        let mut app = app_with(vec![att(A, "one.png")]);
        app.graphics = Graphics::Unsupported;
        press(&mut app, "gp");
        assert!(app.image_view.is_none());
        assert_eq!(
            status(&app).as_deref(),
            Some("images cannot be shown in this terminal")
        );
    }

    #[test]
    fn with_inline_images_off_it_says_so() {
        let mut app = app_with(vec![att(A, "one.png")]);
        app.settings.inline_images = false;
        press(&mut app, "gp");
        assert!(app.image_view.is_none());
        assert!(app.status_msg.is_some());
    }

    #[test]
    fn gp_outside_the_messages_pane_is_silent() {
        let mut app = app_with(vec![att(A, "one.png")]);
        update(&mut app, Action::Clock(0));
        ready(&mut app, A);
        app.focus = Focus::Sidebar;
        press(&mut app, "gp");
        assert!(app.image_view.is_none() && app.status_msg.is_none());
    }

    #[test]
    fn the_view_closes_when_its_message_goes_away() {
        let mut app = app_with(vec![att(A, "one.png")]);
        update(&mut app, Action::Clock(0));
        ready(&mut app, A);
        press(&mut app, "gp");
        let room = app.rooms[0].id.clone();
        app.messages.get_mut(&room).unwrap()[0].attachments.clear();
        update(&mut app, Action::Clock(0));
        assert!(app.image_view.is_none());
    }

    // ---- drawing and the terminal

    fn open_with_pixels(w: u32, h: u32) -> App {
        let mut app = app_with(vec![att(A, "one.png")]);
        update(&mut app, Action::Clock(0));
        ready(&mut app, A);
        press(&mut app, "gp");
        update(
            &mut app,
            Action::AttachmentViewReady {
                image: refer(A),
                decoded: pixels(w, h),
            },
        );
        app.effects.clear();
        app
    }

    fn placeholder_count(app: &App) -> usize {
        render_buffer(app, 80, 24)
            .content()
            .iter()
            .filter(|c| c.symbol().starts_with(crate::kitty::PLACEHOLDER))
            .count()
    }

    fn screen(app: &App) -> String {
        let buffer = render_buffer(app, 80, 24);
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_overlay_shows_the_name_and_draws_the_image_after_it_is_uploaded() {
        let mut app = open_with_pixels(800, 400);
        let text = screen(&app);
        assert!(text.contains("one.png (1/1)"), "{text}");
        assert!(text.contains("Esc: close"));
        // Before the terminal has the pixels: a note, no cells.
        assert!(text.contains("loading image..."));
        assert_eq!(placeholder_count(&app), 0);
        // The frame prepares the terminal, then the cells are drawn.
        let commands = crate::app::attachments::prepare_frame(&mut app);
        assert!(commands.iter().any(|c| c.contains("a=t,")));
        let (cols, rows) = crate::ui::image_view::placement(&app, (800, 400)).expect("room");
        assert!(
            commands
                .iter()
                .any(|c| c.contains(&format!("c={cols},r={rows}")))
        );
        assert_eq!(placeholder_count(&app), cols * rows);
        let text = screen(&app);
        assert!(!text.contains("loading image..."));
        // The pane behind is covered: the sidebar is not drawn.
        assert!(!text.contains("Rooms"));
        // The image is as large as it can be: it spans the inside in one direction.
        let inner = crate::ui::image_view::inner(&app);
        assert!(cols == usize::from(inner.width) || rows == usize::from(inner.height));
        // The id is the view's own, in the attachment range.
        let id = app.attachment_images.view_id().expect("id");
        assert!(id >= crate::kitty::IMAGE_ID_FIRST);
    }

    #[test]
    fn the_overlay_keeps_the_images_shape() {
        let app = open_with_pixels(800, 400);
        let (cols, rows) = crate::ui::image_view::placement(&app, (800, 400)).unwrap();
        // 2:1 pixels in cells of 1:2 is 4 columns to 1 row... within a few cells for rounding.
        let ratio = cols as f64 / (2.0 * rows as f64);
        assert!((ratio - 2.0).abs() < 0.2, "{cols}x{rows}");
        let tall = crate::ui::image_view::placement(&app, (400, 800)).unwrap();
        assert!(tall.1 >= tall.0 / 2, "{tall:?}");
    }

    #[test]
    fn closing_frees_the_image_in_the_terminal_and_clears_the_screen_of_it() {
        let mut app = open_with_pixels(800, 400);
        crate::app::attachments::prepare_frame(&mut app);
        let id = app.attachment_images.view_id().expect("id");
        press(&mut app, "<Esc>");
        let commands = crate::app::attachments::prepare_frame(&mut app);
        assert!(
            commands
                .iter()
                .any(|c| c.contains("a=d") && c.contains(&format!("i={id},"))),
            "{commands:?}"
        );
        assert!(!screen(&app).contains("one.png (1/1)"));
        // What is drawn now is the inline box of the message flow again, not the overlay.
        let (cols, rows) = crate::ui::attachment::box_size(Some((160, 160)), 36, 12);
        assert_eq!(placeholder_count(&app), cols * rows);
        // Nothing is deleted twice.
        assert!(
            crate::app::attachments::prepare_frame(&mut app)
                .iter()
                .all(|c| !c.contains("a=d"))
        );
    }

    #[test]
    fn a_hostile_file_name_is_drawn_clean() {
        let mut hostile = att(A, "a\u{1b}[31mb\u{202e}c.png");
        hostile.name = format!("{}\u{305}{}", hostile.name, crate::kitty::PLACEHOLDER);
        let mut app = app_with(vec![hostile]);
        update(&mut app, Action::Clock(0));
        ready(&mut app, A);
        press(&mut app, "gp");
        let text = screen(&app);
        assert!(!text.contains('\u{1b}') && !text.contains('\u{202e}'));
        assert_eq!(placeholder_count(&app), 0);
    }

    fn open_with_animation(frames: usize) -> App {
        let mut app = app_with(vec![att(A, "one.gif")]);
        update(&mut app, Action::Clock(0));
        ready(&mut app, A);
        press(&mut app, "gp");
        update(
            &mut app,
            Action::AttachmentViewReady {
                image: refer(A),
                decoded: crate::app::attachments::tests::animated(400, 200, frames),
            },
        );
        app.effects.clear();
        app
    }

    #[test]
    fn the_view_asks_for_frames_only_when_animation_is_on() {
        for on in [true, false] {
            let mut app = app_with(vec![att(A, "one.gif")]);
            app.settings.animate_images = on;
            update(&mut app, Action::Clock(0));
            ready(&mut app, A);
            press(&mut app, "gp");
            let asked = app.effects.iter().find_map(|e| match e {
                Effect::LoadAttachmentView(r) => Some(r.animate),
                _ => None,
            });
            assert_eq!(asked, Some(on));
        }
    }

    #[test]
    fn the_view_plays_every_frame_and_stops_and_frees_them_on_close() {
        let mut app = open_with_animation(4);
        let commands = crate::app::attachments::prepare_frame(&mut app);
        let id = app.attachment_images.view_id().expect("id");
        for n in 0..4 {
            assert!(
                commands
                    .iter()
                    .any(|c| c.contains("a=t,") && c.contains(&format!("i={},", id + n))),
                "frame {n} sent"
            );
        }
        assert!(
            crate::app::attachments::prepare_frame(&mut app).is_empty(),
            "once"
        );
        // The overlay shows the frame the clock picks, same cells each time.
        let mut cells = Vec::new();
        for (ms, want) in [(0u64, 0u32), (100, 1), (200, 2), (350, 3), (400, 0)] {
            app.anim_ms = ms;
            let buffer = render_buffer(&app, 80, 24);
            let ids = crate::ui::attachment::ids_on_screen(&buffer);
            assert_eq!(ids, [id + want].into_iter().collect(), "ms {ms}");
            cells.push(placeholder_count(&app));
        }
        assert!(cells.windows(2).all(|w| w[0] == w[1]));
        // It is playing, so the screen needs a timer.
        let ids = crate::ui::attachment::ids_on_screen(&render_buffer(&app, 80, 24));
        let shown = app
            .image_view
            .as_ref()
            .and_then(|v| v.pixels.as_ref())
            .map(|p| &p.image);
        assert!(app.attachment_images.animating_among(&ids, shown));
        // Closing deletes every frame, and the timer is no longer needed.
        press(&mut app, "<Esc>");
        let commands = crate::app::attachments::prepare_frame(&mut app);
        for n in 0..4 {
            assert!(
                commands
                    .iter()
                    .any(|c| c.contains("a=d") && c.contains(&format!("i={},", id + n))),
                "frame {n} freed"
            );
        }
        assert!(
            !app.attachment_images
                .animating_among(&Default::default(), None)
        );
    }

    #[test]
    fn changing_image_frees_the_frames_of_the_one_left() {
        let mut app = app_with(vec![att(A, "one.gif"), att(B, "two.png")]);
        update(&mut app, Action::Clock(0));
        ready(&mut app, A);
        ready(&mut app, B);
        press(&mut app, "gp");
        update(
            &mut app,
            Action::AttachmentViewReady {
                image: refer(A),
                decoded: crate::app::attachments::tests::animated(400, 200, 3),
            },
        );
        crate::app::attachments::prepare_frame(&mut app);
        let id = app.attachment_images.view_id().expect("id");
        press(&mut app, "l");
        let commands = crate::app::attachments::prepare_frame(&mut app);
        for n in 0..3 {
            assert!(
                commands
                    .iter()
                    .any(|c| c.contains("a=d") && c.contains(&format!("i={},", id + n))),
                "frame {n} freed"
            );
        }
    }
}

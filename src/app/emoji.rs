//! The server's custom emojis, owned by the session: requested once when the rooms have loaded
//! (and again when a room opens while the list is still not there), then read by the reaction
//! picker, by message text and by the reaction row.
//!
//! Pure: no I/O. The request is an `Effect`, the answer an `Action`.

use crate::app::App;
use crate::data::{CustomEmoji, MessageKind};
use crate::effect::{Effect, EmojiRef};
use crate::emoji_image::Rgba;
use std::collections::{HashMap, HashSet, VecDeque};

/// At most this many image fetches run at once; the rest wait in a queue.
pub const MAX_IN_FLIGHT: usize = 4;

/// Whether the terminal can show images (the kitty graphics protocol with Unicode placeholders).
/// Decided once at startup (`tui`); nothing is fetched for images or sent to the terminal unless it
/// is `Supported`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Graphics {
    #[default]
    Unsupported,
    Supported,
}

/// Where the image of one custom emoji stands.
#[derive(Clone, Debug, PartialEq)]
pub enum ImageState {
    /// Fetched and decoded, for the etag it was fetched under. `id` is the terminal image id it
    /// is (or will be) transmitted as; `uploaded` is set once the terminal has it, and only then
    /// is the emoji drawn as its image.
    Ready {
        etag: String,
        image: Rgba,
        id: u32,
        uploaded: bool,
    },
    /// Could not be fetched or decoded: final for the session.
    Failed { etag: String },
}

/// The images of the session: what was asked for, what is on its way and what came back. One entry
/// per emoji, never per use.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Images {
    /// Every (name, etag) already asked for, so one is asked for once per session.
    requested: HashSet<(String, String)>,
    /// Asked for and waiting for a free slot.
    queue: VecDeque<EmojiRef>,
    /// Fetches running now.
    in_flight: usize,
    /// The outcome per emoji name.
    state: HashMap<String, ImageState>,
    /// The last terminal image id handed out. Ids start at 1 and stay inside the range the client
    /// reserves for its own cells (`MAX_IMAGE_ID`).
    last_id: u32,
}

/// One frame to transmit to the terminal as image `id`.
#[derive(Clone, Debug, PartialEq)]
pub struct Upload {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: std::sync::Arc<[u8]>,
}

/// The most image ids the client hands out in a session. An emoji arriving after that is drawn as
/// its name.
pub const MAX_IMAGE_ID: u32 = 0x6EED;

/// The private-use character that stands for one cell of image `id` in text that has been drawn
/// but not yet expanded into spans (`ui::emoji`).
pub fn image_cell(id: u32) -> char {
    char::from_u32(crate::ui::sanitize::INTERNAL_FIRST + id).unwrap_or('\u{10FFFD}')
}

/// The image id a cell character stands for, `None` for any other character.
pub fn cell_image(c: char) -> Option<u32> {
    let id = (c as u32).checked_sub(crate::ui::sanitize::INTERNAL_FIRST)?;
    (1..=MAX_IMAGE_ID).contains(&id).then_some(id)
}

/// How a custom emoji is drawn right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    /// Its full `:name:` in the custom style: a terminal without images, or a failed image.
    Text,
    /// Two columns in the custom style, while an image that can still arrive is not on screen.
    Standin,
    /// Its image, as the cells of terminal image `id`.
    Image(u32),
}

/// Everything the drawing of custom emojis reads, in one `Copy` value: the list, the images and
/// whether the terminal can show them. `App::row_ctx` is the one place it is built for the app.
#[derive(Clone, Copy)]
pub struct EmojiView<'a> {
    pub custom: &'a CustomEmojis,
    pub images: &'a Images,
    pub graphics: Graphics,
    /// Milliseconds since the program started, which picks the frame of an animated emoji. Rows
    /// never depend on it: every frame is drawn in the same two cells.
    pub now_ms: u64,
}

/// No images and no list: for a caller (a test) that has none.
pub static NO_IMAGES: std::sync::LazyLock<Images> = std::sync::LazyLock::new(Images::default);

impl EmojiView<'static> {
    /// A view of nothing: no custom emojis, no images, a terminal without graphics.
    pub fn none() -> Self {
        EmojiView {
            custom: &NO_CUSTOM,
            images: &NO_IMAGES,
            graphics: Graphics::Unsupported,
            now_ms: 0,
        }
    }
}

impl EmojiView<'_> {
    /// How the custom emoji `name` (its own name, not an alias) is drawn now. On a terminal that
    /// shows images it takes two columns until its image is on screen, so rows never change when
    /// it arrives; a failed image is final and falls back to the full name.
    pub fn look(&self, name: &str) -> Look {
        if self.graphics != Graphics::Supported {
            return Look::Text;
        }
        match self.images.state(name) {
            Some(ImageState::Failed { .. }) => Look::Text,
            Some(ImageState::Ready {
                id,
                image,
                uploaded: true,
                ..
            }) => Look::Image(id + image.frame_at(self.now_ms)),
            _ => Look::Standin,
        }
    }
}

impl Images {
    /// Put `state` for `name`, for tests that draw with an image in a given state.
    #[cfg(test)]
    pub fn set_state(&mut self, name: &str, state: ImageState) {
        self.state.insert(name.to_string(), state);
    }

    /// The image state of `name`, `None` while it is unknown, queued or on its way.
    pub fn state(&self, name: &str) -> Option<&ImageState> {
        self.state.get(name)
    }

    /// The number of fetches running now.
    pub fn in_flight(&self) -> usize {
        self.in_flight
    }

    /// The number of fetches waiting for a slot.
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// True when an image that is on screen is animated, so the screen must be redrawn on a timer.
    pub fn has_animation(&self) -> bool {
        self.state.values().any(|state| {
            matches!(state, ImageState::Ready { image, uploaded: true, .. } if image.is_animated())
        })
    }

    /// The frames of the images that arrived and are not in the terminal yet, each image marked as
    /// uploaded: the caller transmits them (`kitty::upload`) before the next frame. A still image
    /// is one frame under its `id`; an animated one is a frame per id, from its `id` on. Each is
    /// handed out once.
    pub fn take_uploads(&mut self) -> Vec<Upload> {
        let mut out = Vec::new();
        for state in self.state.values_mut() {
            if let ImageState::Ready {
                image,
                id,
                uploaded,
                ..
            } = state
                && !*uploaded
            {
                *uploaded = true;
                for n in 0..image.frame_count() {
                    if let Some(pixels) = image.frame_pixels(n) {
                        out.push(Upload {
                            id: *id + n,
                            width: image.width,
                            height: image.height,
                            pixels: pixels.clone(),
                        });
                    }
                }
            }
        }
        out.sort_by_key(|u| u.id);
        out
    }
}

/// The server's custom emojis. A failed load goes back to `NotLoaded` so that the next room
/// opening asks again; nothing is shown about it, custom shortcodes are simply drawn as the
/// fallback meanwhile.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CustomEmojis {
    #[default]
    NotLoaded,
    Loading,
    Loaded(Vec<CustomEmoji>),
}

/// No custom emojis: for a caller (a test, a context without a session) that has none to give.
pub static NO_CUSTOM: CustomEmojis = CustomEmojis::NotLoaded;

impl CustomEmojis {
    /// The emojis to search, none until they have arrived.
    pub fn list(&self) -> &[CustomEmoji] {
        match self {
            CustomEmojis::Loaded(list) => list,
            _ => &[],
        }
    }

    /// The custom emoji a name (no colons) stands for, by its own name or by one of its aliases.
    /// An exact name wins over another emoji's alias of the same spelling.
    pub fn find(&self, name: &str) -> Option<&CustomEmoji> {
        let list = self.list();
        list.iter()
            .find(|e| e.name == name)
            .or_else(|| list.iter().find(|e| e.aliases.iter().any(|a| a == name)))
    }
}

/// Ask for the list when it is not loaded and no request is on its way.
pub fn ensure_loaded(app: &mut App) {
    if app.custom_emojis == CustomEmojis::NotLoaded {
        app.custom_emojis = CustomEmojis::Loading;
        app.effects.push(Effect::LoadCustomEmojis);
    }
}

/// Ask for the images of the custom emojis that the open room draws and that were not asked for
/// before: its messages' bodies and reactions. Nothing is asked for unless the terminal can show
/// images and the list has loaded. Each (name, etag) is asked for once per session, at most
/// `MAX_IN_FLIGHT` run at a time, and the rest wait in order.
pub fn request_images(app: &mut App) {
    if app.graphics != Graphics::Supported || app.custom_emojis.list().is_empty() {
        pump(app);
        return;
    }
    // The messages on screen: the open room, and the search results or the context view over it.
    let mut shown: Vec<&crate::data::Message> = Vec::new();
    if let Some(room) = app.current_room.as_ref() {
        shown.extend(app.messages.get(room).map_or(&[][..], Vec::as_slice));
    }
    if let Some(list) = app.results.as_ref() {
        shown.extend(&list.messages);
        if let Some(context) = list.context.as_ref() {
            shown.extend(&context.messages);
        }
    }
    let mut wanted: Vec<String> = Vec::new();
    for message in shown {
        if message.kind == MessageKind::Normal {
            names_in_body(&message.body, app, &mut wanted);
        }
        for reaction in &message.reactions {
            if let Some(crate::ui::emoji::Found::Custom(own)) = crate::ui::emoji::lookup_found(
                &reaction.name,
                &app.settings.emoji_aliases,
                &app.custom_emojis,
            ) {
                wanted.push(own);
            }
        }
    }
    // The rows of the reaction picker that are on screen: the custom emojis it offers.
    if let Some(view) = crate::app::react::view(app) {
        let from = view.selected.saturating_sub(15);
        for candidate in view.candidates.iter().skip(from).take(40) {
            if let Some(crate::ui::emoji::Found::Custom(own)) = crate::ui::emoji::lookup_found(
                &candidate.name,
                &app.settings.emoji_aliases,
                &app.custom_emojis,
            ) {
                wanted.push(own);
            }
        }
    }
    for name in wanted {
        let Some(emoji) = app.custom_emojis.find(&name) else {
            continue;
        };
        let key = (emoji.name.clone(), emoji.etag.clone());
        if app.images.requested.contains(&key) {
            continue;
        }
        app.images.requested.insert(key);
        if !matches!(
            emoji.ext.as_str(),
            "png" | "gif" | "webp" | "jpeg" | "jpg" | "ico"
        ) {
            // A format the client does not decode: the fallback, without a request.
            app.images.state.insert(
                emoji.name.clone(),
                ImageState::Failed {
                    etag: emoji.etag.clone(),
                },
            );
            continue;
        }
        app.images.queue.push_back(EmojiRef {
            name: emoji.name.clone(),
            etag: emoji.etag.clone(),
            ext: emoji.ext.clone(),
        });
    }
    pump(app);
}

/// The names of the custom emojis a message body draws as emoji (code spans and unknown names
/// excluded, exactly as `draw_shortcodes` decides).
fn names_in_body(body: &str, app: &App, out: &mut Vec<String>) {
    if !body.contains(':') {
        return;
    }
    let clean = crate::ui::sanitize::sanitize(body);
    // Drawn as names, whatever the images are doing, so every custom emoji shows between its colons.
    let view = EmojiView {
        custom: &app.custom_emojis,
        images: &NO_IMAGES,
        graphics: Graphics::Unsupported,
        now_ms: 0,
    };
    let drawn = crate::ui::emoji::draw_shortcodes(&clean, &app.settings.emoji_aliases, &view);
    let mut rest: &str = &drawn;
    while let Some(open) = rest.find(crate::ui::emoji::OPEN) {
        let after = &rest[open + crate::ui::emoji::OPEN.len_utf8()..];
        let Some(close) = after.find(crate::ui::emoji::CLOSE) else {
            break;
        };
        out.push(after[..close].to_string());
        rest = &after[close + crate::ui::emoji::CLOSE.len_utf8()..];
    }
}

/// Start queued fetches while there is a free slot.
fn pump(app: &mut App) {
    while app.images.in_flight < MAX_IN_FLIGHT {
        let Some(next) = app.images.queue.pop_front() else {
            break;
        };
        app.images.in_flight += 1;
        app.effects.push(Effect::FetchEmojiImage(next));
    }
}

/// An image arrived, decoded. An answer for an etag the list no longer shows is dropped.
pub fn on_image_ready(app: &mut App, emoji: EmojiRef, image: Rgba) {
    app.images.in_flight = app.images.in_flight.saturating_sub(1);
    if app.custom_emojis.find(&emoji.name).map(|e| e.etag.as_str()) == Some(emoji.etag.as_str())
        || !app.images.state.contains_key(&emoji.name)
    {
        // A frame is an image of its own, so an animated emoji takes an id for each.
        let frames = image.frame_count();
        let state = if app.images.last_id + frames <= MAX_IMAGE_ID {
            let id = app.images.last_id + 1;
            app.images.last_id += frames;
            ImageState::Ready {
                etag: emoji.etag,
                image,
                id,
                uploaded: false,
            }
        } else {
            ImageState::Failed { etag: emoji.etag }
        };
        app.images.state.insert(emoji.name, state);
    }
    pump(app);
}

/// An image could not be fetched or decoded: final for the session, so the emoji is drawn as its
/// name from now on.
pub fn on_image_failed(app: &mut App, emoji: EmojiRef) {
    app.images.in_flight = app.images.in_flight.saturating_sub(1);
    app.images
        .state
        .insert(emoji.name, ImageState::Failed { etag: emoji.etag });
    pump(app);
}

/// The custom emojis arrived.
pub fn on_loaded(app: &mut App, list: Vec<CustomEmoji>) {
    app.custom_emojis = CustomEmojis::Loaded(list);
}

/// The custom emojis could not be fetched: the session goes on and the next room opening asks
/// again.
pub fn on_failed(app: &mut App) {
    app.custom_emojis = CustomEmojis::NotLoaded;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::update::update;
    use crate::data::Room;
    use ratatui::layout::Size;

    fn custom(name: &str, aliases: &[&str]) -> CustomEmoji {
        CustomEmoji {
            name: name.to_string(),
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
            ..Default::default()
        }
    }

    fn rooms() -> Vec<Room> {
        crate::data::fake::seed()
            .into_iter()
            .find_map(|a| match a {
                Action::RoomsLoaded(r) => Some(r),
                _ => None,
            })
            .expect("the fake seed has rooms")
    }

    fn requests(app: &App) -> usize {
        app.effects
            .iter()
            .filter(|e| **e == Effect::LoadCustomEmojis)
            .count()
    }

    #[test]
    fn the_list_is_requested_once_when_the_rooms_have_loaded() {
        let mut app = App::new(Size::new(80, 24));
        update(&mut app, Action::RoomsLoaded(rooms()));
        assert_eq!(requests(&app), 1);
        assert_eq!(app.custom_emojis, CustomEmojis::Loading);
        // Rooms arriving again, or a room opening, while the request is out: no second one.
        update(&mut app, Action::RoomsLoaded(rooms()));
        let id = app.rooms[0].id.clone();
        app.open_room(id);
        assert_eq!(requests(&app), 1);
    }

    #[test]
    fn a_failed_list_is_requested_again_when_a_room_opens() {
        let mut app = App::new(Size::new(80, 24));
        update(&mut app, Action::RoomsLoaded(rooms()));
        update(&mut app, Action::CustomEmojisFailed);
        assert_eq!(app.custom_emojis, CustomEmojis::NotLoaded);
        app.effects.clear();
        let id = app.rooms[0].id.clone();
        app.open_room(id);
        assert_eq!(requests(&app), 1);
        assert!(app.status_msg.is_none(), "a failed list shows nothing");
    }

    #[test]
    fn a_loaded_list_is_not_requested_again() {
        let mut app = App::new(Size::new(80, 24));
        update(&mut app, Action::RoomsLoaded(rooms()));
        update(&mut app, Action::CustomEmojisLoaded(vec![custom("a", &[])]));
        app.effects.clear();
        let id = app.rooms[0].id.clone();
        app.open_room(id);
        update(&mut app, Action::RoomsLoaded(rooms()));
        assert_eq!(requests(&app), 0);
    }

    #[test]
    fn find_takes_the_name_first_then_an_alias() {
        let list = CustomEmojis::Loaded(vec![custom("ship_it", &["ship"]), custom("ship", &[])]);
        assert_eq!(list.find("ship").map(|e| e.name.as_str()), Some("ship"));
        assert_eq!(
            list.find("ship_it").map(|e| e.name.as_str()),
            Some("ship_it")
        );
        assert!(list.find("nope").is_none());
        assert!(CustomEmojis::Loading.find("ship").is_none());
    }

    // ---- images

    use crate::data::Reaction;
    use crate::test_util::{app_80x24, sample_message};

    fn listed(names: &[&str]) -> CustomEmojis {
        CustomEmojis::Loaded(
            names
                .iter()
                .map(|n| CustomEmoji {
                    name: n.to_string(),
                    ext: "png".to_string(),
                    etag: format!("e-{n}"),
                    ..Default::default()
                })
                .collect(),
        )
    }

    /// An app on a room that holds `bodies`, with images allowed and `names` listed.
    fn room_app(names: &[&str], bodies: &[&str]) -> App {
        let mut app = app_80x24();
        app.graphics = Graphics::Supported;
        app.custom_emojis = listed(names);
        let room = app.rooms[0].id.clone();
        let msgs = bodies
            .iter()
            .enumerate()
            .map(|(i, body)| crate::data::Message {
                room_id: room.clone(),
                body: body.to_string(),
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
                Effect::FetchEmojiImage(r) => Some(r.name.clone()),
                _ => None,
            })
            .collect()
    }

    fn tick(app: &mut App) {
        update(app, Action::Clock(0));
    }

    fn done(name: &str) -> EmojiRef {
        EmojiRef {
            name: name.to_string(),
            etag: format!("e-{name}"),
            ext: "png".to_string(),
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
    fn an_emoji_used_five_times_is_requested_once() {
        let mut app = room_app(
            &["rtshipit"],
            &[
                ":rtshipit: :rtshipit:",
                "again :rtshipit: and :rtshipit:",
                ":rtshipit:",
            ],
        );
        app.messages.get_mut(&app.rooms[0].id.clone()).unwrap()[0].reactions = vec![Reaction {
            name: "rtshipit".to_string(),
            users: vec!["a".to_string()],
        }];
        tick(&mut app);
        assert_eq!(fetches(&app), ["rtshipit"]);
        // Later actions, a reload of the list and the answer itself ask for nothing more.
        tick(&mut app);
        update(
            &mut app,
            Action::CustomEmojisLoaded(match listed(&["rtshipit"]) {
                CustomEmojis::Loaded(l) => l,
                _ => unreachable!(),
            }),
        );
        update(
            &mut app,
            Action::EmojiImageReady {
                emoji: done("rtshipit"),
                image: pixel(),
            },
        );
        assert_eq!(fetches(&app), ["rtshipit"]);
        assert!(matches!(
            app.images.state("rtshipit"),
            Some(ImageState::Ready { .. })
        ));
    }

    #[test]
    fn an_emoji_nothing_draws_is_not_requested() {
        let mut app = room_app(
            &["rtshipit", "unused", "tada"],
            &["`:unused:` in code, :tada: is unicode, :unlisted: is unknown, plain"],
        );
        tick(&mut app);
        assert!(fetches(&app).is_empty(), "{:?}", app.effects);
        // A message in another room does not count either.
        let other = app.rooms[1].id.clone();
        app.messages.insert(
            other,
            vec![crate::data::Message {
                body: ":rtshipit:".to_string(),
                ..sample_message("o1")
            }],
        );
        tick(&mut app);
        assert!(fetches(&app).is_empty());
    }

    #[test]
    fn nothing_is_requested_on_a_terminal_without_images_or_before_the_list() {
        let mut app = room_app(&["rtshipit"], &[":rtshipit:"]);
        app.graphics = Graphics::Unsupported;
        tick(&mut app);
        assert!(fetches(&app).is_empty());
        app.graphics = Graphics::Supported;
        app.custom_emojis = CustomEmojis::Loading;
        tick(&mut app);
        assert!(fetches(&app).is_empty());
        app.custom_emojis = listed(&["rtshipit"]);
        tick(&mut app);
        assert_eq!(fetches(&app), ["rtshipit"]);
    }

    #[test]
    fn more_images_than_slots_wait_and_none_is_dropped() {
        let names: Vec<String> = (0..10).map(|n| format!("e{n}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let body = names
            .iter()
            .map(|n| format!(":{n}:"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut app = room_app(&refs, &[&body]);
        tick(&mut app);
        assert_eq!(fetches(&app).len(), MAX_IN_FLIGHT);
        assert_eq!(app.images.in_flight(), MAX_IN_FLIGHT);
        assert_eq!(app.images.queued(), 10 - MAX_IN_FLIGHT);
        // Answers free slots one by one, a failure as well as a success, until all have run.
        let mut seen: Vec<String> = fetches(&app);
        let mut next = 0;
        while next < seen.len() {
            let name = seen[next].clone();
            app.effects.clear();
            if next % 2 == 0 {
                update(
                    &mut app,
                    Action::EmojiImageReady {
                        emoji: done(&name),
                        image: pixel(),
                    },
                );
            } else {
                update(&mut app, Action::EmojiImageFailed(done(&name)));
            }
            assert!(app.images.in_flight() <= MAX_IN_FLIGHT);
            seen.extend(fetches(&app));
            next += 1;
        }
        seen.sort();
        let mut want = names.clone();
        want.sort();
        assert_eq!(seen, want, "every emoji asked for exactly once");
        assert_eq!(app.images.in_flight(), 0);
        assert_eq!(app.images.queued(), 0);
    }

    #[test]
    fn a_failed_image_is_final_but_a_changed_etag_is_asked_for_again() {
        let mut app = room_app(&["rtshipit"], &[":rtshipit:"]);
        tick(&mut app);
        update(&mut app, Action::EmojiImageFailed(done("rtshipit")));
        assert!(matches!(
            app.images.state("rtshipit"),
            Some(ImageState::Failed { .. })
        ));
        app.effects.clear();
        tick(&mut app);
        assert!(
            fetches(&app).is_empty(),
            "a failure is final for the session"
        );
        // The list shows a new etag: that is a different image.
        app.custom_emojis = CustomEmojis::Loaded(vec![CustomEmoji {
            name: "rtshipit".to_string(),
            ext: "png".to_string(),
            etag: "newer".to_string(),
            ..Default::default()
        }]);
        tick(&mut app);
        assert_eq!(fetches(&app), ["rtshipit"]);
    }

    #[test]
    fn a_format_the_client_does_not_decode_is_never_requested() {
        let mut app = room_app(&["vec"], &[":vec:"]);
        app.custom_emojis = CustomEmojis::Loaded(vec![CustomEmoji {
            name: "vec".to_string(),
            ext: "svg".to_string(),
            etag: "x".to_string(),
            ..Default::default()
        }]);
        tick(&mut app);
        assert!(fetches(&app).is_empty());
        assert!(matches!(
            app.images.state("vec"),
            Some(ImageState::Failed { .. })
        ));
    }

    #[test]
    fn five_uses_of_one_emoji_upload_it_once() {
        let mut app = room_app(
            &["rtshipit"],
            &[":rtshipit: :rtshipit: :rtshipit:", ":rtshipit: :rtshipit:"],
        );
        tick(&mut app);
        assert!(
            app.images.take_uploads().is_empty(),
            "nothing has arrived yet"
        );
        update(
            &mut app,
            Action::EmojiImageReady {
                emoji: done("rtshipit"),
                image: pixel(),
            },
        );
        // The emoji is not drawn as an image until the terminal has it.
        fn look(app: &App) -> Look {
            EmojiView {
                custom: &app.custom_emojis,
                images: &app.images,
                graphics: app.graphics,
                now_ms: 0,
            }
            .look("rtshipit")
        }
        assert_eq!(look(&app), Look::Standin);
        let uploads = app.images.take_uploads();
        assert_eq!(uploads.len(), 1);
        assert_eq!(uploads[0].id, 1, "the first id handed out");
        assert_eq!(look(&app), Look::Image(1));
        // Later actions and a second take upload nothing more.
        tick(&mut app);
        assert!(app.images.take_uploads().is_empty());
    }

    #[test]
    fn look_follows_the_terminal_and_the_image() {
        let mut app = room_app(&["rtshipit"], &[":rtshipit:"]);
        let look = |app: &App| {
            EmojiView {
                custom: &app.custom_emojis,
                images: &app.images,
                graphics: app.graphics,
                now_ms: 0,
            }
            .look("rtshipit")
        };
        assert_eq!(look(&app), Look::Standin, "images can still arrive");
        app.graphics = Graphics::Unsupported;
        assert_eq!(look(&app), Look::Text);
        app.graphics = Graphics::Supported;
        tick(&mut app);
        update(&mut app, Action::EmojiImageFailed(done("rtshipit")));
        assert_eq!(look(&app), Look::Text, "a failure is final");
    }

    #[test]
    fn image_cells_round_trip_and_stay_in_the_reserved_range() {
        for id in [1, 2, 0x100, MAX_IMAGE_ID] {
            assert_eq!(cell_image(image_cell(id)), Some(id));
        }
        assert_eq!(cell_image('a'), None);
        assert_eq!(cell_image(crate::ui::emoji::OPEN), None);
        assert_eq!(cell_image(crate::ui::emoji::STANDIN), None);
        assert_eq!(cell_image('\u{10EEEE}'), None);
    }

    fn animated() -> Rgba {
        let frame = |v: u8, ms: u32| crate::emoji_image::Frame {
            pixels: vec![v; 4].into(),
            delay_ms: ms,
        };
        Rgba {
            width: 1,
            height: 1,
            pixels: vec![0; 4].into(),
            delay_ms: 100,
            later: vec![frame(1, 50), frame(2, 150)].into(),
        }
    }

    #[test]
    fn an_animated_emoji_takes_an_id_per_frame_and_uploads_every_frame() {
        let mut app = room_app(&["rtspin", "rtstill"], &[":rtspin: :rtstill:"]);
        tick(&mut app);
        update(
            &mut app,
            Action::EmojiImageReady {
                emoji: done("rtspin"),
                image: animated(),
            },
        );
        update(
            &mut app,
            Action::EmojiImageReady {
                emoji: done("rtstill"),
                image: pixel(),
            },
        );
        assert!(
            !app.images.has_animation(),
            "nothing is on screen before the upload"
        );
        let uploads = app.images.take_uploads();
        let ids: Vec<u32> = uploads.iter().map(|u| u.id).collect();
        assert_eq!(ids, [1, 2, 3, 4], "three frames, then the still one");
        assert_eq!(uploads[1].pixels[0], 1, "frame order is id order");
        assert!(app.images.has_animation());
        assert!(app.images.take_uploads().is_empty());
    }

    #[test]
    fn the_cells_carry_the_id_of_the_frame_that_is_showing() {
        let mut app = room_app(&["rtspin"], &[":rtspin:"]);
        tick(&mut app);
        update(
            &mut app,
            Action::EmojiImageReady {
                emoji: done("rtspin"),
                image: animated(),
            },
        );
        app.images.take_uploads();
        let look_at = |app: &mut App, ms: u64| {
            app.anim_ms = ms;
            app.emoji_view().look("rtspin")
        };
        // Delays 100, 50, 150: frames 0 for 0..100, 1 for 100..150, 2 for 150..300, then again.
        assert_eq!(look_at(&mut app, 0), Look::Image(1));
        assert_eq!(look_at(&mut app, 99), Look::Image(1));
        assert_eq!(look_at(&mut app, 100), Look::Image(2));
        assert_eq!(look_at(&mut app, 150), Look::Image(3));
        assert_eq!(look_at(&mut app, 300), Look::Image(1));
    }

    #[test]
    fn a_still_emoji_does_not_ask_for_redraws() {
        let mut app = room_app(&["rtshipit"], &[":rtshipit:"]);
        tick(&mut app);
        update(
            &mut app,
            Action::EmojiImageReady {
                emoji: done("rtshipit"),
                image: pixel(),
            },
        );
        app.images.take_uploads();
        assert!(!app.images.has_animation());
    }
}

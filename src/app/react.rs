//! The reaction picker (`react_message`, `+`): react to the message under the cursor, or take a
//! reaction back, with a search over unicode emoji, aliases and the server's custom emojis.
//!
//! This module holds what the picker keeps between openings and what answers do to it. The
//! picker itself is state on `App`, not a mode; see the design of the change `add-reaction-picker`.
//! Pure: no I/O and no `Frame`.

use crate::api::ApiError;
use crate::app::{App, Focus, MAX_CMDLINE, Mode, StatusMsg, jump, messages, send};
use crate::data::{CustomEmoji, Message, MessageKind, RoomId, SendState};
use crate::effect::{Effect, EmojiName};
use crate::input::key::Chord;
use crate::ui::emoji::resolve;
use crate::ui::emoji_names::send_name;
use crate::ui::sanitize::sanitize_line;
use crossterm::event::{KeyCode, KeyModifiers};
use std::collections::HashSet;

/// Most candidates a search returns. A one-letter query matches most of the table; the list a
/// person can usefully scroll is far shorter.
pub const MAX_CANDIDATES: usize = 200;

/// Where a candidate comes from. The order is also the order a name is claimed in: a name given by
/// an alias hides the same name from the server's custom emojis and from the unicode table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// A reaction the message already has (an empty search lists only these).
    Existing,
    /// An entry of `[emoji_aliases]` in config.toml.
    Alias,
    /// One of the server's custom emojis.
    Custom,
    /// A unicode emoji of the `emojis` crate, under the shortcode the server accepts.
    Unicode,
}

/// One row the picker can offer: what to draw for it, the name to send, and how the message
/// already stands with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    /// The name that is sent, without colons. Untrusted text for `Existing` and `Custom`: draw
    /// `shown_name`, never this.
    pub name: String,
    /// What the message row would draw for it: the glyph, an alias, or the raw `:name:`.
    pub glyph: String,
    pub source: Source,
    /// How many users gave this reaction on the message already (0 when none).
    pub count: usize,
    /// True when the user's own username is among them: choosing it takes the reaction back.
    pub own: bool,
}

impl Candidate {
    /// `:name:` sanitized for the terminal.
    pub fn shown_name(&self) -> String {
        format!(":{}:", sanitize_line(&self.name))
    }
}

/// True for a message the picker can react to: one the server holds that is neither a deleted
/// line nor a system message. A message still being sent, or that failed, is not on the server.
pub fn reactable(message: &Message) -> bool {
    message.send == SendState::Sent && message.kind == MessageKind::Normal
}

/// True when `own` is among the users who gave reaction `name` on `message`: the reaction then goes
/// back (`shouldReact` false), otherwise it is added.
pub fn is_given_by(message: &Message, own: &str, name: &str) -> bool {
    message
        .reactions
        .iter()
        .any(|r| r.name == name && r.users.iter().any(|u| u == own))
}

/// The search text as the matcher uses it: no leading or trailing colon, lower case.
fn needle(query: &str) -> String {
    query.trim().trim_matches(':').to_lowercase()
}

/// How well `query` matches `name`: 0 exact, 1 prefix, 2 anywhere, `None` not at all. `name` is
/// compared in lower case.
fn rank_of(name: &str, query: &str) -> Option<u8> {
    let name = name.to_lowercase();
    if name == query {
        Some(0)
    } else if name.starts_with(query) {
        Some(1)
    } else if name.contains(query) {
        Some(2)
    } else {
        None
    }
}

/// The best rank among several names of one emoji.
fn best_rank<'a>(names: impl IntoIterator<Item = &'a str>, query: &str) -> Option<u8> {
    names.into_iter().filter_map(|n| rank_of(n, query)).min()
}

/// The reactions of `message` as candidates, in the order of its reaction row.
fn existing(app: &App, message: &Message) -> Vec<Candidate> {
    let me = app.own_username.as_deref();
    message
        .reactions
        .iter()
        .map(|r| Candidate {
            name: r.name.clone(),
            glyph: resolve(&r.name, &app.settings.emoji_aliases),
            source: Source::Existing,
            count: r.users.len(),
            own: me.is_some_and(|me| r.users.iter().any(|u| u == me)),
        })
        .collect()
}

/// What the picker lists for `query` on `message`.
///
/// An empty search (colons and spaces aside) lists the message's own reactions in the order of its
/// row. Otherwise it searches the aliases, the custom emojis (by name and by alias) and the unicode
/// emoji (by every shortcode, and by the name that is sent), each name appearing once and under the
/// name that is sent. Candidates are ranked exact, then prefix, then anywhere, ties on the shorter
/// name and then alphabetically, and cut at `MAX_CANDIDATES`. Each candidate carries the message's
/// current count and own flag for its name, so choosing one the user already gave is a take-back.
pub fn candidates(app: &App, message: &Message, query: &str) -> Vec<Candidate> {
    let query = needle(query);
    if query.is_empty() {
        return existing(app, message);
    }
    let me = app.own_username.as_deref();
    let mark = |name: &str| -> (usize, bool) {
        message
            .reactions
            .iter()
            .find(|r| r.name == name)
            .map_or((0, false), |r| {
                (
                    r.users.len(),
                    me.is_some_and(|me| r.users.iter().any(|u| u == me)),
                )
            })
    };
    let aliases = &app.settings.emoji_aliases;
    let mut claimed: HashSet<String> = HashSet::new();
    let mut found: Vec<(u8, Candidate)> = Vec::new();
    let mut offer = |rank: u8, name: String, glyph: String, source: Source| {
        if claimed.insert(name.clone()) {
            let (count, own) = mark(&name);
            found.push((
                rank,
                Candidate {
                    name,
                    glyph,
                    source,
                    count,
                    own,
                },
            ));
        }
    };

    for (name, text) in aliases {
        if let Some(rank) = rank_of(name, &query) {
            offer(rank, name.clone(), text.clone(), Source::Alias);
        }
    }
    for emoji in app.custom_emojis.list() {
        // A name the terminal could not draw safely, or the server could not have meant, is not
        // offered.
        if sanitize_line(&emoji.name) != emoji.name.as_str() {
            continue;
        }
        let names =
            std::iter::once(emoji.name.as_str()).chain(emoji.aliases.iter().map(String::as_str));
        if let Some(rank) = best_rank(names, &query) {
            let glyph = resolve(&emoji.name, aliases);
            offer(rank, emoji.name.clone(), glyph, Source::Custom);
        }
    }
    for emoji in emojis::iter() {
        let Some(first) = emoji.shortcodes().next() else {
            continue;
        };
        let Some(sent) = send_name(first) else {
            continue;
        };
        let names = emoji.shortcodes().chain(std::iter::once(sent));
        if let Some(rank) = best_rank(names, &query) {
            offer(
                rank,
                sent.to_string(),
                emoji.as_str().to_string(),
                Source::Unicode,
            );
        }
    }

    found.sort_by(|(ra, a), (rb, b)| {
        ra.cmp(rb)
            .then_with(|| a.name.len().cmp(&b.name.len()))
            .then_with(|| a.name.cmp(&b.name))
    });
    found
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, candidate)| candidate)
        .collect()
}

/// The server's custom emojis as the picker knows them. Fetched once per session, on the first
/// opening; a failed fetch goes back to `NotLoaded` so the next opening asks again.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CustomEmojis {
    #[default]
    NotLoaded,
    Loading,
    Loaded(Vec<CustomEmoji>),
}

impl CustomEmojis {
    /// The emojis to search, none until they have arrived.
    pub fn list(&self) -> &[CustomEmoji] {
        match self {
            CustomEmojis::Loaded(list) => list,
            _ => &[],
        }
    }
}

/// True when the messages pane is focused on a message the picker could react to: what the context
/// popup checks before it lists the react action. Does not look at the detached state or the own
/// username, which `open` refuses with a status line of their own.
pub fn reactable_under_cursor(app: &App) -> bool {
    if app.mode != Mode::Normal || app.focus != Focus::Messages {
        return false;
    }
    let Some(room) = app.current_room.as_ref() else {
        return false;
    };
    app.messages
        .get(room)
        .and_then(|held| held.get(messages::view(app, room).cursor))
        .is_some_and(reactable)
}

/// What the status line says when the picker is refused because the own username has not arrived:
/// without it the app cannot tell whether choosing a reaction adds it or takes it back.
const NO_USERNAME: &str = "Username not loaded yet: try again in a moment";

/// The open picker: the message it was opened on (by room and id, never a copy of its reactions),
/// what has been typed and which result is selected. Everything shown is derived from these and
/// the app on every draw, so a live change shows up without any plumbing.
#[derive(Clone, PartialEq, Eq)]
pub struct ReactPicker {
    pub room: RoomId,
    pub id: String,
    pub query: String,
    pub selected: usize,
    /// True while searching, false while browsing. The picker opens browsing, as a vim buffer
    /// opens in Normal mode: keys move and choose, and only `/`, `i` and `a` start typing.
    pub typing: bool,
}

/// Redacting `Debug`: the typed search text is never printed, only its length.
impl std::fmt::Debug for ReactPicker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReactPicker")
            .field("room", &self.room)
            .field("id", &self.id)
            .field("query_len", &self.query.chars().count())
            .field("selected", &self.selected)
            .field("typing", &self.typing)
            .finish()
    }
}

/// What the drawer shows for the open picker, derived from the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub query: String,
    pub candidates: Vec<Candidate>,
    /// Index into `candidates`, always in range when there are any.
    pub selected: usize,
    /// True while searching (the search line is live input), false while browsing.
    pub typing: bool,
}

/// The message the picker was opened on, while the room still holds it.
fn held<'a>(app: &'a App, picker: &ReactPicker) -> Option<&'a Message> {
    app.messages
        .get(&picker.room)?
        .iter()
        .find(|m| m.id == picker.id)
}

/// The picker's content now, `None` when it is closed or its message is gone.
pub fn view(app: &App) -> Option<View> {
    let picker = app.react_picker.as_ref()?;
    let message = held(app, picker)?;
    let candidates = candidates(app, message, &picker.query);
    let selected = picker.selected.min(candidates.len().saturating_sub(1));
    Some(View {
        query: picker.query.clone(),
        candidates,
        selected,
        typing: picker.typing,
    })
}

/// `react_message`: open the picker for the message under the cursor.
///
/// A silent no-op unless the messages pane is focused on a message the server holds that is not a
/// deleted line or a system message. Then, in this order: a detached room is refused with its
/// status line, and so is an unknown own username. On the first opening of a session the custom
/// emojis are asked for.
pub fn open(app: &mut App) {
    if app.mode != Mode::Normal || app.focus != Focus::Messages {
        return;
    }
    let Some(room) = app.current_room.clone() else {
        return;
    };
    let Some(message) = app
        .messages
        .get(&room)
        .and_then(|held| held.get(messages::view(app, &room).cursor))
    else {
        return;
    };
    if !reactable(message) {
        return;
    }
    let id = message.id.clone();
    if jump::refuse_if_detached(app) {
        return;
    }
    if app.own_username.is_none() {
        app.status_msg = Some(StatusMsg::Error(NO_USERNAME.to_string()));
        return;
    }
    app.matcher.reset();
    if app.custom_emojis == CustomEmojis::NotLoaded {
        app.custom_emojis = CustomEmojis::Loading;
        app.effects.push(Effect::LoadCustomEmojis);
    }
    app.react_picker = Some(ReactPicker {
        room,
        id,
        query: String::new(),
        selected: 0,
        typing: false,
    });
}

/// A key while the picker is open: it never reaches the keymap. `<C-c>` always closes it, so
/// nothing can lock the user in. What the other keys do depends on the state: `browse_key` or
/// `search_key`.
pub fn key(app: &mut App, chord: Chord) {
    if chord.code == KeyCode::Char('c') && chord.mods == KeyModifiers::CONTROL {
        app.react_picker = None;
        return;
    }
    let Some(typing) = app.react_picker.as_ref().map(|p| p.typing) else {
        return;
    };
    if typing {
        search_key(app, chord);
    } else {
        browse_key(app, chord);
    }
}

/// True for the keys that move the selection, whatever the state: `<C-n>`, `<C-p>` and the arrows.
/// `Some(true)` is forward (down).
fn move_key(chord: Chord) -> Option<bool> {
    let ctrl = |c: char| chord.code == KeyCode::Char(c) && chord.mods == KeyModifiers::CONTROL;
    if ctrl('n') || chord.code == KeyCode::Down {
        Some(true)
    } else if ctrl('p') || chord.code == KeyCode::Up {
        Some(false)
    } else {
        None
    }
}

/// A key while browsing: `j`, `k`, `<C-n>`, `<C-p>` and the arrows move, `Enter` sends, `q` and
/// `Esc` close, `/` starts a new search, `i` and `a` continue the last one. Nothing else does
/// anything.
fn browse_key(app: &mut App, chord: Chord) {
    let plain = |c: char| chord.code == KeyCode::Char(c) && chord.mods.is_empty();
    if let Some(forward) = move_key(chord) {
        move_selection(app, forward);
    } else if plain('j') {
        move_selection(app, true);
    } else if plain('k') {
        move_selection(app, false);
    } else if chord.code == KeyCode::Enter {
        send_selected(app);
    } else if plain('q') || (chord.code == KeyCode::Esc && chord.mods.is_empty()) {
        app.react_picker = None;
    } else if plain('/') {
        if let Some(picker) = app.react_picker.as_mut() {
            picker.query.clear();
            picker.selected = 0;
            picker.typing = true;
        }
    } else if (plain('i') || plain('a'))
        && let Some(picker) = app.react_picker.as_mut()
    {
        picker.typing = true;
    }
}

/// A key while searching: characters are text (so `j` and `k` are typed), `<C-n>`, `<C-p>` and the
/// arrows move, `Enter` sends, `Esc` goes back to browsing with the results kept, and `Backspace`
/// deletes a character or, on an empty search, goes back to browsing.
fn search_key(app: &mut App, chord: Chord) {
    if let Some(forward) = move_key(chord) {
        move_selection(app, forward);
    } else if chord.code == KeyCode::Enter {
        send_selected(app);
    } else if chord.code == KeyCode::Esc {
        if let Some(picker) = app.react_picker.as_mut() {
            picker.typing = false;
        }
    } else if chord.code == KeyCode::Backspace {
        let Some(picker) = app.react_picker.as_mut() else {
            return;
        };
        if picker.query.pop().is_none() {
            picker.typing = false;
        } else {
            picker.selected = 0;
        }
    } else if let KeyCode::Char(c) = chord.code
        && chord.mods.is_empty()
        && !c.is_control()
        && let Some(picker) = app.react_picker.as_mut()
        && picker.query.chars().count() < MAX_CMDLINE
    {
        picker.query.push(c);
        picker.selected = 0;
    }
}

/// A paste while searching: one line, capped, control characters removed. While browsing there is
/// no text to paste into, so a paste does nothing.
pub fn paste(app: &mut App, text: &str) {
    if let Some(picker) = app.react_picker.as_mut()
        && picker.typing
    {
        let room = MAX_CMDLINE.saturating_sub(picker.query.chars().count());
        picker.query.extend(sanitize_line(text).chars().take(room));
        picker.selected = 0;
    }
}

/// One result down (`forward`) or up, stopping at either end.
fn move_selection(app: &mut App, forward: bool) {
    let Some(view) = view(app) else {
        return;
    };
    let last = view.candidates.len().saturating_sub(1);
    if let Some(picker) = app.react_picker.as_mut() {
        picker.selected = if forward {
            (view.selected + 1).min(last)
        } else {
            view.selected.saturating_sub(1)
        };
    }
}

/// `Enter`: send the selected reaction, adding it, or taking it back when the user's own name is
/// already among those who gave it on this message, and close the picker. With nothing to select
/// nothing is sent and the picker stays open. The row changes from the message the server streams
/// back, not from a local guess.
fn send_selected(app: &mut App) {
    let Some(view) = view(app) else {
        app.react_picker = None;
        return;
    };
    let Some(candidate) = view.candidates.get(view.selected) else {
        return;
    };
    let Some(picker) = app.react_picker.clone() else {
        return;
    };
    // The room may have become detached while the picker was open.
    if jump::refuse_if_detached(app) {
        app.react_picker = None;
        return;
    }
    let Some(own) = app.own_username.clone() else {
        app.react_picker = None;
        return;
    };
    let Some(message) = held(app, &picker) else {
        app.react_picker = None;
        return;
    };
    let add = !is_given_by(message, &own, &candidate.name);
    app.effects.push(Effect::React {
        id: picker.id,
        emoji: EmojiName::new(candidate.name.clone()),
        add,
    });
    app.react_picker = None;
}

/// Keep the open picker in step with the room: close it when its message is gone or can no longer
/// be reacted to (deleted), and keep the selection inside a list that shrank. The reducer calls
/// this after every action, so a live change shows at once.
pub fn close_if_stale(app: &mut App) {
    let Some(picker) = app.react_picker.as_ref() else {
        return;
    };
    let alive = held(app, picker).is_some_and(reactable);
    if !alive {
        app.react_picker = None;
    } else if let Some(view) = view(app)
        && let Some(picker) = app.react_picker.as_mut()
    {
        picker.selected = view.selected;
    }
}

/// The custom emojis arrived.
pub fn on_custom_emojis_loaded(app: &mut App, list: Vec<CustomEmoji>) {
    app.custom_emojis = CustomEmojis::Loaded(list);
}

/// The custom emojis could not be fetched: the picker goes on with unicode emoji and aliases, and
/// the next opening asks again. Nothing is shown; the picker still works.
pub fn on_custom_emojis_failed(app: &mut App) {
    app.custom_emojis = CustomEmojis::NotLoaded;
}

/// A reaction was not sent: one fixed status line naming the failure class, never the server's
/// text, the shortcode or the search text. The message is left as it was.
pub fn on_react_failed(app: &mut App, error: ApiError) {
    app.status_msg = Some(StatusMsg::Error(failure_text(error)));
}

/// `reaction not sent: <reason>` or `reaction may not have been sent: <reason>`, one fixed reason
/// per error kind.
fn failure_text(error: ApiError) -> String {
    let lead = if error.outcome_unknown() {
        "reaction may not have been sent"
    } else {
        "reaction not sent"
    };
    let reason = match error {
        ApiError::Rejected => "refused by the server (unknown emoji?)".to_string(),
        ApiError::Forbidden => "not allowed".to_string(),
        other => send::error_reason(other),
    };
    format!("{lead}: {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::update::update;
    use crate::test_util::app_80x24;

    fn status(app: &App) -> String {
        match &app.status_msg {
            Some(StatusMsg::Error(text)) | Some(StatusMsg::Info(text)) => text.clone(),
            None => String::new(),
        }
    }

    #[test]
    fn a_failed_reaction_shows_one_fixed_line_per_kind() {
        let mut app = app_80x24();
        update(
            &mut app,
            Action::ReactFailed {
                error: ApiError::Rejected,
            },
        );
        let refused = status(&app);
        assert!(refused.starts_with("reaction not sent: "), "{refused}");
        assert!(refused.contains("refused by the server"), "{refused}");
        assert!(matches!(app.status_msg, Some(StatusMsg::Error(_))));

        update(
            &mut app,
            Action::ReactFailed {
                error: ApiError::Forbidden,
            },
        );
        assert_eq!(status(&app), "reaction not sent: not allowed");

        // An outcome the client cannot know says so.
        update(
            &mut app,
            Action::ReactFailed {
                error: ApiError::ConnectionLost,
            },
        );
        assert!(
            status(&app).starts_with("reaction may not have been sent: "),
            "{}",
            status(&app)
        );
        update(
            &mut app,
            Action::ReactFailed {
                error: ApiError::Timeout,
            },
        );
        assert!(status(&app).starts_with("reaction may not have been sent: "));
    }

    #[test]
    fn the_refusal_line_is_short_enough_to_show_whole_in_the_status_row() {
        // The status row is 80 columns and the mode badge and room name take about 20 of them.
        for error in [
            ApiError::Rejected,
            ApiError::Forbidden,
            ApiError::ConnectionLost,
        ] {
            let text = failure_text(error);
            assert!(
                text.chars().count() <= 60,
                "{} chars: {text}",
                text.chars().count()
            );
        }
    }

    #[test]
    fn the_failure_line_never_holds_what_the_user_typed() {
        // The line is built from the error kind alone, so no name can get into it.
        for error in [
            ApiError::Rejected,
            ApiError::Forbidden,
            ApiError::NotFound,
            ApiError::Unauthorized,
            ApiError::Timeout,
            ApiError::Server(502),
            ApiError::ConnectionLost,
            ApiError::Client,
        ] {
            let text = failure_text(error);
            assert!(!text.chars().any(char::is_control), "{text}");
            assert!(text.starts_with("reaction "), "{text}");
        }
    }

    // ---- the search model

    use crate::data::{Message, Reaction};
    use crate::test_util::sample_message;

    fn reaction(name: &str, users: &[&str]) -> Reaction {
        Reaction {
            name: name.to_string(),
            users: users.iter().map(|u| u.to_string()).collect(),
        }
    }

    fn message_with(reactions: Vec<Reaction>) -> Message {
        Message {
            reactions,
            ..sample_message("m1")
        }
    }

    fn custom(name: &str, aliases: &[&str]) -> CustomEmoji {
        CustomEmoji {
            name: name.to_string(),
            aliases: aliases.iter().map(|a| a.to_string()).collect(),
        }
    }

    fn names(found: &[Candidate]) -> Vec<&str> {
        found.iter().map(|c| c.name.as_str()).collect()
    }

    fn app_for_search() -> App {
        let mut app = app_80x24();
        app.own_username = Some("me".to_string());
        app
    }

    #[test]
    fn an_empty_search_lists_the_messages_reactions_in_row_order() {
        let app = app_for_search();
        let message = message_with(vec![
            reaction("+1", &["bob", "me", "ana"]),
            reaction("tada", &["ana"]),
            reaction("rt_unknown", &["zed"]),
        ]);
        for query in ["", "   ", ":", "::"] {
            let found = candidates(&app, &message, query);
            assert_eq!(names(&found), ["+1", "tada", "rt_unknown"], "{query:?}");
            assert!(found.iter().all(|c| c.source == Source::Existing));
        }
        let found = candidates(&app, &message, "");
        assert_eq!(
            (found[0].glyph.as_str(), found[0].count, found[0].own),
            ("👍", 3, true)
        );
        assert_eq!(
            (found[1].glyph.as_str(), found[1].count, found[1].own),
            ("🎉", 1, false)
        );
        // A name that resolves to nothing is drawn raw.
        assert_eq!(found[2].glyph, ":rt_unknown:");
    }

    #[test]
    fn a_reaction_named_with_a_name_the_server_uses_shows_its_emoji_in_the_picker() {
        let app = app_for_search();
        let message = message_with(vec![reaction("flag_af", &["bob"])]);
        let found = candidates(&app, &message, "");
        assert_eq!(
            (found[0].name.as_str(), found[0].glyph.as_str()),
            ("flag_af", "🇦🇫")
        );
        // And a search result for the same emoji, sent under that name, shows its count.
        let found = candidates(&app, &message, "afghanistan");
        let flag = found
            .iter()
            .find(|c| c.name == "flag_af")
            .expect("the flag");
        assert_eq!((flag.glyph.as_str(), flag.count), ("🇦🇫", 1));
    }

    #[test]
    fn an_empty_search_on_a_message_without_reactions_is_empty() {
        let app = app_for_search();
        assert!(candidates(&app, &message_with(Vec::new()), "").is_empty());
    }

    #[test]
    fn unicode_emoji_are_found_by_shortcode_with_the_best_matches_first() {
        let app = app_for_search();
        let found = candidates(&app, &message_with(Vec::new()), "tad");
        assert!(names(&found).contains(&"tada"), "{:?}", names(&found));
        let tada = found.iter().find(|c| c.name == "tada").unwrap();
        assert_eq!((tada.glyph.as_str(), tada.source), ("🎉", Source::Unicode));
        // Everything that starts with the text comes before everything that only contains it.
        let first_infix = found
            .iter()
            .position(|c| !c.name.to_lowercase().starts_with("tad"));
        let last_prefix = found
            .iter()
            .rposition(|c| c.name.to_lowercase().starts_with("tad"));
        if let (Some(infix), Some(prefix)) = (first_infix, last_prefix) {
            assert!(prefix < infix, "prefix matches first: {:?}", names(&found));
        }
        // An exact match beats a longer name that merely starts with the text.
        let exact = candidates(&app, &message_with(Vec::new()), "tada");
        assert_eq!(exact[0].name, "tada");
    }

    #[test]
    fn colons_case_and_spaces_around_the_text_do_not_matter() {
        let app = app_for_search();
        let plain = candidates(&app, &message_with(Vec::new()), "tada");
        for query in [":tada:", ":tada", "tada:", "TADA", " tada "] {
            assert_eq!(
                candidates(&app, &message_with(Vec::new()), query),
                plain,
                "{query:?}"
            );
        }
    }

    #[test]
    fn an_alias_is_found_and_shown_with_its_text() {
        let mut app = app_for_search();
        app.settings
            .emoji_aliases
            .insert("partyparrot".to_string(), "🦜".to_string());
        // `parrot` is also a unicode emoji, so search for text only the alias has.
        let found = candidates(&app, &message_with(Vec::new()), "partyparr");
        assert_eq!(names(&found), ["partyparrot"]);
        assert_eq!(
            (found[0].glyph.as_str(), found[0].source),
            ("🦜", Source::Alias)
        );
    }

    #[test]
    fn a_custom_emoji_is_found_by_name_and_by_alias_and_sent_under_its_name() {
        let mut app = app_for_search();
        app.custom_emojis = CustomEmojis::Loaded(vec![
            custom("ship_it", &["ship", "rt_squirrel"]),
            custom("plain", &[]),
        ]);
        let message = message_with(Vec::new());
        let by_name = candidates(&app, &message, "ship");
        let c = by_name
            .iter()
            .find(|c| c.source == Source::Custom)
            .expect("custom");
        assert_eq!(
            (c.name.as_str(), c.glyph.as_str()),
            ("ship_it", ":ship_it:")
        );
        // By an alias that is not part of the name.
        let by_alias = candidates(&app, &message, "squirrel");
        assert_eq!(names(&by_alias), ["ship_it"]);
        // Nothing before the cache has arrived, and while it is loading.
        for state in [CustomEmojis::NotLoaded, CustomEmojis::Loading] {
            app.custom_emojis = state;
            assert!(candidates(&app, &message, "squirrel").is_empty());
        }
    }

    #[test]
    fn nothing_matches_gives_an_empty_list() {
        let app = app_for_search();
        assert!(candidates(&app, &message_with(Vec::new()), "zzzzqqqq").is_empty());
    }

    #[test]
    fn a_name_appears_once_and_an_alias_wins_over_a_custom_and_a_unicode_name() {
        let mut app = app_for_search();
        app.settings
            .emoji_aliases
            .insert("tada".to_string(), "yay".to_string());
        app.custom_emojis = CustomEmojis::Loaded(vec![custom("tada", &[])]);
        let found = candidates(&app, &message_with(Vec::new()), "tada");
        assert_eq!(found.iter().filter(|c| c.name == "tada").count(), 1);
        let tada = found.iter().find(|c| c.name == "tada").unwrap();
        assert_eq!((tada.source, tada.glyph.as_str()), (Source::Alias, "yay"));
        // A custom emoji beats the unicode name when there is no alias.
        app.settings.emoji_aliases.clear();
        let found = candidates(&app, &message_with(Vec::new()), "tada");
        assert_eq!(
            found.iter().find(|c| c.name == "tada").unwrap().source,
            Source::Custom
        );
    }

    #[test]
    fn ranking_is_exact_then_prefix_then_anywhere_then_shorter_then_alphabetical() {
        let mut app = app_for_search();
        app.custom_emojis = CustomEmojis::Loaded(vec![
            custom("zz_tada", &[]),
            custom("tada_party", &[]),
            custom("tada_a", &[]),
        ]);
        let found = candidates(&app, &message_with(Vec::new()), "tada");
        let order = names(&found);
        let at = |n: &str| {
            order
                .iter()
                .position(|x| *x == n)
                .unwrap_or_else(|| panic!("{n}"))
        };
        assert_eq!(order[0], "tada", "exact first");
        assert!(
            at("tada_a") < at("tada_party"),
            "shorter prefix match first"
        );
        assert!(at("tada_party") < at("zz_tada"), "prefix before infix");
        // The same call gives the same list.
        assert_eq!(found, candidates(&app, &message_with(Vec::new()), "tada"));
    }

    #[test]
    fn a_found_emoji_the_user_already_gave_is_marked_so_choosing_it_takes_it_back() {
        let app = app_for_search();
        let message = message_with(vec![reaction("tada", &["ana", "me"])]);
        let found = candidates(&app, &message, "tada");
        let tada = found.iter().find(|c| c.name == "tada").unwrap();
        assert_eq!((tada.count, tada.own), (2, true));
        let other = found
            .iter()
            .find(|c| c.name != "tada")
            .map(|c| (c.count, c.own));
        assert!(other.is_none_or(|o| o == (0, false)));
        // Given by someone else only: counted, not own.
        let message = message_with(vec![reaction("tada", &["ana"])]);
        let tada = candidates(&app, &message, "tada")
            .into_iter()
            .find(|c| c.name == "tada")
            .unwrap();
        assert_eq!((tada.count, tada.own), (1, false));
    }

    #[test]
    fn unicode_emoji_are_sent_under_the_name_the_server_accepted() {
        let app = app_for_search();
        let message = message_with(Vec::new());
        // Found by the crate's name, sent under the server's (measured on 8.9.1).
        let found = candidates(&app, &message, "afghanistan");
        let flag = found.iter().find(|c| c.glyph == "🇦🇫").expect("the flag");
        assert_eq!(flag.name, "flag_af");
        // And found by the name that is sent.
        assert!(
            candidates(&app, &message, "flag_af")
                .iter()
                .any(|c| c.name == "flag_af")
        );
        // The first shortcode is the name for an emoji the server takes as it is.
        assert_eq!(
            candidates(&app, &message, "+1")
                .iter()
                .find(|c| c.glyph == "👍")
                .map(|c| c.name.as_str()),
            Some("+1")
        );
    }

    #[test]
    fn hostile_names_are_not_offered_or_are_drawn_sanitized() {
        let mut app = app_for_search();
        app.custom_emojis = CustomEmojis::Loaded(vec![
            custom("zzev\u{1b}[31mil", &[]),
            custom("zzev_fine", &[]),
        ]);
        let message = message_with(vec![reaction("bad\u{7}name", &["bob"])]);
        let found = candidates(&app, &message, "zzev");
        assert_eq!(
            names(&found),
            ["zzev_fine"],
            "a name with control characters is not offered"
        );
        // An existing reaction keeps its name for sending but is drawn sanitized.
        let existing = candidates(&app, &message, "");
        assert_eq!(existing[0].name, "bad\u{7}name");
        assert!(!existing[0].shown_name().chars().any(char::is_control));
    }

    #[test]
    fn a_short_query_is_cut_at_the_cap() {
        let app = app_for_search();
        let found = candidates(&app, &message_with(Vec::new()), "a");
        assert_eq!(found.len(), MAX_CANDIDATES);
    }

    #[test]
    fn only_messages_the_server_holds_can_be_reacted_to() {
        let mut m = sample_message("m1");
        assert!(reactable(&m));
        m.kind = MessageKind::System;
        assert!(!reactable(&m));
        m.kind = MessageKind::Deleted;
        assert!(!reactable(&m));
        for send in [
            SendState::Queued { reconcile: false },
            SendState::Sending,
            SendState::Failed,
            SendState::Unconfirmed,
        ] {
            let m = Message {
                send,
                ..sample_message("m1")
            };
            assert!(!reactable(&m), "{:?}", m.send);
        }
    }

    #[test]
    fn a_reaction_is_given_only_when_the_users_own_name_is_among_its_users() {
        let m = message_with(vec![
            reaction("tada", &["ana", "me"]),
            reaction("+1", &["ana"]),
        ]);
        assert!(is_given_by(&m, "me", "tada"));
        assert!(!is_given_by(&m, "me", "+1"), "someone else's reaction");
        assert!(
            !is_given_by(&m, "me", "rocket"),
            "a reaction the message does not have"
        );
        assert!(!is_given_by(&m, "you", "tada"), "another username");
        assert!(!is_given_by(&message_with(Vec::new()), "me", "tada"));
    }

    // ---- opening, keys, sending and live changes

    use crate::app::{Focus, messages};
    use crate::data::RoomId;
    use crate::test_util::press;

    /// An app with the messages pane focused on the middle one of three messages by `author`, the
    /// middle one carrying `reactions`. The user is `me`.
    fn picker_app(author: &str, reactions: Vec<Reaction>) -> (App, RoomId) {
        let mut app = app_80x24();
        let room = app.rooms[0].id.clone();
        let msg = |id: &str, reactions: Vec<Reaction>| Message {
            room_id: room.clone(),
            author: author.to_string(),
            ts: i64::MAX / 4,
            reactions,
            ..sample_message(id)
        };
        app.messages.insert(
            room.clone(),
            vec![
                msg("m0", Vec::new()),
                msg("m1", reactions),
                msg("m2", Vec::new()),
            ],
        );
        app.msg_views.remove(&room);
        app.current_room = Some(room.clone());
        app.own_username = Some("me".to_string());
        app.focus = Focus::Messages;
        press(&mut app, "gg");
        press(&mut app, "j");
        assert_eq!(messages::view(&app, &room).cursor, 1);
        (app, room)
    }

    fn pending(app: &App) -> Vec<Effect> {
        app.effects.clone()
    }

    fn reacts(app: &App) -> Vec<(String, String, bool)> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::React { id, emoji, add } => {
                    Some((id.clone(), emoji.as_str().to_string(), *add))
                }
                _ => None,
            })
            .collect()
    }

    /// Start a new search (`/`) and type `text` into it: the way to search for something.
    fn search_for(app: &mut App, text: &str) {
        press(app, "/");
        type_text(app, text);
    }

    fn typing(app: &App) -> bool {
        app.react_picker.as_ref().expect("open").typing
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, &c.to_string());
        }
    }

    #[test]
    fn plus_opens_the_picker_on_another_users_message_without_touching_anything() {
        let (mut app, room) = picker_app("alice", Vec::new());
        let mode = app.mode;
        press(&mut app, "+");
        let picker = app.react_picker.clone().expect("open");
        assert_eq!((picker.room, picker.id.as_str()), (room.clone(), "m1"));
        assert_eq!((picker.query.as_str(), picker.selected), ("", 0));
        assert_eq!(app.mode, mode);
        assert_eq!(app.mode.label(), "NORMAL");
        assert_eq!(app.focus, Focus::Messages);
        assert_eq!(messages::view(&app, &room).cursor, 1);
    }

    #[test]
    fn the_picker_opens_on_the_users_own_message_too() {
        let (mut app, _) = picker_app("me", Vec::new());
        press(&mut app, "+");
        assert!(app.react_picker.is_some());
    }

    #[test]
    fn plus_does_nothing_on_a_message_that_cannot_be_reacted_to() {
        for (kind, send) in [
            (MessageKind::Deleted, SendState::Sent),
            (MessageKind::System, SendState::Sent),
            (MessageKind::Normal, SendState::Sending),
            (MessageKind::Normal, SendState::Failed),
            (MessageKind::Normal, SendState::Unconfirmed),
            (MessageKind::Normal, SendState::Queued { reconcile: false }),
        ] {
            let (mut app, room) = picker_app("alice", Vec::new());
            let slot = &mut app.messages.get_mut(&room).unwrap()[1];
            slot.kind = kind;
            slot.send = send;
            press(&mut app, "+");
            assert_eq!(app.react_picker, None, "{kind:?} {send:?}");
            assert!(app.effects.is_empty());
            assert!(app.status_msg.is_none());
        }
    }

    #[test]
    fn plus_does_nothing_on_another_pane_or_without_a_room() {
        let (mut app, _) = picker_app("alice", Vec::new());
        app.focus = Focus::Sidebar;
        press(&mut app, "+");
        assert_eq!(app.react_picker, None);
        app.focus = Focus::Input;
        press(&mut app, "+");
        assert_eq!(app.react_picker, None);
        app.focus = Focus::Messages;
        app.current_room = None;
        press(&mut app, "+");
        assert_eq!(app.react_picker, None);
    }

    #[test]
    fn a_detached_room_refuses_with_its_status_line() {
        let (mut app, room) = picker_app("alice", Vec::new());
        app.detached.insert(
            room,
            crate::app::jump::Detached {
                newest_raw: None,
                pending_newer: None,
            },
        );
        press(&mut app, "+");
        assert_eq!(app.react_picker, None);
        assert!(status(&app).contains("older messages"), "{}", status(&app));
        assert!(
            app.effects.is_empty(),
            "no custom emoji fetch for a refused opening"
        );
    }

    #[test]
    fn an_unknown_username_refuses_with_a_status_line() {
        let (mut app, _) = picker_app("alice", Vec::new());
        app.own_username = None;
        press(&mut app, "+");
        assert_eq!(app.react_picker, None);
        assert_eq!(
            status(&app),
            "Username not loaded yet: try again in a moment"
        );
        assert!(app.effects.is_empty());
    }

    #[test]
    fn the_custom_emojis_are_fetched_on_the_first_opening_only_and_again_after_a_failure() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        assert_eq!(pending(&app), [Effect::LoadCustomEmojis]);
        assert_eq!(app.custom_emojis, CustomEmojis::Loading);
        // Closed and opened while the fetch is out: no second request.
        press(&mut app, "<Esc>");
        press(&mut app, "+");
        assert_eq!(pending(&app), [Effect::LoadCustomEmojis]);
        press(&mut app, "<Esc>");

        // The list arrives; later openings do not ask again.
        update(
            &mut app,
            Action::CustomEmojisLoaded(vec![custom("ship_it", &[])]),
        );
        app.effects.clear();
        press(&mut app, "+");
        assert!(app.effects.is_empty());
        press(&mut app, "<Esc>");

        // A failed fetch: the next opening asks again, and the picker works meanwhile.
        app.custom_emojis = CustomEmojis::Loading;
        update(&mut app, Action::CustomEmojisFailed);
        press(&mut app, "+");
        assert_eq!(pending(&app), [Effect::LoadCustomEmojis]);
        search_for(&mut app, "tada");
        assert_eq!(view(&app).expect("open").candidates[0].name, "tada");
    }

    #[test]
    fn arriving_custom_emojis_become_searchable_in_the_open_picker() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "zzship");
        assert!(view(&app).unwrap().candidates.is_empty());
        update(
            &mut app,
            Action::CustomEmojisLoaded(vec![custom("zzship_it", &[])]),
        );
        assert!(app.react_picker.is_some(), "still open");
        assert_eq!(names(&view(&app).unwrap().candidates), ["zzship_it"]);
    }

    #[test]
    fn letters_are_text_while_searching_and_never_reach_the_keymap() {
        let (mut app, room) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        press(&mut app, "/");
        // `j` would move the cursor, `:` would open the command line, `?` the context popup.
        type_text(&mut app, "j:?/gr");
        let picker = app.react_picker.as_ref().expect("still open");
        assert_eq!(picker.query, "j:?/gr");
        assert_eq!(messages::view(&app, &room).cursor, 1);
        assert_eq!(app.mode.label(), "NORMAL");
        assert!(!app.context && app.search_prompt.is_none() && app.reactions_popup.is_none());
    }

    #[test]
    fn ctrl_n_ctrl_p_and_the_arrows_move_the_selection_and_stop_at_the_ends() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "tad");
        let total = view(&app).unwrap().candidates.len();
        assert!(total >= 2, "{total}");
        let selected = |app: &App| view(app).unwrap().selected;
        assert_eq!(selected(&app), 0);
        press(&mut app, "<C-n>");
        assert_eq!(selected(&app), 1);
        press(&mut app, "<Down>");
        assert_eq!(selected(&app), 2.min(total - 1));
        press(&mut app, "<C-p>");
        press(&mut app, "<Up>");
        assert_eq!(selected(&app), 0);
        press(&mut app, "<Up>");
        assert_eq!(selected(&app), 0, "stops at the first");
        for _ in 0..total + 5 {
            press(&mut app, "<C-n>");
        }
        assert_eq!(selected(&app), total - 1, "stops at the last");
    }

    #[test]
    fn changing_the_search_selects_the_first_result_again() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "ta");
        press(&mut app, "<C-n>");
        press(&mut app, "<C-n>");
        assert_eq!(app.react_picker.as_ref().unwrap().selected, 2);
        type_text(&mut app, "d");
        assert_eq!(app.react_picker.as_ref().unwrap().selected, 0);
        press(&mut app, "<C-n>");
        press(&mut app, "<BS>");
        assert_eq!(app.react_picker.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn esc_and_q_close_while_browsing_without_sending() {
        for close in ["<Esc>", "q"] {
            let (mut app, _) = picker_app("alice", vec![reaction("tada", &["ana"])]);
            press(&mut app, "+");
            app.effects.clear();
            press(&mut app, close);
            assert_eq!(app.react_picker, None, "{close}");
            assert!(reacts(&app).is_empty(), "{close}");
        }
    }

    #[test]
    fn ctrl_c_closes_from_either_state_without_sending() {
        for searching in [false, true] {
            let (mut app, _) = picker_app("alice", Vec::new());
            press(&mut app, "+");
            app.effects.clear();
            if searching {
                search_for(&mut app, "tada");
            }
            press(&mut app, "<C-c>");
            assert_eq!(app.react_picker, None, "searching: {searching}");
            assert!(reacts(&app).is_empty());
        }
    }

    #[test]
    fn backspace_deletes_a_character_and_returns_to_browsing_on_an_empty_search() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "ab");
        press(&mut app, "<BS>");
        assert_eq!(app.react_picker.as_ref().unwrap().query, "a");
        assert!(typing(&app));
        press(&mut app, "<BS>");
        assert_eq!(app.react_picker.as_ref().unwrap().query, "");
        assert!(
            typing(&app),
            "an empty search is still a search until the next Backspace"
        );
        press(&mut app, "<BS>");
        assert!(app.react_picker.is_some(), "still open");
        assert!(!typing(&app), "back to browsing");
        assert!(reacts(&app).is_empty());
    }

    #[test]
    fn a_paste_is_one_line_without_control_characters_and_capped() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        // While browsing there is no text to paste into.
        update(&mut app, Action::Paste("ignored".to_string()));
        assert_eq!(app.react_picker.as_ref().unwrap().query, "");
        press(&mut app, "/");
        update(&mut app, Action::Paste("ta\u{1b}[31mda\nx".to_string()));
        let query = app.react_picker.as_ref().unwrap().query.clone();
        assert!(!query.chars().any(char::is_control), "{query:?}");
        assert!(query.starts_with("ta"), "{query:?}");
        update(&mut app, Action::Paste("z".repeat(1000)));
        assert!(app.react_picker.as_ref().unwrap().query.chars().count() <= MAX_CMDLINE);
        // Typing past the cap adds nothing either.
        let before = app.react_picker.as_ref().unwrap().query.clone();
        type_text(&mut app, "q");
        assert_eq!(app.react_picker.as_ref().unwrap().query, before);
    }

    // ---- browsing and searching

    #[test]
    fn the_picker_opens_browsing_even_on_a_message_without_reactions() {
        for reactions in [Vec::new(), vec![reaction("tada", &["ana"])]] {
            let (mut app, _) = picker_app("alice", reactions);
            press(&mut app, "+");
            assert!(!typing(&app));
            // A letter is not text: it is a browsing key or nothing.
            press(&mut app, "x");
            assert_eq!(app.react_picker.as_ref().unwrap().query, "");
        }
    }

    #[test]
    fn slash_starts_a_new_search_and_clears_the_old_one() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "tada");
        press(&mut app, "<Esc>");
        assert_eq!(app.react_picker.as_ref().unwrap().query, "tada");
        press(&mut app, "/");
        assert!(typing(&app));
        assert_eq!(
            app.react_picker.as_ref().unwrap().query,
            "",
            "a new search starts empty"
        );
        assert_eq!(app.react_picker.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn i_and_a_continue_the_search_with_its_text() {
        for key in ["i", "a"] {
            let (mut app, _) = picker_app("alice", Vec::new());
            press(&mut app, "+");
            search_for(&mut app, "tad");
            press(&mut app, "<Esc>");
            assert!(!typing(&app));
            press(&mut app, key);
            assert!(typing(&app), "{key}");
            assert_eq!(app.react_picker.as_ref().unwrap().query, "tad", "{key}");
            type_text(&mut app, "a");
            assert_eq!(app.react_picker.as_ref().unwrap().query, "tada", "{key}");
        }
    }

    #[test]
    fn esc_while_searching_goes_back_to_browsing_and_keeps_the_results() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "tad");
        let results = view(&app).unwrap().candidates;
        assert!(!results.is_empty());
        app.effects.clear();
        press(&mut app, "<Esc>");
        assert!(app.react_picker.is_some(), "not closed");
        assert!(!typing(&app));
        assert_eq!(
            view(&app).unwrap().candidates,
            results,
            "the same results are listed"
        );
        assert!(reacts(&app).is_empty(), "nothing was sent");
        // A second Esc, now browsing, closes.
        press(&mut app, "<Esc>");
        assert_eq!(app.react_picker, None);
    }

    #[test]
    fn j_and_k_move_through_the_reactions_while_browsing() {
        let (mut app, room) = picker_app(
            "alice",
            vec![
                reaction("+1", &["ana"]),
                reaction("tada", &["ana"]),
                reaction("rocket", &["ana"]),
            ],
        );
        press(&mut app, "+");
        let selected = |app: &App| view(app).unwrap().selected;
        assert_eq!(selected(&app), 0);
        press(&mut app, "j");
        assert_eq!(selected(&app), 1);
        press(&mut app, "j");
        press(&mut app, "j");
        assert_eq!(selected(&app), 2, "stops at the last");
        press(&mut app, "k");
        assert_eq!(selected(&app), 1);
        press(&mut app, "<C-p>");
        press(&mut app, "<Up>");
        assert_eq!(selected(&app), 0, "stops at the first");
        press(&mut app, "<Down>");
        press(&mut app, "<C-n>");
        assert_eq!(selected(&app), 2);
        // The message cursor never moved: the keys were the picker's.
        assert_eq!(messages::view(&app, &room).cursor, 1);
    }

    #[test]
    fn enter_sends_the_reaction_that_j_selected() {
        let (mut app, _) = picker_app(
            "alice",
            vec![reaction("+1", &["ana"]), reaction("tada", &["me"])],
        );
        press(&mut app, "+");
        app.effects.clear();
        press(&mut app, "j");
        press(&mut app, "<CR>");
        // The second reaction is the user's own: it is taken back.
        assert_eq!(
            reacts(&app),
            [("m1".to_string(), "tada".to_string(), false)]
        );
        assert_eq!(app.react_picker, None);
    }

    #[test]
    fn j_and_k_walk_the_results_of_a_search_after_esc() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "tad");
        press(&mut app, "<Esc>");
        let results = view(&app).unwrap().candidates;
        assert!(results.len() >= 2);
        press(&mut app, "j");
        assert_eq!(view(&app).unwrap().selected, 1);
        app.effects.clear();
        press(&mut app, "<CR>");
        assert_eq!(reacts(&app)[0].1, results[1].name);
    }

    #[test]
    fn other_keys_do_nothing_while_browsing_and_never_reach_the_keymap() {
        let (mut app, room) = picker_app("alice", vec![reaction("tada", &["ana"])]);
        press(&mut app, "+");
        let before = app.react_picker.clone();
        for key in [
            "h", "l", "x", "z", ":", "?", "d", "e", "r", "n", "g", "G", "+", "1", " ",
        ] {
            press(&mut app, key);
        }
        assert_eq!(app.react_picker, before, "nothing about the picker changed");
        assert_eq!(
            messages::view(&app, &room).cursor,
            1,
            "the message cursor did not move"
        );
        assert_eq!(app.mode.label(), "NORMAL");
        assert!(!app.context && app.search_prompt.is_none() && app.reactions_popup.is_none());
        assert!(
            app.effects
                .iter()
                .all(|e| !matches!(e, Effect::React { .. }))
        );
    }

    #[test]
    fn a_key_timeout_does_not_close_the_picker() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        update(&mut app, Action::KeyTimeout);
        assert!(app.react_picker.is_some());
    }

    #[test]
    fn enter_adds_a_reaction_the_user_has_not_given_and_closes() {
        let (mut app, _) = picker_app("alice", vec![reaction("tada", &["ana"])]);
        press(&mut app, "+");
        app.effects.clear();
        search_for(&mut app, "tada");
        press(&mut app, "<CR>");
        assert_eq!(reacts(&app), [("m1".to_string(), "tada".to_string(), true)]);
        assert_eq!(app.react_picker, None);
        // The row is not guessed at: the message is as the server last said.
        assert_eq!(
            app.messages[&app.current_room.clone().unwrap()][1]
                .reactions
                .len(),
            1
        );
    }

    #[test]
    fn enter_takes_back_a_reaction_the_user_already_gave() {
        let (mut app, _) = picker_app("alice", vec![reaction("tada", &["ana", "me"])]);
        press(&mut app, "+");
        app.effects.clear();
        search_for(&mut app, "tada");
        press(&mut app, "<CR>");
        assert_eq!(
            reacts(&app),
            [("m1".to_string(), "tada".to_string(), false)]
        );
        assert_eq!(app.react_picker, None);
    }

    #[test]
    fn enter_on_the_empty_list_toggles_the_selected_existing_reaction() {
        let (mut app, _) = picker_app(
            "alice",
            vec![reaction("+1", &["ana"]), reaction("tada", &["me"])],
        );
        press(&mut app, "+");
        app.effects.clear();
        // The first entry is someone else's: choosing it adds the user's own.
        press(&mut app, "<CR>");
        assert_eq!(reacts(&app), [("m1".to_string(), "+1".to_string(), true)]);
        // The second is the user's own: choosing it takes it back.
        let (mut app, _) = picker_app(
            "alice",
            vec![reaction("+1", &["ana"]), reaction("tada", &["me"])],
        );
        press(&mut app, "+");
        app.effects.clear();
        press(&mut app, "<C-n>");
        press(&mut app, "<CR>");
        assert_eq!(
            reacts(&app),
            [("m1".to_string(), "tada".to_string(), false)]
        );
    }

    #[test]
    fn enter_sends_the_name_the_server_accepts_for_a_unicode_emoji_and_a_custom_name() {
        let (mut app, _) = picker_app("alice", Vec::new());
        app.custom_emojis = CustomEmojis::Loaded(vec![custom("ship_it", &["ship"])]);
        press(&mut app, "+");
        app.effects.clear();
        search_for(&mut app, "afghanistan");
        press(&mut app, "<CR>");
        assert_eq!(
            reacts(&app),
            [("m1".to_string(), "flag_af".to_string(), true)]
        );

        press(&mut app, "+");
        app.effects.clear();
        // `ship` alone is an exact match for the unicode emoji, so use text only the custom emoji has.
        search_for(&mut app, "ship_i");
        press(&mut app, "<CR>");
        assert_eq!(reacts(&app)[0].1, "ship_it");
    }

    #[test]
    fn enter_with_nothing_to_select_sends_nothing_and_keeps_the_picker() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        app.effects.clear();
        press(&mut app, "<CR>"); // browsing a message without reactions
        assert!(app.react_picker.is_some());
        search_for(&mut app, "zzzzqqqq");
        press(&mut app, "<CR>"); // no match
        assert!(app.react_picker.is_some());
        assert!(reacts(&app).is_empty());
    }

    #[test]
    fn a_room_detached_while_the_picker_is_open_refuses_at_enter() {
        let (mut app, room) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        app.effects.clear();
        search_for(&mut app, "tada");
        app.detached.insert(
            room,
            crate::app::jump::Detached {
                newest_raw: None,
                pending_newer: None,
            },
        );
        press(&mut app, "<CR>");
        assert_eq!(app.react_picker, None);
        assert!(reacts(&app).is_empty());
        assert!(status(&app).contains("older messages"));
    }

    #[test]
    fn a_reaction_arriving_while_open_shows_in_the_empty_list_without_a_key() {
        let (mut app, room) = picker_app("alice", vec![reaction("+1", &["ana"])]);
        press(&mut app, "+");
        assert_eq!(names(&view(&app).unwrap().candidates), ["+1"]);
        let mut live = Message {
            room_id: room.clone(),
            author: "alice".to_string(),
            reactions: vec![reaction("+1", &["ana"]), reaction("rocket", &["zed"])],
            ..sample_message("m1")
        };
        live.ts = app.messages[&room][1].ts;
        update(&mut app, Action::RtMessage(live));
        assert!(app.react_picker.is_some(), "still open");
        assert_eq!(names(&view(&app).unwrap().candidates), ["+1", "rocket"]);
    }

    #[test]
    fn a_shrinking_list_keeps_the_selection_in_range() {
        let (mut app, room) = picker_app(
            "alice",
            vec![reaction("+1", &["ana"]), reaction("tada", &["ana"])],
        );
        press(&mut app, "+");
        press(&mut app, "<C-n>");
        assert_eq!(view(&app).unwrap().selected, 1);
        let mut live = Message {
            room_id: room.clone(),
            author: "alice".to_string(),
            reactions: vec![reaction("+1", &["ana"])],
            ..sample_message("m1")
        };
        live.ts = app.messages[&room][1].ts;
        update(&mut app, Action::RtMessage(live));
        assert_eq!(view(&app).unwrap().selected, 0);
        assert_eq!(app.react_picker.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn deleting_the_message_closes_the_picker() {
        let (mut app, room) = picker_app("alice", vec![reaction("+1", &["ana"])]);
        press(&mut app, "+");
        update(
            &mut app,
            Action::RtDeleted {
                room,
                id: "m1".to_string(),
            },
        );
        assert_eq!(app.react_picker, None);
        // The key after that is an ordinary key again.
        press(&mut app, "j");
        assert!(app.react_picker.is_none());
    }

    #[test]
    fn the_room_updating_underneath_does_not_close_the_picker() {
        let (mut app, room) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        let mut newer = Message {
            room_id: room.clone(),
            author: "bob".to_string(),
            ..sample_message("m3")
        };
        newer.ts = i64::MAX / 4 + 10;
        update(&mut app, Action::RtMessage(newer));
        assert_eq!(app.messages[&room].len(), 4);
        assert!(app.react_picker.is_some());
    }

    #[test]
    fn the_picker_state_never_prints_what_was_typed() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "sentinelquery");
        assert_eq!(
            app.react_picker.as_ref().unwrap().query,
            "sentinelquery",
            "the whole text was typed"
        );
        let shown = format!("{:?} {:#?}", app.react_picker, app.react_picker);
        assert!(!shown.contains("sentinelquery"), "{shown}");
        assert!(shown.contains("query_len"), "{shown}");
    }

    #[test]
    fn the_custom_emoji_cache_follows_its_answers() {
        let mut app = app_80x24();
        assert_eq!(app.custom_emojis, CustomEmojis::NotLoaded);
        assert!(app.custom_emojis.list().is_empty());

        app.custom_emojis = CustomEmojis::Loading;
        assert!(
            app.custom_emojis.list().is_empty(),
            "nothing to search while loading"
        );

        let list = vec![CustomEmoji {
            name: "ship_it".to_string(),
            aliases: vec!["ship".to_string()],
        }];
        update(&mut app, Action::CustomEmojisLoaded(list.clone()));
        assert_eq!(app.custom_emojis, CustomEmojis::Loaded(list.clone()));
        assert_eq!(app.custom_emojis.list(), list.as_slice());

        // A failure is a fresh start, so the next opening asks again.
        app.custom_emojis = CustomEmojis::Loading;
        update(&mut app, Action::CustomEmojisFailed);
        assert_eq!(app.custom_emojis, CustomEmojis::NotLoaded);
        assert!(status(&app).is_empty(), "a failed fetch shows nothing");
    }
}

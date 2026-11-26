//! The reaction picker (`react_message`, `+`): react to the message under the cursor, or take a
//! reaction back, with a search over unicode emoji, aliases and the server's custom emojis.
//!
//! This module holds what the picker keeps between openings and what answers do to it. The
//! picker itself is state on `App`, not a mode; see the design of the change `add-reaction-picker`.
//! Pure: no I/O and no `Frame`.

use crate::api::ApiError;
use crate::app::input_buf::InputBuffer;
use crate::app::update::edit_input;
use crate::app::{App, Focus, MAX_CMDLINE, Mode, StatusMsg, emoji, jump, messages, send, thread};
use crate::data::{Message, MessageKind, RoomId, SendState};
use crate::effect::{Effect, EmojiName};
use crate::input::key::Chord;
use crate::ui::emoji::resolve;
use crate::ui::emoji_names::{send_name, strip_tone, tone_name};
use crate::ui::sanitize::sanitize_line;
use crossterm::event::{KeyCode, KeyModifiers};
use emojis::{Emoji, Group, SkinTone};
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

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
    candidates_for(app, Some(message), query)
}

/// `candidates` for a message or, with `None`, for the draft: no existing reactions, no marks, and
/// the unicode emoji the server refuses are offered too (a character needs no server name).
fn candidates_for(app: &App, message: Option<&Message>, query: &str) -> Vec<Candidate> {
    let query = needle(query);
    let Some(message) = message else {
        return search_all(app, None, &query);
    };
    if query.is_empty() {
        return existing(app, message);
    }
    search_all(app, Some(message), &query)
}

fn search_all(app: &App, message: Option<&Message>, query: &str) -> Vec<Candidate> {
    let insert = message.is_none();
    if query.is_empty() {
        return Vec::new();
    }
    let me = app.own_username.as_deref();
    let mark = |name: &str| -> (usize, bool) {
        let Some(message) = message else {
            return (0, false);
        };
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
    let tone = app.emoji_recents.tone;
    let mut claimed: HashSet<String> = HashSet::new();
    let mut found: Vec<(u8, Candidate)> = Vec::new();
    // `claim` is the name that is held against a later source; `name` is what is sent, which for a
    // unicode emoji in a skin tone is the toned name.
    let mut offer = |rank: u8, claim: &str, name: String, glyph: String, source: Source| {
        if claimed.insert(claim.to_string()) {
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
        if let Some(rank) = rank_of(name, query) {
            offer(rank, name, name.clone(), text.clone(), Source::Alias);
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
        if let Some(rank) = best_rank(names, query) {
            let glyph = resolve(&emoji.name, aliases);
            offer(rank, &emoji.name, emoji.name.clone(), glyph, Source::Custom);
        }
    }
    for emoji in emojis::iter() {
        let Some(first) = emoji.shortcodes().next() else {
            continue;
        };
        let Some(sent) = sendable(first, insert) else {
            continue;
        };
        let names = emoji.shortcodes().chain(std::iter::once(sent));
        if let Some(rank) = best_rank(names, query) {
            let (name, glyph) = unicode_cell(emoji, first, sent, tone, insert);
            offer(rank, sent, name, glyph, Source::Unicode);
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

/// The skin tone (1 to 5) as the crate names it.
fn skin_tone(tone: u8) -> Option<SkinTone> {
    match tone {
        1 => Some(SkinTone::Light),
        2 => Some(SkinTone::MediumLight),
        3 => Some(SkinTone::Medium),
        4 => Some(SkinTone::MediumDark),
        5 => Some(SkinTone::Dark),
        _ => None,
    }
}

/// The name a unicode emoji goes by: the one the server accepts, or for a draft, where the server's
/// opinion does not matter, the crate's first shortcode when it accepts none.
fn sendable(first: &'static str, insert: bool) -> Option<&'static str> {
    send_name(first).or(insert.then_some(first))
}

/// What to send and what to draw for a unicode emoji in skin tone `tone` (0 for none): the toned
/// name and glyph when the emoji has tones and the server accepts their names, else the plain ones.
fn unicode_cell(
    emoji: &'static Emoji,
    first: &'static str,
    sent: &str,
    tone: u8,
    insert: bool,
) -> (String, String) {
    // In a draft the glyph is all there is: the tone applies whenever the emoji has one, and the
    // name stays the plain one so a pick counts once.
    if insert {
        let glyph = skin_tone(tone)
            .and_then(|t| emoji.with_skin_tone(t))
            .unwrap_or(emoji);
        return (sent.to_string(), glyph.as_str().to_string());
    }
    if let Some(toned) = skin_tone(tone).and_then(|t| emoji.with_skin_tone(t))
        && let Some(name) = tone_name(first, tone)
    {
        return (name, toned.as_str().to_string());
    }
    (sent.to_string(), emoji.as_str().to_string())
}

/// Every unicode emoji the server accepts, by the name it is sent under.
static BY_SENT: LazyLock<HashMap<&'static str, &'static Emoji>> = LazyLock::new(|| {
    emojis::iter()
        .filter_map(|emoji| {
            let first = emoji.shortcodes().next()?;
            Some((send_name(first)?, emoji))
        })
        .collect()
});

/// How the message stands with reaction `name`: how many gave it and whether the user did.
fn mark(app: &App, message: Option<&Message>, name: &str) -> (usize, bool) {
    let Some(message) = message else {
        return (0, false);
    };
    let me = app.own_username.as_deref();
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
}

/// A section of the grid. The order of the variants is the order the sections are shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectionKind {
    OnMessage,
    Frequent,
    Smileys,
    Animals,
    Food,
    Travel,
    Activities,
    Objects,
    Symbols,
    Flags,
    Custom,
    /// What a search found, in the order of its ranking.
    Results,
}

impl SectionKind {
    /// The heading drawn over the section.
    pub fn title(self) -> &'static str {
        match self {
            SectionKind::OnMessage => "On this message",
            SectionKind::Frequent => "Frequently used",
            SectionKind::Smileys => "Smileys & People",
            SectionKind::Animals => "Animals & Nature",
            SectionKind::Food => "Food & Drink",
            SectionKind::Travel => "Travel & Places",
            SectionKind::Activities => "Activities",
            SectionKind::Objects => "Objects",
            SectionKind::Symbols => "Symbols",
            SectionKind::Flags => "Flags",
            SectionKind::Custom => "Custom",
            SectionKind::Results => "Results",
        }
    }

    /// The one emoji that stands for the section in the row of tabs.
    pub fn tab(self) -> &'static str {
        match self {
            SectionKind::OnMessage => "💬",
            SectionKind::Frequent => "🕑",
            SectionKind::Smileys => "😀",
            SectionKind::Animals => "🐻",
            SectionKind::Food => "🍔",
            SectionKind::Travel => "🌍",
            SectionKind::Activities => "⚽",
            SectionKind::Objects => "💡",
            SectionKind::Symbols => "🔣",
            SectionKind::Flags => "🏁",
            SectionKind::Custom => "✨",
            SectionKind::Results => "🔍",
        }
    }
}

/// The unicode groups of each section: Smileys & Emotion and People & Body are one section, as in
/// the web client.
const GROUPED: [(SectionKind, &[Group]); 8] = [
    (
        SectionKind::Smileys,
        &[Group::SmileysAndEmotion, Group::PeopleAndBody],
    ),
    (SectionKind::Animals, &[Group::AnimalsAndNature]),
    (SectionKind::Food, &[Group::FoodAndDrink]),
    (SectionKind::Travel, &[Group::TravelAndPlaces]),
    (SectionKind::Activities, &[Group::Activities]),
    (SectionKind::Objects, &[Group::Objects]),
    (SectionKind::Symbols, &[Group::Symbols]),
    (SectionKind::Flags, &[Group::Flags]),
];

/// A titled run of cells, drawn on rows of its own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub kind: SectionKind,
    pub cells: Vec<Candidate>,
}

/// The cell for a name the user picked before, when it is still offered: an alias, a custom emoji
/// or a unicode emoji (in the skin tone in use). `None` for anything else, so a name the server
/// dropped drops out of "Frequently used" without being forgotten.
fn offered_cell(app: &App, message: Option<&Message>, name: &str) -> Option<Candidate> {
    let aliases = &app.settings.emoji_aliases;
    let (sent, glyph, source) = if let Some(text) = aliases.get(name) {
        (name.to_string(), text.clone(), Source::Alias)
    } else if let Some(custom) = app.custom_emojis.find(name)
        && sanitize_line(&custom.name) == custom.name.as_str()
    {
        (
            custom.name.clone(),
            resolve(&custom.name, aliases),
            Source::Custom,
        )
    } else {
        let emoji = match BY_SENT.get(name) {
            Some(emoji) => emoji,
            // A draft also offers the emoji the server refuses, counted under their first name.
            None if message.is_none() => emojis::get_by_shortcode(name)?,
            None => return None,
        };
        let first = emoji.shortcodes().next()?;
        let (sent, glyph) = unicode_cell(
            emoji,
            first,
            name,
            app.emoji_recents.tone,
            message.is_none(),
        );
        (sent, glyph, Source::Unicode)
    };
    let (count, own) = mark(app, message, &sent);
    Some(Candidate {
        name: sent,
        glyph,
        source,
        count,
        own,
    })
}

/// The unicode emoji of `groups` the server accepts, in the crate's order, in the skin tone in use.
fn group_cells(app: &App, message: Option<&Message>, groups: &[Group]) -> Vec<Candidate> {
    let insert = message.is_none();
    let tone = app.emoji_recents.tone;
    let mut cells = Vec::new();
    for emoji in groups.iter().flat_map(|group| group.emojis()) {
        let Some(first) = emoji.shortcodes().next() else {
            continue;
        };
        let Some(sent) = sendable(first, insert) else {
            continue;
        };
        let (name, glyph) = unicode_cell(emoji, first, sent, tone, insert);
        let (count, own) = mark(app, message, &name);
        cells.push(Candidate {
            name,
            glyph,
            source: Source::Unicode,
            count,
            own,
        });
    }
    cells
}

/// What the box shows for `query` on `message`, as sections.
///
/// With nothing typed: "On this message", "Frequently used", the unicode groups and "Custom", each
/// left out when empty. With a search: one "Results" section from `candidates`, or none when the
/// search matches nothing. The skin tone in use is applied to every emoji that has tones.
pub fn sections(app: &App, message: &Message, query: &str) -> Vec<Section> {
    sections_for(app, Some(message), query)
}

/// `sections` for a message or, with `None`, for the draft: no "On this message", no marks, and the
/// emoji the server refuses are offered.
fn sections_for(app: &App, message: Option<&Message>, query: &str) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut push = |kind: SectionKind, cells: Vec<Candidate>| {
        if !cells.is_empty() {
            out.push(Section { kind, cells });
        }
    };
    if !needle(query).is_empty() {
        push(SectionKind::Results, candidates_for(app, message, query));
        return out;
    }
    if let Some(message) = message {
        push(SectionKind::OnMessage, existing(app, message));
    }
    let frequent = app
        .emoji_recents
        .top(crate::app::emoji_recents::MAX_FREQUENT, |name| {
            offered_cell(app, message, name).is_some()
        })
        .into_iter()
        .filter_map(|name| offered_cell(app, message, name))
        .collect();
    push(SectionKind::Frequent, frequent);
    for (kind, groups) in GROUPED {
        push(kind, group_cells(app, message, groups));
    }
    let aliases = &app.settings.emoji_aliases;
    let custom = app
        .custom_emojis
        .list()
        .iter()
        // A name the terminal could not draw safely is not offered, as in a search.
        .filter(|emoji| sanitize_line(&emoji.name) == emoji.name.as_str())
        // An emoji whose image the server would not give (a 404, a format that does not decode)
        // is left out of the grid; a search still finds it, and it is sent by name as ever.
        .filter(|emoji| {
            !matches!(
                app.images.state(&emoji.name),
                Some(crate::app::emoji::ImageState::Failed { .. })
            )
        })
        .map(|emoji| {
            let (count, own) = mark(app, message, &emoji.name);
            Candidate {
                name: emoji.name.clone(),
                glyph: resolve(&emoji.name, aliases),
                source: Source::Custom,
                count,
                own,
            }
        })
        .collect();
    push(SectionKind::Custom, custom);
    out
}

/// The cells of all sections in one list: what the selection indexes.
pub fn flatten(sections: &[Section]) -> Vec<Candidate> {
    sections
        .iter()
        .flat_map(|section| section.cells.iter().cloned())
        .collect()
}

/// One row of the grid as laid out for a column count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    /// The heading of section `0`.
    Heading(usize),
    /// `len` cells of section `section`, starting at flat index `first`.
    Cells {
        section: usize,
        first: usize,
        len: usize,
    },
}

/// The sections laid out in rows for a number of columns: a heading row, then the cells of the
/// section `columns` to a row. Movement and drawing both read it, so they cannot disagree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grid {
    pub rows: Vec<Row>,
}

impl Grid {
    pub fn new(sections: &[Section], columns: usize) -> Grid {
        let columns = columns.max(1);
        let mut rows = Vec::new();
        let mut first = 0;
        for (section, held) in sections.iter().enumerate() {
            rows.push(Row::Heading(section));
            let mut left = held.cells.len();
            while left > 0 {
                let len = left.min(columns);
                rows.push(Row::Cells {
                    section,
                    first,
                    len,
                });
                first += len;
                left -= len;
            }
        }
        Grid { rows }
    }

    /// The index in `rows` of the row that holds flat index `selected`.
    pub fn row_of(&self, selected: usize) -> Option<usize> {
        self.rows.iter().position(|row| {
            matches!(row, Row::Cells { first, len, .. } if (*first..first + len).contains(&selected))
        })
    }

    /// The flat index one row down (`down`) or up in the same column, skipping headings and landing
    /// on the last cell of a shorter row. `selected` itself at the first or last row.
    pub fn vertical(&self, selected: usize, down: bool) -> usize {
        let Some(at) = self.row_of(selected) else {
            return selected;
        };
        let Row::Cells { first, .. } = self.rows[at] else {
            return selected;
        };
        let column = selected - first;
        let mut others: Box<dyn Iterator<Item = &Row>> = if down {
            Box::new(self.rows[at + 1..].iter())
        } else {
            Box::new(self.rows[..at].iter().rev())
        };
        match others.find(|row| matches!(row, Row::Cells { .. })) {
            Some(Row::Cells { first, len, .. }) => first + column.min(len - 1),
            _ => selected,
        }
    }

    /// The rows `start..end` to draw in `visible` rows: the selection is kept in the middle, which
    /// also keeps the heading over the first row of its section in view.
    pub fn window(&self, selected: usize, visible: usize) -> (usize, usize) {
        let total = self.rows.len();
        if visible == 0 || total <= visible {
            return (0, total);
        }
        let at = self.row_of(selected).unwrap_or(0);
        let start = at.saturating_sub(visible / 2).min(total - visible);
        (start, start + visible)
    }
}

/// The flat index of the first cell of the section after (`forward`) or before the one holding
/// `selected`, or `selected` itself at the last or first section.
pub fn jump_section(sections: &[Section], selected: usize, forward: bool) -> usize {
    let mut first = 0;
    let mut starts = Vec::with_capacity(sections.len());
    for section in sections {
        starts.push(first);
        first += section.cells.len();
    }
    let Some(current) = starts.iter().rposition(|start| *start <= selected) else {
        return selected;
    };
    let target = if forward {
        current + 1
    } else {
        current.wrapping_sub(1)
    };
    starts.get(target).copied().unwrap_or(selected)
}

/// The index of the section holding flat index `selected`.
pub fn section_of(sections: &[Section], selected: usize) -> Option<usize> {
    let mut first = 0;
    for (at, section) in sections.iter().enumerate() {
        if selected < first + section.cells.len() {
            return Some(at);
        }
        first += section.cells.len();
    }
    None
}

/// True when the messages pane is focused on a message the picker could react to: what the context
/// popup checks before it lists the react action. Does not look at the detached state or the own
/// username, which `open` refuses with a status line of their own.
pub fn reactable_under_cursor(app: &App) -> bool {
    if app.mode != Mode::Normal {
        return false;
    }
    let Some(room) = app.current_room.as_ref() else {
        return false;
    };
    under_cursor(app, room).is_some_and(reactable)
}

/// The message under the cursor of the focused messages pane or thread pane.
fn under_cursor<'a>(app: &'a App, room: &RoomId) -> Option<&'a Message> {
    match app.focus {
        Focus::Messages => app
            .messages
            .get(room)
            .and_then(|held| held.get(messages::view(app, room).cursor)),
        Focus::Thread => thread::under_pane_cursor(app, room),
        _ => None,
    }
}

/// What the status line says when the picker is refused because the own username has not arrived:
/// without it the app cannot tell whether choosing a reaction adds it or takes it back.
const NO_USERNAME: &str = "Username not loaded yet: try again in a moment";

/// What the box does with the emoji it is given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// React to a message: held by room and id, never a copy of its reactions.
    React { room: RoomId, id: String },
    /// Put the emoji in the draft, in place of the `:` that opened the box.
    Insert,
}

/// The open emoji box: what it is for, what has been typed and which result is selected.
/// Everything shown is derived from these and the app on every draw, so a live change shows up
/// without any plumbing.
#[derive(Clone, PartialEq, Eq)]
pub struct EmojiBox {
    pub target: Target,
    pub query: String,
    pub selected: usize,
    /// True while searching, false while browsing. The box opens browsing, as a vim buffer opens in
    /// Normal mode: keys move and choose, and only `/`, `i` and `a` start typing. The insert target
    /// is always typing.
    pub typing: bool,
}

/// Redacting `Debug`: the typed search text is never printed, only its length.
impl std::fmt::Debug for EmojiBox {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmojiBox")
            .field("target", &self.target)
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
    /// The sections as shown; `candidates` is their cells in one list.
    pub sections: Vec<Section>,
    pub candidates: Vec<Candidate>,
    /// Index into `candidates`, always in range when there are any.
    pub selected: usize,
    /// True while searching (the search line is live input), false while browsing.
    pub typing: bool,
    /// True for the box opened from a colon in the draft.
    pub insert: bool,
}

/// The message the box was opened on, while the room still holds it. `None` for the insert target,
/// which has no message.
fn held<'a>(app: &'a App, picker: &EmojiBox) -> Option<&'a Message> {
    match &picker.target {
        Target::React { room, id } => thread::held_message(app, room, id),
        Target::Insert => None,
    }
}

/// The box's content now, `None` when it is closed or its message is gone.
pub fn view(app: &App) -> Option<View> {
    let picker = app.emoji_box.as_ref()?;
    let insert = picker.target == Target::Insert;
    let message = if insert {
        None
    } else {
        Some(held(app, picker)?)
    };
    let sections = sections_for(app, message, &picker.query);
    let candidates = flatten(&sections);
    let selected = picker.selected.min(candidates.len().saturating_sub(1));
    Some(View {
        query: picker.query.clone(),
        sections,
        candidates,
        selected,
        typing: picker.typing,
        insert,
    })
}

/// `react_message`: open the picker for the message under the cursor.
///
/// A silent no-op unless the messages pane is focused on a message the server holds that is not a
/// deleted line or a system message. Then, in this order: a detached room is refused with its
/// status line, and so is an unknown own username. On the first opening of a session the custom
/// emojis are asked for.
pub fn open(app: &mut App) {
    if app.mode != Mode::Normal {
        return;
    }
    let Some(room) = app.current_room.clone() else {
        return;
    };
    let Some(message) = under_cursor(app, &room) else {
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
    // The list is the session's (`app::emoji`): a failed load is tried again here as well.
    emoji::ensure_loaded(app);
    app.emoji_box = Some(EmojiBox {
        target: Target::React { room, id },
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
        app.emoji_box = None;
        return;
    }
    let Some(typing) = app.emoji_box.as_ref().map(|p| p.typing) else {
        return;
    };
    if app
        .emoji_box
        .as_ref()
        .is_some_and(|b| b.target == Target::Insert)
    {
        insert_key(app, chord);
    } else if typing {
        search_key(app, chord);
    } else {
        browse_key(app, chord);
    }
}

/// What a key does to the selection, the same in both states: `<C-n>` and `<C-p>`, the arrows and
/// `<Tab>` and `<S-Tab>`. Letters are left to the state, since while searching they are text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Step {
    Left,
    Right,
    Down,
    Up,
    NextSection,
    PrevSection,
}

fn step_key(chord: Chord) -> Option<Step> {
    let ctrl = |c: char| chord.code == KeyCode::Char(c) && chord.mods == KeyModifiers::CONTROL;
    let bare = chord.mods.is_empty();
    match chord.code {
        _ if ctrl('n') => Some(Step::Down),
        _ if ctrl('p') => Some(Step::Up),
        KeyCode::Down if bare => Some(Step::Down),
        KeyCode::Up if bare => Some(Step::Up),
        KeyCode::Left if bare => Some(Step::Left),
        KeyCode::Right if bare => Some(Step::Right),
        KeyCode::Tab if bare => Some(Step::NextSection),
        // `normalize` turns BackTab into Tab with SHIFT.
        KeyCode::Tab if chord.mods == KeyModifiers::SHIFT => Some(Step::PrevSection),
        KeyCode::BackTab => Some(Step::PrevSection),
        _ => None,
    }
}

/// `<C-t>`: the next skin tone.
fn tone_key(chord: Chord) -> bool {
    chord.code == KeyCode::Char('t') && chord.mods == KeyModifiers::CONTROL
}

/// A key while browsing: `h`, `j`, `k`, `l`, the arrows, `<C-n>` and `<C-p>` move through the grid,
/// `<Tab>`, `<S-Tab>`, `H` and `L` jump between sections, `<C-t>` changes the skin tone, `Enter`
/// sends, `q` and `Esc` close, `/` starts a new search and `i` and `a` continue the last one.
/// Nothing else does anything.
fn browse_key(app: &mut App, chord: Chord) {
    let plain = |c: char| chord.code == KeyCode::Char(c) && chord.mods.is_empty();
    if let Some(step) = step_key(chord) {
        move_selection(app, step);
    } else if tone_key(chord) {
        cycle_tone(app);
    } else if plain('h') {
        move_selection(app, Step::Left);
    } else if plain('l') {
        move_selection(app, Step::Right);
    } else if plain('j') {
        move_selection(app, Step::Down);
    } else if plain('k') {
        move_selection(app, Step::Up);
    } else if plain('H') {
        move_selection(app, Step::PrevSection);
    } else if plain('L') {
        move_selection(app, Step::NextSection);
    } else if chord.code == KeyCode::Enter {
        send_selected(app);
    } else if plain('q') || (chord.code == KeyCode::Esc && chord.mods.is_empty()) {
        app.emoji_box = None;
    } else if plain('/') {
        if let Some(picker) = app.emoji_box.as_mut() {
            picker.query.clear();
            picker.selected = 0;
            picker.typing = true;
        }
    } else if (plain('i') || plain('a'))
        && let Some(picker) = app.emoji_box.as_mut()
    {
        picker.typing = true;
    }
}

/// A key while searching: characters are text (so `h`, `j`, `k` and `l` are typed), `<C-n>`,
/// `<C-p>`, the arrows and `<Tab>`/`<S-Tab>` move, `<C-t>` changes the skin tone, `Enter` sends,
/// `Esc` goes back to browsing with the results kept, and `Backspace` deletes a character or, on an
/// empty search, goes back to browsing.
fn search_key(app: &mut App, chord: Chord) {
    if let Some(step) = step_key(chord) {
        move_selection(app, step);
    } else if tone_key(chord) {
        cycle_tone(app);
    } else if chord.code == KeyCode::Enter {
        send_selected(app);
    } else if chord.code == KeyCode::Esc {
        if let Some(picker) = app.emoji_box.as_mut() {
            picker.typing = false;
        }
    } else if chord.code == KeyCode::Backspace {
        let Some(picker) = app.emoji_box.as_mut() else {
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
        && let Some(picker) = app.emoji_box.as_mut()
        && picker.query.chars().count() < MAX_CMDLINE
    {
        picker.query.push(c);
        picker.selected = 0;
    }
}

/// A paste while searching: one line, capped, control characters removed. While browsing there is
/// no text to paste into, so a paste does nothing.
pub fn paste(app: &mut App, text: &str) {
    if app
        .emoji_box
        .as_ref()
        .is_some_and(|b| b.target == Target::Insert)
    {
        insert_paste(app, text);
        return;
    }
    if let Some(picker) = app.emoji_box.as_mut()
        && picker.typing
    {
        let room = MAX_CMDLINE.saturating_sub(picker.query.chars().count());
        picker.query.extend(sanitize_line(text).chars().take(room));
        picker.selected = 0;
    }
}

/// Move the selection one step through the grid, staying where it is at either end. Up and down
/// need the number of cells a row holds, which the drawer decides from the width of the pane.
fn move_selection(app: &mut App, step: Step) {
    let Some(view) = view(app) else {
        return;
    };
    let last = view.candidates.len().saturating_sub(1);
    let selected = match step {
        Step::Left => view.selected.saturating_sub(1),
        Step::Right => (view.selected + 1).min(last),
        Step::Down | Step::Up => Grid::new(&view.sections, crate::ui::picker::columns(app))
            .vertical(view.selected, step == Step::Down),
        Step::NextSection => jump_section(&view.sections, view.selected, true),
        Step::PrevSection => jump_section(&view.sections, view.selected, false),
    };
    if let Some(picker) = app.emoji_box.as_mut() {
        picker.selected = selected;
    }
}

/// `<C-t>`: the next skin tone, remembered. The cells keep their places, so the selection stays.
fn cycle_tone(app: &mut App) {
    app.emoji_recents.cycle_tone();
    app.effects
        .push(Effect::SaveRecents(app.emoji_recents.clone()));
}

/// The name a pick is counted under: the name sent, without a skin tone, so the same emoji counts
/// once whichever tone was in use and "Frequently used" shows it in the tone in use. A custom emoji
/// is counted as it is named.
fn pick_name(app: &App, name: &str) -> String {
    let base = strip_tone(name);
    if base != name && BY_SENT.contains_key(base) && app.custom_emojis.find(name).is_none() {
        base.to_string()
    } else {
        name.to_string()
    }
}

/// `Enter`: send the selected reaction, adding it, or taking it back when the user's own name is
/// already among those who gave it on this message, and close the picker. With nothing to select
/// nothing is sent and the picker stays open. The row changes from the message the server streams
/// back, not from a local guess.
fn send_selected(app: &mut App) {
    let Some(view) = view(app) else {
        app.emoji_box = None;
        return;
    };
    let Some(candidate) = view.candidates.get(view.selected) else {
        return;
    };
    let Some(picker) = app.emoji_box.clone() else {
        return;
    };
    let Target::React { id, .. } = picker.target.clone() else {
        return;
    };
    // The room may have become detached while the picker was open.
    if jump::refuse_if_detached(app) {
        app.emoji_box = None;
        return;
    }
    let Some(own) = app.own_username.clone() else {
        app.emoji_box = None;
        return;
    };
    let Some(message) = held(app, &picker) else {
        app.emoji_box = None;
        return;
    };
    let add = !is_given_by(message, &own, &candidate.name);
    let name = candidate.name.clone();
    // Only adding counts as a pick: taking a reaction back is not a choice of that emoji.
    if add {
        let counted = pick_name(app, &name);
        app.emoji_recents.record(&counted);
        app.effects
            .push(Effect::SaveRecents(app.emoji_recents.clone()));
    }
    app.effects.push(Effect::React {
        id,
        emoji: EmojiName::new(name),
        add,
    });
    app.emoji_box = None;
}

/// True for a character a shortcode is made of.
fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '+' | '-')
}

/// True when a colon typed after `before` (the draft up to the colon) could start a shortcode: the
/// character before it is not a letter, a digit or another colon, and it is not inside inline code
/// or a fenced block (an odd number of backticks before it).
pub fn colon_starts_shortcode(before: &str) -> bool {
    let after_word = before
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric() || c == ':');
    !after_word && before.matches('`').count().is_multiple_of(2)
}

/// A colon was just typed into the draft: open the box to put an emoji in its place when the colon
/// could start a shortcode. The colon stays in the draft until the box ends.
pub fn open_insert(app: &mut App) {
    if app.mode != Mode::Insert || app.emoji_box.is_some() {
        return;
    }
    let before = app.input.text_before_cursor();
    let Some(before) = before.strip_suffix(':') else {
        return;
    };
    if !colon_starts_shortcode(before) {
        return;
    }
    app.matcher.reset();
    emoji::ensure_loaded(app);
    app.emoji_box = Some(EmojiBox {
        target: Target::Insert,
        query: String::new(),
        selected: 0,
        typing: true,
    });
}

/// A key while the box is open from a colon: name characters are the search, the movement keys do
/// what they do anywhere in the box, `Enter` puts the emoji in the draft, `Esc` leaves the colon,
/// `Backspace` on an empty search takes it back, and any other character closes the box and goes
/// into the draft after what was typed. Nothing else does anything.
fn insert_key(app: &mut App, chord: Chord) {
    if let Some(step) = step_key(chord) {
        move_selection(app, step);
    } else if tone_key(chord) {
        cycle_tone(app);
    } else if chord.code == KeyCode::Enter {
        insert_selected(app);
    } else if chord.code == KeyCode::Esc {
        app.emoji_box = None;
    } else if chord.code == KeyCode::Backspace {
        let Some(picker) = app.emoji_box.as_mut() else {
            return;
        };
        if picker.query.pop().is_none() {
            app.emoji_box = None;
            edit_input(app, InputBuffer::backspace);
        } else {
            picker.selected = 0;
        }
    } else if let KeyCode::Char(c) = chord.code
        && chord.mods.is_empty()
        && !c.is_control()
    {
        if is_name_char(c) {
            if let Some(picker) = app.emoji_box.as_mut()
                && picker.query.chars().count() < MAX_CMDLINE
            {
                picker.query.push(c);
                picker.selected = 0;
            }
        } else {
            fall_through(app, &c.to_string());
        }
    }
}

/// A paste while the box is open from a colon: all name characters extend the search, anything else
/// closes the box and goes into the draft after the search.
fn insert_paste(app: &mut App, text: &str) {
    if text.chars().all(is_name_char) {
        if let Some(picker) = app.emoji_box.as_mut() {
            let room = MAX_CMDLINE.saturating_sub(picker.query.chars().count());
            picker.query.extend(text.chars().take(room));
            picker.selected = 0;
        }
    } else {
        fall_through(app, text);
    }
}

/// Close the box and put what was typed in the search and then `text` in the draft: what plain
/// typing would have made.
fn fall_through(app: &mut App, text: &str) {
    let Some(picker) = app.emoji_box.take() else {
        return;
    };
    edit_input(app, |input| {
        input.insert_str(&picker.query);
        input.insert_str(text);
    });
}

/// `Enter` in the box opened from a colon: put the selected emoji in the draft in place of the
/// colon, count the pick and close. With nothing selected only the box closes.
fn insert_selected(app: &mut App) {
    let candidate = view(app).and_then(|v| v.candidates.get(v.selected).cloned());
    app.emoji_box = None;
    let Some(candidate) = candidate else {
        return;
    };
    let text = match candidate.source {
        Source::Custom => format!(":{}:", candidate.name),
        _ => candidate.glyph.clone(),
    };
    let counted = pick_name(app, &candidate.name);
    app.emoji_recents.record(&counted);
    app.effects
        .push(Effect::SaveRecents(app.emoji_recents.clone()));
    edit_input(app, |input| {
        if input.text_before_cursor().ends_with(':') {
            input.backspace();
        }
        input.insert_str(&text);
    });
}

/// Keep the open picker in step with the room: close it when its message is gone or can no longer
/// be reacted to (deleted), and keep the selection inside a list that shrank. The reducer calls
/// this after every action, so a live change shows at once.
pub fn close_if_stale(app: &mut App) {
    let Some(picker) = app.emoji_box.as_ref() else {
        return;
    };
    let alive = match picker.target {
        Target::React { .. } => held(app, picker).is_some_and(reactable),
        Target::Insert => app.mode == Mode::Insert,
    };
    if !alive {
        app.emoji_box = None;
    } else if let Some(view) = view(app)
        && let Some(picker) = app.emoji_box.as_mut()
    {
        picker.selected = view.selected;
    }
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
    use crate::app::emoji::CustomEmojis;
    use crate::app::update::update;
    use crate::data::CustomEmoji;
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
            ..Default::default()
        }
    }

    /// The names in "On this message", the first section when the message has reactions.
    fn on_message(app: &App) -> Vec<String> {
        let view = view(app).expect("open");
        view.sections
            .iter()
            .find(|s| s.kind == SectionKind::OnMessage)
            .map(|s| s.cells.iter().map(|c| c.name.clone()).collect())
            .unwrap_or_default()
    }

    fn columns_of(app: &App) -> usize {
        crate::ui::picker::columns(app)
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
        app.emoji_box.as_ref().expect("open").typing
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
        let picker = app.emoji_box.clone().expect("open");
        assert_eq!(
            picker.target,
            Target::React {
                room: room.clone(),
                id: "m1".to_string()
            }
        );
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
        assert!(app.emoji_box.is_some());
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
            assert_eq!(app.emoji_box, None, "{kind:?} {send:?}");
            assert!(app.effects.is_empty());
            assert!(app.status_msg.is_none());
        }
    }

    #[test]
    fn plus_does_nothing_on_another_pane_or_without_a_room() {
        let (mut app, _) = picker_app("alice", Vec::new());
        app.focus = Focus::Sidebar;
        press(&mut app, "+");
        assert_eq!(app.emoji_box, None);
        app.focus = Focus::Input;
        press(&mut app, "+");
        assert_eq!(app.emoji_box, None);
        app.focus = Focus::Messages;
        app.current_room = None;
        press(&mut app, "+");
        assert_eq!(app.emoji_box, None);
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
        assert_eq!(app.emoji_box, None);
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
        assert_eq!(app.emoji_box, None);
        assert_eq!(
            status(&app),
            "Username not loaded yet: try again in a moment"
        );
        assert!(app.effects.is_empty());
    }

    #[test]
    fn the_picker_uses_the_session_list_and_asks_again_only_after_a_failure() {
        // The list the session loaded: the first opening makes no request.
        let (mut app, _) = picker_app("alice", Vec::new());
        app.custom_emojis = CustomEmojis::Loaded(vec![custom("ship_it", &[])]);
        app.effects.clear();
        press(&mut app, "+");
        assert!(app.effects.is_empty());
        search_for(&mut app, "ship_it");
        assert_eq!(names(&view(&app).unwrap().candidates), ["ship_it"]);
        press(&mut app, "<Esc>");
        press(&mut app, "<Esc>");
        assert!(app.emoji_box.is_none());

        // A failed load: the picker works from unicode meanwhile and asks again on opening.
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
        assert!(app.emoji_box.is_some(), "still open");
        assert_eq!(names(&view(&app).unwrap().candidates), ["zzship_it"]);
    }

    #[test]
    fn letters_are_text_while_searching_and_never_reach_the_keymap() {
        let (mut app, room) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        press(&mut app, "/");
        // `j` would move the cursor, `:` would open the command line, `?` the context popup.
        type_text(&mut app, "j:?/gr");
        let picker = app.emoji_box.as_ref().expect("still open");
        assert_eq!(picker.query, "j:?/gr");
        assert_eq!(messages::view(&app, &room).cursor, 1);
        assert_eq!(app.mode.label(), "NORMAL");
        assert!(!app.context && app.search_prompt.is_none() && app.reactions_popup.is_none());
    }

    #[test]
    fn the_arrows_move_the_selection_and_stop_at_the_ends_while_searching() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "tad");
        let total = view(&app).unwrap().candidates.len();
        assert!(total >= 2, "{total}");
        let selected = |app: &App| view(app).unwrap().selected;
        assert_eq!(selected(&app), 0);
        press(&mut app, "<Right>");
        assert_eq!(selected(&app), 1);
        press(&mut app, "<Right>");
        assert_eq!(selected(&app), 2.min(total - 1));
        press(&mut app, "<Left>");
        press(&mut app, "<Left>");
        assert_eq!(selected(&app), 0);
        press(&mut app, "<Left>");
        assert_eq!(selected(&app), 0, "stops at the first");
        for _ in 0..total + 5 {
            press(&mut app, "<Right>");
        }
        assert_eq!(selected(&app), total - 1, "stops at the last");
        // `<C-n>` and `<C-p>` are down and up one row, so they stay inside a one-row list.
        press(&mut app, "<C-p>");
        assert!(selected(&app) < total - 1 || total <= columns_of(&app));
        for _ in 0..total + 5 {
            press(&mut app, "<C-n>");
        }
        assert!(selected(&app) < total);
    }

    #[test]
    fn changing_the_search_selects_the_first_result_again() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "ta");
        press(&mut app, "<Right>");
        press(&mut app, "<Right>");
        assert_eq!(app.emoji_box.as_ref().unwrap().selected, 2);
        type_text(&mut app, "d");
        assert_eq!(app.emoji_box.as_ref().unwrap().selected, 0);
        press(&mut app, "<Right>");
        press(&mut app, "<BS>");
        assert_eq!(app.emoji_box.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn esc_and_q_close_while_browsing_without_sending() {
        for close in ["<Esc>", "q"] {
            let (mut app, _) = picker_app("alice", vec![reaction("tada", &["ana"])]);
            press(&mut app, "+");
            app.effects.clear();
            press(&mut app, close);
            assert_eq!(app.emoji_box, None, "{close}");
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
            assert_eq!(app.emoji_box, None, "searching: {searching}");
            assert!(reacts(&app).is_empty());
        }
    }

    #[test]
    fn backspace_deletes_a_character_and_returns_to_browsing_on_an_empty_search() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "ab");
        press(&mut app, "<BS>");
        assert_eq!(app.emoji_box.as_ref().unwrap().query, "a");
        assert!(typing(&app));
        press(&mut app, "<BS>");
        assert_eq!(app.emoji_box.as_ref().unwrap().query, "");
        assert!(
            typing(&app),
            "an empty search is still a search until the next Backspace"
        );
        press(&mut app, "<BS>");
        assert!(app.emoji_box.is_some(), "still open");
        assert!(!typing(&app), "back to browsing");
        assert!(reacts(&app).is_empty());
    }

    #[test]
    fn a_paste_is_one_line_without_control_characters_and_capped() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        // While browsing there is no text to paste into.
        update(&mut app, Action::Paste("ignored".to_string()));
        assert_eq!(app.emoji_box.as_ref().unwrap().query, "");
        press(&mut app, "/");
        update(&mut app, Action::Paste("ta\u{1b}[31mda\nx".to_string()));
        let query = app.emoji_box.as_ref().unwrap().query.clone();
        assert!(!query.chars().any(char::is_control), "{query:?}");
        assert!(query.starts_with("ta"), "{query:?}");
        update(&mut app, Action::Paste("z".repeat(1000)));
        assert!(app.emoji_box.as_ref().unwrap().query.chars().count() <= MAX_CMDLINE);
        // Typing past the cap adds nothing either.
        let before = app.emoji_box.as_ref().unwrap().query.clone();
        type_text(&mut app, "q");
        assert_eq!(app.emoji_box.as_ref().unwrap().query, before);
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
            assert_eq!(app.emoji_box.as_ref().unwrap().query, "");
        }
    }

    #[test]
    fn slash_starts_a_new_search_and_clears_the_old_one() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "tada");
        press(&mut app, "<Esc>");
        assert_eq!(app.emoji_box.as_ref().unwrap().query, "tada");
        press(&mut app, "/");
        assert!(typing(&app));
        assert_eq!(
            app.emoji_box.as_ref().unwrap().query,
            "",
            "a new search starts empty"
        );
        assert_eq!(app.emoji_box.as_ref().unwrap().selected, 0);
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
            assert_eq!(app.emoji_box.as_ref().unwrap().query, "tad", "{key}");
            type_text(&mut app, "a");
            assert_eq!(app.emoji_box.as_ref().unwrap().query, "tada", "{key}");
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
        assert!(app.emoji_box.is_some(), "not closed");
        assert!(!typing(&app));
        assert_eq!(
            view(&app).unwrap().candidates,
            results,
            "the same results are listed"
        );
        assert!(reacts(&app).is_empty(), "nothing was sent");
        // A second Esc, now browsing, closes.
        press(&mut app, "<Esc>");
        assert_eq!(app.emoji_box, None);
    }

    #[test]
    fn h_and_l_and_the_arrows_move_through_the_reactions_while_browsing() {
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
        press(&mut app, "l");
        assert_eq!(selected(&app), 1);
        press(&mut app, "<Right>");
        assert_eq!(selected(&app), 2);
        press(&mut app, "h");
        assert_eq!(selected(&app), 1);
        press(&mut app, "<Left>");
        press(&mut app, "h");
        assert_eq!(selected(&app), 0, "stops at the first");
        // Up on the first row has nowhere to go.
        press(&mut app, "k");
        press(&mut app, "<Up>");
        press(&mut app, "<C-p>");
        assert_eq!(selected(&app), 0);
        // The message cursor never moved: the keys were the picker's.
        assert_eq!(messages::view(&app, &room).cursor, 1);
    }

    #[test]
    fn enter_sends_the_reaction_that_l_selected() {
        let (mut app, _) = picker_app(
            "alice",
            vec![reaction("+1", &["ana"]), reaction("tada", &["me"])],
        );
        press(&mut app, "+");
        app.effects.clear();
        press(&mut app, "l");
        press(&mut app, "<CR>");
        // The second reaction is the user's own: it is taken back.
        assert_eq!(
            reacts(&app),
            [("m1".to_string(), "tada".to_string(), false)]
        );
        assert_eq!(app.emoji_box, None);
    }

    #[test]
    fn l_and_h_walk_the_results_of_a_search_after_esc() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "tad");
        press(&mut app, "<Esc>");
        let results = view(&app).unwrap().candidates;
        assert!(results.len() >= 2);
        press(&mut app, "l");
        assert_eq!(view(&app).unwrap().selected, 1);
        app.effects.clear();
        press(&mut app, "<CR>");
        assert_eq!(reacts(&app)[0].1, results[1].name);
    }

    #[test]
    fn other_keys_do_nothing_while_browsing_and_never_reach_the_keymap() {
        let (mut app, room) = picker_app("alice", vec![reaction("tada", &["ana"])]);
        press(&mut app, "+");
        let before = app.emoji_box.clone();
        for key in [
            "x", "z", ":", "?", "d", "e", "r", "n", "g", "G", "+", "1", " ",
        ] {
            press(&mut app, key);
        }
        assert_eq!(app.emoji_box, before, "nothing about the picker changed");
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
        assert!(app.emoji_box.is_some());
    }

    #[test]
    fn enter_adds_a_reaction_the_user_has_not_given_and_closes() {
        let (mut app, _) = picker_app("alice", vec![reaction("tada", &["ana"])]);
        press(&mut app, "+");
        app.effects.clear();
        search_for(&mut app, "tada");
        press(&mut app, "<CR>");
        assert_eq!(reacts(&app), [("m1".to_string(), "tada".to_string(), true)]);
        assert_eq!(app.emoji_box, None);
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
        assert_eq!(app.emoji_box, None);
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
        press(&mut app, "l");
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
        search_for(&mut app, "zzzzqqqq");
        press(&mut app, "<CR>"); // no match
        assert!(app.emoji_box.is_some());
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
        assert_eq!(app.emoji_box, None);
        assert!(reacts(&app).is_empty());
        assert!(status(&app).contains("older messages"));
    }

    #[test]
    fn a_reaction_arriving_while_open_shows_in_the_empty_list_without_a_key() {
        let (mut app, room) = picker_app("alice", vec![reaction("+1", &["ana"])]);
        press(&mut app, "+");
        assert_eq!(on_message(&app), ["+1"]);
        let mut live = Message {
            room_id: room.clone(),
            author: "alice".to_string(),
            reactions: vec![reaction("+1", &["ana"]), reaction("rocket", &["zed"])],
            ..sample_message("m1")
        };
        live.ts = app.messages[&room][1].ts;
        update(&mut app, Action::RtMessage(live));
        assert!(app.emoji_box.is_some(), "still open");
        assert_eq!(on_message(&app), ["+1", "rocket"]);
    }

    #[test]
    fn a_reaction_leaving_keeps_the_selection_in_range() {
        let (mut app, room) = picker_app(
            "alice",
            vec![reaction("+1", &["ana"]), reaction("tada", &["ana"])],
        );
        press(&mut app, "+");
        press(&mut app, "l");
        assert_eq!(view(&app).unwrap().selected, 1);
        let mut live = Message {
            room_id: room.clone(),
            author: "alice".to_string(),
            reactions: vec![reaction("+1", &["ana"])],
            ..sample_message("m1")
        };
        live.ts = app.messages[&room][1].ts;
        update(&mut app, Action::RtMessage(live));
        // The selection is an index into the whole grid: it stays in range and now points at what
        // took the place of the reaction that went.
        let view = view(&app).unwrap();
        assert_eq!(view.selected, 1);
        assert!(view.selected < view.candidates.len());
        assert_eq!(app.emoji_box.as_ref().unwrap().selected, 1);
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
        assert_eq!(app.emoji_box, None);
        // The key after that is an ordinary key again.
        press(&mut app, "j");
        assert!(app.emoji_box.is_none());
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
        assert!(app.emoji_box.is_some());
    }

    // ---- sections, the grid, tones and picks

    fn kinds(app: &App) -> Vec<SectionKind> {
        view(app)
            .expect("open")
            .sections
            .iter()
            .map(|s| s.kind)
            .collect()
    }

    fn selected_name(app: &App) -> String {
        let view = view(app).expect("open");
        view.candidates[view.selected].name.clone()
    }

    fn sent_effects(app: &App) -> usize {
        app.effects
            .iter()
            .filter(|e| matches!(e, Effect::SaveRecents(_)))
            .count()
    }

    #[test]
    fn the_sections_come_in_order_and_empty_ones_are_left_out() {
        let (mut app, _) = picker_app("alice", vec![reaction("+1", &["ana"])]);
        app.custom_emojis = CustomEmojis::Loaded(vec![custom("ship_it", &[])]);
        app.emoji_recents.record("tada");
        press(&mut app, "+");
        use SectionKind::*;
        assert_eq!(
            kinds(&app),
            [
                OnMessage, Frequent, Smileys, Animals, Food, Travel, Activities, Objects, Symbols,
                Flags, Custom
            ]
        );
        // Nothing picked, no custom emojis, no reactions: only the unicode groups.
        let (mut app, _) = picker_app("alice", Vec::new());
        app.custom_emojis = CustomEmojis::Loaded(Vec::new());
        press(&mut app, "+");
        assert_eq!(
            kinds(&app),
            [
                Smileys, Animals, Food, Travel, Activities, Objects, Symbols, Flags
            ]
        );
        let view = view(&app).unwrap();
        assert_eq!(view.sections[0].cells[0].glyph, "😀");
        assert_eq!(view.selected, 0);
    }

    #[test]
    fn the_box_opens_on_the_first_reaction_or_the_first_emoji() {
        let (mut app, _) = picker_app(
            "alice",
            vec![reaction("+1", &["ana"]), reaction("tada", &["ana"])],
        );
        press(&mut app, "+");
        assert_eq!(selected_name(&app), "+1");
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        assert_eq!(selected_name(&app), "grinning");
    }

    #[test]
    fn every_unicode_emoji_the_server_accepts_appears_once() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        let view = view(&app).unwrap();
        let mut seen: HashSet<&str> = HashSet::new();
        for cell in view.sections.iter().flat_map(|s| &s.cells) {
            assert!(seen.insert(cell.name.as_str()), "{} twice", cell.name);
        }
        let expected = emojis::iter()
            .filter_map(|e| send_name(e.shortcodes().next()?))
            .count();
        assert_eq!(seen.len(), expected);
    }

    #[test]
    fn frequently_used_follows_the_picks_and_skips_names_not_offered() {
        let (mut app, _) = picker_app("alice", Vec::new());
        for name in ["tada", "tada", "+1", "gone_custom"] {
            app.emoji_recents.record(name);
        }
        press(&mut app, "+");
        let view = view(&app).unwrap();
        assert_eq!(view.sections[0].kind, SectionKind::Frequent);
        let frequent: Vec<&str> = view.sections[0]
            .cells
            .iter()
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(frequent, ["tada", "+1"]);
        // The custom emoji comes back when the server lists it again.
        app.custom_emojis = CustomEmojis::Loaded(vec![custom("gone_custom", &[])]);
        let view = super::view(&app).unwrap();
        assert_eq!(view.sections[0].cells.len(), 3);
    }

    #[test]
    fn a_search_is_one_results_section_and_an_empty_one_brings_the_sections_back() {
        let (mut app, _) = picker_app("alice", vec![reaction("+1", &["ana"])]);
        press(&mut app, "+");
        search_for(&mut app, "tad");
        assert_eq!(kinds(&app), [SectionKind::Results]);
        assert_eq!(view(&app).unwrap().sections[0].cells[0].name, "tada");
        // The ranking is the one `candidates` gives.
        let message = held(&app, app.emoji_box.as_ref().unwrap()).unwrap();
        assert_eq!(
            view(&app).unwrap().candidates,
            candidates(&app, message, "tad")
        );
        press(&mut app, "<BS>");
        press(&mut app, "<BS>");
        press(&mut app, "<BS>");
        assert_eq!(kinds(&app)[0], SectionKind::OnMessage);
        // No match: no section at all.
        search_for(&mut app, "zzzzqqqq");
        assert!(kinds(&app).is_empty());
    }

    fn synthetic(sizes: &[usize]) -> Vec<Section> {
        let mut at = 0;
        sizes
            .iter()
            .map(|n| {
                let cells = (0..*n)
                    .map(|_| {
                        at += 1;
                        Candidate {
                            name: format!("e{at}"),
                            glyph: "x".to_string(),
                            source: Source::Unicode,
                            count: 0,
                            own: false,
                        }
                    })
                    .collect();
                Section {
                    kind: SectionKind::Smileys,
                    cells,
                }
            })
            .collect()
    }

    #[test]
    fn the_grid_has_a_heading_row_and_columns_per_row_for_each_section() {
        let grid = Grid::new(&synthetic(&[5, 2]), 3);
        assert_eq!(
            grid.rows,
            [
                Row::Heading(0),
                Row::Cells {
                    section: 0,
                    first: 0,
                    len: 3
                },
                Row::Cells {
                    section: 0,
                    first: 3,
                    len: 2
                },
                Row::Heading(1),
                Row::Cells {
                    section: 1,
                    first: 5,
                    len: 2
                },
            ]
        );
    }

    #[test]
    fn down_and_up_keep_the_column_and_land_on_the_last_cell_of_a_short_row() {
        let sections = synthetic(&[8, 5, 3]);
        let grid = Grid::new(&sections, 4);
        // Rows: [0..4) [4..8) | [8..12) [12..13) | [13..16)
        assert_eq!(grid.vertical(2, true), 6, "same column, next row");
        assert_eq!(
            grid.vertical(6, true),
            10,
            "skips the heading of the next section"
        );
        assert_eq!(grid.vertical(11, true), 12, "short row: its last cell");
        assert_eq!(grid.vertical(12, true), 13);
        assert_eq!(grid.vertical(10, false), 6);
        assert_eq!(grid.vertical(12, false), 8);
        assert_eq!(grid.vertical(15, true), 15, "last row stays");
        assert_eq!(grid.vertical(1, false), 1, "first row stays");
    }

    #[test]
    fn left_and_right_cross_row_ends_and_sections_and_stop_at_the_edges() {
        let (mut app, _) = picker_app("alice", vec![reaction("+1", &["ana"])]);
        press(&mut app, "+");
        let first_of_next = view(&app).unwrap().sections[0].cells.len();
        assert_eq!(first_of_next, 1);
        press(&mut app, "l");
        assert_eq!(
            view(&app).unwrap().selected,
            1,
            "from the last reaction into the next section"
        );
        press(&mut app, "h");
        press(&mut app, "h");
        assert_eq!(view(&app).unwrap().selected, 0, "edge");
    }

    #[test]
    fn tab_and_shift_tab_jump_between_sections() {
        let sections = synthetic(&[3, 4, 2]);
        assert_eq!(jump_section(&sections, 1, true), 3);
        assert_eq!(jump_section(&sections, 5, true), 7);
        assert_eq!(jump_section(&sections, 8, true), 8, "last section stays");
        assert_eq!(jump_section(&sections, 5, false), 0);
        assert_eq!(jump_section(&sections, 8, false), 3);
        assert_eq!(jump_section(&sections, 1, false), 1, "first section stays");

        let (mut app, _) = picker_app("alice", vec![reaction("+1", &["ana"])]);
        press(&mut app, "+");
        press(&mut app, "<Tab>");
        assert_eq!(view(&app).unwrap().selected, 1);
        assert_eq!(section_of(&view(&app).unwrap().sections, 1), Some(1));
        press(&mut app, "L");
        let second = view(&app).unwrap().sections[1].cells.len();
        assert_eq!(
            view(&app).unwrap().selected,
            1 + second,
            "`L` is the next section"
        );
        press(&mut app, "H");
        assert_eq!(view(&app).unwrap().selected, 1, "`H` is the previous one");
        press(&mut app, "<S-Tab>");
        assert_eq!(view(&app).unwrap().selected, 0);
    }

    #[test]
    fn a_window_keeps_the_selection_and_the_heading_over_its_first_row_in_view() {
        let sections = synthetic(&[30, 30]);
        let grid = Grid::new(&sections, 5);
        // First cell of section two sits on the row after its heading.
        let (start, end) = grid.window(30, 6);
        let at = grid.row_of(30).unwrap();
        assert!(start < at && at < end, "{start}..{end} for row {at}");
        assert_eq!(end - start, 6);
        assert_eq!(grid.window(0, 6), (0, 6));
        assert_eq!(grid.window(59, 6).1, grid.rows.len());
        // A grid that fits is shown whole.
        let small = Grid::new(&synthetic(&[3]), 5);
        assert_eq!(small.window(0, 6), (0, 2));
    }

    #[test]
    fn a_tone_applies_to_emoji_with_tones_in_the_sections_and_in_the_name_sent() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        let find = |app: &App, name: &str| {
            view(app)
                .unwrap()
                .candidates
                .into_iter()
                .find(|c| c.name == name)
        };
        assert_eq!(find(&app, "+1").unwrap().glyph, "👍");
        press(&mut app, "<C-t>");
        press(&mut app, "<C-t>");
        assert_eq!(app.emoji_recents.tone, 2);
        assert!(find(&app, "+1").is_none(), "the plain name is gone");
        let thumbs = find(&app, "+1_tone2").expect("toned");
        assert_eq!(thumbs.glyph, "👍🏼");
        // An emoji without tones does not change.
        assert_eq!(find(&app, "tada").unwrap().glyph, "🎉");
        // Searching shows the tone too, and what is sent carries it.
        search_for(&mut app, "thumbsup");
        let results = view(&app).unwrap().candidates;
        assert_eq!(results[0].name, "+1_tone2");
        app.effects.clear();
        press(&mut app, "<CR>");
        assert_eq!(reacts(&app)[0].1, "+1_tone2");
    }

    #[test]
    fn ctrl_t_cycles_the_tone_in_both_states_and_leaves_the_typed_text() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        press(&mut app, "<C-t>");
        assert_eq!(app.emoji_recents.tone, 1);
        search_for(&mut app, "ab");
        press(&mut app, "<C-t>");
        assert_eq!(app.emoji_recents.tone, 2);
        assert_eq!(app.emoji_box.as_ref().unwrap().query, "ab");
        assert!(typing(&app));
        for _ in 0..4 {
            press(&mut app, "<C-t>");
        }
        assert_eq!(app.emoji_recents.tone, 0, "back to none after five");
        assert_eq!(sent_effects(&app), 6, "every change is saved");
    }

    #[test]
    fn letters_and_tab_while_searching() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        press(&mut app, "/");
        // `h`, `l`, `H` and `L` are text, not movement.
        type_text(&mut app, "hlHL");
        assert_eq!(app.emoji_box.as_ref().unwrap().query, "hlHL");
        press(&mut app, "<BS>");
        press(&mut app, "<BS>");
        press(&mut app, "<BS>");
        press(&mut app, "<BS>");
        type_text(&mut app, "heart");
        press(&mut app, "<Right>");
        assert_eq!(view(&app).unwrap().selected, 1);
        press(&mut app, "<Tab>");
        assert!(typing(&app), "Tab does not leave the search");
    }

    #[test]
    fn a_pick_is_counted_when_added_not_when_taken_back_and_saved() {
        let (mut app, _) = picker_app("alice", vec![reaction("tada", &["me"])]);
        press(&mut app, "+");
        app.effects.clear();
        // Taking `tada` back: nothing is counted or saved.
        press(&mut app, "<CR>");
        assert!(app.emoji_recents.is_empty());
        assert_eq!(sent_effects(&app), 0);
        assert_eq!(
            reacts(&app),
            [("m1".to_string(), "tada".to_string(), false)]
        );

        let (mut app, _) = picker_app("alice", vec![reaction("tada", &["ana"])]);
        press(&mut app, "+");
        app.effects.clear();
        press(&mut app, "<CR>");
        assert_eq!(app.emoji_recents.count_of("tada"), 1);
        assert_eq!(sent_effects(&app), 1);
        assert_eq!(reacts(&app), [("m1".to_string(), "tada".to_string(), true)]);
    }

    #[test]
    fn a_toned_pick_is_counted_under_its_plain_name() {
        let (mut app, _) = picker_app("alice", Vec::new());
        app.emoji_recents.tone = 3;
        press(&mut app, "+");
        search_for(&mut app, "thumbsup");
        press(&mut app, "<CR>");
        assert_eq!(app.emoji_recents.count_of("+1"), 1);
        assert_eq!(app.emoji_recents.count_of("+1_tone3"), 0);
    }

    #[test]
    fn the_loaded_state_arrives_as_an_action() {
        let (mut app, _) = picker_app("alice", Vec::new());
        let mut loaded = crate::app::emoji_recents::Recents::default();
        loaded.record("tada");
        loaded.tone = 4;
        update(&mut app, Action::RecentsLoaded(loaded));
        assert_eq!(app.emoji_recents.tone, 4);
        press(&mut app, "+");
        assert_eq!(kinds(&app)[0], SectionKind::Frequent);
    }

    #[test]
    fn a_custom_emoji_whose_image_failed_is_left_out_of_the_grid_but_still_found() {
        let (mut app, _) = picker_app("alice", Vec::new());
        app.custom_emojis = CustomEmojis::Loaded(vec![custom("good", &[]), custom("gone", &[])]);
        app.images.set_state(
            "gone",
            crate::app::emoji::ImageState::Failed {
                etag: String::new(),
            },
        );
        press(&mut app, "+");
        let view = view(&app).unwrap();
        let custom_cells: Vec<&str> = view
            .sections
            .iter()
            .filter(|s| s.kind == SectionKind::Custom)
            .flat_map(|s| s.cells.iter().map(|c| c.name.as_str()))
            .collect();
        assert_eq!(custom_cells, ["good"]);
        search_for(&mut app, "gone");
        assert_eq!(view_names(&app), ["gone"]);
    }

    fn view_names(app: &App) -> Vec<String> {
        view(app)
            .unwrap()
            .candidates
            .iter()
            .map(|c| c.name.clone())
            .collect()
    }

    #[test]
    fn the_picker_state_never_prints_what_was_typed() {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "+");
        search_for(&mut app, "sentinelquery");
        assert_eq!(
            app.emoji_box.as_ref().unwrap().query,
            "sentinelquery",
            "the whole text was typed"
        );
        let shown = format!("{:?} {:#?}", app.emoji_box, app.emoji_box);
        assert!(!shown.contains("sentinelquery"), "{shown}");
        assert!(shown.contains("query_len"), "{shown}");
    }

    // --- The box opened from a colon in the draft. ---

    /// An app in Insert mode on an empty draft.
    fn insert_app() -> App {
        let (mut app, _) = picker_app("alice", Vec::new());
        press(&mut app, "i");
        assert_eq!(app.mode, Mode::Insert);
        app
    }

    /// Type `text` key by key, spaces included.
    fn type_draft(app: &mut App, text: &str) {
        for c in text.chars() {
            if c == ' ' {
                press(app, "<Space>");
            } else {
                press(app, &c.to_string());
            }
        }
    }

    fn open_box(app: &App) -> bool {
        app.emoji_box
            .as_ref()
            .is_some_and(|b| b.target == Target::Insert)
    }

    #[test]
    fn a_colon_starts_a_shortcode_unless_after_a_word_a_colon_or_inside_code() {
        assert!(colon_starts_shortcode(""));
        assert!(colon_starts_shortcode("ship it "));
        assert!(colon_starts_shortcode("("));
        assert!(colon_starts_shortcode("a,"));
        assert!(colon_starts_shortcode("line\n"));
        assert!(!colon_starts_shortcode("10"));
        assert!(!colon_starts_shortcode("std:"));
        assert!(!colon_starts_shortcode("`code "));
        assert!(!colon_starts_shortcode("```\nfn "));
        assert!(colon_starts_shortcode("`code` "));
    }

    #[test]
    fn a_colon_opens_the_box_searching_at_the_start_and_after_a_space() {
        let mut app = insert_app();
        press(&mut app, ":");
        assert!(open_box(&app));
        assert!(typing(&app));
        assert_eq!(app.input.text(), ":");
        let mut app = insert_app();
        type_draft(&mut app, "ship it :");
        assert!(open_box(&app));
        assert_eq!(app.input.text(), "ship it :");
        assert!(view(&app).unwrap().insert);
    }

    #[test]
    fn colons_in_times_paths_and_code_do_not_open_the_box() {
        for text in ["10:30", "std::fs", "`a :", "```\nx :"] {
            let mut app = insert_app();
            for c in text.chars() {
                match c {
                    ' ' => press(&mut app, "<Space>"),
                    '\n' => press(&mut app, "<S-CR>"),
                    c => press(&mut app, &c.to_string()),
                }
                assert!(!open_box(&app), "{text:?}");
            }
        }
        let mut app = insert_app();
        type_draft(&mut app, "(:");
        assert!(open_box(&app));
    }

    #[test]
    fn a_paste_with_a_colon_does_not_open_the_box() {
        let mut app = insert_app();
        update(&mut app, Action::Paste("see :".to_string()));
        assert!(app.emoji_box.is_none());
        assert_eq!(app.input.text(), "see :");
    }

    #[test]
    fn typing_goes_to_the_search_and_never_to_the_draft_or_the_keymap() {
        let mut app = insert_app();
        press(&mut app, ":");
        type_draft(&mut app, "tadj");
        let b = app.emoji_box.as_ref().unwrap();
        assert_eq!(b.query, "tadj");
        assert_eq!(app.input.text(), ":");
        press(&mut app, "<BS>");
        assert_eq!(app.emoji_box.as_ref().unwrap().query, "tad");
        assert!(
            view(&app)
                .unwrap()
                .candidates
                .iter()
                .any(|c| c.name == "tada")
        );
    }

    #[test]
    fn the_arrows_move_and_enter_puts_that_emoji_in_the_draft() {
        let mut app = insert_app();
        press(&mut app, ":");
        type_draft(&mut app, "tad");
        let second = view(&app).unwrap().candidates[1].glyph.clone();
        press(&mut app, "<Right>");
        press(&mut app, "<CR>");
        assert!(app.emoji_box.is_none());
        assert_eq!(app.input.text(), second);
        assert_eq!(app.mode, Mode::Insert);
    }

    #[test]
    fn enter_on_an_empty_search_takes_the_first_emoji_of_the_grid() {
        let mut app = insert_app();
        press(&mut app, ":");
        let first = view(&app).unwrap().candidates[0].glyph.clone();
        press(&mut app, "<CR>");
        assert_eq!(app.input.text(), first);
    }

    #[test]
    fn a_unicode_emoji_replaces_the_colon_and_the_cursor_follows_it() {
        let mut app = insert_app();
        type_draft(&mut app, "ship it :");
        type_draft(&mut app, "tada");
        press(&mut app, "<CR>");
        assert_eq!(app.input.text(), "ship it \u{1F389}");
        type_draft(&mut app, "!");
        assert_eq!(app.input.text(), "ship it \u{1F389}!");
    }

    #[test]
    fn the_skin_tone_in_use_applies_to_the_inserted_emoji() {
        let mut app = insert_app();
        app.emoji_recents.tone = 3;
        press(&mut app, ":");
        type_draft(&mut app, "thumbsup");
        press(&mut app, "<CR>");
        let want = emojis::get("\u{1F44D}")
            .unwrap()
            .with_skin_tone(SkinTone::Medium)
            .unwrap();
        assert_eq!(app.input.text(), want.as_str());
    }

    #[test]
    fn an_alias_inserts_its_text_and_a_custom_emoji_inserts_its_name() {
        let mut app = insert_app();
        app.settings
            .emoji_aliases
            .insert("partyparrot".to_string(), "\u{1F99C}".to_string());
        app.custom_emojis = emoji::CustomEmojis::Loaded(vec![custom("rtshipit", &[])]);
        press(&mut app, ":");
        type_draft(&mut app, "partyparrot");
        press(&mut app, "<CR>");
        assert_eq!(app.input.text(), "\u{1F99C}");
        type_draft(&mut app, " :");
        type_draft(&mut app, "rtshipit");
        press(&mut app, "<CR>");
        assert_eq!(app.input.text(), "\u{1F99C} :rtshipit:");
    }

    #[test]
    fn the_emoji_replaces_a_colon_in_the_middle_of_a_draft() {
        let mut app = insert_app();
        type_draft(&mut app, "a b");
        press(&mut app, "<Left>");
        press(&mut app, "<Left>");
        type_draft(&mut app, " :");
        assert!(open_box(&app));
        assert_eq!(app.input.text(), "a : b");
        type_draft(&mut app, "tada");
        press(&mut app, "<CR>");
        assert_eq!(app.input.text(), "a \u{1F389} b");
    }

    #[test]
    fn enter_with_no_match_closes_the_box_and_keeps_the_colon() {
        let mut app = insert_app();
        press(&mut app, ":");
        type_draft(&mut app, "zzqqxx");
        press(&mut app, "<CR>");
        assert!(app.emoji_box.is_none());
        assert_eq!(app.input.text(), ":");
        assert!(app.emoji_recents.is_empty());
    }

    #[test]
    fn a_character_that_is_not_a_name_closes_the_box_and_goes_to_the_draft() {
        for (typed, draft) in [(":)", ":)"), (":tada ", ":tada "), (":tada:", ":tada:")] {
            let mut app = insert_app();
            type_draft(&mut app, typed);
            assert!(app.emoji_box.is_none(), "{typed}");
            assert_eq!(app.input.text(), draft);
            assert_eq!(app.mode, Mode::Insert);
        }
    }

    #[test]
    fn a_paste_with_a_space_closes_the_box_after_what_was_typed() {
        let mut app = insert_app();
        press(&mut app, ":");
        type_draft(&mut app, "ta");
        update(&mut app, Action::Paste("da now".to_string()));
        assert!(app.emoji_box.is_none());
        assert_eq!(app.input.text(), ":tada now");
        let mut app = insert_app();
        press(&mut app, ":");
        update(&mut app, Action::Paste("tada".to_string()));
        assert_eq!(app.emoji_box.as_ref().unwrap().query, "tada");
    }

    #[test]
    fn esc_and_ctrl_c_leave_the_colon_and_the_search_out_of_the_draft() {
        for key in ["<Esc>", "<C-c>"] {
            let mut app = insert_app();
            press(&mut app, ":");
            type_draft(&mut app, "tad");
            press(&mut app, key);
            assert!(app.emoji_box.is_none());
            assert_eq!(app.input.text(), ":");
            assert_eq!(app.mode, Mode::Insert);
            assert!(app.emoji_recents.is_empty());
            assert_eq!(sent_effects(&app), 0);
        }
    }

    #[test]
    fn backspace_on_an_empty_search_takes_the_colon_back() {
        let mut app = insert_app();
        type_draft(&mut app, "a :");
        press(&mut app, "<BS>");
        assert!(app.emoji_box.is_none());
        assert_eq!(app.input.text(), "a ");
    }

    #[test]
    fn the_box_from_a_colon_has_no_message_section_and_no_marks() {
        let mut app = insert_app();
        press(&mut app, ":");
        let shown = view(&app).unwrap();
        assert_ne!(shown.sections[0].kind, SectionKind::OnMessage);
        assert!(shown.candidates.iter().all(|c| c.count == 0 && !c.own));
        // The server's table accepts every emoji today, so the refusal path is the rule itself:
        // a draft falls back to the crate's first shortcode, a reaction offers nothing.
        assert_eq!(sendable("tada", true), Some("tada"));
    }

    #[test]
    fn a_pick_is_counted_and_saved_and_escape_counts_nothing() {
        let mut app = insert_app();
        press(&mut app, ":");
        type_draft(&mut app, "tada");
        press(&mut app, "<CR>");
        assert_eq!(app.emoji_recents.count_of("tada"), 1);
        assert!(
            app.effects
                .iter()
                .any(|e| matches!(e, Effect::SaveRecents(_)))
        );
    }

    #[test]
    fn leaving_insert_mode_while_the_box_is_open_closes_it() {
        let mut app = insert_app();
        press(&mut app, ":");
        app.mode = Mode::Normal;
        close_if_stale(&mut app);
        assert!(app.emoji_box.is_none());
    }
}

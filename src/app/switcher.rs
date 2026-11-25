//! The room switcher (`switch_room`, `Ctrl-k`): a popup with a search line and a ranked list of
//! the joined rooms, to jump to one by typing a few letters.
//!
//! The switcher is state on `App`, not a mode: the reducer hands it every key while it is open,
//! like the reaction picker. Everything it shows is derived from the rooms, the recently visited
//! list and the query on every draw, so a live change shows up without any plumbing. Pure: no I/O
//! and no `Frame`.

use crate::app::{App, MAX_CMDLINE, history};
use crate::data::RoomId;
use crate::input::key::Chord;
use crate::ui::sanitize::sanitize_line;
use crossterm::event::{KeyCode, KeyModifiers};
use std::cmp::Reverse;

/// The open switcher: what has been typed and which room is selected. The selection is kept as a
/// room, not a row number, so a list that changes under it keeps it on the same room.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Switcher {
    pub query: String,
    /// The selected room, `None` for the first row.
    pub selected: Option<RoomId>,
}

/// Redacting `Debug`: the typed text is never printed, only its length.
impl std::fmt::Debug for Switcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Switcher")
            .field("query_len", &self.query.chars().count())
            .field("selected", &self.selected)
            .finish()
    }
}

/// What the drawer shows for the open switcher, derived from the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct View {
    pub query: String,
    /// Indexes into `App::rooms`, best first.
    pub rooms: Vec<usize>,
    /// Index into `rooms`, always in range when there are any.
    pub selected: usize,
}

/// How well a query matches a room name; lower is better. See `match_tier`.
type Tier = u8;

/// A character starts a word when it is the first of the name or follows a character that is not
/// a letter or a digit (`-`, `_`, `.`, a space, `/`, ...).
fn word_starts(name: &[char]) -> impl Iterator<Item = usize> + '_ {
    (0..name.len()).filter(|&i| i == 0 || !name[i - 1].is_alphanumeric())
}

/// True when `needle` occurs in `hay` in order, with anything between.
fn is_subsequence(needle: &[char], hay: &[char]) -> bool {
    let mut hay = hay.iter();
    needle.iter().all(|c| hay.any(|h| h == c))
}

/// How well `query` matches `name`, or `None` when it does not match at all. Case is ignored.
/// A match needs every character of the query in the name, in order. The tiers, best first:
/// 0 the name starts with the query, 1 the match starts at the start of a word, 2 the query is a
/// contiguous run inside a word, 3 the characters are scattered. An empty query matches at tier 0.
pub fn match_tier(name: &str, query: &str) -> Option<Tier> {
    let name: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if query.is_empty() || name.starts_with(&query) {
        return Some(0);
    }
    let anchored = word_starts(&name)
        .any(|i| name[i] == query[0] && is_subsequence(&query[1..], &name[i + 1..]));
    if anchored {
        return Some(1);
    }
    if name.windows(query.len()).any(|w| w == query.as_slice()) {
        return Some(2);
    }
    is_subsequence(&query, &name).then_some(3)
}

/// The rooms the switcher offers for `query`, as indexes into `app.rooms`, best first.
///
/// With an empty query: unread rooms first, then by most recent visit, then the rooms never
/// visited by most recent activity, then by name. With a query only the matching rooms are listed,
/// ranked by `match_tier` and, within a tier, in that same order.
pub fn ranked(app: &App, query: &str) -> Vec<usize> {
    let rank_of = |id: &RoomId| {
        app.recent_rooms
            .iter()
            .position(|recent| recent == id)
            .unwrap_or(usize::MAX)
    };
    let mut found: Vec<(Tier, usize)> = app
        .rooms
        .iter()
        .enumerate()
        .filter_map(|(i, room)| match_tier(&sanitize_line(&room.name), query).map(|tier| (tier, i)))
        .collect();
    found.sort_by(|&(ta, a), &(tb, b)| {
        let (a, b) = (&app.rooms[a], &app.rooms[b]);
        let key = |room: &crate::data::Room| {
            (
                !room.unread.is_unread(),
                rank_of(&room.id),
                Reverse(room.last_activity),
                room.name.clone(),
            )
        };
        ta.cmp(&tb).then_with(|| key(a).cmp(&key(b)))
    });
    found.into_iter().map(|(_, i)| i).collect()
}

/// The switcher's content now, `None` when it is closed.
pub fn view(app: &App) -> Option<View> {
    let switcher = app.switcher.as_ref()?;
    let rooms = ranked(app, &switcher.query);
    let selected = switcher
        .selected
        .as_ref()
        .and_then(|id| rooms.iter().position(|&i| app.rooms[i].id == *id))
        .unwrap_or(0);
    Some(View {
        query: switcher.query.clone(),
        rooms,
        selected,
    })
}

/// `switch_room`: open the switcher. It changes neither the mode, the focus nor the open room,
/// and it opens already taking text.
pub fn open(app: &mut App) {
    app.matcher.reset();
    app.switcher = Some(Switcher::default());
}

/// A key while the switcher is open: it never reaches the keymap. `Esc` and `<C-c>` close it
/// without choosing, so nothing can lock the user in. Characters are the query, `<C-j>`, `<C-k>`, `<C-n>`, `<C-p>`
/// and the arrows move the selection, `Enter` opens the selected room, `Backspace` deletes a
/// character and `<C-u>` clears the query. Nothing else does anything.
pub fn key(app: &mut App, chord: Chord) {
    let ctrl = |c: char| chord.code == KeyCode::Char(c) && chord.mods == KeyModifiers::CONTROL;
    if ctrl('c') || (chord.code == KeyCode::Esc && chord.mods.is_empty()) {
        app.switcher = None;
    } else if ctrl('n') || ctrl('j') || chord.code == KeyCode::Down {
        move_selection(app, true);
    } else if ctrl('p') || ctrl('k') || chord.code == KeyCode::Up {
        move_selection(app, false);
    } else if chord.code == KeyCode::Enter {
        choose(app);
    } else if ctrl('u') {
        edit(app, |query| query.clear());
    } else if chord.code == KeyCode::Backspace {
        edit(app, |query| {
            query.pop();
        });
    } else if let KeyCode::Char(c) = chord.code
        && chord.mods.is_empty()
        && !c.is_control()
    {
        edit(app, |query| {
            if query.chars().count() < MAX_CMDLINE {
                query.push(c);
            }
        });
    }
}

/// A paste: one line, capped, control characters removed.
pub fn paste(app: &mut App, text: &str) {
    let Some(room_left) = app
        .switcher
        .as_ref()
        .map(|s| MAX_CMDLINE.saturating_sub(s.query.chars().count()))
    else {
        return;
    };
    let text: String = sanitize_line(text).chars().take(room_left).collect();
    edit(app, |query| query.push_str(&text));
}

/// Change the query and put the selection back on the first row.
fn edit(app: &mut App, change: impl FnOnce(&mut String)) {
    if let Some(switcher) = app.switcher.as_mut() {
        change(&mut switcher.query);
        switcher.selected = None;
    }
}

/// One row down (`forward`) or up, stopping at either end.
fn move_selection(app: &mut App, forward: bool) {
    let Some(view) = view(app) else {
        return;
    };
    let last = view.rooms.len().saturating_sub(1);
    let next = if forward {
        (view.selected + 1).min(last)
    } else {
        view.selected.saturating_sub(1)
    };
    let id = view.rooms.get(next).map(|&i| app.rooms[i].id.clone());
    if let Some(switcher) = app.switcher.as_mut() {
        switcher.selected = id;
    }
}

/// `Enter`: open the selected room and close the switcher. With nothing to select nothing
/// happens and the switcher stays open. Choosing the room that is already open only closes it,
/// so that room, its cursor and the focus stay as they were.
fn choose(app: &mut App) {
    let Some(view) = view(app) else {
        return;
    };
    let Some(&index) = view.rooms.get(view.selected) else {
        return;
    };
    let id = app.rooms[index].id.clone();
    app.switcher = None;
    if app.current_room.as_ref() != Some(&id) {
        // The same two steps as `Enter` on a sidebar room: `open_room` shows it, and `on_open`
        // asks for its first page of messages when the room has none yet.
        app.open_room(id.clone());
        history::on_open(app, &id);
    }
}

/// Keep the open switcher in step with the rooms. The list is derived on every draw, so there is
/// nothing to rebuild; this only drops a selection whose room is gone, which puts it back on the
/// first row. The reducer calls it after every action.
pub fn refresh(app: &mut App) {
    let Some(selected) = app.switcher.as_ref().and_then(|s| s.selected.clone()) else {
        return;
    };
    if !app.rooms.iter().any(|room| room.id == selected)
        && let Some(switcher) = app.switcher.as_mut()
    {
        switcher.selected = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::update::update;
    use crate::app::{Focus, Mode};
    use crate::data::{Room, RoomKind, Unread};
    use crate::test_util::app_80x24;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};

    fn press(app: &mut App, code: KeyCode, mods: KeyModifiers) {
        update(
            app,
            Action::Key(KeyEvent {
                code,
                modifiers: mods,
                kind: KeyEventKind::Press,
                state: KeyEventState::NONE,
            }),
        );
    }

    fn ctrl(app: &mut App, c: char) {
        press(app, KeyCode::Char(c), KeyModifiers::CONTROL);
    }

    fn type_text(app: &mut App, text: &str) {
        for c in text.chars() {
            press(app, KeyCode::Char(c), KeyModifiers::NONE);
        }
    }

    fn room(id: &str, name: &str, unread: u32, activity: i64) -> Room {
        Room {
            id: RoomId(id.to_string()),
            name: name.to_string(),
            kind: RoomKind::Channel,
            encrypted: false,
            private: false,
            unread: Unread {
                flagged: false,
                count: unread,
                mentions: 0,
            },
            last_seen: None,
            last_activity: activity,
        }
    }

    /// An app holding exactly `rooms`, nothing open.
    fn app_with(rooms: Vec<Room>) -> App {
        let mut app = app_80x24();
        app.rooms = rooms;
        app.folders.clear();
        app.messages.clear();
        app.current_room = None;
        app.recent_rooms.clear();
        app
    }

    fn names(app: &App, query: &str) -> Vec<String> {
        ranked(app, query)
            .into_iter()
            .map(|i| app.rooms[i].name.clone())
            .collect()
    }

    // -- the matcher --------------------------------------------------------------------------

    #[test]
    fn tiers_run_from_prefix_to_scattered() {
        assert_eq!(match_tier("dev-ops", "dev"), Some(0));
        assert_eq!(match_tier("team-dev", "dev"), Some(1));
        assert_eq!(match_tier("general", "ner"), Some(2));
        assert_eq!(match_tier("platform", "lfm"), Some(3));
    }

    #[test]
    fn a_word_start_is_after_a_non_alphanumeric() {
        assert_eq!(match_tier("a_b.c d/dev", "dev"), Some(1));
        assert_eq!(match_tier("xdev", "dev"), Some(2));
    }

    #[test]
    fn case_is_ignored() {
        assert_eq!(match_tier("General", "GEN"), Some(0));
        assert_eq!(match_tier("general", "GNRL"), match_tier("general", "gnrl"));
    }

    #[test]
    fn a_subsequence_matches_and_a_miss_does_not() {
        assert!(match_tier("general", "gnrl").is_some());
        assert_eq!(match_tier("general", "glx"), None);
        // Order matters: the characters must come in the order typed.
        assert_eq!(match_tier("general", "lg"), None);
        assert_eq!(match_tier("ab", "abc"), None);
    }

    #[test]
    fn the_empty_query_matches_everything() {
        assert_eq!(match_tier("anything", ""), Some(0));
        assert_eq!(match_tier("", ""), Some(0));
    }

    #[test]
    fn open_room_keeps_the_recent_list_newest_first() {
        let mut app = app_with(vec![
            room("a", "alpha", 0, 1),
            room("b", "beta", 0, 2),
            room("c", "gamma", 0, 3),
        ]);
        let id = |s: &str| RoomId(s.to_string());
        app.open_room(id("a"));
        app.open_room(id("b"));
        assert_eq!(app.recent_rooms, [id("b"), id("a")]);
        app.open_room(id("a"));
        assert_eq!(app.recent_rooms, [id("a"), id("b")]);
        // Opening the room that is already open changes nothing.
        app.open_room(id("a"));
        assert_eq!(app.recent_rooms, [id("a"), id("b")]);
        // `Ctrl-k` then Down then Enter goes back to the previous room.
        ctrl(&mut app, 'k');
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.current_room, Some(id("b")));
    }

    // -- the list -----------------------------------------------------------------------------

    #[test]
    fn empty_query_lists_recent_first_then_activity_then_name() {
        let mut app = app_with(vec![
            room("a", "alpha", 0, 10),
            room("b", "beta", 0, 50),
            room("c", "gamma", 0, 50),
            room("d", "delta", 0, 20),
        ]);
        app.recent_rooms = vec![RoomId("d".into()), RoomId("a".into())];
        // Visited: delta then alpha. Never visited by activity (50, 50 tie by name): beta, gamma.
        assert_eq!(names(&app, ""), ["delta", "alpha", "beta", "gamma"]);
    }

    #[test]
    fn unread_rooms_come_before_visited_read_rooms() {
        let mut app = app_with(vec![
            room("a", "alpha", 0, 10),
            room("b", "beta", 0, 10),
            room("c", "gamma", 3, 1),
        ]);
        app.recent_rooms = vec![RoomId("b".into()), RoomId("a".into())];
        assert_eq!(names(&app, ""), ["gamma", "beta", "alpha"]);
    }

    #[test]
    fn a_flagged_room_without_a_count_is_unread() {
        let mut app = app_with(vec![room("a", "alpha", 0, 10), room("b", "beta", 0, 1)]);
        app.rooms[1].unread.flagged = true;
        assert_eq!(names(&app, ""), ["beta", "alpha"]);
    }

    #[test]
    fn nothing_visited_orders_by_activity() {
        let app = app_with(vec![
            room("a", "alpha", 0, 10),
            room("b", "beta", 0, 30),
            room("c", "gamma", 0, 20),
        ]);
        assert_eq!(names(&app, ""), ["beta", "gamma", "alpha"]);
    }

    #[test]
    fn a_query_lists_only_matches_best_tier_first() {
        let app = app_with(vec![
            room("a", "team-dev", 0, 99),
            room("b", "dev-ops", 0, 1),
            room("c", "random", 0, 50),
        ]);
        assert_eq!(names(&app, "dev"), ["dev-ops", "team-dev"]);
        assert!(names(&app, "zzz").is_empty());
    }

    #[test]
    fn within_a_tier_the_empty_query_order_decides() {
        let mut app = app_with(vec![room("a", "dev-b", 0, 1), room("b", "dev-a", 0, 2)]);
        assert_eq!(names(&app, "dev"), ["dev-a", "dev-b"]);
        app.recent_rooms = vec![RoomId("a".into())];
        assert_eq!(names(&app, "dev"), ["dev-b", "dev-a"]);
    }

    #[test]
    fn rooms_in_a_collapsed_folder_are_listed() {
        let mut app = app_80x24();
        let in_folder = app.folders.iter().find_map(|f| f.room_ids.first().cloned());
        let id = in_folder.expect("the fake data has a folder with a room");
        app.sidebar.collapsed = app.folders.iter().map(|f| f.id.clone()).collect();
        crate::app::sidebar::on_rows_changed(&mut app);
        let listed = ranked(&app, "");
        assert!(listed.iter().any(|&i| app.rooms[i].id == id));
        assert_eq!(listed.len(), app.rooms.len());
    }

    #[test]
    fn a_hostile_name_is_matched_as_it_is_shown() {
        let app = app_with(vec![room("a", "ev\u{1b}[31mil", 0, 1)]);
        let shown = sanitize_line(&app.rooms[0].name).into_owned();
        assert!(!shown.contains('\u{1b}'));
        assert_eq!(ranked(&app, &shown).len(), 1);
    }

    // -- opening, typing, choosing ------------------------------------------------------------

    #[test]
    fn ctrl_k_opens_it_without_changing_mode_focus_or_room() {
        for focus in [Focus::Sidebar, Focus::Messages] {
            let mut app = app_80x24();
            app.focus = focus;
            let room = app.current_room.clone();
            ctrl(&mut app, 'k');
            assert!(app.switcher.is_some(), "{focus:?}");
            assert_eq!(app.mode, Mode::Normal);
            assert_eq!(app.focus, focus);
            assert_eq!(app.current_room, room);
        }
    }

    #[test]
    fn it_opens_with_no_room_open_and_with_no_rooms() {
        let mut app = app_with(Vec::new());
        ctrl(&mut app, 'k');
        let view = view(&app).expect("open");
        assert!(view.rooms.is_empty());
        // Enter with nothing to choose does nothing and keeps it open.
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.switcher.is_some());
    }

    #[test]
    fn typing_goes_to_the_query_not_the_keymap() {
        let mut app = app_80x24();
        ctrl(&mut app, 'k');
        type_text(&mut app, "gj");
        assert_eq!(app.switcher.as_ref().expect("open").query, "gj");
        assert_eq!(app.mode, Mode::Normal);
    }

    #[test]
    fn backspace_and_ctrl_u_edit_the_query() {
        let mut app = app_80x24();
        ctrl(&mut app, 'k');
        type_text(&mut app, "abc");
        press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(app.switcher.as_ref().expect("open").query, "ab");
        ctrl(&mut app, 'u');
        assert_eq!(app.switcher.as_ref().expect("open").query, "");
        // Backspace on an empty query keeps it open.
        press(&mut app, KeyCode::Backspace, KeyModifiers::NONE);
        assert!(app.switcher.is_some());
    }

    #[test]
    fn the_query_is_capped() {
        let mut app = app_80x24();
        ctrl(&mut app, 'k');
        type_text(&mut app, &"x".repeat(MAX_CMDLINE + 50));
        assert_eq!(
            app.switcher.as_ref().expect("open").query.chars().count(),
            MAX_CMDLINE
        );
    }

    #[test]
    fn paste_is_one_line() {
        let mut app = app_80x24();
        ctrl(&mut app, 'k');
        update(&mut app, Action::Paste("ge\nne\u{1b}ral".to_string()));
        let query = app.switcher.as_ref().expect("open").query.clone();
        assert!(
            !query.contains('\n') && !query.contains('\u{1b}'),
            "{query:?}"
        );
    }

    #[test]
    fn selection_moves_and_stops_at_the_ends() {
        let mut app = app_with(vec![
            room("a", "alpha", 0, 3),
            room("b", "beta", 0, 2),
            room("c", "gamma", 0, 1),
        ]);
        ctrl(&mut app, 'k');
        assert_eq!(view(&app).expect("open").selected, 0);
        ctrl(&mut app, 'n');
        ctrl(&mut app, 'n');
        assert_eq!(view(&app).expect("open").selected, 2);
        press(&mut app, KeyCode::Down, KeyModifiers::NONE);
        assert_eq!(view(&app).expect("open").selected, 2);
        ctrl(&mut app, 'p');
        assert_eq!(view(&app).expect("open").selected, 1);
        press(&mut app, KeyCode::Up, KeyModifiers::NONE);
        press(&mut app, KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(view(&app).expect("open").selected, 0);
    }

    #[test]
    fn ctrl_j_and_ctrl_k_move_like_ctrl_n_and_ctrl_p() {
        let mut app = app_with(vec![
            room("a", "alpha", 0, 3),
            room("b", "beta", 0, 2),
            room("c", "gamma", 0, 1),
        ]);
        ctrl(&mut app, 'k');
        ctrl(&mut app, 'j');
        ctrl(&mut app, 'j');
        assert_eq!(view(&app).expect("open").selected, 2);
        ctrl(&mut app, 'j');
        assert_eq!(view(&app).expect("open").selected, 2, "stops at the end");
        ctrl(&mut app, 'k');
        assert_eq!(view(&app).expect("open").selected, 1);
        ctrl(&mut app, 'k');
        ctrl(&mut app, 'k');
        assert_eq!(view(&app).expect("open").selected, 0, "stops at the start");
        // Ctrl-k while open moves; it does not reopen or close the switcher.
        assert!(app.switcher.is_some());
    }

    #[test]
    fn an_edit_puts_the_selection_on_the_first_row() {
        let mut app = app_with(vec![
            room("a", "alpha", 0, 3),
            room("b", "beta", 0, 2),
            room("c", "gamma", 0, 1),
        ]);
        ctrl(&mut app, 'k');
        ctrl(&mut app, 'n');
        ctrl(&mut app, 'n');
        type_text(&mut app, "a");
        assert_eq!(view(&app).expect("open").selected, 0);
    }

    #[test]
    fn enter_opens_the_room_focuses_messages_and_closes() {
        let mut app = app_80x24();
        app.focus = Focus::Sidebar;
        ctrl(&mut app, 'k');
        type_text(&mut app, "random");
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.switcher.is_none());
        let open = app.current_room.clone().expect("a room opened");
        assert_eq!(
            app.rooms.iter().find(|r| r.id == open).expect("held").name,
            "random"
        );
        assert_eq!(app.focus, Focus::Messages);
    }

    #[test]
    fn choosing_a_room_never_opened_loads_its_history() {
        use crate::effect::Effect;
        let mut app = app_80x24();
        // The fake source marks every room as loaded; a live server does not, so start clean.
        app.history.clear();
        app.effects.clear();
        let random = app
            .rooms
            .iter()
            .find(|r| r.name == "random")
            .expect("fake")
            .id
            .clone();
        ctrl(&mut app, 'k');
        type_text(&mut app, "random");
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.current_room, Some(random.clone()));
        let asked = app
            .effects
            .iter()
            .filter(|e| matches!(e, Effect::LoadHistory { room, .. } if *room == random))
            .count();
        assert_eq!(asked, 1, "{:?}", app.effects);
    }

    #[test]
    fn choosing_the_same_way_as_the_sidebar_gives_the_same_effects() {
        let mut via_sidebar = app_80x24();
        via_sidebar.history.clear();
        via_sidebar.effects.clear();
        let id = via_sidebar
            .rooms
            .iter()
            .find(|r| r.name == "dev-team")
            .expect("fake")
            .id
            .clone();
        via_sidebar.open_room(id.clone());
        history::on_open(&mut via_sidebar, &id);

        let mut via_switcher = app_80x24();
        via_switcher.history.clear();
        via_switcher.effects.clear();
        ctrl(&mut via_switcher, 'k');
        type_text(&mut via_switcher, "dev-team");
        press(&mut via_switcher, KeyCode::Enter, KeyModifiers::NONE);

        assert_eq!(
            format!("{:?}", via_switcher.effects),
            format!("{:?}", via_sidebar.effects)
        );
        assert_eq!(via_switcher.history.len(), via_sidebar.history.len());
    }

    #[test]
    fn choosing_the_open_room_only_closes_it() {
        let mut app = app_80x24();
        let general = app
            .rooms
            .iter()
            .find(|r| r.name == "general")
            .expect("fake")
            .id
            .clone();
        app.open_room(general.clone());
        app.focus = Focus::Sidebar;
        ctrl(&mut app, 'k');
        type_text(&mut app, "general");
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert!(app.switcher.is_none());
        assert_eq!(app.current_room, Some(general));
        assert_eq!(app.focus, Focus::Sidebar, "the focus was left alone");
    }

    #[test]
    fn an_unsent_draft_is_kept_across_a_switch() {
        let mut app = app_80x24();
        let general = app
            .rooms
            .iter()
            .find(|r| r.name == "general")
            .expect("fake")
            .id
            .clone();
        app.open_room(general.clone());
        app.input.insert_str("half a thought");
        ctrl(&mut app, 'k');
        type_text(&mut app, "random");
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_ne!(app.current_room, Some(general.clone()));
        assert!(app.input.is_empty());
        assert_eq!(
            app.drafts
                .get(&crate::app::DraftKey::room(general.clone()))
                .map(|d| d.text()),
            Some("half a thought".to_string())
        );
    }

    #[test]
    fn esc_and_ctrl_c_close_without_changing_anything() {
        for close in [
            (KeyCode::Esc, KeyModifiers::NONE),
            (KeyCode::Char('c'), KeyModifiers::CONTROL),
        ] {
            let mut app = app_80x24();
            let general = app.rooms[0].id.clone();
            app.open_room(general.clone());
            app.focus = Focus::Sidebar;
            let recent = app.recent_rooms.clone();
            ctrl(&mut app, 'k');
            type_text(&mut app, "random");
            press(&mut app, close.0, close.1);
            assert!(app.switcher.is_none());
            assert!(!app.should_quit, "Ctrl-c must not quit");
            assert_eq!(app.current_room, Some(general));
            assert_eq!(app.focus, Focus::Sidebar);
            assert_eq!(app.recent_rooms, recent);
        }
    }

    #[test]
    fn a_room_that_disappears_is_never_opened() {
        let mut app = app_with(vec![room("a", "alpha", 0, 3), room("b", "beta", 0, 2)]);
        ctrl(&mut app, 'k');
        ctrl(&mut app, 'n');
        assert_eq!(app.rooms[view(&app).expect("open").rooms[1]].name, "beta");
        // A live update removes the selected room.
        let remaining = vec![room("a", "alpha", 0, 3)];
        update(&mut app, Action::RoomsLoaded(remaining));
        assert_eq!(view(&app).expect("open").selected, 0);
        press(&mut app, KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(app.current_room, Some(RoomId("a".into())));
    }

    #[test]
    fn the_selection_stays_on_its_room_when_the_list_changes() {
        let mut app = app_with(vec![
            room("a", "alpha", 0, 3),
            room("b", "beta", 0, 2),
            room("c", "gamma", 0, 1),
        ]);
        ctrl(&mut app, 'k');
        ctrl(&mut app, 'n');
        ctrl(&mut app, 'n');
        // A new, more active room is listed first: gamma keeps the selection at its new row.
        let mut rooms = app.rooms.clone();
        rooms.push(room("d", "delta", 0, 100));
        update(&mut app, Action::RoomsLoaded(rooms));
        let view = view(&app).expect("open");
        assert_eq!(app.rooms[view.rooms[view.selected]].name, "gamma");
    }

    #[test]
    fn debug_does_not_print_the_query() {
        let switcher = Switcher {
            query: "hunter2".to_string(),
            selected: None,
        };
        let shown = format!("{switcher:?}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(shown.contains("query_len"));
    }
}

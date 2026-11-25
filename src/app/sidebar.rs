//! Sidebar model: sections (Unread, Folders, Recent), fold state and the cursor over selectable
//! rows (D-02, D-11, D-12, D-19). The rows are derived from `App` data by `rows`; only the
//! cursor and the collapsed folders are stored.

use crate::action::Action;
use crate::app::App;
use crate::data::{FolderId, Room, RoomId};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    Unread,
    Folders,
    Recent,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Section::Unread => "Unread",
            Section::Folders => "Folders",
            Section::Recent => "Recent",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarRow {
    Header(Section),
    Folder {
        id: FolderId,
        open: bool,
        unread: usize,
    },
    Room {
        id: RoomId,
        indent: u8,
    },
}

impl SidebarRow {
    pub fn is_selectable(&self) -> bool {
        !matches!(self, SidebarRow::Header(_))
    }
}

#[derive(Debug, Default)]
pub struct SidebarState {
    pub cursor: usize,
    pub collapsed: HashSet<FolderId>,
}

/// Where a room is listed. Every room has exactly one home, with precedence Unread over Folder
/// over Recent (A1): an unread room is under Unread even when a folder lists it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Home {
    Unread,
    /// Index into `app.folders` of the first folder that lists the room (A19).
    Folder(usize),
    Recent,
}

/// Newest activity first, ties by name.
fn by_activity(mut rooms: Vec<&Room>) -> Vec<&Room> {
    rooms.sort_by(|a, b| {
        b.last_activity
            .cmp(&a.last_activity)
            .then_with(|| a.name.cmp(&b.name))
    });
    rooms
}

/// The sidebar rows, derived from the rooms, folders and fold state. Pure; a section without
/// rows has no header. A room whose folder is collapsed is hidden and listed nowhere else.
pub fn rows(app: &App) -> Vec<SidebarRow> {
    let homed: Vec<(&Room, Home)> = app
        .rooms
        .iter()
        .map(|room| {
            let home = if room.unread.is_unread() {
                Home::Unread
            } else {
                match app
                    .folders
                    .iter()
                    .position(|f| f.room_ids.contains(&room.id))
                {
                    Some(index) => Home::Folder(index),
                    None => Home::Recent,
                }
            };
            (room, home)
        })
        .collect();
    let with_home = |wanted: Home| -> Vec<&Room> {
        by_activity(
            homed
                .iter()
                .filter(|(_, home)| *home == wanted)
                .map(|(room, _)| *room)
                .collect(),
        )
    };
    let room_row = |room: &Room, indent: u8| SidebarRow::Room {
        id: room.id.clone(),
        indent,
    };

    let mut out = Vec::new();

    let unread = with_home(Home::Unread);
    if !unread.is_empty() {
        out.push(SidebarRow::Header(Section::Unread));
        out.extend(unread.into_iter().map(|room| room_row(room, 0)));
    }

    if !app.folders.is_empty() {
        out.push(SidebarRow::Header(Section::Folders));
        for (index, folder) in app.folders.iter().enumerate() {
            let open = !app.sidebar.collapsed.contains(&folder.id);
            let unread = app
                .rooms
                .iter()
                .filter(|room| room.unread.is_unread() && folder.room_ids.contains(&room.id))
                .count();
            out.push(SidebarRow::Folder {
                id: folder.id.clone(),
                open,
                unread,
            });
            if !open {
                continue;
            }
            let mut seen = HashSet::new();
            for room_id in &folder.room_ids {
                if !seen.insert(room_id) {
                    continue;
                }
                let listed = homed
                    .iter()
                    .find(|(room, home)| room.id == *room_id && *home == Home::Folder(index));
                if let Some((room, _)) = listed {
                    out.push(room_row(room, 1));
                }
            }
        }
    }

    let recent = with_home(Home::Recent);
    if !recent.is_empty() {
        out.push(SidebarRow::Header(Section::Recent));
        out.extend(recent.into_iter().map(|room| room_row(room, 0)));
    }
    out
}

/// The row under the cursor, `None` when there are no rows.
pub fn selected_row(app: &App) -> Option<SidebarRow> {
    rows(app).into_iter().nth(app.sidebar.cursor)
}

/// The room under the cursor; `None` when the cursor is not on a room row.
pub fn selected_room(app: &App) -> Option<RoomId> {
    match selected_row(app)? {
        SidebarRow::Room { id, .. } => Some(id),
        _ => None,
    }
}

/// Indices into `rows` of the selectable rows, in order.
fn selectable_indices(rows: &[SidebarRow]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.is_selectable())
        .map(|(index, _)| index)
        .collect()
}

/// Rows a half-page motion moves: half the sidebar's inner height, at least one.
fn half_page(app: &App) -> usize {
    let inner = app.areas().sidebar.height.saturating_sub(2);
    usize::from(inner / 2).max(1)
}

/// Collapse or expand `id`, then put the cursor on that folder's row.
fn set_folder_open(app: &mut App, id: &FolderId, open: bool) {
    if open {
        app.sidebar.collapsed.remove(id);
    } else {
        app.sidebar.collapsed.insert(id.clone());
    }
    let position = rows(app)
        .iter()
        .position(|row| matches!(row, SidebarRow::Folder { id: row_id, .. } if row_id == id));
    match position {
        Some(index) => app.sidebar.cursor = index,
        None => on_rows_changed(app),
    }
}

/// Apply a motion to the sidebar. Cursor motions move across selectable rows only and clamp at
/// the ends. `h` / `l` fold and unfold folder rows and do nothing on room rows (D-19). `za`
/// toggles a folder, or collapses the parent of an indented room; Enter opens a room or toggles
/// a folder (A2).
pub fn apply_motion(app: &mut App, motion: &Action, count: Option<u32>) {
    let all = rows(app);
    let selectable = selectable_indices(&all);
    let Some(last) = selectable.len().checked_sub(1) else {
        return;
    };
    let n = count.unwrap_or(1) as usize;
    let at = selectable
        .iter()
        .position(|&index| index >= app.sidebar.cursor)
        .unwrap_or(last);
    let target = match motion {
        Action::MoveDown => Some(at.saturating_add(n)),
        Action::MoveUp => Some(at.saturating_sub(n)),
        Action::GotoTop => Some(count.map_or(0, |_| n.saturating_sub(1))),
        Action::GotoBottom => Some(count.map_or(last, |_| n.saturating_sub(1))),
        Action::HalfPageDown => Some(at.saturating_add(half_page(app).saturating_mul(n))),
        Action::HalfPageUp => Some(at.saturating_sub(half_page(app).saturating_mul(n))),
        _ => None,
    };
    if let Some(target) = target {
        app.sidebar.cursor = selectable[target.min(last)];
        return;
    }

    let Some(row) = all.get(app.sidebar.cursor) else {
        return;
    };
    match (motion, row) {
        (Action::MoveLeft, SidebarRow::Folder { id, open: true, .. }) => {
            set_folder_open(app, id, false);
        }
        (
            Action::MoveRight,
            SidebarRow::Folder {
                id, open: false, ..
            },
        ) => {
            set_folder_open(app, id, true);
        }
        (Action::ToggleFold | Action::Select, SidebarRow::Folder { id, open, .. }) => {
            set_folder_open(app, id, !*open);
        }
        (Action::ToggleFold, SidebarRow::Room { indent, .. }) if *indent > 0 => {
            let parent = all[..app.sidebar.cursor]
                .iter()
                .rev()
                .find_map(|above| match above {
                    SidebarRow::Folder { id, .. } => Some(id.clone()),
                    _ => None,
                });
            if let Some(id) = parent {
                set_folder_open(app, &id, false);
            }
        }
        (Action::Select, SidebarRow::Room { id, .. }) => {
            let id = id.clone();
            app.open_room(id.clone());
            crate::app::history::on_open(app, &id);
        }
        _ => {}
    }
}

/// Keep the cursor on a selectable row after the rows changed: clamp it, and if it sits on a
/// header move to the next selectable row, or the previous one when none follows.
pub fn on_rows_changed(app: &mut App) {
    let all = rows(app);
    let selectable = selectable_indices(&all);
    let Some(&last) = selectable.last() else {
        app.sidebar.cursor = 0;
        return;
    };
    let cursor = app.sidebar.cursor.min(all.len().saturating_sub(1));
    app.sidebar.cursor = selectable
        .iter()
        .copied()
        .find(|&index| index >= cursor)
        .unwrap_or(last);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Focus;
    use crate::test_util::{app_80x24, press, render_to_string};
    use ratatui::layout::Size;

    fn rid(name: &str) -> RoomId {
        RoomId(name.to_string())
    }

    fn fid(name: &str) -> FolderId {
        FolderId(name.to_string())
    }

    fn room(name: &str) -> SidebarRow {
        SidebarRow::Room {
            id: rid(name),
            indent: 0,
        }
    }

    fn child(name: &str) -> SidebarRow {
        SidebarRow::Room {
            id: rid(name),
            indent: 1,
        }
    }

    fn folder(name: &str, open: bool, unread: usize) -> SidebarRow {
        SidebarRow::Folder {
            id: fid(name),
            open,
            unread,
        }
    }

    fn selected(app: &App) -> SidebarRow {
        selected_row(app).expect("a selected row")
    }

    fn cursor_to(app: &mut App, row: &SidebarRow) {
        let index = rows(app)
            .iter()
            .position(|r| r == row)
            .expect("row is listed");
        app.sidebar.cursor = index;
    }

    fn has_room(app: &App, name: &str) -> bool {
        rows(app)
            .iter()
            .any(|r| matches!(r, SidebarRow::Room { id, .. } if id.0 == name))
    }

    fn selectable(app: &App) -> Vec<SidebarRow> {
        rows(app)
            .into_iter()
            .filter(SidebarRow::is_selectable)
            .collect()
    }

    #[test]
    fn sections_order_and_membership() {
        let app = app_80x24();
        let expected = vec![
            SidebarRow::Header(Section::Unread),
            room("general"),
            room("dev-team"),
            room("alice"),
            SidebarRow::Header(Section::Folders),
            folder("work", true, 1),
            child("design-review"),
            child("platform"),
            folder("people", true, 1),
            SidebarRow::Header(Section::Recent),
            room("random"),
            room("secrets"),
            room("quiet"),
        ];
        assert_eq!(rows(&app), expected);
        assert_eq!(Section::Unread.label(), "Unread");
        assert_eq!(Section::Folders.label(), "Folders");
        assert_eq!(Section::Recent.label(), "Recent");
    }

    #[test]
    fn each_room_once() {
        let app = app_80x24();
        let all = rows(&app);
        let room_rows = all
            .iter()
            .filter(|r| matches!(r, SidebarRow::Room { .. }))
            .count();
        assert_eq!(room_rows, app.rooms.len());
        for r in &app.rooms {
            let count = all
                .iter()
                .filter(|row| matches!(row, SidebarRow::Room { id, .. } if *id == r.id))
                .count();
            assert_eq!(count, 1, "{}", r.name);
        }
    }

    #[test]
    fn cursor_never_on_header() {
        let mut app = app_80x24();
        assert!(selected(&app).is_selectable());
        for start in [0, 4, 9, 999] {
            app.sidebar.cursor = start;
            on_rows_changed(&mut app);
            assert!(selected(&app).is_selectable(), "start {start}");
        }
        app.sidebar.cursor = 0;
        on_rows_changed(&mut app);
        let seq = [
            "j", "j", "j", "j", "j", "j", "j", "j", "j", "j", "k", "k", "k", "k", "gg", "G", "j",
            "k",
        ];
        for keys in seq {
            press(&mut app, keys);
            assert!(selected(&app).is_selectable(), "after {keys}");
        }
    }

    #[test]
    fn edges() {
        let mut app = app_80x24();
        let first = selected(&app);
        assert_eq!(first, room("general"));
        press(&mut app, "k");
        assert_eq!(selected(&app), first);
        press(&mut app, "G");
        let last = selected(&app);
        assert_eq!(last, room("quiet"));
        press(&mut app, "j");
        assert_eq!(selected(&app), last);

        let mut counted = app_80x24();
        press(&mut counted, "99j");
        assert_eq!(selected(&counted), last);

        let mut three = app_80x24();
        press(&mut three, "3j");
        let mut repeated = app_80x24();
        press(&mut repeated, "jjj");
        assert_eq!(selected(&three), selected(&repeated));

        let mut fifth = app_80x24();
        press(&mut fifth, "5gg");
        assert_eq!(selected(&fifth), selectable(&fifth)[4]);
        assert_eq!(selected(&fifth), child("design-review"));
        press(&mut fifth, "gg");
        assert_eq!(selected(&fifth), first);
        press(&mut fifth, "3G");
        assert_eq!(selected(&fifth), selectable(&fifth)[2]);
    }

    #[test]
    fn half_page() {
        // 80x10: the sidebar is 8 rows tall, 6 inside the border, so half a page is 3 rows.
        let mut app = app_80x24();
        app.size = Size::new(80, 10);
        press(&mut app, "<C-d>");
        assert_eq!(selected(&app), selectable(&app)[3]);
        press(&mut app, "<C-u>");
        assert_eq!(selected(&app), selectable(&app)[0]);
        press(&mut app, "<C-d><C-d><C-d><C-d>");
        assert_eq!(selected(&app), room("quiet"));
        press(&mut app, "<C-u>");
        assert_eq!(selected(&app), selectable(&app)[6]);

        // 80x24: half a page (10 rows) is more than the ten selectable rows, so it clamps.
        let mut tall = app_80x24();
        press(&mut tall, "<C-d>");
        assert_eq!(selected(&tall), room("quiet"));
        press(&mut tall, "<C-u>");
        assert_eq!(selected(&tall), room("general"));
    }

    #[test]
    fn folds() {
        let mut app = app_80x24();
        let open_work = folder("work", true, 1);
        let closed_work = folder("work", false, 1);
        cursor_to(&mut app, &open_work);
        press(&mut app, "h");
        assert_eq!(selected(&app), closed_work);
        assert!(!has_room(&app, "design-review"));
        assert!(!has_room(&app, "platform"));
        // D-19: h on an already collapsed folder row changes nothing.
        let collapsed_rows = rows(&app);
        press(&mut app, "h");
        assert_eq!(rows(&app), collapsed_rows);
        assert_eq!(selected(&app), closed_work);

        press(&mut app, "l");
        assert_eq!(selected(&app), open_work);
        assert!(has_room(&app, "design-review"));
        assert!(has_room(&app, "platform"));

        press(&mut app, "za");
        assert_eq!(selected(&app), closed_work);
        assert!(!has_room(&app, "platform"));
        press(&mut app, "za");
        assert_eq!(selected(&app), open_work);
        press(&mut app, "<CR>");
        assert_eq!(selected(&app), closed_work);
        assert!(app.current_room.is_none());
        press(&mut app, "<CR>");
        assert_eq!(selected(&app), open_work);

        // D-19: h and l on room rows and l on an open folder are intentional no-ops.
        let before = rows(&app);
        for (target, keys) in [
            (room("general"), "h"),
            (room("general"), "l"),
            (child("design-review"), "h"),
            (child("design-review"), "l"),
            (folder("people", true, 1), "l"),
        ] {
            cursor_to(&mut app, &target);
            press(&mut app, keys);
            assert_eq!(rows(&app), before, "{keys} on {target:?}");
            assert_eq!(selected(&app), target, "{keys} on {target:?}");
        }
    }

    #[test]
    fn child_za_collapses_parent() {
        let mut app = app_80x24();
        cursor_to(&mut app, &child("design-review"));
        press(&mut app, "h");
        assert_eq!(selected(&app), child("design-review"));
        press(&mut app, "za");
        assert_eq!(selected(&app), folder("work", false, 1));
        assert!(!has_room(&app, "design-review"));
        assert!(!has_room(&app, "platform"));
        // za on a top-level room row does nothing.
        cursor_to(&mut app, &room("general"));
        let before = rows(&app);
        press(&mut app, "za");
        assert_eq!(rows(&app), before);
        assert_eq!(selected(&app), room("general"));
    }

    #[test]
    fn enter_opens_room() {
        let mut app = app_80x24();
        cursor_to(&mut app, &room("alice"));
        assert_eq!(app.current_room, None);
        press(&mut app, "<CR>");
        assert_eq!(app.current_room, Some(rid("alice")));
        assert_eq!(app.focus, Focus::Messages);
        let status = render_to_string(&app, 80, 24)
            .lines()
            .nth(22)
            .unwrap_or_default()
            .to_string();
        assert!(status.contains("alice"), "{status:?}");
    }

    #[test]
    fn empty_sidebar() {
        let mut app = App::new(Size::new(80, 24));
        assert!(rows(&app).is_empty());
        assert!(selected_room(&app).is_none());
        assert!(selected_row(&app).is_none());
        for keys in [
            "j", "k", "gg", "G", "5j", "<C-d>", "<C-u>", "h", "l", "za", "<CR>",
        ] {
            press(&mut app, keys);
            assert!(rows(&app).is_empty(), "{keys}");
            assert!(selected_room(&app).is_none(), "{keys}");
            assert_eq!(app.sidebar.cursor, 0, "{keys}");
        }
        assert_eq!(app.current_room, None);
        assert_eq!(app.focus, Focus::Sidebar);
    }
}

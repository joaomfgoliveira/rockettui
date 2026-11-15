//! Sidebar pane: section headers, foldable folders and rooms with kind glyphs and unread and
//! encrypted markers (D-11, D-12). Every server-controlled name goes through `sanitize_line`.

use crate::app::sidebar::{self, SidebarRow};
use crate::app::{App, Focus};
use crate::data::RoomKind;
use crate::ui::pane_block;
use crate::ui::sanitize::sanitize_line;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{List, ListItem, ListState};

/// The glyph drawn before a room name for its kind. The app draws it; it never comes from the
/// name, so a room cannot pose as another kind (T-01-14).
pub fn kind_glyph(kind: RoomKind) -> &'static str {
    match kind {
        RoomKind::Channel => "#",
        RoomKind::PrivateGroup => "&",
        RoomKind::Discussion => "~",
        RoomKind::Team => "+",
        RoomKind::DirectMessage => "@",
    }
}

/// The list item for one sidebar row. A row is one line of one style, so a room name is a
/// contiguous run of identically styled cells.
fn row_item(app: &App, row: &SidebarRow) -> ListItem<'static> {
    match row {
        SidebarRow::Header(section) => ListItem::new(Line::styled(
            section.label(),
            Style::new().add_modifier(Modifier::BOLD | Modifier::DIM),
        )),
        SidebarRow::Folder { id, open, unread } => {
            let name = app
                .folders
                .iter()
                .find(|folder| folder.id == *id)
                .map(|folder| sanitize_line(&folder.name))
                .unwrap_or_default();
            let marker = if *open { "\u{25be} " } else { "\u{25b8} " };
            let count = if *unread > 0 {
                format!(" ({unread})")
            } else {
                String::new()
            };
            ListItem::new(format!("{marker}{name}{count}"))
        }
        SidebarRow::Room { id, indent } => {
            let Some(room) = app.rooms.iter().find(|room| room.id == *id) else {
                return ListItem::new("");
            };
            let indent = "  ".repeat(usize::from(*indent));
            let unread = if room.unread { "* " } else { "  " };
            let glyph = kind_glyph(room.kind);
            let name = sanitize_line(&room.name);
            let tag = if room.encrypted { " [e2e]" } else { "" };
            let style = if room.unread {
                Style::new().add_modifier(Modifier::BOLD)
            } else {
                Style::new()
            };
            ListItem::new(Line::styled(
                format!("{indent}{unread}{glyph}{name}{tag}"),
                style,
            ))
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Sidebar;
    let rows = sidebar::rows(app);
    let items: Vec<ListItem> = rows.iter().map(|row| row_item(app, row)).collect();
    let highlight = if focused {
        Style::new().add_modifier(Modifier::REVERSED)
    } else {
        Style::new().add_modifier(Modifier::BOLD)
    };
    let list = List::new(items)
        .block(pane_block(" Rooms ", focused))
        .highlight_symbol("> ")
        .highlight_style(highlight);
    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(app.sidebar.cursor));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::sidebar;
    use crate::data::FolderId;
    use crate::test_util::{app_80x24, press};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;

    fn draw(app: &App) -> Terminal<TestBackend> {
        let mut terminal = Terminal::new(TestBackend::new(28, 20)).expect("backend");
        terminal
            .draw(|frame| render(frame, frame.area(), app))
            .expect("draw");
        terminal
    }

    /// One `Vec<String>` entry per buffer row, cell symbols concatenated.
    fn screen_rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn sidebar_sections_28x20() {
        let app = app_80x24();
        let terminal = draw(&app);
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn sidebar_work_collapsed_28x20() {
        let mut app = app_80x24();
        app.sidebar.collapsed.insert(FolderId("work".to_string()));
        sidebar::on_rows_changed(&mut app);
        let terminal = draw(&app);
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn unread_rooms_are_bold() {
        let mut app = app_80x24();
        // Unfocus the sidebar so the highlight style does not decide the result.
        press(&mut app, "<C-w>l");
        let terminal = draw(&app);
        let rows = screen_rows(&terminal);
        let row = rows
            .iter()
            .position(|r| r.contains("general"))
            .expect("general row");
        let text = &rows[row];
        let marker = text.chars().position(|c| c == '*').expect("* marker");
        let name = text.chars().position(|c| c == 'g').expect("name start");
        assert!(text.contains("* #general"), "{text:?}");
        assert!(marker < name);
        let buffer = terminal.backend().buffer();
        for x in name..name + "general".len() {
            let cell = &buffer[(
                u16::try_from(x).expect("x fits"),
                u16::try_from(row).expect("y"),
            )];
            assert!(cell.modifier.contains(Modifier::BOLD), "cell {x}");
        }
        // A read room is not bold and carries no marker.
        let random = rows.iter().position(|r| r.contains("random")).expect("row");
        assert!(!rows[random].contains('*'), "{:?}", rows[random]);
        let x = rows[random].chars().position(|c| c == 'r').expect("x");
        let cell = &buffer[(
            u16::try_from(x).expect("x"),
            u16::try_from(random).expect("y"),
        )];
        assert!(!cell.modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn glyphs_and_markers() {
        let app = app_80x24();
        let text = screen_rows(&draw(&app)).join("\n");
        for needle in [
            "#general",
            "&dev-team",
            "~design-review",
            "+platform",
            "@alice",
            "[e2e]",
            "\u{25be} Work (1)",
            "\u{25be} People (1)",
            "Unread",
            "Folders",
            "Recent",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in\n{text}");
        }
        assert_eq!(kind_glyph(RoomKind::Channel), "#");
        assert_eq!(kind_glyph(RoomKind::PrivateGroup), "&");
        assert_eq!(kind_glyph(RoomKind::Discussion), "~");
        assert_eq!(kind_glyph(RoomKind::Team), "+");
        assert_eq!(kind_glyph(RoomKind::DirectMessage), "@");
    }

    #[test]
    fn collapsed_folder_shows_closed_marker() {
        let mut app = app_80x24();
        app.sidebar.collapsed.insert(FolderId("work".to_string()));
        sidebar::on_rows_changed(&mut app);
        let text = screen_rows(&draw(&app)).join("\n");
        assert!(text.contains("\u{25b8} Work (1)"), "{text}");
        assert!(!text.contains("design-review"), "{text}");
    }

    #[test]
    fn hostile_room_name_sanitized() {
        let mut app = app_80x24();
        app.rooms[0].name = "\u{1b}]0;x\u{7}evil\u{202e}name".to_string();
        app.folders[0].name = "\u{1b}[2Jfold\u{202e}er".to_string();
        let terminal = draw(&app);
        let buffer = terminal.backend().buffer();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                for c in buffer[(x, y)].symbol().chars() {
                    assert!(!c.is_control(), "control char {:?} at {x},{y}", c as u32);
                    assert!(
                        !matches!(c as u32,
                            0x61c | 0x200b | 0x200e | 0x200f | 0x202a..=0x202e
                            | 0x2060 | 0x2066..=0x2069 | 0xfeff | 0xe0000..=0xe007f),
                        "spoofing char U+{:04X} at {x},{y}",
                        c as u32
                    );
                }
            }
        }
        let text = screen_rows(&terminal).join("\n");
        assert!(text.contains("evilname"), "{text}");
        assert!(text.contains("folder"), "{text}");
    }
}

//! Sidebar pane: section headers, foldable folders and rooms with kind glyphs and unread and
//! encrypted markers (D-11, D-12). Every server-controlled name goes through `sanitize_line`.

use crate::app::sidebar::{self, SidebarRow};
use crate::app::{App, Focus};
use crate::data::{Room, RoomKind};
use crate::ui::pane_block;
use crate::ui::sanitize::sanitize_line;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// The default glyph drawn before a room name for its kind; `Markers::default` takes its glyphs
/// from here. The app draws it; it never comes from the name, so a room cannot pose as another
/// kind (T-01-14).
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
///
/// `width` is the columns the row may use; it only matters for an unread room, whose name is cut
/// (with `…`) before its count and mention badge are.
fn row_item(app: &App, row: &SidebarRow, width: usize) -> ListItem<'static> {
    match row {
        SidebarRow::Header(section) => {
            ListItem::new(Line::styled(section.label(), app.theme.sidebar_header))
        }
        SidebarRow::Folder { id, open, unread } => {
            let name = app
                .folders
                .iter()
                .find(|folder| folder.id == *id)
                .map(|folder| sanitize_line(&folder.name))
                .unwrap_or_default();
            let marker = if *open {
                &app.theme.markers.fold_open
            } else {
                &app.theme.markers.fold_closed
            };
            let count = if *unread > 0 {
                format!(" ({unread})")
            } else {
                String::new()
            };
            ListItem::new(Line::styled(
                format!("{marker}{name}{count}"),
                app.theme.sidebar_folder,
            ))
        }
        SidebarRow::Room { id, indent } => {
            let Some(room) = app.rooms.iter().find(|room| room.id == *id) else {
                return ListItem::new("");
            };
            let indent = "  ".repeat(usize::from(*indent));
            ListItem::new(room_line(app, room, &indent, width))
        }
    }
}

/// The line of one room: the unread marker, the kind glyph, the sanitized name, the encrypted tag,
/// the unread count and the mention badge. The sidebar and the room switcher both draw it, so the
/// two never differ. `width` is the columns the line may use; it only matters for an unread room,
/// whose name is cut (with `…`) before its count and badge are.
pub fn room_line(app: &App, room: &Room, indent: &str, width: usize) -> Line<'static> {
    // The placeholder is as wide as the unread marker, so read rooms stay aligned.
    let unread = if room.is_unread() {
        app.theme.markers.unread.clone()
    } else {
        " ".repeat(app.theme.markers.unread.width())
    };
    let glyph = app.theme.markers.glyph(room.kind);
    let name = sanitize_line(&room.name);
    let tag = if room.encrypted {
        app.theme.markers.encrypted.as_str()
    } else {
        ""
    };
    let style = if room.is_unread() {
        app.theme.sidebar_unread
    } else {
        app.theme.sidebar_room
    };
    let count = if room.unread.count > 0 {
        format!(" ({})", capped(room.unread.count))
    } else {
        String::new()
    };
    let mention = if room.unread.mentions > 0 {
        format!(" @{}", capped(room.unread.mentions))
    } else {
        String::new()
    };
    // The thread badge: `T` and the number of unread threads, after the count and the mention badge.
    let threads = match room.threads.count() {
        0 => String::new(),
        n => format!(" T({})", capped(u32::try_from(n).unwrap_or(u32::MAX))),
    };
    let prefix = format!("{indent}{unread}{glyph}");
    let fixed = prefix.width() + tag.width() + count.width() + mention.width() + threads.width();
    let name = if (count.is_empty() && mention.is_empty() && threads.is_empty())
        || fixed + name.width() <= width
    {
        name.into_owned()
    } else {
        truncate_to(&name, width.saturating_sub(fixed))
    };
    let mut spans = vec![Span::styled(format!("{prefix}{name}{tag}{count}"), style)];
    if !mention.is_empty() {
        spans.push(Span::styled(mention, app.theme.sidebar_mention));
    }
    if !threads.is_empty() {
        let style = if room.threads.has_mention() {
            app.theme.sidebar_thread_badge_mention
        } else {
            // The row's own style (the name's colour), plus whatever the theme adds.
            style.patch(app.theme.sidebar_thread_badge)
        };
        spans.push(Span::styled(threads, style));
    }
    Line::from(spans)
}

/// A count as shown in the sidebar: `99+` over 99.
fn capped(n: u32) -> String {
    if n > 99 {
        "99+".to_string()
    } else {
        n.to_string()
    }
}

/// `text` cut to at most `max` columns, ending in `…` when something was dropped.
fn truncate_to(text: &str, max: usize) -> String {
    if text.width() <= max {
        return text.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > max {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Sidebar;
    let rows = sidebar::rows(app);
    // The pane's borders and the cursor marker take their columns.
    let width = usize::from(area.width)
        .saturating_sub(2)
        .saturating_sub(app.theme.markers.cursor.width());
    let items: Vec<ListItem> = rows.iter().map(|row| row_item(app, row, width)).collect();
    let highlight = if focused {
        app.theme.sidebar_cursor
    } else {
        app.theme.sidebar_cursor_unfocused
    };
    let list = List::new(items)
        .block(pane_block(" Rooms ", focused, &app.theme))
        .highlight_symbol(app.theme.markers.cursor.as_str())
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
    use crate::test_util::{app_80x24, app_with_config, config_from_theme, press};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::{Color, Modifier};

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

    /// Give the fake room `name` this unread state and draw the sidebar, unfocused.
    fn draw_with(app: &mut App, name: &str, unread: crate::data::Unread) -> Vec<String> {
        app.rooms
            .iter_mut()
            .find(|r| r.name == name)
            .expect("room")
            .unread = unread;
        press(app, "<C-w>l");
        screen_rows(&draw(app))
    }

    fn row_of<'a>(rows: &'a [String], name: &str) -> &'a str {
        rows.iter().find(|r| r.contains(name)).expect("row")
    }

    #[test]
    fn count_and_mention_after_the_name() {
        use crate::data::Unread;
        let mut app = app_80x24();
        let rows = draw_with(
            &mut app,
            "general",
            Unread {
                flagged: true,
                count: 3,
                mentions: 2,
            },
        );
        assert!(row_of(&rows, "general").contains("#general (3) @2"));
        let rows = draw_with(
            &mut app,
            "general",
            Unread {
                flagged: false,
                count: 12,
                mentions: 0,
            },
        );
        let row = row_of(&rows, "general");
        assert!(
            row.contains("#general (12)") && !row.contains('@'),
            "{row:?}"
        );
        // Flagged with a zero count: unread, no number, no badge.
        let rows = draw_with(&mut app, "general", Unread::flag());
        let row = row_of(&rows, "general");
        assert!(row.contains("* #general") && !row.contains('(') && !row.contains('@'));
        let rows = draw_with(
            &mut app,
            "general",
            Unread {
                flagged: true,
                count: 250,
                mentions: 120,
            },
        );
        assert!(row_of(&rows, "general").contains("(99+) @99+"));
    }

    #[test]
    fn a_long_name_is_cut_before_the_count_and_badge() {
        use crate::data::Unread;
        let mut app = app_80x24();
        app.rooms
            .iter_mut()
            .find(|r| r.name == "general")
            .unwrap()
            .name = "a-very-long-room-name-indeed".to_string();
        let rows = draw_with(
            &mut app,
            "a-very-long-room-name-indeed",
            Unread {
                flagged: true,
                count: 7,
                mentions: 1,
            },
        );
        let row = row_of(&rows, "a-very-long");
        assert!(row.contains("… (7) @1"), "{row:?}");
    }

    /// Give the fake room `name` these unread threads (the first `mentioned` of them with a user
    /// mention) and draw the sidebar, unfocused. The room's own unread state is left as it is.
    fn draw_threads(app: &mut App, name: &str, threads: usize, mentioned: usize) -> Vec<String> {
        let room = app.rooms.iter_mut().find(|r| r.name == name).expect("room");
        room.threads.unread = (0..threads).map(|i| format!("t{i}")).collect();
        room.threads.user = (0..mentioned).map(|i| format!("t{i}")).collect();
        press(app, "<C-w>l");
        screen_rows(&draw(app))
    }

    #[test]
    fn the_thread_badge_follows_the_count_and_the_mention_badge() {
        use crate::data::Unread;
        let mut app = app_80x24();
        // Only threads unread: `T(2)`, nothing else.
        let rows = draw_threads(&mut app, "random", 2, 0);
        let row = row_of(&rows, "random");
        assert!(row.contains("#random T(2)"), "{row:?}");
        let rest = row.replace("T(2)", "");
        assert!(!rest.contains('(') && !rest.contains('@'), "{row:?}");
        // Count, mention badge and thread badge, in that order.
        app.rooms
            .iter_mut()
            .find(|r| r.name == "general")
            .unwrap()
            .unread = Unread {
            flagged: true,
            count: 3,
            mentions: 1,
        };
        let rows = draw_threads(&mut app, "general", 2, 0);
        assert!(row_of(&rows, "general").contains("#general (3) @1 T(2)"));
        // A count over 99 is `99+`; no threads, no badge.
        let rows = draw_threads(&mut app, "random", 250, 0);
        assert!(row_of(&rows, "random").contains("T(99+)"));
        let rows = draw_threads(&mut app, "random", 0, 0);
        assert!(!row_of(&rows, "random").contains(" T"), "{rows:?}");
    }

    #[test]
    fn the_old_badges_are_unchanged_without_threads() {
        use crate::data::Unread;
        let mut app = app_80x24();
        let rows = draw_with(
            &mut app,
            "general",
            Unread {
                flagged: true,
                count: 3,
                mentions: 1,
            },
        );
        let row = row_of(&rows, "general");
        assert!(row.contains("#general (3) @1"), "{row:?}");
        assert!(!row.contains(" T"), "{row:?}");
    }

    #[test]
    fn a_long_name_is_cut_before_the_thread_badge_too() {
        let mut app = app_80x24();
        app.rooms
            .iter_mut()
            .find(|r| r.name == "random")
            .unwrap()
            .name = "a-very-long-room-name-indeed".to_string();
        let rows = draw_threads(&mut app, "a-very-long-room-name-indeed", 3, 0);
        let row = row_of(&rows, "a-very-long");
        assert!(row.contains("… T(3)"), "{row:?}");
    }

    #[test]
    fn the_thread_badge_stands_out_with_a_mention_and_is_themeable() {
        for (theme, plain_fg, mention_fg) in [
            ("", Color::Reset, Color::Red),
            (
                "[sidebar.thread_badge]\nfg = \"magenta\"\n",
                Color::Magenta,
                Color::Red,
            ),
        ] {
            for (mentioned, want) in [(0, plain_fg), (1, mention_fg)] {
                let mut app = app_with_config(config_from_theme(theme), 80, 24);
                let rows = draw_threads(&mut app, "random", 2, mentioned);
                let y = rows.iter().position(|r| r.contains("random")).unwrap();
                let byte = rows[y].find(" T(2)").expect("badge") + 1;
                let x = u16::try_from(rows[y][..byte].chars().count()).unwrap();
                let terminal = draw(&app);
                let cell = &terminal.backend().buffer()[(x, u16::try_from(y).unwrap())];
                assert_eq!(cell.fg, want, "mentioned {mentioned}, theme {theme:?}");
            }
        }
    }

    #[test]
    fn a_room_with_only_unread_threads_is_bold_with_the_unread_marker() {
        let mut app = app_80x24();
        let rows = draw_threads(&mut app, "random", 1, 0);
        let row = row_of(&rows, "random");
        assert!(row.contains("* #random T(1)"), "{row:?}");
        let y = rows.iter().position(|r| r.contains("random")).unwrap();
        let x = u16::try_from(rows[y].chars().position(|c| c == 'r').unwrap()).unwrap();
        let terminal = draw(&app);
        let cell = &terminal.backend().buffer()[(x, u16::try_from(y).unwrap())];
        assert!(cell.modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn sidebar_thread_badges_28x20() {
        let mut app = app_80x24();
        for (name, threads, mentioned) in
            [("random", 2, 0), ("general", 120, 1), ("platform", 1, 0)]
        {
            let room = app.rooms.iter_mut().find(|r| r.name == name).expect(name);
            room.threads.unread = (0..threads).map(|i| format!("t{i}")).collect();
            room.threads.user = (0..mentioned).map(|i| format!("t{i}")).collect();
        }
        sidebar::on_rows_changed(&mut app);
        press(&mut app, "<C-w>l");
        let terminal = draw(&app);
        insta::assert_snapshot!(terminal.backend());
    }

    #[test]
    fn the_mention_badge_is_red_bold_and_themeable() {
        use crate::data::Unread;
        let unread = Unread {
            flagged: true,
            count: 3,
            mentions: 1,
        };
        for (theme, fg) in [
            ("", Color::Red),
            ("[sidebar.mention]\nfg = \"yellow\"\n", Color::Yellow),
        ] {
            let mut app = app_with_config(config_from_theme(theme), 80, 24);
            let rows = draw_with(&mut app, "general", unread);
            let y = rows.iter().position(|r| r.contains("general")).unwrap();
            let byte = rows[y].find(" @1").expect("badge") + 1;
            let x = u16::try_from(rows[y][..byte].chars().count()).unwrap();
            let terminal = draw(&app);
            let cell = &terminal.backend().buffer()[(x, u16::try_from(y).unwrap())];
            assert_eq!(cell.fg, fg);
            if fg == Color::Red {
                assert!(cell.modifier.contains(Modifier::BOLD));
            }
        }
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
    fn placement_characters_in_a_room_name_are_removed() {
        let mut app = app_80x24();
        app.rooms[0].name = "ro\u{10EEEE}\u{305}\u{30d}\u{10FFF0}om".to_string();
        let terminal = draw(&app);
        let text = screen_rows(&terminal).join("\n");
        assert!(text.contains("room"), "{text}");
        assert!(!text.chars().any(|c| (c as u32) >= 0x10_8000), "{text:?}");
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

    #[test]
    fn themed_cursor_and_unread() {
        let config = config_from_theme("[sidebar.cursor]\nbg = 4\n[sidebar.unread]\nfg = 1\n");
        let mut app = app_with_config(config, 28, 20);
        let terminal = draw(&app);
        let buffer = terminal.backend().buffer();
        // The cursor row is the one after the top border; every cell inside the borders is themed.
        let y = u16::try_from(app.sidebar.cursor).expect("cursor") + 1;
        for x in 1..27 {
            assert_eq!(buffer[(x, y)].bg, Color::Indexed(4), "cursor cell {x}");
        }
        // Another row keeps the terminal background.
        assert_eq!(buffer[(5, y + 1)].bg, Color::Reset);

        // Unfocused, the unread room's name takes the themed foreground.
        press(&mut app, "<C-w>l");
        let terminal = draw(&app);
        let rows = screen_rows(&terminal);
        let row = rows
            .iter()
            .position(|r| r.contains("general"))
            .expect("general row");
        let name = rows[row].chars().position(|c| c == 'g').expect("name");
        let buffer = terminal.backend().buffer();
        for x in name..name + "general".len() {
            let cell = &buffer[(u16::try_from(x).expect("x"), u16::try_from(row).expect("y"))];
            assert_eq!(cell.fg, Color::Indexed(1), "cell {x}");
        }
        // A read room is not recolored.
        let random = rows.iter().position(|r| r.contains("random")).expect("row");
        let x = rows[random].chars().position(|c| c == 'r').expect("x");
        let cell = &buffer[(
            u16::try_from(x).expect("x"),
            u16::try_from(random).expect("y"),
        )];
        assert_eq!(cell.fg, Color::Reset);
    }

    #[test]
    fn themed_markers() {
        let config = config_from_theme("[markers]\nunread = \"!!! \"\n");
        let app = app_with_config(config, 28, 20);
        let rows = screen_rows(&draw(&app));
        let general = rows.iter().find(|r| r.contains("general")).expect("row");
        assert!(general.contains("!!! #general"), "{general:?}");
        let random = rows.iter().find(|r| r.contains("random")).expect("row");
        assert!(!random.contains('!'), "{random:?}");
        // The read room's glyph sits under the unread room's glyph: the placeholder is as wide as
        // the unread marker, and neither row has an indent.
        let glyph_col = |row: &str, glyph: char| row.chars().position(|c| c == glyph);
        assert_eq!(glyph_col(general, '#'), glyph_col(random, '#'));

        let config = config_from_theme("[markers]\nchannel = \"c\"\nunread = \"\"\n");
        let app = app_with_config(config, 28, 20);
        let text = screen_rows(&draw(&app)).join("\n");
        assert!(text.contains("cgeneral"), "{text}");
        assert!(text.contains("crandom"), "{text}");
        assert!(text.contains("&dev-team"), "other kinds keep their glyph");

        let config = config_from_theme(
            "[markers]\nfold_open = \"v \"\nfold_closed = \"> \"\ncursor = \"\\u00bb \"\nencrypted = \" (e2e)\"\n",
        );
        let mut app = app_with_config(config, 28, 20);
        let text = screen_rows(&draw(&app)).join("\n");
        assert!(text.contains("v Work (1)"), "{text}");
        assert!(text.contains("\u{bb} "), "{text}");
        assert!(text.contains("(e2e)") && !text.contains("[e2e]"), "{text}");
        app.sidebar.collapsed.insert(FolderId("work".to_string()));
        sidebar::on_rows_changed(&mut app);
        let text = screen_rows(&draw(&app)).join("\n");
        assert!(text.contains("> Work (1)"), "{text}");

        for kind in [
            RoomKind::Channel,
            RoomKind::PrivateGroup,
            RoomKind::Discussion,
            RoomKind::Team,
            RoomKind::DirectMessage,
        ] {
            assert_eq!(
                crate::config::Theme::default().markers.glyph(kind),
                kind_glyph(kind)
            );
        }
    }
}

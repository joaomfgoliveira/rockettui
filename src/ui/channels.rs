//! The channel picker (`open_channel` on a message with several channels): a small bordered panel
//! centred at the top of the screen, one row per channel the message names.
//!
//! The content comes from `app::channels::view`; this only lays it out. The panel sits where the
//! room switcher does and takes its colours from the `picker_*` elements of theme.toml, so a theme
//! styles every overlay alike. It is as tall as its list (up to the switcher's height) and the
//! window of rows scrolls so the selection stays visible. What does not fit is cut, never
//! overflowed.

use crate::app::App;
use crate::app::channels;
use crate::ui::layout::Areas;
use crate::ui::switcher::{panel, window_start};
use crate::ui::wrap::fit_author;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

/// Draw the picker when it is open.
pub fn render(frame: &mut Frame, areas: &Areas, app: &App) {
    let Some(view) = channels::view(app) else {
        return;
    };
    let theme = &app.theme;
    let (full, _) = panel(areas);
    let wanted = u16::try_from(view.names.len() + 2).unwrap_or(u16::MAX);
    let area = Rect {
        height: wanted.min(full.height),
        ..full
    };
    frame.render_widget(Clear, area);

    let title = " Open channel ";
    let hint = " j/k move  Enter open  Esc close ";
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.picker_border)
        .title(Span::styled(title, theme.picker_title));
    if usize::from(area.width) >= title.width() + hint.width() + 4 {
        block =
            block.title_bottom(Line::from(Span::styled(hint, theme.picker_hint)).right_aligned());
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let width = usize::from(inner.width);
    let visible = usize::from(inner.height);
    let start = window_start(view.selected, visible, view.names.len());
    let lines: Vec<Line<'static>> = view
        .names
        .iter()
        .enumerate()
        .skip(start)
        .take(visible)
        .map(|(i, name)| {
            // One space of margin on each side, the name cut to what is left.
            let text = format!(" #{name}");
            let mut text = fit_author(&text, width.saturating_sub(1)).into_owned();
            let pad = width.saturating_sub(text.width());
            text.push_str(&" ".repeat(pad));
            let style = if i == view.selected {
                theme.picker_item.patch(theme.picker_selected)
            } else {
                theme.picker_item
            };
            Line::from(Span::styled(text, style))
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use crate::app::{Focus, StatusMsg};
    use crate::data::Refs;
    use crate::data::{Message, RoomId, SendState};
    use crate::test_util::{app_80x24, press, render_buffer, render_to_string, sample_message};

    fn picker_app(channels: &[&str]) -> crate::app::App {
        let mut app = app_80x24();
        let general = RoomId("general".to_string());
        app.open_room(general.clone());
        let message = Message {
            room_id: general.clone(),
            send: SendState::Sent,
            ts: i64::MAX / 4,
            refs: Some(Refs {
                mentions: Vec::new(),
                channels: channels.iter().map(|s| s.to_string()).collect(),
            }),
            ..sample_message("m-ref")
        };
        app.messages.insert(general.clone(), vec![message]);
        app.msg_views.remove(&general);
        app.focus = Focus::Messages;
        app
    }

    #[test]
    fn nothing_is_drawn_while_closed() {
        let app = picker_app(&["random", "quiet"]);
        assert!(!render_to_string(&app, 80, 24).contains("Open channel"));
    }

    #[test]
    fn the_picker_lists_only_the_channels_of_the_message() {
        let mut app = picker_app(&["random", "quiet"]);
        press(&mut app, "gc");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Open channel"), "{out}");
        assert!(out.contains(" #random"), "{out}");
        assert!(out.contains(" #quiet"), "{out}");
        // Rooms of the sidebar that the message does not name are not in the panel.
        assert!(!out.contains("#dev-team"), "{out}");
        assert!(!out.contains("#design-review"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn the_selected_row_is_themed_and_moves() {
        let mut app = picker_app(&["random", "quiet"]);
        press(&mut app, "gc");
        let row_of = |app: &crate::app::App, name: &str| -> (u16, ratatui::style::Style) {
            let buffer = render_buffer(app, 80, 24);
            for y in 0..24 {
                let text: String = (0..80).map(|x| buffer[(x, y)].symbol()).collect();
                if let Some(x) = text.find(name) {
                    let cell = &buffer[(u16::try_from(x).unwrap(), y)];
                    return (y, cell.style());
                }
            }
            panic!("{name} not drawn");
        };
        let selected = app.theme.picker_selected;
        let (_, first) = row_of(&app, "#random");
        let (_, second) = row_of(&app, "#quiet");
        assert_eq!(first.add_modifier, selected.add_modifier);
        assert_ne!(
            first, second,
            "only the selected row has the selected style"
        );
        press(&mut app, "j");
        let (_, first) = row_of(&app, "#random");
        let (_, second) = row_of(&app, "#quiet");
        assert_eq!(second.add_modifier, selected.add_modifier);
        assert_ne!(first, second);
    }

    #[test]
    fn a_hostile_channel_name_is_drawn_clean() {
        let mut app = picker_app(&["a\u{1b}[31mb", "c"]);
        press(&mut app, "gc");
        let out = render_to_string(&app, 80, 24);
        assert!(!out.contains('\u{1b}'), "{out:?}");
        assert!(out.contains("#a[31mb"), "{out}");
    }

    #[test]
    fn the_picker_closes_and_the_notice_shows_for_an_unknown_room() {
        let mut app = picker_app(&["nowhere", "elsewhere"]);
        press(&mut app, "gc<Enter>");
        let out = render_to_string(&app, 80, 24);
        assert!(!out.contains("Open channel"), "{out}");
        assert!(matches!(app.status_msg, Some(StatusMsg::Info(_))));
        assert!(out.contains("#nowhere is not one of your rooms"), "{out}");
    }
}

//! The reactions popup: a small bordered panel over the bottom of the messages and input column,
//! above the status row, listing each reaction of the message under the cursor with who gave it.
//!
//! The content comes from `app::reactions::content` (already sanitized); this only lays it out:
//! a glyph column, a `:shortcode:` column and the users, one reaction per row. The user's own
//! entries take `theme.reactions_own`. What does not fit is cut, never overflowed: a row that is
//! too wide ends in an ellipsis and reactions beyond the height end in a `... N more` row.

use crate::app::App;
use crate::app::reactions::{self, Entry};
use crate::ui::layout::Areas;
use crate::ui::wrap::fit_author;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

/// Most rows of text the popup grows to, borders excluded.
const MAX_INNER_HEIGHT: u16 = 10;

/// Draw the popup when it is open and its message still has reactions.
pub fn render(frame: &mut Frame, areas: &Areas, app: &App) {
    let Some(content) = reactions::content(app) else {
        return;
    };
    let theme = &app.theme;

    let column_bottom = areas.input.y + areas.input.height;
    let available = column_bottom.saturating_sub(areas.messages.y);
    let inner_height = u16::try_from(content.entries.len())
        .unwrap_or(u16::MAX)
        .min(MAX_INNER_HEIGHT)
        .min(available.saturating_sub(2));
    let height = inner_height + 2;
    let area = Rect {
        x: areas.messages.x,
        y: column_bottom.saturating_sub(height),
        width: areas.messages.width,
        height,
    };
    frame.render_widget(Clear, area);
    let title = " Reactions ";
    let hint = " any key closes ";
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.reactions_border)
        .title(Span::styled(title, theme.reactions_title));
    // The hint only when the border has room for it next to the title and the corners.
    if usize::from(area.width) >= title.width() + hint.width() + 4 {
        block = block
            .title_bottom(Line::from(Span::styled(hint, theme.reactions_name)).right_aligned());
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // When the entries do not all fit, the last row says how many are left out.
    let slots = usize::from(inner.height);
    let total = content.entries.len();
    let (shown, hidden) = if total > slots && slots > 0 {
        (slots - 1, total - (slots - 1))
    } else {
        (total.min(slots), 0)
    };
    let entries = &content.entries[..shown];

    let width = usize::from(inner.width);
    // A reaction that resolved to nothing is drawn raw: its `:shortcode:` already is the glyph, so
    // the glyph cell stays empty and the glyph column is sized from real glyphs only.
    let emoji = app.emoji_view();
    let glyph_of = |e: &Entry| -> String {
        if let Some(cells) =
            crate::ui::emoji::image_glyph(&e.shortcode, &app.settings.emoji_aliases, &emoji)
        {
            cells
        } else if e.glyph == e.shortcode {
            String::new()
        } else {
            e.glyph.clone()
        }
    };
    let glyph_width = entries
        .iter()
        .map(|e| glyph_of(e).width())
        .max()
        .unwrap_or(0);
    // The glyph column and the space after it, absent when no entry has a glyph.
    let lead = if glyph_width > 0 { glyph_width + 1 } else { 0 };
    // A shortcode column never takes more than half of what the glyph column leaves.
    let code_cap = width.saturating_sub(lead + 2) / 2;
    let code_of = |e: &Entry| fit_author(&e.shortcode, code_cap).into_owned();
    let code_width = entries
        .iter()
        .map(|e| code_of(e).width())
        .max()
        .unwrap_or(0);

    let mut lines: Vec<Line<'static>> = entries
        .iter()
        .map(|entry| {
            let (name_style, users_style) = if entry.own {
                (theme.reactions_own, theme.reactions_own)
            } else {
                (theme.reactions_name, theme.reactions_users)
            };
            let glyph = glyph_of(entry);
            let glyph = format!(
                "{glyph}{}",
                " ".repeat(glyph_width.saturating_sub(glyph.width()))
            );
            let gap = " ".repeat(usize::from(glyph_width > 0));
            let code = code_of(entry);
            let code = format!(
                "{code}{}",
                " ".repeat(code_width.saturating_sub(code.width()))
            );
            let prefix = lead + code_width + 2;
            if width <= prefix {
                // No room for the users: the left part, cut to the width.
                let left = format!("{glyph}{gap}{code}");
                return Line::from(crate::ui::emoji::expand(
                    vec![Span::styled(
                        fit_author(&left, width).into_owned(),
                        name_style,
                    )],
                    theme.messages_custom_emoji,
                    &mut false,
                ));
            }
            let users = entry.users.join(", ");
            let users = fit_author(&users, width - prefix).into_owned();
            let mut spans = crate::ui::emoji::expand(
                vec![Span::raw(glyph)],
                theme.messages_custom_emoji,
                &mut false,
            );
            spans.extend([
                Span::raw(gap),
                Span::styled(code, name_style),
                Span::raw("  "),
                Span::styled(users, users_style),
            ]);
            Line::from(spans)
        })
        .collect();
    if hidden > 0 {
        lines.push(Line::from(Span::styled(
            fit_author(&format!("... {hidden} more"), width).into_owned(),
            theme.reactions_name,
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

#[cfg(test)]
mod tests {
    use crate::app::{App, Focus};
    use crate::data::{Message, Reaction, RoomId, SendState};
    use crate::test_util::{
        app_80x24, app_with_config, config_from_theme, press, render_buffer, render_to_string,
        sample_message,
    };
    use ratatui::style::{Color, Modifier};

    fn reaction(name: &str, users: &[&str]) -> Reaction {
        Reaction {
            name: name.to_string(),
            users: users.iter().map(|u| u.to_string()).collect(),
        }
    }

    /// The messages pane focused on a message carrying `reactions`; the user is `me`.
    fn on_reactions(mut app: App, reactions: Vec<Reaction>) -> App {
        let room: RoomId = app.rooms[0].id.clone();
        let message = Message {
            room_id: room.clone(),
            reactions,
            send: SendState::Sent,
            ts: i64::MAX / 4,
            body: "deploy finished".to_string(),
            ..sample_message("m-react")
        };
        app.messages.insert(room.clone(), vec![message]);
        app.msg_views.remove(&room);
        app.current_room = Some(room);
        app.own_username = Some("me".to_string());
        app.focus = Focus::Messages;
        app
    }

    /// The rows of the popup in a rendered screen: from the title row to the bottom border. The
    /// message pane underneath draws its own reaction row, which must not be mistaken for them.
    fn popup_rows(out: &str) -> Vec<&str> {
        let lines: Vec<&str> = out.lines().collect();
        let top = lines
            .iter()
            .position(|l| l.contains(" Reactions "))
            .expect("popup title row");
        let bottom = lines
            .iter()
            .position(|l| l.contains("any key closes"))
            .expect("popup bottom border");
        lines[top..=bottom].to_vec()
    }

    fn typical() -> Vec<Reaction> {
        vec![
            reaction("+1", &["bob", "me", "ana"]),
            reaction("tada", &["ana"]),
            reaction("partyparrot", &["bob", "ana"]),
            reaction("rt_unknown", &["zed"]),
        ]
    }

    #[test]
    fn reactions_popup_80x24() {
        let mut app = on_reactions(app_80x24(), typical());
        app.settings
            .emoji_aliases
            .insert("partyparrot".to_string(), "🦜".to_string());
        press(&mut app, "gr");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Reactions"), "{out}");
        assert!(out.contains("you, bob, ana"), "{out}");
        assert!(out.contains("any key closes"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn reactions_popup_smallest_size() {
        let mut app = on_reactions(app_80x24(), typical());
        app.size = ratatui::layout::Size::new(40, 8);
        press(&mut app, "gr");
        let out = render_to_string(&app, 40, 8);
        assert!(out.contains("Reactions"), "{out}");
        insta::assert_snapshot!(out);
    }

    /// Custom emoji `rtshipit` loaded, the terminal able to show images and its image on screen.
    fn with_ready_image(app: &mut crate::app::App) {
        use crate::app::emoji::{CustomEmojis, Graphics, ImageState};
        app.graphics = Graphics::Supported;
        app.custom_emojis = CustomEmojis::Loaded(vec![crate::data::CustomEmoji {
            name: "rtshipit".to_string(),
            ext: "png".to_string(),
            ..Default::default()
        }]);
        app.images.set_state(
            "rtshipit",
            ImageState::Ready {
                etag: String::new(),
                image: crate::emoji_image::Rgba {
                    width: 1,
                    height: 1,
                    pixels: vec![0, 0, 0, 255].into(),
                    ..Default::default()
                },
                id: 9,
                uploaded: true,
            },
        );
    }

    /// The placeholder cells of the buffer, as (x, y, foreground, reversed).
    fn image_cells(
        buffer: &ratatui::buffer::Buffer,
    ) -> Vec<(u16, u16, ratatui::style::Color, bool)> {
        let mut cells = Vec::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                if cell.symbol().starts_with(crate::kitty::PLACEHOLDER) {
                    cells.push((
                        x,
                        y,
                        cell.fg,
                        cell.modifier.contains(ratatui::style::Modifier::REVERSED),
                    ));
                }
            }
        }
        cells
    }

    #[test]
    fn a_custom_emoji_shows_its_image_in_the_popup_with_its_name() {
        let mut app = on_reactions(app_80x24(), vec![reaction("rtshipit", &["bob", "me"])]);
        with_ready_image(&mut app);
        press(&mut app, "gr");
        let buffer = render_buffer(&app, 80, 24);
        let cells = image_cells(&buffer);
        assert_eq!(cells.len(), 2, "{cells:?}");
        assert!(
            cells
                .iter()
                .all(|(_, _, fg, rev)| *fg == Color::Rgb(0, 0, 9) && !rev)
        );
        let out = render_to_string(&app, 80, 24);
        assert!(
            out.contains(":rtshipit:") && out.contains("you, bob"),
            "{out}"
        );
    }

    #[test]
    fn a_custom_emoji_shows_as_its_name_in_the_popup() {
        let mut app = on_reactions(app_80x24(), vec![reaction("rtshipit", &["bob", "me"])]);
        app.custom_emojis =
            crate::app::emoji::CustomEmojis::Loaded(vec![crate::data::CustomEmoji {
                name: "rtshipit".to_string(),
                aliases: Vec::new(),
                ..Default::default()
            }]);
        press(&mut app, "gr");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains(":rtshipit:"), "{out}");
        assert!(out.contains("you, bob"), "{out}");
    }

    #[test]
    fn closed_draws_nothing() {
        let app = on_reactions(app_80x24(), typical());
        let out = render_to_string(&app, 80, 24);
        assert!(!out.contains("Reactions"), "{out}");
        assert!(!out.contains("any key closes"), "{out}");
    }

    #[test]
    fn more_reactions_than_rows_end_in_a_marker() {
        let many: Vec<Reaction> = (0..15)
            .map(|n| reaction(&format!("custom_{n}"), &["bob"]))
            .collect();
        let mut app = on_reactions(app_80x24(), many);
        press(&mut app, "gr");
        let out = render_to_string(&app, 80, 24);
        // Ten rows of text: nine reactions and the marker for the other six.
        let rows = popup_rows(&out);
        let body = &rows[1..rows.len() - 1];
        assert_eq!(body.len(), 10, "{out}");
        assert!(body[8].contains(":custom_8:"), "{out}");
        assert!(!rows.iter().any(|r| r.contains(":custom_9:")), "{out}");
        assert!(body[9].contains("... 6 more"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn long_users_and_shortcodes_are_cut_not_overflowed() {
        let long_users: Vec<String> = (0..40).map(|n| format!("user_number_{n}")).collect();
        let users: Vec<&str> = long_users.iter().map(String::as_str).collect();
        let long_name = "n".repeat(120);
        let mut app = on_reactions(
            app_80x24(),
            vec![reaction("+1", &users), reaction(&long_name, &["bob"])],
        );
        press(&mut app, "gr");
        let out = render_to_string(&app, 80, 24);
        let rows = popup_rows(&out);
        let body = &rows[1..rows.len() - 1];
        assert_eq!(body.len(), 2, "{out}");
        for row in body {
            assert!(row.ends_with('\u{2502}'), "the border is intact: {row:?}");
            assert!(
                row.contains('\u{2026}'),
                "an ellipsis marks the cut: {row:?}"
            );
        }
        // The cut users are not shown whole.
        assert!(!out.contains("user_number_39"), "{out}");
        insta::assert_snapshot!(out);
    }

    /// The (x, y) of the first cell of `needle` on the first row of `out` that also holds `row_tag`.
    fn cell_of(out: &str, row_tag: &str, needle: &str) -> (u16, u16) {
        let (y, line) = out
            .lines()
            .enumerate()
            .find(|(_, l)| l.contains(row_tag))
            .unwrap_or_else(|| panic!("no row with {row_tag}: {out}"));
        let x = line[..line.find(needle).expect("needle in row")]
            .chars()
            .count();
        (x as u16, y as u16)
    }

    #[test]
    fn own_entries_and_themed_elements_are_styled() {
        let theme = "[reactions.users]\nfg = 3\n[reactions.own]\nfg = 5\n";
        let mut app = on_reactions(app_with_config(config_from_theme(theme), 80, 24), typical());
        press(&mut app, "gr");
        let buffer = render_buffer(&app, 80, 24);
        let out = render_to_string(&app, 80, 24);
        // The +1 row holds the user's own reaction: its users take the own style.
        let (x, y) = cell_of(&out, ":+1:", "you, bob, ana");
        assert_eq!(buffer[(x, y)].fg, Color::Indexed(5));
        // The tada row does not: its users take the themed users style.
        let (x, y) = cell_of(&out, ":tada:", "ana");
        assert_eq!(buffer[(x, y)].fg, Color::Indexed(3));
    }

    #[test]
    fn default_styles_make_the_own_entry_stand_out_without_colour() {
        let mut app = on_reactions(app_80x24(), typical());
        press(&mut app, "gr");
        let buffer = render_buffer(&app, 80, 24);
        let out = render_to_string(&app, 80, 24);
        let (x, y) = cell_of(&out, ":+1:", "you");
        let own = buffer[(x, y)].modifier;
        let (x, y) = cell_of(&out, ":tada:", "ana");
        let other = buffer[(x, y)].modifier;
        assert!(own.contains(Modifier::BOLD), "{own:?}");
        assert!(!other.contains(Modifier::BOLD), "{other:?}");
    }
}

//! The reaction picker: a small bordered panel over the bottom of the messages and input column,
//! above the status row, with a search line and the results under it.
//!
//! The content comes from `app::react::view` (candidates, selection, query); this only lays it
//! out. Each result row is `* glyph :name:  note`: a `*` for a reaction the user already gave, the
//! glyph (empty when the name already is the raw `:name:`), the sanitized name, and a short note
//! (`alias` or `custom`, the count, `you`). The window of rows scrolls so the selection stays
//! visible, centred on it. What does not fit is cut, never overflowed: a row that is too wide ends
//! in an ellipsis. The hardware cursor sits after the typed text.

use crate::app::App;
use crate::app::react::{self, Candidate, Source};
use crate::ui::layout::Areas;
use crate::ui::sanitize::sanitize_line;
use crate::ui::wrap::fit_author;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

/// Rows of text the panel has, borders excluded: the search line and the results. It is the same
/// whatever the list holds, as far as the screen allows.
const MAX_INNER_HEIGHT: u16 = 12;

/// What the search line starts with: the key that starts a search, as in vim.
const PROMPT: &str = "/";

/// What the search line says while browsing with no search to show.
const PRESS_TO_SEARCH: &str = "press / to search";

/// The text of the empty list: nothing to browse, nothing typed yet, nothing found.
const NO_REACTIONS_YET: &str = "no reactions yet: press / to search for an emoji";
const TYPE_TO_SEARCH: &str = "type to search for an emoji";
const NO_MATCHES: &str = "no matches";

/// The panel and its inner area. The height is the same whatever the list holds, so the search
/// line stays where it is while results come and go as the user types; it is the most that fits.
fn panel(areas: &Areas) -> (Rect, Rect) {
    let column_bottom = areas.input.y + areas.input.height;
    let available = column_bottom.saturating_sub(areas.messages.y);
    let inner_height = MAX_INNER_HEIGHT.min(available.saturating_sub(2));
    let height = inner_height + 2;
    let area = Rect {
        x: areas.messages.x,
        y: column_bottom.saturating_sub(height),
        width: areas.messages.width,
        height,
    };
    let inner = Block::default().borders(Borders::ALL).inner(area);
    (area, inner)
}

/// First result shown: the selection is kept in the middle of the window when the list is longer
/// than it, so moving either way scrolls smoothly. Always in range.
fn window_start(selected: usize, visible: usize, total: usize) -> usize {
    if visible == 0 || total <= visible {
        return 0;
    }
    selected.saturating_sub(visible / 2).min(total - visible)
}

/// The hint on the bottom border: the keys of the current state, and what `Enter` will do with the
/// selected result, which depends on it: `remove` when the user already gave that reaction, `add`
/// otherwise. The longest form of each state still fits next to the title in the standard
/// 52-column pane.
fn hint_for(view: &react::View) -> String {
    let verb = if view.candidates.get(view.selected).is_some_and(|c| c.own) {
        "remove"
    } else {
        "add"
    };
    if view.typing {
        format!(" C-n/C-p move  Enter {verb}  Esc done ")
    } else {
        format!(" j/k move  Enter {verb}  / find  q close ")
    }
}

/// The note on the right of a result row: where it comes from, how many gave it, and `you`.
fn note_of(c: &Candidate) -> String {
    let mut parts: Vec<String> = Vec::new();
    match c.source {
        Source::Alias => parts.push("alias".to_string()),
        Source::Custom => parts.push("custom".to_string()),
        Source::Existing | Source::Unicode => {}
    }
    if c.count > 0 {
        parts.push(c.count.to_string());
    }
    if c.own {
        parts.push("you".to_string());
    }
    parts.join(", ")
}

/// Where the hardware cursor goes: right after the typed text on the search line, kept inside the
/// panel, and only while searching (browsing has no text input). `None` when the picker is closed
/// or browsing, or there is no room to draw it.
pub fn cursor_position(app: &App, areas: &Areas) -> Option<Position> {
    let view = react::view(app)?;
    if !view.typing {
        return None;
    }
    let (_, inner) = panel(areas);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }
    let typed = sanitize_line(&view.query).width();
    let x = inner.x + u16::try_from(PROMPT.width() + typed).unwrap_or(u16::MAX);
    Some(Position::new(x.min(inner.x + inner.width - 1), inner.y))
}

/// Draw the picker when it is open and its message is still held.
pub fn render(frame: &mut Frame, areas: &Areas, app: &App) {
    let Some(view) = react::view(app) else {
        return;
    };
    let theme = &app.theme;
    let (area, inner) = panel(areas);
    frame.render_widget(Clear, area);

    let title = " React ";
    let hint = hint_for(&view);
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.picker_border)
        .title(Span::styled(title, theme.picker_title));
    // The hint only when the border has room for it next to the title and the corners.
    if usize::from(area.width) >= title.width() + hint.width() + 4 {
        block =
            block.title_bottom(Line::from(Span::styled(hint, theme.picker_hint)).right_aligned());
    }
    frame.render_widget(block, area);

    let width = usize::from(inner.width);
    let visible = usize::from(inner.height).saturating_sub(1);
    let start = window_start(view.selected, visible, view.candidates.len());
    let shown = &view.candidates[start..(start + visible).min(view.candidates.len())];

    // The search line: the prompt and what was typed, cut to the width.
    let query = sanitize_line(&view.query).into_owned();
    let typed = fit_author(&query, width.saturating_sub(PROMPT.width())).into_owned();
    // Searching: live input. Browsing: dim, showing the last search, or saying how to start one.
    let search_line = if view.typing {
        Line::from(vec![
            Span::styled(PROMPT, theme.picker_item),
            Span::styled(typed, theme.picker_input),
        ])
    } else if query.is_empty() {
        Line::from(Span::styled(
            fit_author(PRESS_TO_SEARCH, width).into_owned(),
            theme.picker_hint,
        ))
    } else {
        Line::from(Span::styled(format!("{PROMPT}{typed}"), theme.picker_hint))
    };
    let mut lines: Vec<Line<'static>> = vec![search_line];

    if view.candidates.is_empty() {
        let empty_query = view.query.trim().trim_matches(':').is_empty();
        let text = match (empty_query, view.typing) {
            (false, _) => NO_MATCHES,
            (true, true) => TYPE_TO_SEARCH,
            (true, false) => NO_REACTIONS_YET,
        };
        lines.push(Line::from(Span::styled(
            fit_author(text, width).into_owned(),
            theme.picker_hint,
        )));
    } else {
        // A reaction that resolved to nothing is drawn raw: its `:name:` already is the glyph, so
        // the glyph cell stays empty and the glyph column is sized from real glyphs only.
        let emoji = app.emoji_view();
        let glyph_of = |c: &Candidate| -> String {
            if let Some(cells) =
                crate::ui::emoji::image_glyph(&c.name, &app.settings.emoji_aliases, &emoji)
            {
                cells
            } else if c.glyph == c.shown_name() {
                String::new()
            } else {
                c.glyph.clone()
            }
        };
        let glyph_width = shown.iter().map(|c| glyph_of(c).width()).max().unwrap_or(0);
        let lead = 2 + if glyph_width > 0 { glyph_width + 1 } else { 0 };
        // The name column never takes more than half of what is left after the lead.
        let name_cap = width.saturating_sub(lead + 2) / 2;
        let name_of = |c: &Candidate| fit_author(&c.shown_name(), name_cap).into_owned();
        let name_width = shown.iter().map(|c| name_of(c).width()).max().unwrap_or(0);

        for (offset, candidate) in shown.iter().enumerate() {
            let selected = start + offset == view.selected;
            let base = if candidate.own {
                theme.picker_own
            } else {
                theme.picker_item
            };
            let style = |s: Style| {
                if selected {
                    s.patch(theme.picker_selected)
                } else {
                    s
                }
            };
            let mark = if candidate.own { "* " } else { "  " };
            let glyph = glyph_of(candidate);
            let glyph = format!(
                "{glyph}{}",
                " ".repeat(glyph_width.saturating_sub(glyph.width()))
            );
            let gap = " ".repeat(usize::from(glyph_width > 0));
            let name = name_of(candidate);
            let name = format!(
                "{name}{}",
                " ".repeat(name_width.saturating_sub(name.width()))
            );
            let prefix = lead + name_width + 2;
            let note = note_of(candidate);
            let note = if width > prefix {
                fit_author(&note, width - prefix).into_owned()
            } else {
                String::new()
            };
            let mut text = format!("{mark}{glyph}{gap}{name}  {note}");
            // Cut a row that is still too wide, then pad it so a highlighted row spans the panel.
            if text.width() > width {
                text = fit_author(&text, width).into_owned();
            }
            let pad = " ".repeat(width.saturating_sub(text.width()));
            // The glyph of a custom emoji is the client's own cells until here.
            lines.push(Line::from(crate::ui::emoji::expand(
                vec![Span::styled(format!("{text}{pad}"), style(base))],
                theme.messages_custom_emoji,
                &mut false,
            )));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
    if let Some(position) = cursor_position(app, areas) {
        frame.set_cursor_position(position);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Focus, emoji::CustomEmojis};
    use crate::data::{CustomEmoji, Message, Reaction, RoomId, SendState};
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
    fn on_reactions(mut app: crate::app::App, reactions: Vec<Reaction>) -> crate::app::App {
        let room: RoomId = app.rooms[0].id.clone();
        let message = Message {
            room_id: room.clone(),
            reactions,
            send: SendState::Sent,
            ts: i64::MAX / 4,
            body: "deploy finished".to_string(),
            ..sample_message("m-pick")
        };
        app.messages.insert(room.clone(), vec![message]);
        app.msg_views.remove(&room);
        app.current_room = Some(room);
        app.own_username = Some("me".to_string());
        app.focus = Focus::Messages;
        app
    }

    fn typical() -> Vec<Reaction> {
        vec![
            reaction("+1", &["bob", "me", "ana"]),
            reaction("tada", &["ana"]),
            reaction("partyparrot", &["bob", "ana"]),
            reaction("rt_unknown", &["zed"]),
        ]
    }

    fn type_text(app: &mut crate::app::App, text: &str) {
        for c in text.chars() {
            press(app, &c.to_string());
        }
    }

    /// The rows of the panel in a rendered screen: from the title row to its bottom border.
    fn panel_rows(out: &str) -> Vec<&str> {
        let lines: Vec<&str> = out.lines().collect();
        let top = lines
            .iter()
            .position(|l| l.contains(" React "))
            .expect("panel title row");
        let bottom = lines
            .iter()
            .enumerate()
            .skip(top + 1)
            .find(|(_, l)| l.ends_with('\u{2518}'))
            .map(|(i, _)| i)
            .expect("panel bottom border");
        lines[top..=bottom].to_vec()
    }

    /// Start a new search (`/`) and type `text`: what it takes to search for something.
    fn search(app: &mut crate::app::App, text: &str) {
        press(app, "/");
        type_text(app, text);
    }

    fn open_picker(app: &mut crate::app::App) {
        press(app, "+");
        assert!(app.react_picker.is_some(), "the picker opened");
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
    fn a_custom_emoji_shows_its_image_in_the_picker_rows() {
        let mut app = on_reactions(app_80x24(), vec![reaction("rtshipit", &["me"])]);
        with_ready_image(&mut app);
        open_picker(&mut app);
        let buffer = render_buffer(&app, 80, 24);
        // The message behind the panel shows the emoji in its reaction row too (top rows).
        let cells: Vec<_> = image_cells(&buffer)
            .into_iter()
            .filter(|c| c.1 > 5)
            .collect();
        assert_eq!(cells.len(), 2, "one emoji, two cells: {cells:?}");
        for (_, _, fg, reversed) in &cells {
            assert_eq!(*fg, Color::Rgb(0, 0, 9));
            assert!(!reversed, "the selected row must not reverse the id");
        }
        let out = render_to_string(&app, 80, 24);
        assert!(
            out.contains(":rtshipit:"),
            "the name stays in its column: {out}"
        );
    }

    #[test]
    fn the_picker_asks_for_the_images_of_the_custom_emojis_it_lists() {
        let mut app = on_reactions(app_80x24(), vec![reaction("rtshipit", &["me"])]);
        with_ready_image(&mut app);
        app.images = crate::app::emoji::Images::default();
        app.effects.clear();
        open_picker(&mut app);
        press(&mut app, "<Esc>");
        assert!(
            app.effects
                .iter()
                .any(|e| matches!(e, crate::effect::Effect::FetchEmojiImage(_))),
            "{:?}",
            app.effects
        );
    }

    #[test]
    fn picker_browsing_80x24() {
        let mut app = on_reactions(app_80x24(), typical());
        app.settings
            .emoji_aliases
            .insert("partyparrot".to_string(), "🦜".to_string());
        open_picker(&mut app);
        let out = render_to_string(&app, 80, 24);
        let rows = panel_rows(&out);
        assert!(rows[0].contains(" React "), "{out}");
        assert!(rows[1].contains("press / to search"), "{out}");
        let all = rows.join("\n");
        assert!(all.contains(":+1:") && all.contains("3, you"), "{out}");
        assert!(all.contains("* "), "an own reaction is marked: {out}");
        assert!(all.contains("q close"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn picker_searching_80x24() {
        let mut app = on_reactions(app_80x24(), vec![reaction("ship_it", &["me"])]);
        app.custom_emojis = CustomEmojis::Loaded(vec![
            CustomEmoji {
                name: "ship_it".to_string(),
                aliases: vec!["rt_ship".to_string()],
                ..Default::default()
            },
            CustomEmoji {
                name: "shipping_cart".to_string(),
                aliases: Vec::new(),
                ..Default::default()
            },
        ]);
        app.settings
            .emoji_aliases
            .insert("ship_log".to_string(), "📜".to_string());
        open_picker(&mut app);
        search(&mut app, "ship");
        let out = render_to_string(&app, 80, 24);
        let all = panel_rows(&out).join("\n");
        assert!(all.contains("/ship"), "{out}");
        assert!(all.contains("Esc done"), "the searching hint: {out}");
        assert!(all.contains("custom, 1, you"), "{out}");
        assert!(all.contains("alias"), "{out}");
        assert!(all.contains(":ship:"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn picker_long_list_scrolled_80x24() {
        let mut app = on_reactions(app_80x24(), Vec::new());
        open_picker(&mut app);
        search(&mut app, "a");
        for _ in 0..20 {
            press(&mut app, "<C-n>");
        }
        let view = crate::app::react::view(&app).expect("open");
        assert_eq!(view.selected, 20);
        let out = render_to_string(&app, 80, 24);
        let rows = panel_rows(&out);
        let selected_name = view.candidates[view.selected].shown_name();
        // The selected result is on screen, and so are some before it and some after it.
        let at = rows
            .iter()
            .position(|r| r.contains(&format!("{selected_name} ")))
            .unwrap_or_else(|| panic!("{selected_name} not visible: {out}"));
        assert!(
            at > 2 && at < rows.len() - 2,
            "centred, not at an edge: {at} of {}",
            rows.len()
        );
        insta::assert_snapshot!(out);
    }

    #[test]
    fn picker_browsing_after_a_search_80x24() {
        let mut app = on_reactions(app_80x24(), vec![reaction("tada", &["me"])]);
        open_picker(&mut app);
        search(&mut app, "tad");
        press(&mut app, "<Esc>");
        press(&mut app, "j");
        let out = render_to_string(&app, 80, 24);
        let rows = panel_rows(&out);
        // The last search stays on the dim search line, its results stay, and the hint is the
        // browsing one.
        assert!(rows[1].contains("/tad"), "{out}");
        assert!(rows.last().unwrap().contains("j/k move"), "{out}");
        assert!(!rows.last().unwrap().contains("Esc done"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn picker_smallest_size() {
        let mut app = on_reactions(app_80x24(), typical());
        app.size = ratatui::layout::Size::new(40, 8);
        open_picker(&mut app);
        let out = render_to_string(&app, 40, 8);
        assert!(out.contains(" React "), "{out}");
        assert!(out.contains("press"), "the search line is there: {out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn the_window_keeps_the_selection_in_view_at_every_position() {
        for total in [1usize, 5, 9, 10, 11, 200] {
            for visible in [1usize, 3, 8, 11] {
                for selected in 0..total {
                    let start = window_start(selected, visible, total);
                    assert!(
                        start + visible.min(total) <= total.max(visible),
                        "{selected}/{total}/{visible}"
                    );
                    assert!(
                        selected >= start && selected < start + visible.max(1),
                        "selected {selected} outside {start}..+{visible} of {total}"
                    );
                }
            }
        }
        assert_eq!(window_start(0, 8, 200), 0);
        assert_eq!(window_start(199, 8, 200), 192);
        assert_eq!(window_start(100, 8, 200), 96);
    }

    #[test]
    fn the_hint_says_what_enter_will_do_with_the_selected_result() {
        let mut app = on_reactions(
            app_80x24(),
            vec![reaction("tada", &["ana"]), reaction("rocket", &["me"])],
        );
        open_picker(&mut app);
        let hint = |app: &crate::app::App| -> String {
            panel_rows(&render_to_string(app, 80, 24))
                .last()
                .expect("bottom border")
                .to_string()
        };
        // Someone else's reaction is selected: Enter adds the user's own.
        let bottom = hint(&app);
        assert!(bottom.contains("Enter add "), "{bottom}");
        assert!(!bottom.contains("remove"), "{bottom}");
        // The user's own reaction: Enter takes it back.
        press(&mut app, "j");
        let bottom = hint(&app);
        assert!(bottom.contains("Enter remove "), "{bottom}");
        // Searching for a name the user already gave says remove for it, and add for another.
        search(&mut app, "rocket");
        assert!(hint(&app).contains("Enter remove "), "{}", hint(&app));
        press(&mut app, "<C-c>");
        open_picker(&mut app);
        search(&mut app, "tada");
        assert!(hint(&app).contains("Enter add "), "{}", hint(&app));
    }

    #[test]
    fn the_hint_names_the_keys_of_the_current_state() {
        let mut app = on_reactions(app_80x24(), typical());
        open_picker(&mut app);
        let hint = |app: &crate::app::App| -> String {
            panel_rows(&render_to_string(app, 80, 24))
                .last()
                .expect("bottom border")
                .to_string()
        };
        let browsing = hint(&app);
        assert!(
            browsing.contains("j/k move") && browsing.contains("q close"),
            "{browsing}"
        );
        assert!(!browsing.contains("C-n/C-p"), "{browsing}");
        press(&mut app, "/");
        let searching = hint(&app);
        assert!(
            searching.contains("C-n/C-p move") && searching.contains("Esc done"),
            "{searching}"
        );
        assert!(!searching.contains("j/k"), "{searching}");
        press(&mut app, "<Esc>");
        assert!(hint(&app).contains("j/k move"), "{}", hint(&app));
    }

    #[test]
    fn every_hint_fits_the_standard_pane() {
        // The pane is 52 columns at 80x24: title, hint and the two corners must fit, in both
        // states and for the longer verb.
        let title = " React ".width();
        for verb in ["add", "remove"] {
            for hint in [
                format!(" C-n/C-p move  Enter {verb}  Esc done "),
                format!(" j/k move  Enter {verb}  / find  q close "),
            ] {
                assert!(title + hint.width() + 4 <= 52, "{hint:?}: {}", hint.width());
            }
        }
    }

    #[test]
    fn closed_draws_nothing() {
        let app = on_reactions(app_80x24(), typical());
        let out = render_to_string(&app, 80, 24);
        assert!(!out.contains(" React "), "{out}");
        assert!(!out.contains("q close"), "{out}");
    }

    #[test]
    fn text_for_an_empty_list_follows_the_state() {
        let mut app = on_reactions(app_80x24(), Vec::new());
        open_picker(&mut app);
        let panel = |app: &crate::app::App| panel_rows(&render_to_string(app, 80, 24)).join("\n");
        // Browsing a message with no reactions: nothing to browse, so say how to search.
        assert!(
            panel(&app).contains("no reactions yet: press / to search"),
            "{}",
            panel(&app)
        );
        // Searching with nothing typed yet.
        press(&mut app, "/");
        assert!(
            panel(&app).contains("type to search for an emoji"),
            "{}",
            panel(&app)
        );
        assert!(!panel(&app).contains("no reactions yet"), "{}", panel(&app));
        // Nothing found.
        type_text(&mut app, "zzzzqqqq");
        assert!(panel(&app).contains("no matches"), "{}", panel(&app));
    }

    #[test]
    fn the_cursor_is_on_the_search_line_only_while_searching() {
        let mut app = on_reactions(app_80x24(), typical());
        let areas = crate::ui::layout::areas(
            Rect::new(0, 0, 80, 24),
            app.settings.sidebar_width,
            app.input_rows(),
        );
        assert_eq!(cursor_position(&app, &areas), None, "closed");
        open_picker(&mut app);
        assert_eq!(
            cursor_position(&app, &areas),
            None,
            "browsing has no text input"
        );
        let (_, inner) = panel(&areas);
        press(&mut app, "/");
        assert_eq!(
            cursor_position(&app, &areas),
            Some(Position::new(inner.x + 1, inner.y)),
            "after the prompt"
        );
        type_text(&mut app, "ab");
        assert_eq!(
            cursor_position(&app, &areas),
            Some(Position::new(inner.x + 3, inner.y))
        );
        type_text(&mut app, &"x".repeat(200));
        let at = cursor_position(&app, &areas).expect("open");
        assert_eq!(at.y, inner.y);
        assert!(at.x < inner.x + inner.width, "inside the panel: {at:?}");
        // Leaving the search takes the cursor away again.
        press(&mut app, "<Esc>");
        assert_eq!(cursor_position(&app, &areas), None);
    }

    #[test]
    fn the_selected_row_and_own_rows_stand_out_without_colour() {
        let mut app = on_reactions(app_80x24(), typical());
        open_picker(&mut app);
        let buffer = render_buffer(&app, 80, 24);
        let out = render_to_string(&app, 80, 24);
        let rows = panel_rows(&out);
        let first = out
            .lines()
            .position(|l| l == rows[2])
            .expect("first result row");
        let cell = |tag: &str, within: &str| -> (u16, u16) {
            let (y, line) = out
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains(tag) && l.contains(within))
                .unwrap_or_else(|| panic!("no row with {tag}: {out}"));
            (
                line.find(tag).map(|b| line[..b].chars().count()).unwrap() as u16,
                y as u16,
            )
        };
        // The first result is selected (`+1`, which is also the user's own): reversed.
        let (x, y) = cell(":+1:", ":+1:");
        assert_eq!(y as usize, first);
        assert!(
            buffer[(x, y)].modifier.contains(Modifier::REVERSED),
            "selected row"
        );
        // A row that is neither selected nor own is plain; an own row that is not selected is bold.
        let (x, y) = cell(":tada:", ":tada:");
        let plain = buffer[(x, y)].modifier;
        assert!(
            !plain.contains(Modifier::REVERSED) && !plain.contains(Modifier::BOLD),
            "{plain:?}"
        );
        press(&mut app, "<C-n>");
        press(&mut app, "<C-n>");
        press(&mut app, "<C-n>");
        let buffer = render_buffer(&app, 80, 24);
        let (x, y) = cell(":+1:", ":+1:");
        let own = buffer[(x, y)].modifier;
        assert!(
            own.contains(Modifier::BOLD) && !own.contains(Modifier::REVERSED),
            "{own:?}"
        );
    }

    #[test]
    fn themed_elements_are_used() {
        let theme = "[picker.item]\nfg = 3\n[picker.selected]\nbg = 5\n[picker.input]\nfg = 6\n[picker.hint]\nfg = 2\n";
        let mut app = on_reactions(app_with_config(config_from_theme(theme), 80, 24), typical());
        open_picker(&mut app);
        let buffer = render_buffer(&app, 80, 24);
        let out = render_to_string(&app, 80, 24);
        let find = |out: &str, tag: &str| -> (u16, u16) {
            let (y, line) = out
                .lines()
                .enumerate()
                .find(|(_, l)| l.contains(tag))
                .unwrap_or_else(|| panic!("no row with {tag}: {out}"));
            (
                line[..line.find(tag).unwrap()].chars().count() as u16,
                y as u16,
            )
        };
        let (x, y) = find(&out, ":tada:");
        assert_eq!(buffer[(x, y)].fg, Color::Indexed(3), "themed item");
        let (x, y) = find(&out, ":+1:");
        assert_eq!(buffer[(x, y)].bg, Color::Indexed(5), "themed selected row");
        // Browsing: the search line says how to start, in the hint style.
        let (x, y) = find(&out, "press / to search");
        assert_eq!(buffer[(x, y)].fg, Color::Indexed(2), "themed hint");
        // Searching: the typed text is the input style.
        search(&mut app, "tad");
        let buffer = render_buffer(&app, 80, 24);
        let out2 = render_to_string(&app, 80, 24);
        let (y, line) = out2
            .lines()
            .enumerate()
            .find(|(_, l)| l.contains("/tad"))
            .expect("search line");
        let x = line.find("tad").map(|b| line[..b].chars().count()).unwrap() as u16;
        assert_eq!(buffer[(x, y as u16)].fg, Color::Indexed(6), "themed input");
    }

    #[test]
    fn hostile_text_is_drawn_sanitized_and_rows_never_overflow() {
        let long = "n".repeat(120);
        let mut app = on_reactions(
            app_80x24(),
            vec![
                reaction("bad\u{1b}[31mname\u{7}", &["bob"]),
                reaction(&long, &["bob"]),
            ],
        );
        open_picker(&mut app);
        let out = render_to_string(&app, 80, 24);
        assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'), "{out:?}");
        let rows = panel_rows(&out);
        for row in &rows[1..rows.len() - 1] {
            assert!(row.ends_with('\u{2502}'), "the border is intact: {row:?}");
        }
        assert!(
            rows.join("\n").contains('\u{2026}'),
            "an ellipsis marks the cut: {out}"
        );
        // A paste with control characters lands on one line (while searching).
        press(&mut app, "/");
        crate::app::update::update(
            &mut app,
            crate::action::Action::Paste("x\u{1b}[31my\nz".to_string()),
        );
        let out = render_to_string(&app, 80, 24);
        assert!(!out.contains('\u{1b}'), "{out:?}");
    }
}

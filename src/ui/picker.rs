//! The emoji box (the reaction picker): a small bordered panel over the bottom of the messages and
//! input column, above the status row. Top to bottom inside it: the search line, a row of category
//! tabs, the grid of emoji in sections and a footer that names the selected one.
//!
//! The content comes from `app::react::view` (sections, selection, query); this only lays it out.
//! Each emoji is a cell of three columns (two for the glyph, one blank), a section starts on a row
//! of its own under its heading, and the window of rows scrolls so the selection stays visible.
//! When the panel is short the footer goes first and then the tabs; the search line and one grid
//! row always stay. What does not fit is cut, never overflowed. The hardware cursor sits after the
//! typed text.

use crate::app::App;
use crate::app::react::{self, Candidate, Grid, Row, Source};
use crate::ui::layout::Areas;
use crate::ui::sanitize::sanitize_line;
use crate::ui::wrap::fit_author;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
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

/// What the grid and the footer say when a search matches nothing.
const NO_MATCHES: &str = "no matches";

/// Drawn in the two glyph columns of an emoji that has nothing to draw: a custom emoji whose image
/// is not available as an image, or a reaction the client has no glyph for.
const STAND_IN: &str = "··";

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

/// How many columns one emoji cell takes: two for the glyph and one blank.
const CELL_WIDTH: u16 = 3;

/// The areas of the screen as `ui::view` lays them out, from the size the app knows.
fn areas_of(app: &App) -> Areas {
    let area = Rect::new(0, 0, app.size.width, app.size.height);
    crate::ui::layout::areas_with_thread(
        area,
        app.settings.sidebar_width,
        app.input_rows(),
        app.thread_input_rows(),
        app.column_open(),
    )
}

/// Cells in one row of the grid on this screen: what up and down move by. At least one.
pub fn columns(app: &App) -> usize {
    let (_, inner) = panel(&areas_of(app));
    cells_per_row(inner.width)
}

/// Cells in a row of `width` columns: each takes its three, and one more column at the end holds
/// the closing marker of the selected cell.
fn cells_per_row(width: u16) -> usize {
    usize::from(width.saturating_sub(1) / CELL_WIDTH).max(1)
}

/// The hint on the bottom border: the keys of the current state, and what `Enter` will do with the
/// selected emoji, which depends on it: `remove` when the user already gave that reaction, `add`
/// otherwise. The longest form of each state still fits next to the title in the standard
/// 52-column pane.
fn hint_for(view: &react::View) -> String {
    let verb = if view.candidates.get(view.selected).is_some_and(|c| c.own) {
        "remove"
    } else {
        "add"
    };
    if view.typing {
        format!(" arrows move  Enter {verb}  Esc done ")
    } else {
        format!(" hjkl  Enter {verb}  / find  C-t tone ")
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

/// How the inner area of the panel is divided: the search line is always the first row, then the
/// tabs and the footer when the height allows, and the grid takes the rest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Split {
    tabs: bool,
    footer: bool,
    grid_rows: usize,
}

/// The footer goes below 5 inner rows and the tabs below 4; at least one grid row is left whenever
/// there is a row after the search line.
fn split(inner_height: u16) -> Split {
    let footer = inner_height >= 5;
    let tabs = inner_height >= 4;
    let used = 1 + usize::from(tabs) + usize::from(footer);
    Split {
        tabs,
        footer,
        grid_rows: usize::from(inner_height).saturating_sub(used),
    }
}

/// The flat indexes of the cells on the rows of the grid that are on screen, for the open picker.
/// `request_images` asks for the images of these and no others.
pub fn visible_cells(app: &App, view: &react::View) -> std::ops::Range<usize> {
    let (_, inner) = panel(&areas_of(app));
    let rows = Grid::new(&view.sections, cells_per_row(inner.width));
    let (start, end) = rows.window(view.selected, split(inner.height).grid_rows);
    let mut cells = rows.rows[start..end].iter().filter_map(|row| match row {
        Row::Cells { first, len, .. } => Some(*first..first + len),
        Row::Heading(_) => None,
    });
    match cells.next() {
        None => 0..0,
        Some(first) => first.start..cells.next_back().map_or(first.end, |last| last.end),
    }
}

/// `text` as exactly two columns: padded when it is narrower, the stand-in when it is wider.
fn two_columns(text: &str) -> String {
    match text.width() {
        2 => text.to_string(),
        w if w < 2 => format!("{text}{}", " ".repeat(2 - w)),
        _ => STAND_IN.to_string(),
    }
}

/// The two glyph columns of a cell: a custom emoji's image or stand-in, an emoji or alias as it is,
/// and the stand-in for a reaction the client could not turn into a glyph.
fn glyph_cells(app: &App, c: &Candidate) -> String {
    let emoji = app.emoji_view();
    if let Some(cells) = crate::ui::emoji::image_glyph(&c.name, &app.settings.emoji_aliases, &emoji)
    {
        cells
    } else if c.glyph == c.shown_name() || c.source == Source::Custom {
        STAND_IN.to_string()
    } else {
        two_columns(&sanitize_line(&c.glyph))
    }
}

/// The footer: the selected emoji as `:name:` with what the message knows about it, and the skin
/// tone in use on the right. Cut to the width.
fn footer_line(view: &react::View, tone: u8, width: usize, app: &App) -> Line<'static> {
    let style = app.theme.picker_footer;
    let right = if tone == 0 {
        "tone: default".to_string()
    } else {
        format!("tone: {tone}")
    };
    let left = match view.candidates.get(view.selected) {
        Some(candidate) => {
            let note = note_of(candidate);
            if note.is_empty() {
                candidate.shown_name()
            } else {
                format!("{}  {note}", candidate.shown_name())
            }
        }
        None => NO_MATCHES.to_string(),
    };
    let room = width.saturating_sub(right.width() + 1);
    let left = fit_author(&left, room).into_owned();
    let pad = " ".repeat(width.saturating_sub(left.width() + right.width()));
    Line::from(Span::styled(format!("{left}{pad}{right}"), style))
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
    let layout = split(inner.height);

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

    // The tabs: one emoji per section, the one the selection is in marked. While a search has
    // results the only section is the results, which is not a tab to go back to: none is marked.
    if layout.tabs {
        let current = react::section_of(&view.sections, view.selected)
            .filter(|at| view.sections[*at].kind != react::SectionKind::Results);
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut used = 0;
        for (at, section) in view.sections.iter().enumerate() {
            if section.kind == react::SectionKind::Results || used + usize::from(CELL_WIDTH) > width
            {
                continue;
            }
            let style = if Some(at) == current {
                theme.picker_tab_current
            } else {
                theme.picker_tab
            };
            spans.push(Span::styled(
                format!("{} ", two_columns(section.kind.tab())),
                style,
            ));
            used += usize::from(CELL_WIDTH);
        }
        lines.push(Line::from(spans));
    }

    // The grid: the window of rows around the selection.
    let columns = cells_per_row(inner.width);
    let rows = Grid::new(&view.sections, columns);
    let (start, end) = rows.window(view.selected, layout.grid_rows);
    for row in &rows.rows[start..end] {
        match *row {
            Row::Heading(section) => {
                let title = view.sections[section].kind.title();
                lines.push(Line::from(Span::styled(
                    fit_author(title, width).into_owned(),
                    theme.picker_heading,
                )));
            }
            Row::Cells { first, len, .. } => {
                // Each cell is a marker column and two glyph columns. The selected cell is
                // wrapped in `[` and `]`, because a terminal may leave the background of a colour
                // emoji unpainted, so the highlight alone can be missed.
                let mut spans: Vec<Span<'static>> = Vec::new();
                let mut closing = false;
                for index in first..first + len {
                    let candidate = &view.candidates[index];
                    let base = if candidate.own {
                        theme.picker_own
                    } else {
                        theme.picker_item
                    };
                    let selected = index == view.selected;
                    let marked = base.patch(theme.picker_selected);
                    let marker = if selected {
                        Span::styled("[", marked)
                    } else if closing {
                        Span::styled("]", marked)
                    } else {
                        Span::styled(" ", theme.picker_item)
                    };
                    spans.push(marker);
                    closing = selected;
                    let style = if selected { marked } else { base };
                    spans.push(Span::styled(glyph_cells(app, candidate), style));
                }
                spans.push(if closing {
                    Span::styled("]", theme.picker_item.patch(theme.picker_selected))
                } else {
                    Span::styled(" ", theme.picker_item)
                });
                // The glyph of a custom emoji is the client's own cells until here.
                let mut spans =
                    crate::ui::emoji::expand(spans, theme.messages_custom_emoji, &mut false);
                // Write the blank columns after the last cell too, so a glyph the terminal draws
                // wider than measured, or one left from the frame before, cannot stay there.
                let rest = width.saturating_sub(len * usize::from(CELL_WIDTH) + 1);
                spans.push(Span::styled(" ".repeat(rest), theme.picker_item));
                lines.push(Line::from(spans));
            }
        }
    }
    if rows.rows.is_empty() && layout.grid_rows > 0 && !layout.footer {
        lines.push(Line::from(Span::styled(
            fit_author(NO_MATCHES, width).into_owned(),
            theme.picker_hint,
        )));
    }

    // The footer sits on the last row of the panel, whatever the grid holds.
    if layout.footer {
        let used = lines.len();
        let want = usize::from(inner.height) - 1;
        for _ in used..want {
            lines.push(Line::default());
        }
        lines.push(footer_line(&view, app.emoji_recents.tone, width, app));
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

    /// Where the panel's inner area is on the screen of `app`.
    fn inner_of(app: &crate::app::App) -> Rect {
        panel(&areas_of(app)).1
    }

    /// The first cell of the panel's inner area that draws `symbol`.
    fn find_symbol(
        buffer: &ratatui::buffer::Buffer,
        inner: Rect,
        symbol: &str,
    ) -> Option<(u16, u16)> {
        (inner.y..inner.y + inner.height)
            .flat_map(|y| (inner.x..inner.x + inner.width).map(move |x| (x, y)))
            .find(|(x, y)| buffer[(*x, *y)].symbol() == symbol)
    }

    #[test]
    fn a_custom_emoji_shows_its_image_in_its_grid_cell() {
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
            assert!(!reversed, "the selected cell must not reverse the id");
        }
        // It is the selected cell, so the footer names it.
        let out = render_to_string(&app, 80, 24);
        assert!(
            panel_rows(&out).iter().any(|r| r.contains(":rtshipit:")),
            "{out}"
        );
    }

    #[test]
    fn a_custom_emoji_without_an_image_takes_two_stand_in_cells() {
        let mut app = on_reactions(app_80x24(), Vec::new());
        app.custom_emojis = CustomEmojis::Loaded(vec![CustomEmoji {
            name: "rtshipit".to_string(),
            ext: "png".to_string(),
            ..Default::default()
        }]);
        open_picker(&mut app);
        // Walk to the last section, "Custom".
        for _ in 0..12 {
            press(&mut app, "L");
        }
        let out = render_to_string(&app, 80, 24);
        let all = panel_rows(&out).join("\n");
        assert!(all.contains("Custom"), "the heading: {out}");
        assert!(all.contains(STAND_IN), "the stand-in: {out}");
        assert!(all.contains(":rtshipit:"), "the footer names it: {out}");
        insta::assert_snapshot!(out);
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
    fn only_the_custom_emojis_on_the_rows_on_screen_are_requested() {
        let mut app = on_reactions(app_80x24(), Vec::new());
        app.graphics = crate::app::emoji::Graphics::Supported;
        app.custom_emojis = CustomEmojis::Loaded(
            (0..400)
                .map(|n| CustomEmoji {
                    name: format!("c{n:03}"),
                    ext: "png".to_string(),
                    etag: format!("e{n}"),
                    ..Default::default()
                })
                .collect(),
        );
        open_picker(&mut app);
        crate::app::update::update(&mut app, crate::action::Action::KeyTimeout);
        assert!(
            app.images.requested_count() == 0,
            "the Custom section is far below the first rows"
        );
        for _ in 0..12 {
            press(&mut app, "L");
        }
        crate::app::update::update(&mut app, crate::action::Action::KeyTimeout);
        let view = crate::app::react::view(&app).unwrap();
        let on_screen = visible_cells(&app, &view);
        let asked = app.images.requested_count();
        assert!(asked > 0, "the section on screen is asked for");
        assert!(asked <= on_screen.len(), "{asked} for {on_screen:?}");
        assert!(asked < 400, "not the whole list");
    }

    #[test]
    fn picker_browsing_80x24() {
        let mut app = on_reactions(app_80x24(), typical());
        app.settings
            .emoji_aliases
            .insert("partyparrot".to_string(), "🦜".to_string());
        app.emoji_recents.record("rocket");
        open_picker(&mut app);
        let out = render_to_string(&app, 80, 24);
        let rows = panel_rows(&out);
        assert!(rows[0].contains(" React "), "{out}");
        assert!(rows[1].contains("press / to search"), "{out}");
        let all = rows.join("\n");
        assert!(all.contains("On this message"), "{out}");
        assert!(
            all.contains("Enter remove"),
            "the first reaction is the user's own: {out}"
        );
        // The footer names the selection with its count and `you`, and the tone on the right.
        let footer = &rows[rows.len() - 2];
        assert!(
            footer.contains(":+1:") && footer.contains("3, you"),
            "{out}"
        );
        assert!(footer.contains("tone: default"), "{out}");
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
        assert!(all.contains("Results"), "one results section: {out}");
        assert!(!all.contains("On this message"), "{out}");
        assert!(all.contains("Esc done"), "the searching hint: {out}");
        // The exact match comes first, and the footer names it.
        assert!(
            panel_rows(&out).iter().any(|r| r.contains(":ship:")),
            "{out}"
        );
        insta::assert_snapshot!(out);
    }

    #[test]
    fn picker_scrolled_to_a_later_section_80x24() {
        let mut app = on_reactions(app_80x24(), Vec::new());
        open_picker(&mut app);
        for _ in 0..3 {
            press(&mut app, "L");
        }
        let out = render_to_string(&app, 80, 24);
        let all = panel_rows(&out).join("\n");
        // Three jumps from the first section land on "Travel & Places". Its heading is in view.
        assert!(all.contains("Travel & Places"), "{out}");
        assert!(!all.contains("Smileys & People"), "scrolled past: {out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn picker_browsing_after_a_search_80x24() {
        let mut app = on_reactions(app_80x24(), vec![reaction("tada", &["me"])]);
        open_picker(&mut app);
        search(&mut app, "tad");
        press(&mut app, "<Esc>");
        press(&mut app, "l");
        let out = render_to_string(&app, 80, 24);
        let rows = panel_rows(&out);
        // The last search stays on the dim search line, its results stay, and the hint is the
        // browsing one.
        assert!(rows[1].contains("/tad"), "{out}");
        assert!(rows.last().unwrap().contains("hjkl"), "{out}");
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
        let rows = panel_rows(&out);
        assert!(rows.len() >= 4, "one grid row at least: {out}");
        assert!(
            rows.iter().any(|r| r.contains("On this")),
            "the heading, cut: {out}"
        );
        insta::assert_snapshot!(out);
    }

    #[test]
    fn the_footer_goes_first_then_the_tabs_and_the_search_line_and_a_grid_row_stay() {
        let rows = |h: u16| split(h);
        assert_eq!(
            rows(12),
            Split {
                tabs: true,
                footer: true,
                grid_rows: 9
            }
        );
        assert_eq!(
            rows(5),
            Split {
                tabs: true,
                footer: true,
                grid_rows: 2
            }
        );
        assert_eq!(
            rows(4),
            Split {
                tabs: true,
                footer: false,
                grid_rows: 2
            }
        );
        assert_eq!(
            rows(3),
            Split {
                tabs: false,
                footer: false,
                grid_rows: 2
            }
        );
        assert_eq!(
            rows(2),
            Split {
                tabs: false,
                footer: false,
                grid_rows: 1
            }
        );

        // And on a real screen: shorter and shorter terminals.
        let screen = |height: u16| {
            let mut app = on_reactions(app_80x24(), typical());
            app.size = ratatui::layout::Size::new(80, height);
            open_picker(&mut app);
            render_to_string(&app, 80, height)
        };
        let tall = screen(24);
        assert!(
            tall.contains("tone: default") && tall.contains('💬'),
            "{tall}"
        );
        // The smallest supported screen has four rows inside the box: the footer is the one that goes.
        let small = screen(8);
        assert!(!small.contains("tone: default"), "the footer went: {small}");
        assert!(small.contains('💬'), "the tabs stayed: {small}");
        assert!(
            small.contains("On this message"),
            "a grid row stayed: {small}"
        );
        assert!(small.contains("press / to search"), "{small}");
    }

    #[test]
    fn a_row_holds_as_many_cells_as_fit_and_none_is_wider_than_the_box() {
        let mut app = on_reactions(app_80x24(), Vec::new());
        open_picker(&mut app);
        // 52 columns: 50 inside the borders, 16 cells of 3.
        assert_eq!(columns(&app), 16);
        let out = render_to_string(&app, 80, 24);
        let rows = panel_rows(&out);
        for row in &rows[1..rows.len() - 1] {
            assert!(row.ends_with('\u{2502}'), "the border is intact: {row:?}");
        }
        let view = crate::app::react::view(&app).unwrap();
        let grid = Grid::new(&view.sections, columns(&app));
        assert!(grid.rows.iter().all(|row| match row {
            Row::Cells { len, .. } => *len <= 16,
            Row::Heading(_) => true,
        }));
        // The same at another width.
        app.settings.sidebar_width = 20;
        assert_eq!(columns(&app), (80 - 20 - 2) / 3);
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
        press(&mut app, "l");
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
            browsing.contains("hjkl")
                && browsing.contains("/ find")
                && browsing.contains("C-t tone"),
            "{browsing}"
        );
        assert!(!browsing.contains("arrows"), "{browsing}");
        press(&mut app, "/");
        let searching = hint(&app);
        assert!(
            searching.contains("arrows move") && searching.contains("Esc done"),
            "{searching}"
        );
        assert!(!searching.contains("hjkl"), "{searching}");
        press(&mut app, "<Esc>");
        assert!(hint(&app).contains("hjkl"), "{}", hint(&app));
    }

    #[test]
    fn every_hint_fits_the_standard_pane() {
        // The pane is 52 columns at 80x24: title, hint and the two corners must fit, in both
        // states and for the longer verb.
        let title = " React ".width();
        for verb in ["add", "remove"] {
            for hint in [
                format!(" arrows move  Enter {verb}  Esc done "),
                format!(" hjkl  Enter {verb}  / find  C-t tone "),
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
        assert!(!out.contains("tone: default"), "{out}");
    }

    #[test]
    fn a_search_with_no_match_says_so_in_the_footer_and_without_one_in_the_grid() {
        let mut app = on_reactions(app_80x24(), Vec::new());
        open_picker(&mut app);
        search(&mut app, "zzzzqqqq");
        let out = render_to_string(&app, 80, 24);
        let rows = panel_rows(&out);
        assert!(rows[rows.len() - 2].contains("no matches"), "{out}");
        assert!(!rows.join("\n").contains("Results"), "{out}");
        // Without a footer the grid says it instead.
        let mut app = on_reactions(app_80x24(), Vec::new());
        app.size = ratatui::layout::Size::new(80, 8);
        open_picker(&mut app);
        search(&mut app, "zzzzqqqq");
        let out = render_to_string(&app, 80, 8);
        assert!(out.contains("no matches"), "{out}");
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
    fn the_selected_cell_the_own_cells_and_the_current_tab_stand_out_without_colour() {
        let mut app = on_reactions(
            app_80x24(),
            vec![reaction("tada", &["ana"]), reaction("+1", &["me"])],
        );
        open_picker(&mut app);
        let inner = inner_of(&app);
        let buffer = render_buffer(&app, 80, 24);
        // Rows: search, tabs, heading, then the cells of "On this message".
        let (x, y) = (inner.x + 1, inner.y + 3);
        assert_eq!(
            buffer[(x - 1, y)].symbol(),
            "[",
            "the selected cell is bracketed"
        );
        assert_eq!(buffer[(x + 2, y)].symbol(), "]");
        assert_eq!(buffer[(x, y)].symbol(), "🎉");
        assert!(
            buffer[(x, y)].modifier.contains(Modifier::REVERSED),
            "selected"
        );
        // The user's own reaction is bold when it is not the selected one.
        let own = buffer[(x + 3, y)].modifier;
        assert_eq!(buffer[(x + 3, y)].symbol(), "👍");
        assert!(
            own.contains(Modifier::BOLD) && !own.contains(Modifier::REVERSED),
            "{own:?}"
        );
        // A plain cell is neither.
        let (px, py) = find_symbol(&buffer, inner, "😀").expect("a plain emoji");
        let plain = buffer[(px, py)].modifier;
        assert!(
            !plain.contains(Modifier::REVERSED) && !plain.contains(Modifier::BOLD),
            "{plain:?}"
        );
        // The first tab is the current one: underlined. The second is not.
        let tab = |col: u16| buffer[(inner.x + col, inner.y + 1)].modifier;
        assert!(tab(0).contains(Modifier::UNDERLINED), "{:?}", tab(0));
        assert!(!tab(3).contains(Modifier::UNDERLINED), "{:?}", tab(3));
    }

    #[test]
    fn the_current_tab_follows_the_selection_and_none_is_current_in_results() {
        let mut app = on_reactions(app_80x24(), Vec::new());
        open_picker(&mut app);
        let underlined = |app: &crate::app::App| -> Vec<u16> {
            let inner = inner_of(app);
            let buffer = render_buffer(app, 80, 24);
            (0..inner.width / 3)
                .filter(|n| {
                    buffer[(inner.x + n * 3, inner.y + 1)]
                        .modifier
                        .contains(Modifier::UNDERLINED)
                })
                .collect()
        };
        assert_eq!(underlined(&app), [0], "Smileys & People");
        press(&mut app, "L");
        assert_eq!(underlined(&app), [1], "Animals & Nature");
        search(&mut app, "tad");
        assert!(underlined(&app).is_empty(), "no current tab in results");
    }

    #[test]
    fn a_skin_tone_shows_in_the_grid_and_the_footer() {
        let mut app = on_reactions(app_80x24(), vec![reaction("tada", &["ana"])]);
        open_picker(&mut app);
        press(&mut app, "<C-t>");
        press(&mut app, "<C-t>");
        search(&mut app, "thumbsup");
        let inner = inner_of(&app);
        let buffer = render_buffer(&app, 80, 24);
        assert!(
            find_symbol(&buffer, inner, "👍🏼").is_some(),
            "toned thumbs up"
        );
        assert!(find_symbol(&buffer, inner, "👍").is_none());
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("tone: 2"), "{out}");
    }

    #[test]
    fn themed_elements_are_used() {
        let theme = "[picker.item]\nfg = 3\n[picker.selected]\nbg = 5\n[picker.input]\nfg = 6\n\
                     [picker.hint]\nfg = 2\n[picker.tab]\nfg = 1\n[picker.tab_current]\nfg = 4\n\
                     [picker.heading]\nfg = 7\n[picker.footer]\nfg = 8\n";
        let mut app = on_reactions(app_with_config(config_from_theme(theme), 80, 24), typical());
        open_picker(&mut app);
        let inner = inner_of(&app);
        let buffer = render_buffer(&app, 80, 24);
        // The cells of "On this message": the first is selected, the second a plain one.
        assert_eq!(
            buffer[(inner.x, inner.y + 3)].bg,
            Color::Indexed(5),
            "themed selected"
        );
        assert_eq!(
            buffer[(inner.x + 3, inner.y + 3)].fg,
            Color::Indexed(3),
            "themed item"
        );
        assert_eq!(
            buffer[(inner.x, inner.y + 1)].fg,
            Color::Indexed(4),
            "themed current tab"
        );
        assert_eq!(
            buffer[(inner.x + 3, inner.y + 1)].fg,
            Color::Indexed(1),
            "themed tab"
        );
        assert_eq!(
            buffer[(inner.x, inner.y + 2)].fg,
            Color::Indexed(7),
            "themed heading"
        );
        assert_eq!(
            buffer[(inner.x, inner.y + inner.height - 1)].fg,
            Color::Indexed(8),
            "themed footer"
        );
        // Browsing: the search line says how to start, in the hint style.
        assert_eq!(
            buffer[(inner.x, inner.y)].fg,
            Color::Indexed(2),
            "themed hint"
        );
        // Searching: the typed text is the input style.
        search(&mut app, "tad");
        let buffer = render_buffer(&app, 80, 24);
        assert_eq!(
            buffer[(inner.x + 1, inner.y)].fg,
            Color::Indexed(6),
            "themed input"
        );
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
        app.custom_emojis = CustomEmojis::Loaded(vec![CustomEmoji {
            name: "evil\u{1b}[2Jname".to_string(),
            ..Default::default()
        }]);
        open_picker(&mut app);
        let out = render_to_string(&app, 80, 24);
        assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'), "{out:?}");
        // The footer names the first reaction, sanitized.
        assert!(panel_rows(&out).iter().any(|r| r.contains(":bad")), "{out}");
        // And the long one, selected, is cut with an ellipsis in the footer.
        press(&mut app, "l");
        let out = render_to_string(&app, 80, 24);
        let rows = panel_rows(&out);
        let footer = &rows[rows.len() - 2];
        assert!(
            footer.contains('\u{2026}'),
            "an ellipsis marks the cut: {out}"
        );
        assert!(footer.contains("tone: default"), "the tone stays: {out}");
        for row in &rows[1..rows.len() - 1] {
            assert!(row.ends_with('\u{2502}'), "the border is intact: {row:?}");
        }
        // The hostile custom emoji is not offered, in the grid or in a search.
        for _ in 0..12 {
            press(&mut app, "L");
        }
        assert!(
            !crate::app::react::view(&app)
                .unwrap()
                .candidates
                .iter()
                .any(|c| c.source == crate::app::react::Source::Custom),
            "not offered"
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

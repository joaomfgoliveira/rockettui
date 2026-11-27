//! Input pane: the multiline draft with its view window, an empty-state hint and the hardware
//! cursor rules (D-14, D-18).

use crate::app::input_buf::InputBuffer;
use crate::app::{App, Focus, InputTarget, Mode};
use crate::ui::emoji::{expand, keep_image_ids};
use crate::ui::pane_block;
use ratatui::Frame;
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

/// Where the hardware cursor goes: on the draft in Insert mode and in Normal mode while the
/// input bar is focused (so the D-18 motions are visible), nowhere otherwise. Command mode keeps
/// its cursor on the command line (`status::cmdline_cursor`).
pub fn cursor_position(app: &App, area: Rect) -> Option<Position> {
    cursor_in(app, area, &app.input)
}

fn cursor_in(app: &App, area: Rect, buffer: &InputBuffer) -> Option<Position> {
    let editing =
        app.mode == Mode::Insert || (app.mode == Mode::Normal && app.focus == Focus::Input);
    let inner = pane_block("", false, &app.theme).inner(area);
    if !editing || inner.width == 0 || inner.height == 0 {
        return None;
    }
    let view = app.emoji_view();
    let (x, y) = buffer.cursor_visual(inner.width, &view);
    let top = u16::try_from(buffer.view_top(inner.width, inner.height, &view)).unwrap_or(u16::MAX);
    Some(Position::new(
        inner.x + x.min(inner.width - 1),
        inner.y + y.saturating_sub(top).min(inner.height - 1),
    ))
}

/// The room's input bar. While the thread has a bar of its own and the live text is the thread's,
/// this one shows the room's draft and is not the one being edited.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let live = !app.thread_bar_is_live();
    let title = if live && app.editing.is_some() {
        " Editing: Enter saves, Esc cancels ".to_string()
    } else if live && app.input_target != InputTarget::Room {
        // One shared bar (the thread takes the messages' place): say where the text goes.
        reply_title(&app.input_target)
    } else {
        " Input ".to_string()
    };
    bar(frame, area, app, app.room_bar_buffer(), &title, live);
}

/// The thread's own input bar, under the thread pane.
pub fn render_thread(frame: &mut Frame, area: Rect, app: &App) {
    let live = app.thread_bar_is_live();
    let target = if live {
        app.input_target.clone()
    } else {
        InputTarget::Room
    };
    let title = if live && app.editing.is_some() {
        " Editing: Enter saves, Esc cancels ".to_string()
    } else if live {
        reply_title(&target)
    } else {
        " Reply ".to_string()
    };
    bar(frame, area, app, app.thread_bar_buffer(), &title, live);
}

/// The title of a bar whose text is a thread reply, with the also-in-channel state.
fn reply_title(target: &InputTarget) -> String {
    match target {
        InputTarget::Thread {
            also_in_channel: true,
            ..
        } => " Reply in thread, also to channel ".to_string(),
        _ => " Reply in thread ".to_string(),
    }
}

/// One drawn draft row as spans: the draft style, with each mention or channel reference of
/// `marks` (byte ranges of `row`, from `mention::strip`) in its kind's style over it.
fn mention_spans(
    row: String,
    marks: &[(std::ops::Range<usize>, crate::ui::mention::Kind)],
    theme: &crate::config::Theme,
) -> Vec<Span<'static>> {
    let base = theme.input_text;
    let mut spans = Vec::new();
    let mut at = 0;
    for (range, kind) in marks {
        if range.start > at {
            spans.push(Span::styled(row[at..range.start].to_string(), base));
        }
        spans.push(Span::styled(
            row[range.start..range.end].to_string(),
            base.patch(kind.style(theme)),
        ));
        at = range.end;
    }
    if at < row.len() || spans.is_empty() {
        spans.push(Span::styled(row[at..].to_string(), base));
    }
    spans
}

/// Draw one bar. `live` is true when the bar shows the text being edited: only that one takes the
/// focus border and the hardware cursor.
fn bar(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    buffer: Option<&InputBuffer>,
    title: &str,
    live: bool,
) {
    let focused = live && app.focus == Focus::Input;
    let block = pane_block(title, focused, &app.theme);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return;
    }
    match buffer.filter(|b| !b.is_empty()) {
        Some(buffer) => {
            let view = app.emoji_view();
            let top = buffer.view_top(inner.width, inner.height, &view);
            // `drawn_rows` cleans the text it draws (T-01-01) and swaps each custom emoji for the
            // cells of its image or stand-in, which `expand` turns into what is painted, as for
            // a message. The cursor math reads the same units, so it stays where the text ends.
            let mut inside = false;
            // The mentions of the rows above the first drawn one still decide whether a token is
            // open at its top, so every row goes through `strip` and only then is cut to the view.
            let mut open = None;
            let rows: Vec<Line> = buffer
                .drawn_rows(inner.width, &view, app.own_username.as_deref())
                .into_iter()
                .map(|row| crate::ui::mention::strip(&row, &mut open))
                .skip(top)
                .take(usize::from(inner.height))
                .map(|(row, marks)| {
                    let spans = mention_spans(row, &marks, &app.theme);
                    let spans = expand(spans, app.theme.messages_custom_emoji, &mut inside);
                    Line::from(spans).style(app.theme.input_text)
                })
                .collect();
            frame.render_widget(Paragraph::new(rows), inner);
            keep_image_ids(frame.buffer_mut(), inner);
        }
        None if live && app.mode == Mode::Insert => {}
        None => {
            let hint = Line::styled("press i to write", app.theme.input_hint);
            frame.render_widget(Paragraph::new(hint), inner);
        }
    }
    if live && let Some(pos) = cursor_in(app, area, &app.input) {
        frame.set_cursor_position(pos);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Mode;
    use crate::test_util::{
        app_80x24, app_with_config, config_from_theme, press, render_buffer, render_to_string,
    };
    use crate::ui;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;

    fn input_area(app: &App) -> Rect {
        app.areas().input
    }

    #[test]
    fn hardware_cursor_rules() {
        let mut app = app_80x24();
        // Normal mode, sidebar or messages focus: no cursor.
        assert_eq!(cursor_position(&app, input_area(&app)), None);
        press(&mut app, "<C-w>l");
        assert_eq!(cursor_position(&app, input_area(&app)), None);
        // Normal mode with the input bar focused: the D-18 cursor.
        press(&mut app, "<C-w>j");
        let area = input_area(&app);
        assert_eq!(
            cursor_position(&app, area),
            Some(Position::new(area.x + 1, area.y + 1))
        );
        // Insert mode: the cursor follows the draft.
        press(&mut app, "iab");
        let area = input_area(&app);
        assert_eq!(
            cursor_position(&app, area),
            Some(Position::new(area.x + 1 + 2, area.y + 1))
        );
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("backend");
        terminal.draw(|f| ui::view(f, &app)).expect("draw");
        let shown = terminal.get_cursor_position().expect("cursor");
        assert_eq!(Some(shown), cursor_position(&app, area));
        // A multi-line draft moves the cursor down inside the growing bar.
        press(&mut app, "<C-j>x");
        let area = input_area(&app);
        assert_eq!(app.input_rows(), 2);
        assert_eq!(
            cursor_position(&app, area),
            Some(Position::new(area.x + 1 + 1, area.y + 1 + 1))
        );
        // Command mode: the cursor lives on the command line instead.
        press(&mut app, "<Esc>:");
        assert_eq!(app.mode, Mode::Command);
        assert_eq!(cursor_position(&app, input_area(&app)), None);
    }

    #[test]
    fn themed_hint() {
        let config = config_from_theme("[input.hint]\nfg = 5\n");
        let app = app_with_config(config, 80, 24);
        let screen = render_to_string(&app, 80, 24);
        let (y, row) = screen
            .lines()
            .enumerate()
            .find(|(_, row)| row.contains("press i to write"))
            .expect("hint row");
        let x = row[..row.find("press i to write").expect("hint")]
            .chars()
            .count();
        let buffer = render_buffer(&app, 80, 24);
        for dx in 0.."press i to write".len() {
            let cell = &buffer[(
                u16::try_from(x + dx).expect("x"),
                u16::try_from(y).expect("y"),
            )];
            assert_eq!(cell.fg, Color::Indexed(5), "cell {dx}");
        }
        // Positive control: the default hint is dim with no color.
        let app = app_80x24();
        let buffer = render_buffer(&app, 80, 24);
        let cell = &buffer[(u16::try_from(x).expect("x"), u16::try_from(y).expect("y"))];
        assert_eq!(cell.fg, Color::Reset);
    }

    #[test]
    fn themed_draft_text() {
        let config = config_from_theme("[input.text]\nfg = 2\n");
        let mut app = app_with_config(config, 80, 24);
        press(&mut app, "<C-w>jiab");
        let buffer = render_buffer(&app, 80, 24);
        let area = input_area(&app);
        for x in area.x + 1..area.x + 3 {
            assert_eq!(buffer[(x, area.y + 1)].fg, Color::Indexed(2), "cell {x}");
        }
    }

    // ---- mentions in the draft ----

    /// The foreground of the draft cells at the first row of the bar, from its left edge.
    fn draft_fgs(app: &App, width: u16) -> Vec<(String, Color, Color)> {
        let buffer = render_buffer(app, width, 24);
        let area = input_area(app);
        (area.x + 1..area.right() - 1)
            .map(|x| {
                let cell = &buffer[(x, area.y + 1)];
                (cell.symbol().to_string(), cell.fg, cell.bg)
            })
            .collect()
    }

    #[test]
    fn a_mention_in_the_draft_is_highlighted_and_the_text_is_unchanged() {
        let mut app = app_80x24();
        app.own_username = Some("me".to_string());
        press(&mut app, "<C-w>jihi @bob and @me see #dev");
        let t = app.theme.clone();
        let cells = draft_fgs(&app, 80);
        let at = |needle: &str| {
            let text: String = cells.iter().map(|c| c.0.as_str()).collect();
            text.find(needle).expect(needle)
        };
        let user = at("@bob");
        assert_eq!(cells[user].1, t.messages_mention.fg.unwrap());
        assert_eq!(cells[user + 3].1, t.messages_mention.fg.unwrap());
        assert_ne!(
            cells[user + 4].1,
            t.messages_mention.fg.unwrap(),
            "the space after"
        );
        let own = at("@me");
        assert_eq!(cells[own].1, t.messages_mention_self.fg.unwrap());
        assert_ne!(
            cells[own].1, cells[user].1,
            "own differs from another user's"
        );
        let chan = at("#dev");
        assert_eq!(cells[chan].1, t.messages_channel.fg.unwrap());
        // The plain words keep the draft style, and the buffer holds exactly what was typed.
        assert_eq!(cells[0].1, Color::Reset);
        assert_eq!(app.input.text(), "hi @bob and @me see #dev");
    }

    #[test]
    fn an_email_address_and_a_code_span_in_the_draft_stay_plain() {
        let mut app = app_80x24();
        press(&mut app, "<C-w>jimail a@b.com `@bob`");
        let t = app.theme.clone();
        for (symbol, fg, bg) in draft_fgs(&app, 80) {
            assert_ne!(Some(fg), t.messages_mention.fg, "{symbol:?}");
            assert_eq!(bg, Color::Reset, "{symbol:?}");
        }
    }

    #[test]
    fn a_mention_wrapped_in_the_draft_is_highlighted_on_both_rows() {
        let mut app = app_80x24();
        let name = "x".repeat(90);
        press(&mut app, &format!("<C-w>jihi @{name}"));
        let t = app.theme.clone();
        let buffer = render_buffer(&app, 80, 24);
        let area = input_area(&app);
        let mention = t.messages_mention.fg.unwrap();
        // The bar has the room of its content, so the cut token is on at least two rows.
        let rows_with_mention = (area.y + 1..area.bottom() - 1)
            .filter(|&y| (area.x + 1..area.right() - 1).any(|x| buffer[(x, y)].fg == mention))
            .count();
        assert!(rows_with_mention >= 2, "{rows_with_mention}");
        assert_eq!(app.input.text(), format!("hi @{name}"));
    }

    // ---- custom emojis in the draft (draft-emoji) ----

    use crate::app::emoji::{CustomEmojis, Graphics, ImageState};

    fn emoji_app(size: (u16, u16), graphics: Graphics, image: bool, draft: &str) -> App {
        let mut app = App::new(ratatui::layout::Size::new(size.0, size.1));
        app.graphics = graphics;
        app.custom_emojis = CustomEmojis::Loaded(vec![crate::data::CustomEmoji {
            name: "rtshipit".to_string(),
            aliases: vec!["rtship".to_string()],
            ext: "png".to_string(),
            ..Default::default()
        }]);
        if image {
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
        press(&mut app, "<C-w>ji");
        app.input.insert_str(draft);
        app
    }

    /// The rows of the input bar, as drawn.
    fn bar_rows(app: &App, size: (u16, u16)) -> String {
        let area = input_area(app);
        render_to_string(app, size.0, size.1)
            .lines()
            .skip(usize::from(area.y))
            .take(usize::from(area.height))
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_custom_emoji_in_the_draft_is_drawn_as_its_image() {
        let app = emoji_app((80, 24), Graphics::Supported, true, "ship it :rtshipit:");
        let buffer = render_buffer(&app, 80, 24);
        let area = input_area(&app);
        let cells: Vec<(u16, Color)> = (area.x..area.right())
            .map(|x| (x, &buffer[(x, area.y + 1)]))
            .filter(|(_, c)| c.symbol().starts_with(crate::kitty::PLACEHOLDER))
            .map(|(x, c)| (x, c.fg))
            .collect();
        // Two cells, side by side after "ship it " inside the border, carrying image id 9.
        assert_eq!(cells.len(), 2, "{}", bar_rows(&app, (80, 24)));
        assert_eq!(cells[0].0 + 1, cells[1].0);
        assert_eq!(cells[0].0, area.x + 1 + 8);
        assert_eq!(cells[0].1, Color::Rgb(0, 0, 9));
        // The hardware cursor is two columns past the text before it, where the drawing ends.
        assert_eq!(
            cursor_position(&app, area),
            Some(Position::new(area.x + 1 + 10, area.y + 1))
        );
        insta::assert_snapshot!(bar_rows(&app, (80, 24)));
    }

    #[test]
    fn an_alias_is_drawn_like_the_name() {
        let by_alias = emoji_app((80, 24), Graphics::Supported, true, "a :rtship:");
        let by_name = emoji_app((80, 24), Graphics::Supported, true, "a :rtshipit:");
        assert_eq!(bar_rows(&by_alias, (80, 24)), bar_rows(&by_name, (80, 24)));
    }

    #[test]
    fn a_custom_emoji_without_its_image_is_two_stand_in_cells() {
        let app = emoji_app((80, 24), Graphics::Supported, false, "ship it :rtshipit:");
        let rows = bar_rows(&app, (80, 24));
        assert!(!rows.contains("rtshipit"), "{rows}");
        insta::assert_snapshot!(rows);
    }

    #[test]
    fn a_terminal_without_images_draws_the_draft_as_written() {
        let app = emoji_app((80, 24), Graphics::Unsupported, false, "ship it :rtshipit:");
        let rows = bar_rows(&app, (80, 24));
        assert!(rows.contains("ship it :rtshipit:"), "{rows}");
        insta::assert_snapshot!(rows);
    }

    #[test]
    fn a_narrow_bar_wraps_by_the_drawn_width_and_keeps_an_emoji_whole() {
        let size = (50, 20);
        let app = emoji_app(
            size,
            Graphics::Supported,
            true,
            "abc,:rtshipit::rtshipit: and a longer line of text :rtshipit:",
        );
        let buffer = render_buffer(&app, size.0, size.1);
        let area = input_area(&app);
        // Every row fits inside the border and every emoji has both of its cells on the same row.
        for y in area.y + 1..area.bottom() - 1 {
            let n = (area.x..area.right())
                .filter(|&x| {
                    buffer[(x, y)]
                        .symbol()
                        .starts_with(crate::kitty::PLACEHOLDER)
                })
                .count();
            assert_eq!(n % 2, 0, "row {y}: {}", bar_rows(&app, size));
        }
        insta::assert_snapshot!(bar_rows(&app, size));
    }
}

//! The full-size image view: a bordered overlay over everything above the status row, with the
//! image of `app::image_view` as large as fits, centred, keeping its shape.
//!
//! The border and the text use the theme's existing focused-border and notice styles, so the view
//! adds nothing to the theme vocabulary. The cells are the same placeholder cells as the inline
//! boxes (`kitty::cell_at`), under the view's own image id, and the cells the terminal is told
//! about (`app::attachments::prepare_frame`) come from `placement`, the same function this draws
//! with.

use crate::app::App;
use crate::app::image_view::{self, Body};
use crate::kitty;
use crate::ui::attachment::fitted;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

/// The area the overlay covers: the whole screen above the status row.
pub fn area(app: &App) -> Rect {
    let status = app.areas().status;
    Rect::new(0, 0, app.size.width, status.y.min(app.size.height))
}

/// The room inside the overlay's border, where the image goes.
pub fn inner(app: &App) -> Rect {
    Block::default().borders(Borders::ALL).inner(area(app))
}

/// The cells (columns, rows) an image of `pixels` covers in the overlay: its shape, as large as
/// the inside allows. `None` when there is no room at all.
pub fn placement(app: &App, pixels: (u32, u32)) -> Option<(usize, usize)> {
    let inner = inner(app);
    if inner.width == 0 || inner.height == 0 {
        return None;
    }
    let boxed = (
        usize::from(inner.width).min(kitty::MAX_CELLS),
        usize::from(inner.height).min(kitty::MAX_CELLS),
    );
    Some(fitted(boxed, pixels))
}

/// Draw the view when it is open and its message still has an image.
pub fn render(frame: &mut Frame, app: &App) {
    let Some(content) = image_view::content(app) else {
        return;
    };
    let area = area(app);
    if area.width < 4 || area.height < 3 {
        return;
    }
    let theme = &app.theme;
    frame.render_widget(Clear, area);
    let (at, total) = content.position;
    let title = format!(" {} ({at}/{total}) ", content.name);
    let hint = if total > 1 {
        " h/l: other image   Esc: close "
    } else {
        " Esc: close "
    };
    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(theme.border_type_focused)
        .border_style(theme.border_focused)
        .title_style(theme.border_focused)
        .title(title);
    if usize::from(area.width) >= hint.width() + 4 {
        block = block
            .title_bottom(Line::from(Span::styled(hint, theme.messages_notice)).right_aligned());
    }
    let inner = block.inner(area);
    frame.render_widget(block, area);
    match content.body {
        Body::Image { image, uploaded } => {
            let (Some(id), Some((cols, rows)), true) = (
                app.attachment_images.view_id(),
                placement(app, (image.width, image.height)),
                uploaded,
            ) else {
                note(frame, inner, "loading image...", app);
                return;
            };
            let (r, g, b) = kitty::id_color(id + image.frame_at(app.anim_ms));
            let style = Style::default().fg(Color::Rgb(r, g, b));
            let x0 = inner.x
                + (inner
                    .width
                    .saturating_sub(u16::try_from(cols).unwrap_or(u16::MAX)))
                    / 2;
            let y0 = inner.y
                + (inner
                    .height
                    .saturating_sub(u16::try_from(rows).unwrap_or(u16::MAX)))
                    / 2;
            for row in 0..rows {
                for col in 0..cols {
                    let (Ok(dx), Ok(dy)) = (u16::try_from(col), u16::try_from(row)) else {
                        continue;
                    };
                    let (x, y) = (x0 + dx, y0 + dy);
                    if x < inner.right() && y < inner.bottom() {
                        frame
                            .buffer_mut()
                            .set_string(x, y, kitty::cell_at(row, col), style);
                    }
                }
            }
        }
        Body::Loading => note(frame, inner, "loading image...", app),
        Body::Failed => note(frame, inner, "the image could not be loaded", app),
    }
}

/// A short dim line in the middle of the overlay.
fn note(frame: &mut Frame, inner: Rect, text: &str, app: &App) {
    if inner.height == 0 {
        return;
    }
    let row = Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1);
    frame.render_widget(
        Paragraph::new(Line::styled(text.to_string(), app.theme.messages_notice)).centered(),
        row,
    );
}

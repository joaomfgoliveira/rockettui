//! Inline attachment images in the message flow: how many cells an image's box takes, and the rows
//! that draw it.
//!
//! The box is decided from the pixel size the server reports (or a default), the width of the
//! pane and `image_max_rows`, before a single byte is fetched, so a ready image fills rows that
//! were already there and nothing scrolls. Every function here is pure.

use crate::app::attachments::{AttachmentImages, ImageState, is_fetchable_image};
use crate::app::emoji::Graphics;
use crate::data::{Message, MessageKind};
use crate::kitty;
use ratatui::style::{Color, Style};
use ratatui::text::Span;
use unicode_width::UnicodeWidthStr;

/// The terminal cell is assumed twice as tall as it is wide, and 8 by 16 pixels: an image is never
/// drawn larger than its own pixels, and keeps its shape in a 1:2 grid.
/// (The real size is read from the terminal when it reports one: `set_cell_size`.)
const DEFAULT_CELL_W_PX: u32 = 8;
const DEFAULT_CELL_H_PX: u32 = 16;

/// The pixel size of one terminal cell, width in the high half. Kitty fits an image inside its
/// cells keeping its shape and centres it, so a box sized for another cell shape would show blank
/// bands above and below (or beside) the picture.
static CELL_PX: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new((DEFAULT_CELL_W_PX as u64) << 32 | DEFAULT_CELL_H_PX as u64);

/// Tell the layout the terminal's cell size in pixels (`window_pixels` over `columns` by `rows`).
/// A report with a zero anywhere (a terminal that does not give pixel sizes) keeps what was set.
pub fn set_cell_size(window_pixels: (u16, u16), columns: u16, rows: u16) {
    if let Some((w, h)) = cell_from(window_pixels, columns, rows) {
        CELL_PX.store(
            u64::from(w) << 32 | u64::from(h),
            std::sync::atomic::Ordering::Relaxed,
        );
    }
}

/// One cell's pixel size from the window's, `None` when any number is zero.
fn cell_from(window_pixels: (u16, u16), columns: u16, rows: u16) -> Option<(u32, u32)> {
    if window_pixels.0 == 0 || window_pixels.1 == 0 || columns == 0 || rows == 0 {
        return None;
    }
    Some((
        (u32::from(window_pixels.0) / u32::from(columns)).max(1),
        (u32::from(window_pixels.1) / u32::from(rows)).max(1),
    ))
}

/// The pixel size (width, height) of one cell.
fn cell_px() -> (u32, u32) {
    let v = CELL_PX.load(std::sync::atomic::Ordering::Relaxed);
    ((v >> 32) as u32, v as u32)
}
/// The box of an image whose size the server did not report: 4:3, 40 columns wide at most.
const DEFAULT_COLS: usize = 40;
const DEFAULT_ROWS: usize = 15;
/// What the box says while its image is on its way.
const LOADING: &str = "loading image...";

/// Everything the drawing of attachment images reads, in one `Copy` value. `App::row_ctx` is the
/// one place it is built for the app.
#[derive(Clone, Copy)]
pub struct AttachView<'a> {
    pub images: &'a AttachmentImages,
    pub graphics: Graphics,
    /// The `inline_images` setting.
    pub enabled: bool,
    /// The `image_max_rows` setting.
    pub max_rows: u16,
    /// Milliseconds since the program started, which picks the frame of an animated image. Rows
    /// never depend on it.
    pub now_ms: u64,
}

/// No attachment images: for a caller (a test, a list that never draws them) that has none.
pub static NO_ATTACH_IMAGES: std::sync::LazyLock<AttachmentImages> =
    std::sync::LazyLock::new(AttachmentImages::default);

impl AttachView<'static> {
    /// A view of nothing: images off, a terminal without graphics.
    pub fn none() -> Self {
        AttachView {
            images: &NO_ATTACH_IMAGES,
            graphics: Graphics::Unsupported,
            enabled: false,
            max_rows: crate::config::settings::DEFAULT_IMAGE_MAX_ROWS,
            now_ms: 0,
        }
    }
}

impl AttachView<'_> {
    /// True when attachment images are drawn at all: the terminal can show them and the user did
    /// not turn them off.
    pub fn active(&self) -> bool {
        self.graphics == Graphics::Supported && self.enabled
    }
}

/// The box, in cells (columns, rows), of an image of `dims` pixels in a pane with `width` columns
/// to draw in, at most `max_rows` tall. Never larger than the image's own pixels (at 8 by 16 a
/// cell), never wider than `width` or the mark table, and always at least one cell. Without
/// dimensions it is the default 4:3 box. `(0, 0)` when there is no column to draw in.
pub fn box_size(dims: Option<(u32, u32)>, width: usize, max_rows: u16) -> (usize, usize) {
    if width == 0 {
        return (0, 0);
    }
    let (cell_w, cell_h) = cell_px();
    let max_cols = width.min(kitty::MAX_CELLS);
    let max_rows = usize::from(max_rows).clamp(1, kitty::MAX_CELLS);
    let (nat_cols, nat_rows) = match dims {
        Some((w, h)) if w > 0 && h > 0 => (
            (w.div_ceil(cell_w) as usize).max(1),
            (h.div_ceil(cell_h) as usize).max(1),
        ),
        _ => (DEFAULT_COLS, DEFAULT_ROWS),
    };
    scale_into((nat_cols, nat_rows), (max_cols, max_rows))
}

/// `natural` scaled down, keeping its shape, to fit in `limit`; never up. At least 1 by 1.
fn scale_into(natural: (usize, usize), limit: (usize, usize)) -> (usize, usize) {
    let (nc, nr) = natural;
    if nc <= limit.0 && nr <= limit.1 {
        return (nc, nr);
    }
    // The smaller of the two ratios, compared as fractions so no float is needed.
    let by_cols = limit.0 * nr;
    let by_rows = limit.1 * nc;
    let (cols, rows) = if by_cols <= by_rows {
        (limit.0, (limit.0 * nr).div_ceil(nc).max(1))
    } else {
        ((limit.1 * nc).div_ceil(nr).max(1), limit.1)
    };
    (cols.clamp(1, limit.0), rows.clamp(1, limit.1))
}

/// The cells (columns, rows) a decoded image of `pixels` covers inside `boxed`: its shape in a 1:2
/// grid, as large as fits. The same as the box when the server reported the true shape.
pub fn fitted(boxed: (usize, usize), pixels: (u32, u32)) -> (usize, usize) {
    if pixels.0 == 0 || pixels.1 == 0 {
        return boxed;
    }
    let (cell_w, cell_h) = cell_px();
    let natural = (
        (pixels.0.div_ceil(cell_w) as usize).max(1),
        (pixels.1.div_ceil(cell_h) as usize).max(1),
    );
    // Enlarge as well as shrink: the box was sized from the reported shape, and this fills as much
    // of it as the real shape allows.
    let (nc, nr) = natural;
    let by_cols = boxed.0 * nr;
    let by_rows = boxed.1 * nc;
    let (cols, rows) = if by_cols <= by_rows {
        (boxed.0, (boxed.0 * nr).div_ceil(nc).max(1))
    } else {
        ((boxed.1 * nc).div_ceil(nr).max(1), boxed.1)
    };
    (cols.clamp(1, boxed.0.max(1)), rows.clamp(1, boxed.1.max(1)))
}

/// The rows, without any prefix, that draw the image attachments of `msg` in `width` columns: one
/// box per fetchable image, in the order the server lists them, under the caption and the text.
/// Empty unless images are drawn at all (`view.active`). A box waiting for its image is blank with
/// a dim notice; a failed one is not drawn; a ready one is its image's cells.
pub fn image_rows(
    msg: &Message,
    width: usize,
    view: &AttachView<'_>,
    notice: Style,
) -> Vec<Vec<Span<'static>>> {
    if !view.active() || msg.kind != MessageKind::Normal {
        return Vec::new();
    }
    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
    for attachment in msg.attachments.iter().filter(|a| is_fetchable_image(a)) {
        let Some(path) = attachment.path.as_deref() else {
            continue;
        };
        let state = view.images.state(path);
        if matches!(state, Some(ImageState::Failed)) {
            continue;
        }
        let boxed = box_size(attachment.dims, width, view.max_rows);
        if boxed.0 == 0 {
            continue;
        }
        match state {
            Some(ImageState::Ready {
                image,
                id,
                uploaded: true,
                ..
            }) => {
                let (cols, used_rows) = fitted(boxed, (image.width, image.height));
                let (r, g, b) = kitty::id_color(*id + image.frame_at(view.now_ms));
                // Reversed colours would make the terminal read the id from the wrong colour.
                let style = Style::default().fg(Color::Rgb(r, g, b));
                for row in 0..boxed.1 {
                    if row < used_rows {
                        let cells: String = (0..cols).map(|col| kitty::cell_at(row, col)).collect();
                        rows.push(vec![Span::styled(cells, style)]);
                    } else {
                        rows.push(Vec::new());
                    }
                }
            }
            _ => {
                for row in 0..boxed.1 {
                    if row == 0 {
                        let text = fit_text(LOADING, boxed.0);
                        rows.push(vec![Span::styled(text, notice)]);
                    } else {
                        rows.push(Vec::new());
                    }
                }
            }
        }
    }
    rows
}

/// `text` cut to at most `width` columns (it is plain ASCII).
fn fit_text(text: &str, width: usize) -> String {
    if text.width() <= width {
        text.to_string()
    } else {
        text.chars().take(width).collect()
    }
}

/// The ids of the image cells drawn in `buffer`: the foreground colour of each placeholder cell
/// that falls in the attachment id range. The run loop reads it after a draw to know which images
/// are on screen.
pub fn ids_on_screen(buffer: &ratatui::buffer::Buffer) -> std::collections::HashSet<u32> {
    let mut ids = std::collections::HashSet::new();
    for cell in buffer.content() {
        if !cell.symbol().starts_with(kitty::PLACEHOLDER) {
            continue;
        }
        if let Color::Rgb(r, g, b) = cell.fg {
            let id = u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b);
            if (kitty::IMAGE_ID_FIRST..=kitty::IMAGE_ID_LAST).contains(&id) {
                ids.insert(id);
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_cell_size_comes_from_the_window_and_ignores_a_report_of_zero() {
        assert_eq!(cell_from((1600, 800), 200, 50), Some((8, 16)));
        assert_eq!(cell_from((1800, 1100), 200, 50), Some((9, 22)));
        assert_eq!(cell_from((0, 800), 200, 50), None);
        assert_eq!(cell_from((1600, 800), 0, 50), None);
        assert_eq!(cell_from((1600, 800), 200, 0), None);
    }

    #[test]
    fn a_box_follows_the_shape_of_the_cell() {
        // Same logic with 9x22 cells: the image keeps its shape, so fewer rows are needed for the
        // same columns than at 8x16.
        let (cw, ch) = (9u32, 22u32);
        let natural = (1800u32.div_ceil(cw) as usize, 1200u32.div_ceil(ch) as usize);
        let boxed = scale_into(natural, (36, 12));
        let shape = boxed.1 as f64 * f64::from(ch) / (boxed.0 as f64 * f64::from(cw));
        assert!((shape - 1200.0 / 1800.0).abs() < 0.1, "{boxed:?}");
    }

    #[test]
    fn a_wide_image_is_limited_by_the_pane_width() {
        // 1600x400: natural 200 x 25 cells. In 60 columns it is 60 wide and keeps its shape.
        assert_eq!(box_size(Some((1600, 400)), 60, 12), (60, 8));
    }

    #[test]
    fn a_tall_image_is_limited_by_the_row_cap_and_its_width_follows() {
        // 600x1200: natural 75 x 75. At most 12 rows: 12 wide.
        assert_eq!(box_size(Some((600, 1200)), 80, 12), (12, 12));
        assert_eq!(box_size(Some((600, 1200)), 80, 20), (20, 20));
        // Wider than tall but still capped by rows: 800x600 is 100 x 38.
        assert_eq!(box_size(Some((800, 600)), 80, 12), (32, 12));
    }

    #[test]
    fn a_tiny_image_is_not_enlarged_and_is_at_least_one_cell() {
        assert_eq!(box_size(Some((16, 32)), 80, 12), (2, 2));
        assert_eq!(box_size(Some((1, 1)), 80, 12), (1, 1));
        assert_eq!(box_size(Some((100, 100)), 80, 12), (13, 7));
    }

    #[test]
    fn missing_or_zero_dimensions_get_the_default_box() {
        assert_eq!(
            box_size(None, 80, 12),
            (32, 12),
            "capped by the rows, 40x15 shape"
        );
        assert_eq!(box_size(None, 80, 40), (40, 15));
        assert_eq!(box_size(Some((0, 10)), 80, 40), (40, 15));
        assert_eq!(box_size(Some((10, 0)), 80, 40), (40, 15));
    }

    #[test]
    fn a_narrow_pane_shrinks_the_box_and_a_zero_width_has_none() {
        assert_eq!(box_size(Some((800, 600)), 10, 12), (10, 4));
        assert_eq!(box_size(Some((800, 600)), 1, 12), (1, 1));
        assert_eq!(box_size(Some((800, 600)), 0, 12), (0, 0));
    }

    #[test]
    fn the_box_never_exceeds_its_limits() {
        for (dims, width, rows) in [
            (Some((10_000, 10)), 300, 40),
            (Some((10, 10_000)), 5, 3),
            (Some((4096, 4096)), 500, 40),
            (None, 1000, 40),
            (Some((3, 7)), 2, 3),
        ] {
            let (c, r) = box_size(dims, width, rows);
            assert!(c >= 1 && r >= 1, "{dims:?}");
            assert!(c <= width.min(kitty::MAX_CELLS), "{dims:?} {c}");
            assert!(r <= usize::from(rows), "{dims:?} {r}");
        }
    }

    #[test]
    fn a_decoded_image_of_the_reported_shape_fills_the_box() {
        let boxed = box_size(Some((800, 600)), 80, 12);
        assert_eq!(fitted(boxed, (640, 480)), boxed);
    }

    #[test]
    fn an_image_of_another_shape_is_fitted_inside_the_box() {
        let boxed = (32, 12);
        // Much wider than the box: limited by the columns.
        let (c, r) = fitted(boxed, (1600, 200));
        assert_eq!(c, 32);
        assert!(r < 12);
        // Much taller: limited by the rows.
        let (c, r) = fitted(boxed, (100, 1000));
        assert_eq!(r, 12);
        assert!(c < 32);
        for pixels in [(1, 1), (3, 5000), (5000, 3), (640, 480)] {
            let (c, r) = fitted(boxed, pixels);
            assert!(c >= 1 && r >= 1 && c <= 32 && r <= 12, "{pixels:?}");
        }
        assert_eq!(fitted(boxed, (0, 0)), boxed);
    }

    #[test]
    fn ids_on_screen_reads_the_colour_of_placeholder_cells() {
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;
        let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 1));
        let (r, g, b) = kitty::id_color(kitty::IMAGE_ID_FIRST + 5);
        buffer[(0, 0)]
            .set_symbol(&kitty::cell_at(0, 0))
            .set_fg(Color::Rgb(r, g, b));
        // An emoji id (small) and an ordinary cell with a colour are not attachment images.
        buffer[(1, 0)]
            .set_symbol(&kitty::cell(0))
            .set_fg(Color::Rgb(0, 0, 9));
        buffer[(2, 0)].set_symbol("x").set_fg(Color::Rgb(r, g, b));
        let ids = ids_on_screen(&buffer);
        assert_eq!(ids.len(), 1);
        assert!(ids.contains(&(kitty::IMAGE_ID_FIRST + 5)));
    }

    // ---- drawing in the pane

    use crate::app::App;
    use crate::app::attachments::ImageState;
    use crate::data::{Attachment, AttachmentKind};
    use crate::emoji_image::Rgba;
    use crate::test_util::{app_80x24, render_buffer, sample_message};

    const PATH_A: &str = "/file-upload/a/one.png";
    const PATH_B: &str = "/file-upload/b/two.png";

    fn image_att(path: &str, dims: Option<(u32, u32)>) -> Attachment {
        Attachment {
            kind: AttachmentKind::Image,
            name: "photo.png".to_string(),
            path: Some(path.to_string()),
            full_path: None,
            mime: Some("image/png".to_string()),
            size: Some(1000),
            dims,
        }
    }

    /// An app on its first room with one message per list of attachments, each with the caption.
    fn app_with(lists: Vec<Vec<Attachment>>) -> App {
        let mut app = app_80x24();
        app.graphics = Graphics::Supported;
        let room = app.rooms[0].id.clone();
        let msgs = lists
            .into_iter()
            .enumerate()
            .map(|(i, attachments)| Message {
                room_id: room.clone(),
                body: format!("see {i} [attachment: photo.png]"),
                attachments,
                plain_text: false,
                ..sample_message(&format!("m{i}"))
            })
            .collect();
        app.messages.insert(room.clone(), msgs);
        app.open_room(room);
        app.effects.clear();
        app
    }

    fn ready(app: &mut App, path: &str, id: u32, w: u32, h: u32) {
        let image = Rgba {
            width: w,
            height: h,
            pixels: vec![0u8; (w * h * 4) as usize].into(),
            ..Default::default()
        };
        app.attachment_images.set_state(
            path,
            ImageState::Ready {
                image,
                dims: None,
                id,
                uploaded: true,
                placed: None,
            },
        );
    }

    fn screen(app: &App) -> Vec<String> {
        let buffer = render_buffer(app, 80, 24);
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    fn placeholder_cells(app: &App) -> usize {
        let buffer = render_buffer(app, 80, 24);
        buffer
            .content()
            .iter()
            .filter(|c| c.symbol().starts_with(kitty::PLACEHOLDER))
            .count()
    }

    fn row_of(app: &App, needle: &str) -> usize {
        screen(app)
            .iter()
            .position(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} not on screen"))
    }

    fn two_messages(dims: Option<(u32, u32)>) -> App {
        app_with(vec![vec![image_att(PATH_A, dims)], Vec::new()])
    }

    #[test]
    fn a_pending_image_reserves_its_box_with_a_dim_notice() {
        let app = two_messages(Some((800, 600)));
        let lines = screen(&app);
        let caption = row_of(&app, "see 0 [attachment: photo.png]");
        assert!(
            lines[caption + 1].contains("loading image..."),
            "{lines:#?}"
        );
        let (_, rows) = box_size(Some((800, 600)), 36, 12);
        // The next message starts right after the box.
        assert_eq!(row_of(&app, "see 1"), caption + 1 + rows);
        assert_eq!(
            placeholder_cells(&app),
            0,
            "nothing is drawn as an image yet"
        );
        // The notice is dim, not the body style.
        let buffer = render_buffer(&app, 80, 24);
        let x = lines[caption + 1]
            .find("loading")
            .map(|b| lines[caption + 1][..b].chars().count());
        let cell = &buffer[(x.unwrap() as u16, (caption + 1) as u16)];
        assert_eq!(
            cell.fg,
            app.theme.messages_notice.fg.unwrap_or(Color::Reset)
        );
    }

    #[test]
    fn a_ready_image_fills_the_same_rows_and_nothing_moves() {
        let mut app = two_messages(Some((800, 600)));
        let (waiting_next, waiting_caption) = (row_of(&app, "see 1"), row_of(&app, "see 0"));
        ready(&mut app, PATH_A, kitty::IMAGE_ID_FIRST + 3, 640, 480);
        assert_eq!(row_of(&app, "see 1"), waiting_next, "no row moved");
        assert_eq!(row_of(&app, "see 0"), waiting_caption);
        let (cols, rows) = box_size(Some((800, 600)), 36, 12);
        assert_eq!(placeholder_cells(&app), cols * rows);
        assert!(!screen(&app).join("\n").contains("loading image"));
        // The cells carry the id in their colour.
        let buffer = render_buffer(&app, 80, 24);
        let (r, g, b) = kitty::id_color(kitty::IMAGE_ID_FIRST + 3);
        let cells: Vec<_> = buffer
            .content()
            .iter()
            .filter(|c| c.symbol().starts_with(kitty::PLACEHOLDER))
            .collect();
        assert!(cells.iter().all(|c| c.fg == Color::Rgb(r, g, b)));
        assert_eq!(
            ids_on_screen(&buffer),
            [kitty::IMAGE_ID_FIRST + 3].into_iter().collect()
        );
    }

    #[test]
    fn an_image_of_another_shape_is_fitted_inside_the_reserved_rows() {
        let mut app = two_messages(Some((800, 600)));
        let next = row_of(&app, "see 1");
        // The server said 4:3, the bytes are a wide strip.
        ready(&mut app, PATH_A, kitty::IMAGE_ID_FIRST, 1600, 100);
        assert_eq!(row_of(&app, "see 1"), next, "the box keeps its rows");
        let (cols, rows) = box_size(Some((800, 600)), 36, 12);
        let (fit_cols, fit_rows) = fitted((cols, rows), (1600, 100));
        assert!(fit_rows < rows);
        assert_eq!(placeholder_cells(&app), fit_cols * fit_rows);
    }

    #[test]
    fn a_failed_image_removes_its_box_once() {
        let mut app = two_messages(Some((800, 600)));
        let waiting = row_of(&app, "see 1");
        app.attachment_images.set_state(PATH_A, ImageState::Failed);
        let caption = row_of(&app, "see 0");
        assert_eq!(
            row_of(&app, "see 1"),
            caption + 1,
            "only the caption remains"
        );
        assert!(row_of(&app, "see 1") < waiting);
        assert!(!screen(&app).join("\n").contains("loading image"));
        assert_eq!(placeholder_cells(&app), 0);
    }

    #[test]
    fn two_images_are_drawn_in_server_order_one_below_the_other() {
        let mut app = app_with(vec![vec![
            image_att(PATH_A, Some((80, 64))),
            image_att(PATH_B, Some((80, 96))),
        ]]);
        ready(&mut app, PATH_A, kitty::IMAGE_ID_FIRST + 1, 80, 64);
        ready(&mut app, PATH_B, kitty::IMAGE_ID_FIRST + 2, 80, 96);
        let (c1, r1) = box_size(Some((80, 64)), 36, 12);
        let (c2, r2) = box_size(Some((80, 96)), 36, 12);
        assert_eq!(placeholder_cells(&app), c1 * r1 + c2 * r2);
        let buffer = render_buffer(&app, 80, 24);
        let first_row_of = |id: u32| {
            (0..buffer.area.height).find(|&y| {
                (0..buffer.area.width).any(|x| {
                    let c = &buffer[(x, y)];
                    c.symbol().starts_with(kitty::PLACEHOLDER)
                        && c.fg
                            == Color::Rgb(
                                kitty::id_color(id).0,
                                kitty::id_color(id).1,
                                kitty::id_color(id).2,
                            )
                })
            })
        };
        let (a, b) = (
            first_row_of(kitty::IMAGE_ID_FIRST + 1).expect("first"),
            first_row_of(kitty::IMAGE_ID_FIRST + 2).expect("second"),
        );
        assert_eq!(
            b,
            a + r1 as u16,
            "the second box starts where the first ends"
        );
    }

    #[test]
    fn the_cursor_highlight_covers_the_image_rows_without_reversing_the_image() {
        let mut app = two_messages(Some((800, 600)));
        ready(&mut app, PATH_A, kitty::IMAGE_ID_FIRST + 7, 640, 480);
        // The cursor is on the last message; move it up to the one with the image.
        crate::test_util::press(&mut app, "gg");
        let buffer = render_buffer(&app, 80, 24);
        let caption = row_of(&app, "see 0");
        let text_x = screen(&app)[caption]
            .find("see 0")
            .map(|b| screen(&app)[caption][..b].chars().count())
            .unwrap();
        assert!(
            buffer[(text_x as u16, caption as u16)]
                .modifier
                .contains(ratatui::style::Modifier::REVERSED),
            "the caption is highlighted"
        );
        let mut seen = 0;
        for cell in buffer.content() {
            if cell.symbol().starts_with(kitty::PLACEHOLDER) {
                seen += 1;
                assert!(!cell.modifier.contains(ratatui::style::Modifier::REVERSED));
                assert_eq!(
                    cell.fg,
                    Color::Rgb(
                        kitty::id_color(kitty::IMAGE_ID_FIRST + 7).0,
                        kitty::id_color(kitty::IMAGE_ID_FIRST + 7).1,
                        kitty::id_color(kitty::IMAGE_ID_FIRST + 7).2
                    )
                );
            }
        }
        assert!(seen > 0);
        // A blank row of the box, past the image, is highlighted too.
        let (_, rows) = box_size(Some((800, 600)), 36, 12);
        assert!(rows >= 2);
    }

    #[test]
    fn a_terminal_without_images_or_the_setting_off_shows_the_caption_only() {
        let plain = {
            let mut app = app_with(vec![Vec::new(), Vec::new()]);
            app.graphics = Graphics::Unsupported;
            screen(&app)
        };
        for tweak in [0, 1] {
            let mut app = two_messages(Some((800, 600)));
            if tweak == 0 {
                app.graphics = Graphics::Unsupported;
            } else {
                app.settings.inline_images = false;
            }
            ready(&mut app, PATH_A, kitty::IMAGE_ID_FIRST, 640, 480);
            let lines = screen(&app);
            assert_eq!(placeholder_cells(&app), 0);
            assert!(!lines.join("\n").contains("loading image"));
            // The same rows as the same messages with no attachment at all.
            assert_eq!(
                row_of(&app, "see 1"),
                plain.iter().position(|l| l.contains("see 1")).unwrap()
            );
            assert!(app.effects.is_empty());
        }
    }

    #[test]
    fn only_the_room_pane_draws_boxes() {
        let app = two_messages(Some((800, 600)));
        let msgs = app.messages[&app.rooms[0].id].clone();
        assert!(!app.row_ctx(&msgs).attach.active(), "results, thread pane");
        assert!(app.pane_row_ctx(&app.rooms[0].id, &msgs).attach.active());
    }

    #[test]
    fn server_text_cannot_draw_an_image() {
        let mut app = app_with(vec![vec![Attachment {
            kind: AttachmentKind::Other,
            name: "x".to_string(),
            path: None,
            full_path: None,
            mime: None,
            size: None,
            dims: None,
        }]]);
        let room = app.rooms[0].id.clone();
        let cell = format!("{}\u{305}\u{30d}", kitty::PLACEHOLDER);
        let hostile = format!(
            "evil {cell}{cell} \u{1b}_Ga=T,f=32,s=1,v=1,i=1,U=1,c=2,r=1;AAAA\u{1b}\\ [attachment: {cell}]"
        );
        app.messages.get_mut(&room).unwrap()[0].body = hostile;
        assert_eq!(placeholder_cells(&app), 0);
        assert!(!screen(&app).join("\n").contains('\u{1b}'));
    }

    #[test]
    fn the_frame_shown_follows_the_clock_and_no_row_moves() {
        let mut app = two_messages(Some((800, 600)));
        let frames = 3usize;
        let frame = |n: usize| crate::emoji_image::Frame {
            pixels: vec![n as u8; 640 * 480 * 4].into(),
            delay_ms: 100,
        };
        let image = Rgba {
            width: 640,
            height: 480,
            pixels: vec![0u8; 640 * 480 * 4].into(),
            delay_ms: 100,
            later: (1..frames).map(frame).collect::<Vec<_>>().into(),
        };
        let first = kitty::IMAGE_ID_FIRST + 10;
        app.attachment_images.set_state(
            PATH_A,
            ImageState::Ready {
                image,
                dims: None,
                id: first,
                uploaded: true,
                placed: None,
            },
        );
        let mut seen = Vec::new();
        let mut layout = None;
        for (ms, want) in [
            (0u64, 0u32),
            (99, 0),
            (100, 1),
            (250, 2),
            (300, 0),
            (1_000_000_200, 0),
        ] {
            app.anim_ms = ms;
            let lines = screen(&app);
            // Rows of text and cells do not depend on the frame.
            let shape = (
                row_of(&app, "see 0"),
                row_of(&app, "see 1"),
                placeholder_cells(&app),
            );
            assert_eq!(*layout.get_or_insert(shape), shape, "ms {ms}: {lines:#?}");
            let buffer = render_buffer(&app, 80, 24);
            assert_eq!(
                ids_on_screen(&buffer),
                [first + want].into_iter().collect(),
                "ms {ms}"
            );
            seen.push(want);
        }
        assert!(seen.contains(&1) && seen.contains(&2));
    }

    #[test]
    fn a_still_image_never_changes_id() {
        let mut app = two_messages(Some((800, 600)));
        ready(&mut app, PATH_A, kitty::IMAGE_ID_FIRST + 4, 640, 480);
        for ms in [0u64, 100, 12_345] {
            app.anim_ms = ms;
            assert_eq!(
                ids_on_screen(&render_buffer(&app, 80, 24)),
                [kitty::IMAGE_ID_FIRST + 4].into_iter().collect()
            );
        }
    }
}

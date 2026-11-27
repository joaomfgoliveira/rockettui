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
pub(crate) fn cell_px() -> (u32, u32) {
    let v = CELL_PX.load(std::sync::atomic::Ordering::Relaxed);
    ((v >> 32) as u32, v as u32)
}
/// The box of an image whose size the server did not report: 4:3, 40 columns wide at most.
const DEFAULT_COLS: usize = 40;
const DEFAULT_ROWS: usize = 15;
/// What the box says while its image is on its way.
const LOADING: &str = "loading image...";

/// What the box of a video says before its poster is there, with the video's size when the server
/// reports one and the key that loads it when one is bound.
const VIDEO_LOADING: &str = "loading video...";

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
    /// The `inline_video` setting, where video boxes are drawn at all (the open room's pane).
    pub video_enabled: bool,
    /// The live keymap, which names the key a video placeholder tells to press.
    pub keymap: Option<&'a crate::input::keymap::Keymap>,
    /// The video that plays or is paused: its frames and its status row.
    pub player: Option<&'a crate::app::video::Player>,
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
            video_enabled: false,
            keymap: None,
            player: None,
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

impl AttachView<'_> {
    /// True when video boxes are drawn: the terminal can show images and `inline_video` is on.
    pub fn video_active(&self) -> bool {
        self.graphics == Graphics::Supported && self.video_enabled
    }
}

/// The box of `attachment`: an image from the size the server reports, a video from the fixed
/// 16:9 shape (the server reports no video size).
pub fn box_of(attachment: &crate::data::Attachment, width: usize, max_rows: u16) -> (usize, usize) {
    if attachment.kind == crate::data::AttachmentKind::Video {
        box_size(Some(crate::app::video::BOX_DIMS), width, max_rows)
    } else {
        box_size(attachment.dims, width, max_rows)
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

/// The rows, without any prefix, that draw the video boxes of `msg` in `width` columns: one box
/// per fetchable video, each followed by one row for its status line (blank unless the video is
/// active), so a box never changes its row count. Empty unless video boxes are drawn at all
/// (`view.video_active`). A box waiting for its poster says so (and, when nothing was asked for
/// yet, which key loads it); a failed one is not drawn; a ready one is its poster's cells.
pub fn video_rows(
    msg: &Message,
    width: usize,
    view: &AttachView<'_>,
    notice: Style,
) -> Vec<Vec<Span<'static>>> {
    use crate::app::video::is_fetchable_video;
    if !view.video_active() || msg.kind != MessageKind::Normal {
        return Vec::new();
    }
    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
    for attachment in msg.attachments.iter().filter(|a| is_fetchable_video(a)) {
        let Some(path) = attachment.path.as_deref() else {
            continue;
        };
        let state = view.images.state(path);
        if matches!(state, Some(ImageState::Failed)) {
            continue;
        }
        let boxed = box_of(attachment, width, view.max_rows);
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
                // A playing video's cells carry the id of the frame the terminal shows now; the
                // poster's id shows until the first frame is sent and again once it stops.
                // While the overlay plays it, the box keeps its poster.
                let playing = view
                    .player
                    .filter(|p| p.video.path == path && !p.fullscreen);
                let frame = playing.and_then(|p| Some((p, p.shown?)));
                let (size, id) = match frame {
                    Some((player, shown)) => (player.frame_size, player.ids[shown]),
                    None => ((image.width, image.height), *id),
                };
                let (cols, used_rows) = fitted(boxed, size);
                let (r, g, b) = kitty::id_color(id);
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
                let poster_here = matches!(state, Some(ImageState::Ready { .. }));
                let text = video_placeholder(
                    attachment.size,
                    view.images.is_requested(path) || poster_here,
                    view.keymap.and_then(crate::app::video::play_key).as_deref(),
                );
                // Without a poster the placeholder is one line: an empty box of a video that
                // may never be loaded would only be a gap. A poster that is in memory but not
                // in the terminal yet already has its full box, so the rows move once.
                let box_rows = if poster_here { boxed.1 } else { 1 };
                for row in 0..box_rows {
                    if row == 0 {
                        rows.push(vec![Span::styled(fit_text(&text, boxed.0), notice)]);
                    } else {
                        rows.push(Vec::new());
                    }
                }
                if !poster_here {
                    continue;
                }
            }
        }
        // The status row: blank until the video is active, so activating it moves no row.
        match view
            .player
            .filter(|p| p.video.path == path && !p.fullscreen)
        {
            Some(player) => {
                let text = status_text(player, view.keymap, boxed.0);
                rows.push(vec![Span::styled(text, notice)]);
            }
            // A poster that is showing says what a press does; a placeholder says it in the box.
            None if matches!(state, Some(ImageState::Ready { uploaded: true, .. })) => {
                let key = view.keymap.and_then(crate::app::video::play_key);
                let text = match key {
                    Some(key) => format!("video \u{b7} {key} to play"),
                    None => "video".to_string(),
                };
                rows.push(vec![Span::styled(fit_text_width(&text, boxed.0), notice)]);
            }
            None => rows.push(Vec::new()),
        }
    }
    rows
}

/// A length as `m:ss`, or `h:mm:ss` from an hour on.
fn clock(at: std::time::Duration) -> String {
    let secs = at.as_secs();
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `text` cut to at most `width` columns, not padded.
fn fit_text_width(text: &str, width: usize) -> String {
    fixed_width(text, width).trim_end().to_string()
}

/// `text` cut to at most `width` columns and padded with spaces to exactly that, so the row has
/// the same width whatever the digits are.
fn fixed_width(text: &str, width: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for c in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.extend(std::iter::repeat_n(' ', width - used));
    out
}

/// The status row of the active video: its position and length (`0:12 / 0:48`, `--:--` when the
/// length is unknown) and the keys that pause or resume it and stop it. The key names come from
/// the user's keymap, so they pass through the sanitizer; the row is exactly `width` columns.
pub fn status_text(
    player: &crate::app::video::Player,
    keymap: Option<&crate::input::keymap::Keymap>,
    width: usize,
) -> String {
    use crate::action::Action;
    let position = clock(player.position());
    let length = player.duration.map_or_else(|| "--:--".to_string(), clock);
    let key = |action: Action| {
        keymap
            .and_then(|keymap| crate::app::video::video_key(keymap, &action))
            .map(|key| format!("  {key}"))
    };
    let mut text = format!("{position} / {length}");
    if player.ended {
        text.push_str(" ended");
    }
    if let Some(key) = key(Action::VideoPause) {
        text.push_str(&format!(
            "{key} {}",
            if player.paused {
                "\u{25b6}"
            } else {
                "\u{25ae}\u{25ae}"
            }
        ));
    }
    if let Some(key) = key(Action::VideoStop) {
        text.push_str(&format!("{key} \u{25a0}"));
    }
    // The rest of the player's keys: the seeks (the short and the far one) and the fullscreen key.
    let pair = |a: Action, b: Action, label: String| {
        let keys: Vec<String> = [a, b]
            .into_iter()
            .filter_map(|action| {
                keymap.and_then(|keymap| crate::app::video::video_key(keymap, &action))
            })
            .collect();
        (!keys.is_empty()).then(|| format!("  {} {label}", keys.join("/")))
    };
    // Short labels (the seconds each seeks), so the row fits a narrow box.
    if let Some(seek) = pair(
        Action::VideoSeekBack,
        Action::VideoSeekForward,
        format!("{}s", crate::video::SEEK_SECS),
    ) {
        text.push_str(&seek);
    }
    if let Some(seek) = pair(
        Action::VideoSeekBackFar,
        Action::VideoSeekForwardFar,
        format!("{}s", crate::video::SEEK_FAR_SECS),
    ) {
        text.push_str(&seek);
    }
    if let Some(key) = keymap.and_then(crate::app::video::fullscreen_key) {
        text.push_str(&format!(
            "  {key} {}",
            if player.fullscreen { "close" } else { "full" }
        ));
    }
    let text = crate::ui::sanitize::sanitize_line(&text);
    fixed_width(&text, width)
}

/// The text of a video placeholder: `[video 40 MB: gv to load]` before anything was asked for,
/// `loading video...` while the poster is on its way. Plain ASCII built from the size and the
/// bound key, never from server text.
fn video_placeholder(size: Option<u64>, asked: bool, key: Option<&str>) -> String {
    if asked {
        return VIDEO_LOADING.to_string();
    }
    let size = size.map(|n| format!(" {}", crate::api::map::human_size(n)));
    let size = size.as_deref().unwrap_or("");
    match key {
        Some(key) => format!("[video{size}: {key} to load]"),
        None => format!("[video{size}]"),
    }
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
    // ---- video boxes -----------------------------------------------------------------------

    const VIDEO_A: &str = "/file-upload/v/one.mp4";

    fn video_att(path: &str, size: Option<u64>) -> Attachment {
        crate::app::video::tests::video_attachment(path, size)
    }

    fn two_videos(size: Option<u64>) -> App {
        app_with(vec![vec![video_att(VIDEO_A, size)], Vec::new()])
    }

    fn poster_ready(app: &mut App, path: &str, id: u32, w: u32, h: u32) {
        ready(app, path, id, w, h);
    }

    /// The box of the first video in the room, from the width the pane really draws in.
    fn pane_video_box(app: &App) -> (usize, usize) {
        let room = app.rooms[0].id.clone();
        let msgs = app.messages[&room].clone();
        let message = msgs
            .iter()
            .find(|m| {
                m.attachments
                    .iter()
                    .any(|a| a.kind == AttachmentKind::Video)
            })
            .expect("a message with a video");
        let ctx = app.pane_row_ctx(&room, &msgs);
        let body = crate::ui::wrap::body_width(
            message,
            usize::from(crate::app::messages::viewport(app).0),
            &ctx,
        );
        box_of(&message.attachments[0], body, app.settings.image_max_rows)
    }

    /// The rows a video box takes in this pane: the box and its status row.
    fn video_rows_in_pane(app: &App) -> usize {
        pane_video_box(app).1 + 1
    }

    #[test]
    fn a_video_box_has_a_fixed_shape_from_the_pane_and_the_row_cap_only() {
        let wide = box_of(&video_att(VIDEO_A, Some(1000)), 80, 12);
        assert_eq!(wide, box_size(Some((1280, 720)), 80, 12));
        // Whatever the server says about a video's pixels, the shape is 16:9.
        let mut odd = video_att(VIDEO_A, Some(1000));
        odd.dims = Some((100, 5000));
        assert_eq!(box_of(&odd, 80, 12), wide);
        // And an image keeps its own.
        assert_eq!(
            box_of(&image_att(PATH_A, Some((800, 600))), 80, 12),
            box_size(Some((800, 600)), 80, 12)
        );
        assert!(wide.0 > wide.1, "wider than tall: {wide:?}");
        assert_eq!(box_of(&video_att(VIDEO_A, None), 0, 12), (0, 0));
    }

    #[test]
    fn a_large_video_shows_a_one_line_placeholder_with_size_and_key() {
        let app = two_videos(Some(40 * 1024 * 1024));
        let lines = screen(&app);
        let caption = row_of(&app, "see 0 [attachment: photo.png]");
        assert!(
            lines[caption + 1].contains("[video 40.0 MB: gv to load]"),
            "{lines:#?}"
        );
        // No empty box: the next message follows the one line.
        assert_eq!(row_of(&app, "see 1"), caption + 2);
        assert_eq!(placeholder_cells(&app), 0);
        // The notice is dim.
        let buffer = render_buffer(&app, 80, 24);
        let x = lines[caption + 1]
            .find("[video")
            .map(|b| lines[caption + 1][..b].chars().count())
            .unwrap();
        let cell = &buffer[(x as u16, (caption + 1) as u16)];
        assert_eq!(
            cell.fg,
            app.theme.messages_notice.fg.unwrap_or(Color::Reset)
        );
    }

    #[test]
    fn the_placeholder_follows_the_live_key_and_the_reported_size() {
        let text = |size, asked, key| video_placeholder(size, asked, key);
        assert_eq!(
            text(Some(1536), false, Some("gv")),
            "[video 1.5 KB: gv to load]"
        );
        assert_eq!(text(None, false, Some("gv")), "[video: gv to load]");
        assert_eq!(
            text(Some(1536), false, None),
            "[video 1.5 KB]",
            "unbound: no hint"
        );
        assert_eq!(text(Some(1536), true, Some("gv")), "loading video...");
        // A rebound key is the one named.
        let mut app = two_videos(Some(40 * 1024 * 1024));
        app.keymap = crate::test_util::app_with_config(
            crate::test_util::config_from_keymap(
                "[normal]\n\"gv\" = \"none\"\n\"V\" = \"play_video\"\n",
            ),
            80,
            24,
        )
        .keymap;
        assert!(
            screen(&app)
                .join("\n")
                .contains("[video 40.0 MB: V to load]")
        );
        app.keymap = crate::test_util::app_with_config(
            crate::test_util::config_from_keymap("[normal]\n\"gv\" = \"none\"\n"),
            80,
            24,
        )
        .keymap;
        let text = screen(&app).join("\n");
        assert!(
            text.contains("[video 40.0 MB]") && !text.contains("to load"),
            "{text}"
        );
    }

    #[test]
    fn a_video_whose_poster_is_on_its_way_says_so() {
        let mut app = two_videos(Some(1000));
        app.attachment_images.queue_video(
            crate::app::video::reference(&video_att(VIDEO_A, Some(1000)), "ffmpeg").unwrap(),
            false,
        );
        assert!(screen(&app).join("\n").contains("loading video..."));
    }

    #[test]
    fn a_ready_poster_fills_the_same_rows_and_nothing_moves() {
        let mut app = two_videos(Some(1000));
        let caption = row_of(&app, "see 0");
        poster_ready(&mut app, VIDEO_A, kitty::IMAGE_ID_FIRST + 5, 480, 270);
        let next = caption + 1 + video_rows_in_pane(&app);
        assert_eq!(row_of(&app, "see 1"), next, "the box and its status row");
        assert_eq!(row_of(&app, "see 0"), caption);
        let boxed = pane_video_box(&app);
        assert_eq!(placeholder_cells(&app), boxed.0 * boxed.1);
        assert!(!screen(&app).join("\n").contains("[video"));
        let buffer = render_buffer(&app, 80, 24);
        let (r, g, b) = kitty::id_color(kitty::IMAGE_ID_FIRST + 5);
        assert!(
            buffer
                .content()
                .iter()
                .filter(|c| c.symbol().starts_with(kitty::PLACEHOLDER))
                .all(|c| c.fg == Color::Rgb(r, g, b))
        );
        // A poster of another shape is fitted inside the same rows.
        let mut tall = two_videos(Some(1000));
        poster_ready(&mut tall, VIDEO_A, kitty::IMAGE_ID_FIRST, 100, 400);
        assert_eq!(row_of(&tall, "see 1"), next);
        let (c, r) = fitted(boxed, (100, 400));
        assert_eq!(placeholder_cells(&tall), c * r);
    }

    #[test]
    fn a_failed_poster_leaves_the_caption_only() {
        let mut app = two_videos(Some(1000));
        app.attachment_images.set_state(VIDEO_A, ImageState::Failed);
        let caption = row_of(&app, "see 0");
        assert_eq!(row_of(&app, "see 1"), caption + 1);
        assert!(!screen(&app).join("\n").contains("[video"));
        assert_eq!(placeholder_cells(&app), 0);
    }

    #[test]
    fn the_rows_move_once_when_the_poster_arrives_and_not_again_when_it_is_uploaded() {
        let size = Some(40 * 1024 * 1024);
        let one_line = row_of(&two_videos(size), "see 1");
        let mut loading = two_videos(size);
        loading.attachment_images.queue_video(
            crate::app::video::reference(&video_att(VIDEO_A, size), "ffmpeg").unwrap(),
            true,
        );
        assert_eq!(
            row_of(&loading, "see 1"),
            one_line,
            "loading is one line too"
        );
        // The poster is in memory but not yet in the terminal: the full box is already there.
        let mut in_memory = two_videos(size);
        in_memory.attachment_images.set_state(
            VIDEO_A,
            ImageState::Ready {
                image: Rgba {
                    width: 480,
                    height: 270,
                    pixels: vec![0u8; 480 * 270 * 4].into(),
                    ..Default::default()
                },
                dims: None,
                id: kitty::IMAGE_ID_FIRST,
                uploaded: false,
                placed: None,
            },
        );
        let full = row_of(&in_memory, "see 1");
        assert!(full > one_line + 1);
        let mut shown = two_videos(size);
        poster_ready(&mut shown, VIDEO_A, kitty::IMAGE_ID_FIRST, 480, 270);
        assert_eq!(row_of(&shown, "see 1"), full, "uploading moves nothing");
        // Playing moves nothing either.
        let (playing, _dir, _pid) = crate::app::video::tests::playing("rows-once");
        let idle = crate::app::video::tests::ready_app();
        let next = |app: &App| row_of(app, "see 1");
        assert_eq!(next(&playing), next(&idle));
    }

    #[test]
    fn a_terminal_without_images_or_the_setting_off_shows_the_video_caption_only() {
        let plain = screen(&{
            let mut app = app_with(vec![Vec::new(), Vec::new()]);
            app.graphics = Graphics::Unsupported;
            app
        });
        let plain_next = plain.iter().position(|l| l.contains("see 1")).unwrap();
        for tweak in [0, 1] {
            let mut app = two_videos(Some(1000));
            if tweak == 0 {
                app.graphics = Graphics::Unsupported;
            } else {
                app.settings.inline_video = false;
            }
            poster_ready(&mut app, VIDEO_A, kitty::IMAGE_ID_FIRST, 480, 270);
            let text = screen(&app).join("\n");
            assert_eq!(placeholder_cells(&app), 0);
            assert!(!text.contains("[video") && !text.contains("loading video"));
            assert_eq!(row_of(&app, "see 1"), plain_next);
        }
        // `inline_images` is the images' setting only.
        let mut app = two_videos(Some(40 * 1024 * 1024));
        app.settings.inline_images = false;
        assert!(screen(&app).join("\n").contains("[video 40.0 MB"));
    }

    #[test]
    fn only_the_room_pane_draws_video_boxes() {
        let app = two_videos(Some(1000));
        let msgs = app.messages[&app.rooms[0].id].clone();
        assert!(
            !app.row_ctx(&msgs).attach.video_active(),
            "results, thread pane"
        );
        assert!(
            app.pane_row_ctx(&app.rooms[0].id, &msgs)
                .attach
                .video_active()
        );
        // And the thread, results and the rows of the model agree: no video rows outside the pane.
        let ctx = app.row_ctx(&msgs);
        assert!(video_rows(&msgs[0], 36, &ctx.attach, Style::default()).is_empty());
    }

    #[test]
    fn a_video_that_cannot_be_fetched_has_no_box() {
        let mut no_path = video_att(VIDEO_A, Some(1000));
        no_path.path = None;
        let mut odd = video_att(VIDEO_A, Some(1000));
        odd.mime = Some("application/x-unknown".to_string());
        for attachment in [no_path, odd] {
            let app = app_with(vec![vec![attachment], Vec::new()]);
            let caption = row_of(&app, "see 0");
            assert_eq!(row_of(&app, "see 1"), caption + 1);
        }
    }

    #[test]
    fn server_text_cannot_draw_a_video_box() {
        let mut attachment = video_att(VIDEO_A, Some(1000));
        let cell = format!("{}\u{305}\u{30d}", kitty::PLACEHOLDER);
        attachment.name = format!("{cell}\u{1b}_Ga=T;AAAA\u{1b}\\{cell}.mp4");
        attachment.mime = Some(format!("video/mp4; {cell}"));
        let app = app_with(vec![vec![attachment]]);
        assert_eq!(placeholder_cells(&app), 0);
        assert!(!screen(&app).join("\n").contains('\u{1b}'));
    }
    // ---- the status row --------------------------------------------------------------------

    /// The row under the box of the playing video on the 80 by 24 screen.
    fn status_row(app: &App) -> String {
        let lines = screen(app);
        let caption = lines
            .iter()
            .position(|l| l.contains("see 1"))
            .expect("the video message");
        let boxed = pane_video_box(app);
        // The pane's columns, without the sidebar and the borders.
        let row = &lines[caption + 1 + boxed.1];
        row.chars().skip(28 + 1 + 13).take(boxed.0).collect()
    }

    fn set_position(app: &mut App, start_secs: u64) {
        let player = app.video.as_mut().unwrap();
        player.start_secs = start_secs;
        player.index = 0;
    }

    #[test]
    fn the_status_row_shows_position_length_and_the_keys() {
        let (mut app, _dir, _pid) = crate::app::video::tests::playing("status-playing");
        set_position(&mut app, 12);
        let row = status_row(&app);
        assert!(row.starts_with("0:12 / 0:48  Space ▮▮  q ■"), "{row:?}");
        // Fixed width: the row is as wide as the box whatever the digits.
        assert_eq!(row.chars().count(), pane_video_box(&app).0);
        set_position(&mut app, 3);
        assert_eq!(status_row(&app).chars().count(), row.chars().count());
        assert!(status_row(&app).starts_with("0:03 / 0:48"));
    }

    #[test]
    fn a_paused_video_offers_to_resume() {
        let (mut app, _dir, _pid) = crate::app::video::tests::playing("status-paused");
        set_position(&mut app, 12);
        app.video.as_mut().unwrap().paused = true;
        let row = status_row(&app);
        assert!(row.starts_with("0:12 / 0:48  Space ▶  q ■"), "{row:?}");
    }

    #[test]
    fn an_unknown_length_shows_dashes() {
        let (mut app, _dir, _pid) = crate::app::video::tests::playing("status-unknown");
        app.video.as_mut().unwrap().duration = None;
        set_position(&mut app, 75);
        let row = status_row(&app);
        assert!(row.starts_with("1:15 / --:--  Space ▮▮"), "{row:?}");
        // An hour or more reads h:mm:ss.
        assert_eq!(clock(std::time::Duration::from_secs(3725)), "1:02:05");
        assert_eq!(clock(std::time::Duration::from_secs(59)), "0:59");
    }

    #[test]
    fn the_status_row_names_the_live_keys_and_leaves_out_an_unbound_action() {
        let (mut app, _dir, _pid) = crate::app::video::tests::playing("status-keys");
        app.keymap = crate::test_util::app_with_config(
            crate::test_util::config_from_keymap(
                "[video]\n\"s\" = \"video_pause\"\n\"<Space>\" = \"none\"\n\"<Esc>\" = \"none\"\nq = \"none\"\n\"x\" = \"video_stop\"\n",
            ),
            80,
            24,
        )
        .keymap;
        let row = status_row(&app);
        assert!(row.contains("s ▮▮") && row.contains("x ■"), "{row:?}");
        assert!(!row.contains("Space"), "{row:?}");
    }

    #[test]
    fn a_narrow_pane_cuts_the_status_row_to_the_box() {
        assert_eq!(fixed_width("0:12 / 0:48  Space ▮▮  q ■", 10), "0:12 / 0:4");
        assert_eq!(fixed_width("ab", 5), "ab   ");
        assert_eq!(fixed_width("", 0), "");
        // A wide character is not cut in half.
        assert_eq!(fixed_width("a\u{3042}b", 2), "a ");
        let (mut app, _dir, _pid) = crate::app::video::tests::playing("status-narrow");
        app.size = ratatui::layout::Size::new(50, 24);
        let lines: Vec<String> = {
            let buffer = render_buffer(&app, 50, 24);
            (0..buffer.area.height)
                .map(|y| {
                    (0..buffer.area.width)
                        .map(|x| buffer[(x, y)].symbol().to_string())
                        .collect::<String>()
                })
                .collect()
        };
        // The box is only 10 columns here, and the status row is cut to it.
        assert!(lines.iter().any(|l| l.contains("0:00 / 0:4")), "{lines:#?}");
        assert!(
            !lines.iter().any(|l| l.contains("0:00 / 0:48")),
            "{lines:#?}"
        );
        for line in &lines {
            assert!(
                line.chars().count() <= 50 + 60,
                "no row grows wider than the screen"
            );
        }
    }

    #[test]
    fn the_status_row_is_blank_until_the_video_is_active_and_moves_no_row() {
        let idle = crate::app::video::tests::ready_app();
        let next = |app: &App| {
            screen(app)
                .iter()
                .position(|l| l.contains("see 1"))
                .unwrap()
        };
        let before = next(&idle);
        assert!(status_row(&idle).trim().is_empty());
        let (mut app, _dir, _pid) = crate::app::video::tests::playing("status-moves");
        assert_eq!(next(&app), before, "starting moves no row");
        set_position(&mut app, 12);
        assert_eq!(next(&app), before);
        app.video.as_mut().unwrap().paused = true;
        assert_eq!(next(&app), before, "pausing moves no row");
        crate::app::video::stop(&mut app);
        assert_eq!(next(&app), before, "stopping moves no row");
        assert!(status_row(&app).trim().is_empty());
    }

    #[test]
    fn key_names_from_the_keymap_cannot_draw_an_image_or_an_escape() {
        let (mut app, _dir, _pid) = crate::app::video::tests::playing("status-hostile");
        let cell = kitty::PLACEHOLDER;
        app.keymap = crate::test_util::app_with_config(
            crate::test_util::config_from_keymap(&format!(
                "[video]\n\"{cell}\" = \"video_pause\"\n\"<Space>\" = \"none\"\n"
            )),
            80,
            24,
        )
        .keymap;
        // A key spelled as a placeholder character is not drawn as one on the status row.
        let row = status_row(&app);
        assert!(!row.contains(cell), "{row:?}");
        assert!(!row.contains('\u{1b}'));
    }

    #[test]
    fn an_idle_poster_says_how_to_play_in_the_row_under_it() {
        let mut app = crate::app::video::tests::ready_app();
        // The poster is uploaded once the terminal has it.
        crate::app::attachments::prepare_frame(&mut app);
        let row = status_row(&app);
        assert!(row.starts_with("video \u{b7} gv to play"), "{row:?}");
    }

    // ---- the fullscreen overlay ------------------------------------------------------------

    fn fs_screen(app: &App, w: u16, h: u16) -> Vec<String> {
        let buffer = crate::test_util::render_buffer(app, w, h);
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    /// The ids of the image cells drawn on a `w` by `h` screen, with how many cells each has.
    fn fs_ids(app: &App, w: u16, h: u16) -> std::collections::HashMap<u32, usize> {
        let buffer = crate::test_util::render_buffer(app, w, h);
        let mut ids = std::collections::HashMap::new();
        for cell in buffer.content() {
            if !cell.symbol().starts_with(kitty::PLACEHOLDER) {
                continue;
            }
            if let Color::Rgb(r, g, b) = cell.fg {
                *ids.entry(u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
                    .or_insert(0) += 1;
            }
        }
        ids
    }

    fn fs_frame(app: &mut App, index: u64) {
        let generation = app.video.as_ref().unwrap().generation;
        crate::app::update::update(
            app,
            crate::action::Action::VideoFrame {
                generation,
                index,
                pixels: vec![1u8; 568 * 320 * 4].into(),
            },
        );
        crate::app::attachments::prepare_frame(app);
    }

    #[test]
    fn the_overlay_says_it_is_loading_before_the_first_frame() {
        let (app, _dir, _pid) = crate::app::video::tests::playing_fullscreen("ui-fs-loading");
        let lines = fs_screen(&app, 80, 24);
        let text = lines.join("\n");
        assert!(text.contains("loading..."), "{text}");
        assert!(lines[0].contains(" video "), "{}", lines[0]);
        // The overlay is over the whole screen above the status row: the room is not visible.
        assert!(!text.contains("see 1"), "{text}");
        // The status row is in the bottom border, as wide as the overlay's border allows.
        assert!(
            lines[21].contains("0:00 / 0:48  Space ▮▮  q ■"),
            "{}",
            lines[21]
        );
        // No image cell yet: the player has not shown a frame.
        assert!(fs_ids(&app, 80, 24).is_empty());
    }

    #[test]
    fn the_overlay_draws_the_frame_in_the_players_id_and_the_status_follows_the_state() {
        let (mut app, _dir, _pid) = crate::app::video::tests::playing_fullscreen("ui-fs-playing");
        crate::app::attachments::prepare_frame(&mut app);
        fs_frame(&mut app, 0);
        let player = app.video.as_ref().unwrap();
        let id = player.ids[player.shown.unwrap()];
        let ids = fs_ids(&app, 80, 24);
        assert_eq!(ids.keys().copied().collect::<Vec<_>>(), [id]);
        let cells = crate::ui::image_view::placement(&app, player.frame_size).unwrap();
        assert_eq!(ids[&id], cells.0 * cells.1, "{cells:?}");
        assert!(!fs_screen(&app, 80, 24).join("\n").contains("loading..."));
        // Paused: the row offers to resume.
        app.video.as_mut().unwrap().paused = true;
        let lines = fs_screen(&app, 80, 24);
        assert!(lines[21].contains("Space ▶  q ■"), "{}", lines[21]);
    }

    #[test]
    fn a_narrow_screen_still_draws_a_fitted_overlay() {
        let (mut app, _dir, _pid) = crate::app::video::tests::playing_fullscreen("ui-fs-narrow");
        app.size = ratatui::layout::Size::new(40, 10);
        crate::app::attachments::prepare_frame(&mut app);
        fs_frame(&mut app, 0);
        let lines = fs_screen(&app, 40, 10);
        assert_eq!(lines.len(), 10);
        let player = app.video.as_ref().unwrap();
        let id = player.ids[player.shown.unwrap()];
        let ids = fs_ids(&app, 40, 10);
        let cells = crate::ui::image_view::placement(&app, player.frame_size).unwrap();
        assert!(cells.0 <= 38 && cells.1 <= 6, "{cells:?}");
        assert_eq!(ids.get(&id), Some(&(cells.0 * cells.1)));
    }

    #[test]
    fn the_box_keeps_its_poster_and_no_row_moves_while_the_overlay_is_open() {
        let (mut full, _dir, _pid) = crate::app::video::tests::playing_fullscreen("ui-fs-rows");
        let mut idle = crate::app::video::tests::ready_app();
        let (mut inline, _dir2, _pid2) = crate::app::video::tests::playing("ui-fs-rows-inline");
        // The posters are in the terminal, so the boxes are drawn in full.
        for app in [&mut full, &mut idle, &mut inline] {
            crate::app::attachments::prepare_frame(app);
        }
        let rows_of = |app: &App| {
            let room = app.rooms[0].id.clone();
            let msgs = app.messages[&room].clone();
            let ctx = app.pane_row_ctx(&room, &msgs);
            video_rows(&msgs[1], 36, &ctx.attach, Style::default())
        };
        let (a, b, c) = (rows_of(&idle), rows_of(&inline), rows_of(&full));
        assert_eq!((a.len(), b.len()), (c.len(), c.len()), "no row moves");
        // The box draws the poster's id (not a frame of the player) while the overlay plays.
        let id_of = |rows: &[Vec<Span<'static>>]| {
            rows.iter()
                .flatten()
                .find(|span| span.content.starts_with(kitty::PLACEHOLDER))
                .map(|span| span.style.fg)
        };
        assert_eq!(id_of(&a), id_of(&c));
        // And its row under it still says how to play, not the player's position.
        let last = c.last().unwrap();
        let text: String = last.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("to play"), "{text:?}");
    }

    #[test]
    fn the_status_row_lists_the_seek_and_fullscreen_keys() {
        let (mut app, _dir, _pid) = crate::app::video::tests::playing("status-all-keys");
        set_position(&mut app, 12);
        let width = pane_video_box(&app).0;
        let text = status_text(app.video.as_ref().unwrap(), Some(&app.keymap), 200);
        assert!(
            text.trim_end()
                .ends_with("Space ▮▮  q ■  h/l 5s  H/L 30s  gV full"),
            "{text:?}"
        );
        // Cut to the box on a narrow one, never wider.
        let cut = status_text(
            app.video.as_ref().unwrap(),
            Some(&app.keymap),
            width.min(30),
        );
        assert_eq!(cut.chars().count(), width.min(30));
        // In the overlay the same key closes it, and an ended video says so.
        let player = app.video.as_mut().unwrap();
        player.fullscreen = true;
        player.ended = true;
        let text = status_text(app.video.as_ref().unwrap(), Some(&app.keymap), 200);
        assert!(text.contains("0:12 / 0:48 ended"), "{text:?}");
        assert!(text.trim_end().ends_with("gV close"), "{text:?}");
    }
}

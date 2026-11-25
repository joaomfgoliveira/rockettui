//! Custom emoji images: decoding what the server sends and keeping it on disk.
//!
//! The bytes are untrusted (a server, or whoever may upload an emoji to it, chose them), so
//! everything here is defensive: only png, gif and webp are read, the dimensions come from the
//! header and are checked before any pixel is decoded, an animated image gives its first frame, and
//! nothing here panics or formats the bytes, a name or a path into an error.
//!
//! Pure apart from the cache functions, which are the only file I/O.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

/// Widest or tallest image accepted, in pixels. A larger one is refused unread.
pub const MAX_SIDE: u32 = 1024;
/// Side an accepted image is shrunk to when it is larger: it is drawn in two terminal cells, and a
/// small image is far cheaper to send to the terminal.
pub const SHRINK_TO: u32 = 128;

/// Why an image was not used. No payload: nothing about the bytes is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// Not a png, gif or webp, or the bytes do not decode.
    Bad,
    /// Larger than `MAX_SIDE` on a side (read from the header, nothing was decoded).
    TooBig,
}

/// A decoded image: straight RGBA, 4 bytes a pixel, row by row. Cheap to clone and to send in an
/// `Action`; the pixels are the server's and are never printed.
#[derive(Clone, Default, PartialEq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    /// The first frame.
    pub pixels: Arc<[u8]>,
    /// How long the first frame shows, in milliseconds; 0 for a still image.
    pub delay_ms: u32,
    /// The frames after the first, of an animated image, same size as the first. Empty for a still.
    pub later: Arc<[Frame]>,
}

/// One later frame of an animated image: complete RGBA pixels (not a difference to the frame
/// before) and how long it shows.
#[derive(Clone, PartialEq)]
pub struct Frame {
    pub pixels: Arc<[u8]>,
    pub delay_ms: u32,
}

impl Rgba {
    /// True when the image has more than one frame.
    pub fn is_animated(&self) -> bool {
        !self.later.is_empty()
    }

    /// The number of frames, 1 for a still image.
    pub fn frame_count(&self) -> u32 {
        1 + u32::try_from(self.later.len()).unwrap_or(u32::MAX - 1)
    }

    /// Which frame shows `ms` milliseconds into a loop that starts at 0: the frames in order, each
    /// for its delay, round and round. Always 0 for a still image.
    pub fn frame_at(&self, ms: u64) -> u32 {
        if !self.is_animated() {
            return 0;
        }
        let delays = std::iter::once(self.delay_ms).chain(self.later.iter().map(|f| f.delay_ms));
        let total: u64 = delays.clone().map(u64::from).sum();
        if total == 0 {
            return 0;
        }
        let mut at = ms % total;
        for (i, d) in delays.enumerate() {
            if at < u64::from(d) {
                return i as u32;
            }
            at -= u64::from(d);
        }
        0
    }

    /// The pixels of frame `n` (0 is the first), `None` past the last.
    pub fn frame_pixels(&self, n: u32) -> Option<&Arc<[u8]>> {
        match n {
            0 => Some(&self.pixels),
            n => self.later.get(n as usize - 1).map(|f| &f.pixels),
        }
    }
}

impl std::fmt::Debug for Rgba {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Rgba({}x{}, {} frames)",
            self.width,
            self.height,
            1 + self.later.len()
        )
    }
}

/// Most frames of an animated image that are kept; the rest are dropped, so the animation is cut
/// short rather than refused.
pub const MAX_FRAMES: usize = 64;
/// Shortest a frame shows, as browsers do: a gif with a zero or tiny delay would spin the terminal.
const MIN_DELAY_MS: u32 = 20;
/// What a frame with no delay shows for.
const DEFAULT_DELAY_MS: u32 = 100;

/// Decode `bytes` as a png, gif or webp image, its first frame only.
pub fn decode(bytes: &[u8]) -> Result<Rgba, ImageError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ImageError::Bad)?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP)
    ) {
        return Err(ImageError::Bad);
    }
    let decoder = reader.into_decoder().map_err(|_| ImageError::Bad)?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return Err(ImageError::Bad);
    }
    if width > MAX_SIDE || height > MAX_SIDE {
        return Err(ImageError::TooBig);
    }
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| ImageError::Bad)?;
    if image.width() > SHRINK_TO || image.height() > SHRINK_TO {
        image = image.thumbnail(SHRINK_TO, SHRINK_TO);
    }
    let rgba = image.into_rgba8();
    let (width, height) = (rgba.width(), rgba.height());
    let mut out = Rgba {
        width,
        height,
        pixels: rgba.into_raw().into(),
        delay_ms: 0,
        later: Arc::from(Vec::new()),
    };
    // A still image is done. A gif or webp is read again for its frames; anything that goes wrong
    // there leaves the first frame, which already decoded.
    if let Some((first_delay, later)) = animation(bytes, width, height) {
        out.delay_ms = first_delay;
        out.later = later.into();
    }
    Ok(out)
}

/// The frames of an animated gif or webp: the delay of the first and the frames after it, each
/// shrunk like the first to `width` by `height`. `None` for a still image or on any failure.
fn animation(bytes: &[u8], width: u32, height: u32) -> Option<(u32, Vec<Frame>)> {
    use image::AnimationDecoder;
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let frames = match reader.format()? {
        ImageFormat::Gif => image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .ok()?
            .into_frames(),
        ImageFormat::WebP => {
            let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).ok()?;
            if !decoder.has_animation() {
                return None;
            }
            decoder.into_frames()
        }
        _ => return None,
    };
    let mut out: Vec<(Arc<[u8]>, u32)> = Vec::new();
    for frame in frames.take(MAX_FRAMES) {
        let frame = frame.ok()?;
        let (n, d) = frame.delay().numer_denom_ms();
        let ms = n.checked_div(d).unwrap_or(0);
        let ms = if ms == 0 {
            DEFAULT_DELAY_MS
        } else {
            ms.max(MIN_DELAY_MS)
        };
        let mut buf = frame.into_buffer();
        if buf.width() != width || buf.height() != height {
            buf = DynamicImage::ImageRgba8(buf)
                .thumbnail(width, height)
                .into_rgba8();
        }
        // A frame that is not the size of the first would break the terminal's frame list.
        if buf.width() != width || buf.height() != height {
            return None;
        }
        out.push((buf.into_raw().into(), ms));
    }
    if out.len() < 2 {
        return None;
    }
    let (_, first_delay) = out[0].clone();
    let later = out
        .into_iter()
        .skip(1)
        .map(|(pixels, delay_ms)| Frame { pixels, delay_ms })
        .collect();
    Some((first_delay, later))
}

/// The cache directory of the emoji images: `<cache>/rockettui/emoji`, where `<cache>` is
/// `$XDG_CACHE_HOME` when that is an absolute path, else `$HOME/.cache`. `None` when neither is
/// usable, which means the images stay in memory.
pub fn cache_dir(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    let base = match xdg.map(PathBuf::from) {
        Some(p) if p.is_absolute() => p,
        _ => PathBuf::from(home.filter(|h| !h.is_empty())?).join(".cache"),
    };
    base.is_absolute()
        .then(|| base.join("rockettui").join("emoji"))
}

/// The file name for an emoji in the cache: made only of `[A-Za-z0-9_-]`, with the extension, so a
/// hostile name or etag (`../x`) can never leave the cache directory. A character outside the safe
/// set becomes `~` plus its code, so two different names cannot share a file. `None` for an
/// extension that is not a decoded format.
pub fn cache_file_name(name: &str, etag: &str, ext: &str) -> Option<String> {
    if !matches!(ext, "png" | "gif" | "webp") {
        return None;
    }
    let safe = |s: &str| {
        let mut out = String::new();
        for c in s.chars() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                out.push(c);
            } else {
                out.push_str(&format!("~{:x}", c as u32));
            }
        }
        out
    };
    // `.` is not in the safe set, so a name cannot hold an extension or a parent reference, and a
    // `-` in the etag part is told apart from the one in the name by the `.`-free `@` below.
    Some(format!("{}@{}.{ext}", safe(name), safe(etag)))
}

/// The cached bytes of an emoji, when the file for this name and etag exists and is small enough.
/// Any failure is a miss.
pub fn read_cached(dir: &Path, name: &str, etag: &str, ext: &str, limit: usize) -> Option<Vec<u8>> {
    let file = dir.join(cache_file_name(name, etag, ext)?);
    let meta = std::fs::metadata(&file).ok()?;
    if !meta.is_file() || meta.len() > limit as u64 {
        return None;
    }
    std::fs::read(file).ok()
}

/// Store the bytes of an emoji in the cache: written to a temporary file next to the target and
/// renamed, so a reader never sees half a file. Failure is returned as `false` and is not an error
/// for the client: the image stays in memory.
pub fn write_cached(dir: &Path, name: &str, etag: &str, ext: &str, bytes: &[u8]) -> bool {
    let Some(file_name) = cache_file_name(name, etag, ext) else {
        return false;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let target = dir.join(&file_name);
    let temp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
    if std::fs::write(&temp, bytes).is_err() {
        let _ = std::fs::remove_file(&temp);
        return false;
    }
    if std::fs::rename(&temp, &target).is_err() {
        let _ = std::fs::remove_file(&temp);
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageEncoder, RgbaImage};

    fn png(w: u32, h: u32, color: [u8; 4]) -> Vec<u8> {
        let img = RgbaImage::from_pixel(w, h, image::Rgba(color));
        let mut out = Vec::new();
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(img.as_raw(), w, h, image::ExtendedColorType::Rgba8)
            .expect("encode");
        out
    }

    fn gif(frames: &[[u8; 4]]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            for c in frames {
                let frame = image::Frame::new(RgbaImage::from_pixel(8, 8, image::Rgba(*c)));
                enc.encode_frame(frame).expect("frame");
            }
        }
        out
    }

    #[test]
    fn a_good_image_decodes_to_rgba() {
        let got = decode(&png(4, 2, [10, 20, 30, 255])).expect("decodes");
        assert_eq!((got.width, got.height), (4, 2));
        assert_eq!(got.pixels.len(), 4 * 2 * 4);
        assert_eq!(&got.pixels[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn a_large_image_within_the_limit_is_shrunk() {
        let got = decode(&png(512, 256, [1, 2, 3, 255])).expect("decodes");
        assert!(got.width <= SHRINK_TO && got.height <= SHRINK_TO);
        assert_eq!(got.pixels.len() as u32, got.width * got.height * 4);
    }

    #[test]
    fn an_oversized_image_is_refused_from_its_header() {
        // 4000x4000 of one colour is a small png and tens of megabytes of pixels.
        let bytes = png(4000, 4000, [0, 0, 0, 255]);
        assert!(bytes.len() < 4 * 1024 * 1024, "the fixture must stay small");
        assert_eq!(decode(&bytes), Err(ImageError::TooBig));
        assert_eq!(
            decode(&png(MAX_SIDE + 1, 1, [0; 4])),
            Err(ImageError::TooBig)
        );
        assert!(decode(&png(MAX_SIDE, 1, [0; 4])).is_ok());
    }

    #[test]
    fn something_that_is_not_an_image_is_refused() {
        assert_eq!(decode(b""), Err(ImageError::Bad));
        assert_eq!(decode(b"<html>not an image</html>"), Err(ImageError::Bad));
        assert_eq!(
            decode(b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            Err(ImageError::Bad)
        );
        // A truncated png: the header is fine, the data is not.
        let mut cut = png(64, 64, [9, 9, 9, 255]);
        cut.truncate(cut.len() / 2);
        assert_eq!(decode(&cut), Err(ImageError::Bad));
        // A format the crate could read but the client does not accept.
        assert_eq!(decode(b"BM\x00\x00\x00\x00"), Err(ImageError::Bad));
    }

    #[test]
    fn an_animated_image_gives_its_first_frame() {
        let got = decode(&gif(&[[255, 0, 0, 255], [0, 0, 255, 255]])).expect("decodes");
        assert_eq!((got.width, got.height), (8, 8));
        assert_eq!(
            &got.pixels[..3],
            &[255, 0, 0],
            "the first frame, not the last"
        );
    }

    #[test]
    fn cache_dir_follows_xdg_and_ignores_a_relative_path() {
        use std::ffi::OsString;
        let os = |s: &str| Some(OsString::from(s));
        assert_eq!(
            cache_dir(os("/x"), os("/home/u")),
            Some(PathBuf::from("/x/rockettui/emoji"))
        );
        let home = Some(PathBuf::from("/home/u/.cache/rockettui/emoji"));
        assert_eq!(cache_dir(None, os("/home/u")), home);
        assert_eq!(cache_dir(os("rel"), os("/home/u")), home);
        assert_eq!(cache_dir(os(""), os("/home/u")), home);
        assert_eq!(cache_dir(None, None), None);
        assert_eq!(cache_dir(None, os("")), None);
        assert_eq!(cache_dir(None, os("relative")), None);
    }

    #[test]
    fn file_names_cannot_leave_the_directory() {
        for hostile in [
            "../x",
            "/etc/passwd",
            "a/../../b",
            "..",
            ".",
            "a\\b",
            "x\0y",
            "é",
        ] {
            let name = cache_file_name(hostile, "46961c", "png").expect("a name");
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "_-@.~".contains(c)),
                "{hostile:?} -> {name}"
            );
            assert!(!name.contains('/') && !name.contains(".."), "{name}");
            assert!(name.starts_with(|c: char| c != '.'), "{name}");
            let name = cache_file_name("ok", hostile, "png").expect("a name");
            assert!(!name.contains('/') && !name.contains(".."), "{name}");
        }
        // Different names never share a file.
        assert_ne!(
            cache_file_name("a~2e", "e", "png"),
            cache_file_name("a.", "e", "png")
        );
        assert_ne!(
            cache_file_name("a-b", "c", "png"),
            cache_file_name("a", "b-c", "png")
        );
        assert_eq!(cache_file_name("ok", "e", "svg"), None);
        assert_eq!(cache_file_name("ok", "e", "../png"), None);
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("rockettui-test-emoji-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_cached_image_is_a_hit_until_its_etag_changes() {
        let dir = temp_dir("hit");
        assert!(
            read_cached(&dir, "rtshipit", "v1", "png", 1000).is_none(),
            "nothing yet"
        );
        assert!(write_cached(&dir, "rtshipit", "v1", "png", b"bytes-v1"));
        assert_eq!(
            read_cached(&dir, "rtshipit", "v1", "png", 1000).as_deref(),
            Some(&b"bytes-v1"[..])
        );
        // A changed etag is a different file: the old bytes are not used.
        assert!(read_cached(&dir, "rtshipit", "v2", "png", 1000).is_none());
        // A file over the limit is a miss.
        assert!(read_cached(&dir, "rtshipit", "v1", "png", 3).is_none());
        // No temporary file is left behind.
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(left.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unwritable_cache_is_a_quiet_miss() {
        let dir = temp_dir("blocked");
        std::fs::create_dir_all(&dir).unwrap();
        // The "directory" is a file, so it can be neither created nor written.
        let blocker = dir.join("emoji");
        std::fs::write(&blocker, b"x").unwrap();
        assert!(!write_cached(&blocker, "a", "b", "png", b"bytes"));
        assert!(!write_cached(
            &blocker.join("deeper"),
            "a",
            "b",
            "png",
            b"bytes"
        ));
        assert!(read_cached(&blocker, "a", "b", "png", 100).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hostile_name_writes_inside_the_directory_only() {
        let dir = temp_dir("hostile");
        assert!(write_cached(&dir, "../x", "../y", "png", b"bytes"));
        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1);
        assert!(!dir.parent().unwrap().join("x").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

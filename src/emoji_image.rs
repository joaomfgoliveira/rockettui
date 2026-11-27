//! Custom emoji and attachment images: decoding what the server sends and keeping it on disk.
//!
//! The bytes are untrusted (a server, or whoever may upload an emoji to it, chose them), so
//! everything here is defensive: only png, gif, webp, jpeg (and ico, for an emoji) are read, the dimensions come from the
//! header and are checked before any pixel is decoded, an animated image gives its first frame, and
//! nothing here panics or formats the bytes, a name or a path into an error.
//!
//! Pure apart from the cache functions, which are the only file I/O.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

/// Widest or tallest emoji image accepted, in pixels. A larger one is refused unread.
pub const MAX_SIDE: u32 = 1024;
/// Side an accepted emoji image is shrunk to when it is larger: it is drawn in two terminal cells,
/// and a small image is far cheaper to send to the terminal.
pub const SHRINK_TO: u32 = 128;
/// Widest or tallest attachment image accepted, in pixels, read from the header.
pub const ATTACHMENT_MAX_SIDE: u32 = 4096;
/// Side an attachment image is shrunk to for its inline box: a box of at most 40 rows does not
/// need more, and it keeps one upload to about a megabyte of pixels.
pub const INLINE_SHRINK_TO: u32 = 640;
/// Side an attachment image is shrunk to for the full-size view.
pub const VIEW_SHRINK_TO: u32 = 1600;
/// Side every frame of an animated attachment image is shrunk to for its inline box: up to 64
/// frames at this size are about 25 MB of pixels.
pub const INLINE_ANIM_SHRINK_TO: u32 = 320;
/// Side every frame of an animated attachment image is shrunk to for the full-size view.
pub const VIEW_ANIM_SHRINK_TO: u32 = 800;

/// What `decode` accepts and what it makes of it: the one place the emoji and the attachment
/// images differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Widest or tallest image accepted, in pixels, checked on the header before any pixel.
    pub max_side: u32,
    /// Side a larger accepted image is shrunk to.
    pub shrink_to: u32,
    /// Whether an icon file is accepted (only meaningful for an emoji).
    pub allow_ico: bool,
    /// Whether the frames after the first of a gif or webp are kept.
    pub animated: bool,
    /// Side every frame of an animated image is shrunk to when it is larger (`shrink_to` is for a
    /// still image). Only read when `animated`.
    pub animated_side: u32,
}

/// The limits of a custom emoji.
pub const EMOJI: Limits = Limits {
    max_side: MAX_SIDE,
    shrink_to: SHRINK_TO,
    allow_ico: true,
    animated: true,
    animated_side: SHRINK_TO,
};

/// The limits of an attachment image shrunk to `shrink_to`: png, gif, webp and jpeg. A gif or
/// webp keeps its frames (up to `MAX_FRAMES`, each shrunk to `animated_side`) when that is
/// `Some`, and only its first frame (shrunk to `shrink_to`) when it is `None`.
pub const fn attachment(shrink_to: u32, animated_side: Option<u32>) -> Limits {
    Limits {
        max_side: ATTACHMENT_MAX_SIDE,
        shrink_to,
        allow_ico: false,
        animated: animated_side.is_some(),
        animated_side: match animated_side {
            Some(side) => side,
            None => shrink_to,
        },
    }
}

/// Why an image was not used. No payload: nothing about the bytes is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageError {
    /// Not a png, gif, webp, jpeg or ico, or the bytes do not decode.
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

/// Decode `bytes` as a png, gif, webp, jpeg or (when `limits` allow it) ico image, within
/// `limits`: the first frame, and the later ones only when `limits.animated`.
pub fn decode(bytes: &[u8], limits: &Limits) -> Result<Rgba, ImageError> {
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| ImageError::Bad)?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP | ImageFormat::Jpeg)
    ) && !(limits.allow_ico && reader.format() == Some(ImageFormat::Ico))
    {
        return Err(ImageError::Bad);
    }
    let decoder = reader.into_decoder().map_err(|_| ImageError::Bad)?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return Err(ImageError::Bad);
    }
    if width > limits.max_side || height > limits.max_side {
        return Err(ImageError::TooBig);
    }
    // A gif or webp with several frames is read frame by frame, each shrunk as it comes so only
    // one full-size frame is in memory at a time. A still image, or anything that goes wrong
    // there, is read as one picture below.
    if limits.animated
        && let Some(out) = animation(bytes, limits.animated_side)
    {
        return Ok(out);
    }
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| ImageError::Bad)?;
    if image.width() > limits.shrink_to || image.height() > limits.shrink_to {
        image = image.thumbnail(limits.shrink_to, limits.shrink_to);
    }
    let rgba = image.into_rgba8();
    Ok(Rgba {
        width: rgba.width(),
        height: rgba.height(),
        pixels: rgba.into_raw().into(),
        delay_ms: 0,
        later: Arc::from(Vec::new()),
    })
}

/// An animated gif or webp as an image with its frames, each larger than `side` shrunk to fit it,
/// at most `MAX_FRAMES` of them. `None` for a still image or on any failure.
fn animation(bytes: &[u8], side: u32) -> Option<Rgba> {
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
    let mut size: Option<(u32, u32)> = None;
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
        if buf.width() > side || buf.height() > side {
            buf = DynamicImage::ImageRgba8(buf)
                .thumbnail(side, side)
                .into_rgba8();
        }
        // A frame that is not the size of the first would break the terminal's frame list.
        match size {
            None => size = Some((buf.width(), buf.height())),
            Some(first) if first != (buf.width(), buf.height()) => return None,
            Some(_) => {}
        }
        out.push((buf.into_raw().into(), ms));
    }
    let (width, height) = size?;
    if out.len() < 2 {
        return None;
    }
    let (pixels, delay_ms) = out[0].clone();
    let later: Vec<Frame> = out
        .into_iter()
        .skip(1)
        .map(|(pixels, delay_ms)| Frame { pixels, delay_ms })
        .collect();
    Some(Rgba {
        width,
        height,
        pixels,
        delay_ms,
        later: later.into(),
    })
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
    if !matches!(ext, "png" | "gif" | "webp" | "jpeg" | "jpg" | "ico") {
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
    match cache_file_name(name, etag, ext) {
        Some(file_name) => write_atomic(dir, &file_name, bytes),
        None => false,
    }
}

/// Write `bytes` to `dir/file_name` through a temporary file and a rename, creating `dir`.
fn write_atomic(dir: &Path, file_name: &str, bytes: &[u8]) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let target = dir.join(file_name);
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

/// The cache directory of the attachment images: `<cache>/rockettui/attachments`, beside the emoji
/// one. `None` when there is no usable cache directory, which means the images stay in memory.
pub fn attachment_cache_dir(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    Some(cache_dir(xdg, home)?.parent()?.join("attachments"))
}

/// A 64-bit FNV-1a hash of `bytes` from `seed`.
fn fnv(seed: u64, bytes: &[u8]) -> u64 {
    let mut hash = seed;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// The file name of an attachment image in its cache: 32 hex digits of a hash of the server path
/// and the reported size, so nothing of the server's text is in the name and a path such as
/// `../x` can never leave the directory. The file-upload path holds the file's id and a changed
/// file gets a new one, so no etag is needed. A different path or size is a different name.
pub fn attachment_file_name(path: &str, size: Option<u64>) -> String {
    let mut key = path.as_bytes().to_vec();
    key.push(0);
    key.extend_from_slice(&size.unwrap_or(0).to_le_bytes());
    // Two seeds make 128 bits, so that two paths sharing a file by accident is not a concern.
    format!(
        "{:016x}{:016x}",
        fnv(0xcbf2_9ce4_8422_2325, &key),
        fnv(0x8422_2325_cbf2_9ce4, &key)
    )
}

/// The cached bytes of an attachment image, when its file exists and is small enough. Any failure
/// is a miss.
pub fn read_attachment_cached(
    dir: &Path,
    path: &str,
    size: Option<u64>,
    limit: usize,
) -> Option<Vec<u8>> {
    let file = dir.join(attachment_file_name(path, size));
    let meta = std::fs::metadata(&file).ok()?;
    if !meta.is_file() || meta.len() > limit as u64 {
        return None;
    }
    std::fs::read(file).ok()
}

/// Store the bytes of an attachment image in its cache. `false` on any failure, which is not an
/// error for the client: the image stays in memory.
pub fn write_attachment_cached(dir: &Path, path: &str, size: Option<u64>, bytes: &[u8]) -> bool {
    write_atomic(dir, &attachment_file_name(path, size), bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageEncoder, RgbaImage};

    /// The emoji decode, as the older tests below call it.
    fn decode(bytes: &[u8]) -> Result<Rgba, ImageError> {
        super::decode(bytes, &EMOJI)
    }

    fn attach(bytes: &[u8]) -> Result<Rgba, ImageError> {
        super::decode(bytes, &attachment(INLINE_SHRINK_TO, None))
    }

    fn ico() -> Vec<u8> {
        let rgba = [10u8, 200, 30, 255].repeat(16 * 16);
        let mut out = Vec::new();
        image::codecs::ico::IcoEncoder::new(&mut out)
            .write_image(&rgba, 16, 16, image::ExtendedColorType::Rgba8)
            .expect("encode");
        out
    }

    #[test]
    fn the_emoji_limits_are_unchanged() {
        assert_eq!(EMOJI.max_side, 1024);
        assert_eq!(EMOJI.shrink_to, 128);
        const { assert!(EMOJI.allow_ico) };
        const { assert!(EMOJI.animated) };
        // 1025 is still refused for an emoji, and accepted for an attachment.
        assert_eq!(decode(&png(1025, 1, [0; 4])), Err(ImageError::TooBig));
        assert!(attach(&png(1025, 1, [0; 4])).is_ok());
    }

    #[test]
    fn an_attachment_image_over_the_limit_is_refused_from_its_header() {
        let bytes = png(ATTACHMENT_MAX_SIDE + 1, 1, [0; 4]);
        assert_eq!(attach(&bytes), Err(ImageError::TooBig));
        let bytes = png(8000, 8000, [0, 0, 0, 255]);
        assert!(bytes.len() < 4 * 1024 * 1024, "a small file, a huge image");
        assert_eq!(attach(&bytes), Err(ImageError::TooBig));
        assert!(attach(&png(ATTACHMENT_MAX_SIDE, 1, [0; 4])).is_ok());
    }

    #[test]
    fn an_attachment_image_is_shrunk_to_the_inline_size() {
        let got = attach(&png(2000, 1000, [1, 2, 3, 255])).expect("decodes");
        assert_eq!(got.width.max(got.height), INLINE_SHRINK_TO);
        assert_eq!(got.pixels.len() as u32, got.width * got.height * 4);
        // The view asks for more pixels from the same bytes.
        let big = super::decode(
            &png(2000, 1000, [1, 2, 3, 255]),
            &attachment(VIEW_SHRINK_TO, None),
        )
        .expect("decodes");
        assert_eq!(big.width.max(big.height), VIEW_SHRINK_TO);
        // A small image is never enlarged.
        let small = attach(&png(30, 20, [1, 2, 3, 255])).expect("decodes");
        assert_eq!((small.width, small.height), (30, 20));
    }

    #[test]
    fn an_attachment_that_is_not_an_image_or_is_corrupt_is_refused() {
        assert_eq!(attach(b""), Err(ImageError::Bad));
        assert_eq!(attach(b"<html>no</html>"), Err(ImageError::Bad));
        assert_eq!(
            attach(b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            Err(ImageError::Bad)
        );
        let mut cut = png(200, 200, [9, 9, 9, 255]);
        cut.truncate(cut.len() / 2);
        assert_eq!(attach(&cut), Err(ImageError::Bad));
        let mut garbage = png(8, 8, [9, 9, 9, 255]);
        let n = garbage.len();
        garbage[n / 2..].fill(0xA5);
        assert_eq!(attach(&garbage), Err(ImageError::Bad));
    }

    #[test]
    fn an_attachment_gif_gives_its_first_frame_only() {
        let got = attach(&gif(&[[255, 0, 0, 255], [0, 0, 255, 255]])).expect("decodes");
        assert_eq!(&got.pixels[..3], &[255, 0, 0]);
        assert!(!got.is_animated() && got.frame_count() == 1);
        // The emoji decode still keeps the animation.
        assert!(
            decode(&gif(&[[255, 0, 0, 255], [0, 0, 255, 255]]))
                .expect("decodes")
                .is_animated()
        );
    }

    #[test]
    fn an_icon_is_refused_for_an_attachment_and_accepted_for_an_emoji() {
        assert_eq!(attach(&ico()), Err(ImageError::Bad));
        assert!(decode(&ico()).is_ok());
    }

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
    fn a_jpeg_decodes() {
        // Server emojis are sometimes jpegs (`.jpeg` extension).
        let img = image::RgbImage::from_pixel(8, 8, image::Rgb([200, 30, 30]));
        let mut out = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut out)
            .encode_image(&img)
            .expect("encode");
        let got = decode(&out).expect("decodes");
        assert_eq!((got.width, got.height), (8, 8));
        assert_eq!(got.pixels[3], 255, "opaque");
    }

    #[test]
    fn an_ico_decodes() {
        // Server emojis are sometimes icons (`.ico` extension).
        let rgba = vec![10u8, 200, 30, 255].repeat(16 * 16);
        let mut out = Vec::new();
        image::codecs::ico::IcoEncoder::new(&mut out)
            .write_image(&rgba, 16, 16, image::ExtendedColorType::Rgba8)
            .expect("encode");
        let got = decode(&out).expect("decodes");
        assert_eq!((got.width, got.height), (16, 16));
        assert_eq!(
            cache_file_name("pminds", "e", "ico").as_deref(),
            Some("pminds@e.ico")
        );
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

    /// A gif of `n` frames of `side` by `side` pixels, frame `i` a different red.
    fn big_gif(side: u32, n: u8) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            for i in 0..n {
                let img = RgbaImage::from_pixel(side, side, image::Rgba([i * 3, 0, 0, 255]));
                enc.encode_frame(image::Frame::from_parts(
                    img,
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(50, 1),
                ))
                .expect("frame");
            }
        }
        out
    }

    /// An animated lossless webp of two frames of `side` by `side` pixels, red then blue, 50 ms
    /// each: a still lossless webp per frame, its bitstream chunk wrapped in an animation frame.
    fn animated_webp(side: u32) -> Vec<u8> {
        fn chunk(tag: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut out = tag.to_vec();
            out.extend((body.len() as u32).to_le_bytes());
            out.extend(body);
            if body.len() % 2 == 1 {
                out.push(0);
            }
            out
        }
        fn u24(n: u32) -> [u8; 3] {
            let b = n.to_le_bytes();
            [b[0], b[1], b[2]]
        }
        let still = |color: [u8; 4]| {
            let img = RgbaImage::from_pixel(side, side, image::Rgba(color));
            let mut out = Vec::new();
            image::codecs::webp::WebPEncoder::new_lossless(&mut out)
                .write_image(img.as_raw(), side, side, image::ExtendedColorType::Rgba8)
                .expect("encode");
            // The VP8L chunk of the still file: after "RIFF" size "WEBP", tag and size.
            let at = out.windows(4).position(|w| w == b"VP8L").expect("VP8L");
            let len = u32::from_le_bytes(out[at + 4..at + 8].try_into().unwrap()) as usize;
            out[at + 8..at + 8 + len].to_vec()
        };
        let mut body = b"WEBP".to_vec();
        let mut vp8x = vec![0b0001_0010u8, 0, 0, 0]; // animation and alpha
        vp8x.extend(u24(side - 1));
        vp8x.extend(u24(side - 1));
        body.extend(chunk(b"VP8X", &vp8x));
        body.extend(chunk(b"ANIM", &[0, 0, 0, 0, 0, 0]));
        for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
            let mut frame = Vec::new();
            frame.extend(u24(0));
            frame.extend(u24(0));
            frame.extend(u24(side - 1));
            frame.extend(u24(side - 1));
            frame.extend(u24(50));
            frame.push(0); // no blending, no disposal
            frame.extend(chunk(b"VP8L", &still(color)));
            body.extend(chunk(b"ANMF", &frame));
        }
        let mut out = b"RIFF".to_vec();
        out.extend((body.len() as u32).to_le_bytes());
        out.extend(body);
        out
    }

    fn animate(bytes: &[u8], side: u32) -> Result<Rgba, ImageError> {
        super::decode(bytes, &attachment(INLINE_SHRINK_TO, Some(side)))
    }

    #[test]
    fn an_attachment_gif_keeps_its_frames_when_animation_is_on() {
        let got = animate(&gif(&[[255, 0, 0, 255], [0, 0, 255, 255]]), 320).expect("decodes");
        assert!(got.is_animated());
        assert_eq!(got.frame_count(), 2);
        assert_eq!(&got.pixels[..3], &[255, 0, 0]);
        assert_eq!(&got.frame_pixels(1).expect("second")[..3], &[0, 0, 255]);
        assert!(got.delay_ms > 0);
    }

    #[test]
    fn an_attachment_gif_is_its_first_frame_when_animation_is_off() {
        let got = attach(&gif(&[[255, 0, 0, 255], [0, 0, 255, 255]])).expect("decodes");
        assert!(!got.is_animated() && got.frame_count() == 1);
        assert_eq!(&got.pixels[..3], &[255, 0, 0]);
    }

    #[test]
    fn an_animated_attachment_webp_plays_too() {
        // A webp decoder may be a step off in the last bit of a channel.
        let near = |px: &[u8], want: [u8; 3]| px.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 2);
        let got = animate(&animated_webp(16), 320).expect("decodes");
        assert_eq!((got.width, got.height), (16, 16));
        assert_eq!(got.frame_count(), 2);
        assert!(near(&got.pixels[..3], [255, 0, 0]));
        assert!(near(
            &got.frame_pixels(1).expect("second")[..3],
            [0, 0, 255]
        ));
        // Off, it is the first frame.
        let still = attach(&animated_webp(16)).expect("decodes");
        assert!(!still.is_animated());
        assert!(near(&still.pixels[..3], [255, 0, 0]));
    }

    #[test]
    fn every_frame_is_shrunk_to_the_animation_size() {
        let got = animate(&big_gif(600, 3), 320).expect("decodes");
        assert_eq!((got.width, got.height), (320, 320));
        assert_eq!(got.frame_count(), 3);
        for n in 0..3 {
            assert_eq!(got.frame_pixels(n).expect("frame").len(), 320 * 320 * 4);
        }
        // The view's size keeps more of it.
        let got = super::decode(
            &big_gif(1000, 2),
            &attachment(VIEW_SHRINK_TO, Some(VIEW_ANIM_SHRINK_TO)),
        )
        .expect("decodes");
        assert_eq!((got.width, got.height), (800, 800));
        // A small animation is not enlarged.
        let got = animate(&big_gif(40, 2), 320).expect("decodes");
        assert_eq!((got.width, got.height), (40, 40));
    }

    #[test]
    fn an_animation_with_too_many_frames_keeps_the_first_ones() {
        let got = animate(&big_gif(8, 70), 320).expect("decodes");
        assert_eq!(got.frame_count() as usize, MAX_FRAMES);
        assert_eq!(&got.pixels[..3], &[0, 0, 0]);
    }

    #[test]
    fn a_still_image_is_one_frame_whatever_the_animation_setting() {
        let got = animate(&png(30, 20, [1, 2, 3, 255]), 320).expect("decodes");
        assert!(!got.is_animated() && got.frame_count() == 1);
        // A gif with one frame is a still.
        let got = animate(&gif(&[[1, 2, 3, 255]]), 320).expect("decodes");
        assert!(!got.is_animated());
        // A still image is shrunk to the still size, not the animation size.
        let got = animate(&png(2000, 1000, [1, 2, 3, 255]), 320).expect("decodes");
        assert_eq!(got.width.max(got.height), INLINE_SHRINK_TO);
    }

    #[test]
    fn an_animated_attachment_over_the_header_limit_is_refused_unread() {
        let bytes = big_gif(8, 2);
        let mut huge = bytes.clone();
        // The logical screen is the 4 bytes after the 6 byte signature: make it 5000 by 5000.
        huge[6..10].copy_from_slice(&[0x88, 0x13, 0x88, 0x13]);
        assert_eq!(animate(&huge, 320), Err(ImageError::TooBig));
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

    #[test]
    fn attachment_cache_dir_sits_beside_the_emoji_one() {
        use std::ffi::OsString;
        let os = |s: &str| Some(OsString::from(s));
        assert_eq!(
            attachment_cache_dir(os("/x"), os("/home/u")),
            Some(PathBuf::from("/x/rockettui/attachments"))
        );
        assert_eq!(attachment_cache_dir(None, None), None);
    }

    #[test]
    fn attachment_file_names_are_a_hash_of_path_and_size() {
        let name = attachment_file_name("/file-upload/abc/photo.png", Some(10));
        assert_eq!(name.len(), 32);
        assert!(name.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(
            name,
            attachment_file_name("/file-upload/abc/photo.png", Some(10))
        );
        assert_ne!(
            name,
            attachment_file_name("/file-upload/abc/photo.png", Some(11))
        );
        assert_ne!(
            name,
            attachment_file_name("/file-upload/abc/photo.png", None)
        );
        assert_ne!(
            name,
            attachment_file_name("/file-upload/abd/photo.png", Some(10))
        );
        for hostile in ["../x", "/etc/passwd", "a/../../b", "x\0y", "é"] {
            let name = attachment_file_name(hostile, Some(1));
            assert!(name.chars().all(|c| c.is_ascii_hexdigit()), "{name}");
        }
    }

    #[test]
    fn a_cached_attachment_is_a_hit_for_its_path_and_size_only() {
        let dir = temp_dir("attach-hit");
        let path = "/file-upload/abc/photo.png";
        assert!(read_attachment_cached(&dir, path, Some(8), 1000).is_none());
        assert!(write_attachment_cached(&dir, path, Some(8), b"bytes"));
        assert_eq!(
            read_attachment_cached(&dir, path, Some(8), 1000).as_deref(),
            Some(&b"bytes"[..])
        );
        assert!(read_attachment_cached(&dir, path, Some(9), 1000).is_none());
        assert!(read_attachment_cached(&dir, "/file-upload/other/x.png", Some(8), 1000).is_none());
        assert!(
            read_attachment_cached(&dir, path, Some(8), 3).is_none(),
            "over the limit"
        );
        let left: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(left.len(), 1, "no temporary file is left");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unwritable_attachment_cache_is_a_quiet_miss() {
        let dir = temp_dir("attach-blocked");
        std::fs::create_dir_all(&dir).unwrap();
        let blocker = dir.join("attachments");
        std::fs::write(&blocker, b"x").unwrap();
        assert!(!write_attachment_cached(
            &blocker,
            "/file-upload/a/b",
            None,
            b"x"
        ));
        assert!(!write_attachment_cached(
            &blocker.join("deeper"),
            "/file-upload/a/b",
            None,
            b"x"
        ));
        assert!(read_attachment_cached(&blocker, "/file-upload/a/b", None, 100).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_hostile_attachment_path_writes_inside_the_directory_only() {
        let dir = temp_dir("attach-hostile");
        assert!(write_attachment_cached(&dir, "../../x", Some(1), b"bytes"));
        let entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().collect();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].path().starts_with(&dir));
        assert!(!dir.parent().unwrap().join("x").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

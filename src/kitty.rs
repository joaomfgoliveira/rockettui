//! The kitty graphics protocol, as much of it as custom emoji need: transmit an RGBA image once,
//! make it available to Unicode placeholder cells, and delete everything at exit.
//!
//! An image shown with placeholders is drawn wherever cells hold `U+10EEEE` followed by combining
//! marks that give the cell's row and column in the image, with the image id in the cell's
//! foreground colour. The cells are ordinary text, so wrapping, scrolling and redrawing need no
//! bookkeeping (see `design.md`, D1 and the spike that checked it).
//!
//! Pure: every function returns the bytes to write; `tui` writes them.

/// The placeholder character of the protocol.
pub const PLACEHOLDER: char = '\u{10EEEE}';
/// The first entries of the protocol's row/column diacritic table: index 0 and index 1.
const MARK: [char; 2] = ['\u{0305}', '\u{030D}'];

/// Columns and rows of cells an emoji covers.
pub const COLS: u8 = 2;

/// Base64 of `data` (standard alphabet, padded), as the protocol's payload wants it.
fn base64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = u32::from(chunk[0]) << 16
            | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// One graphics command whose payload (`pixels`, base64) goes in chunks of at most 4096 bytes.
/// `control` is the key list of the first chunk; later chunks carry only `m` and `q`.
fn command(control: &str, pixels: &[u8]) -> String {
    let data = base64(pixels);
    let chunks: Vec<&str> = data
        .as_bytes()
        .chunks(4096)
        .map(|c| std::str::from_utf8(c).unwrap_or(""))
        .collect();
    let mut out = String::with_capacity(data.len() + 128);
    for (i, chunk) in chunks.iter().enumerate() {
        let more = u8::from(i + 1 < chunks.len());
        if i == 0 {
            out.push_str(&format!("\x1b_G{control},m={more};{chunk}\x1b\\"));
        } else {
            out.push_str(&format!("\x1b_Gm={more},q=2;{chunk}\x1b\\"));
        }
    }
    out
}

/// The escape sequences that transmit one frame as image `id`: `width` by `height` RGBA `pixels`,
/// with a virtual placement of `COLS` by one cell for it. `q=2` silences every answer, so nothing
/// comes back on the input.
///
/// An animated emoji is not animated by the terminal (kitty does not advance an animation on an
/// image drawn with placeholders): every frame is its own image, and the client changes which id
/// the cells carry (`app::emoji`).
pub fn upload(id: u32, width: u32, height: u32, pixels: &[u8]) -> String {
    command(
        &format!("a=T,f=32,s={width},v={height},i={id},U=1,c={COLS},r=1,q=2"),
        pixels,
    )
}

/// Delete every image this client transmitted (and free the terminal's memory for them).
pub const DELETE_ALL: &str = "\x1b_Ga=d,d=A,q=2\x1b\\";

/// One placeholder cell of the first row: column `col` (0 or 1) of the image.
pub fn cell(col: usize) -> String {
    format!("{PLACEHOLDER}{}{}", MARK[0], MARK[col.min(1)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emoji_image::Rgba;
    use unicode_width::UnicodeWidthStr;

    fn image(w: u32, h: u32) -> Rgba {
        Rgba {
            width: w,
            height: h,
            pixels: vec![7u8; (w * h * 4) as usize].into(),
            ..Default::default()
        }
    }

    #[test]
    fn base64_matches_the_standard_examples() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn a_small_image_is_one_command_with_a_virtual_placement() {
        let out = upload(5, 1, 1, &image(1, 1).pixels);
        assert!(
            out.starts_with("\x1b_Ga=T,f=32,s=1,v=1,i=5,U=1,c=2,r=1,q=2,m=0;"),
            "{out:?}"
        );
        assert!(out.ends_with("\x1b\\"));
        assert_eq!(out.matches("\x1b_G").count(), 1);
    }

    #[test]
    fn a_large_image_goes_in_chunks_of_at_most_4096_bytes() {
        let out = upload(9, 64, 64, &image(64, 64).pixels);
        let commands: Vec<&str> = out.split("\x1b\\").filter(|c| !c.is_empty()).collect();
        assert!(commands.len() > 1);
        for (i, c) in commands.iter().enumerate() {
            let (control, payload) = c.split_once(';').expect("control;payload");
            assert!(payload.len() <= 4096, "chunk {i}");
            let last = i + 1 == commands.len();
            assert!(
                control.contains(if last { "m=0" } else { "m=1" }),
                "{control}"
            );
            if i > 0 {
                assert!(
                    !control.contains("a=T") && !control.contains("i="),
                    "{control}"
                );
            }
        }
        let payload: String = commands
            .iter()
            .map(|c| c.split_once(';').unwrap().1)
            .collect();
        assert_eq!(payload.len(), (64 * 64 * 4_usize).div_ceil(3) * 4);
    }

    #[test]
    fn a_cell_is_one_column_and_a_run_of_two_is_two() {
        assert_eq!(cell(0).width(), 1);
        assert_eq!(format!("{}{}", cell(0), cell(1)).width(), 2);
        assert_ne!(cell(0), cell(1));
    }

    #[test]
    fn the_delete_command_only_deletes() {
        assert!(DELETE_ALL.contains("a=d") && !DELETE_ALL.contains("a=T"));
    }
}

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
/// The protocol's row/column diacritic table (kitty's `rowcolumn-diacritics.txt`): entry `n` is the
/// combining mark that stands for row or column `n` of an image drawn with placeholders.
const MARKS: [char; 297] = [
    '\u{305}',
    '\u{30D}',
    '\u{30E}',
    '\u{310}',
    '\u{312}',
    '\u{33D}',
    '\u{33E}',
    '\u{33F}',
    '\u{346}',
    '\u{34A}',
    '\u{34B}',
    '\u{34C}',
    '\u{350}',
    '\u{351}',
    '\u{352}',
    '\u{357}',
    '\u{35B}',
    '\u{363}',
    '\u{364}',
    '\u{365}',
    '\u{366}',
    '\u{367}',
    '\u{368}',
    '\u{369}',
    '\u{36A}',
    '\u{36B}',
    '\u{36C}',
    '\u{36D}',
    '\u{36E}',
    '\u{36F}',
    '\u{483}',
    '\u{484}',
    '\u{485}',
    '\u{486}',
    '\u{487}',
    '\u{592}',
    '\u{593}',
    '\u{594}',
    '\u{595}',
    '\u{597}',
    '\u{598}',
    '\u{599}',
    '\u{59C}',
    '\u{59D}',
    '\u{59E}',
    '\u{59F}',
    '\u{5A0}',
    '\u{5A1}',
    '\u{5A8}',
    '\u{5A9}',
    '\u{5AB}',
    '\u{5AC}',
    '\u{5AF}',
    '\u{5C4}',
    '\u{610}',
    '\u{611}',
    '\u{612}',
    '\u{613}',
    '\u{614}',
    '\u{615}',
    '\u{616}',
    '\u{617}',
    '\u{657}',
    '\u{658}',
    '\u{659}',
    '\u{65A}',
    '\u{65B}',
    '\u{65D}',
    '\u{65E}',
    '\u{6D6}',
    '\u{6D7}',
    '\u{6D8}',
    '\u{6D9}',
    '\u{6DA}',
    '\u{6DB}',
    '\u{6DC}',
    '\u{6DF}',
    '\u{6E0}',
    '\u{6E1}',
    '\u{6E2}',
    '\u{6E4}',
    '\u{6E7}',
    '\u{6E8}',
    '\u{6EB}',
    '\u{6EC}',
    '\u{730}',
    '\u{732}',
    '\u{733}',
    '\u{735}',
    '\u{736}',
    '\u{73A}',
    '\u{73D}',
    '\u{73F}',
    '\u{740}',
    '\u{741}',
    '\u{743}',
    '\u{745}',
    '\u{747}',
    '\u{749}',
    '\u{74A}',
    '\u{7EB}',
    '\u{7EC}',
    '\u{7ED}',
    '\u{7EE}',
    '\u{7EF}',
    '\u{7F0}',
    '\u{7F1}',
    '\u{7F3}',
    '\u{816}',
    '\u{817}',
    '\u{818}',
    '\u{819}',
    '\u{81B}',
    '\u{81C}',
    '\u{81D}',
    '\u{81E}',
    '\u{81F}',
    '\u{820}',
    '\u{821}',
    '\u{822}',
    '\u{823}',
    '\u{825}',
    '\u{826}',
    '\u{827}',
    '\u{829}',
    '\u{82A}',
    '\u{82B}',
    '\u{82C}',
    '\u{82D}',
    '\u{951}',
    '\u{953}',
    '\u{954}',
    '\u{F82}',
    '\u{F83}',
    '\u{F86}',
    '\u{F87}',
    '\u{135D}',
    '\u{135E}',
    '\u{135F}',
    '\u{17DD}',
    '\u{193A}',
    '\u{1A17}',
    '\u{1A75}',
    '\u{1A76}',
    '\u{1A77}',
    '\u{1A78}',
    '\u{1A79}',
    '\u{1A7A}',
    '\u{1A7B}',
    '\u{1A7C}',
    '\u{1B6B}',
    '\u{1B6D}',
    '\u{1B6E}',
    '\u{1B6F}',
    '\u{1B70}',
    '\u{1B71}',
    '\u{1B72}',
    '\u{1B73}',
    '\u{1CD0}',
    '\u{1CD1}',
    '\u{1CD2}',
    '\u{1CDA}',
    '\u{1CDB}',
    '\u{1CE0}',
    '\u{1DC0}',
    '\u{1DC1}',
    '\u{1DC3}',
    '\u{1DC4}',
    '\u{1DC5}',
    '\u{1DC6}',
    '\u{1DC7}',
    '\u{1DC8}',
    '\u{1DC9}',
    '\u{1DCB}',
    '\u{1DCC}',
    '\u{1DD1}',
    '\u{1DD2}',
    '\u{1DD3}',
    '\u{1DD4}',
    '\u{1DD5}',
    '\u{1DD6}',
    '\u{1DD7}',
    '\u{1DD8}',
    '\u{1DD9}',
    '\u{1DDA}',
    '\u{1DDB}',
    '\u{1DDC}',
    '\u{1DDD}',
    '\u{1DDE}',
    '\u{1DDF}',
    '\u{1DE0}',
    '\u{1DE1}',
    '\u{1DE2}',
    '\u{1DE3}',
    '\u{1DE4}',
    '\u{1DE5}',
    '\u{1DE6}',
    '\u{1DFE}',
    '\u{20D0}',
    '\u{20D1}',
    '\u{20D4}',
    '\u{20D5}',
    '\u{20D6}',
    '\u{20D7}',
    '\u{20DB}',
    '\u{20DC}',
    '\u{20E1}',
    '\u{20E7}',
    '\u{20E9}',
    '\u{20F0}',
    '\u{2CEF}',
    '\u{2CF0}',
    '\u{2CF1}',
    '\u{2DE0}',
    '\u{2DE1}',
    '\u{2DE2}',
    '\u{2DE3}',
    '\u{2DE4}',
    '\u{2DE5}',
    '\u{2DE6}',
    '\u{2DE7}',
    '\u{2DE8}',
    '\u{2DE9}',
    '\u{2DEA}',
    '\u{2DEB}',
    '\u{2DEC}',
    '\u{2DED}',
    '\u{2DEE}',
    '\u{2DEF}',
    '\u{2DF0}',
    '\u{2DF1}',
    '\u{2DF2}',
    '\u{2DF3}',
    '\u{2DF4}',
    '\u{2DF5}',
    '\u{2DF6}',
    '\u{2DF7}',
    '\u{2DF8}',
    '\u{2DF9}',
    '\u{2DFA}',
    '\u{2DFB}',
    '\u{2DFC}',
    '\u{2DFD}',
    '\u{2DFE}',
    '\u{2DFF}',
    '\u{A66F}',
    '\u{A67C}',
    '\u{A67D}',
    '\u{A6F0}',
    '\u{A6F1}',
    '\u{A8E0}',
    '\u{A8E1}',
    '\u{A8E2}',
    '\u{A8E3}',
    '\u{A8E4}',
    '\u{A8E5}',
    '\u{A8E6}',
    '\u{A8E7}',
    '\u{A8E8}',
    '\u{A8E9}',
    '\u{A8EA}',
    '\u{A8EB}',
    '\u{A8EC}',
    '\u{A8ED}',
    '\u{A8EE}',
    '\u{A8EF}',
    '\u{A8F0}',
    '\u{A8F1}',
    '\u{AAB0}',
    '\u{AAB2}',
    '\u{AAB3}',
    '\u{AAB7}',
    '\u{AAB8}',
    '\u{AABE}',
    '\u{AABF}',
    '\u{AAC1}',
    '\u{FE20}',
    '\u{FE21}',
    '\u{FE22}',
    '\u{FE23}',
    '\u{FE24}',
    '\u{FE25}',
    '\u{FE26}',
    '\u{10A0F}',
    '\u{10A38}',
    '\u{1D185}',
    '\u{1D186}',
    '\u{1D187}',
    '\u{1D188}',
    '\u{1D189}',
    '\u{1D1AA}',
    '\u{1D1AB}',
    '\u{1D1AC}',
    '\u{1D1AD}',
    '\u{1D242}',
    '\u{1D243}',
    '\u{1D244}',
];

/// The most rows or columns an image can span: the length of the mark table.
pub const MAX_CELLS: usize = MARKS.len();

/// Ids of attachment images (the inline boxes and the full view) start here and run to
/// `IMAGE_ID_LAST`, the most a 24-bit foreground colour can carry. Emoji ids stay far below
/// (`app::emoji::MAX_IMAGE_ID`), so neither kind can ever name, replace or delete the other.
pub const IMAGE_ID_FIRST: u32 = 0x80_0000;
/// The last id of the attachment range.
pub const IMAGE_ID_LAST: u32 = 0xFF_FFFF;

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
    cell_at(0, col.min(1))
}

/// One placeholder cell: row `row` and column `col` of the image. A row or column past the end of
/// the mark table is clamped to its last entry; callers keep boxes inside `MAX_CELLS`.
pub fn cell_at(row: usize, col: usize) -> String {
    let mark = |n: usize| MARKS[n.min(MAX_CELLS - 1)];
    format!("{PLACEHOLDER}{}{}", mark(row), mark(col))
}

/// The foreground colour (red, green, blue) whose value names image `id` in a placeholder cell.
pub fn id_color(id: u32) -> (u8, u8, u8) {
    ((id >> 16) as u8, (id >> 8) as u8, id as u8)
}

/// The escape sequences that transmit `width` by `height` RGBA `pixels` as image `id`, stored
/// without being shown. Pair it with `place`, which says how many cells the image covers; the two
/// are separate so that a changed box only needs the second.
pub fn transmit(id: u32, width: u32, height: u32, pixels: &[u8]) -> String {
    command(&format!("a=t,f=32,s={width},v={height},i={id},q=2"), pixels)
}

/// The escape sequence that gives image `id` a virtual placement of `cols` by `rows` cells, which
/// placeholder cells then show. A new placement replaces the earlier one of the same image.
pub fn place(id: u32, cols: usize, rows: usize) -> String {
    format!(
        "\x1b_Ga=p,U=1,i={id},c={},r={},q=2\x1b\\",
        cols.clamp(1, MAX_CELLS),
        rows.clamp(1, MAX_CELLS)
    )
}

/// The escape sequence that deletes image `id` and frees its data in the terminal.
pub fn delete(id: u32) -> String {
    format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\")
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
    fn the_mark_table_is_the_protocols() {
        assert_eq!(MAX_CELLS, 297);
        // The ends, and that no mark repeats (a repeat would make two rows or columns the same).
        assert_eq!((MARKS[0], MARKS[1]), ('\u{0305}', '\u{030D}'));
        assert_eq!(MARKS[MAX_CELLS - 1], '\u{1D244}');
        let mut sorted = MARKS.to_vec();
        sorted.dedup();
        assert_eq!(sorted.len(), MAX_CELLS);
        assert!(MARKS.windows(2).all(|w| w[0] < w[1]), "ascending");
    }

    #[test]
    fn a_1x1_placement_is_one_cell() {
        assert_eq!(cell_at(0, 0).width(), 1);
        assert_eq!(cell_at(0, 0), cell(0));
        assert_eq!(cell_at(0, 1), cell(1));
        assert!(place(0x80_0001, 1, 1).contains("c=1,r=1"));
    }

    #[test]
    fn wide_and_tall_placements_give_each_cell_its_own_marks() {
        // A 40 by 3 box: every (row, column) is a different cell, each one column wide.
        let mut seen = std::collections::HashSet::new();
        for r in 0..3 {
            for c in 0..40 {
                let cell = cell_at(r, c);
                assert_eq!(cell.width(), 1);
                assert!(cell.starts_with(PLACEHOLDER));
                assert!(seen.insert(cell));
            }
        }
        // A 2 by 40 box likewise.
        let tall: std::collections::HashSet<String> = (0..40)
            .flat_map(|r| (0..2).map(move |c| cell_at(r, c)))
            .collect();
        assert_eq!(tall.len(), 80);
        let out = place(0x80_0002, 40, 3);
        assert_eq!(out, "\x1b_Ga=p,U=1,i=8388610,c=40,r=3,q=2\x1b\\");
        // Past the table is clamped, never a panic or a wrap.
        assert!(place(1, 9999, 9999).contains(&format!("c={MAX_CELLS},r={MAX_CELLS}")));
        assert_eq!(
            cell_at(10_000, 10_000),
            cell_at(MAX_CELLS - 1, MAX_CELLS - 1)
        );
    }

    #[test]
    fn attachment_ids_never_overlap_emoji_ids() {
        const { assert!(IMAGE_ID_FIRST > crate::app::emoji::MAX_IMAGE_ID) };
        const { assert!(IMAGE_ID_LAST > IMAGE_ID_FIRST) };
        assert_eq!(id_color(IMAGE_ID_LAST), (255, 255, 255));
        assert_eq!(id_color(IMAGE_ID_FIRST), (0x80, 0, 0));
        // The id of a cell is its colour: a round trip.
        let (r, g, b) = id_color(0x80_1234);
        assert_eq!(
            u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b),
            0x80_1234
        );
    }

    #[test]
    fn transmit_stores_and_delete_frees_one_image() {
        let out = transmit(IMAGE_ID_FIRST, 1, 1, &image(1, 1).pixels);
        assert!(
            out.starts_with("\x1b_Ga=t,f=32,s=1,v=1,i=8388608,q=2,m=0;"),
            "{out:?}"
        );
        assert!(!out.contains("a=T") && !out.contains("U=1"));
        let gone = delete(IMAGE_ID_FIRST);
        assert!(gone.contains("a=d") && gone.contains("i=8388608") && gone.contains("d=I"));
        assert!(!gone.contains("d=A"), "one image only, not all");
    }

    #[test]
    fn the_delete_command_only_deletes() {
        assert!(DELETE_ALL.contains("a=d") && !DELETE_ALL.contains("a=T"));
        // Every image the client sent, emoji and attachments alike: no id is named.
        assert!(DELETE_ALL.contains("d=A") && !DELETE_ALL.contains("i="));
    }
}

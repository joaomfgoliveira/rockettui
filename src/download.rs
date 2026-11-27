//! Where a downloaded attachment is kept and what it may be called: the pure side of
//! `open_attachment`.
//!
//! Nothing of the server's text reaches a file name. The name is a hash of the file's server path
//! and an extension taken from a fixed table keyed by the media type, so a hostile file name
//! (`../../x.sh`) never becomes a path, and a type that is not in the table (a script, a
//! `.desktop` file, an archive, an SVG) is refused before anything is written, so it never reaches
//! an opener that might act on it.

use std::path::PathBuf;

/// The extension for a media type, `None` for a type that is not an image, video or audio type of
/// the list. Parameters (`; codecs=...`) and case are ignored. Image types that can carry script
/// (`image/svg+xml`) are not in the list.
pub fn extension(mime: &str) -> Option<&'static str> {
    let essence = mime.split(';').next()?.trim().to_ascii_lowercase();
    Some(match essence.as_str() {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/bmp" => "bmp",
        "image/avif" => "avif",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "video/x-matroska" => "mkv",
        "video/ogg" => "ogv",
        "video/x-msvideo" => "avi",
        "video/mpeg" => "mpg",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/mp4" | "audio/x-m4a" | "audio/m4a" => "m4a",
        "audio/aac" => "aac",
        "audio/ogg" => "ogg",
        "audio/opus" => "opus",
        "audio/wav" | "audio/x-wav" | "audio/wave" => "wav",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/webm" => "weba",
        _ => return None,
    })
}

/// The directory of the downloaded attachments: `<cache>/rockettui/downloads`, beside the other
/// caches. `None` when there is no usable cache directory.
pub fn downloads_dir(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    Some(
        crate::emoji_image::cache_dir(xdg, home)?
            .parent()?
            .join("downloads"),
    )
}

/// The file name of the download of the file at server path `path`, with the extension for `mime`:
/// 32 hex digits and the extension. `None` when the media type is not in the list. The same path
/// always gives the same name, which is what lets a second open skip the request.
pub fn file_name(path: &str, mime: &str) -> Option<String> {
    let ext = extension(mime)?;
    Some(format!(
        "{}.{ext}",
        crate::emoji_image::attachment_file_name(path, None)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn the_table_gives_an_extension_to_image_video_and_audio_types_only() {
        for (mime, ext) in [
            ("image/png", "png"),
            ("image/jpeg", "jpg"),
            ("video/mp4", "mp4"),
            ("video/webm", "webm"),
            ("video/quicktime", "mov"),
            ("audio/mpeg", "mp3"),
            ("audio/ogg", "ogg"),
            ("audio/wav", "wav"),
            ("audio/x-m4a", "m4a"),
        ] {
            assert_eq!(extension(mime), Some(ext), "{mime}");
        }
        // Case and parameters do not matter.
        assert_eq!(extension("VIDEO/MP4; codecs=\"avc1\""), Some("mp4"));
        assert_eq!(extension(" audio/ogg ;x=y"), Some("ogg"));
    }

    #[test]
    fn a_type_outside_the_list_is_refused() {
        for mime in [
            "",
            "text/html",
            "application/pdf",
            "application/zip",
            "application/x-sh",
            "application/x-desktop",
            "image/svg+xml",
            "image/",
            "video/",
            "video/x-unknown",
            "audio/../x",
            "../video/mp4",
            "x-scheme/video/mp4",
        ] {
            assert_eq!(extension(mime), None, "{mime:?}");
            assert_eq!(file_name("/file-upload/a/b", mime), None, "{mime:?}");
        }
    }

    #[test]
    fn a_hostile_path_never_leaves_the_directory() {
        let dir = PathBuf::from("/cache/rockettui/downloads");
        for hostile in [
            "../../x.sh",
            "/etc/passwd",
            "/file-upload/a/../../b",
            "a\\b",
            "x\0y",
            "é",
            "",
        ] {
            let name = file_name(hostile, "video/mp4").expect("a name");
            let stem = name.strip_suffix(".mp4").expect("the extension");
            assert!(
                stem.len() == 32 && stem.chars().all(|c| c.is_ascii_hexdigit()),
                "{hostile:?} -> {name}"
            );
            assert!(name.ends_with(".mp4") && !name.contains(".."), "{name}");
            assert_eq!(dir.join(&name).parent(), Some(dir.as_path()));
        }
    }

    #[test]
    fn the_name_follows_the_path_and_the_type() {
        let a = file_name("/file-upload/a/clip.mp4", "video/mp4").unwrap();
        assert_eq!(
            a,
            file_name("/file-upload/a/clip.mp4", "video/mp4").unwrap()
        );
        assert_ne!(
            a,
            file_name("/file-upload/b/clip.mp4", "video/mp4").unwrap()
        );
        assert_eq!(a.len(), 32 + ".mp4".len());
        assert!(
            file_name("/file-upload/a/clip.mp4", "video/webm")
                .unwrap()
                .ends_with(".webm")
        );
    }

    #[test]
    fn the_directory_follows_xdg_and_ignores_a_relative_path() {
        let os = |s: &str| Some(OsString::from(s));
        assert_eq!(
            downloads_dir(os("/x"), os("/home/u")),
            Some(PathBuf::from("/x/rockettui/downloads"))
        );
        let home = Some(PathBuf::from("/home/u/.cache/rockettui/downloads"));
        assert_eq!(downloads_dir(None, os("/home/u")), home);
        assert_eq!(downloads_dir(os("rel"), os("/home/u")), home);
        assert_eq!(downloads_dir(None, None), None);
        assert_eq!(downloads_dir(None, os("relative")), None);
    }
}

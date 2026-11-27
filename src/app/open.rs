//! Opening an attachment with the user's own program (`open_attachment`, `go`): download the file
//! of the first image, video or audio attachment of the message under the cursor to the cache,
//! then run the configured program on it.
//!
//! Pure: the download and the program are `Effect`s, their answers `Action`s. The reducer keeps the
//! attachments being downloaded, so a second `go` on one of them does nothing. Every notice is
//! fixed wording: no file name, path or program output is ever shown or logged.

use crate::api::{DownloadError, MAX_DOWNLOAD_BYTES};
use crate::app::{App, StatusMsg, image_view};
use crate::data::{Attachment, AttachmentKind};
use crate::effect::{DownloadRef, Effect, OpenError};
use std::collections::HashMap;

/// The downloads in flight, by the server path of the file, with the kind that picks the program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Opening {
    in_flight: HashMap<String, AttachmentKind>,
}

impl Opening {
    /// True when the file at `path` is being downloaded.
    pub fn is_downloading(&self, path: &str) -> bool {
        self.in_flight.contains_key(path)
    }

    /// How many downloads are running.
    pub fn len(&self) -> usize {
        self.in_flight.len()
    }

    pub fn is_empty(&self) -> bool {
        self.in_flight.is_empty()
    }
}

fn info(app: &mut App, text: &str) {
    app.status_msg = Some(StatusMsg::Info(text.to_string()));
}

fn error(app: &mut App, text: &str) {
    app.status_msg = Some(StatusMsg::Error(text.to_string()));
}

/// True when `attachment` is of a kind `open_attachment` opens.
fn openable_kind(attachment: &Attachment) -> bool {
    matches!(
        attachment.kind,
        AttachmentKind::Image | AttachmentKind::Video | AttachmentKind::Audio
    )
}

/// True when the message under the cursor has an image, video or audio attachment: what the
/// context popup checks before it lists the action.
pub fn has_attachment_under_cursor(app: &App) -> bool {
    image_view::message_under_cursor(app).is_some_and(|(_, m)| {
        m.kind == crate::data::MessageKind::Normal && m.attachments.iter().any(openable_kind)
    })
}

/// `open_attachment`: start opening the first image, video or audio attachment of the message
/// under the cursor. A message with none, an attachment with no address the client may request,
/// a type that is not on the list or a file over the size limit gets a status notice and nothing
/// else changes. Anything but the messages pane in Normal mode is a silent no-op, and so is a
/// second press while the same file downloads.
pub fn open(app: &mut App) {
    let Some((_, message)) = image_view::message_under_cursor(app) else {
        return;
    };
    let first = (message.kind == crate::data::MessageKind::Normal)
        .then(|| message.attachments.iter().find(|a| openable_kind(a)))
        .flatten();
    let Some(attachment) = first else {
        info(app, "nothing to open in this message");
        return;
    };
    let Some(path) = attachment.path.clone() else {
        error(app, "this attachment cannot be opened");
        return;
    };
    let supported = attachment
        .mime
        .as_deref()
        .is_some_and(|mime| crate::download::extension(mime).is_some());
    let Some(mime) = attachment.mime.clone().filter(|_| supported) else {
        error(app, "this file type is not supported");
        return;
    };
    if attachment
        .size
        .is_some_and(|n| n > MAX_DOWNLOAD_BYTES as u64)
    {
        error(app, "the file is too large to download");
        return;
    }
    if app.opening.is_downloading(&path) {
        return;
    }
    let (kind, size) = (attachment.kind, attachment.size);
    app.opening.in_flight.insert(path.clone(), kind);
    info(app, "downloading attachment...");
    app.effects
        .push(Effect::DownloadAttachment(DownloadRef { path, mime, size }));
}

/// The file arrived (or was there): run the configured program on it.
pub fn on_downloaded(app: &mut App, download: DownloadRef, file: std::path::PathBuf) {
    let Some(kind) = app.opening.in_flight.remove(&download.path) else {
        return;
    };
    let Some(command) = app.settings.open.command(kind) else {
        return;
    };
    info(app, "opening attachment...");
    app.effects.push(Effect::OpenFile { command, file });
}

/// The download failed: say why, in fixed words, and allow a new try.
pub fn on_download_failed(app: &mut App, download: DownloadRef, why: DownloadError) {
    app.opening.in_flight.remove(&download.path);
    error(
        app,
        match why {
            DownloadError::Refused => "this attachment cannot be opened",
            DownloadError::TooLarge => "the file is too large to download",
            DownloadError::Save => "could not save the file",
            DownloadError::Failed => "could not download the attachment",
        },
    );
}

/// The program was started, or not.
pub fn on_opened(app: &mut App, result: Result<(), OpenError>) {
    match result {
        Ok(()) => info(app, "attachment opened"),
        Err(OpenError::CannotStart) => error(app, "could not start the program"),
        Err(OpenError::ExitedWithError) => error(app, "the program exited with an error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::Action;
    use crate::app::{Focus, update::update};
    use crate::config::settings::{OpenSettings, platform_opener};
    use crate::data::Message;
    use crate::test_util::{app_80x24, press, sample_message};

    const VIDEO: &str = "/file-upload/v1/SENTINEL-clip.mp4";

    fn attachment(kind: AttachmentKind, path: Option<&str>, mime: Option<&str>) -> Attachment {
        Attachment {
            kind,
            name: "SENTINEL-NAME.mp4".to_string(),
            path: path.map(str::to_string),
            full_path: None,
            mime: mime.map(str::to_string),
            size: Some(5_000_000),
            dims: None,
        }
    }

    fn video() -> Attachment {
        attachment(AttachmentKind::Video, Some(VIDEO), Some("video/mp4"))
    }

    /// An app with the cursor on a message holding `attachments`, messages pane focused.
    fn app_with(attachments: Vec<Attachment>) -> App {
        let mut app = app_80x24();
        let room = app.rooms[0].id.clone();
        let msg = Message {
            room_id: room.clone(),
            body: "look".to_string(),
            attachments,
            plain_text: false,
            ..sample_message("m-open")
        };
        app.messages.insert(room.clone(), vec![msg]);
        app.open_room(room);
        app.focus = Focus::Messages;
        app.effects.clear();
        app
    }

    fn status(app: &App) -> Option<String> {
        match app.status_msg.as_ref()? {
            StatusMsg::Info(t) | StatusMsg::Error(t) => Some(t.clone()),
        }
    }

    fn downloads(app: &App) -> Vec<DownloadRef> {
        app.effects
            .iter()
            .filter_map(|e| match e {
                Effect::DownloadAttachment(d) => Some(d.clone()),
                _ => None,
            })
            .collect()
    }

    fn nothing_changed(app: &App) {
        assert!(app.effects.is_empty(), "{:?}", app.effects);
        assert!(app.opening.is_empty());
    }

    #[test]
    fn go_downloads_the_first_supported_attachment() {
        let mut app = app_with(vec![
            attachment(
                AttachmentKind::Other,
                Some("/file-upload/o/a.pdf"),
                Some("application/pdf"),
            ),
            video(),
            attachment(
                AttachmentKind::Audio,
                Some("/file-upload/a/v.ogg"),
                Some("audio/ogg"),
            ),
        ]);
        press(&mut app, "go");
        assert_eq!(
            downloads(&app),
            [DownloadRef {
                path: VIDEO.to_string(),
                mime: "video/mp4".to_string(),
                size: Some(5_000_000),
            }]
        );
        assert_eq!(status(&app).as_deref(), Some("downloading attachment..."));
        assert!(app.opening.is_downloading(VIDEO));
    }

    #[test]
    fn a_double_press_downloads_once() {
        let mut app = app_with(vec![video()]);
        press(&mut app, "go");
        press(&mut app, "go");
        assert_eq!(downloads(&app).len(), 1);
        assert_eq!(app.opening.len(), 1);
    }

    #[test]
    fn a_message_with_nothing_to_open_says_so() {
        let mut app = app_with(Vec::new());
        press(&mut app, "go");
        assert_eq!(
            status(&app).as_deref(),
            Some("nothing to open in this message")
        );
        nothing_changed(&app);
        // A file that is not an image, video or audio is nothing to open either.
        let mut app = app_with(vec![attachment(
            AttachmentKind::Other,
            Some("/file-upload/o/a.pdf"),
            Some("application/pdf"),
        )]);
        press(&mut app, "go");
        assert_eq!(
            status(&app).as_deref(),
            Some("nothing to open in this message")
        );
        nothing_changed(&app);
    }

    #[test]
    fn an_address_the_client_may_not_request_is_refused() {
        let mut app = app_with(vec![attachment(
            AttachmentKind::Video,
            None,
            Some("video/mp4"),
        )]);
        press(&mut app, "go");
        assert_eq!(
            status(&app).as_deref(),
            Some("this attachment cannot be opened")
        );
        nothing_changed(&app);
    }

    #[test]
    fn a_type_that_is_not_on_the_list_is_refused() {
        for mime in [Some("video/x-unknown"), Some("application/x-sh"), None] {
            let mut app = app_with(vec![attachment(AttachmentKind::Video, Some(VIDEO), mime)]);
            press(&mut app, "go");
            assert_eq!(
                status(&app).as_deref(),
                Some("this file type is not supported"),
                "{mime:?}"
            );
            nothing_changed(&app);
        }
    }

    #[test]
    fn a_file_over_the_limit_is_refused_without_a_request() {
        let mut big = video();
        big.size = Some(MAX_DOWNLOAD_BYTES as u64 + 1);
        let mut app = app_with(vec![big]);
        press(&mut app, "go");
        assert_eq!(
            status(&app).as_deref(),
            Some("the file is too large to download")
        );
        nothing_changed(&app);
        // Exactly the limit is allowed.
        let mut exact = video();
        exact.size = Some(MAX_DOWNLOAD_BYTES as u64);
        let mut app = app_with(vec![exact]);
        press(&mut app, "go");
        assert_eq!(downloads(&app).len(), 1);
    }

    #[test]
    fn another_pane_or_mode_does_nothing() {
        for focus in [Focus::Sidebar, Focus::Input] {
            let mut app = app_with(vec![video()]);
            app.focus = focus;
            update(&mut app, Action::OpenAttachment);
            nothing_changed(&app);
            assert!(app.status_msg.is_none());
        }
    }

    fn done(app: &mut App) -> Vec<Effect> {
        let download = downloads(app).remove(0);
        app.effects.clear();
        update(
            app,
            Action::AttachmentDownloaded {
                download,
                file: "/cache/f.mp4".into(),
            },
        );
        app.effects.clone()
    }

    #[test]
    fn a_downloaded_file_is_opened_with_the_program_of_its_kind() {
        let mut app = app_with(vec![video()]);
        app.settings.open = OpenSettings {
            video: Some(vec!["mpv".into(), "--force-window".into()]),
            default: Some(vec!["other".into()]),
            ..Default::default()
        };
        press(&mut app, "go");
        let effects = done(&mut app);
        assert_eq!(
            effects,
            [Effect::OpenFile {
                command: vec!["mpv".into(), "--force-window".into()],
                file: "/cache/f.mp4".into(),
            }]
        );
        assert!(app.opening.is_empty(), "the download is over");
        assert_eq!(status(&app).as_deref(), Some("opening attachment..."));
    }

    #[test]
    fn the_program_falls_back_to_default_then_the_platform_opener() {
        let mut app = app_with(vec![video()]);
        app.settings.open.default = Some(vec!["fallback".into()]);
        press(&mut app, "go");
        assert!(
            matches!(&done(&mut app)[..], [Effect::OpenFile { command, .. }] if command == &["fallback"])
        );
        let mut app = app_with(vec![video()]);
        press(&mut app, "go");
        assert!(
            matches!(&done(&mut app)[..], [Effect::OpenFile { command, .. }] if command == &[platform_opener()])
        );
    }

    #[test]
    fn an_image_is_opened_too_whatever_the_terminal() {
        let mut app = app_with(vec![attachment(
            AttachmentKind::Image,
            Some("/file-upload/i/p.png"),
            Some("image/png"),
        )]);
        app.graphics = crate::app::emoji::Graphics::Unsupported;
        press(&mut app, "go");
        assert_eq!(downloads(&app).len(), 1);
    }

    #[test]
    fn a_failed_download_says_why_and_can_be_tried_again() {
        for (error, text) in [
            (DownloadError::Failed, "could not download the attachment"),
            (DownloadError::TooLarge, "the file is too large to download"),
            (DownloadError::Save, "could not save the file"),
            (DownloadError::Refused, "this attachment cannot be opened"),
        ] {
            let mut app = app_with(vec![video()]);
            press(&mut app, "go");
            let download = downloads(&app).remove(0);
            app.effects.clear();
            update(
                &mut app,
                Action::AttachmentDownloadFailed { download, error },
            );
            assert_eq!(status(&app).as_deref(), Some(text));
            assert!(app.opening.is_empty() && app.effects.is_empty());
            press(&mut app, "go");
            assert_eq!(
                downloads(&app).len(),
                1,
                "a new try is allowed after {error:?}"
            );
        }
    }

    #[test]
    fn the_result_of_the_program_is_told_in_fixed_words() {
        let mut app = app_with(vec![video()]);
        for (result, text) in [
            (Ok(()), "attachment opened"),
            (Err(OpenError::CannotStart), "could not start the program"),
            (
                Err(OpenError::ExitedWithError),
                "the program exited with an error",
            ),
        ] {
            update(&mut app, Action::AttachmentOpened(result));
            assert_eq!(status(&app).as_deref(), Some(text));
        }
    }

    #[test]
    fn no_notice_holds_the_name_or_the_path() {
        let mut app = app_with(vec![video()]);
        let mut seen = Vec::new();
        press(&mut app, "go");
        seen.extend(status(&app));
        let download = downloads(&app).remove(0);
        for error in [
            DownloadError::Failed,
            DownloadError::TooLarge,
            DownloadError::Save,
            DownloadError::Refused,
        ] {
            update(
                &mut app,
                Action::AttachmentDownloadFailed {
                    download: download.clone(),
                    error,
                },
            );
            seen.extend(status(&app));
        }
        for result in [
            Ok(()),
            Err(OpenError::CannotStart),
            Err(OpenError::ExitedWithError),
        ] {
            update(&mut app, Action::AttachmentOpened(result));
            seen.extend(status(&app));
        }
        assert!(seen.len() >= 8);
        for text in seen {
            assert!(
                !text.contains("SENTINEL") && !text.contains("file-upload"),
                "{text}"
            );
        }
        // Nor does the debug form of the effect or the actions.
        let shown = format!("{:?}", Effect::DownloadAttachment(download.clone()));
        assert!(
            !shown.contains("SENTINEL") && !shown.contains("file-upload"),
            "{shown}"
        );
    }
}

//! The curated names a `keymap.toml` value may use (D-07).
//!
//! Only the unit variants of [`Action`] that make sense as a key press have a name. Typed text,
//! raw keys, resize, clock, paste, loaded data, the key timeout and counted actions are produced by
//! the program, never by a keymap row, so they have none (T-02-11). The names are the user-facing
//! vocabulary: renaming one breaks a user's `keymap.toml`.
//!
//! `retry_send` and `discard_send` are user-facing vocabulary too (D-06): they act on a failed or
//! unconfirmed message under the messages-pane cursor, bound by default to `r` (retry). `edit_message`
//! and `delete_message` act on the user's own server messages: `e` and `dd` by default.
//! `delete_message` also discards a failed or unconfirmed local row, so `dd` keeps that meaning;
//! `discard_send` remains a name that only discards. `show_reactions` opens the popup of who reacted
//! to the message under the cursor (`gr` by default) and does nothing on a message without any.
//! `react_message` opens the picker to add a reaction or take one back (`+` by default).
//! `switch_room` opens the room switcher (`<C-k>` by default).

use crate::action::Action;

/// Every bindable action name, snake_case. `none` and `cmd:<command>` are handled by the loader.
pub const ACTION_NAMES: &[&str] = &[
    "move_down",
    "move_up",
    "move_left",
    "move_right",
    "goto_top",
    "goto_bottom",
    "half_page_down",
    "half_page_up",
    "select",
    "toggle_fold",
    "focus_left",
    "focus_down",
    "focus_right",
    "focus_next",
    "focus_prev",
    "enter_command",
    "enter_insert",
    "cancel",
    "delete_back",
    "insert_newline",
    "delete_word_back",
    "delete_to_line_start",
    "cursor_left",
    "cursor_right",
    "cursor_up",
    "cursor_down",
    "cursor_home",
    "cursor_end",
    "submit_input",
    "retry_send",
    "discard_send",
    "edit_message",
    "delete_message",
    "show_reactions",
    "open_channel",
    "show_image",
    "open_attachment",
    "play_video",
    "play_video_fullscreen",
    "react_message",
    "open_thread",
    "list_threads",
    "toggle_follow_thread",
    "close_thread",
    "reply_in_thread",
    "toggle_reply_to_channel",
    "switch_room",
    "show_help",
    "show_context",
    "search_start",
    "search_next",
    "search_prev",
    "execute_command",
    "quit",
];

/// The names that only the `[video]` table accepts, the player's keys while a video plays.
pub const VIDEO_ACTION_NAMES: &[&str] = &[
    "video_pause",
    "video_seek_back",
    "video_seek_forward",
    "video_seek_back_far",
    "video_seek_forward_far",
    "video_stop",
];

/// The player action a `[video]` name stands for. `None` for any other name.
pub fn video_action_from_name(name: &str) -> Option<Action> {
    Some(match name {
        "video_pause" => Action::VideoPause,
        "video_seek_back" => Action::VideoSeekBack,
        "video_seek_forward" => Action::VideoSeekForward,
        "video_seek_back_far" => Action::VideoSeekBackFar,
        "video_seek_forward_far" => Action::VideoSeekForwardFar,
        "video_stop" => Action::VideoStop,
        _ => return None,
    })
}

/// The action a name stands for. `none`, `cmd:<command>` and every unknown name give `None`.
pub fn action_from_name(name: &str) -> Option<Action> {
    Some(match name {
        "move_down" => Action::MoveDown,
        "move_up" => Action::MoveUp,
        "move_left" => Action::MoveLeft,
        "move_right" => Action::MoveRight,
        "goto_top" => Action::GotoTop,
        "goto_bottom" => Action::GotoBottom,
        "half_page_down" => Action::HalfPageDown,
        "half_page_up" => Action::HalfPageUp,
        "select" => Action::Select,
        "toggle_fold" => Action::ToggleFold,
        "focus_left" => Action::FocusLeft,
        "focus_down" => Action::FocusDown,
        "focus_right" => Action::FocusRight,
        "focus_next" => Action::FocusNext,
        "focus_prev" => Action::FocusPrev,
        "enter_command" => Action::EnterCommand,
        "enter_insert" => Action::EnterInsert,
        "cancel" => Action::Cancel,
        "delete_back" => Action::DeleteBack,
        "insert_newline" => Action::InsertNewline,
        "delete_word_back" => Action::DeleteWordBack,
        "delete_to_line_start" => Action::DeleteToLineStart,
        "cursor_left" => Action::CursorLeft,
        "cursor_right" => Action::CursorRight,
        "cursor_up" => Action::CursorUp,
        "cursor_down" => Action::CursorDown,
        "cursor_home" => Action::CursorHome,
        "cursor_end" => Action::CursorEnd,
        "submit_input" => Action::SubmitInput,
        "retry_send" => Action::RetrySend,
        "discard_send" => Action::DiscardSend,
        "edit_message" => Action::EditMessage,
        "delete_message" => Action::DeleteMessage,
        "show_reactions" => Action::ShowReactions,
        "open_channel" => Action::OpenChannel,
        "show_image" => Action::ShowImage,
        "open_attachment" => Action::OpenAttachment,
        "play_video" => Action::PlayVideo,
        "play_video_fullscreen" => Action::PlayVideoFullscreen,
        "react_message" => Action::ReactMessage,
        "open_thread" => Action::OpenThread,
        "list_threads" => Action::ListThreads,
        "toggle_follow_thread" => Action::ToggleFollowThread,
        "reply_in_thread" => Action::ReplyInThread,
        "toggle_reply_to_channel" => Action::ToggleReplyToChannel,
        "close_thread" => Action::CloseThread,
        "switch_room" => Action::SwitchRoom,
        "show_help" => Action::ShowHelp,
        "show_context" => Action::ShowContext,
        "search_start" => Action::SearchStart,
        "search_next" => Action::SearchNext,
        "search_prev" => Action::SearchPrev,
        "execute_command" => Action::ExecuteCommand,
        "quit" => Action::Quit,
        _ => return None,
    })
}

/// The name of a bindable action. The match is exhaustive on purpose (no wildcard arm): every
/// `Action` variant added later must choose whether it is bindable, as in the `Debug` impl.
pub fn name_of(action: &Action) -> Option<&'static str> {
    match action {
        Action::MoveDown => Some("move_down"),
        Action::MoveUp => Some("move_up"),
        Action::MoveLeft => Some("move_left"),
        Action::MoveRight => Some("move_right"),
        Action::GotoTop => Some("goto_top"),
        Action::GotoBottom => Some("goto_bottom"),
        Action::HalfPageDown => Some("half_page_down"),
        Action::HalfPageUp => Some("half_page_up"),
        Action::Select => Some("select"),
        Action::ToggleFold => Some("toggle_fold"),
        Action::FocusLeft => Some("focus_left"),
        Action::FocusDown => Some("focus_down"),
        Action::FocusRight => Some("focus_right"),
        Action::FocusNext => Some("focus_next"),
        Action::FocusPrev => Some("focus_prev"),
        Action::EnterCommand => Some("enter_command"),
        Action::EnterInsert => Some("enter_insert"),
        Action::Cancel => Some("cancel"),
        Action::DeleteBack => Some("delete_back"),
        Action::InsertNewline => Some("insert_newline"),
        Action::DeleteWordBack => Some("delete_word_back"),
        Action::DeleteToLineStart => Some("delete_to_line_start"),
        Action::CursorLeft => Some("cursor_left"),
        Action::CursorRight => Some("cursor_right"),
        Action::CursorUp => Some("cursor_up"),
        Action::CursorDown => Some("cursor_down"),
        Action::CursorHome => Some("cursor_home"),
        Action::CursorEnd => Some("cursor_end"),
        Action::SubmitInput => Some("submit_input"),
        Action::RetrySend => Some("retry_send"),
        Action::DiscardSend => Some("discard_send"),
        Action::EditMessage => Some("edit_message"),
        Action::DeleteMessage => Some("delete_message"),
        Action::ShowReactions => Some("show_reactions"),
        Action::OpenChannel => Some("open_channel"),
        Action::ShowImage => Some("show_image"),
        Action::OpenAttachment => Some("open_attachment"),
        Action::PlayVideo => Some("play_video"),
        Action::PlayVideoFullscreen => Some("play_video_fullscreen"),
        Action::VideoPause => Some("video_pause"),
        Action::VideoSeekBack => Some("video_seek_back"),
        Action::VideoSeekForward => Some("video_seek_forward"),
        Action::VideoSeekBackFar => Some("video_seek_back_far"),
        Action::VideoSeekForwardFar => Some("video_seek_forward_far"),
        Action::VideoStop => Some("video_stop"),
        Action::ReactMessage => Some("react_message"),
        Action::OpenThread => Some("open_thread"),
        Action::ListThreads => Some("list_threads"),
        Action::ToggleFollowThread => Some("toggle_follow_thread"),
        Action::ReplyInThread => Some("reply_in_thread"),
        Action::ToggleReplyToChannel => Some("toggle_reply_to_channel"),
        Action::CloseThread => Some("close_thread"),
        Action::SwitchRoom => Some("switch_room"),
        Action::ShowHelp => Some("show_help"),
        Action::ShowContext => Some("show_context"),
        Action::SearchStart => Some("search_start"),
        Action::SearchNext => Some("search_next"),
        Action::SearchPrev => Some("search_prev"),
        Action::ExecuteCommand => Some("execute_command"),
        Action::Quit => Some("quit"),
        // `cmd:<command>` is a form of the value, not a name of its own.
        Action::RunCommand(_)
        | Action::InsertChar(_)
        | Action::Key(_)
        | Action::Resize(..)
        | Action::Clock(_)
        | Action::Paste(_)
        | Action::RoomsLoaded(_)
        | Action::FoldersLoaded(_)
        | Action::MeLoaded(_)
        | Action::OwnIdLoaded(_)
        | Action::FollowDone { .. }
        | Action::FollowFailed { .. }
        | Action::MarkReadDue
        | Action::MarkReadDone { .. }
        | Action::MarkThreadReadDue
        | Action::MarkThreadReadDone { .. }
        | Action::MessagesLoaded(..)
        | Action::LoadFailed(_)
        | Action::HistoryLoaded { .. }
        | Action::HistoryFailed { .. }
        | Action::ThreadLoaded { .. }
        | Action::ThreadLoadFailed { .. }
        | Action::ThreadListPage { .. }
        | Action::ThreadListSearchDue
        | Action::ThreadListParent { .. }
        | Action::SearchPage { .. }
        | Action::ContextLoaded { .. }
        | Action::ContextFailed { .. }
        | Action::NewerLoaded { .. }
        | Action::NewerFailed { .. }
        | Action::MessageSent { .. }
        | Action::SendFailed { .. }
        | Action::EditDone { .. }
        | Action::EditFailed { .. }
        | Action::DeleteDone { .. }
        | Action::DeleteFailed { .. }
        | Action::ReactFailed { .. }
        | Action::CustomEmojisLoaded(_)
        | Action::CustomEmojisFailed
        | Action::RecentsLoaded(_)
        | Action::EmojiImageReady { .. }
        | Action::EmojiImageFailed(_)
        | Action::AttachmentImageReady { .. }
        | Action::AttachmentImageFailed(_)
        | Action::AttachmentViewReady { .. }
        | Action::AttachmentViewFailed(_)
        | Action::AttachmentDownloaded { .. }
        | Action::AttachmentDownloadFailed { .. }
        | Action::AttachmentOpened(_)
        | Action::VideoPosterReady { .. }
        | Action::VideoPosterFailed { .. }
        | Action::VideoFrame { .. }
        | Action::VideoEnded { .. }
        | Action::VideoStarted { .. }
        | Action::VideoStartFailed { .. }
        | Action::RtConnected
        | Action::RtDisconnected
        | Action::RtRejected
        | Action::RtMessage(_)
        | Action::RtSubscription { .. }
        | Action::RtDeleted { .. }
        | Action::KeyTimeout
        | Action::Counted(..) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use std::collections::HashSet;

    #[test]
    fn names_round_trip() {
        let unique: HashSet<&&str> = ACTION_NAMES.iter().collect();
        assert_eq!(unique.len(), ACTION_NAMES.len(), "duplicate name");
        for name in ACTION_NAMES {
            let action = action_from_name(name).unwrap_or_else(|| panic!("{name} has no action"));
            assert_eq!(name_of(&action), Some(*name));
        }
    }

    #[test]
    fn video_names_round_trip_and_stay_out_of_the_normal_vocabulary() {
        let unique: HashSet<&&str> = VIDEO_ACTION_NAMES.iter().collect();
        assert_eq!(unique.len(), VIDEO_ACTION_NAMES.len(), "duplicate name");
        for name in VIDEO_ACTION_NAMES {
            let action =
                video_action_from_name(name).unwrap_or_else(|| panic!("{name} has no action"));
            assert_eq!(name_of(&action), Some(*name));
            assert!(action_from_name(name).is_none(), "{name} is a [video] name");
            assert!(!ACTION_NAMES.contains(name));
        }
        assert!(video_action_from_name("move_down").is_none());
        assert!(video_action_from_name("play_video").is_none());
    }

    #[test]
    fn internal_actions_unnamed() {
        let key = Action::Key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        for action in [
            Action::InsertChar('x'),
            key,
            Action::Resize(1, 1),
            Action::Clock(0),
            Action::Paste("p".to_string()),
            Action::RunCommand("quit".to_string()),
            Action::KeyTimeout,
            Action::EditDone {
                room: crate::data::RoomId("r".to_string()),
                id: "m".to_string(),
                message: crate::test_util::sample_message("m"),
            },
            Action::EditFailed {
                room: crate::data::RoomId("r".to_string()),
                id: "m".to_string(),
                error: crate::api::ApiError::Rejected,
            },
            Action::DeleteDone {
                room: crate::data::RoomId("r".to_string()),
                id: "m".to_string(),
            },
            Action::DeleteFailed {
                room: crate::data::RoomId("r".to_string()),
                id: "m".to_string(),
                error: crate::api::ApiError::Rejected,
            },
            Action::ReactFailed {
                error: crate::api::ApiError::Rejected,
            },
            Action::CustomEmojisLoaded(Vec::new()),
            Action::CustomEmojisFailed,
            Action::EmojiImageFailed(crate::effect::EmojiRef {
                name: "n".to_string(),
                etag: "e".to_string(),
                ext: "png".to_string(),
            }),
            Action::AttachmentViewFailed(crate::effect::AttachmentRef {
                path: "/file-upload/a/b.png".to_string(),
                size: None,
                dims: None,
                animate: false,
                source: None,
            }),
            Action::AttachmentImageFailed(crate::effect::AttachmentRef {
                path: "/file-upload/a/b.png".to_string(),
                size: None,
                dims: None,
                animate: false,
                source: None,
            }),
            Action::RtConnected,
            Action::RtDisconnected,
            Action::RtRejected,
            Action::RtSubscription {
                room: crate::data::RoomId("r".to_string()),
                update: crate::data::UnreadUpdate {
                    alert: Some(true),
                    ..Default::default()
                },
            },
            Action::RtDeleted {
                room: crate::data::RoomId("r".to_string()),
                id: "m".to_string(),
            },
            Action::Counted(2, Box::new(Action::MoveDown)),
        ] {
            assert_eq!(name_of(&action), None, "{action:?}");
        }
        assert!(action_from_name("none").is_none());
        assert!(action_from_name("insert_char").is_none());
        assert!(action_from_name("").is_none());
        assert!(action_from_name("MOVE_DOWN").is_none());
    }
}

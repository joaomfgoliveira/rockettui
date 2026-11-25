//! The thread pane: the open thread's parent and replies, drawn with the same rows as the main
//! flow and its own cursor. It draws only the window `app::thread::view` selects, from the same row
//! model `app::thread` scrolls with.

use crate::app::thread::{self, ThreadLoad};
use crate::app::{App, Focus};
use crate::ui::sanitize::sanitize_line;
use crate::ui::wrap;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// The pane block: the usual focused or unfocused border, with `thread_pane.border` laid over it.
fn block<'a>(app: &App, title: &'a str) -> ratatui::widgets::Block<'a> {
    let focused = app.focus == Focus::Thread;
    let theme = &app.theme;
    let base = if focused {
        theme.border_focused
    } else {
        theme.border_unfocused
    };
    let style = base.patch(theme.thread_pane_border);
    crate::ui::pane_block(title, focused, theme)
        .border_style(style)
        .title_style(style)
}

/// Draw the thread pane into `area`. Nothing is drawn while no thread is open.
pub fn render(frame: &mut Frame, area: Rect, app: &App) {
    let Some(pane) = app.thread.as_ref() else {
        return;
    };
    // The header says whether the user follows the thread.
    let following = app.own_user_id.as_deref().is_some_and(|id| {
        pane.messages
            .first()
            .is_some_and(|m| m.id == pane.parent_id && m.followed_by(id))
    });
    let title = if following {
        " Thread, following "
    } else {
        " Thread "
    };
    let block = block(app, title);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let notice = app.theme.messages_notice;

    // A first page in flight or failed leaves the parent (when held) with a line under it: the
    // failure class only, never message text.
    let status = match pane.load {
        ThreadLoad::Loading => Some("Loading…".to_string()),
        ThreadLoad::Failed(error) => {
            Some(sanitize_line(&format!("Could not load thread: {error}")).into_owned())
        }
        ThreadLoad::Loaded => None,
    };
    let msgs = pane.messages.as_slice();
    if msgs.is_empty() {
        let text = status.unwrap_or_else(|| "No messages".to_string());
        frame.render_widget(Paragraph::new(Line::styled(text, notice)), inner);
        return;
    }

    let v = thread::view(app);
    let ctx = app.thread_row_ctx(msgs);
    let mut lines: Vec<Line> = Vec::new();
    let mut cursor_rows = 0..0;
    for (i, msg) in msgs.iter().enumerate() {
        let prev = i.checked_sub(1).map(|p| &msgs[p]);
        lines.extend(wrap::divider_row(msg, prev, inner.width, &ctx));
        let rows = wrap::render_rows(msg, inner.width, &ctx);
        if i == v.cursor {
            cursor_rows = lines.len()..lines.len() + rows.len();
        }
        lines.extend(rows);
    }
    if let Some(text) = status {
        lines.push(Line::styled(text, notice));
    }
    let highlight = if app.focus == Focus::Thread {
        app.theme.messages_cursor
    } else {
        app.theme.messages_cursor_unfocused
    };
    let window: Vec<Line> = lines
        .into_iter()
        .skip(v.top_row)
        .take(usize::from(inner.height))
        .collect();
    frame.render_widget(Paragraph::new(window), inner);
    let first = cursor_rows.start.max(v.top_row);
    let end = cursor_rows.end.min(v.top_row + usize::from(inner.height));
    for row in first..end {
        let y = inner.y + u16::try_from(row - v.top_row).unwrap_or(u16::MAX);
        frame
            .buffer_mut()
            .set_style(Rect::new(inner.x, y, inner.width, 1), highlight);
        crate::ui::emoji::keep_image_ids(frame.buffer_mut(), Rect::new(inner.x, y, inner.width, 1));
    }
}

#[cfg(test)]
mod tests {
    use crate::action::Action;
    use crate::api::ThreadPage;
    use crate::app::update::update;
    use crate::app::{App, Focus};
    use crate::data::{Message, MessageKind, RoomId, SendState};
    use crate::test_util::{press, render_buffer, render_to_string};
    use ratatui::layout::Size;
    use ratatui::style::Modifier;

    fn msg(id: &str, ts: i64, author: &str, body: &str) -> Message {
        Message {
            id: id.to_string(),
            room_id: RoomId("quiet".to_string()),
            author: author.to_string(),
            ts,
            body: body.to_string(),
            kind: MessageKind::Normal,
            send: SendState::Sent,
            edited: false,
            plain_text: true,
            reactions: Vec::new(),
            tmid: None,
            tshow: false,
            tcount: 0,
            tlm: None,
            followers: None,
        }
    }

    /// The fake-data app at `w` x `h` with `quiet` open and a thread of the newest message open and
    /// loaded: a parent with two replies, the last one also shown in the channel.
    fn app_with_thread(w: u16, h: u16) -> App {
        let mut app = App::with_fake_data(Size::new(w, h));
        let room = RoomId("quiet".to_string());
        app.messages.insert(
            room.clone(),
            vec![
                msg("0", 30, "bob", "before the thread"),
                Message {
                    tcount: 2,
                    tlm: Some(660),
                    followers: None,
                    ..msg("1", 60, "alice", "deploy plan: who takes the rollback?")
                },
            ],
        );
        app.open_room(room.clone());
        press(&mut app, "gt");
        assert!(app.thread.is_some(), "the thread opened");
        update(
            &mut app,
            Action::ThreadLoaded {
                room,
                parent_id: "1".to_string(),
                offset: 0,
                parent: None,
                page: ThreadPage {
                    messages: vec![
                        Message {
                            tmid: Some("1".to_string()),
                            ..msg("2", 120, "bob", "I can take it")
                        },
                        Message {
                            tmid: Some("1".to_string()),
                            tshow: true,
                            ..msg("3", 660, "alice", "thanks, done")
                        },
                    ],
                    total: 2,
                },
            },
        );
        app
    }

    #[test]
    fn the_thread_sits_beside_the_messages_at_160_columns() {
        let app = app_with_thread(160, 24);
        let out = render_to_string(&app, 160, 24);
        assert!(out.contains("Thread"), "{out}");
        assert!(out.contains("I can take it"), "{out}");
        assert!(
            out.contains("before the thread"),
            "the message pane is still drawn: {out}"
        );
        insta::assert_snapshot!(out);
    }

    #[test]
    fn each_pane_has_its_own_input_bar_beside_the_messages() {
        let mut app = app_with_thread(160, 24);
        app.focus = Focus::Thread;
        press(&mut app, "ia thread reply<C-b>");
        let out = render_to_string(&app, 160, 24);
        assert!(out.contains("Reply in thread, also to channel"), "{out}");
        assert!(out.contains("a thread reply"), "{out}");
        assert!(out.contains("Input"), "the room bar stays: {out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn the_room_draft_and_the_thread_draft_show_at_once() {
        let mut app = app_with_thread(160, 24);
        app.focus = Focus::Messages;
        press(&mut app, "iroom draft<Esc>");
        app.focus = Focus::Thread;
        press(&mut app, "ithread draft<Esc>");
        let out = render_to_string(&app, 160, 24);
        assert!(out.contains("room draft"), "{out}");
        assert!(out.contains("thread draft"), "{out}");
    }

    #[test]
    fn one_shared_bar_names_the_thread_when_it_takes_the_pane_at_80_columns() {
        let mut app = app_with_thread(80, 24);
        app.focus = Focus::Thread;
        press(&mut app, "ihi");
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Reply in thread"), "{out}");
        assert!(!out.contains("Input"), "{out}");
    }

    #[test]
    fn a_failed_reply_is_drawn_in_the_pane_with_the_failed_marker() {
        let mut app = app_with_thread(160, 24);
        app.focus = Focus::Thread;
        press(&mut app, "inot delivered<CR>");
        let id = app
            .thread
            .as_ref()
            .and_then(|p| p.messages.last())
            .map(|m| m.id.clone())
            .expect("the pending reply");
        update(
            &mut app,
            Action::SendFailed {
                room: RoomId("quiet".to_string()),
                id,
                error: crate::api::ApiError::Rejected,
            },
        );
        let out = render_to_string(&app, 160, 24);
        assert!(out.contains("not delivered"), "{out}");
        assert!(out.contains(&app.theme.markers.failed), "{out}");
        insta::assert_snapshot!(out);
    }

    /// The 160-column app with the parent's follower list and the own id set.
    fn followed_thread_app(followers: Vec<&str>) -> App {
        let mut app = app_with_thread(160, 24);
        app.own_user_id = Some("me".to_string());
        let list: Vec<String> = followers.into_iter().map(str::to_string).collect();
        for m in app
            .messages
            .get_mut(&RoomId("quiet".to_string()))
            .into_iter()
            .flatten()
            .chain(app.thread.iter_mut().flat_map(|p| p.messages.iter_mut()))
            .filter(|m| m.tcount > 0)
        {
            m.followers = Some(list.clone());
        }
        app
    }

    #[test]
    fn a_followed_thread_says_following_in_the_marker_row_and_the_header() {
        let app = followed_thread_app(vec!["me"]);
        let out = render_to_string(&app, 160, 24);
        assert!(out.contains("Thread, following"), "{out}");
        assert!(out.contains("replies, last 00:11, following"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn an_unfollowed_thread_has_no_follow_text() {
        let app = followed_thread_app(vec!["other"]);
        let out = render_to_string(&app, 160, 24);
        assert!(!out.contains("following"), "{out}");
        insta::assert_snapshot!(out);
    }

    #[test]
    fn the_thread_takes_the_message_pane_at_80_columns() {
        let app = app_with_thread(80, 24);
        let out = render_to_string(&app, 80, 24);
        assert!(out.contains("Thread"), "{out}");
        assert!(out.contains("I can take it"), "{out}");
        assert!(
            !out.contains("before the thread"),
            "the message pane is replaced: {out}"
        );
        insta::assert_snapshot!(out);
    }

    #[test]
    fn closing_brings_the_message_pane_back_unchanged() {
        let mut app = app_with_thread(80, 24);
        let open = render_to_string(&app, 80, 24);
        assert!(!open.contains("before the thread"));
        let before = {
            let mut closed = app_with_thread(80, 24);
            closed.thread = None;
            closed.focus = Focus::Messages;
            render_to_string(&closed, 80, 24)
        };
        press(&mut app, "<Esc>");
        assert!(app.thread.is_none());
        assert_eq!(render_to_string(&app, 80, 24), before);
        assert!(before.contains("before the thread"));
    }

    #[test]
    fn a_loading_and_a_failed_thread_say_so_without_message_text() {
        let mut app = app_with_thread(160, 24);
        app.thread.as_mut().unwrap().load = crate::app::thread::ThreadLoad::Loading;
        assert!(render_to_string(&app, 160, 24).contains("Loading"));
        app.thread.as_mut().unwrap().load =
            crate::app::thread::ThreadLoad::Failed(crate::api::ApiError::Rejected);
        let out = render_to_string(&app, 160, 24);
        assert!(out.contains("Could not load thread"), "{out}");
    }

    #[test]
    fn the_focused_pane_border_differs_from_the_unfocused_one() {
        let mut app = app_with_thread(160, 24);
        assert_eq!(app.focus, Focus::Thread);
        let focused = render_to_string(&app, 160, 24);
        app.focus = Focus::Messages;
        let unfocused = render_to_string(&app, 160, 24);
        assert_ne!(focused, unfocused, "thick borders mark the focused pane");
    }

    #[test]
    fn the_cursor_of_the_pane_is_highlighted_and_the_pane_border_is_themed() {
        let mut app = app_with_thread(160, 24);
        let buffer = render_buffer(&app, 160, 24);
        // The pane's cursor is on its first message (the parent): one row is reversed in the pane.
        let thread = app.areas().thread.expect("open");
        let reversed_rows = (thread.y..thread.bottom())
            .filter(|&y| {
                buffer[(thread.x + 3, y)]
                    .modifier
                    .contains(Modifier::REVERSED)
            })
            .count();
        assert!(reversed_rows >= 1, "the cursor row is highlighted");
        // A theme.toml colour on thread_pane.border reaches the border cells.
        app.theme.thread_pane_border =
            ratatui::style::Style::default().fg(ratatui::style::Color::Magenta);
        let buffer = render_buffer(&app, 160, 24);
        assert_eq!(
            buffer[(thread.x, thread.y)].fg,
            ratatui::style::Color::Magenta
        );
    }

    #[test]
    fn no_size_panics_with_a_thread_open() {
        for w in [0u16, 1, 10, 39, 40, 41, 80, 99, 100, 101, 160, 300] {
            for h in [0u16, 1, 7, 8, 9, 24, 60] {
                let mut app = app_with_thread(80, 24);
                app.size = Size::new(w, h);
                for focus in [Focus::Thread, Focus::Messages, Focus::Input, Focus::Sidebar] {
                    app.focus = focus;
                    let _ = render_to_string(&app, w, h);
                }
                // Motions and a resize in every size do not panic either.
                app.focus = Focus::Thread;
                for keys in ["j", "k", "G", "gg", "<C-d>", "<C-u>"] {
                    press(&mut app, keys);
                }
            }
        }
    }
}

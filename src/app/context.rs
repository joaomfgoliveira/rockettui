//! The context popup's content: what the selected object can do, with the keys of the live keymap.
//!
//! Pure: no I/O and no `Frame`; `ui::context` draws it. The object kinds use the same predicates
//! the actions themselves use, so the popup does not promise an action that would be a silent
//! no-op. Only built-in text is shown, never a message, an author or a room name.

use crate::app::{App, Focus, Mode, channels, edit, react, reactions, send, sidebar, thread};
use crate::input::actions::name_of;
use crate::input::key::format_keys;

/// What is selected, as far as the available actions are concerned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Room,
    Folder,
    Header,
    OwnMessage,
    UnsentMessage,
    OtherMessage,
    Input,
    /// The `:search` results list.
    SearchResults,
    /// The context view of one search result.
    SearchContext,
    /// The thread pane.
    Thread,
    /// The thread list pane.
    ThreadList,
    Nothing,
}

impl Kind {
    /// The name line of the popup.
    pub fn title(self) -> &'static str {
        match self {
            Kind::Room => "Room",
            Kind::Folder => "Folder",
            Kind::Header => "Section header",
            Kind::OwnMessage => "Your message",
            Kind::UnsentMessage => "Your unsent message",
            Kind::OtherMessage => "Message",
            Kind::Input => "Input bar",
            Kind::SearchResults => "Search results",
            Kind::SearchContext => "Search result in context",
            Kind::Thread => "Thread",
            Kind::ThreadList => "Thread list",
            Kind::Nothing => "Nothing selected",
        }
    }

    /// The action names listed for the kind, with the wording used in the popup. A test checks
    /// every name is a real action.
    pub fn actions(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Kind::Room => &[("select", "open the room")],
            Kind::Folder => &[
                ("select", "open or close the folder"),
                ("toggle_fold", "fold or unfold the folder"),
            ],
            Kind::OwnMessage => &[
                ("edit_message", "edit the message"),
                ("delete_message", "delete the message"),
                ("goto_top", "jump to the oldest message"),
                ("goto_bottom", "jump to the newest message"),
                ("search_start", "find text in the room"),
                ("search_next", "next match, older"),
                ("search_prev", "previous match, newer"),
            ],
            Kind::UnsentMessage => &[
                ("retry_send", "retry sending"),
                ("discard_send", "discard it"),
                ("delete_message", "discard it"),
                ("goto_top", "jump to the oldest message"),
                ("goto_bottom", "jump to the newest message"),
                ("search_start", "find text in the room"),
                ("search_next", "next match, older"),
                ("search_prev", "previous match, newer"),
            ],
            Kind::OtherMessage => &[
                ("goto_top", "jump to the oldest message"),
                ("goto_bottom", "jump to the newest message"),
                ("search_start", "find text in the room"),
                ("search_next", "next match, older"),
                ("search_prev", "previous match, newer"),
            ],
            Kind::Input => &[
                ("enter_insert", "type a message (Insert mode)"),
                ("move_left", "move left in the draft"),
                ("move_right", "move right in the draft"),
                ("move_up", "move up in the draft"),
                ("move_down", "move down in the draft"),
            ],
            Kind::SearchResults => &[
                ("select", "open the result in context"),
                ("move_down", "next result"),
                ("move_up", "previous result"),
                ("goto_top", "newest result"),
                ("goto_bottom", "oldest result (loads more)"),
                ("half_page_down", "half a page down"),
                ("half_page_up", "half a page up"),
            ],
            Kind::SearchContext => &[
                ("select", "open in the chat"),
                ("move_down", "next message"),
                ("move_up", "previous message"),
                ("goto_top", "first message"),
                ("goto_bottom", "last message"),
                ("half_page_down", "half a page down"),
                ("half_page_up", "half a page up"),
            ],
            Kind::Thread => &[
                ("close_thread", "close the thread"),
                ("reply_in_thread", "reply in this thread"),
                ("toggle_follow_thread", "follow or unfollow the thread"),
                LIST_THREADS_ACTION,
                ("move_down", "next message"),
                ("move_up", "previous message"),
                ("goto_top", "first message"),
                ("goto_bottom", "last message"),
                ("half_page_down", "half a page down"),
                ("half_page_up", "half a page up"),
            ],
            Kind::ThreadList => &[
                ("select", "open the thread"),
                ("search_start", "search the threads"),
                ("move_right", "next filter"),
                ("move_left", "previous filter"),
                ("move_down", "next thread"),
                ("move_up", "previous thread"),
                ("goto_top", "first thread"),
                ("goto_bottom", "last thread"),
                ("half_page_down", "half a page down"),
                ("half_page_up", "half a page up"),
            ],
            Kind::Header | Kind::Nothing => &[],
        }
    }

    /// Fixed keys that are not keymap actions (`app::results::key` handles them before the
    /// keymap), shown after the action rows.
    fn fixed_rows(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Kind::SearchResults => &[("q Esc", "close the list")],
            Kind::SearchContext => &[("q Esc", "back to the results")],
            Kind::ThreadList => &[("q Esc", "close the list")],
            _ => &[],
        }
    }
}

/// The thread action: listed on a message that has a thread (replies, or a reply shown in the
/// channel), first among the actions that depend on the message (`lines` places it). A test checks
/// the name is a real action.
const THREAD_ACTION: (&str, &str) = ("open_thread", "open the thread");

/// The follow action: listed on a message that has a thread, right after the thread action. A test
/// checks the name is a real action.
const FOLLOW_ACTION: (&str, &str) = ("toggle_follow_thread", "follow or unfollow the thread");

/// The reply action: listed on a message that can be replied to (not a system message, not a
/// deleted one, not one still being sent), right after the thread action. A test checks the name is
/// a real action.
const REPLY_ACTION: (&str, &str) = ("reply_in_thread", "reply in its thread");

/// The react action: listed on a message the user can react to, after the message's own actions
/// and before the reactions row and the jumps (`lines` places it). Not part of `Kind::actions`
/// because it depends on the message, not on its kind. A test checks the name is a real action.
const REACT_ACTION: (&str, &str) = ("react_message", "add or remove your reaction");

/// The reactions action: listed on a message that has reactions, after its own actions and before
/// the jumps (`lines` places it). Not part of `Kind::actions` because it depends on the message,
/// not on its kind. A test checks the name is a real action.
const REACTIONS_ACTION: (&str, &str) = ("show_reactions", "show who reacted");

/// The open-channel action: listed on a message that references a channel, after the reactions
/// action and before the jumps (`lines` places it). A test checks the name is a real action.
const CHANNEL_ACTION: (&str, &str) = ("open_channel", "open the channel it names");

/// The thread list action: listed with the thread actions, on a message that has a thread and in
/// the thread pane. It is about the room, not the message, but the popup has no room for it on
/// every message. A test checks the name is a real action.
const LIST_THREADS_ACTION: (&str, &str) = ("list_threads", "list this room's threads");

/// One row of the popup: the keys bound to an action and what it does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextLine {
    pub keys: String,
    pub desc: String,
}

/// What the popup shows: the object name, the action rows, and the way to the full help last.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub kind: Kind,
    pub rows: Vec<ContextLine>,
    pub full_help: ContextLine,
}

/// The kind of the selected object. Follows `App.focus` only, as the motions do. An unsent row is
/// classified before an own server message, mirroring `delete_message`.
pub fn classify(app: &App) -> Kind {
    // An open results list or context view takes the keys whatever pane is focused.
    if let Some(list) = app.results.as_ref() {
        return if list.context.is_some() {
            Kind::SearchContext
        } else {
            Kind::SearchResults
        };
    }
    match app.focus {
        Focus::Sidebar => match sidebar::selected_row(app) {
            Some(sidebar::SidebarRow::Room { .. }) => Kind::Room,
            Some(sidebar::SidebarRow::Folder { .. }) => Kind::Folder,
            Some(sidebar::SidebarRow::Header(_)) => Kind::Header,
            None => Kind::Nothing,
        },
        Focus::Input => Kind::Input,
        Focus::Thread => Kind::Thread,
        Focus::ThreadList => Kind::ThreadList,
        Focus::Messages => {
            if send::unsent_under_cursor(app).is_some() {
                Kind::UnsentMessage
            } else if edit::own_under_cursor(app).is_some() {
                Kind::OwnMessage
            } else if message_under_cursor(app) {
                Kind::OtherMessage
            } else {
                Kind::Nothing
            }
        }
    }
}

/// True when a room is open and holds a message for the cursor to be on.
fn message_under_cursor(app: &App) -> bool {
    app.current_room
        .as_ref()
        .and_then(|room| app.messages.get(room))
        .is_some_and(|held| !held.is_empty())
}

/// The Normal-mode keys bound to `name`, sorted and joined by a space. `cmd:` rows never match
/// because they have no action name.
pub fn keys_for(app: &App, name: &str) -> Option<String> {
    let mut keys: Vec<String> = app
        .keymap
        .bindings(Mode::Normal)
        .into_iter()
        .filter(|(_, action)| name_of(action) == Some(name))
        .map(|(chords, _)| format_keys(&chords))
        .collect();
    if keys.is_empty() {
        return None;
    }
    keys.sort();
    Some(keys.join(" "))
}

/// The popup content for the current selection, recomputed on every draw.
pub fn lines(app: &App) -> Context {
    let kind = classify(app);
    let mut rows: Vec<ContextLine> = Vec::new();
    // Right before the first jump, a message lists the actions that depend on the message and
    // not on its kind: reacting to it (when it can be reacted to), then showing its reactions
    // (when it has any).
    let is_message = matches!(
        kind,
        Kind::OwnMessage | Kind::UnsentMessage | Kind::OtherMessage
    );
    let mut message_actions: Vec<(&str, &str)> = Vec::new();
    if is_message && thread::under_cursor(app).is_some() {
        message_actions.push(THREAD_ACTION);
        message_actions.push(FOLLOW_ACTION);
        message_actions.push(LIST_THREADS_ACTION);
    }
    if is_message && thread::replyable_under_cursor(app) {
        message_actions.push(REPLY_ACTION);
    }
    if is_message && react::reactable_under_cursor(app) {
        message_actions.push(REACT_ACTION);
    }
    if is_message && reactions::under_cursor(app).is_some() {
        message_actions.push(REACTIONS_ACTION);
    }
    if is_message && channels::under_cursor(app).is_some() {
        message_actions.push(CHANNEL_ACTION);
    }
    let mut message_actions_pending = !message_actions.is_empty();
    for (name, desc) in kind.actions() {
        if message_actions_pending && *name == "goto_top" {
            message_actions_pending = false;
            for (extra_name, extra_desc) in &message_actions {
                if let Some(keys) = keys_for(app, extra_name) {
                    rows.push(ContextLine {
                        keys,
                        desc: (*extra_desc).to_string(),
                    });
                }
            }
        }
        let Some(keys) = keys_for(app, name) else {
            continue;
        };
        // Two names can share a wording (`discard_send` and `delete_message`): keep one row per
        // wording, with every key.
        if let Some(row) = rows.iter_mut().find(|r| r.desc == *desc) {
            row.keys = format!("{} {keys}", row.keys);
        } else {
            rows.push(ContextLine {
                keys,
                desc: (*desc).to_string(),
            });
        }
    }
    // `Esc` closes the focused thread pane: it is `cancel`, so every key bound to it counts with
    // the keys of `close_thread` itself.
    if kind == Kind::Thread {
        let mut keys: Vec<String> = ["cancel", "close_thread"]
            .iter()
            .filter_map(|name| keys_for(app, name))
            .collect();
        keys.sort();
        if let Some(row) = rows.iter_mut().find(|r| r.desc == "close the thread") {
            row.keys = keys.join(" ");
        } else {
            rows.insert(
                0,
                ContextLine {
                    keys: keys.join(" "),
                    desc: "close the thread".to_string(),
                },
            );
        }
    }
    rows.extend(kind.fixed_rows().iter().map(|(keys, desc)| ContextLine {
        keys: (*keys).to_string(),
        desc: (*desc).to_string(),
    }));
    let keys = keys_for(app, "show_context")
        .or_else(|| keys_for(app, "show_help"))
        .unwrap_or_else(|| ":help".to_string());
    Context {
        kind,
        rows,
        full_help: ContextLine {
            keys,
            desc: "all keys (full help)".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{MessageKind, SendState};
    use crate::input::actions::ACTION_NAMES;
    use crate::test_util::{app_80x24, app_with_config, config_from_keymap, press, sample_message};

    fn descs(ctx: &Context) -> Vec<&str> {
        ctx.rows.iter().map(|r| r.desc.as_str()).collect()
    }

    /// An app with the messages pane focused on a room holding one message by `author`.
    fn messages_app(author: &str, send: SendState) -> App {
        messages_app_in(app_80x24(), author, send)
    }

    /// `messages_app` on a given app (for one built with a config).
    fn messages_app_in(mut app: App, author: &str, send: SendState) -> App {
        let room = app.rooms[0].id.clone();
        let mut m = sample_message("m-ctx");
        m.room_id = room.clone();
        m.author = author.to_string();
        m.send = send;
        m.ts = i64::MAX / 4;
        app.messages.insert(room.clone(), vec![m]);
        app.msg_views.remove(&room);
        app.current_room = Some(room);
        app.own_username = Some("me".to_string());
        app.focus = Focus::Messages;
        app
    }

    #[test]
    fn room_row() {
        let mut app = app_80x24();
        while !matches!(
            sidebar::selected_row(&app),
            Some(sidebar::SidebarRow::Room { .. })
        ) {
            press(&mut app, "j");
        }
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Room);
        assert_eq!(descs(&ctx), ["open the room"]);
        assert_eq!(ctx.rows[0].keys, "<CR>");
        assert_eq!(ctx.full_help.keys, "?");
    }

    #[test]
    fn folder_row() {
        let mut app = app_80x24();
        let mut n = 0;
        while !matches!(
            sidebar::selected_row(&app),
            Some(sidebar::SidebarRow::Folder { .. })
        ) {
            press(&mut app, "j");
            n += 1;
            assert!(n < 50, "no folder row in the fixture");
        }
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Folder);
        assert_eq!(
            descs(&ctx),
            ["open or close the folder", "fold or unfold the folder"]
        );
    }

    #[test]
    fn header_row() {
        let mut app = app_80x24();
        app.sidebar.cursor = 0;
        assert!(matches!(
            sidebar::selected_row(&app),
            Some(sidebar::SidebarRow::Header(_))
        ));
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Header);
        assert!(ctx.rows.is_empty());
    }

    #[test]
    fn own_message() {
        let app = messages_app("me", SendState::Sent);
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::OwnMessage);
        assert_eq!(
            descs(&ctx),
            [
                "edit the message",
                "delete the message",
                "reply in its thread",
                "add or remove your reaction",
                "jump to the oldest message",
                "jump to the newest message",
                "find text in the room",
                "next match, older",
                "previous match, newer"
            ]
        );
        assert_eq!(ctx.rows[0].keys, "e");
        assert_eq!(ctx.rows[1].keys, "dd");
        assert_eq!(ctx.rows[2].keys, "gR");
        assert_eq!(ctx.rows[3].keys, "+");
        assert_eq!(ctx.rows[4].keys, "gg");
        assert_eq!(ctx.rows[5].keys, "G");
        assert_eq!(ctx.rows[6].keys, "/");
        assert_eq!(ctx.rows[7].keys, "n");
        assert_eq!(ctx.rows[8].keys, "N");
    }

    #[test]
    fn unsent_message() {
        let app = messages_app("me", SendState::Failed);
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::UnsentMessage);
        assert_eq!(
            &descs(&ctx)[..2],
            ["retry sending", "discard it"],
            "the search rows come after the unsent ones"
        );
        assert!(!descs(&ctx).contains(&"edit the message"));
        assert!(descs(&ctx).contains(&"find text in the room"));
        assert_eq!(ctx.rows[0].keys, "r");
        assert_eq!(ctx.rows[1].keys, "dd");
    }

    /// Give the one message of `app` a reaction from one user.
    fn react(app: &mut App) {
        let room = app.current_room.clone().expect("a room");
        app.messages.get_mut(&room).unwrap()[0]
            .reactions
            .push(crate::data::Reaction {
                name: "+1".to_string(),
                users: vec!["bob".to_string()],
            });
    }

    #[test]
    fn reactions_row_on_a_message_with_reactions() {
        let mut app = messages_app("someone-else", SendState::Sent);
        react(&mut app);
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::OtherMessage);
        assert_eq!(
            descs(&ctx),
            [
                "reply in its thread",
                "add or remove your reaction",
                "show who reacted",
                "jump to the oldest message",
                "jump to the newest message",
                "find text in the room",
                "next match, older",
                "previous match, newer"
            ]
        );
        assert_eq!(ctx.rows[0].keys, "gR");
        assert_eq!(ctx.rows[1].keys, "+");
        assert_eq!(ctx.rows[2].keys, "gr");
    }

    #[test]
    fn reactions_row_comes_after_the_own_actions_and_before_the_jumps() {
        let mut app = messages_app("me", SendState::Sent);
        react(&mut app);
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::OwnMessage);
        assert_eq!(
            descs(&ctx)[..5],
            [
                "edit the message",
                "delete the message",
                "reply in its thread",
                "add or remove your reaction",
                "show who reacted"
            ]
        );
    }

    #[test]
    fn no_reactions_row_without_reactions_or_off_a_message() {
        let app = messages_app("someone-else", SendState::Sent);
        assert!(!descs(&lines(&app)).contains(&"show who reacted"));
        // Reactions on a message do not show in the popup of another object.
        let mut app = messages_app("someone-else", SendState::Sent);
        react(&mut app);
        app.focus = Focus::Input;
        assert!(!descs(&lines(&app)).contains(&"show who reacted"));
        app.focus = Focus::Sidebar;
        assert!(!descs(&lines(&app)).contains(&"show who reacted"));
    }

    #[test]
    fn reactions_row_follows_the_live_keymap() {
        let rebound = app_with_config(
            config_from_keymap("[normal]\n\"R\" = \"show_reactions\"\n"),
            80,
            24,
        );
        let mut app = messages_app_in(rebound, "someone-else", SendState::Sent);
        react(&mut app);
        // Both the new key and the preset key are bound; every key is listed, sorted.
        assert_eq!(lines(&app).rows[2].keys, "R gr");

        let unbound = app_with_config(config_from_keymap("[normal]\n\"gr\" = \"none\"\n"), 80, 24);
        let mut app = messages_app_in(unbound, "someone-else", SendState::Sent);
        react(&mut app);
        assert!(
            !descs(&lines(&app)).contains(&"show who reacted"),
            "an action with no key bound is left out"
        );
    }

    /// Make the only message of `app` reference `names` as channels.
    fn reference(app: &mut App, names: &[&str]) {
        let room = app.current_room.clone().expect("a room");
        app.messages.get_mut(&room).unwrap()[0].refs = Some(crate::data::Refs {
            mentions: Vec::new(),
            channels: names.iter().map(|s| s.to_string()).collect(),
        });
    }

    #[test]
    fn open_channel_row_on_a_message_that_references_a_channel() {
        let mut app = messages_app("someone-else", SendState::Sent);
        reference(&mut app, &["dev-chat"]);
        let ctx = lines(&app);
        let at = descs(&ctx)
            .iter()
            .position(|d| *d == "open the channel it names")
            .expect("a row for the channel");
        assert_eq!(ctx.rows[at].keys, "gc");
        // Before the jumps, after the message's own actions.
        let jump = descs(&ctx)
            .iter()
            .position(|d| *d == "jump to the oldest message")
            .unwrap();
        assert_eq!(at + 1, jump);
    }

    #[test]
    fn no_open_channel_row_without_a_channel_or_off_a_message() {
        let has = |app: &App| descs(&lines(app)).contains(&"open the channel it names");
        let mut app = messages_app("someone-else", SendState::Sent);
        assert!(!has(&app), "no lists at all");
        reference(&mut app, &[]);
        assert!(!has(&app), "empty lists");
        // Mentions alone are not a channel.
        let room = app.current_room.clone().unwrap();
        app.messages.get_mut(&room).unwrap()[0]
            .refs
            .as_mut()
            .unwrap()
            .mentions
            .push("bob".to_string());
        assert!(!has(&app), "a mention is not a channel");
        reference(&mut app, &["dev-chat"]);
        assert!(has(&app));
        app.focus = Focus::Input;
        assert!(!has(&app));
        app.focus = Focus::Sidebar;
        assert!(!has(&app));
    }

    #[test]
    fn open_channel_row_follows_the_live_keymap() {
        let rebound = app_with_config(
            config_from_keymap("[normal]\n\"C\" = \"open_channel\"\n"),
            80,
            24,
        );
        let mut app = messages_app_in(rebound, "someone-else", SendState::Sent);
        reference(&mut app, &["dev-chat"]);
        let ctx = lines(&app);
        let row = ctx
            .rows
            .iter()
            .find(|r| r.desc == "open the channel it names")
            .expect("row");
        assert_eq!(row.keys, "C gc", "every key, sorted");

        let unbound = app_with_config(config_from_keymap("[normal]\n\"gc\" = \"none\"\n"), 80, 24);
        let mut app = messages_app_in(unbound, "someone-else", SendState::Sent);
        reference(&mut app, &["dev-chat"]);
        assert!(
            !descs(&lines(&app)).contains(&"open the channel it names"),
            "an action with no key bound is left out"
        );
    }

    #[test]
    fn the_open_channel_action_is_a_real_described_action() {
        let (name, desc) = CHANNEL_ACTION;
        assert!(ACTION_NAMES.contains(&name));
        assert!(!desc.is_empty());
        assert!(
            crate::app::help::DESCRIPTIONS
                .iter()
                .any(|(n, _)| *n == name),
            "{name} has no help description"
        );
    }

    #[test]
    fn react_row_on_a_message_the_user_can_react_to() {
        // Any server message, with or without reactions, and the user's own as well.
        for author in ["someone-else", "me"] {
            let app = messages_app(author, SendState::Sent);
            let ctx = lines(&app);
            let at = descs(&ctx)
                .iter()
                .position(|d| *d == "add or remove your reaction")
                .unwrap_or_else(|| panic!("no react row for {author}"));
            assert_eq!(ctx.rows[at].keys, "+");
            // Before the jumps, after the message's own actions.
            let jump = descs(&ctx)
                .iter()
                .position(|d| *d == "jump to the oldest message")
                .unwrap();
            assert_eq!(at + 1, jump, "{author}");
        }
    }

    #[test]
    fn no_react_row_where_the_user_cannot_react() {
        let has = |app: &App| descs(&lines(app)).contains(&"add or remove your reaction");
        // A deleted line and a system message.
        for kind in [MessageKind::Deleted, MessageKind::System] {
            let mut app = messages_app("someone-else", SendState::Sent);
            let room = app.current_room.clone().unwrap();
            app.messages.get_mut(&room).unwrap()[0].kind = kind;
            assert!(!has(&app), "{kind:?}");
        }
        // A message still being sent or that failed.
        for send in [
            SendState::Sending,
            SendState::Failed,
            SendState::Unconfirmed,
        ] {
            assert!(!has(&messages_app("me", send)), "{send:?}");
        }
        // Other panes.
        let mut app = messages_app("someone-else", SendState::Sent);
        app.focus = Focus::Input;
        assert!(!has(&app));
        app.focus = Focus::Sidebar;
        assert!(!has(&app));
    }

    #[test]
    fn react_row_follows_the_live_keymap() {
        let rebound = app_with_config(
            config_from_keymap("[normal]\n\"R\" = \"react_message\"\n"),
            80,
            24,
        );
        let app = messages_app_in(rebound, "someone-else", SendState::Sent);
        let ctx = lines(&app);
        let row = ctx
            .rows
            .iter()
            .find(|r| r.desc == "add or remove your reaction")
            .unwrap();
        assert_eq!(row.keys, "+ R", "every key is listed, sorted");

        let unbound = app_with_config(config_from_keymap("[normal]\n\"+\" = \"none\"\n"), 80, 24);
        let app = messages_app_in(unbound, "someone-else", SendState::Sent);
        assert!(
            !descs(&lines(&app)).contains(&"add or remove your reaction"),
            "an action with no key bound is left out"
        );
    }

    /// `messages_app` with the one message turned into a thread parent with `replies` replies.
    fn thread_app(replies: u32) -> App {
        let mut app = messages_app("someone-else", SendState::Sent);
        let room = app.current_room.clone().unwrap();
        app.messages.get_mut(&room).unwrap()[0].tcount = replies;
        app
    }

    #[test]
    fn the_thread_row_is_on_a_parent_and_a_channel_visible_reply_only() {
        let ctx = lines(&thread_app(3));
        assert_eq!(descs(&ctx)[0], "open the thread");
        assert_eq!(ctx.rows[0].keys, "gt");
        // A reply also shown in the channel opens its parent's thread.
        let mut app = thread_app(0);
        let room = app.current_room.clone().unwrap();
        app.messages.get_mut(&room).unwrap()[0].tmid = Some("parent".to_string());
        assert_eq!(lines(&app).rows[0].desc, "open the thread");
        // A plain message has none.
        let ctx = lines(&thread_app(0));
        assert!(
            !descs(&ctx).contains(&"open the thread"),
            "{:?}",
            descs(&ctx)
        );
    }

    #[test]
    fn the_thread_row_follows_the_live_keymap() {
        let app = messages_app_in(
            app_with_config(config_from_keymap("[normal]\n\"gt\" = \"none\"\n"), 80, 24),
            "someone-else",
            SendState::Sent,
        );
        let mut app = app;
        let room = app.current_room.clone().unwrap();
        app.messages.get_mut(&room).unwrap()[0].tcount = 2;
        assert!(
            !descs(&lines(&app)).contains(&"open the thread"),
            "an action with no key bound is left out"
        );
        // Rebound to another key, the row shows that key.
        let mut app = messages_app_in(
            app_with_config(
                config_from_keymap("[normal]\n\"T\" = \"open_thread\"\n"),
                80,
                24,
            ),
            "someone-else",
            SendState::Sent,
        );
        let room = app.current_room.clone().unwrap();
        app.messages.get_mut(&room).unwrap()[0].tcount = 2;
        assert_eq!(lines(&app).rows[0].keys, "T gt");
    }

    #[test]
    fn the_thread_list_pane_lists_its_own_keys() {
        let mut app = thread_app(3);
        crate::app::thread_list::open(&mut app);
        assert_eq!(app.focus, Focus::ThreadList);
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::ThreadList);
        for desc in ["open the thread", "search the threads", "next thread"] {
            assert!(descs(&ctx).contains(&desc), "{desc}");
        }
        for desc in ["next filter", "previous filter"] {
            assert!(descs(&ctx).contains(&desc), "{desc}");
        }
        let next = ctx
            .rows
            .iter()
            .find(|r| r.desc == "next filter")
            .expect("row");
        assert!(next.keys.split(' ').any(|k| k == "l"), "{next:?}");
        assert!(
            ctx.rows
                .iter()
                .any(|r| r.keys == "q Esc" && r.desc == "close the list")
        );
    }

    #[test]
    fn the_pane_lists_closing_the_thread_with_esc() {
        let mut app = thread_app(3);
        app.focus = Focus::Thread;
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Thread);
        assert_eq!(ctx.rows[0].desc, "close the thread");
        assert!(ctx.rows[0].keys.contains("<Esc>"), "{:?}", ctx.rows[0]);
        assert!(!descs(&ctx).contains(&"open the thread"));
        // A key bound to `close_thread` joins the list.
        let mut app = thread_app(3);
        app.keymap = app_with_config(
            config_from_keymap("[normal]\n\"Q\" = \"close_thread\"\n"),
            80,
            24,
        )
        .keymap;
        app.focus = Focus::Thread;
        assert!(lines(&app).rows[0].keys.contains('Q'));
    }

    #[test]
    fn list_threads_is_listed_in_the_pane_and_on_a_message_with_a_thread() {
        let mut app = thread_app(3);
        app.focus = Focus::Thread;
        let rows = lines(&app).rows;
        let row = rows.iter().find(|r| r.desc == LIST_THREADS_ACTION.1);
        assert_eq!(row.map(|r| r.keys.as_str()), Some("gT"), "{rows:?}");
        // Rebinding changes the keys; unbinding drops the row.
        app.keymap = app_with_config(
            config_from_keymap("[normal]\n\"<C-t>\" = \"list_threads\"\n\"gT\" = \"none\"\n"),
            80,
            24,
        )
        .keymap;
        let rows = lines(&app).rows;
        let row = rows.iter().find(|r| r.desc == LIST_THREADS_ACTION.1);
        assert_eq!(row.map(|r| r.keys.as_str()), Some("<C-t>"));
        app.keymap =
            app_with_config(config_from_keymap("[normal]\n\"gT\" = \"none\"\n"), 80, 24).keymap;
        assert!(
            !lines(&app)
                .rows
                .iter()
                .any(|r| r.desc == LIST_THREADS_ACTION.1)
        );

        // On a message with a thread, after the follow row; on one without, not at all.
        let mut app = messages_app("someone-else", SendState::Sent);
        let room = app.current_room.clone().unwrap();
        assert!(!descs(&lines(&app)).contains(&LIST_THREADS_ACTION.1));
        app.messages.get_mut(&room).unwrap()[0].tcount = 2;
        let ctx = lines(&app);
        let at = |d: &str| descs(&ctx).iter().position(|x| *x == d);
        assert_eq!(
            at(LIST_THREADS_ACTION.1),
            at(FOLLOW_ACTION.1).map(|i| i + 1)
        );
    }

    #[test]
    fn the_thread_actions_are_real_described_actions() {
        let (name, desc) = THREAD_ACTION;
        assert!(ACTION_NAMES.contains(&name));
        assert!(!desc.is_empty());
        for name in [
            name,
            REPLY_ACTION.0,
            FOLLOW_ACTION.0,
            LIST_THREADS_ACTION.0,
            "close_thread",
        ] {
            assert!(
                crate::app::help::DESCRIPTIONS
                    .iter()
                    .any(|(n, _)| *n == name),
                "{name} has no help description"
            );
        }
    }

    #[test]
    fn the_react_action_is_a_real_described_action() {
        let (name, desc) = REACT_ACTION;
        assert!(ACTION_NAMES.contains(&name));
        assert!(!desc.is_empty());
        assert!(
            crate::app::help::DESCRIPTIONS
                .iter()
                .any(|(n, _)| *n == name),
            "{name} has no help description"
        );
    }

    #[test]
    fn the_reactions_action_is_a_real_described_action() {
        let (name, desc) = REACTIONS_ACTION;
        assert!(ACTION_NAMES.contains(&name));
        assert!(!desc.is_empty());
        assert!(
            crate::app::help::DESCRIPTIONS
                .iter()
                .any(|(n, _)| *n == name),
            "{name} has no help description"
        );
    }

    #[test]
    fn other_message() {
        let app = messages_app("someone-else", SendState::Sent);
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::OtherMessage);
        assert_eq!(
            descs(&ctx),
            [
                "reply in its thread",
                "add or remove your reaction",
                "jump to the oldest message",
                "jump to the newest message",
                "find text in the room",
                "next match, older",
                "previous match, newer"
            ],
            "someone else's message lists reacting, the jumps and the search actions"
        );
    }

    #[test]
    fn input_bar() {
        let mut app = app_80x24();
        app.focus = Focus::Input;
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Input);
        assert!(descs(&ctx).contains(&"type a message (Insert mode)"));
    }

    #[test]
    fn nothing_selected() {
        let mut app = app_80x24();
        app.current_room = None;
        app.focus = Focus::Messages;
        let ctx = lines(&app);
        assert_eq!(ctx.kind, Kind::Nothing);
        assert!(ctx.rows.is_empty());
    }

    #[test]
    fn rebound_key_shown() {
        let config = config_from_keymap("[normal]\n\"E\" = \"edit_message\"\n\"e\" = \"none\"\n");
        let mut app = app_with_config(config, 80, 24);
        let room = app.rooms[0].id.clone();
        let mut m = sample_message("m-ctx");
        m.room_id = room.clone();
        m.author = "me".to_string();
        m.ts = i64::MAX / 4;
        app.messages.insert(room.clone(), vec![m]);
        app.current_room = Some(room);
        app.own_username = Some("me".to_string());
        app.focus = Focus::Messages;
        let ctx = lines(&app);
        assert_eq!(ctx.rows[0].desc, "edit the message");
        assert_eq!(ctx.rows[0].keys, "E");
    }

    #[test]
    fn unbound_action_dropped() {
        let config = config_from_keymap("[normal]\n\"e\" = \"none\"\n");
        let mut app = app_with_config(config, 80, 24);
        let room = app.rooms[0].id.clone();
        let mut m = sample_message("m-ctx");
        m.room_id = room.clone();
        m.author = "me".to_string();
        m.ts = i64::MAX / 4;
        app.messages.insert(room.clone(), vec![m]);
        app.current_room = Some(room);
        app.own_username = Some("me".to_string());
        app.focus = Focus::Messages;
        let ctx = lines(&app);
        assert_eq!(&descs(&ctx)[..1], ["delete the message"]);
        assert!(!descs(&ctx).contains(&"edit the message"));
    }

    #[test]
    fn search_rows_follow_the_keymap() {
        let config =
            config_from_keymap("[normal]\n\"<C-f>\" = \"search_start\"\n\"N\" = \"none\"\n");
        let mut app = app_with_config(config, 80, 24);
        let room = app.rooms[0].id.clone();
        let mut m = sample_message("m-ctx");
        m.room_id = room.clone();
        m.author = "someone-else".to_string();
        m.ts = i64::MAX / 4;
        app.messages.insert(room.clone(), vec![m]);
        app.current_room = Some(room);
        app.focus = Focus::Messages;
        let ctx = lines(&app);
        let find = ctx
            .rows
            .iter()
            .find(|r| r.desc == "find text in the room")
            .expect("find row");
        assert_eq!(find.keys, "/ <C-f>");
        assert!(!descs(&ctx).contains(&"previous match, newer"));
        assert!(descs(&ctx).contains(&"next match, older"));
    }

    #[test]
    fn cmd_binding_not_listed() {
        let config = config_from_keymap("[normal]\n\"<C-q>\" = \"cmd:quit\"\n");
        let app = app_with_config(config, 80, 24);
        let ctx = lines(&app);
        assert!(ctx.rows.iter().all(|r| !r.keys.contains("<C-q>")));
        assert!(!ctx.full_help.keys.contains("<C-q>"));
    }

    #[test]
    fn table_names_are_valid_actions() {
        for kind in [
            Kind::Room,
            Kind::Folder,
            Kind::Header,
            Kind::OwnMessage,
            Kind::UnsentMessage,
            Kind::OtherMessage,
            Kind::Input,
            Kind::Thread,
            Kind::ThreadList,
            Kind::Nothing,
        ] {
            for (name, desc) in kind.actions() {
                assert!(ACTION_NAMES.contains(name), "{name}");
                assert!(!desc.is_empty());
                assert!(
                    crate::app::help::DESCRIPTIONS
                        .iter()
                        .any(|(n, _)| n == name),
                    "{name} has no help description"
                );
            }
        }
    }
}

#[cfg(test)]
mod reducer_tests {
    use crate::action::Action;
    use crate::app::update::update;
    use crate::app::{Focus, Mode, sidebar};
    use crate::test_util::{app_80x24, press, sample_message};

    #[test]
    fn opens_without_changing_mode_focus_or_selection() {
        let mut app = app_80x24();
        let (mode, focus, cursor) = (app.mode, app.focus, app.sidebar.cursor);
        let room = app.current_room.clone();
        press(&mut app, "?");
        assert!(app.context);
        assert_eq!(app.help, None);
        assert_eq!(app.mode, mode);
        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.focus, focus);
        assert_eq!(app.sidebar.cursor, cursor);
        assert_eq!(app.current_room, room);
    }

    #[test]
    fn any_key_closes_without_acting() {
        for keys in ["j", "dd", "i", "<CR>", ":", "q"] {
            let mut app = app_80x24();
            let before = (
                app.sidebar.cursor,
                app.mode,
                app.focus,
                app.current_room.clone(),
                app.cmdline.clone(),
            );
            press(&mut app, "?");
            press(&mut app, keys);
            assert!(!app.context, "{keys}");
            assert_eq!(app.help, None, "{keys}");
            assert_eq!(
                before,
                (
                    app.sidebar.cursor,
                    app.mode,
                    app.focus,
                    app.current_room.clone(),
                    app.cmdline.clone()
                ),
                "{keys}"
            );
            assert!(!app.should_quit, "{keys}");
        }
    }

    #[test]
    fn second_question_mark_opens_help_and_third_closes_it() {
        let mut app = app_80x24();
        press(&mut app, "??");
        assert!(!app.context);
        assert_eq!(app.help, Some(0));
        assert_eq!(app.mode, Mode::Normal);
        press(&mut app, "?");
        assert_eq!(app.help, None);
        assert!(!app.context);
    }

    #[test]
    fn key_timeout_and_counts_do_not_act() {
        let mut app = app_80x24();
        press(&mut app, "?");
        update(&mut app, Action::KeyTimeout);
        assert!(app.context);
        press(&mut app, "3");
        assert!(!app.context);
        assert_eq!(app.help, None);
    }

    #[test]
    fn follows_the_object_after_closing() {
        let mut app = app_80x24();
        assert!(matches!(
            sidebar::selected_row(&app),
            Some(sidebar::SidebarRow::Room { .. }) | Some(sidebar::SidebarRow::Folder { .. })
        ));
        app.focus = Focus::Input;
        press(&mut app, "?");
        assert_eq!(super::classify(&app), super::Kind::Input);
    }

    #[test]
    fn live_messages_still_applied_underneath() {
        let mut app = app_80x24();
        let room = app.rooms[0].id.clone();
        let before = app.messages.get(&room).map_or(0, Vec::len);
        press(&mut app, "?");
        let mut live = sample_message("live-ctx");
        live.room_id = room.clone();
        live.ts = i64::MAX / 4;
        update(&mut app, Action::RtMessage(live));
        assert_eq!(app.messages.get(&room).map_or(0, Vec::len), before + 1);
        assert!(app.context);
    }
}

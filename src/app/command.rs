//! Table-driven `:` commands (D-16). Later phases add rows to `COMMANDS`.

use crate::app::{App, StatusMsg};

pub struct CommandDef {
    pub names: &'static [&'static str],
    pub run: fn(&mut App, &[&str]) -> Result<(), String>,
}

pub const COMMANDS: &[CommandDef] = &[CommandDef {
    names: &["q", "quit"],
    run: cmd_quit,
}];

/// `:q` / `:quit` — the only way to leave the app.
pub fn cmd_quit(app: &mut App, _args: &[&str]) -> Result<(), String> {
    app.should_quit = true;
    Ok(())
}

/// Run one command line (without the leading `:`).
pub fn execute(app: &mut App, line: &str) {
    let mut tokens = line.split_whitespace();
    let Some(name) = tokens.next() else {
        return;
    };
    let args: Vec<&str> = tokens.collect();
    match COMMANDS.iter().find(|def| def.names.contains(&name)) {
        Some(def) => {
            if let Err(msg) = (def.run)(app, &args) {
                app.status_msg = Some(StatusMsg::Error(msg));
            }
        }
        None => {
            app.status_msg = Some(StatusMsg::Error(format!("Unknown command: {name}")));
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::test_util::{app_80x24, press};

    #[test]
    fn colon_q_quits() {
        let mut app = app_80x24();
        press(&mut app, ":q<CR>");
        assert!(app.should_quit);
    }
}

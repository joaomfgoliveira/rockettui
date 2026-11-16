//! Thin binary: load the config, set up the terminal, run the app, always restore the terminal.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    // Config is read before the terminal starts, so a problem can be reported on a clean screen.
    let config = match rockettui::config::load() {
        Ok(config) => config,
        Err(report) => {
            eprint!("{report}");
            return ExitCode::from(1);
        }
    };
    let terminal = rockettui::tui::init();
    let result = rockettui::run(terminal, config).await;
    rockettui::tui::restore();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rockettui: {e}");
            ExitCode::FAILURE
        }
    }
}

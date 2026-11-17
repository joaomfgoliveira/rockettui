//! Thin binary: load the config, set up the terminal, run the app, always restore the terminal.

use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    // Config is read before the terminal starts. A bad config shows the error screen, then the
    // full report goes to stderr once the terminal is restored (D-02).
    let config = match rockettui::config::load() {
        Ok(config) => config,
        Err(report) => {
            let terminal = rockettui::tui::init();
            let _ = rockettui::show_config_errors(terminal, &report).await;
            rockettui::tui::restore();
            eprint!("{report}");
            return ExitCode::from(1);
        }
    };
    // The warnings are also printed once the terminal is restored, so they outlive the alternate
    // screen (D-04); they hold no token, user id or URL.
    let warnings = config.warnings.clone();
    let terminal = rockettui::tui::init();
    let result = rockettui::run(terminal, config).await;
    rockettui::tui::restore();
    for warning in &warnings {
        eprintln!("rockettui: warning: {warning}");
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("rockettui: {e}");
            ExitCode::FAILURE
        }
    }
}

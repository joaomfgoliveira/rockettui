//! Thin binary: set up the terminal, run the app, always restore the terminal.

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let terminal = rockettui::tui::init();
    let result = rockettui::run(terminal).await;
    rockettui::tui::restore();
    result
}

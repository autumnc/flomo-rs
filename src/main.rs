mod api;
mod app;
mod ui;

use std::env;
use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use app::App;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::CrosstermBackend, Terminal};

use api::{ApiRequest, ApiResponse};
use app::StatusKind;

fn main() -> io::Result<()> {
    // Parse args
    let args: Vec<String> = env::args().collect();
    let high_contrast = args.iter().any(|a| a == "-hc");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new(high_contrast);

    // API communication channels
    let (req_tx, req_rx) = mpsc::channel::<ApiRequest>();
    let (res_tx, res_rx) = mpsc::channel::<ApiResponse>();

    // Start API background thread (pass saved token so startup sync works)
    let saved_token = api::load_token().unwrap_or_default();
    let res_tx_clone = res_tx.clone();
    thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let client = rt.block_on(async { reqwest::Client::new() });
        let current_token = std::cell::RefCell::new(saved_token);

        while let Ok(req) = req_rx.recv() {
            // Extract token from login response or use cached
            let tx = res_tx_clone.clone();
            let client = client.clone();

            let token = match &req {
                ApiRequest::Login { .. } => String::new(),
                _ => current_token.borrow().clone(),
            };

            rt.block_on(async {
                let resp = api::process_request(req, &client, &token).await;
                if let ApiResponse::LoginOk { ref token, .. } = resp {
                    *current_token.borrow_mut() = token.clone();
                }
                let _ = tx.send(resp);
            });
        }
    });

    // Main event loop
    let result = run_loop(&mut terminal, &mut app, &req_tx, &res_rx);

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    req_tx: &mpsc::Sender<ApiRequest>,
    res_rx: &mpsc::Receiver<ApiResponse>,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll events with 50ms timeout (for responsive UI + channel checking)
        let timeout = Duration::from_millis(50);
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if app.handle_key(key, req_tx) {
                    break;
                }
            }
        }

        // Process API responses
        while let Ok(resp) = res_rx.try_recv() {
            app.handle_response(resp);
        }

        // Auto-sync when needed (startup with saved token, or after login)
        if app.needs_sync {
            if app.token.is_some() {
                app.needs_sync = false;
                app.is_loading = true;
                app.set_status("正在同步...", StatusKind::Info);
                let _ = req_tx.send(ApiRequest::ListMemos);
                let _ = req_tx.send(ApiRequest::GetTagTree);
            } else {
                app.needs_sync = false;
            }
        }
    }

    Ok(())
}

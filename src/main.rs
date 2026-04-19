mod api;
mod app;
mod image;
mod ui;

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
    // ─── Startup diagnostics (printed to stderr BEFORE entering alternate screen) ───
    eprintln!("[flomo-rs] v0.3.1 (stdin-daemon) build=20260419");
    let exe = std::env::current_exe().unwrap_or_default();
    eprintln!("[flomo-rs] running binary: {}", exe.display());
    if image::is_ueberzugpp_available() {
        eprintln!("[image] ueberzugpp detected ✓");
    } else {
        eprintln!("[image] ueberzugpp NOT found — image preview disabled");
    }
    eprintln!("[image] debug log: ~/.flomo-cli/image-debug.log");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app (this starts the image manager)
    let mut app = App::new();

    // Report image manager status (after entering alternate screen, write to log)
    {
        use std::io::Write;
        let log_msg = format!(
            "[main] ImageManager initialized, available={}\n",
            app.image_manager.is_available()
        );
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(image::log_path())
            .and_then(|mut f| f.write_all(log_msg.as_bytes()));
    }

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

    // Clear all image overlays before exiting
    app.image_manager.clear_overlays();

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
    let mut cached_area = ratatui::layout::Rect::default();

    loop {
        // Update image state based on current memo selection
        app.update_image_memo();

        // Process any completed image downloads
        app.image_manager.process_downloads();

        // Draw the TUI
        terminal.draw(|f| {
            cached_area = f.area();
            ui::draw(f, app);
        })?;

        // After draw: display images via ueberzugpp overlays
        if app.image_manager.is_available() && app.mode == app::Mode::Normal {
            display_current_images(app, cached_area);
        } else {
            app.image_manager.clear_overlays();
        }

        // Poll events with 50ms timeout
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

        // Auto-sync when needed
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

/// Display images using ueberzugpp at positions recorded during the draw phase.
fn display_current_images(
    app: &mut App,
    _size: ratatui::layout::Rect,
) {
    use crate::image::{ImageDisplayRect, cache_path_for_url};

    if !app.image_manager.is_available() {
        return;
    }

    let positions = &app.image_render_positions;
    let image_urls = app.image_manager.current_images_urls();

    if positions.is_empty() {
        // No visible image positions — clear overlays
        app.image_manager.clear_overlays();
        return;
    }

    let mut rects: Vec<ImageDisplayRect> = Vec::new();

    for (i, &(x, y, w, h)) in positions.iter().enumerate() {
        if let Some(img_info) = image_urls.get(i) {
            if let Some(local_path) = app.image_manager.get_cached_path(&img_info.url) {
                let identifier = format!(
                    "flomo-{}",
                    cache_path_for_url(&img_info.url)
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                );

                rects.push(ImageDisplayRect {
                    identifier,
                    x,
                    y,
                    width: w,
                    height: h,
                    local_path,
                });
            }
        }
    }

    app.image_manager.display_images(rects);
}

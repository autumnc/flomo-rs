//! Image preview module using ueberzugpp.
//!
//! Uses the `ueberzugpp layer` stdin pipe approach:
//! 1. Start one long-lived `ueberzugpp layer` process
//! 2. Keep its stdin open — write JSON commands (add/remove) directly
//! 3. The process persists; we only write to stdin to update overlays
//!
//! No socket, no spawning per frame.

use md5::{Digest, Md5};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use crate::api::ImageInfo;

// ─── Configuration ────────────────────────────────────────────────────────

const IMAGE_CACHE_DIR: &str = ".flomo-cli/cache/images";
pub const DEFAULT_IMAGE_HEIGHT_CHARS: u16 = 12;
const DOWNLOAD_TIMEOUT_SECS: u64 = 15;

// ─── Logging ─────────────────────────────────────────────────────────────

/// Log file path for image debugging
pub fn log_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".flomo-cli")
        .join("image-debug.log")
}

/// Write a line to the log file (append mode).
macro_rules! img_log {
    ($($arg:tt)*) => {
        {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(crate::image::log_path())
            {
                let _ = writeln!(f, "[{}] {}", chrono::Local::now().format("%H:%M:%S%.3f"), format!($($arg)*));
            }
        }
    };
}

// ─── Image Display Record ────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ImageDisplayRect {
    pub identifier: String,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub local_path: PathBuf,
}

// ─── Download Messages ───────────────────────────────────────────────────

enum DownloadMsg {
    Download { url: String, identifier: String },
    Shutdown,
}

pub enum DownloadResult {
    Downloaded { url: String, identifier: String, path: PathBuf },
    Failed { url: String, identifier: String, error: String },
}

// ─── Ueberzug Daemon ────────────────────────────────────────────────────

/// A long-lived ueberzugpp layer process with stdin pipe for JSON commands.
struct UeberzugDaemon {
    /// The daemon child process (kept alive).
    child: std::process::Child,
    /// Stdin pipe — we write JSON commands here.
    stdin: std::process::ChildStdin,
    /// Set of currently displayed image identifiers.
    active_ids: HashSet<String>,
}

impl UeberzugDaemon {
    /// Start the daemon with stdin pipe.
    fn start() -> Option<Self> {
        img_log!("[daemon] starting ueberzugpp layer (stdin mode)...");

        // Try different flag combinations
        let attempts = [
            vec!["layer"],
            vec!["layer", "--no-cache"],
            vec!["layer", "--silent"],
        ];

        let mut last_err = String::new();

        for args in &attempts {
            let arg_str = args.join(" ");
            img_log!("[daemon] trying: ueberzugpp {}", arg_str);

            match Command::new("ueberzugpp")
                .args(args)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())  // we don't need its output
                .stderr(Stdio::null())  // suppress noise
                .spawn()
            {
                Ok(mut child) => {
                    // Take ownership of stdin
                    let stdin = match child.stdin.take() {
                        Some(s) => s,
                        None => {
                            img_log!("[daemon] ERROR: cannot open stdin");
                            let _ = child.kill();
                            continue;
                        }
                    };

                    // Give it a moment to start
                    std::thread::sleep(Duration::from_millis(100));

                    // Verify it's still alive
                    match child.try_wait() {
                        Ok(None) => {
                            img_log!("[daemon] started successfully with: ueberzugpp {}", arg_str);
                            return Some(Self {
                                child,
                                stdin,
                                active_ids: HashSet::new(),
                            });
                        }
                        Ok(Some(status)) => {
                            last_err = format!("process exited with status {}", status);
                            img_log!("[daemon] ERROR: ueberzugpp {} {}", arg_str, last_err);
                        }
                        Err(e) => {
                            last_err = format!("try_wait failed: {}", e);
                            img_log!("[daemon] ERROR: {}", last_err);
                        }
                    }
                }
                Err(e) => {
                    last_err = format!("spawn failed: {}", e);
                    img_log!("[daemon] ERROR: ueberzugpp {} {}", arg_str, last_err);
                }
            }
        }

        img_log!("[daemon] all attempts failed, last error: {}", last_err);
        None
    }

    /// Send a JSON command to the daemon via stdin.
    fn send_command(&mut self, cmd: &serde_json::Value) -> bool {
        let json_str = match serde_json::to_string(cmd) {
            Ok(s) => s,
            Err(e) => {
                img_log!("[daemon] ERROR: json serialize: {}", e);
                return false;
            }
        };
        match writeln!(self.stdin, "{}", json_str) {
            Ok(_) => {
                match self.stdin.flush() {
                    Ok(_) => true,
                    Err(e) => {
                        img_log!("[daemon] ERROR flushing stdin: {}", e);
                        false
                    }
                }
            }
            Err(e) => {
                img_log!("[daemon] ERROR writing to stdin: {}", e);
                false
            }
        }
    }

    /// Add or update an image overlay.
    fn add_image(&mut self, identifier: &str, x: u16, y: u16, width: u16, height: u16, path: &std::path::Path) {
        let cmd = serde_json::json!({
            "action": "add",
            "identifier": identifier,
            "x": x,
            "y": y,
            "width": width,
            "height": height,
            "path": path.to_string_lossy(),
        });
        if self.send_command(&cmd) {
            img_log!("[daemon] add: id={} pos=({},{}) {}x{} path={}", identifier, x, y, width, height, path.display());
            self.active_ids.insert(identifier.to_string());
        }
    }

    /// Remove an image overlay.
    fn remove_image(&mut self, identifier: &str) {
        let cmd = serde_json::json!({
            "action": "remove",
            "identifier": identifier,
        });
        let _ = self.send_command(&cmd);
        img_log!("[daemon] remove: id={}", identifier);
        self.active_ids.remove(identifier);
    }

    /// Remove all active overlays.
    fn clear_all(&mut self) {
        let ids: Vec<String> = self.active_ids.drain().collect();
        for id in &ids {
            let cmd = serde_json::json!({
                "action": "remove",
                "identifier": id,
            });
            let _ = self.send_command(&cmd);
        }
        if !ids.is_empty() {
            img_log!("[daemon] cleared {} overlays", ids.len());
        }
    }

    /// Check if the daemon process is still alive.
    fn is_alive(&mut self) -> bool {
        match self.child.try_wait() {
            Ok(None) => true,
            Ok(Some(status)) => {
                img_log!("[daemon] process exited with status {}", status);
                false
            }
            Err(_) => false,
        }
    }
}

impl Drop for UeberzugDaemon {
    fn drop(&mut self) {
        self.clear_all();
        std::thread::sleep(Duration::from_millis(100));
        let _ = self.child.kill();
        std::thread::sleep(Duration::from_millis(50));
        let _ = self.child.wait();
        img_log!("[daemon] dropped");
    }
}

// ─── Image Manager ──────────────────────────────────────────────────────

pub struct ImageManager {
    /// Whether ueberzugpp is available
    available: bool,

    /// The long-lived ueberzugpp daemon (lazy-started on first image display)
    daemon: Option<UeberzugDaemon>,

    /// Download channels
    download_tx: Option<Sender<DownloadMsg>>,
    download_rx: Option<Receiver<DownloadResult>>,

    /// Cached image map: URL -> local path
    cached_images: HashMap<String, PathBuf>,

    /// Current memo tracking
    current_memo_slug: String,
    current_images: Vec<ImageInfo>,
    downloading: HashSet<String>,
}

impl ImageManager {
    pub fn new() -> Self {
        // Clear old log file on startup
        let _ = std::fs::write(log_path(), format!(
            "=== flomo-rs v0.3.1 (stdin-daemon) image debug log — started at {} ===\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ));

        let available = is_ueberzugpp_available();
        img_log!("ueberzugpp available: {}", available);

        // Ensure cache dir exists
        let _ = std::fs::create_dir_all(cache_dir());

        // Create download channels
        let (tx, rx) = mpsc::channel::<DownloadMsg>();
        let (res_tx, res_rx) = mpsc::channel::<DownloadResult>();

        if available {
            let _handle = thread::spawn(move || {
                download_worker(rx, res_tx);
            });
        }

        Self {
            available,
            daemon: None,
            download_tx: if available { Some(tx) } else { None },
            download_rx: if available { Some(res_rx) } else { None },
            cached_images: HashMap::new(),
            current_memo_slug: String::new(),
            current_images: Vec::new(),
            downloading: HashSet::new(),
        }
    }

    pub fn is_available(&self) -> bool {
        self.available
    }

    pub fn set_current_memo(&mut self, slug: &str, images: Vec<ImageInfo>) {
        if slug == self.current_memo_slug {
            return;
        }

        self.clear_overlays();
        self.current_memo_slug = slug.to_string();
        self.current_images = images;
        self.downloading.clear();

        img_log!(
            "set memo: {}, images: {}",
            slug,
            self.current_images.len()
        );

        // Start downloading images
        if let Some(ref tx) = self.download_tx {
            for img in &self.current_images {
                let url = &img.url;
                if !self.cached_images.contains_key(url) && !self.downloading.contains(url) {
                    let identifier = make_identifier(url);
                    img_log!("start download: {} -> {}", url, identifier);
                    let _ = tx.send(DownloadMsg::Download {
                        url: url.clone(),
                        identifier,
                    });
                    self.downloading.insert(url.clone());
                }
            }
        }
    }

    pub fn process_downloads(&mut self) {
        if let Some(ref rx) = self.download_rx {
            while let Ok(result) = rx.try_recv() {
                match result {
                    DownloadResult::Downloaded { url, path, .. } => {
                        img_log!("downloaded: {} -> {}", url, path.display());
                        self.cached_images.insert(url.clone(), path);
                        self.downloading.remove(&url);
                    }
                    DownloadResult::Failed { url, error, .. } => {
                        img_log!("download FAILED: {} - {}", url, error);
                        self.downloading.remove(&url);
                    }
                }
            }
        }
    }

    pub fn downloading_count(&self) -> usize {
        self.downloading.len()
    }

    pub fn cached_count(&self) -> usize {
        self.current_images
            .iter()
            .filter(|img| self.cached_images.contains_key(&img.url))
            .count()
    }

    pub fn current_images_urls(&self) -> &[ImageInfo] {
        &self.current_images
    }

    pub fn get_cached_path(&self, url: &str) -> Option<PathBuf> {
        self.cached_images.get(url).cloned()
    }

    /// Display images at the given positions using the daemon.
    pub fn display_images(&mut self, rects: Vec<ImageDisplayRect>) {
        if !self.available {
            return;
        }

        if rects.is_empty() {
            if self.daemon.is_some() {
                self.clear_overlays();
            }
            return;
        }

        // Ensure daemon is running
        if self.daemon.is_none() {
            match UeberzugDaemon::start() {
                Some(d) => {
                    img_log!("[daemon] ready");
                    self.daemon = Some(d);
                }
                None => {
                    img_log!("[daemon] ERROR: failed to start daemon, disabling image preview");
                    self.available = false;
                    return;
                }
            }
        }

        let daemon = match &mut self.daemon {
            Some(d) => d,
            None => return,
        };

        // Check daemon is still alive
        if !daemon.is_alive() {
            img_log!("[daemon] daemon died, restarting...");
            self.daemon = None;
            if let Some(d) = UeberzugDaemon::start() {
                self.daemon = Some(d);
                let daemon = self.daemon.as_mut().unwrap();
                // Re-send all images after restart
                for rect in &rects {
                    daemon.add_image(
                        &rect.identifier,
                        rect.x, rect.y, rect.width, rect.height,
                        &rect.local_path,
                    );
                }
                return;
            } else {
                self.available = false;
                return;
            }
        }

        // Determine which identifiers are still needed
        let new_ids: HashSet<String> = rects.iter().map(|r| r.identifier.clone()).collect();

        // Remove overlays no longer needed
        let to_remove: Vec<String> = daemon
            .active_ids
            .iter()
            .filter(|id| !new_ids.contains(*id))
            .cloned()
            .collect();

        for id in to_remove {
            daemon.remove_image(&id);
        }

        // Add/update overlays (only if not already active)
        for rect in &rects {
            if !daemon.active_ids.contains(&rect.identifier) {
                daemon.add_image(
                    &rect.identifier,
                    rect.x, rect.y, rect.width, rect.height,
                    &rect.local_path,
                );
            }
        }
    }

    /// Clear all overlays
    pub fn clear_overlays(&mut self) {
        if let Some(ref mut daemon) = self.daemon {
            daemon.clear_all();
        }
    }

    pub fn shutdown(&self) {
        if let Some(ref tx) = self.download_tx {
            let _ = tx.send(DownloadMsg::Shutdown);
        }
    }
}

impl Drop for ImageManager {
    fn drop(&mut self) {
        img_log!("ImageManager dropping...");
        self.clear_overlays();
        self.daemon = None;
        self.shutdown();
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────

fn cache_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(IMAGE_CACHE_DIR)
}

pub fn cache_path_for_url(url: &str) -> PathBuf {
    let mut hasher = Md5::new();
    hasher.update(url.as_bytes());
    let hash = format!("{:x}", hasher.finalize());

    let ext = if url.contains(".png") {
        "png"
    } else if url.contains(".gif") {
        "gif"
    } else if url.contains(".webp") {
        "webp"
    } else if url.contains(".jpeg") || url.contains(".jpg") {
        "jpg"
    } else {
        "jpg"
    };

    cache_dir().join(format!("{}.{}", &hash[..16], ext))
}

fn make_identifier(url: &str) -> String {
    format!(
        "flomo-{}",
        &cache_path_for_url(url)
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
    )
}

pub fn is_ueberzugpp_available() -> bool {
    Command::new("ueberzugpp")
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ─── Download Worker ────────────────────────────────────────────────────

fn download_worker(rx: Receiver<DownloadMsg>, res_tx: Sender<DownloadResult>) {
    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(DownloadMsg::Download { url, identifier }) => {
                let result = download_image(&url);
                match result {
                    Ok(path) => {
                        let _ = res_tx.send(DownloadResult::Downloaded {
                            url,
                            identifier,
                            path,
                        });
                    }
                    Err(e) => {
                        let _ = res_tx.send(DownloadResult::Failed {
                            url,
                            identifier,
                            error: e,
                        });
                    }
                }
            }
            Ok(DownloadMsg::Shutdown) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn download_image(url: &str) -> Result<PathBuf, String> {
    let cache_path = cache_path_for_url(url);

    if cache_path.exists() {
        return Ok(cache_path);
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(DOWNLOAD_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| format!("Download failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP error: {}", response.status()));
    }

    let bytes = response
        .bytes()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create cache dir: {}", e))?;
    }

    std::fs::write(&cache_path, &bytes)
        .map_err(|e| format!("Failed to write cache file: {}", e))?;

    Ok(cache_path)
}

use std::path::PathBuf;

use crate::api::Memo;

fn db_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".flomo-cli")
        .join("memos.json")
}

pub fn save_memos(memos: &[Memo]) {
    let path = db_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, serde_json::to_string_pretty(memos).unwrap_or_default());
}

pub fn load_memos() -> Option<Vec<Memo>> {
    let path = db_path();
    if !path.exists() {
        return None;
    }
    let data = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&data).ok()
}

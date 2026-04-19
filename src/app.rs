use chrono::{Datelike, Local, Months, NaiveDate};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthChar;

/// Convert a display-column position to a byte offset within the string.
/// For CJK characters (3 bytes, 2 columns), this correctly finds char boundaries.
fn col_to_byte_offset(s: &str, col: usize) -> usize {
    let mut byte_off = 0usize;
    let mut display_col = 0usize;
    for ch in s.chars() {
        if display_col >= col {
            break;
        }
        byte_off += ch.len_utf8();
        display_col += UnicodeWidthChar::width(ch).unwrap_or(1);
    }
    byte_off
}

use crate::api::{self, ApiRequest, ApiResponse, Memo, TagInfo};
use crate::image::ImageManager;

// ─── App Modes ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Mode {
    Normal,
    Search,
    Edit,
    Calendar,
    Tags,
    Login,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    Sidebar,
    Main,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatusKind {
    Info,
    Success,
    Error,
}

// ─── Application State ────────────────────────────────────────────────────

pub struct App {
    pub mode: Mode,
    pub focus: Focus,
    #[allow(dead_code)]
    pub should_quit: bool,

    // Data
    pub memos: Vec<Memo>,
    pub filtered_indices: Vec<usize>,
    pub all_tags: Vec<TagInfo>,

    // Sidebar
    pub sidebar_index: usize,
    pub sidebar_scroll: u16,

    // Main content scroll
    pub main_scroll: u16,

    // Edit mode
    pub edit_lines: Vec<String>,
    pub edit_cursor_row: usize,
    pub edit_cursor_col: usize,
    pub edit_scroll: u16,
    pub edit_is_new: bool,
    pub edit_original_slug: String,

    // Search
    pub search_query: String,

    // Calendar
    pub cal_year: i32,
    pub cal_month: u32,
    pub cal_cursor: NaiveDate,
    pub cal_has_memos: std::collections::HashSet<String>,

    // Tags popup
    pub tag_index: usize,
    pub tag_scroll: u16,

    // Filters
    pub filter_tag: Option<String>,
    pub filter_date: Option<NaiveDate>,

    // Login
    pub login_email: String,
    pub login_password: String,
    pub login_step: usize, // 0=email, 1=password
    pub login_error: Option<String>,

    // Auth
    pub token: Option<String>,

    // Status
    pub status_msg: Option<(String, StatusKind)>,
    pub is_loading: bool,
    pub needs_sync: bool,

    // Detail panel visible height (set during draw)
    pub detail_visible_height: u16,

    // Image preview manager
    pub image_manager: ImageManager,

    // Track the last displayed memo slug for image switching
    pub last_displayed_memo_slug: String,

    // Image placeholder positions in terminal coordinates (x, y, width, height).
    // Populated during draw_detail_panel, consumed by display_current_images.
    pub image_render_positions: Vec<(u16, u16, u16, u16)>,
}

impl App {
    pub fn new() -> Self {
        let today = Local::now().date_naive();
        let has_token = api::load_token();
        let need_login = has_token.is_none();

        let image_manager = ImageManager::new();

        App {
            mode: if need_login { Mode::Login } else { Mode::Normal },
            focus: Focus::Sidebar,
            should_quit: false,
            memos: Vec::new(),
            filtered_indices: Vec::new(),
            all_tags: Vec::new(),
            sidebar_index: 0,
            sidebar_scroll: 0,
            main_scroll: 0,
            edit_lines: vec![String::new()],
            edit_cursor_row: 0,
            edit_cursor_col: 0,
            edit_scroll: 0,
            edit_is_new: false,
            edit_original_slug: String::new(),
            search_query: String::new(),
            cal_year: today.year(),
            cal_month: today.month(),
            cal_cursor: today,
            cal_has_memos: std::collections::HashSet::new(),
            tag_index: 0,
            tag_scroll: 0,
            filter_tag: None,
            filter_date: None,
            login_email: String::new(),
            login_password: String::new(),
            login_step: 0,
            login_error: None,
            token: has_token.clone(),
            status_msg: None,
            is_loading: has_token.is_some(),
            needs_sync: has_token.is_some(),
            detail_visible_height: 0,
            image_manager,
            last_displayed_memo_slug: String::new(),
            image_render_positions: Vec::new(),
        }
    }

    pub fn current_memo(&self) -> Option<&Memo> {
        self.filtered_indices
            .get(self.sidebar_index)
            .and_then(|&i| self.memos.get(i))
    }

    fn apply_filters(&mut self) {
        self.filtered_indices = (0..self.memos.len()).collect();
        if let Some(ref tag) = self.filter_tag {
            self.filtered_indices.retain(|&i| {
                self.memos[i]
                    .tags
                    .iter()
                    .any(|t| t.eq_ignore_ascii_case(tag))
            });
        }
        if let Some(date) = self.filter_date {
            let date_str = date.format("%Y-%m-%d").to_string();
            self.filtered_indices
                .retain(|&i| self.memos[i].created_at.starts_with(&date_str));
        }
        if !self.search_query.is_empty() {
            let q = self.search_query.to_lowercase();
            self.filtered_indices.retain(|&i| {
                let text = self.memos[i].content_text().to_lowercase();
                let tags = self.memos[i].tags.join(" ").to_lowercase();
                text.contains(&q) || tags.contains(&q)
            });
        }
        if self.sidebar_index >= self.filtered_indices.len() {
            self.sidebar_index = self.filtered_indices.len().saturating_sub(1);
        }
        self.sidebar_scroll = 0;
    }

    pub fn set_status(&mut self, msg: &str, kind: StatusKind) {
        self.status_msg = Some((msg.to_string(), kind));
    }

    fn rebuild_calendar_memo_dates(&mut self) {
        self.cal_has_memos.clear();
        for m in &self.memos {
            if let Some(d) = m.created_date() {
                self.cal_has_memos.insert(d.format("%Y-%m-%d").to_string());
            }
        }
    }

    fn build_tags_from_memos(&mut self) {
        let mut tag_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for m in &self.memos {
            for t in &m.tags {
                *tag_counts.entry(t.clone()).or_insert(0) += 1;
            }
        }
        let mut tags: Vec<TagInfo> = tag_counts
            .into_iter()
            .map(|(name, count)| TagInfo { name, count })
            .collect();
        tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        self.all_tags = tags;
    }

    /// Update image manager with current memo selection.
    /// Should be called whenever the displayed memo might change.
    pub fn update_image_memo(&mut self) {
        if self.mode == Mode::Edit || self.mode == Mode::Login {
            self.image_manager.clear_overlays();
            self.last_displayed_memo_slug.clear();
            return;
        }

        let current_slug = self
            .current_memo()
            .map(|m| m.slug.clone())
            .unwrap_or_default();

        if current_slug != self.last_displayed_memo_slug {
            if let Some(memo) = self.current_memo() {
                let slug = memo.slug.clone();
                let (_, images) = memo.content_text_with_images();
                self.image_manager
                    .set_current_memo(&slug, images);
            } else {
                self.image_manager.clear_overlays();
            }
            self.last_displayed_memo_slug = current_slug;
        }
    }

    pub fn handle_response(&mut self, resp: ApiResponse) {
        match resp {
            ApiResponse::LoginOk { token, user_name } => {
                self.token = Some(token);
                self.mode = Mode::Normal;
                self.set_status(&format!("登录成功，欢迎 {}", user_name), StatusKind::Success);
                self.is_loading = true;
                self.needs_sync = true;
            }
            ApiResponse::LoginErr(msg) => {
                self.login_error = Some(msg);
                self.login_step = 0;
                self.login_password.clear();
            }
            ApiResponse::MemosLoaded(memos) => {
                let count = memos.len();
                self.memos = memos;
                self.rebuild_calendar_memo_dates();
                self.build_tags_from_memos();
                self.apply_filters();
                self.is_loading = false;
                if count > 0 {
                    self.set_status(&format!("已同步 {} 条笔记", count), StatusKind::Success);
                }
            }
            ApiResponse::MemoCreated(memo) => {
                self.memos.insert(0, memo);
                self.rebuild_calendar_memo_dates();
                self.build_tags_from_memos();
                self.apply_filters();
                self.sidebar_index = 0;
                self.set_status("笔记已创建", StatusKind::Success);
            }
            ApiResponse::MemoUpdated(memo) => {
                if let Some(pos) = self.memos.iter().position(|m| m.slug == memo.slug) {
                    self.memos[pos] = memo;
                }
                self.build_tags_from_memos();
                self.apply_filters();
                self.set_status("笔记已保存", StatusKind::Success);
            }
            ApiResponse::MemoDeleted => {
                if let Some(memo) = self.current_memo().cloned() {
                    let slug = memo.slug.clone();
                    if let Some(pos) = self.memos.iter().position(|m| m.slug == slug) {
                        self.memos.remove(pos);
                    }
                    self.rebuild_calendar_memo_dates();
                    self.build_tags_from_memos();
                    self.apply_filters();
                }
                self.set_status("笔记已删除", StatusKind::Success);
            }
            ApiResponse::TagTreeLoaded(tags) => {
                if !tags.is_empty() {
                    self.all_tags = tags;
                }
            }
            ApiResponse::Error(msg) => {
                self.set_status(&msg, StatusKind::Error);
                self.is_loading = false;
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent, req_tx: &std::sync::mpsc::Sender<ApiRequest>) -> bool {
        match self.mode {
            Mode::Login => self.handle_login_key(key, req_tx),
            Mode::Normal => self.handle_normal_key(key, req_tx),
            Mode::Search => self.handle_search_key(key),
            Mode::Edit => self.handle_edit_key(key, req_tx),
            Mode::Calendar => self.handle_calendar_key(key, req_tx),
            Mode::Tags => self.handle_tags_key(key, req_tx),
        }
    }

    fn handle_login_key(
        &mut self,
        key: KeyEvent,
        req_tx: &std::sync::mpsc::Sender<ApiRequest>,
    ) -> bool {
        if self.login_step == 0 {
            match key.code {
                KeyCode::Enter => {
                    if !self.login_email.is_empty() {
                        self.login_step = 1;
                        self.login_error = None;
                    }
                }
                KeyCode::Char(c) => self.login_email.push(c),
                KeyCode::Backspace => {
                    self.login_email.pop();
                }
                KeyCode::Esc => {
                    if self.token.is_none() {
                        return true;
                    }
                    self.mode = Mode::Normal;
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Enter => {
                    if !self.login_password.is_empty() {
                        let email = self.login_email.clone();
                        let password = self.login_password.clone();
                        let _ = req_tx.send(ApiRequest::Login { email, password });
                        self.login_error = None;
                    }
                }
                KeyCode::Char(c) => self.login_password.push(c),
                KeyCode::Backspace => {
                    self.login_password.pop();
                }
                KeyCode::Esc => {
                    self.login_step = 0;
                    self.login_password.clear();
                }
                _ => {}
            }
        }
        false
    }

    fn handle_normal_key(
        &mut self,
        key: KeyEvent,
        req_tx: &std::sync::mpsc::Sender<ApiRequest>,
    ) -> bool {
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('/') => {
                self.mode = Mode::Search;
                self.search_query.clear();
            }
            KeyCode::Char('n') => {
                self.edit_lines = vec![String::new()];
                self.edit_cursor_row = 0;
                self.edit_cursor_col = 0;
                self.edit_scroll = 0;
                self.edit_is_new = true;
                self.edit_original_slug.clear();
                self.mode = Mode::Edit;
            }
            KeyCode::Char('e') => {
                if let Some(memo) = self.current_memo().cloned() {
                    let text = memo.content_text();
                    self.edit_lines = if text.is_empty() {
                        vec![String::new()]
                    } else {
                        text.lines().map(|l| l.to_string()).collect()
                    };
                    self.edit_cursor_row = 0;
                    self.edit_cursor_col = 0;
                    self.edit_scroll = 0;
                    self.edit_is_new = false;
                    self.edit_original_slug = memo.slug.clone();
                    self.mode = Mode::Edit;
                } else {
                    self.set_status("请先选择笔记", StatusKind::Error);
                }
            }
            KeyCode::Char('d') => {
                if let Some(memo) = self.current_memo() {
                    let _ = req_tx.send(ApiRequest::DeleteMemo {
                        slug: memo.slug.clone(),
                    });
                }
            }
            KeyCode::Char('s') => {
                if self.token.is_some() {
                    self.is_loading = true;
                    self.set_status("正在同步...", StatusKind::Info);
                    let _ = req_tx.send(ApiRequest::ListMemos);
                    let _ = req_tx.send(ApiRequest::GetTagTree);
                } else {
                    self.mode = Mode::Login;
                }
            }
            KeyCode::Char('t') => {
                if self.all_tags.is_empty() {
                    self.build_tags_from_memos();
                }
                if !self.all_tags.is_empty() {
                    self.tag_index = 0;
                    self.tag_scroll = 0;
                    self.mode = Mode::Tags;
                } else {
                    self.set_status("暂无标签", StatusKind::Info);
                }
            }
            KeyCode::Char('D') => {
                self.rebuild_calendar_memo_dates();
                let today = Local::now().date_naive();
                self.cal_year = today.year();
                self.cal_month = today.month();
                self.cal_cursor = today;
                self.mode = Mode::Calendar;
            }
            KeyCode::Char('j') | KeyCode::Down => self.move_sidebar(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_sidebar(0),
            KeyCode::Char('G') => {
                if let Some(len) = self.filtered_indices.len().checked_sub(1) {
                    self.sidebar_index = len;
                }
            }
            KeyCode::Char('g') => self.sidebar_index = 0,
            KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => self.focus = Focus::Main,
            KeyCode::Esc => {
                self.filter_tag = None;
                self.filter_date = None;
                self.search_query.clear();
                self.apply_filters();
                self.set_status("已清除筛选", StatusKind::Info);
            }
            KeyCode::Char('J') => {
                self.main_scroll = self.main_scroll.saturating_add(1);
            }
            KeyCode::Char('K') => {
                self.main_scroll = self.main_scroll.saturating_sub(1);
            }
            KeyCode::PageDown => {
                let page = (self.detail_visible_height as usize).max(1);
                self.main_scroll = self.main_scroll.saturating_add(page as u16);
            }
            KeyCode::PageUp => {
                let page = (self.detail_visible_height as usize).max(1);
                self.main_scroll = self.main_scroll.saturating_sub(page as u16);
            }
            _ => {}
        }
        false
    }

    fn move_sidebar(&mut self, direction: usize) {
        let len = self.filtered_indices.len();
        if len == 0 {
            return;
        }
        if direction == 1 {
            if self.sidebar_index < len - 1 {
                self.sidebar_index += 1;
            }
        } else {
            if self.sidebar_index > 0 {
                self.sidebar_index -= 1;
            }
        }
        self.main_scroll = 0;
    }

    fn handle_search_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Enter => {
                self.apply_filters();
                self.mode = Mode::Normal;
            }
            KeyCode::Esc => {
                self.search_query.clear();
                self.apply_filters();
                self.mode = Mode::Normal;
            }
            KeyCode::Char(c) => self.search_query.push(c),
            KeyCode::Backspace => {
                self.search_query.pop();
                self.apply_filters();
            }
            _ => {}
        }
        false
    }

    fn handle_edit_key(
        &mut self,
        key: KeyEvent,
        req_tx: &std::sync::mpsc::Sender<ApiRequest>,
    ) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('s') {
            let content = self.edit_lines.join("\n");
            if content.trim().is_empty() {
                self.set_status("内容不能为空", StatusKind::Error);
                return false;
            }
            if self.edit_is_new {
                let _ = req_tx.send(ApiRequest::CreateMemo { content });
            } else {
                let slug = self.edit_original_slug.clone();
                let _ = req_tx.send(ApiRequest::UpdateMemo { slug, content });
            }
            self.mode = Mode::Normal;
            return false;
        }
        if key.code == KeyCode::Esc {
            self.mode = Mode::Normal;
            return false;
        }

        match key.code {
            KeyCode::Char(c) => {
                let byte_off = col_to_byte_offset(&self.edit_lines[self.edit_cursor_row], self.edit_cursor_col);
                self.edit_lines[self.edit_cursor_row].insert(byte_off, c);
                self.edit_cursor_col += UnicodeWidthChar::width(c).unwrap_or(1);
            }
            KeyCode::Backspace => {
                if self.edit_cursor_col > 0 {
                    let byte_off = col_to_byte_offset(&self.edit_lines[self.edit_cursor_row], self.edit_cursor_col);
                    let prev_ch = self.edit_lines[self.edit_cursor_row][..byte_off]
                        .chars().last().unwrap();
                    let prev_w = UnicodeWidthChar::width(prev_ch).unwrap_or(1);
                    let prev_bytes = prev_ch.len_utf8();
                    self.edit_cursor_col -= prev_w;
                    self.edit_lines[self.edit_cursor_row].drain((byte_off - prev_bytes)..byte_off);
                } else if self.edit_cursor_row > 0 {
                    self.edit_cursor_row -= 1;
                    let prev_len = self.edit_lines[self.edit_cursor_row]
                        .chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(1)).sum();
                    let line = self.edit_lines.remove(self.edit_cursor_row + 1);
                    self.edit_lines[self.edit_cursor_row].push_str(&line);
                    self.edit_cursor_col = prev_len;
                }
            }
            KeyCode::Delete => {
                let line = &self.edit_lines[self.edit_cursor_row];
                let byte_off = col_to_byte_offset(line, self.edit_cursor_col);
                if byte_off < line.len() {
                    let del_end = byte_off + line[byte_off..].chars().next().map(|c| c.len_utf8()).unwrap_or(1);
                    self.edit_lines[self.edit_cursor_row].drain(byte_off..del_end);
                } else if self.edit_cursor_row + 1 < self.edit_lines.len() {
                    let next = self.edit_lines.remove(self.edit_cursor_row + 1);
                    self.edit_lines[self.edit_cursor_row].push_str(&next);
                }
            }
            KeyCode::Enter => {
                let byte_off = col_to_byte_offset(&self.edit_lines[self.edit_cursor_row], self.edit_cursor_col);
                let rest: String = self.edit_lines[self.edit_cursor_row][byte_off..].to_string();
                self.edit_lines[self.edit_cursor_row].truncate(byte_off);
                self.edit_lines.insert(self.edit_cursor_row + 1, rest);
                self.edit_cursor_row += 1;
                self.edit_cursor_col = 0;
            }
            KeyCode::Left => {
                if self.edit_cursor_col > 0 {
                    let line = &self.edit_lines[self.edit_cursor_row];
                    let byte_off = col_to_byte_offset(line, self.edit_cursor_col);
                    let prev_w = line[..byte_off]
                        .chars()
                        .last()
                        .map(|c| UnicodeWidthChar::width(c).unwrap_or(1))
                        .unwrap_or(1);
                    self.edit_cursor_col -= prev_w;
                } else if self.edit_cursor_row > 0 {
                    self.edit_cursor_row -= 1;
                    self.edit_cursor_col = self.edit_lines[self.edit_cursor_row].chars()
                        .map(|c| UnicodeWidthChar::width(c).unwrap_or(1))
                        .sum();
                }
            }
            KeyCode::Right => {
                let line = &self.edit_lines[self.edit_cursor_row];
                let max_col = line.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(1)).sum();
                if self.edit_cursor_col < max_col {
                    let byte_off = col_to_byte_offset(line, self.edit_cursor_col);
                    let next_c = line[byte_off..].chars().next();
                    self.edit_cursor_col += next_c.map(|c| UnicodeWidthChar::width(c).unwrap_or(1)).unwrap_or(1);
                } else if self.edit_cursor_row + 1 < self.edit_lines.len() {
                    self.edit_cursor_row += 1;
                    self.edit_cursor_col = 0;
                }
            }
            KeyCode::Up => {
                if self.edit_cursor_row > 0 {
                    self.edit_cursor_row -= 1;
                    let line = &self.edit_lines[self.edit_cursor_row];
                    let max_col = line.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(1)).sum();
                    if self.edit_cursor_col > max_col {
                        self.edit_cursor_col = max_col;
                    }
                }
            }
            KeyCode::Down => {
                if self.edit_cursor_row + 1 < self.edit_lines.len() {
                    self.edit_cursor_row += 1;
                    let line = &self.edit_lines[self.edit_cursor_row];
                    let max_col = line.chars().map(|c| UnicodeWidthChar::width(c).unwrap_or(1)).sum();
                    if self.edit_cursor_col > max_col {
                        self.edit_cursor_col = max_col;
                    }
                }
            }
            KeyCode::Home => self.edit_cursor_col = 0,
            KeyCode::End => {
                self.edit_cursor_col = self.edit_lines[self.edit_cursor_row].chars()
                    .map(|c| UnicodeWidthChar::width(c).unwrap_or(1))
                    .sum();
            }
            _ => {}
        }
        false
    }

    fn handle_calendar_key(
        &mut self,
        key: KeyEvent,
        _req_tx: &std::sync::mpsc::Sender<ApiRequest>,
    ) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let cur = self.cal_cursor;

        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                self.filter_date = Some(self.cal_cursor);
                self.apply_filters();
                self.mode = Mode::Normal;
                self.set_status(
                    &format!(
                        "按日期筛选: {}",
                        self.cal_cursor.format("%Y-%m-%d")
                    ),
                    StatusKind::Info,
                );
            }
            KeyCode::Left | KeyCode::Char('h') if !ctrl => {
                if let Some(d) = cur.pred_opt() {
                    self.cal_cursor = d;
                }
            }
            KeyCode::Right | KeyCode::Char('l') if !ctrl => {
                if let Some(d) = cur.succ_opt() {
                    self.cal_cursor = d;
                }
            }
            KeyCode::Up | KeyCode::Char('k') if !ctrl => {
                if let Some(d) = cur.checked_sub_days(chrono::Days::new(7)) {
                    self.cal_cursor = d;
                }
            }
            KeyCode::Down | KeyCode::Char('j') if !ctrl => {
                self.cal_cursor = cur + chrono::Days::new(7);
            }
            KeyCode::Left | KeyCode::Char('h') if ctrl => {
                if let Some(d) = NaiveDate::from_ymd_opt(self.cal_year, self.cal_month, 1)
                    .and_then(|m| m.checked_sub_months(Months::new(1)))
                {
                    self.cal_year = d.year();
                    self.cal_month = d.month();
                }
            }
            KeyCode::Right | KeyCode::Char('l') if ctrl => {
                if let Some(d) = NaiveDate::from_ymd_opt(self.cal_year, self.cal_month, 1)
                    .and_then(|m| m.checked_add_months(Months::new(1)))
                {
                    self.cal_year = d.year();
                    self.cal_month = d.month();
                }
            }
            KeyCode::Up | KeyCode::Char('k') if ctrl => {
                self.cal_year -= 1;
            }
            KeyCode::Down | KeyCode::Char('j') if ctrl => {
                self.cal_year += 1;
            }
            _ => {}
        }
        false
    }

    fn handle_tags_key(
        &mut self,
        key: KeyEvent,
        _req_tx: &std::sync::mpsc::Sender<ApiRequest>,
    ) -> bool {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
            }
            KeyCode::Enter => {
                if let Some(tag) = self.all_tags.get(self.tag_index) {
                    let tag_name = tag.name.clone();
                    self.filter_tag = Some(tag_name.clone());
                    self.apply_filters();
                    self.mode = Mode::Normal;
                    self.set_status(
                        &format!("按标签筛选: #{}", tag_name),
                        StatusKind::Info,
                    );
                }
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.tag_index + 1 < self.all_tags.len() {
                    self.tag_index += 1;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.tag_index > 0 {
                    self.tag_index -= 1;
                }
            }
            KeyCode::Char('q') => {
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        false
    }
}

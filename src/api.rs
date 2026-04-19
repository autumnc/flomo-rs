use chrono::{NaiveDate, NaiveDateTime};
use md5::{Digest, Md5};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const API_BASE: &str = "https://flomoapp.com/api/v1";
const API_KEY: &str = "flomo_web";
const APP_VERSION: &str = "4.0";
const PLATFORM: &str = "web";
const SIGN_SECRET: &str = "dbbc3dd73364b4084c3a69346e0ce2b2";
const TIMEZONE: &str = "8:0";
const MAX_PAGE_SIZE: i32 = 200;

fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".flomo-cli")
}

fn token_path() -> PathBuf {
    config_dir().join("token.json")
}

pub fn load_token() -> Option<String> {
    let path = token_path();
    if !path.exists() {
        return None;
    }
    let data: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    data.get("access_token").and_then(|v| v.as_str()).map(|s| s.to_string())
}

pub fn save_token_to_file(data: &Value) {
    let dir = config_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(token_path(), serde_json::to_string_pretty(data).unwrap_or_default());
}

pub fn clear_token_file() {
    let _ = std::fs::remove_file(token_path());
}

fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

fn base_params() -> HashMap<String, String> {
    let mut p = HashMap::new();
    p.insert("timestamp".into(), timestamp());
    p.insert("api_key".into(), API_KEY.into());
    p.insert("app_version".into(), APP_VERSION.into());
    p.insert("platform".into(), PLATFORM.into());
    p.insert("webp".into(), "1".into());
    p
}

fn generate_sign(params: &HashMap<String, String>) -> String {
    let mut keys: Vec<&String> = params.keys().collect();
    keys.sort();
    let parts: Vec<String> = keys
        .iter()
        .filter_map(|k| {
            let v = params.get(*k)?;
            if v.is_empty() {
                return None;
            }
            Some(format!("{}={}", k, v))
        })
        .collect();
    let raw = format!("{}{}", parts.join("&"), SIGN_SECRET);
    let mut hasher = Md5::new();
    hasher.update(raw.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn signed_params(extra: Option<HashMap<String, String>>) -> HashMap<String, String> {
    let mut p = base_params();
    if let Some(e) = extra {
        p.extend(e);
    }
    let sign = generate_sign(&p);
    p.insert("sign".into(), sign);
    p
}

#[derive(Debug, Clone)]
pub struct FlomoClient {
    client: Client,
    token: String,
}

impl FlomoClient {
    pub fn new(token: &str) -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            token: token.to_string(),
        }
    }

    async fn get(&self, path: &str, extra: Option<HashMap<String, String>>) -> Result<Value, String> {
        let params = signed_params(extra);
        let resp = self
            .client
            .get(format!("{}/{}", API_BASE, path))
            .query(&params)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| format!("网络错误: {}", e))?;
        handle_response(resp).await
    }

    async fn put(&self, path: &str, data: HashMap<String, String>) -> Result<Value, String> {
        let mut body = base_params();
        body.extend(data);
        let sign = generate_sign(&body);
        body.insert("sign".into(), sign);
        let resp = self
            .client
            .put(format!("{}/{}", API_BASE, path))
            .header("Authorization", format!("Bearer {}", self.token))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("网络错误: {}", e))?;
        handle_response(resp).await
    }

    async fn delete_req(&self, path: &str) -> Result<Value, String> {
        let params = signed_params(None);
        let resp = self
            .client
            .delete(format!("{}/{}", API_BASE, path))
            .query(&params)
            .header("Authorization", format!("Bearer {}", self.token))
            .send()
            .await
            .map_err(|e| format!("网络错误: {}", e))?;
        handle_response(resp).await
    }

    pub async fn login(email: &str, password: &str) -> Result<Value, String> {
        let mut params: HashMap<String, String> = HashMap::new();
        params.insert("email".into(), email.into());
        params.insert("password".into(), password.into());
        params.insert("wechat_union_id".into(), String::new());
        params.insert("wechat_oa_open_id".into(), String::new());
        params.insert("timestamp".into(), timestamp());
        params.insert("api_key".into(), API_KEY.into());
        params.insert("app_version".into(), APP_VERSION.into());
        params.insert("platform".into(), PLATFORM.into());
        params.insert("webp".into(), "1".into());
        let sign = generate_sign(&params);
        params.insert("sign".into(), sign);

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let resp = client
            .post(format!("{}/user/login_by_email", API_BASE))
            .json(&params)
            .send()
            .await
            .map_err(|e| format!("网络错误: {}", e))?;
        handle_response(resp).await
    }

    pub async fn list_memos(&self) -> Result<Vec<Memo>, String> {
        let mut all_memos: Vec<Memo> = Vec::new();
        let mut latest_updated_at: i64 = 0;
        let mut latest_slug = String::new();

        loop {
            let mut extra = HashMap::new();
            extra.insert("limit".into(), MAX_PAGE_SIZE.to_string());
            extra.insert("latest_updated_at".into(), latest_updated_at.to_string());
            extra.insert("tz".into(), TIMEZONE.into());
            if !latest_slug.is_empty() {
                extra.insert("latest_slug".into(), latest_slug.clone());
            }

            let result = self.get("memo/updated/", Some(extra)).await?;
            let memos_json: Value = match result {
                Value::Array(_) => result,
                Value::Object(obj) => obj.get("data").cloned().unwrap_or(Value::Array(vec![])),
                _ => Value::Array(vec![]),
            };

            let page: Vec<Memo> = serde_json::from_value(memos_json).unwrap_or_default();
            let page: Vec<Memo> = page.into_iter().filter(|m| m.deleted_at.is_none()).collect();

            if page.is_empty() {
                break;
            }

            if let Some(last) = page.last() {
                latest_slug = last.slug.clone();
                if let Ok(dt) = NaiveDateTime::parse_from_str(&last.updated_at, "%Y-%m-%d %H:%M:%S") {
                    latest_updated_at = dt.and_utc().timestamp();
                } else {
                    break;
                }
            }
            all_memos.extend(page);
        }

        all_memos.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(all_memos)
    }

    pub async fn create_memo(&self, content: &str) -> Result<Memo, String> {
        let mut data = HashMap::new();
        data.insert("content".into(), text_to_html(content));
        data.insert("source".into(), "web".into());
        data.insert("tz".into(), TIMEZONE.into());
        let result = self.put("memo", data).await?;
        serde_json::from_value(result).map_err(|e| e.to_string())
    }

    pub async fn update_memo(&self, slug: &str, content: &str) -> Result<Memo, String> {
        let mut data = HashMap::new();
        data.insert("content".into(), text_to_html(content));
        data.insert("source".into(), "web".into());
        data.insert("tz".into(), TIMEZONE.into());
        let result = self.put(&format!("memo/{}", slug), data).await?;
        serde_json::from_value(result).map_err(|e| e.to_string())
    }

    pub async fn delete_memo(&self, slug: &str) -> Result<(), String> {
        self.delete_req(&format!("memo/{}", slug)).await?;
        Ok(())
    }

    pub async fn get_tag_tree(&self) -> Result<Vec<TagInfo>, String> {
        let result = self.get("tag/tree", None).await?;
        parse_tag_tree(&result)
    }
}

async fn handle_response(resp: reqwest::Response) -> Result<Value, String> {
    let _status = resp.status();
    let body: Value = resp.json().await.map_err(|e| format!("无效的JSON响应: {}", e))?;
    let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    let message = body
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if code == 0 {
        return Ok(body.get("data").cloned().unwrap_or(body));
    }
    if code == -10 || code == -20 {
        return Err(format!("Token已过期，请重新登录: {}", message));
    }
    Err(format!("API错误(code={}): {}", code, message))
}

// ─── Data Types ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memo {
    pub slug: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub files: Vec<Value>,
}

/// Information about an image extracted from a memo
#[derive(Debug, Clone)]
pub struct ImageInfo {
    pub url: String,
    pub display_index: usize,
}

impl Memo {
    pub fn content_text(&self) -> String {
        html_to_text(&self.content)
    }

    /// Extract text content with image placeholders replaced.
    /// Returns (text_with_placeholders, list_of_image_infos)
    pub fn content_text_with_images(&self) -> (String, Vec<ImageInfo>) {
        let mut images = Vec::new();
        let mut img_index = 0usize;

        // First convert HTML line-break tags to newlines
        let text = self
            .content
            .replace("<br>", "\n")
            .replace("<br/>", "\n")
            .replace("<br />", "\n")
            .replace("</p>", "\n");

        // Replace <img> tags with placeholders BEFORE stripping other tags
        let img_re = Regex::new(r#"<img[^>]*src=["']([^"']+)["'][^>]*>"#).unwrap();
        let text = img_re.replace_all(&text, |caps: &regex::Captures| {
            let url = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            img_index += 1;
            images.push(ImageInfo {
                url: url.to_string(),
                display_index: img_index,
            });
            format!("\n[🖼 图片 {}]\n", img_index)
        });

        // Strip remaining HTML tags
        let re = Regex::new(r"<[^>]+>").unwrap();
        let text = re.replace_all(&text, "");
        let text = text
            .replace("&nbsp;", " ")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");

        // Also extract image URLs from files array
        for file in &self.files {
            let file_type = file.get("type").and_then(|v| v.as_str()).unwrap_or("");
            if file_type == "image" {
                if let Some(url) = file.get("url").and_then(|v| v.as_str()) {
                    // Only add if not already found in content
                    if !images.iter().any(|img| img.url == url) {
                        img_index += 1;
                        images.push(ImageInfo {
                            url: url.to_string(),
                            display_index: img_index,
                        });
                    }
                }
                if let Some(url) = file.get("file_path").and_then(|v| v.as_str()) {
                    if !images.iter().any(|img| img.url == url) {
                        img_index += 1;
                        images.push(ImageInfo {
                            url: url.to_string(),
                            display_index: img_index,
                        });
                    }
                }
                if let Some(url) = file.get("thumb").and_then(|v| v.as_str()) {
                    if !images.iter().any(|img| img.url == url) {
                        img_index += 1;
                        images.push(ImageInfo {
                            url: url.to_string(),
                            display_index: img_index,
                        });
                    }
                }
            }
        }

        let result = text
            .split('\n')
            .map(|l| l.trim().to_string())
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_string();

        (result, images)
    }

    pub fn preview(&self, max_width: usize) -> String {
        let text = self.content_text();
        let first_line = text.lines().next().unwrap_or("");
        let w = unicode_width::UnicodeWidthStr::width(first_line);
        if w > max_width {
            let mut chars = first_line.chars();
            let mut result = String::new();
            let mut cw: usize = 0;
            while let Some(c) = chars.next() {
                let char_w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                if cw + char_w > max_width.saturating_sub(3) {
                    result.push_str("...");
                    break;
                }
                cw += char_w;
                result.push(c);
            }
            result
        } else {
            first_line.to_string()
        }
    }

    pub fn created_date(&self) -> Option<NaiveDate> {
        NaiveDateTime::parse_from_str(&self.created_at, "%Y-%m-%d %H:%M:%S")
            .ok()
            .map(|dt| dt.date())
    }

    pub fn tags_display(&self) -> String {
        if self.tags.is_empty() {
            String::new()
        } else {
            self.tags.iter().map(|t| format!("#{}", t)).collect::<Vec<_>>().join(" ")
        }
    }
}

#[derive(Debug, Clone)]
pub struct TagInfo {
    pub name: String,
    pub count: usize,
}

fn parse_tag_tree(value: &Value) -> Result<Vec<TagInfo>, String> {
    let mut tags = Vec::new();
    if let Some(arr) = value.as_array() {
        collect_tags(arr, &mut tags);
    } else if let Some(obj) = value.as_object() {
        if let Some(arr) = obj.get("data").and_then(|v| v.as_array()) {
            collect_tags(arr, &mut tags);
        }
    }
    tags.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(tags)
}

fn collect_tags(arr: &[Value], out: &mut Vec<TagInfo>) {
    for item in arr {
        if let Some(name) = item.get("tag").and_then(|v| v.as_str()) {
            let count = item.get("count").and_then(|v| v.as_i64()).unwrap_or(1) as usize;
            out.push(TagInfo {
                name: name.to_string(),
                count,
            });
        }
        if let Some(children) = item.get("children").and_then(|v| v.as_array()) {
            collect_tags(children, out);
        }
    }
}

// ─── HTML Helpers ─────────────────────────────────────────────────────────

pub fn html_to_text(html: &str) -> String {
    // First convert HTML line-break tags to newlines BEFORE stripping tags
    let text = html
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n")
        .replace("</p>", "\n");
    // Then strip remaining HTML tags
    let re = Regex::new(r"<[^>]+>").unwrap();
    let text = re.replace_all(&text, "");
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .split('\n')
        .map(|l| l.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn text_to_html(text: &str) -> String {
    if text.starts_with('<') {
        return text.to_string();
    }
    text.split('\n')
        .map(|line| {
            if line.trim().is_empty() {
                "<p><br></p>".to_string()
            } else {
                format!("<p>{}</p>", line)
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

pub fn extract_tags(text: &str) -> Vec<String> {
    let re = Regex::new(r"#([^\s#]+)").unwrap();
    re.captures_iter(text)
        .map(|cap| cap[1].to_string())
        .collect()
}

// ─── API Message Types ────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ApiRequest {
    Login { email: String, password: String },
    ListMemos,
    CreateMemo { content: String },
    UpdateMemo { slug: String, content: String },
    DeleteMemo { slug: String },
    GetTagTree,
}

#[derive(Debug)]
pub enum ApiResponse {
    LoginOk { token: String, user_name: String },
    LoginErr(String),
    MemosLoaded(Vec<Memo>),
    MemoCreated(Memo),
    MemoUpdated(Memo),
    MemoDeleted,
    TagTreeLoaded(Vec<TagInfo>),
    Error(String),
}

pub async fn process_request(req: ApiRequest, _client: &Client, token: &str) -> ApiResponse {
    match req {
        ApiRequest::Login { email, password } => match FlomoClient::login(&email, &password).await {
            Ok(data) => {
                let tk = data
                    .get("access_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let name = data
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("用户")
                    .to_string();
                save_token_to_file(&data);
                ApiResponse::LoginOk {
                    token: tk,
                    user_name: name,
                }
            }
            Err(e) => ApiResponse::LoginErr(e),
        },
        ApiRequest::ListMemos => {
            let fc = FlomoClient::new(token);
            match fc.list_memos().await {
                Ok(memos) => ApiResponse::MemosLoaded(memos),
                Err(e) => ApiResponse::Error(format!("同步失败: {}", e)),
            }
        }
        ApiRequest::CreateMemo { content } => {
            let fc = FlomoClient::new(token);
            match fc.create_memo(&content).await {
                Ok(memo) => ApiResponse::MemoCreated(memo),
                Err(e) => ApiResponse::Error(format!("创建失败: {}", e)),
            }
        }
        ApiRequest::UpdateMemo { slug, content } => {
            let fc = FlomoClient::new(token);
            match fc.update_memo(&slug, &content).await {
                Ok(memo) => ApiResponse::MemoUpdated(memo),
                Err(e) => ApiResponse::Error(format!("更新失败: {}", e)),
            }
        }
        ApiRequest::DeleteMemo { slug } => {
            let fc = FlomoClient::new(token);
            match fc.delete_memo(&slug).await {
                Ok(()) => ApiResponse::MemoDeleted,
                Err(e) => ApiResponse::Error(format!("删除失败: {}", e)),
            }
        }
        ApiRequest::GetTagTree => {
            let fc = FlomoClient::new(token);
            match fc.get_tag_tree().await {
                Ok(tags) => ApiResponse::TagTreeLoaded(tags),
                Err(e) => ApiResponse::Error(format!("获取标签失败: {}", e)),
            }
        }
    }
}

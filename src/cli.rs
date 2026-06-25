use std::env;
use std::io::{self, IsTerminal, Read};
use std::process;

use chrono::Datelike;
use serde::Serialize;

use crate::api::{self, FlomoClient, Memo};

// ─── CLI Types ──────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum Command {
    Login {
        email: Option<String>,
        password: Option<String>,
    },
    Logout,
    Status,
    List {
        limit: usize,
        json: bool,
    },
    Get {
        slug: String,
        json: bool,
    },
    New {
        content: Option<String>,
        file: Option<String>,
        json: bool,
    },
    Edit {
        slug: String,
        content: String,
        json: bool,
    },
    Delete {
        slug: String,
        yes: bool,
        json: bool,
    },
    Search {
        keyword: String,
        tag: Option<String>,
        json: bool,
    },
    Tags {
        json: bool,
    },
    Review {
        json: bool,
    },
}

pub struct CliArgs {
    pub command: Option<Command>,
    pub token: Option<String>,
    pub high_contrast: bool,
    pub version: bool,
}

// ─── Parse Args ─────────────────────────────────────────────────────────────

pub fn parse_args() -> CliArgs {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut token = None;
    let mut high_contrast = false;
    let mut version = false;
    let mut help = false;
    let mut json_global = false;

    // Build remaining args (excluding global flags and their values)
    let mut remaining: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--token" => {
                i += 1;
                if i < args.len() {
                    token = Some(args[i].clone());
                }
                i += 1;
            }
            "-hc" => {
                high_contrast = true;
                i += 1;
            }
            "--version" | "-V" => {
                version = true;
                i += 1;
            }
            "--help" | "-h" => {
                help = true;
                i += 1;
            }
            "--json" => {
                json_global = true;
                i += 1;
            }
            _ => {
                remaining.push(args[i].clone());
                i += 1;
            }
        }
    }

    // --help / -h prints usage
    if help {
        print_usage();
        process::exit(0);
    }

    if remaining.is_empty() {
        return CliArgs {
            command: None,
            token,
            high_contrast,
            version,
        };
    }

    // Determine command from first remaining arg
    let cmd = match remaining[0].as_str() {
        "login" => parse_login(&remaining, json_global),
        "logout" => Some(Command::Logout),
        "status" => Some(Command::Status),
        "list" => parse_list(&remaining, json_global),
        "get" => parse_get(&remaining, json_global),
        "search" => parse_search(&remaining, json_global),
        "new" => parse_new_cmd(&remaining, json_global),
        "edit" => parse_edit(&remaining, json_global),
        "delete" => parse_delete(&remaining, json_global),
        "tags" => {
            let json = json_global || remaining.iter().any(|f| f == "--json");
            Some(Command::Tags { json })
        }
        "review" => {
            let json = json_global || remaining.iter().any(|f| f == "--json");
            Some(Command::Review { json })
        }
        _ => {
            eprintln!("未知命令: {}", remaining[0]);
            print_usage();
            process::exit(1);
        }
    };

    CliArgs {
        command: cmd,
        token,
        high_contrast,
        version,
    }
}

fn next_arg(remaining: &[String], i: &mut usize) -> Option<String> {
    if *i < remaining.len() {
        let val = remaining[*i].clone();
        *i += 1;
        Some(val)
    } else {
        None
    }
}

fn parse_login(remaining: &[String], json_global: bool) -> Option<Command> {
    let _json = json_global;
    let mut email = None;
    let mut password = None;
    let mut i = 1; // skip "login"
    while i < remaining.len() {
        match remaining[i].as_str() {
            "--email" => {
                i += 1;
                email = next_arg(remaining, &mut i);
            }
            "--password" => {
                i += 1;
                password = next_arg(remaining, &mut i);
            }
            _ => {
                i += 1;
            }
        }
    }
    Some(Command::Login { email, password })
}

fn parse_list(remaining: &[String], json_global: bool) -> Option<Command> {
    let mut limit = 20;
    let mut json = json_global;
    let mut i = 1; // skip "list"
    while i < remaining.len() {
        match remaining[i].as_str() {
            "--limit" => {
                i += 1;
                if let Some(v) = next_arg(remaining, &mut i) {
                    limit = v.parse().unwrap_or(20);
                }
            }
            "--json" => {
                json = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    Some(Command::List { limit, json })
}

fn parse_get(remaining: &[String], json_global: bool) -> Option<Command> {
    let slug = remaining.get(1).cloned().unwrap_or_default();
    let json = json_global || remaining.iter().any(|f| f == "--json");
    Some(Command::Get { slug, json })
}

fn parse_new_cmd(remaining: &[String], json_global: bool) -> Option<Command> {
    let mut content = None;
    let mut file = None;
    let mut json = json_global;
    let mut i = 1; // skip "new"
    while i < remaining.len() {
        match remaining[i].as_str() {
            "-f" => {
                i += 1;
                file = next_arg(remaining, &mut i);
            }
            "--json" => {
                json = true;
                i += 1;
            }
            other if !other.starts_with('-') && content.is_none() && file.is_none() => {
                content = Some(other.to_string());
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    Some(Command::New { content, file, json })
}

fn parse_edit(remaining: &[String], json_global: bool) -> Option<Command> {
    let slug = remaining.get(1).cloned().unwrap_or_default();
    let content = remaining.get(2).cloned().unwrap_or_default();
    let json = json_global || remaining.iter().any(|f| f == "--json");
    Some(Command::Edit { slug, content, json })
}

fn parse_delete(remaining: &[String], json_global: bool) -> Option<Command> {
    let slug = remaining.get(1).cloned().unwrap_or_default();
    let yes = remaining.iter().any(|f| f == "-y");
    let json = json_global || remaining.iter().any(|f| f == "--json");
    Some(Command::Delete { slug, yes, json })
}

fn parse_search(remaining: &[String], json_global: bool) -> Option<Command> {
    let keyword = remaining.get(1).cloned().unwrap_or_default();
    let mut tag = None;
    let mut json = json_global;
    let mut i = 2; // skip "search" and keyword
    while i < remaining.len() {
        match remaining[i].as_str() {
            "--tag" => {
                i += 1;
                tag = next_arg(remaining, &mut i);
            }
            "--json" => {
                json = true;
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    Some(Command::Search { keyword, tag, json })
}

fn print_usage() {
    eprintln!(
        r#"flomo-rs - flomo 笔记终端客户端

用法:
  flomo-rs                       启动 TUI 交互界面
  flomo-rs login --email E --password P  登录
  flomo-rs logout                退出登录
  flomo-rs status                查看登录状态
  flomo-rs list [--limit N] [--json]     列出笔记
  flomo-rs get <slug> [--json]   查看笔记详情
  flomo-rs new [content] [-f FILE] [--json]  新建笔记
  flomo-rs edit <slug> <content> [--json]   编辑笔记
  flomo-rs delete <slug> [-y] [--json]      删除笔记
  flomo-rs search <keyword> [--tag TAG] [--json]  搜索笔记
  flomo-rs tags [--json]         列出标签
  flomo-rs review [--json]       回顾往年今日

全局选项:
  --token TOKEN  指定 token（优先于缓存和 FLOMO_TOKEN 环境变量）
  --version, -V   显示版本
  --json          全局 JSON 输出
  -hc             TUI 高对比度主题
"#
    );
}

// ─── JSON Output ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct JsonOk<T: Serialize> {
    ok: bool,
    data: T,
}

#[derive(Serialize)]
struct JsonError {
    ok: bool,
    error: String,
}

fn is_tty() -> bool {
    io::stdout().is_terminal()
}

fn output<T: Serialize>(json: bool, data: T) {
    if json || !is_tty() {
        let envelope = JsonOk { ok: true, data };
        println!("{}", serde_json::to_string(&envelope).unwrap_or_default());
    }
}

fn output_error(json: bool, msg: &str) {
    if json || !is_tty() {
        let envelope = JsonError {
            ok: false,
            error: msg.to_string(),
        };
        println!("{}", serde_json::to_string(&envelope).unwrap_or_default());
    } else {
        eprintln!("错误: {}", msg);
    }
}

// ─── Token Resolution ──────────────────────────────────────────────────────

fn resolve_token(cli_token: Option<&str>) -> Option<String> {
    if let Some(t) = cli_token {
        return Some(t.to_string());
    }
    if let Ok(t) = env::var("FLOMO_TOKEN") {
        if !t.is_empty() {
            return Some(t);
        }
    }
    api::load_token()
}

// ─── Command Execution ─────────────────────────────────────────────────────

pub async fn run_command(cmd: Command, cli_token: Option<String>) {
    match cmd {
        Command::Login { email, password } => cmd_login(email, password).await,
        Command::Logout => cmd_logout(),
        Command::Status => cmd_status(cli_token).await,
        Command::List { limit, json } => cmd_list(limit, json, cli_token).await,
        Command::Get { slug, json } => cmd_get(&slug, json, cli_token).await,
        Command::New { content, file, json } => cmd_new(content, file, json, cli_token).await,
        Command::Edit { slug, content, json } => cmd_edit(&slug, &content, json, cli_token).await,
        Command::Delete { slug, yes, json } => cmd_delete(&slug, yes, json, cli_token).await,
        Command::Search { keyword, tag, json } => cmd_search(&keyword, tag, json, cli_token).await,
        Command::Tags { json } => cmd_tags(json, cli_token).await,
        Command::Review { json } => cmd_review(json, cli_token).await,
    }
}

// ─── Login ─────────────────────────────────────────────────────────────────

async fn cmd_login(email: Option<String>, password: Option<String>) {
    let email = email.unwrap_or_else(|| {
        eprint!("邮箱: ");
        let mut s = String::new();
        let _ = io::stdin().read_line(&mut s);
        s.trim().to_string()
    });
    let password = password.unwrap_or_else(|| {
        eprint!("密码: ");
        let mut s = String::new();
        let _ = io::stdin().read_line(&mut s);
        s.trim().to_string()
    });

    match FlomoClient::login(&email, &password).await {
        Ok(data) => {
            let name = data
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("用户");
            api::save_token_to_file(&data);
            println!("登录成功，欢迎 {}", name);
        }
        Err(e) => {
            output_error(false, &e);
            process::exit(1);
        }
    }
}

// ─── Logout ────────────────────────────────────────────────────────────────

fn cmd_logout() {
    api::clear_token_file();
    println!("已退出登录");
}

// ─── Status ────────────────────────────────────────────────────────────────

async fn cmd_status(cli_token: Option<String>) {
    let token = resolve_token(cli_token.as_deref());
    match token {
        Some(t) => {
            let client = FlomoClient::new(&t);
            match client.list_memos().await {
                Ok(memos) => {
                    println!("已登录，共 {} 条笔记", memos.len());
                }
                Err(e) => {
                    output_error(false, &format!("Token 无效: {}", e));
                    process::exit(1);
                }
            }
        }
        None => {
            println!("未登录");
        }
    }
}

// ─── List ──────────────────────────────────────────────────────────────────

async fn cmd_list(limit: usize, json: bool, cli_token: Option<String>) {
    let token = match resolve_token(cli_token.as_deref()) {
        Some(t) => t,
        None => {
            output_error(json, "未登录，请先运行 flomo-rs login");
            process::exit(1);
        }
    };

    let client = FlomoClient::new(&token);
    match client.list_memos().await {
        Ok(memos) => {
            let count = memos.len().min(limit);
            if json || !is_tty() {
                output(json, &memos[..count]);
            } else {
                for memo in memos.iter().take(limit) {
                    let date = if memo.created_at.len() >= 10 {
                        &memo.created_at[..10]
                    } else {
                        &memo.created_at
                    };
                    let preview = memo.preview(60);
                    println!("{:<12} {:<28} {}", date, memo.slug, preview);
                }
                if memos.len() > limit {
                    println!("... 共 {} 条，仅显示前 {} 条", memos.len(), limit);
                }
            }
        }
        Err(e) => {
            output_error(json, &e);
            process::exit(1);
        }
    }
}

// ─── Get ───────────────────────────────────────────────────────────────────

async fn cmd_get(slug: &str, json: bool, cli_token: Option<String>) {
    let token = match resolve_token(cli_token.as_deref()) {
        Some(t) => t,
        None => {
            output_error(json, "未登录");
            process::exit(1);
        }
    };

    let client = FlomoClient::new(&token);
    match client.list_memos().await {
        Ok(memos) => {
            if let Some(memo) = memos.iter().find(|m| m.slug == slug) {
                if json || !is_tty() {
                    output(json, memo);
                } else {
                    println!("ID:      {}", memo.slug);
                    println!("创建:    {}", memo.created_at);
                    println!("修改:    {}", memo.updated_at);
                    if !memo.tags.is_empty() {
                        println!("标签:    {}", memo.tags_display());
                    }
                    println!();
                    println!("{}", memo.content_text());
                }
            } else {
                output_error(json, &format!("笔记不存在: {}", slug));
                process::exit(1);
            }
        }
        Err(e) => {
            output_error(json, &e);
            process::exit(1);
        }
    }
}

// ─── New ───────────────────────────────────────────────────────────────────

async fn cmd_new(content: Option<String>, file: Option<String>, json: bool, cli_token: Option<String>) {
    let token = match resolve_token(cli_token.as_deref()) {
        Some(t) => t,
        None => {
            output_error(json, "未登录");
            process::exit(1);
        }
    };

    let content = if let Some(c) = content {
        c
    } else if let Some(ref f) = file {
        match std::fs::read_to_string(f) {
            Ok(c) => c,
            Err(e) => {
                output_error(json, &format!("读取文件失败: {}", e));
                process::exit(1);
            }
        }
    } else {
        // Read from stdin
        let mut buf = String::new();
        if io::stdin().read_to_string(&mut buf).is_err() {
            output_error(json, "读取 stdin 失败");
            process::exit(1);
        }
        buf.trim().to_string()
    };

    if content.is_empty() {
        output_error(json, "内容不能为空");
        process::exit(1);
    }

    let client = FlomoClient::new(&token);
    match client.create_memo(&content).await {
        Ok(memo) => {
            if json || !is_tty() {
                output(json, &memo);
            } else {
                println!("笔记已创建: {}", memo.slug);
            }
        }
        Err(e) => {
            output_error(json, &e);
            process::exit(1);
        }
    }
}

// ─── Edit ──────────────────────────────────────────────────────────────────

async fn cmd_edit(slug: &str, content: &str, json: bool, cli_token: Option<String>) {
    let token = match resolve_token(cli_token.as_deref()) {
        Some(t) => t,
        None => {
            output_error(json, "未登录");
            process::exit(1);
        }
    };

    if content.is_empty() {
        output_error(json, "内容不能为空");
        process::exit(1);
    }

    let client = FlomoClient::new(&token);
    match client.update_memo(slug, content).await {
        Ok(memo) => {
            if json || !is_tty() {
                output(json, &memo);
            } else {
                println!("笔记已更新: {}", memo.slug);
            }
        }
        Err(e) => {
            output_error(json, &e);
            process::exit(1);
        }
    }
}

// ─── Delete ────────────────────────────────────────────────────────────────

async fn cmd_delete(slug: &str, yes: bool, json: bool, cli_token: Option<String>) {
    let token = match resolve_token(cli_token.as_deref()) {
        Some(t) => t,
        None => {
            output_error(json, "未登录");
            process::exit(1);
        }
    };

    // Find memo to confirm
    let client = FlomoClient::new(&token);
    if !yes && !json && is_tty() {
        match client.list_memos().await {
            Ok(memos) => {
                if let Some(memo) = memos.iter().find(|m| m.slug == slug) {
                    let preview = memo.preview(80);
                    eprint!("确认删除 \"{}\"? [y/N] ", preview);
                    let mut answer = String::new();
                    let _ = io::stdin().read_line(&mut answer);
                    if answer.trim().to_lowercase() != "y" {
                        println!("已取消");
                        return;
                    }
                }
            }
            Err(_) => {}
        }
    }

    match client.delete_memo(slug).await {
        Ok(()) => {
            if json || !is_tty() {
                let msg = serde_json::json!({"deleted": slug});
                output(json, &msg);
            } else {
                println!("笔记已删除: {}", slug);
            }
        }
        Err(e) => {
            output_error(json, &e);
            process::exit(1);
        }
    }
}

// ─── Search ────────────────────────────────────────────────────────────────

async fn cmd_search(keyword: &str, tag: Option<String>, json: bool, cli_token: Option<String>) {
    let token = match resolve_token(cli_token.as_deref()) {
        Some(t) => t,
        None => {
            output_error(json, "未登录");
            process::exit(1);
        }
    };

    let client = FlomoClient::new(&token);
    match client.list_memos().await {
        Ok(memos) => {
            let kw = keyword.to_lowercase();
            let results: Vec<&Memo> = memos
                .iter()
                .filter(|m| {
                    let text = m.content_text().to_lowercase();
                    let tags = m.tags.join(" ").to_lowercase();
                    let matches_keyword = text.contains(&kw) || tags.contains(&kw);
                    let matches_tag = tag.as_ref().map_or(true, |t| {
                        m.tags.iter().any(|mt| mt.eq_ignore_ascii_case(t))
                    });
                    matches_keyword && matches_tag
                })
                .collect();

            if json || !is_tty() {
                output(json, &results);
            } else {
                for memo in &results {
                    let date = if memo.created_at.len() >= 10 {
                        &memo.created_at[..10]
                    } else {
                        &memo.created_at
                    };
                    let preview = memo.preview(60);
                    println!("{:<12} {:<28} {}", date, memo.slug, preview);
                }
                println!("共 {} 条结果", results.len());
            }
        }
        Err(e) => {
            output_error(json, &e);
            process::exit(1);
        }
    }
}

// ─── Tags ──────────────────────────────────────────────────────────────────

async fn cmd_tags(json: bool, cli_token: Option<String>) {
    let token = match resolve_token(cli_token.as_deref()) {
        Some(t) => t,
        None => {
            output_error(json, "未登录");
            process::exit(1);
        }
    };

    let client = FlomoClient::new(&token);

    // Try remote tag tree; fallback to extracting from memos if empty or fails
    let tags = match client.get_tag_tree().await {
        Ok(tags) if !tags.is_empty() => tags,
        _ => {
            match client.list_memos().await {
                Ok(memos) => {
                    let mut tag_counts: std::collections::HashMap<String, usize> =
                        std::collections::HashMap::new();
                    for m in &memos {
                        for t in &m.tags {
                            *tag_counts.entry(t.clone()).or_insert(0) += 1;
                        }
                    }
                    let mut tags: Vec<crate::api::TagInfo> = tag_counts
                        .into_iter()
                        .map(|(name, count)| crate::api::TagInfo { name, count })
                        .collect();
                    tags.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
                    tags
                }
                Err(e) => {
                    output_error(json, &e);
                    process::exit(1);
                }
            }
        }
    };

    if json || !is_tty() {
        output(json, &tags);
    } else {
        for tag in &tags {
            println!("{:>4}  #{}", tag.count, tag.name);
        }
    }
}

// ─── Review ────────────────────────────────────────────────────────────────

async fn cmd_review(json: bool, cli_token: Option<String>) {
    let token = match resolve_token(cli_token.as_deref()) {
        Some(t) => t,
        None => {
            output_error(json, "未登录");
            process::exit(1);
        }
    };

    let client = FlomoClient::new(&token);
    match client.list_memos().await {
        Ok(memos) => {
            let today = chrono::Local::now().date_naive();
            let month = today.month();
            let day = today.day();

            let reviews: Vec<&Memo> = memos
                .iter()
                .filter(|m| {
                    if let Some(d) = m.created_date() {
                        d.month() == month && d.day() == day && d != today
                    } else {
                        false
                    }
                })
                .collect();

            if json || !is_tty() {
                output(json, &reviews);
            } else {
                if reviews.is_empty() {
                    println!("往年今日无笔记");
                } else {
                    for memo in &reviews {
                        let date = if memo.created_at.len() >= 10 {
                            &memo.created_at[..10]
                        } else {
                            &memo.created_at
                        };
                        let preview = memo.preview(60);
                        println!("{:<12} {}", date, preview);
                    }
                    println!("共 {} 条", reviews.len());
                }
            }
        }
        Err(e) => {
            output_error(json, &e);
            process::exit(1);
        }
    }
}

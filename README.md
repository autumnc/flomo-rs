# flomo-rs

flomo 笔记服务的终端 TUI 客户端，使用 Rust 编写。

## 功能

- 浏览、新建、编辑、删除 flomo 笔记
- 按标签、日期、关键词搜索和筛选
- 日历视图选择日期范围
- 离线优先：本地 JSON 缓存，断网也能查看已有笔记
- 双主题：TokyoNight（默认）和 Obsidian（高对比度）
- 正确处理 CJK 字符宽度

## 安装

```bash
cargo build --release
```

二进制文件在 `target/release/flomo-rs`。

## 使用

```bash
# 默认 TokyoNight 主题
flomo-rs

# Obsidian 高对比度主题
flomo-rs -hc
```

首次启动进入登录页，输入 flomo 账号邮箱和密码。登录后 token 保存在 `~/.flomo-cli/token.json`，后续启动自动登录并同步。

## 快捷键

### 全局

| 按键 | 功能 |
|------|------|
| `n` | 新建笔记 |
| `e` | 编辑当前笔记 |
| `d` | 删除当前笔记 |
| `s` | 手动同步 |
| `/` | 搜索 |
| `t` | 标签筛选 |
| `D` | 日期筛选（日历） |
| `T` | 切换主题 |
| `q` | 退出 |
| `Esc` | 清除筛选 / 返回 |

### 列表浏览

| 按键 | 功能 |
|------|------|
| `j` / `k` / `↑` / `↓` | 上下移动 |
| `g` | 跳到顶部 |
| `G` | 跳到底部 |
| `h` / `l` / `←` / `→` | 切换侧边栏 / 详情焦点 |
| `J` / `K` | 详情区逐行滚动 |
| `PageUp` / `PageDown` | 详情区翻页 |

### 编辑模式

| 按键 | 功能 |
|------|------|
| `Ctrl+s` | 保存 |
| `Esc` | 取消 |

### 日历

| 按键 | 功能 |
|------|------|
| `←` `→` `↑` `↓` | 移动日期 |
| `Ctrl+←` `Ctrl+→` | 切换月份 |
| `Ctrl+↑` `Ctrl+↓` | 切换年份 |
| `Enter` | 确认筛选 |
| `Esc` | 取消 |

## 数据存储

- `~/.flomo-cli/token.json` — 登录 token
- `~/.flomo-cli/memos.json` — 本地笔记缓存

## 技术栈

- [ratatui](https://ratatui.rs) + [crossterm](https://github.com/crossterm-rs/crossterm) — 终端 UI
- [reqwest](https://github.com/seanmonstar/reqwest) — HTTP 客户端（rustls）
- [tokio](https://tokio.rs) — 异步运行时
- [serde](https://serde.rs) / serde_json — 序列化

## License

MIT

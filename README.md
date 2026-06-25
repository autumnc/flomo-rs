# flomo-rs

flomo 笔记服务的终端客户端，支持 TUI 交互界面和非交互式 CLI 命令，使用 Rust 编写。

## 安装

```bash
# 从源码安装
cargo install --git https://github.com/autumnc/flomo-rs.git

# 或下载预编译二进制
# https://github.com/autumnc/flomo-rs/releases
```

## 使用

### TUI 交互模式

```bash
flomo-rs          # 默认 TokyoNight 主题
flomo-rs -hc      # Obsidian 高对比度主题
```

首次启动进入登录页。Token 保存在 `~/.flomo-cli/token.json`，后续自动登录并同步。

### CLI 非交互模式

支持脚本和管道使用，`--json` 输出结构化数据。管道时自动启用 JSON。

```bash
flomo-rs login --email E --password P
flomo-rs logout
flomo-rs status
flomo-rs list --limit 10 --json
flomo-rs get <slug>
flomo-rs new "内容"             # 命令行参数
flomo-rs new -f file.txt         # 从文件读取
echo "note" | flomo-rs new       # 从 stdin 管道
flomo-rs edit <slug> "新内容"
flomo-rs delete <slug> -y
flomo-rs search "关键词" --tag 标签
flomo-rs tags
flomo-rs review                  # 回顾往年今日
```

全局选项：

| 选项 | 说明 |
|------|------|
| `--token TOKEN` | 指定 token（优先于缓存和 `FLOMO_TOKEN` 环境变量） |
| `--json` | 全局 JSON 输出 |
| `--version` / `-V` | 显示版本 |
| `--help` / `-h` | 帮助 |

Token 优先级：`--token` > `FLOMO_TOKEN` 环境变量 > `~/.flomo-cli/token.json`

## TUI 快捷键

| 按键 | 功能 |
|------|------|
| `n` / `e` / `d` | 新建 / 编辑 / 删除笔记 |
| `j` `k` `↑` `↓` | 列表移动 |
| `/` | 搜索 |
| `t` / `D` | 标签筛选 / 日期日历 |
| `s` | 手动同步 |
| `T` | 切换主题 |
| `Esc` | 清除筛选 |
| `Ctrl+s` | 编辑模式保存 |
| `q` | 退出 |

## 数据存储

- `~/.flomo-cli/token.json` — 登录 token
- `~/.flomo-cli/memos.json` — 本地笔记缓存

离线时自动使用本地缓存，联网后支持手动同步。

## 技术栈

- [ratatui](https://ratatui.rs) + [crossterm](https://github.com/crossterm-rs/crossterm) — 终端 UI
- [reqwest](https://github.com/seanmonstar/reqwest) — HTTP 客户端（rustls）
- [tokio](https://tokio.rs) — 异步运行时
- [serde](https://serde.rs) / serde_json — 序列化

## License

MIT

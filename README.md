# flomo-rs

一款基于终端的 [flomo](https://flomoapp.com/) 笔记管理工具，使用 Rust 编写。

![Rust](https://img.shields.io/badge/Rust-1.75+-orange)
![License](https://img.shields.io/badge/License-MIT-blue)
![Platform](https://img.shields.io/badge/Platform-Linux-green)

## 功能特性

- **笔记管理** — 浏览、创建、编辑、删除 flomo 笔记
- **标签系统** — 按标签筛选笔记，查看所有标签及使用次数
- **日期筛选** — 通过日历弹窗按日期筛选笔记
- **全文搜索** — 实时搜索笔记内容和标签
- **图片预览** — 通过 ueberzugpp 在终端内直接显示笔记中的图片
- **自动同步** — 启动时自动从 flomo 服务器同步所有笔记
- **分页加载** — 支持大量笔记的分页加载（每页 200 条）
- **Vim 风格键位** — j/k 上下移动，g/G 跳转首尾，ESC 返回
- **CJK 支持** — 完美支持中文、日文、韩文等宽字符的显示和对齐
- **TokyoNight 主题** — 精心设计的暗色主题，长时间使用不疲劳
- **Token 持久化** — 登录凭据保存在 `~/.flomo-cli/token.json`，无需重复登录

## 依赖

### 必需

| 依赖 | 最低版本 | 说明 |
|------|---------|------|
| [Rust](https://www.rust-lang.org/tools/install) | 1.75+ | 编译工具链 |
| [OpenSSL](https://www.openssl.org/) | 1.1+ | HTTPS 通信（代码中已 vendored，通常无需额外安装） |

### 可选（图片预览）

| 依赖 | 说明 |
|------|------|
| [ueberzugpp](https://github.com/ueberzugpp/ueberzugpp) | 终端图片渲染工具，安装后自动启用图片预览 |

> 如果不安装 ueberzugpp，程序仍可正常使用，只是笔记中的图片不会显示。

## 安装

### 从源码编译

```bash
# 克隆或下载源码
cd flomo-rs

# 编译 release 版本
cargo build --release

# 二进制文件位于
# target/release/flomo-rs

# 可选：安装到系统路径
cp target/release/flomo-rs ~/.local/bin/
```

### 直接使用预编译二进制

```bash
# 解压
tar xzf flomo-rs-linux-x86_64.tar.gz

# 运行
./flomo-rs

# 可选：安装到 PATH
cp flomo-rs ~/.local/bin/
```

## 使用方法

### 启动

```bash
flomo-rs
```

首次运行会进入登录界面，输入 flomo 账号的邮箱和密码即可。

### 快捷键

#### 全局

| 按键 | 功能 |
|------|------|
| `q` | 退出程序 |
| `s` | 同步笔记 |
| `n` | 新建笔记 |
| `e` | 编辑当前笔记 |
| `d` | 删除当前笔记 |
| `/` | 进入搜索模式 |
| `t` | 打开标签筛选弹窗 |
| `D` | 打开日期筛选日历 |
| `Esc` | 清除筛选条件 / 返回上级模式 |

#### 侧边栏导航

| 按键 | 功能 |
|------|------|
| `j` / `↓` | 下一项 |
| `k` / `↑` | 上一项 |
| `g` | 跳到第一项 |
| `G` | 跳到最后一项 |
| `h` / `←` | 聚焦侧边栏 |
| `l` / `→` / `Enter` | 聚焦详情面板 |

#### 详情面板滚动

| 按键 | 功能 |
|------|------|
| `J` | 向下滚动一行 |
| `K` | 向上滚动一行 |
| `PageDown` | 向下翻页 |
| `PageUp` | 向上翻页 |

#### 编辑模式

| 按键 | 功能 |
|------|------|
| 方向键 | 移动光标 |
| `Home` / `End` | 行首 / 行尾 |
| `Backspace` / `Delete` | 删除字符 |
| `Enter` | 换行 |
| `Ctrl+S` | 保存 |
| `Esc` | 取消编辑 |

#### 搜索模式

| 按键 | 功能 |
|------|------|
| 输入字符 | 搜索关键词（支持中文） |
| `Enter` | 确认搜索 |
| `Esc` | 取消搜索 |

#### 日历弹窗

| 按键 | 功能 |
|------|------|
| `h` `j` `k` `l` / 方向键 | 移动日期光标 |
| `Ctrl+←/→` | 切换月份 |
| `Ctrl+↑/↓` | 切换年份 |
| `Enter` | 按选中日期筛选 |
| `Esc` | 关闭日历 |

## 项目结构

```
flomo-rs/
├── Cargo.toml          # 项目配置与依赖声明
├── Cargo.lock          # 依赖版本锁定
├── README.md           # 本文件
└── src/
    ├── main.rs         # 程序入口、终端初始化、主事件循环
    ├── app.rs          # 应用状态管理、键盘事件处理
    ├── ui.rs           # TUI 界面渲染（TokyoNight 主题）
    ├── api.rs          # flomo API 通信、数据模型、HTML 处理
    └── image.rs        # ueberzugpp 图片预览（守护进程 + stdin 管道）
```

### 模块说明

#### `main.rs` — 程序入口

- 终端原始模式（raw mode）和备用屏幕（alternate screen）管理
- 创建 API 通信的后台线程（使用 tokio 运行时）
- 主事件循环：绘制 UI → 处理图片显示 → 轮询键盘事件 → 处理 API 响应
- 程序退出时清理图片覆盖层并恢复终端状态

#### `app.rs` — 应用状态

- 管理所有运行时状态：笔记列表、侧边栏索引、滚动位置、编辑器状态等
- 定义 6 种交互模式：Normal、Search、Edit、Calendar、Tags、Login
- 处理所有键盘输入事件，分发到对应模式的处理函数
- 笔记筛选逻辑：标签筛选、日期筛选、全文搜索

#### `ui.rs` — 界面渲染

- 使用 [ratatui](https://github.com/ratatui/ratatui) 框架渲染 TUI
- TokyoNight 配色方案（暗色主题）
- 布局：左侧边栏（40%）+ 右侧详情面板
- CJK 字符宽度正确处理，文本自动换行
- 图片占位区域为纯空白行，由 ueberzugpp 覆盖显示

#### `api.rs` — API 通信

- 封装 flomo API v1 的所有接口
- 自动签名算法（MD5 签名，参数排序拼接）
- 支持分页加载全部笔记（自动翻页）
- HTML 内容与纯文本互转
- 从笔记内容和附件中提取图片 URL
- Token 持久化存储

#### `image.rs` — 图片预览

- 检测 ueberzugpp 是否可用
- 启动 `ueberzugpp layer` 守护进程，通过 stdin 管道发送 JSON 命令
- 异步图片下载（独立线程，不阻塞 UI）
- 图片缓存到 `~/.flomo-cli/cache/images/`（MD5 哈希命名）
- 智能覆盖管理：只在位置变化时更新，切换笔记时自动清理
- 守护进程崩溃自动重启

## 数据存储

flomo-rs 在本地存储以下数据：

```
~/.flomo-cli/
├── token.json                    # 登录 Token
├── image-debug.log               # 图片预览调试日志
└── cache/
    └── images/
        ├── 32a01abae8c1133c.jpg  # 缓存的图片文件
        └── ...
```

## 技术栈

| 库 | 用途 |
|----|------|
| [ratatui](https://crates.io/crates/ratatui) 0.29 | 终端 UI 框架 |
| [crossterm](https://crates.io/crates/crossterm) 0.28 | 终端控制（原始模式、光标、事件） |
| [reqwest](https://crates.io/crates/reqwest) 0.12 | HTTP 客户端（API 通信、图片下载） |
| [tokio](https://crates.io/crates/tokio) 1.x | 异步运行时（API 线程） |
| [serde](https://crates.io/crates/serde) 1.x | JSON 序列化/反序列化 |
| [regex](https://crates.io/crates/regex) 1.x | HTML 标签处理 |
| [chrono](https://crates.io/crates/chrono) 0.4 | 日期时间处理 |
| [unicode-width](https://crates.io/crates/unicode-width) 0.2 | CJK 字符宽度计算 |
| [md-5](https://crates.io/crates/md-5) 0.10 | API 签名、图片缓存命名 |
| [dirs](https://crates.io/crates/dirs) 6.x | 跨平台目录路径 |

## 图片预览工作原理

flomo-rs 使用 [ueberzugpp](https://github.com/ueberzugpp/ueberzugpp) 在终端中显示图片：

1. 程序启动时检测 `ueberzugpp` 是否可用
2. 首次需要显示图片时，启动一个 `ueberzugpp layer` 长驻守护进程
3. 通过守护进程的 stdin 管道发送 JSON 命令：
   - `{"action":"add", "identifier":"...", "x":..., "y":..., "width":..., "height":..., "path":"..."}` — 显示图片
   - `{"action":"remove", "identifier":"..."}` — 移除图片
4. 图片以守护进程方式持久运行，切换笔记时自动清理旧图片、加载新图片
5. 程序退出时自动清理所有图片覆盖层

> **注意**：需要使用支持图形协议的终端模拟器（如 kitty、alacritty、wezterm 等），ueberzugpp 才能正确显示图片。

## 调试

如遇图片显示问题，可查看调试日志：

```bash
cat ~/.flomo-cli/image-debug.log
```

日志包含：
- ueberzugpp 检测状态
- 守护进程启动信息
- 图片下载进度
- 覆盖层添加/移除记录
- 错误信息

## 许可证

MIT License

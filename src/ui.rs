use chrono::{Datelike, NaiveDate};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Mode, StatusKind};
use crate::image::DEFAULT_IMAGE_HEIGHT_CHARS;

// ─── TokyoNight Theme ─────────────────────────────────────────────────────

struct Theme;
impl Theme {
    const BG: Color = Color::Rgb(26, 27, 38);       // #1a1b26
    const SURFACE: Color = Color::Rgb(22, 22, 30);   // #16161e
    const OVERLAY: Color = Color::Rgb(30, 30, 46);   // #1e1e2e
    const TEXT: Color = Color::Rgb(192, 202, 245);    // #c0caf5
    const SUBTEXT: Color = Color::Rgb(169, 177, 214); // #a9b1d6
    const BLUE: Color = Color::Rgb(122, 162, 247);    // #7aa2f7
    const CYAN: Color = Color::Rgb(125, 207, 255);    // #7dcfff
    const GREEN: Color = Color::Rgb(158, 206, 106);   // #9ece6a
    const YELLOW: Color = Color::Rgb(224, 175, 104);  // #e0af68
    const RED: Color = Color::Rgb(247, 118, 142);     // #f7768e
    const MAGENTA: Color = Color::Rgb(187, 154, 247); // #bb9af7
    const ORANGE: Color = Color::Rgb(255, 158, 100);  // #ff9e64
    const DIM: Color = Color::Rgb(86, 95, 137);       // #565f89
    const BORDER: Color = Color::Rgb(59, 66, 97);     // #3b4261
    const DARK_BLUE: Color = Color::Rgb(35, 37, 56);  // #232538
}

fn base_style() -> Style {
    Style::default().bg(Theme::BG).fg(Theme::TEXT)
}

/// Explicitly re-draw the 4 borders of a popup area via the raw buffer.
/// This prevents CJK characters from outside the area from bleeding into
/// border cells (the terminal may render a wide char's 2nd column over the border).
fn redraw_popup_borders(f: &mut Frame, area: Rect, border_color: Color) {
    let style = Style::default().fg(border_color);
    let buf = f.buffer_mut();
    let right = area.right() - 1;
    let bottom = area.bottom() - 1;
    // Horizontal top + bottom
    let h_len = (area.width.saturating_sub(2)) as usize;
    let h_line: String = "─".repeat(h_len);
    buf.set_string(area.x + 1, area.y, &h_line, style);
    buf.set_string(area.x + 1, bottom, &h_line, style);
    // Corners
    buf.set_string(area.x, area.y, "┌", style);
    buf.set_string(right, area.y, "┐", style);
    buf.set_string(area.x, bottom, "└", style);
    buf.set_string(right, bottom, "┘", style);
    // Vertical left + right (skip corner rows)
    for y in (area.y + 1)..bottom {
        buf.set_string(area.x, y, "│", style);
        buf.set_string(right, y, "│", style);
    }
}

// ─── Main Draw ────────────────────────────────────────────────────────────

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();
    f.render_widget(Clear, size);

    match app.mode {
        Mode::Login => draw_login(f, app, size),
        _ => draw_main(f, app, size),
    }
}

fn draw_main(f: &mut Frame, app: &mut App, size: Rect) {
    // Search bar at top
    let search_h = 1u16;
    let footer_h = 1u16;
    let main_h = size.height.saturating_sub(search_h + footer_h);

    let chunks = Layout::vertical([
        Constraint::Length(search_h),
        Constraint::Max(main_h),
        Constraint::Length(footer_h),
    ])
    .split(size);

    draw_search_bar(f, app, chunks[0]);
    draw_body(f, app, chunks[1]);
    draw_footer(f, app, chunks[2]);

    // Popups
    match app.mode {
        Mode::Calendar => draw_calendar(f, app, size),
        Mode::Tags => draw_tags_popup(f, app, size),
        _ => {}
    }
}

// ─── Search Bar ───────────────────────────────────────────────────────────

fn draw_search_bar(f: &mut Frame, app: &App, area: Rect) {
    let (label, content, cursor_style) = match app.mode {
        Mode::Search => {
            let query = format!("/{}", app.search_query);
            (
                Span::styled("/", Style::default().fg(Theme::YELLOW).add_modifier(Modifier::BOLD)),
                query,
                Style::default().fg(Theme::TEXT).add_modifier(Modifier::UNDERLINED),
            )
        }
        _ => {
            let filter_parts: Vec<String> = vec![
                app.filter_tag.as_ref().map(|t| format!("#{}", t)),
                app.filter_date.as_ref().map(|d| d.format("%Y-%m-%d").to_string()),
                if !app.search_query.is_empty() {
                    Some(format!("/{}", app.search_query))
                } else {
                    None
                },
            ]
            .into_iter()
            .flatten()
            .collect();
            if filter_parts.is_empty() {
                (
                    Span::styled("  Flomo ", Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD)),
                    String::new(),
                    Style::default(),
                )
            } else {
                (
                    Span::styled("  筛选: ", Style::default().fg(Theme::DIM)),
                    filter_parts.join(" "),
                    Style::default().fg(Theme::YELLOW),
                )
            }
        }
    };

    let mut spans = vec![label];
    if !content.is_empty() {
        spans.push(Span::styled(content, cursor_style));
    }
    if app.mode == Mode::Search {
        spans.push(Span::styled("▏", Style::default().fg(Theme::YELLOW)));
    }

    let line = Line::from(spans);
    let bg = if app.mode == Mode::Search {
        Theme::DARK_BLUE
    } else {
        Theme::BG
    };
    let paragraph = Paragraph::new(line).style(Style::default().bg(bg));
    f.render_widget(paragraph, area);
}

// ─── Body (Sidebar + Main) ────────────────────────────────────────────────

fn draw_body(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::horizontal([Constraint::Percentage(40), Constraint::Min(0)])
        .split(area);

    draw_sidebar(f, app, chunks[0]);

    // Offset detail panel 1 column right so separator doesn't overlap its content
    let detail_area = Rect::new(
        chunks[0].right() + 1,
        area.y,
        area.right().saturating_sub(chunks[0].right() + 1),
        area.height,
    );
    draw_main_panel(f, app, detail_area);

    // Draw vertical separator line between sidebar and detail
    let sep_x = chunks[0].right();
    for y in area.top()..area.bottom() {
        f.buffer_mut().set_string(
            sep_x,
            y,
            "│",
            Style::default().fg(Theme::BORDER),
        );
    }
}

// ─── Sidebar ──────────────────────────────────────────────────────────────

fn draw_sidebar(f: &mut Frame, app: &App, area: Rect) {
    let title = format!(" 笔记 ({}) ", app.filtered_indices.len());
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(Theme::BLUE).add_modifier(Modifier::BOLD))
        .style(base_style())
        .border_style(Style::default().fg(Theme::BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let item_h = 2u16;
    let visible = (inner.height as usize).div_ceil(item_h as usize);

    let scroll = app.sidebar_scroll as usize;
    let start = app.sidebar_index.saturating_sub(scroll);
    let end = (start + visible).min(app.filtered_indices.len());

    let mut lines: Vec<Line> = Vec::new();
    for idx in start..end {
        if let Some(&memo_idx) = app.filtered_indices.get(idx) {
            if let Some(memo) = app.memos.get(memo_idx) {
                let is_selected = idx == app.sidebar_index;
                let preview_w = inner.width.saturating_sub(2) as usize;
                let preview = memo.preview(preview_w);
                let preview_text_w = UnicodeWidthStr::width(preview.as_str());
                let date_display = if memo.created_at.len() >= 10 {
                    memo.created_at[5..10].to_string()
                } else {
                    memo.created_at.clone()
                };

                let mut preview_spans = vec![Span::styled(
                    "● ",
                    Style::default().fg(if is_selected { Theme::BLUE } else { Theme::DIM }),
                )];
                preview_spans.push(Span::styled(
                    preview,
                    if is_selected {
                        Style::default().fg(Theme::TEXT).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Theme::SUBTEXT)
                    },
                ));
                let date_pad = preview_w.saturating_sub(preview_text_w + 2);
                if date_pad > 0 {
                    preview_spans.push(Span::styled(
                        format!("{:width$}", "", width = date_pad),
                        Style::default(),
                    ));
                }
                preview_spans.push(Span::styled(
                    date_display,
                    Style::default().fg(Theme::DIM),
                ));

                lines.push(Line::from(preview_spans));

                if !memo.tags.is_empty() {
                    let tags_str = memo
                        .tags
                        .iter()
                        .take(3)
                        .map(|t| format!("#{}", t))
                        .collect::<Vec<_>>()
                        .join(" ");
                    lines.push(Line::from(Span::styled(
                        format!("  {}", tags_str),
                        Style::default().fg(if is_selected { Theme::CYAN } else { Theme::DIM }),
                    )));
                } else {
                    lines.push(Line::from(""));
                }


            }
        }
    }

    if app.filtered_indices.is_empty() && !app.is_loading {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  暂无笔记",
            Style::default().fg(Theme::DIM),
        )));
        lines.push(Line::from(Span::styled(
            "  按 n 新建 / s 同步",
            Style::default().fg(Theme::DIM),
        )));
    }

    if app.is_loading {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  同步中...",
            Style::default().fg(Theme::YELLOW),
        )));
    }

    let paragraph = Paragraph::new(lines).scroll((app.sidebar_scroll * item_h, 0));
    f.render_widget(paragraph, inner);

    // Minimal sidebar scrollbar (no arrows, no track)
    let total_items = app.filtered_indices.len();
    if total_items > visible {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("┃")
            .track_symbol(None)
            .begin_symbol(None)
            .end_symbol(None)
            .style(Style::default().fg(Theme::DIM));
        let mut sb_state = ScrollbarState::new(total_items).position(app.sidebar_index);
        f.render_stateful_widget(scrollbar, inner, &mut sb_state);
    }
}

// ─── Main Content Panel ───────────────────────────────────────────────────

fn draw_main_panel(f: &mut Frame, app: &mut App, area: Rect) {
    match app.mode {
        Mode::Edit => draw_edit_panel(f, app, area),
        _ => draw_detail_panel(f, app, area),
    }
}

fn draw_detail_panel(f: &mut Frame, app: &mut App, area: Rect) {
    // Clear previous image render positions (recalculated every frame)
    app.image_render_positions.clear();

    let title = " 笔记详情 ";
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(Theme::GREEN).add_modifier(Modifier::BOLD))
        .style(base_style())
        .border_style(Style::default().fg(Theme::BORDER));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Extract memo data to avoid borrow conflict with app.main_scroll
    let memo_data = app.current_memo().map(|m| {
        (
            m.slug.clone(),
            m.created_at.clone(),
            m.updated_at.clone(),
            m.tags.clone(),
            m.content_text(),
            m.content_text_with_images(),
        )
    });

    let mut lines: Vec<Line> = Vec::new();
    // Track the raw `lines` index where each image placeholder's top border is pushed.
    // Image placeholder lines don't wrap (they're sized to inner.width-2),
    // but lines before them might wrap, so we'll resolve to wrapped indices later.
    let mut image_raw_indices: Vec<usize> = Vec::new();

    if let Some((slug, created_at, updated_at, tags, _content_text, (text_with_images, images))) = memo_data {
        lines.push(Line::from(vec![
            Span::styled("ID: ", Style::default().fg(Theme::DIM)),
            Span::styled(slug, Style::default().fg(Theme::ORANGE)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("创建: ", Style::default().fg(Theme::DIM)),
            Span::styled(created_at, Style::default().fg(Theme::SUBTEXT)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("修改: ", Style::default().fg(Theme::DIM)),
            Span::styled(updated_at, Style::default().fg(Theme::SUBTEXT)),
        ]));
        lines.push(Line::from(""));

        if !tags.is_empty() {
            let tag_spans: Vec<Span> = std::iter::once(Span::styled("标签: ", Style::default().fg(Theme::DIM)))
                .chain(tags.iter().flat_map(|t| {
                    vec![
                        Span::styled(format!("#{}", t), Style::default().fg(Theme::CYAN)),
                        Span::styled(" ", Style::default()),
                    ]
                }))
                .collect();
            lines.push(Line::from(tag_spans));
            lines.push(Line::from(""));
        }

        lines.push(Line::from(Span::styled(
            format!("{:─<width$}", "", width = inner.width as usize),
            Style::default().fg(Theme::BORDER),
        )));
        lines.push(Line::from(""));

        // Render text content (which now includes [🖼 图片 N] placeholders)
        let img_count = images.len();
        for line in text_with_images.lines() {
            if line.contains("[🖼 图片") {
                // Record where this image placeholder starts
                image_raw_indices.push(lines.len());

                // Image area — pure blank space for ueberzugpp to overlay.
                // No borders, no text — let the image be the only thing visible.
                let img_h = DEFAULT_IMAGE_HEIGHT_CHARS as usize;

                for _ in 0..img_h {
                    lines.push(Line::from(""));
                }

                lines.push(Line::from("")); // gap after image
            } else {
                // Regular text line
                lines.push(Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(Theme::TEXT),
                )));
            }
        }

        // If there are images in the files array but not in content, show them at the bottom
        let content_img_count = text_with_images.matches("[🖼 图片").count();
        if img_count > content_img_count {
            lines.push(Line::from(""));
            for _i in (content_img_count + 1)..=img_count {
                image_raw_indices.push(lines.len());

                let img_h = DEFAULT_IMAGE_HEIGHT_CHARS as usize;

                for _ in 0..img_h {
                    lines.push(Line::from(""));
                }

                lines.push(Line::from(""));
            }
        }
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  选择左侧笔记查看详情",
            Style::default().fg(Theme::DIM),
        )));
    }

    // Pre-wrap content lines to match actual display height.
    // Also build a mapping: raw_lines_index -> first wrapped_line_index
    // so we can resolve image placeholder positions accurately.
    let max_w = inner.width as usize;
    let mut wrapped_lines: Vec<Line> = Vec::new();
    let mut raw_to_wrapped: Vec<usize> = Vec::new();
    let mut wrapped_idx = 0usize;

    for line in &lines {
        raw_to_wrapped.push(wrapped_idx);

        if line.spans.is_empty() {
            wrapped_lines.push(line.clone());
            wrapped_idx += 1;
            continue;
        }
        let line_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        let line_w = UnicodeWidthStr::width(line_text.as_str());
        if line_w <= max_w {
            wrapped_lines.push(line.clone());
            wrapped_idx += 1;
        } else {
            // Split into chunks that fit
            let mut chunks = wrap_line_to_spans(&line.spans, max_w);
            wrapped_idx += chunks.len();
            wrapped_lines.append(&mut chunks);
        }
    }

    // Convert image raw indices to terminal coordinates.
    // Each image placeholder is now pure blank lines — no borders.
    // The position maps directly to the first blank line of the image area.
    let scroll = app.main_scroll as usize;

    for &raw_idx in &image_raw_indices {
        // wrapped_idx of the first blank line of the image area
        let img_start_wrapped = raw_to_wrapped.get(raw_idx).copied().unwrap_or(0);

        // Terminal y = inner.y + wrapped_index - scroll
        let term_y = inner.y as i32 + img_start_wrapped as i32 - scroll as i32;
        // Terminal x = inner.x (no border to skip)
        let term_x = inner.x;
        // Width = inner.width (full width, no borders)
        let term_w = inner.width;
        // Height = DEFAULT_IMAGE_HEIGHT_CHARS (the image area)
        let term_h = DEFAULT_IMAGE_HEIGHT_CHARS;

        // Only include if at least partially visible
        if term_y + term_h as i32 > inner.y as i32 && term_y < (inner.y + inner.height) as i32 {
            app.image_render_positions.push((
                term_x,
                term_y.max(inner.y as i32) as u16,
                term_w,
                term_h,
            ));
        }
    }

    let text = Text::from(wrapped_lines);

    app.detail_visible_height = inner.height;

    let total_lines = text.height() as u16;
    let max_scroll = total_lines.saturating_sub(inner.height);
    if app.main_scroll > max_scroll {
        app.main_scroll = max_scroll;
    }

    let paragraph = Paragraph::new(text)
        .scroll((app.main_scroll, 0));

    f.render_widget(paragraph, inner);

    // Detail scrollbar (minimal, no arrows)
    if total_lines > inner.height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .thumb_symbol("┃")
            .track_symbol(None)
            .begin_symbol(None)
            .end_symbol(None)
            .style(Style::default().fg(Theme::DIM));
        f.render_stateful_widget(
            scrollbar,
            inner,
            &mut ScrollbarState::new(total_lines as usize)
                .position(app.main_scroll as usize),
        );
    }
}

/// Wrap a line (list of spans) into multiple lines that fit within `max_width`.
/// Handles CJK characters correctly via unicode_width.
fn wrap_line_to_spans<'a>(spans: &'a [Span<'a>], max_width: usize) -> Vec<Line<'a>> {
    if max_width == 0 {
        return vec![Line::from(spans.to_vec())];
    }
    let mut result: Vec<Line> = Vec::new();
    let mut cur_spans: Vec<Span> = Vec::new();
    let mut cur_buf = String::new();
    let mut cur_w: usize = 0;

    for span in spans {
        let content = span.content.as_ref();
        let style = span.style;
        let mut char_start = 0;

        for (i, ch) in content.char_indices() {
            let ch_w = UnicodeWidthChar::width(ch).unwrap_or(1);
            if cur_w + ch_w > max_width && !cur_buf.is_empty() {
                // Flush current chunk
                cur_spans.push(Span::styled(cur_buf.clone(), style));
                result.push(Line::from(std::mem::take(&mut cur_spans)));
                cur_buf.clear();
                cur_w = 0;
                char_start = i;
            }
            cur_buf.push(ch);
            cur_w += ch_w;
        }

        // Remaining chars in this span go into the next chunk
        let remaining = &content[char_start..];
        if !remaining.is_empty() {
            cur_buf = remaining.to_string();
            cur_w = UnicodeWidthStr::width(cur_buf.as_str());
            cur_spans.push(Span::styled(cur_buf.clone(), style));
        } else if cur_buf.is_empty() {
            cur_spans.push(Span::styled(String::new(), style));
        }
    }

    if !cur_spans.is_empty() {
        result.push(Line::from(cur_spans));
    }

    if result.is_empty() {
        result.push(Line::from(""));
    }
    result
}

/// Wrap a plain text string into multiple Lines that fit within `max_width`.
/// Handles CJK characters correctly via unicode_width. Returns owned Lines.
fn wrap_plain_text(text: &str, max_width: usize) -> Vec<Line<'static>> {
    if max_width == 0 || text.is_empty() {
        return vec![Line::from(text.to_string())];
    }
    let mut result: Vec<Line<'static>> = Vec::new();
    let mut chunk = String::new();
    let mut chunk_w = 0usize;

    for ch in text.chars() {
        let ch_w = UnicodeWidthChar::width(ch).unwrap_or(1);
        if chunk_w + ch_w > max_width {
            result.push(Line::from(std::mem::take(&mut chunk)));
            chunk_w = 0;
        }
        chunk.push(ch);
        chunk_w += ch_w;
    }

    if !chunk.is_empty() {
        result.push(Line::from(chunk));
    }
    if result.is_empty() {
        result.push(Line::from(String::new()));
    }
    result
}

fn draw_edit_panel(f: &mut Frame, app: &App, area: Rect) {
    let title = if app.edit_is_new {
        " 新建笔记 (Ctrl+s 保存, Esc 取消) "
    } else {
        " 编辑笔记 (Ctrl+s 保存, Esc 取消) "
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(Theme::YELLOW)
                .add_modifier(Modifier::BOLD),
        )
        .style(base_style())
        .border_style(Style::default().fg(Theme::YELLOW))
        .borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let max_w = inner.width as usize;

    // Pre-wrap each logical line for display, tracking visual row offset for cursor
    let mut display_lines: Vec<Line> = Vec::new();
    let mut visual_row_before_cursor: usize = 0;

    for (li, logical_line) in app.edit_lines.iter().enumerate() {
        let wrapped = wrap_plain_text(logical_line, max_w);

        if li < app.edit_cursor_row {
            visual_row_before_cursor += wrapped.len();
        }
        display_lines.extend(wrapped);
    }

    let text = Text::from(display_lines);

    // Compute visual cursor position by walking the cursor's logical line
    let (visual_cursor_row, visual_cursor_col) = {
        let cursor_line = app
            .edit_lines
            .get(app.edit_cursor_row)
            .map(|s| s.as_str())
            .unwrap_or("");
        let mut vrow = 0usize;
        let mut vcol = 0usize;
        let mut dcol = 0usize; // display columns consumed

        for ch in cursor_line.chars() {
            let ch_w = UnicodeWidthChar::width(ch).unwrap_or(1);
            if dcol == app.edit_cursor_col {
                break;
            }
            if dcol + ch_w > max_w && max_w > 0 {
                // character doesn't fit on current wrapped line
                vrow += 1;
                dcol = 0;
            }
            dcol += ch_w;
            vcol = dcol;
            if dcol == app.edit_cursor_col {
                break;
            }
        }

        // If cursor is at end of line, check if it overflows
        if dcol == app.edit_cursor_col && max_w > 0 && dcol > 0 && dcol == max_w {
            // cursor sits exactly at the right edge — next char would wrap
            // keep cursor at end of current wrapped line
        }

        (visual_row_before_cursor + vrow, vcol)
    };

    // Auto-scroll to keep cursor visible
    let mut edit_scroll = app.edit_scroll as usize;
    if visual_cursor_row >= edit_scroll + inner.height as usize {
        edit_scroll = visual_cursor_row.saturating_sub(inner.height as usize - 1);
    } else if visual_cursor_row < edit_scroll {
        edit_scroll = visual_cursor_row;
    }

    let paragraph = Paragraph::new(text).scroll((edit_scroll as u16, 0));
    f.render_widget(paragraph, inner);

    // Cursor
    let cursor_x = inner.x + visual_cursor_col as u16;
    let cursor_y = inner.y + visual_cursor_row as u16 - edit_scroll as u16;
    f.set_cursor_position(Position::new(cursor_x, cursor_y));
}

// ─── Footer ───────────────────────────────────────────────────────────────

fn draw_footer(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![Span::styled(" ", Style::default())];

    let shortcuts = [
        ("n", "新建"),
        ("d", "删除"),
        ("e", "编辑"),
        ("s", "同步"),
        ("/", "搜索"),
        ("q", "退出"),
        ("t", "标签"),
        ("D", "日期"),
    ];

    for (i, (key, desc)) in shortcuts.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("│", Style::default().fg(Theme::DARK_BLUE)));
        }
        spans.push(Span::styled(
            format!(" {} ", key),
            Style::default().fg(Theme::ORANGE).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!("{} ", desc),
            Style::default().fg(Theme::DIM),
        ));
    }

    // Status message
    if let Some((ref msg, ref kind)) = app.status_msg {
        spans.push(Span::styled("  ", Style::default()));
        spans.push(Span::styled(
            format!(" {}", msg),
            Style::default().fg(match kind {
                StatusKind::Success => Theme::GREEN,
                StatusKind::Error => Theme::RED,
                StatusKind::Info => Theme::CYAN,
            }),
        ));
    }

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line).style(Style::default().bg(Theme::SURFACE));
    f.render_widget(paragraph, area);
}

// ─── Calendar Popup ───────────────────────────────────────────────────────

fn draw_calendar(f: &mut Frame, app: &App, _area: Rect) {
    let cal_w = 28u16;
    let cal_h = 11u16;
    let x = _area.width.saturating_sub(cal_w) / 2;
    let y = _area.height.saturating_sub(cal_h) / 2;
    let area = Rect::new(x, y, cal_w, cal_h);

    f.render_widget(Clear, area);

    let block = Block::default()
        .title(format!(
            " 日期筛选 {:04}-{:02} ",
            app.cal_year, app.cal_month
        ))
        .title_style(
            Style::default()
                .fg(Theme::YELLOW)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Theme::OVERLAY).fg(Theme::TEXT))
        .border_style(Style::default().fg(Theme::YELLOW))
        .borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let weekdays = ["日", "一", "二", "三", "四", "五", "六"];
    let mut lines: Vec<Line> = Vec::new();

    // Weekday headers
    let header_spans: Vec<Span> = weekdays
        .iter()
        .map(|d| {
            Span::styled(
                format!(" {:^2}", d),
                Style::default().fg(Theme::CYAN).add_modifier(Modifier::BOLD),
            )
        })
        .collect();
    lines.push(Line::from(header_spans));
    lines.push(Line::from(""));

    // Calendar days
    if let Some(first_day) = NaiveDate::from_ymd_opt(app.cal_year, app.cal_month, 1) {
        let weekday = first_day.weekday().num_days_from_sunday() as u16;
        let days_in_month = days_in_month(app.cal_year, app.cal_month);
        let today = chrono::Local::now().date_naive();

        // Pad start
        let mut row = vec![Span::styled("   ", Style::default()); weekday as usize];

        for day in 1..=days_in_month {
            let date = first_day + chrono::Days::new((day - 1) as u64);
            let date_key = date.format("%Y-%m-%d").to_string();
            let is_cursor = date == app.cal_cursor;
            let is_today = date == today;
            let has_memo = app.cal_has_memos.contains(&date_key);

            let (fg, bg) = if is_cursor {
                (Theme::BG, Theme::YELLOW)
            } else if is_today {
                (Theme::BLUE, Theme::DARK_BLUE)
            } else if has_memo {
                (Theme::GREEN, Theme::OVERLAY)
            } else {
                (Theme::TEXT, Theme::OVERLAY)
            };

            let marker = if has_memo && !is_cursor {
                "*"
            } else {
                " "
            };
            row.push(Span::styled(
                format!("{}{:>2}", marker, day),
                Style::default().fg(fg).bg(bg),
            ));

            let col = (weekday + day as u16) % 7;
            if col == 6 {
                // Fill to 7 columns
                while row.len() < 7 {
                    row.push(Span::styled("   ", Style::default().fg(Theme::TEXT)));
                }
                lines.push(Line::from(row));
                row = Vec::new();
            }
        }

        // Pad end
        if !row.is_empty() {
            while row.len() < 7 {
                row.push(Span::styled("   ", Style::default()));
            }
            lines.push(Line::from(row));
        }
    }

    // Help line
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " ↑↓←→选择  Ctrl+←→月  ↑↓年  回车确认  Esc取消",
        Style::default().fg(Theme::DIM),
    )));

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);

    // Re-draw borders to prevent CJK characters from bleeding into them
    redraw_popup_borders(f, area, Theme::YELLOW);
}

fn days_in_month(year: i32, month: u32) -> u32 {
    NaiveDate::from_ymd_opt(year, month + 1, 1)
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap())
        .pred_opt()
        .map(|d| d.day() as u32)
        .unwrap_or(31)
}

// ─── Tags Popup ───────────────────────────────────────────────────────────

fn draw_tags_popup(f: &mut Frame, app: &App, _area: Rect) {
    let popup_w = 30u16.min(_area.width.saturating_sub(4));
    let popup_h = 18u16.min(_area.height.saturating_sub(4));
    let x = _area.width.saturating_sub(popup_w) / 2;
    let y = _area.height.saturating_sub(popup_h) / 2;
    let area = Rect::new(x, y, popup_w, popup_h);

    f.render_widget(Clear, area);

    let title = format!(" 标签筛选 ({}) ", app.all_tags.len());
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(Theme::MAGENTA)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Theme::OVERLAY).fg(Theme::TEXT))
        .border_style(Style::default().fg(Theme::MAGENTA))
        .borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let max_tag_w = (inner.width.saturating_sub(8)) as usize;

    let visible = inner.height as usize;
    let scroll = app.tag_scroll as usize;

    for (i, tag) in app
        .all_tags
        .iter()
        .skip(scroll)
        .take(visible)
        .enumerate()
    {
        let is_selected = (i + scroll) == app.tag_index;
        let name = if UnicodeWidthStr::width(tag.name.as_str()) > max_tag_w {
            let mut s = String::new();
            let mut w = 0usize;
            for c in tag.name.chars() {
                let cw = UnicodeWidthChar::width(c).unwrap_or(1);
                if w + cw > max_tag_w.saturating_sub(3) {
                    s.push_str("..");
                    break;
                }
                w += cw;
                s.push(c);
            }
            s
        } else {
            tag.name.clone()
        };

        let (fg, marker) = if is_selected {
            (Theme::OVERLAY, "▶ ")
        } else {
            (Theme::TEXT, "  ")
        };

        let name_w = UnicodeWidthStr::width(name.as_str());
        let _count_pad = max_tag_w.saturating_sub(name_w);

        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(if is_selected { Theme::MAGENTA } else { Theme::DIM })),
            Span::styled(
                format!("{:width$}", name, width = max_tag_w),
                if is_selected {
                    Style::default().fg(fg).bg(Theme::MAGENTA).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Theme::TEXT)
                },
            ),
            Span::styled(
                format!(" {:>3}", tag.count),
                Style::default().fg(if is_selected { Theme::OVERLAY } else { Theme::DIM }).bg(if is_selected { Theme::MAGENTA } else { Theme::BG }),
            ),
        ]));
    }

    if app.all_tags.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  暂无标签",
            Style::default().fg(Theme::DIM),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " ↑↓选择  回车筛选  Esc取消",
        Style::default().fg(Theme::DIM),
    )));

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);

    // Re-draw borders to prevent CJK characters from bleeding into them
    redraw_popup_borders(f, area, Theme::MAGENTA);
}

// ─── Login Dialog ─────────────────────────────────────────────────────────

fn draw_login(f: &mut Frame, app: &App, size: Rect) {
    // Background
    let bg = Block::default().style(Style::default().bg(Theme::BG));
    f.render_widget(bg, size);

    let w = 44u16.min(size.width.saturating_sub(4));
    let h = 16u16.min(size.height.saturating_sub(4));
    let x = size.width.saturating_sub(w) / 2;
    let y = size.height.saturating_sub(h) / 2;
    let area = Rect::new(x, y, w, h);

    let block = Block::default()
        .title(" Flomo 登录 ")
        .title_style(
            Style::default()
                .fg(Theme::BLUE)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(Theme::OVERLAY).fg(Theme::TEXT))
        .border_style(Style::default().fg(Theme::BLUE))
        .borders(Borders::ALL);

    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  请输入邮箱和密码登录 flomo",
        Style::default().fg(Theme::DIM),
    )));
    lines.push(Line::from(""));

    // Email field
    let email_label = if app.login_step == 0 {
        Style::default().fg(Theme::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Theme::DIM)
    };
    lines.push(Line::from(vec![
        Span::styled("  邮箱: ", email_label),
        Span::styled(
            if app.login_step == 0 {
                format!("{}▏", app.login_email)
            } else {
                app.login_email.clone()
            },
            Style::default().fg(Theme::TEXT),
        ),
    ]));
    lines.push(Line::from(""));

    // Password field
    let pwd_label = if app.login_step == 1 {
        Style::default().fg(Theme::YELLOW).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Theme::DIM)
    };
    let pwd_display = "•".repeat(app.login_password.len());
    lines.push(Line::from(vec![
        Span::styled("  密码: ", pwd_label),
        Span::styled(
            if app.login_step == 1 {
                format!("{}▏", pwd_display)
            } else {
                pwd_display
            },
            Style::default().fg(Theme::TEXT),
        ),
    ]));

    // Error
    if let Some(ref err) = app.login_error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("  ✗ {}", err),
            Style::default().fg(Theme::RED),
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Enter 下一步  Esc 退出",
        Style::default().fg(Theme::DIM),
    )));

    let paragraph = Paragraph::new(lines);
    f.render_widget(paragraph, inner);

    // Cursor position
    let (cursor_x, cursor_y) = if app.login_step == 0 {
        (
            inner.x + 8 + UnicodeWidthStr::width(app.login_email.as_str()) as u16,
            inner.y + 3,
        )
    } else {
        (
            inner.x + 8 + app.login_password.len() as u16,
            inner.y + 5,
        )
    };
    f.set_cursor_position(Position::new(cursor_x, cursor_y));
}

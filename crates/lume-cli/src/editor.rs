use crate::tui::{clear_kitty_images, query_cell_px, strip_ansi_for_ratatui};
use anyhow::Result;
use crossterm::cursor::MoveTo;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseButton,
    MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use lume_core::kitty::{encode_png_placed, TerminalCaps};
use lume_core::{
    parse_markdown, render_ansi, render_pixels, AnsiOptions, PixelOptions, RenderedDocument,
    ThemePreset,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Terminal;
use std::io::{self, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const EDIT_KITTY_IMG_ID: u32 = 4243;

#[derive(Clone)]
struct EditorSnapshot {
    lines: Vec<String>,
    row: usize,
    col: usize,
}

struct TextBuffer {
    lines: Vec<String>,
    row: usize,
    col: usize,
    scroll_row: usize,
    dirty: bool,
    undo: Vec<EditorSnapshot>,
    redo: Vec<EditorSnapshot>,
}

impl TextBuffer {
    fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        Self {
            lines,
            row: 0,
            col: 0,
            scroll_row: 0,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    fn text(&self) -> String {
        self.lines.join("\n")
    }

    fn snapshot(&mut self) {
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.undo.push(EditorSnapshot {
            lines: self.lines.clone(),
            row: self.row,
            col: self.col,
        });
        self.redo.clear();
    }

    fn undo(&mut self) -> bool {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(EditorSnapshot {
                lines: self.lines.clone(),
                row: self.row,
                col: self.col,
            });
            self.lines = prev.lines;
            self.row = prev.row;
            self.col = prev.col;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    fn redo(&mut self) -> bool {
        if let Some(next) = self.redo.pop() {
            self.undo.push(EditorSnapshot {
                lines: self.lines.clone(),
                row: self.row,
                col: self.col,
            });
            self.lines = next.lines;
            self.row = next.row;
            self.col = next.col;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    fn char_len(&self, row: usize) -> usize {
        self.lines
            .get(row)
            .map(|l| l.chars().count())
            .unwrap_or(0)
    }

    fn byte_idx(s: &str, char_col: usize) -> usize {
        s.char_indices()
            .nth(char_col)
            .map(|(i, _)| i)
            .unwrap_or(s.len())
    }

    fn insert_char(&mut self, ch: char) {
        self.snapshot();
        let line = &mut self.lines[self.row];
        let b = Self::byte_idx(line, self.col);
        line.insert(b, ch);
        self.col += 1;
        self.dirty = true;
    }

    fn insert_newline(&mut self) {
        self.snapshot();
        let b = Self::byte_idx(&self.lines[self.row], self.col);
        let tail = self.lines[self.row].split_off(b);
        self.row += 1;
        self.lines.insert(self.row, tail);
        self.col = 0;
        self.dirty = true;
    }

    fn backspace(&mut self) {
        if self.col > 0 {
            self.snapshot();
            let line = &mut self.lines[self.row];
            let b_end = Self::byte_idx(line, self.col);
            let b_start = Self::byte_idx(line, self.col - 1);
            line.replace_range(b_start..b_end, "");
            self.col -= 1;
            self.dirty = true;
        } else if self.row > 0 {
            self.snapshot();
            let cur = self.lines.remove(self.row);
            self.row -= 1;
            self.col = self.char_len(self.row);
            self.lines[self.row].push_str(&cur);
            self.dirty = true;
        }
    }

    fn delete_char(&mut self) {
        let len = self.char_len(self.row);
        if self.col < len {
            self.snapshot();
            let line = &mut self.lines[self.row];
            let b_start = Self::byte_idx(line, self.col);
            let b_end = Self::byte_idx(line, self.col + 1);
            line.replace_range(b_start..b_end, "");
            self.dirty = true;
        } else if self.row + 1 < self.lines.len() {
            self.snapshot();
            let next = self.lines.remove(self.row + 1);
            self.lines[self.row].push_str(&next);
            self.dirty = true;
        }
    }

    fn ensure_visible(&mut self, view_h: usize) {
        if view_h == 0 {
            return;
        }
        if self.row < self.scroll_row {
            self.scroll_row = self.row;
        } else if self.row >= self.scroll_row + view_h {
            self.scroll_row = self.row + 1 - view_h;
        }
    }
}

/// Run the two-pane split Markdown editor (left) + live scroll-synced preview (right).
pub fn run_editor(
    path: PathBuf,
    initial_theme: ThemePreset,
    initial_pixel: bool,
    initial_font_size: f32,
) -> Result<()> {
    let initial_text = std::fs::read_to_string(&path).unwrap_or_default();
    let base_dir = path.parent().map(|p| p.to_path_buf());
    let caps = TerminalCaps::detect();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut buf = TextBuffer::from_text(&initial_text);
    let mut theme_preset = initial_theme;
    let mut pixel_mode = initial_pixel;
    let font_size = initial_font_size;
    let mut sync_scroll = true;
    let mut preview_only = false;
    let mut preview_scroll_y: u32 = 0;
    let mut status_msg = String::from("Ctrl+S:save  Ctrl+Z/Y:undo/redo  F2:preview-only  Ctrl+L:sync  Ctrl+Q:quit");

    let mut rendered: Option<RenderedDocument> = None;
    let mut ansi_lines: Vec<String> = Vec::new();
    let mut last_edit_at = Instant::now() - Duration::from_secs(1);
    let mut needs_rerender = true;
    let mut dirty_blit = true;
    let mut last_preview_px: (u32, u32) = (0, 0);

    let run_res: Result<()> = loop {
        let size = terminal.size()?;
        let (cell_w, cell_h) = query_cell_px();

        let outer = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(4),
                Constraint::Length(1),
            ])
            .split(Rect::new(0, 0, size.width, size.height));

        let (edit_area, preview_area) = if preview_only {
            (Rect::default(), outer[1])
        } else {
            let panes = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(48), Constraint::Percentage(52)])
                .split(outer[1]);
            (panes[0], panes[1])
        };

        let preview_px_w = (preview_area.width as u32 * cell_w).clamp(280, 2200);
        let preview_px_h = (preview_area.height as u32 * cell_h).clamp(120, 2000);

        // Debounced live re-render (110ms after typing)
        if (needs_rerender && last_edit_at.elapsed() >= Duration::from_millis(110))
            || (pixel_mode && last_preview_px.0 != preview_px_w)
        {
            let text = buf.text();
            let doc = parse_markdown(&text);
            if pixel_mode {
                let mut opts = PixelOptions::new(preview_px_w, theme_preset.theme());
                opts.font_size = font_size;
                opts.base_dir = base_dir.clone();
                if let Ok(r) = render_pixels(&doc, &opts) {
                    if sync_scroll {
                        let target_y = r
                            .y_for_source_line(buf.row + 1)
                            .saturating_sub(preview_px_h / 4);
                        preview_scroll_y = target_y.min(r.height.saturating_sub(preview_px_h));
                    }
                    rendered = Some(r);
                }
                last_preview_px = (preview_px_w, preview_px_h);
                dirty_blit = true;
            } else {
                let ansi = render_ansi(
                    &doc,
                    &AnsiOptions::new(
                        preview_area.width.saturating_sub(2) as usize,
                        theme_preset.theme(),
                        caps,
                    ),
                );
                ansi_lines = ansi.lines().map(strip_ansi_for_ratatui).collect();
            }
            needs_rerender = false;
        }

        if !preview_only {
            buf.ensure_visible(edit_area.height.saturating_sub(2) as usize);
        }

        let fname = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        let dirty_mark = if buf.dirty { " *" } else { "" };

        terminal.draw(|f| {
            let t = theme_preset.theme();
            let hdr_bg = Color::Rgb(t.code_header_bg.0, t.code_header_bg.1, t.code_header_bg.2);
            let hdr_fg = Color::Rgb(t.text.0, t.text.1, t.text.2);
            let muted = Color::Rgb(t.text_muted.0, t.text_muted.1, t.text_muted.2);
            let acc = Color::Rgb(t.accent_note.0, t.accent_note.1, t.accent_note.2);
            let ok = Color::Rgb(t.accent_tip.0, t.accent_tip.1, t.accent_tip.2);

            let header = Line::from(vec![
                Span::styled(
                    " lume edit ",
                    Style::default()
                        .fg(Color::Rgb(t.bg.0, t.bg.1, t.bg.2))
                        .bg(ok)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" {fname}{dirty_mark} "),
                    Style::default().fg(hdr_fg).bg(hdr_bg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "Ln {}, Col {}  [{}] [SYNC:{}] [{}]",
                        buf.row + 1,
                        buf.col + 1,
                        if pixel_mode { "PIXEL" } else { "ANSI" },
                        if sync_scroll { "ON" } else { "OFF" },
                        theme_preset.label()
                    ),
                    Style::default().fg(muted).bg(hdr_bg),
                ),
            ]);
            f.render_widget(
                Paragraph::new(header).style(Style::default().bg(hdr_bg)),
                outer[0],
            );

            if !preview_only {
                let inner_h = edit_area.height.saturating_sub(2) as usize;
                let mut editor_lines = Vec::with_capacity(inner_h);
                for (i, line) in buf
                    .lines
                    .iter()
                    .enumerate()
                    .skip(buf.scroll_row)
                    .take(inner_h)
                {
                    let num = format!("{:>4} ", i + 1);
                    let line_style = if line.starts_with('#') {
                        Style::default().fg(acc).add_modifier(Modifier::BOLD)
                    } else if line.trim_start().starts_with("```") {
                        Style::default().fg(ok)
                    } else if line.trim_start().starts_with('>') {
                        Style::default().fg(muted).add_modifier(Modifier::ITALIC)
                    } else {
                        Style::default().fg(hdr_fg)
                    };
                    editor_lines.push(Line::from(vec![
                        Span::styled(num, Style::default().fg(muted)),
                        Span::styled(line.as_str(), line_style),
                    ]));
                }
                let editor_widget = Paragraph::new(editor_lines).block(
                    Block::default()
                        .title(" Markdown ")
                        .borders(Borders::RIGHT)
                        .border_style(Style::default().fg(Color::Rgb(t.border.0, t.border.1, t.border.2))),
                );
                f.render_widget(editor_widget, edit_area);

                let cur_x = edit_area.x + 5 + buf.col as u16;
                let cur_y = edit_area.y + (buf.row.saturating_sub(buf.scroll_row)) as u16;
                if cur_x < edit_area.x + edit_area.width.saturating_sub(1)
                    && cur_y < edit_area.y + edit_area.height
                {
                    f.set_cursor_position((cur_x, cur_y));
                }
            }

            if !pixel_mode {
                let visible: Vec<Line> = ansi_lines
                    .iter()
                    .skip(preview_scroll_y as usize)
                    .take(preview_area.height as usize)
                    .map(|l| Line::from(l.as_str()))
                    .collect();
                f.render_widget(
                    Paragraph::new(visible).style(Style::default().fg(hdr_fg)),
                    preview_area,
                );
            } else {
                let bg = Color::Rgb(t.bg.0, t.bg.1, t.bg.2);
                f.render_widget(Block::default().style(Style::default().bg(bg)), preview_area);
            }

            f.render_widget(
                Paragraph::new(format!(" {status_msg}"))
                    .style(Style::default().fg(hdr_fg).bg(hdr_bg)),
                outer[2],
            );
        })?;

        if pixel_mode && dirty_blit {
            if let Some(ref r) = rendered {
                if let Ok(png) =
                    r.crop_png(preview_scroll_y, preview_px_h, theme_preset.theme().bg)
                {
                    let seq = encode_png_placed(
                        &png,
                        EDIT_KITTY_IMG_ID,
                        preview_area.width,
                        preview_area.height,
                    );
                    let mut out = io::stdout().lock();
                    let _ = execute!(out, MoveTo(preview_area.x, preview_area.y));
                    let _ = out.write_all(seq.as_bytes());
                    let _ = out.flush();
                }
            }
            dirty_blit = false;
        }

        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
                    if ctrl {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Char('c') => break Ok(()),
                            KeyCode::Char('s') => {
                                match std::fs::write(&path, buf.text()) {
                                    Ok(_) => {
                                        buf.dirty = false;
                                        status_msg = format!("Saved {}", path.display());
                                    }
                                    Err(e) => status_msg = format!("Save failed: {e}"),
                                }
                                continue;
                            }
                            KeyCode::Char('z') => {
                                if buf.undo() {
                                    needs_rerender = true;
                                    last_edit_at = Instant::now();
                                    status_msg = "Undo".to_string();
                                }
                                continue;
                            }
                            KeyCode::Char('y') => {
                                if buf.redo() {
                                    needs_rerender = true;
                                    last_edit_at = Instant::now();
                                    status_msg = "Redo".to_string();
                                }
                                continue;
                            }
                            KeyCode::Char('l') => {
                                sync_scroll = !sync_scroll;
                                status_msg = format!(
                                    "Scroll-sync {}",
                                    if sync_scroll { "ON" } else { "OFF" }
                                );
                                continue;
                            }
                            KeyCode::Char('g') => {
                                pixel_mode = !pixel_mode;
                                clear_kitty_images();
                                needs_rerender = true;
                                dirty_blit = true;
                                last_edit_at = Instant::now() - Duration::from_secs(1);
                                continue;
                            }
                            KeyCode::Char('t') => {
                                theme_preset = theme_preset.next();
                                needs_rerender = true;
                                dirty_blit = true;
                                last_edit_at = Instant::now() - Duration::from_secs(1);
                                status_msg = format!("Theme: {}", theme_preset.label());
                                continue;
                            }
                            _ => {}
                        }
                    }

                    if key.code == KeyCode::F(2) {
                        preview_only = !preview_only;
                        clear_kitty_images();
                        needs_rerender = true;
                        dirty_blit = true;
                        last_edit_at = Instant::now() - Duration::from_secs(1);
                        continue;
                    }

                    if preview_only {
                        let max_s = rendered
                            .as_ref()
                            .map(|r| r.height.saturating_sub(preview_px_h))
                            .unwrap_or(0);
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                            KeyCode::Down | KeyCode::Char('j') => {
                                preview_scroll_y = (preview_scroll_y + cell_h * 3).min(max_s);
                                dirty_blit = true;
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                preview_scroll_y = preview_scroll_y.saturating_sub(cell_h * 3);
                                dirty_blit = true;
                            }
                            _ => {}
                        }
                        continue;
                    }

                    let prev_row = buf.row;
                    match key.code {
                        KeyCode::Enter => {
                            buf.insert_newline();
                            needs_rerender = true;
                            last_edit_at = Instant::now();
                        }
                        KeyCode::Backspace => {
                            buf.backspace();
                            needs_rerender = true;
                            last_edit_at = Instant::now();
                        }
                        KeyCode::Delete => {
                            buf.delete_char();
                            needs_rerender = true;
                            last_edit_at = Instant::now();
                        }
                        KeyCode::Tab => {
                            buf.insert_char(' ');
                            buf.insert_char(' ');
                            needs_rerender = true;
                            last_edit_at = Instant::now();
                        }
                        KeyCode::Left => {
                            if buf.col > 0 {
                                buf.col -= 1;
                            } else if buf.row > 0 {
                                buf.row -= 1;
                                buf.col = buf.char_len(buf.row);
                            }
                        }
                        KeyCode::Right => {
                            if buf.col < buf.char_len(buf.row) {
                                buf.col += 1;
                            } else if buf.row + 1 < buf.lines.len() {
                                buf.row += 1;
                                buf.col = 0;
                            }
                        }
                        KeyCode::Up => {
                            if buf.row > 0 {
                                buf.row -= 1;
                                buf.col = buf.col.min(buf.char_len(buf.row));
                            }
                        }
                        KeyCode::Down => {
                            if buf.row + 1 < buf.lines.len() {
                                buf.row += 1;
                                buf.col = buf.col.min(buf.char_len(buf.row));
                            }
                        }
                        KeyCode::Home => buf.col = 0,
                        KeyCode::End => buf.col = buf.char_len(buf.row),
                        KeyCode::PageUp => {
                            buf.row = buf.row.saturating_sub(15);
                            buf.col = buf.col.min(buf.char_len(buf.row));
                        }
                        KeyCode::PageDown => {
                            buf.row = (buf.row + 15).min(buf.lines.len().saturating_sub(1));
                            buf.col = buf.col.min(buf.char_len(buf.row));
                        }
                        KeyCode::Char(c) => {
                            buf.insert_char(c);
                            needs_rerender = true;
                            last_edit_at = Instant::now();
                        }
                        _ => {}
                    }

                    // Live scroll-sync when cursor moves to a new line
                    if sync_scroll && buf.row != prev_row && pixel_mode {
                        if let Some(ref r) = rendered {
                            let target_y = r
                                .y_for_source_line(buf.row + 1)
                                .saturating_sub(preview_px_h / 4);
                            let clamped = target_y.min(r.height.saturating_sub(preview_px_h));
                            if clamped != preview_scroll_y {
                                preview_scroll_y = clamped;
                                dirty_blit = true;
                            }
                        }
                    }
                }
                Event::Mouse(m) => match m.kind {
                    MouseEventKind::Down(MouseButton::Left) if !preview_only => {
                        if m.column >= edit_area.x
                            && m.column < edit_area.x + edit_area.width
                            && m.row >= edit_area.y
                            && m.row < edit_area.y + edit_area.height
                        {
                            let target_row = buf.scroll_row + (m.row - edit_area.y) as usize;
                            buf.row = target_row.min(buf.lines.len().saturating_sub(1));
                            let target_col = m.column.saturating_sub(edit_area.x + 5) as usize;
                            buf.col = target_col.min(buf.char_len(buf.row));
                        }
                    }
                    MouseEventKind::ScrollDown => {
                        if m.column >= preview_area.x {
                            let max_s = rendered
                                .as_ref()
                                .map(|r| r.height.saturating_sub(preview_px_h))
                                .unwrap_or(0);
                            preview_scroll_y = (preview_scroll_y + cell_h * 4).min(max_s);
                            dirty_blit = true;
                        } else if !preview_only {
                            buf.scroll_row =
                                (buf.scroll_row + 3).min(buf.lines.len().saturating_sub(1));
                        }
                    }
                    MouseEventKind::ScrollUp => {
                        if m.column >= preview_area.x {
                            preview_scroll_y = preview_scroll_y.saturating_sub(cell_h * 4);
                            dirty_blit = true;
                        } else if !preview_only {
                            buf.scroll_row = buf.scroll_row.saturating_sub(3);
                        }
                    }
                    _ => {}
                },
                Event::Resize(_, _) => {
                    clear_kitty_images();
                    needs_rerender = true;
                    dirty_blit = true;
                    last_edit_at = Instant::now() - Duration::from_secs(1);
                }
                _ => {}
            }
        }
    };

    clear_kitty_images();
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    run_res
}

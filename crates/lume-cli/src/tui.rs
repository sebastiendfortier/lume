use crate::editor;
use anyhow::Result;
use crossterm::cursor::MoveTo;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, window_size, EnterAlternateScreen, LeaveAlternateScreen,
};
use lume_core::kitty::{delete_all_images, encode_png_placed, TerminalCaps};
use lume_core::{
    parse_markdown, render_ansi, render_pixels, AnsiOptions, Document, PixelOptions,
    RenderedDocument, ThemePreset,
};
use notify::{Event as NotifyEvent, RecommendedWatcher, RecursiveMode, Watcher};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Terminal;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const KITTY_IMG_ID: u32 = 4242;

pub fn query_cell_px() -> (u32, u32) {
    if let Ok(ws) = window_size() {
        if ws.columns > 0 && ws.rows > 0 && ws.width > 0 && ws.height > 0 {
            let cw = (ws.width as u32 / ws.columns as u32).clamp(6, 32);
            let ch = (ws.height as u32 / ws.rows as u32).clamp(10, 64);
            return (cw, ch);
        }
    }
    (10, 20)
}

pub fn run_viewer(
    path: PathBuf,
    initial_theme: ThemePreset,
    initial_pixel: bool,
    initial_font_size: f32,
) -> Result<()> {
    let mut source = std::fs::read_to_string(&path).unwrap_or_default();
    let base_dir = path.parent().map(|p| p.to_path_buf());
    let caps = TerminalCaps::detect();

    // Start live file watcher on parent directory (handles atomic editor saves like vim/fresh/nvim)
    let (watch_tx, watch_rx) = mpsc::channel::<()>();
    let target_name = path.file_name().map(|n| n.to_os_string());
    let watch_dir = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut _watcher: Option<RecommendedWatcher> =
        notify::recommended_watcher(move |res: notify::Result<NotifyEvent>| {
            if let Ok(ev) = res {
                let matches = target_name.as_ref().is_none_or(|tn| {
                    ev.paths.iter().any(|p| p.file_name() == Some(tn.as_os_str()))
                });
                if matches {
                    let _ = watch_tx.send(());
                }
            }
        })
        .ok();
    if let Some(ref mut w) = _watcher {
        let _ = w.watch(&watch_dir, RecursiveMode::NonRecursive);
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    let mut theme_preset = initial_theme;
    let mut pixel_mode = initial_pixel;
    let mut font_size = initial_font_size;
    let mut show_toc = false;
    let mut toc_selected = 0usize;
    let mut scroll_y: u32 = 0; // pixel Y in pixel mode, line index in ANSI mode
    let mut search_active = false;
    let mut search_query = String::new();
    let mut status_msg = String::from("Live watch active");
    let mut switch_to_edit = false;

    let mut doc: Document = parse_markdown(&source);
    let mut rendered: Option<RenderedDocument> = None;
    let mut ansi_lines: Vec<String> = Vec::new();
    let mut last_preview_px: (u32, u32) = (0, 0);
    let mut last_preview_cells: (u16, u16) = (0, 0);
    let mut dirty_render = true;
    let mut dirty_blit = true;
    let mut last_reload = Instant::now() - Duration::from_secs(1);

    let run_res: Result<()> = loop {
        // Poll file watcher
        let mut file_changed = false;
        while watch_rx.try_recv().is_ok() {
            file_changed = true;
        }
        if file_changed && last_reload.elapsed() > Duration::from_millis(80) {
            if let Ok(new_src) = std::fs::read_to_string(&path) {
                if new_src != source {
                    source = new_src;
                    doc = parse_markdown(&source);
                    dirty_render = true;
                    dirty_blit = true;
                    last_reload = Instant::now();
                    status_msg = "Synced from disk".to_string();
                }
            }
        }

        let size = terminal.size()?;
        let (cell_w, cell_h) = query_cell_px();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(4),
                Constraint::Length(1),
            ])
            .split(Rect::new(0, 0, size.width, size.height));

        let body_chunks = if show_toc {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(30), Constraint::Min(20)])
                .split(chunks[1])
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(20)])
                .split(chunks[1])
        };

        let preview_area = if show_toc {
            body_chunks[1]
        } else {
            body_chunks[0]
        };

        let preview_px_w = (preview_area.width as u32 * cell_w).clamp(320, 2200);
        let preview_px_h = (preview_area.height as u32 * cell_h).clamp(120, 2000);

        if pixel_mode {
            if dirty_render || last_preview_px.0 != preview_px_w {
                let mut opts = PixelOptions::new(preview_px_w, theme_preset.theme());
                opts.font_size = font_size;
                opts.base_dir = base_dir.clone();
                if let Ok(r) = render_pixels(&doc, &opts) {
                    let max_scroll = r.height.saturating_sub(preview_px_h);
                    scroll_y = scroll_y.min(max_scroll);
                    rendered = Some(r);
                }
                last_preview_px = (preview_px_w, preview_px_h);
                dirty_render = false;
                dirty_blit = true;
            }
        } else if dirty_render || last_preview_cells.0 != preview_area.width {
            let ansi = render_ansi(
                &doc,
                &AnsiOptions::new(
                    preview_area.width.saturating_sub(2) as usize,
                    theme_preset.theme(),
                    caps,
                ),
            );
            ansi_lines = ansi.lines().map(strip_ansi_for_ratatui).collect();
            last_preview_cells = (preview_area.width, preview_area.height);
            dirty_render = false;
            dirty_blit = false;
        }

        let pct = if pixel_mode {
            rendered
                .as_ref()
                .map(|r| {
                    let max_s = r.height.saturating_sub(preview_px_h);
                    (scroll_y as u64 * 100)
                        .checked_div(max_s as u64)
                        .map_or(100, |v| v.min(100) as u32)
                })
                .unwrap_or(0)
        } else {
            let max_s = ansi_lines
                .len()
                .saturating_sub(preview_area.height as usize);
            (scroll_y as usize * 100)
                .checked_div(max_s)
                .map_or(100, |v| v.min(100) as u32)
        };

        let fname = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());

        terminal.draw(|f| {
            let t = theme_preset.theme();
            let hdr_bg = Color::Rgb(t.code_header_bg.0, t.code_header_bg.1, t.code_header_bg.2);
            let hdr_fg = Color::Rgb(t.text.0, t.text.1, t.text.2);
            let acc = Color::Rgb(t.accent_note.0, t.accent_note.1, t.accent_note.2);
            let ok = Color::Rgb(t.accent_tip.0, t.accent_tip.1, t.accent_tip.2);

            let mode_badge = if pixel_mode { "PIXEL" } else { "ANSI" };
            let header = Line::from(vec![
                Span::styled(
                    " lume ",
                    Style::default()
                        .fg(Color::Rgb(t.bg.0, t.bg.1, t.bg.2))
                        .bg(acc)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {fname} "), Style::default().fg(hdr_fg).bg(hdr_bg).add_modifier(Modifier::BOLD)),
                Span::styled(" [LIVE] ", Style::default().fg(ok).bg(hdr_bg).add_modifier(Modifier::BOLD)),
                Span::styled(
                    format!("[{mode_badge}] [{}] {pct:>3}%  {status_msg}", theme_preset.label()),
                    Style::default().fg(Color::Rgb(t.text_muted.0, t.text_muted.1, t.text_muted.2)).bg(hdr_bg),
                ),
            ]);
            f.render_widget(
                Paragraph::new(header).style(Style::default().bg(hdr_bg)),
                chunks[0],
            );

            if show_toc {
                let items: Vec<ListItem> = doc
                    .toc
                    .iter()
                    .map(|h| {
                        let indent = "  ".repeat(h.level.saturating_sub(1) as usize);
                        ListItem::new(format!("{indent}{}", h.title))
                    })
                    .collect();
                let mut state = ListState::default();
                if !doc.toc.is_empty() {
                    state.select(Some(toc_selected.min(doc.toc.len() - 1)));
                }
                let list = List::new(items)
                    .block(Block::default().title(" Contents ").borders(Borders::RIGHT))
                    .highlight_style(Style::default().fg(acc).add_modifier(Modifier::BOLD));
                f.render_stateful_widget(list, body_chunks[0], &mut state);
            }

            if !pixel_mode {
                let visible: Vec<Line> = ansi_lines
                    .iter()
                    .skip(scroll_y as usize)
                    .take(preview_area.height as usize)
                    .map(|l| Line::from(l.as_str()))
                    .collect();
                f.render_widget(
                    Paragraph::new(visible).style(Style::default().fg(hdr_fg)),
                    preview_area,
                );
            } else {
                // Clear preview cells with theme background so Kitty graphics sits cleanly on top
                let bg = Color::Rgb(t.bg.0, t.bg.1, t.bg.2);
                f.render_widget(Block::default().style(Style::default().bg(bg)), preview_area);
            }

            let footer_text = if search_active {
                format!(" /{search_query}_   (Enter: jump, Esc: cancel)")
            } else {
                " j/k/Wheel:scroll  t:TOC  /:find  g:Pixel/ANSI  T:theme  +/-:zoom  e:edit-split  q:quit"
                    .to_string()
            };
            f.render_widget(
                Paragraph::new(footer_text).style(Style::default().fg(hdr_fg).bg(hdr_bg)),
                chunks[2],
            );
        })?;

        // Blit pixel viewport via Kitty Graphics Protocol
        if pixel_mode && dirty_blit {
            if let Some(ref r) = rendered {
                if let Ok(png) = r.crop_png(scroll_y, preview_px_h, theme_preset.theme().bg) {
                    let seq = encode_png_placed(
                        &png,
                        KITTY_IMG_ID,
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

        if event::poll(Duration::from_millis(60))? {
            match event::read()? {
                Event::Key(key) => {
                    if search_active {
                        match key.code {
                            KeyCode::Esc => {
                                search_active = false;
                                search_query.clear();
                            }
                            KeyCode::Enter => {
                                search_active = false;
                                let q = search_query.to_lowercase();
                                if let Some(ref r) = rendered {
                                    if let Some((entry, hy)) = r
                                        .heading_ys
                                        .iter()
                                        .find(|(e, _)| e.title.to_lowercase().contains(&q))
                                    {
                                        scroll_y = *hy;
                                        dirty_blit = true;
                                        status_msg = format!("Jumped to: {}", entry.title);
                                    } else {
                                        status_msg = format!("No heading matching '{search_query}'");
                                    }
                                }
                            }
                            KeyCode::Backspace => {
                                search_query.pop();
                            }
                            KeyCode::Char(c) => {
                                search_query.push(c);
                            }
                            _ => {}
                        }
                        continue;
                    }

                    let step_small = if pixel_mode { cell_h * 3 } else { 2 };
                    let step_page = if pixel_mode {
                        preview_px_h.saturating_sub(cell_h * 2).max(60)
                    } else {
                        preview_area.height.saturating_sub(2).max(5) as u32
                    };
                    let max_scroll = if pixel_mode {
                        rendered
                            .as_ref()
                            .map(|r| r.height.saturating_sub(preview_px_h))
                            .unwrap_or(0)
                    } else {
                        ansi_lines
                            .len()
                            .saturating_sub(preview_area.height as usize) as u32
                    };

                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break Ok(()),
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break Ok(())
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            if show_toc && key.modifiers.contains(KeyModifiers::SHIFT) {
                                if !doc.toc.is_empty() {
                                    toc_selected = (toc_selected + 1) % doc.toc.len();
                                }
                            } else {
                                scroll_y = (scroll_y + step_small).min(max_scroll);
                                dirty_blit = true;
                            }
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            if show_toc && key.modifiers.contains(KeyModifiers::SHIFT) {
                                if !doc.toc.is_empty() {
                                    toc_selected = toc_selected
                                        .checked_sub(1)
                                        .unwrap_or(doc.toc.len() - 1);
                                }
                            } else {
                                scroll_y = scroll_y.saturating_sub(step_small);
                                dirty_blit = true;
                            }
                        }
                        KeyCode::PageDown | KeyCode::Char(' ') => {
                            scroll_y = (scroll_y + step_page).min(max_scroll);
                            dirty_blit = true;
                        }
                        KeyCode::PageUp => {
                            scroll_y = scroll_y.saturating_sub(step_page);
                            dirty_blit = true;
                        }
                        KeyCode::Home => {
                            scroll_y = 0;
                            dirty_blit = true;
                        }
                        KeyCode::End | KeyCode::Char('G') => {
                            scroll_y = max_scroll;
                            dirty_blit = true;
                        }
                        KeyCode::Char('t') => {
                            show_toc = !show_toc;
                            clear_kitty_images();
                            dirty_render = true;
                            dirty_blit = true;
                        }
                        KeyCode::Enter if show_toc => {
                            if let Some(ref r) = rendered {
                                if let Some((entry, hy)) = r.heading_ys.get(toc_selected) {
                                    scroll_y = (*hy).min(max_scroll);
                                    dirty_blit = true;
                                    status_msg = format!("Jumped to: {}", entry.title);
                                }
                            }
                        }
                        KeyCode::Tab if show_toc && !doc.toc.is_empty() => {
                            toc_selected = (toc_selected + 1) % doc.toc.len();
                            if let Some(ref r) = rendered {
                                if let Some((_, hy)) = r.heading_ys.get(toc_selected) {
                                    scroll_y = (*hy).min(max_scroll);
                                    dirty_blit = true;
                                }
                            }
                        }
                        KeyCode::Char('g') => {
                            pixel_mode = !pixel_mode;
                            clear_kitty_images();
                            scroll_y = 0;
                            dirty_render = true;
                            dirty_blit = true;
                            status_msg = format!(
                                "Switched to {} mode",
                                if pixel_mode { "Pixel" } else { "ANSI" }
                            );
                        }
                        KeyCode::Char('T') => {
                            theme_preset = theme_preset.next();
                            dirty_render = true;
                            dirty_blit = true;
                            status_msg = format!("Theme: {}", theme_preset.label());
                        }
                        KeyCode::Char('+') | KeyCode::Char('=') => {
                            font_size = (font_size + 1.5).min(28.0);
                            dirty_render = true;
                            dirty_blit = true;
                            status_msg = format!("Font size: {font_size:.0}px");
                        }
                        KeyCode::Char('-') => {
                            font_size = (font_size - 1.5).max(10.0);
                            dirty_render = true;
                            dirty_blit = true;
                            status_msg = format!("Font size: {font_size:.0}px");
                        }
                        KeyCode::Char('/') => {
                            search_active = true;
                            search_query.clear();
                        }
                        KeyCode::Char('e') => {
                            switch_to_edit = true;
                            break Ok(());
                        }
                        KeyCode::Char('r') => {
                            if let Ok(new_src) = std::fs::read_to_string(&path) {
                                source = new_src;
                                doc = parse_markdown(&source);
                                dirty_render = true;
                                dirty_blit = true;
                                status_msg = "Reloaded".to_string();
                            }
                        }
                        _ => {}
                    }
                }
                Event::Mouse(m) => {
                    let step = if pixel_mode { cell_h * 4 } else { 3 };
                    let max_scroll = if pixel_mode {
                        rendered
                            .as_ref()
                            .map(|r| r.height.saturating_sub(preview_px_h))
                            .unwrap_or(0)
                    } else {
                        ansi_lines
                            .len()
                            .saturating_sub(preview_area.height as usize) as u32
                    };
                    match m.kind {
                        MouseEventKind::ScrollDown => {
                            scroll_y = (scroll_y + step).min(max_scroll);
                            dirty_blit = true;
                        }
                        MouseEventKind::ScrollUp => {
                            scroll_y = scroll_y.saturating_sub(step);
                            dirty_blit = true;
                        }
                        _ => {}
                    }
                }
                Event::Resize(_, _) => {
                    clear_kitty_images();
                    dirty_render = true;
                    dirty_blit = true;
                }
                _ => {}
            }
        }
    };

    clear_kitty_images();
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);

    run_res?;
    if switch_to_edit {
        return editor::run_editor(path, theme_preset, pixel_mode, font_size);
    }
    Ok(())
}

pub fn clear_kitty_images() {
    let mut out = io::stdout().lock();
    let _ = out.write_all(delete_all_images().as_bytes());
    let _ = out.flush();
}

pub fn strip_ansi_for_ratatui(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == 0x1b {
            i += 1;
            if i < bytes.len() && bytes[i] == b'[' {
                i += 1;
                while i < bytes.len() && !(bytes[i] >= 0x40 && bytes[i] <= 0x7e) {
                    i += 1;
                }
                i += 1;
                continue;
            } else if i < bytes.len() && bytes[i] == b']' {
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == 0x07 {
                        i += 1;
                        break;
                    }
                    if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
                continue;
            }
        }
        let ch = s[i..].chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

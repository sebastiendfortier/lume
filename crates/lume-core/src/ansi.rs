use crate::ast::{AlertKind, Block, BlockKind, ColAlign, Document, InlineSpan};
use crate::highlight::highlight_code;
use crate::kitty::TerminalCaps;
use crate::math::latex_to_unicode;
use crate::theme::{Rgb, Theme};
use unicode_width::UnicodeWidthStr;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const ITALIC: &str = "\x1b[3m";
const UNDERLINE: &str = "\x1b[4m";
const STRIKE: &str = "\x1b[9m";

#[derive(Debug, Clone)]
pub struct AnsiOptions {
    pub width: usize,
    pub theme: Theme,
    pub osc8_links: bool,
    /// Emit Kitty OSC 66 scaled text for H1/H2 headings when supported.
    pub osc66_headings: bool,
}

impl AnsiOptions {
    pub fn new(width: usize, theme: Theme, caps: TerminalCaps) -> Self {
        Self {
            width: width.clamp(30, 200),
            theme,
            osc8_links: true,
            osc66_headings: caps.supports_osc66(),
        }
    }
}

/// Render a parsed Markdown `Document` to rich 24-bit ANSI text
/// (supports OSC 8 links, OSC 66 headings in Kitty, GitHub alerts, tables, syntax highlighting, math).
pub fn render_ansi(doc: &Document, opts: &AnsiOptions) -> String {
    let mut lines: Vec<String> = Vec::new();
    for blk in &doc.blocks {
        render_block(blk, opts, 0, opts.width, &mut lines);
    }
    lines.join("\n")
}

fn render_block(
    blk: &Block,
    opts: &AnsiOptions,
    indent: usize,
    avail_w: usize,
    out: &mut Vec<String>,
) {
    let t = opts.theme;
    let pad = " ".repeat(indent);
    let w = avail_w.saturating_sub(indent).max(20);

    match &blk.kind {
        BlockKind::Heading { level, plain, .. } => {
            if !out.is_empty() {
                out.push(String::new());
            }
            let col = if *level >= 5 { t.text_muted } else { t.heading };
            if opts.osc66_headings && *level <= 2 {
                // Kitty Text Sizing Protocol (OSC 66)
                out.push(format!(
                    "{pad}\x1b]66;s=2;{BOLD}{}{plain}{RESET}\x07",
                    col.ansi_fg()
                ));
                out.push(String::new());
            } else {
                let prefix = match level {
                    1 => "",
                    2 => "",
                    3 => "### ",
                    4 => "#### ",
                    _ => "##### ",
                };
                out.push(format!(
                    "{pad}{BOLD}{}{prefix}{plain}{RESET}",
                    col.ansi_fg()
                ));
            }
            if *level == 1 {
                out.push(format!("{pad}{}{}{RESET}", t.accent_note.ansi_fg(), "━".repeat(w.min(plain.width() + 4).max(16))));
            } else if *level == 2 {
                out.push(format!("{pad}{}{}{RESET}", t.border.ansi_fg(), "─".repeat(w.min(plain.width() + 4).max(16))));
            }
            out.push(String::new());
        }
        BlockKind::Paragraph { spans } => {
            let rendered = render_inlines(spans, opts, t.text);
            for line in wrap_ansi(&rendered, w) {
                out.push(format!("{pad}{line}"));
            }
            out.push(String::new());
        }
        BlockKind::CodeBlock { lang, code } => {
            let label = if lang.is_empty() { "code" } else { lang.as_str() };
            let inner_w = w.saturating_sub(4).max(16);
            let top_fill = inner_w.saturating_sub(label.len() + 3);
            out.push(format!(
                "{pad}{}╭─{RESET} {BOLD}{}{label}{RESET} {}{}╮{RESET}",
                t.border.ansi_fg(),
                t.accent_note.ansi_fg(),
                t.border.ansi_fg(),
                "─".repeat(top_fill)
            ));
            let highlighted = highlight_code(code, lang, &t);
            for line_tokens in highlighted {
                let mut s = String::new();
                for tok in line_tokens {
                    s.push_str(&tok.fg.ansi_fg());
                    if tok.bold {
                        s.push_str(BOLD);
                    }
                    if tok.italic {
                        s.push_str(ITALIC);
                    }
                    s.push_str(&tok.text);
                    s.push_str(RESET);
                }
                out.push(format!("{pad}{}│{RESET} {s}", t.border.ansi_fg()));
            }
            out.push(format!(
                "{pad}{}╰{}╯{RESET}",
                t.border.ansi_fg(),
                "─".repeat(inner_w + 2)
            ));
            out.push(String::new());
        }
        BlockKind::Mermaid { code } => {
            let inner_w = w.saturating_sub(4).max(16);
            let fill = inner_w.saturating_sub(18);
            out.push(format!(
                "{pad}{}╭─{RESET} {BOLD}{}mermaid diagram{RESET} {}{}╮{RESET}",
                t.accent_important.ansi_fg(),
                t.accent_important.ansi_fg(),
                t.accent_important.ansi_fg(),
                "─".repeat(fill)
            ));
            for line in code.lines() {
                out.push(format!(
                    "{pad}{}│{RESET} {}{line}{RESET}",
                    t.accent_important.ansi_fg(),
                    t.text_muted.ansi_fg()
                ));
            }
            out.push(format!(
                "{pad}{}╰{}╯{RESET}",
                t.accent_important.ansi_fg(),
                "─".repeat(inner_w + 2)
            ));
            out.push(String::new());
        }
        BlockKind::DisplayMath { latex } => {
            let formatted = latex_to_unicode(latex);
            out.push(format!(
                "{pad}  {BOLD}{}{formatted}{RESET}",
                t.heading.ansi_fg()
            ));
            out.push(String::new());
        }
        BlockKind::BlockQuote { blocks } => {
            let mut inner = Vec::new();
            for b in blocks {
                render_block(b, opts, 0, w.saturating_sub(3), &mut inner);
            }
            while inner.last().map(|s| s.is_empty()).unwrap_or(false) {
                inner.pop();
            }
            for l in inner {
                out.push(format!(
                    "{pad}{}┃{RESET} {ITALIC}{}{l}{RESET}",
                    t.quote_bar.ansi_fg(),
                    t.text_muted.ansi_fg()
                ));
            }
            out.push(String::new());
        }
        BlockKind::Alert { kind, blocks } => {
            let accent = match kind {
                AlertKind::Note => t.accent_note,
                AlertKind::Tip => t.accent_tip,
                AlertKind::Important => t.accent_important,
                AlertKind::Warning => t.accent_warning,
                AlertKind::Caution => t.accent_caution,
            };
            out.push(format!(
                "{pad}{}┃{RESET} {BOLD}{}[{}] {}{RESET}",
                accent.ansi_fg(),
                accent.ansi_fg(),
                kind.icon().to_uppercase(),
                kind.label()
            ));
            let mut inner = Vec::new();
            for b in blocks {
                render_block(b, opts, 0, w.saturating_sub(3), &mut inner);
            }
            while inner.last().map(|s| s.is_empty()).unwrap_or(false) {
                inner.pop();
            }
            for l in inner {
                out.push(format!("{pad}{}┃{RESET} {l}", accent.ansi_fg()));
            }
            out.push(String::new());
        }
        BlockKind::List {
            ordered_start,
            items,
        } => {
            for (i, item) in items.iter().enumerate() {
                let marker = if let Some(checked) = item.checked {
                    if checked {
                        format!("{}☑{RESET} ", t.check_done.ansi_fg())
                    } else {
                        format!("{}☐{RESET} ", t.text_muted.ansi_fg())
                    }
                } else if let Some(start) = ordered_start {
                    format!("{}{}.{RESET} ", t.accent_note.ansi_fg(), start + i as u64)
                } else {
                    format!("{}•{RESET} ", t.accent_note.ansi_fg())
                };
                let mut inner = Vec::new();
                for b in &item.blocks {
                    render_block(b, opts, 0, w.saturating_sub(3), &mut inner);
                }
                while inner.last().map(|s| s.is_empty()).unwrap_or(false) {
                    inner.pop();
                }
                for (li, l) in inner.into_iter().enumerate() {
                    if li == 0 {
                        out.push(format!("{pad}{marker}{l}"));
                    } else {
                        out.push(format!("{pad}  {l}"));
                    }
                }
            }
            out.push(String::new());
        }
        BlockKind::Table {
            alignments,
            headers,
            rows,
        } => {
            let ncols = headers
                .len()
                .max(rows.iter().map(|r| r.len()).max().unwrap_or(0))
                .max(1);
            let mut col_widths = vec![3usize; ncols];
            let plain_cell = |spans: &[InlineSpan]| -> String {
                spans.iter().map(|s| s.text.as_str()).collect::<String>()
            };
            for (ci, h) in headers.iter().enumerate() {
                col_widths[ci] = col_widths[ci].max(plain_cell(h).width());
            }
            for row in rows {
                for (ci, c) in row.iter().enumerate() {
                    if ci < ncols {
                        col_widths[ci] = col_widths[ci].max(plain_cell(c).width());
                    }
                }
            }
            // Clamp total width to terminal width
            let max_col = ((w.saturating_sub(ncols * 3 + 1)) / ncols).max(6);
            for cw in &mut col_widths {
                *cw = (*cw).min(max_col);
            }

            let border_fg = t.border.ansi_fg();
            let top: String = col_widths
                .iter()
                .map(|&cw| "─".repeat(cw + 2))
                .collect::<Vec<_>>()
                .join("┬");
            let mid: String = col_widths
                .iter()
                .map(|&cw| "─".repeat(cw + 2))
                .collect::<Vec<_>>()
                .join("┼");
            let bot: String = col_widths
                .iter()
                .map(|&cw| "─".repeat(cw + 2))
                .collect::<Vec<_>>()
                .join("┴");

            out.push(format!("{pad}{border_fg}┌{top}┐{RESET}"));
            if !headers.is_empty() {
                let cells: Vec<String> = (0..ncols)
                    .map(|ci| {
                        let txt = headers.get(ci).map(|s| plain_cell(s)).unwrap_or_default();
                        let align = alignments.get(ci).copied().unwrap_or(ColAlign::Left);
                        let padded = pad_cell(&txt, col_widths[ci], align);
                        format!(" {BOLD}{}{padded}{RESET} ", t.heading.ansi_fg())
                    })
                    .collect();
                out.push(format!(
                    "{pad}{border_fg}│{RESET}{}{border_fg}│{RESET}",
                    cells.join(&format!("{border_fg}│{RESET}"))
                ));
                out.push(format!("{pad}{border_fg}├{mid}┤{RESET}"));
            }
            for row in rows {
                let cells: Vec<String> = (0..ncols)
                    .map(|ci| {
                        let txt = row.get(ci).map(|s| plain_cell(s)).unwrap_or_default();
                        let align = alignments.get(ci).copied().unwrap_or(ColAlign::Left);
                        let padded = pad_cell(&txt, col_widths[ci], align);
                        format!(" {}{padded}{RESET} ", t.text.ansi_fg())
                    })
                    .collect();
                out.push(format!(
                    "{pad}{border_fg}│{RESET}{}{border_fg}│{RESET}",
                    cells.join(&format!("{border_fg}│{RESET}"))
                ));
            }
            out.push(format!("{pad}{border_fg}└{bot}┘{RESET}"));
            out.push(String::new());
        }
        BlockKind::Image { url, alt } => {
            let label = if alt.is_empty() { url } else { alt };
            out.push(format!(
                "{pad}{ITALIC}{}[image: {label}]{RESET}",
                t.text_muted.ansi_fg()
            ));
            out.push(String::new());
        }
        BlockKind::Rule => {
            out.push(format!("{pad}{}{}{RESET}", t.hr.ansi_fg(), "─".repeat(w)));
            out.push(String::new());
        }
    }
}

fn render_inlines(spans: &[InlineSpan], opts: &AnsiOptions, base_fg: Rgb) -> String {
    let t = opts.theme;
    let mut out = String::new();
    for sp in spans {
        let text = if sp.style.math {
            latex_to_unicode(&sp.text)
        } else {
            sp.text.clone()
        };
        if sp.style.code {
            out.push_str(&format!(
                "{}{} {text} {RESET}",
                t.inline_code_bg.ansi_bg(),
                t.inline_code_fg.ansi_fg()
            ));
            continue;
        }
        let mut s = String::new();
        if let Some(ref url) = sp.style.link {
            if opts.osc8_links {
                s.push_str(&format!("\x1b]8;;{url}\x1b\\"));
            }
            s.push_str(&t.link.ansi_fg());
            s.push_str(UNDERLINE);
        } else {
            s.push_str(&base_fg.ansi_fg());
        }
        if sp.style.bold {
            s.push_str(BOLD);
        }
        if sp.style.italic {
            s.push_str(ITALIC);
        }
        if sp.style.strikethrough {
            s.push_str(STRIKE);
        }
        s.push_str(&text);
        s.push_str(RESET);
        if sp.style.link.is_some() && opts.osc8_links {
            s.push_str("\x1b]8;;\x1b\\");
        }
        out.push_str(&s);
    }
    out
}

fn pad_cell(s: &str, width: usize, align: ColAlign) -> String {
    let w = s.width();
    if w > width {
        let mut out = String::new();
        let mut acc = 0;
        for ch in s.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if acc + cw + 1 > width {
                out.push('…');
                break;
            }
            out.push(ch);
            acc += cw;
        }
        while out.width() < width {
            out.push(' ');
        }
        return out;
    }
    let diff = width - w;
    match align {
        ColAlign::Left => format!("{s}{}", " ".repeat(diff)),
        ColAlign::Right => format!("{}{s}", " ".repeat(diff)),
        ColAlign::Center => {
            let left = diff / 2;
            let right = diff - left;
            format!("{}{s}{}", " ".repeat(left), " ".repeat(right))
        }
    }
}

fn wrap_ansi(s: &str, max_w: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for hard_line in s.split('\n') {
        let mut cur = String::new();
        let mut cur_w = 0usize;
        for word in hard_line.split_inclusive(' ') {
            let ww = visible_width(word);
            if cur_w > 0 && cur_w + ww > max_w {
                lines.push(cur.trim_end().to_string());
                cur = word.to_string();
                cur_w = ww;
            } else {
                cur.push_str(word);
                cur_w += ww;
            }
        }
        lines.push(cur);
    }
    lines
}

fn visible_width(s: &str) -> usize {
    let mut w = 0usize;
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
        w += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        i += ch.len_utf8();
    }
    w
}

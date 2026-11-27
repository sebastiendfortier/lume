use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalCaps {
    /// Kitty terminal: Kitty Graphics Protocol + OSC 66 text sizing + OSC 8 links.
    Kitty,
    /// WezTerm / Ghostty / Konsole: Kitty Graphics Protocol + OSC 8 links.
    KittyGraphics,
    /// Standard truecolor ANSI terminal (fallback).
    AnsiOnly,
}

impl TerminalCaps {
    pub fn detect() -> Self {
        let term = std::env::var("TERM").unwrap_or_default().to_lowercase();
        let prog = std::env::var("TERM_PROGRAM")
            .unwrap_or_default()
            .to_lowercase();

        if std::env::var_os("KITTY_WINDOW_ID").is_some() || term.contains("kitty") {
            return Self::Kitty;
        }
        if std::env::var_os("WEZTERM_PANE").is_some()
            || prog.contains("wezterm")
            || prog.contains("ghostty")
            || term.contains("ghostty")
        {
            return Self::KittyGraphics;
        }
        Self::AnsiOnly
    }

    pub fn supports_kitty_graphics(self) -> bool {
        matches!(self, Self::Kitty | Self::KittyGraphics)
    }

    pub fn supports_osc66(self) -> bool {
        matches!(self, Self::Kitty)
    }
}

/// Encode PNG bytes as a chunked Kitty Graphics Protocol sequence (one-shot inline display).
/// Works natively in WezTerm, Kitty, and Ghostty.
pub fn encode_png_inline(png_bytes: &[u8], cols: Option<u16>, rows: Option<u16>) -> String {
    encode_png_inner(png_bytes, None, cols, rows, false)
}

/// Encode PNG bytes with an explicit image ID and cursor-preserving placement (`C=1`)
/// for flicker-free TUI rendering at the current cursor position.
pub fn encode_png_placed(png_bytes: &[u8], image_id: u32, cols: u16, rows: u16) -> String {
    encode_png_inner(png_bytes, Some(image_id), Some(cols), Some(rows), true)
}

/// Delete a specific Kitty Graphics image ID (and its placements).
pub fn delete_image(image_id: u32) -> String {
    format!("\x1b_Ga=d,d=I,i={image_id},q=2\x1b\\")
}

/// Delete all Kitty Graphics placements on screen.
pub fn delete_all_images() -> String {
    "\x1b_Ga=d,d=A,q=2\x1b\\".to_string()
}

fn encode_png_inner(
    png_bytes: &[u8],
    image_id: Option<u32>,
    cols: Option<u16>,
    rows: Option<u16>,
    no_cursor_move: bool,
) -> String {
    let b64 = B64.encode(png_bytes);
    const CHUNK: usize = 4096;
    let total = b64.len();
    let mut out = String::with_capacity(total + 256);

    let mut offset = 0;
    let mut first = true;
    while offset < total {
        let end = (offset + CHUNK).min(total);
        let more = if end < total { 1 } else { 0 };
        let chunk = &b64[offset..end];

        if first {
            out.push_str("\x1b_Ga=T,f=100,q=2");
            if let Some(id) = image_id {
                out.push_str(&format!(",i={id}"));
            }
            if let Some(c) = cols {
                out.push_str(&format!(",c={c}"));
            }
            if let Some(r) = rows {
                out.push_str(&format!(",r={r}"));
            }
            if no_cursor_move {
                out.push_str(",C=1");
            }
            out.push_str(&format!(",m={more};{chunk}\x1b\\"));
            first = false;
        } else {
            out.push_str(&format!("\x1b_Gm={more};{chunk}\x1b\\"));
        }

        offset = end;
    }

    out
}

mod editor;
mod tui;

use anyhow::{bail, Context, Result};
use clap::Parser;
use lume_core::kitty::{encode_png_inline, TerminalCaps};
use lume_core::{
    parse_markdown, render_ansi, render_pixels, AnsiOptions, PixelOptions, ThemePreset,
};
use std::io::{self, IsTerminal, Read, Write};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "lume",
    version,
    about = "Pixel-rendered & ANSI terminal Markdown viewer and live two-pane editor"
)]
struct Cli {
    /// Markdown file to view or edit (reads stdin if omitted or '-')
    file: Option<PathBuf>,

    /// Open interactive viewer with live file watch/sync (default on TTY for a file)
    #[arg(short = 'w', long = "watch")]
    watch: bool,

    /// Open two-pane split mode: Markdown editor (left) + live scroll-synced preview (right)
    #[arg(short = 'e', long = "edit")]
    edit: bool,

    /// One-shot output to stdout (no TUI), suitable for pipes or pagers
    #[arg(short = 'p', long = "print")]
    print_oneshot: bool,

    /// Force ANSI text rendering (with OSC 8 links and OSC 66 headings in Kitty)
    #[arg(long = "ansi")]
    ansi: bool,

    /// Force Pixel rendering (Kitty Graphics Protocol — WezTerm/Kitty/Ghostty)
    #[arg(long = "pixel")]
    pixel: bool,

    /// Render the Markdown file directly to a PNG image (ideal for Yazi previewers)
    #[arg(long = "render-png", value_name = "OUT_PNG")]
    render_png: Option<PathBuf>,

    /// Color theme: github-dark, github-light, vscode-dark, catppuccin-mocha
    #[arg(short = 's', long = "theme", default_value = "github-dark")]
    theme: String,

    /// Text column width for ANSI mode (defaults to terminal width or 100)
    #[arg(long = "width")]
    width: Option<usize>,

    /// Canvas width in pixels for Pixel / PNG mode (default: 960)
    #[arg(long = "pixel-width", default_value_t = 960)]
    pixel_width: u32,

    /// Maximum PNG height in pixels when using --render-png (crops top viewport if set)
    #[arg(long = "max-height")]
    max_height: Option<u32>,

    /// Base font size in pixels for Pixel / PNG mode (default: 15.0)
    #[arg(long = "font-size", default_value_t = 15.0)]
    font_size: f32,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let theme_preset = ThemePreset::parse(&cli.theme).unwrap_or_else(|| {
        eprintln!(
            "lume: unknown theme '{}', using github-dark (available: github-dark, github-light, vscode-dark, catppuccin-mocha)",
            cli.theme
        );
        ThemePreset::GithubDark
    });
    let theme = theme_preset.theme();
    let caps = TerminalCaps::detect();

    // Mode 1: Direct PNG export (for Yazi preview plugin or scripts)
    if let Some(ref out_png) = cli.render_png {
        let (md, base_dir) = read_input(cli.file.as_ref())?;
        let doc = parse_markdown(&md);
        let mut opts = PixelOptions::new(cli.pixel_width, theme);
        opts.font_size = cli.font_size;
        opts.base_dir = base_dir;
        let rendered = render_pixels(&doc, &opts)?;
        let png = if let Some(max_h) = cli.max_height {
            if rendered.height > max_h {
                rendered.crop_png(0, max_h, theme.bg)?
            } else {
                rendered.to_png()?
            }
        } else {
            rendered.to_png()?
        };
        if let Some(parent) = out_png.parent() {
            if !parent.as_os_str().is_empty() {
                let _ = std::fs::create_dir_all(parent);
            }
        }
        std::fs::write(out_png, png)
            .with_context(|| format!("failed to write PNG to {}", out_png.display()))?;
        return Ok(());
    }

    // Mode 2: Two-pane Editor + Live Preview
    if cli.edit {
        let path = match cli.file {
            Some(p) if p.as_os_str() != "-" => p,
            _ => bail!("--edit requires a file path (e.g. `lume --edit README.md`)"),
        };
        let use_pixel = if cli.ansi {
            false
        } else if cli.pixel {
            true
        } else {
            caps.supports_kitty_graphics()
        };
        return editor::run_editor(path, theme_preset, use_pixel, cli.font_size);
    }

    let stdout_is_tty = io::stdout().is_terminal();
    let has_file = cli
        .file
        .as_ref()
        .map(|p| p.as_os_str() != "-")
        .unwrap_or(false);

    // Mode 3: Interactive Viewer + Watch TUI
    let want_tui = cli.watch || (stdout_is_tty && has_file && !cli.print_oneshot && !cli.ansi && !cli.pixel);
    if want_tui {
        let path = match cli.file {
            Some(p) if p.as_os_str() != "-" => p,
            _ => bail!("interactive watch mode requires a file path"),
        };
        let use_pixel = if cli.ansi {
            false
        } else if cli.pixel {
            true
        } else {
            caps.supports_kitty_graphics()
        };
        return tui::run_viewer(path, theme_preset, use_pixel, cli.font_size);
    }

    // Mode 4: One-shot stdout rendering (Pixel inline or Rich ANSI)
    let (md, base_dir) = read_input(cli.file.as_ref())?;
    let doc = parse_markdown(&md);

    let want_pixel_oneshot = cli.pixel || (!cli.ansi && stdout_is_tty && caps.supports_kitty_graphics());
    if want_pixel_oneshot && !cli.ansi {
        let mut opts = PixelOptions::new(cli.pixel_width, theme);
        opts.font_size = cli.font_size;
        opts.base_dir = base_dir;
        let rendered = render_pixels(&doc, &opts)?;
        let png = rendered.to_png()?;
        let seq = encode_png_inline(&png, None, None);
        let mut out = io::stdout().lock();
        out.write_all(seq.as_bytes())?;
        out.write_all(b"\n")?;
        out.flush()?;
    } else {
        let cols = cli.width.unwrap_or_else(|| {
            crossterm::terminal::size()
                .map(|(w, _)| (w as usize).saturating_sub(2).clamp(40, 140))
                .unwrap_or(100)
        });
        let ansi = render_ansi(&doc, &AnsiOptions::new(cols, theme, caps));
        let mut out = io::stdout().lock();
        out.write_all(ansi.as_bytes())?;
        out.write_all(b"\n")?;
        out.flush()?;
    }

    Ok(())
}

fn read_input(file: Option<&PathBuf>) -> Result<(String, Option<PathBuf>)> {
    match file {
        Some(p) if p.as_os_str() != "-" => {
            let content = std::fs::read_to_string(p)
                .with_context(|| format!("failed to read {}", p.display()))?;
            let base = p.parent().map(|d| d.to_path_buf());
            Ok((content, base))
        }
        _ => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            Ok((buf, std::env::current_dir().ok()))
        }
    }
}

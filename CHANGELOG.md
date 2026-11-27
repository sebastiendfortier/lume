# Changelog

All notable changes to `lume` are documented in this file.

## [0.1.0] - 2026-11-27

### Added
- **Workspace architecture**: `lume-core` reusable Rust library crate + `lume` static CLI/TUI binary (`x86_64-unknown-linux-musl`, zero runtime dependencies).
- **CPU Pixel Renderer (`lume-core`)**:
  - Real proportional typography (`Liberation Sans` + `DejaVu Sans Mono` bundled statically via `include_bytes!`) using `cosmic-text` + `tiny-skia` + `resvg`.
  - GitHub-grade `H1`–`H6` heading hierarchy with bottom dividers, inline code, links, bold/italic/strikethrough, GFM tables with alignment & zebra rows, checkboxes, blockquotes, and GitHub Alerts (`[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`).
  - `syntect` syntax-highlighted code cards with language header bars.
  - Pure-Rust **Mermaid diagram renderer** (flowcharts `TD`/`LR`, sequence diagrams, pie charts) rasterized via `resvg` with zero Node/Chromium.
  - Pure-Rust **LaTeX math** formatter (`$...$` and `$$...$$`) and local image embedding (`.png`, `.jpg`, `.webp`, `.gif`, `.svg`).
  - 4 built-in themes: `github-dark`, `github-light`, `vscode-dark`, `catppuccin-mocha`.
- **Rich ANSI Renderer (`--ansi`)**: 24-bit truecolor ANSI output with `OSC 8` clickable hyperlinks and automatic **Kitty `OSC 66` scaled headings** when running inside Kitty.
- **Interactive Watch Viewer (`lume file.md` / `lume -w`)**: smooth pixel scrolling, live reload on save (`notify`), toggleable TOC sidebar (`t`), heading search (`/`), zoom (`+`/`-`), theme switcher (`T`), and Pixel ↔ ANSI toggle (`g`).
- **Two-Pane Split Editor (`lume --edit file.md`)**: Markdown source editor on the left (`Ctrl+S`, `Ctrl+Z`/`Ctrl+Y`, mouse click/scroll) + live scroll-synced preview on the right (`F2` toggles preview-only).
- **Integrations & CI**: Yazi pixel previewer plugin (`integrations/yazi/lume.yazi`), `agent-tui` Markdown preview integration, GitHub Actions CI + multi-platform static release workflow, and GNU LGPL-3.0-or-later licensing.

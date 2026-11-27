<p align="center">
  <img src="docs/lume-logo.png" alt="lume logo" width="220">
</p>

# lume

[![CI](https://github.com/sebastiendfortier/lume/actions/workflows/ci.yml/badge.svg)](https://github.com/sebastiendfortier/lume/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/sebastiendfortier/lume)](https://github.com/sebastiendfortier/lume/releases)
[![License: LGPL-3.0-or-later](https://img.shields.io/badge/license-LGPL--3.0--or--later-blue.svg)](LICENSE)

Pixel-rendered & ANSI **zero-dependency static CLI/TUI** for viewing and editing Markdown files in the terminal—bringing GitHub / VS Code / Cursor-grade rendering directly into **WezTerm**, **Kitty**, and **Ghostty** with **zero GPU, zero Electron, and zero Node/Chromium**.

Designed for modern terminal workflows on local workstations, SSH sessions, and headless HPC nodes:
- **Pixel-Rendered Mode (default in WezTerm / Kitty / Ghostty)**: CPU-rasterizes the full document with bundled proportional (`Liberation Sans`) and monospace (`DejaVu Sans Mono`) fonts via `cosmic-text` + `tiny-skia` + `resvg`, then blits crisp tiles over the **Kitty Graphics Protocol**—delivering real heading sizes (`H1`–`H6` with GitHub dividers), `syntect` syntax-highlighted code cards, GFM tables with alignment & zebra rows, checkboxes, GitHub Alerts (`[!NOTE]`, `[!TIP]`, `[!IMPORTANT]`, `[!WARNING]`, `[!CAUTION]`), local images (`.png`/`.jpg`/`.webp`/`.svg`), pure-Rust **Mermaid diagrams**, and **LaTeX math**.
- **Rich ANSI Mode (`--ansi`)**: 24-bit truecolor ANSI output with `OSC 8` clickable hyperlinks, Unicode box-drawing tables, and automatic **Kitty `OSC 66` scaled headings** when running inside Kitty.
- **Interactive Watch Viewer (`lume file.md` / `lume -w`)**: Smooth pixel scrolling, real-time live reload on disk saves (`notify`), toggleable TOC sidebar (`t`), heading search (`/`), theme switcher (`T`), zoom (`+`/`-`), and live Pixel ↔ ANSI toggle (`g`).
- **Two-Pane Split Editor (`lume --edit file.md`)**: Markdown source editor on the left (`Ctrl+S`, `Ctrl+Z`/`Ctrl+Y`, mouse click/scroll) + **live scroll-synced preview** on the right (`F2` toggles preview-only).
- **Ecosystem Ready**: Ships with a **Yazi** pixel previewer plugin (`integrations/yazi/lume.yazi`) and a reusable `lume-core` Rust crate.

---

## Installation

### Prebuilt binaries (GitHub Releases)
Download the latest archive from [Releases](https://github.com/sebastiendfortier/lume/releases) (Linux static `musl` x86_64/aarch64, macOS arm64/x86_64, Windows static CRT x86_64) and verify with `SHA256SUMS.txt`.

### From source (Cargo)
```bash
cargo install --git https://github.com/sebastiendfortier/lume.git lume-cli
```

---

## Quick Start

```bash
# 1. Interactive pixel viewer with live watch (default on TTY in WezTerm/Kitty/Ghostty)
lume README.md

# 2. Two-pane Markdown editor (left) + live scroll-synced preview (right)
lume --edit README.md

# 3. One-shot inline pixel render (no alternate screen)
lume --pixel README.md

# 4. One-shot ANSI render (drop-in glow replacement / pipes)
lume --ansi README.md
cat README.md | lume --ansi

# 5. Export crisp PNG preview (used by Yazi plugin or scripts)
lume --render-png preview.png --pixel-width 960 --max-height 1600 README.md

# 6. Pick a theme: github-dark (default), github-light, vscode-dark, catppuccin-mocha
lume --theme vscode-dark README.md
```

---

## Keyboard Shortcuts

### Interactive Watch Viewer (`lume file.md`)
| Key | Action |
| :--- | :--- |
| `j` / `k` / `↑` / `↓` / Mouse Wheel | Scroll smoothly |
| `PgUp` / `PgDn` / `Space` / `Home` / `G` | Page scroll / jump top & bottom |
| `t` · `Tab` / `Enter` | Toggle Table of Contents sidebar · jump to heading |
| `/` | Search headings |
| `g` | Toggle **Pixel** ↔ **ANSI** mode live |
| `T` | Cycle theme (`github-dark` → `vscode-dark` → `catppuccin-mocha` → `github-light`) |
| `+` / `-` | Increase / decrease font size |
| `e` | Switch directly into two-pane `--edit` mode |
| `r` | Manual reload from disk |
| `q` / `Esc` | Quit |

### Two-Pane Split Editor (`lume --edit file.md`)
| Key | Action |
| :--- | :--- |
| `Ctrl+S` | Save file |
| `Ctrl+Z` / `Ctrl+Y` | Undo / Redo |
| `F2` | Toggle **2-Pane Edit** ↔ **Preview-Only** |
| `Ctrl+L` | Toggle live cursor-to-preview scroll sync |
| `Ctrl+G` | Toggle **Pixel** ↔ **ANSI** preview |
| `Ctrl+T` | Cycle theme |
| `Ctrl+Q` | Quit |

---

## Integrations

### Yazi Pixel Previewer
Copy `integrations/yazi/lume.yazi` to `~/.config/yazi/plugins/lume.yazi` and add to `~/.config/yazi/yazi.toml`:
```toml
[plugin]
prepend_previewers = [
  { url = "*.md", run = "lume" },
  { url = "*.markdown", run = "lume" },
]
```

### `agent-tui`
`agent-tui` automatically detects `lume` on `PATH` and uses `lume --ansi` to render Markdown file previews in its right-hand Files pane.

---

## Documentation

- [Architecture & Rendering Pipeline](docs/architecture.md)
- [Changelog](CHANGELOG.md)
- [Contributing](CONTRIBUTING.md)
- [Security Policy](SECURITY.md)

## License

Licensed under the [GNU Lesser General Public License v3.0 or later (LGPL-3.0-or-later)](LICENSE). See [LICENSE](LICENSE) and [COPYING](COPYING).

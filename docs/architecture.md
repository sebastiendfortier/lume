# `lume` Architecture

`lume` is split into two crates in a Cargo workspace:

* **`lume-core`** — Pure-Rust, zero-C-dependency Markdown parsing, layout, CPU rasterization, Kitty Graphics encoding, and ANSI rendering library.
* **`lume-cli`** — Static CLI & TUI binary (`lume`) providing one-shot rendering, PNG export, interactive live-watch viewing, and a two-pane split editor.

## Rendering Pipeline

```
Markdown source
   │
   ▼
pulldown-cmark (GFM + tables + tasklists + math + source byte offsets)
   │
   ▼
lume_core::ast::Document  (blocks + TOC + GitHub Alert promotion)
   ├──▶ Pixel Backend (default on WezTerm / Kitty / Ghostty)
   │       ├─ cosmic-text (bundled Liberation Sans + DejaVu Sans Mono)
   │       ├─ syntect (pure-Rust fancy-regex syntax highlighting)
   │       ├─ mermaid.rs + resvg (pure-Rust Mermaid -> SVG -> RGBA)
   │       ├─ math.rs (LaTeX -> Unicode math typography)
   │       └─ tiny-skia (rounded cards, borders, checkboxes, compositing)
   │            └──▶ RenderedDocument { rgba, line_map, heading_ys }
   │                   ├── crop_png(scroll_y, view_h) -> Kitty Graphics Protocol
   │                   └── to_png() -> --render-png (Yazi image previewer)
   │
   └──▶ ANSI Backend (--ansi / pipes / agent-tui)
           ├─ 24-bit truecolor + Unicode box-drawing tables & code frames
           ├─ OSC 8 clickable hyperlinks
           └─ OSC 66 scaled H1/H2 headings (auto-enabled when Kitty detected)
```

## Scroll-Sync & Live Watch

* Every AST block records its 1-based `source_line` from `pulldown-cmark` byte offsets.
* Pass 1 of `render_pixels` records `(source_line, pixel_y)` pairs (`line_map`) and heading coordinates (`heading_ys`).
* In `--edit` mode, moving the cursor on the left smoothly aligns the right-hand pixel viewport to `rendered.y_for_source_line(cursor_line)`.
* In `-w` / viewer mode, `notify` watches the parent directory (handling atomic editor saves from `fresh`, `nvim`, `vim`, VS Code, etc.) and re-renders on change.

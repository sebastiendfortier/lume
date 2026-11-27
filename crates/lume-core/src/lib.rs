pub mod ansi;
pub mod ast;
pub mod highlight;
pub mod kitty;
pub mod math;
pub mod mermaid;
pub mod parser;
pub mod pixel;
pub mod theme;

pub use ansi::{render_ansi, AnsiOptions};
pub use ast::{AlertKind, Block, BlockKind, Document, TocEntry};
pub use kitty::TerminalCaps;
pub use parser::parse_markdown;
pub use pixel::{render_pixels, PixelOptions, RenderedDocument};
pub use theme::{Rgb, Theme, ThemePreset};

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_MD: &str = r#"# Lume Demo

A paragraph with **bold**, *italic*, `inline_code`, and a [link](https://example.com).

> [!TIP]
> Pixel-rendered Markdown works natively in WezTerm with zero GPU!

## Code & Math

```rust
fn main() {
    println!("Hello, lume!");
}
```

$$E = mc^2 + \sum_{i=1}^{n} \alpha_i$$

## Diagram

```mermaid
flowchart LR
    A[Markdown] --> B(lume-core)
    B --> C{Backend}
    C -->|WezTerm| D[Pixel PNG]
    C -->|Pipe| E[ANSI]
```

## Table & Checklist

| Feature | Pixel | ANSI |
|---|:---:|---:|
| Proportional Fonts | Yes | No |
| Mermaid SVG | Yes | Fallback |

- [x] Static musl binary
- [ ] Take over the world
"#;

    #[test]
    fn parses_gfm_alerts_mermaid_math_and_toc() {
        let doc = parse_markdown(SAMPLE_MD);
        assert_eq!(doc.toc.len(), 4);
        assert_eq!(doc.toc[0].title, "Lume Demo");
        assert!(doc
            .blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::Alert { kind: AlertKind::Tip, .. })));
        assert!(doc
            .blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::Mermaid { .. })));
        assert!(doc
            .blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::DisplayMath { .. })));
    }

    #[test]
    fn renders_pixels_to_valid_png() {
        let doc = parse_markdown(SAMPLE_MD);
        let opts = PixelOptions::new(900, ThemePreset::GithubDark.theme());
        let rendered = render_pixels(&doc, &opts).expect("pixel render failed");
        assert_eq!(rendered.width, 900);
        assert!(rendered.height > 400);
        assert!(!rendered.heading_ys.is_empty());
        let png = rendered.to_png().expect("png encode failed");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let crop = rendered
            .crop_png(0, 300, opts.theme.bg)
            .expect("crop png failed");
        assert_eq!(&crop[..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn renders_ansi_cleanly() {
        let doc = parse_markdown(SAMPLE_MD);
        let ansi = render_ansi(
            &doc,
            &AnsiOptions::new(80, ThemePreset::GithubDark.theme(), TerminalCaps::Kitty),
        );
        assert!(ansi.contains("Lume Demo"));
        assert!(ansi.contains("Tip"));
        assert!(ansi.contains("Proportional Fonts"));
        assert!(ansi.contains("\x1b]66;s=2;")); // OSC 66 in Kitty
    }
}

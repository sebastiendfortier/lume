use crate::ast::{AlertKind, Block, BlockKind, ColAlign, Document, InlineSpan, TocEntry};
use crate::highlight::highlight_code;
use crate::math::latex_to_unicode;
use crate::mermaid::render_mermaid_svg;
use crate::theme::{Rgb, Theme};
use anyhow::{anyhow, Result};
use cosmic_text::{
    Attrs, Buffer, Color as CosmicColor, Family, FontSystem, Metrics, Shaping, Style as FontStyle,
    SwashCache, Weight, Wrap,
};
use image::codecs::png::PngEncoder;
use image::{ColorType, ImageEncoder};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

static SANS_REG: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Regular.ttf");
static SANS_BOLD: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Bold.ttf");
static SANS_ITAL: &[u8] = include_bytes!("../assets/fonts/LiberationSans-Italic.ttf");
static SANS_BOLD_ITAL: &[u8] = include_bytes!("../assets/fonts/LiberationSans-BoldItalic.ttf");
static MONO_REG: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono.ttf");
static MONO_BOLD: &[u8] = include_bytes!("../assets/fonts/DejaVuSansMono-Bold.ttf");

const SANS_FAMILY: &str = "Liberation Sans";
const MONO_FAMILY: &str = "DejaVu Sans Mono";

static RESVG_FONTDB: OnceLock<resvg::usvg::fontdb::Database> = OnceLock::new();

fn svg_fontdb() -> &'static resvg::usvg::fontdb::Database {
    RESVG_FONTDB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        for data in [SANS_REG, SANS_BOLD, SANS_ITAL, SANS_BOLD_ITAL, MONO_REG, MONO_BOLD] {
            db.load_font_data(data.to_vec());
        }
        db
    })
}

fn new_font_system() -> FontSystem {
    let mut db = cosmic_text::fontdb::Database::new();
    for data in [SANS_REG, SANS_BOLD, SANS_ITAL, SANS_BOLD_ITAL, MONO_REG, MONO_BOLD] {
        db.load_font_data(data.to_vec());
    }
    FontSystem::new_with_locale_and_db("en-US".to_string(), db)
}

#[derive(Debug, Clone)]
pub struct PixelOptions {
    /// Output canvas width in pixels.
    pub width: u32,
    /// Horizontal outer padding in pixels.
    pub padding_x: u32,
    /// Vertical outer padding in pixels.
    pub padding_y: u32,
    /// Base body font size in pixels (default: 15.0).
    pub font_size: f32,
    /// Base directory for resolving relative image paths.
    pub base_dir: Option<PathBuf>,
    /// Theme palette.
    pub theme: Theme,
}

impl PixelOptions {
    pub fn new(width: u32, theme: Theme) -> Self {
        Self {
            width: width.clamp(320, 2400),
            padding_x: 28,
            padding_y: 24,
            font_size: 15.0,
            base_dir: None,
            theme,
        }
    }
}

/// A fully rasterized Markdown document in premultiplied-free straight RGBA8 pixels,
/// along with source-line and TOC Y-coordinate mappings for live scroll-sync.
pub struct RenderedDocument {
    pub width: u32,
    pub height: u32,
    /// Straight RGBA8 pixels (`width * height * 4`).
    pub rgba: Vec<u8>,
    /// `(source_line_1based, pixel_y)` pairs sorted by source line.
    pub line_map: Vec<(usize, u32)>,
    /// `(TocEntry, pixel_y)` for TOC navigation.
    pub heading_ys: Vec<(TocEntry, u32)>,
}

impl RenderedDocument {
    /// Encode the entire rendered document as PNG bytes.
    pub fn to_png(&self) -> Result<Vec<u8>> {
        encode_rgba_to_png(&self.rgba, self.width, self.height)
    }

    /// Encode a vertical viewport slice `[scroll_y .. scroll_y + view_height]` as PNG bytes.
    /// If the document is shorter than `view_height`, pads the bottom with `bg`.
    pub fn crop_png(&self, scroll_y: u32, view_height: u32, bg: Rgb) -> Result<Vec<u8>> {
        let vh = view_height.max(1);
        let row_bytes = (self.width as usize) * 4;
        let mut slice = vec![0u8; row_bytes * (vh as usize)];
        let bg_px = bg.to_rgba(255);

        for dst_y in 0..vh {
            let src_y = scroll_y.saturating_add(dst_y);
            let dst_off = (dst_y as usize) * row_bytes;
            if src_y < self.height {
                let src_off = (src_y as usize) * row_bytes;
                slice[dst_off..dst_off + row_bytes]
                    .copy_from_slice(&self.rgba[src_off..src_off + row_bytes]);
            } else {
                for px in slice[dst_off..dst_off + row_bytes].as_chunks_mut::<4>().0 {
                    *px = bg_px;
                }
            }
        }

        encode_rgba_to_png(&slice, self.width, vh)
    }

    /// Map a 1-based Markdown source line to its approximate pixel Y coordinate.
    pub fn y_for_source_line(&self, line: usize) -> u32 {
        if self.line_map.is_empty() {
            return 0;
        }
        let mut best = 0u32;
        for &(src_line, y) in &self.line_map {
            if src_line <= line {
                best = y;
            } else {
                break;
            }
        }
        best
    }
}

fn encode_rgba_to_png(rgba: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let encoder = PngEncoder::new(&mut out);
    encoder.write_image(rgba, width, height, ColorType::Rgba8.into())?;
    Ok(out)
}

enum DrawCmd {
    FillRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Rgb,
        alpha: u8,
    },
    FillRoundedRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        color: Rgb,
        alpha: u8,
    },
    StrokeRoundedRect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        width: f32,
        color: Rgb,
    },
    Checkmark {
        x: f32,
        y: f32,
        size: f32,
        color: Rgb,
    },
    Text {
        x: f32,
        y: f32,
        buffer: Buffer,
        default_color: Rgb,
    },
    Image {
        x: i32,
        y: i32,
        w: u32,
        h: u32,
        rgba: Vec<u8>,
    },
}

struct Layouter<'a> {
    font_sys: FontSystem,
    opts: &'a PixelOptions,
    cmds: Vec<DrawCmd>,
    line_map: Vec<(usize, u32)>,
    heading_ys: Vec<(TocEntry, u32)>,
}

/// Render a parsed `Document` to crisp CPU-rasterized RGBA pixels.
pub fn render_pixels(doc: &Document, opts: &PixelOptions) -> Result<RenderedDocument> {
    let mut layouter = Layouter {
        font_sys: new_font_system(),
        opts,
        cmds: Vec::new(),
        line_map: Vec::new(),
        heading_ys: Vec::new(),
    };

    let content_x = opts.padding_x as f32;
    let content_w = (opts.width as f32 - (opts.padding_x * 2) as f32).max(160.0);
    let mut y = opts.padding_y as f32;

    for (bi, blk) in doc.blocks.iter().enumerate() {
        let start_y = y as u32;
        layouter.line_map.push((blk.source_line, start_y));
        if let BlockKind::Heading { level, plain, .. } = &blk.kind {
            layouter.heading_ys.push((
                TocEntry {
                    level: *level,
                    title: plain.clone(),
                    source_line: blk.source_line,
                    block_index: bi,
                },
                start_y,
            ));
        }
        y = layouter.layout_block(blk, content_x, y, content_w, 0);
    }

    let total_h = ((y + opts.padding_y as f32).ceil() as u32).clamp(64, 24000);
    let mut pixmap =
        Pixmap::new(opts.width, total_h).ok_or_else(|| anyhow!("failed to allocate pixmap"))?;
    pixmap.fill(opts.theme.bg.to_skia());

    let mut swash_cache = SwashCache::new();
    for cmd in &layouter.cmds {
        execute_cmd(&mut pixmap, &mut layouter.font_sys, &mut swash_cache, cmd);
    }

    // Convert premultiplied tiny-skia pixels to straight RGBA8 for PNG / image ops
    let premul = pixmap.data();
    let mut rgba = vec![0u8; premul.len()];
    for (src, dst) in premul
        .as_chunks::<4>()
        .0
        .iter()
        .zip(rgba.as_chunks_mut::<4>().0.iter_mut())
    {
        let a = src[3];
        if a == 255 || a == 0 {
            *dst = *src;
        } else {
            let af = a as f32 / 255.0;
            dst[0] = ((src[0] as f32 / af).min(255.0)) as u8;
            dst[1] = ((src[1] as f32 / af).min(255.0)) as u8;
            dst[2] = ((src[2] as f32 / af).min(255.0)) as u8;
            dst[3] = a;
        }
    }

    Ok(RenderedDocument {
        width: opts.width,
        height: total_h,
        rgba,
        line_map: layouter.line_map,
        heading_ys: layouter.heading_ys,
    })
}

impl<'a> Layouter<'a> {
    fn theme(&self) -> Theme {
        self.opts.theme
    }

    fn layout_block(&mut self, blk: &Block, x: f32, mut y: f32, w: f32, depth: usize) -> f32 {
        let t = self.theme();
        let base = self.opts.font_size;

        match &blk.kind {
            BlockKind::Heading { level, spans, .. } => {
                if y > self.opts.padding_y as f32 + 4.0 {
                    y += match level {
                        1 => 14.0,
                        2 => 12.0,
                        _ => 8.0,
                    };
                }
                let size = match level {
                    1 => base * 1.85,
                    2 => base * 1.45,
                    3 => base * 1.22,
                    4 => base * 1.08,
                    5 => base * 0.98,
                    _ => base * 0.92,
                };
                let lh = size * 1.32;
                let col = if *level >= 5 { t.text_muted } else { t.heading };
                let (buf, h) = self.shape_inlines(spans, w, size, lh, col, true, false);
                self.cmds.push(DrawCmd::Text {
                    x,
                    y,
                    buffer: buf,
                    default_color: col,
                });
                y += h;
                if *level <= 2 {
                    y += 5.0;
                    self.cmds.push(DrawCmd::FillRect {
                        x,
                        y,
                        w,
                        h: 1.0,
                        color: t.hr,
                        alpha: 255,
                    });
                    y += 11.0;
                } else {
                    y += 8.0;
                }
                y
            }
            BlockKind::Paragraph { spans } => {
                let lh = base * 1.52;
                let (buf, h) = self.shape_inlines(spans, w, base, lh, t.text, false, false);
                self.cmds.push(DrawCmd::Text {
                    x,
                    y,
                    buffer: buf,
                    default_color: t.text,
                });
                y + h + 11.0
            }
            BlockKind::CodeBlock { lang, code } => {
                let code_size = (base * 0.88).max(12.0);
                let code_lh = code_size * 1.45;
                let pad_x = 14.0;
                let pad_y = 11.0;
                let header_h = if lang.is_empty() { 0.0 } else { 26.0 };

                let highlighted = highlight_code(code, lang, &t);
                let mut rich_spans: Vec<(String, Attrs<'static>)> = Vec::new();
                for (li, line_tokens) in highlighted.iter().enumerate() {
                    if li > 0 {
                        rich_spans.push(("\n".to_string(), Attrs::new().family(Family::Name(MONO_FAMILY))));
                    }
                    if line_tokens.is_empty() {
                        rich_spans.push((" ".to_string(), Attrs::new().family(Family::Name(MONO_FAMILY))));
                    } else {
                        for tok in line_tokens {
                            let mut attrs = Attrs::new()
                                .family(Family::Name(MONO_FAMILY))
                                .color(tok.fg.to_cosmic());
                            if tok.bold {
                                attrs = attrs.weight(Weight::BOLD);
                            }
                            if tok.italic {
                                attrs = attrs.style(FontStyle::Italic);
                            }
                            rich_spans.push((tok.text.clone(), attrs));
                        }
                    }
                }

                let (buf, text_h) = self.shape_rich(
                    &rich_spans,
                    (w - pad_x * 2.0).max(80.0),
                    code_size,
                    code_lh,
                    t.text,
                );
                let box_h = header_h + text_h + pad_y * 2.0;

                self.cmds.push(DrawCmd::FillRoundedRect {
                    x,
                    y,
                    w,
                    h: box_h,
                    r: 7.0,
                    color: t.code_bg,
                    alpha: 255,
                });
                if !lang.is_empty() {
                    self.cmds.push(DrawCmd::FillRoundedRect {
                        x,
                        y,
                        w,
                        h: header_h,
                        r: 7.0,
                        color: t.code_header_bg,
                        alpha: 255,
                    });
                    // Square off bottom of header bar
                    self.cmds.push(DrawCmd::FillRect {
                        x,
                        y: y + header_h - 6.0,
                        w,
                        h: 6.0,
                        color: t.code_header_bg,
                        alpha: 255,
                    });
                    self.cmds.push(DrawCmd::FillRect {
                        x,
                        y: y + header_h,
                        w,
                        h: 1.0,
                        color: t.border,
                        alpha: 255,
                    });
                    let (hbuf, _) = self.shape_plain(
                        &lang.to_uppercase(),
                        w - pad_x * 2.0,
                        11.0,
                        16.0,
                        t.text_muted,
                        true,
                        true,
                    );
                    self.cmds.push(DrawCmd::Text {
                        x: x + pad_x,
                        y: y + 5.0,
                        buffer: hbuf,
                        default_color: t.text_muted,
                    });
                }
                self.cmds.push(DrawCmd::StrokeRoundedRect {
                    x,
                    y,
                    w,
                    h: box_h,
                    r: 7.0,
                    width: 1.0,
                    color: t.border,
                });
                self.cmds.push(DrawCmd::Text {
                    x: x + pad_x,
                    y: y + header_h + pad_y,
                    buffer: buf,
                    default_color: t.text,
                });
                y + box_h + 13.0
            }
            BlockKind::Mermaid { code } => {
                let svg = render_mermaid_svg(code, &t, w);
                if let Some((iw, ih, rgba)) = rasterize_svg(&svg, w) {
                    let draw_x = x + ((w - iw as f32).max(0.0) * 0.5);
                    self.cmds.push(DrawCmd::Image {
                        x: draw_x as i32,
                        y: y as i32,
                        w: iw,
                        h: ih,
                        rgba,
                    });
                    return y + ih as f32 + 14.0;
                }
                y + 8.0
            }
            BlockKind::DisplayMath { latex } => {
                let formatted = latex_to_unicode(latex);
                let math_size = base * 1.15;
                let math_lh = math_size * 1.45;
                let pad_x = 18.0;
                let pad_y = 12.0;
                let (buf, text_h) = self.shape_plain(
                    &formatted,
                    (w - pad_x * 2.0).max(80.0),
                    math_size,
                    math_lh,
                    t.heading,
                    false,
                    false,
                );
                let box_h = text_h + pad_y * 2.0;
                self.cmds.push(DrawCmd::FillRoundedRect {
                    x,
                    y,
                    w,
                    h: box_h,
                    r: 7.0,
                    color: t.surface,
                    alpha: 255,
                });
                self.cmds.push(DrawCmd::StrokeRoundedRect {
                    x,
                    y,
                    w,
                    h: box_h,
                    r: 7.0,
                    width: 1.0,
                    color: t.border,
                });
                self.cmds.push(DrawCmd::Text {
                    x: x + pad_x,
                    y: y + pad_y,
                    buffer: buf,
                    default_color: t.heading,
                });
                y + box_h + 13.0
            }
            BlockKind::BlockQuote { blocks } => {
                let bar_w = 4.0;
                let indent = 16.0;
                let start_y = y;
                let mut cy = y + 2.0;
                for inner in blocks {
                    cy = self.layout_block(inner, x + indent, cy, (w - indent).max(80.0), depth + 1);
                }
                let h = (cy - start_y - 6.0).max(16.0);
                self.cmds.push(DrawCmd::FillRoundedRect {
                    x,
                    y: start_y,
                    w: bar_w,
                    h,
                    r: 2.0,
                    color: t.quote_bar,
                    alpha: 255,
                });
                cy + 4.0
            }
            BlockKind::Alert { kind, blocks } => {
                let accent = match kind {
                    AlertKind::Note => t.accent_note,
                    AlertKind::Tip => t.accent_tip,
                    AlertKind::Important => t.accent_important,
                    AlertKind::Warning => t.accent_warning,
                    AlertKind::Caution => t.accent_caution,
                };
                let pad_x = 16.0;
                let pad_y = 10.0;
                let start_y = y;

                // Reserve space for background card by recording index
                let bg_idx = self.cmds.len();
                self.cmds.push(DrawCmd::FillRect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                    color: accent,
                    alpha: 0,
                });
                let bar_idx = self.cmds.len();
                self.cmds.push(DrawCmd::FillRect {
                    x: 0.0,
                    y: 0.0,
                    w: 0.0,
                    h: 0.0,
                    color: accent,
                    alpha: 0,
                });

                let title = format!("[{}] {}", kind.icon().to_uppercase(), kind.label());
                let (tbuf, th) = self.shape_plain(
                    &title,
                    (w - pad_x * 2.0).max(80.0),
                    base * 0.96,
                    base * 1.35,
                    accent,
                    true,
                    false,
                );
                self.cmds.push(DrawCmd::Text {
                    x: x + pad_x,
                    y: y + pad_y,
                    buffer: tbuf,
                    default_color: accent,
                });
                let mut cy = y + pad_y + th + 6.0;
                for inner in blocks {
                    cy = self.layout_block(
                        inner,
                        x + pad_x,
                        cy,
                        (w - pad_x * 2.0).max(80.0),
                        depth + 1,
                    );
                }
                let card_h = (cy - start_y).max(38.0);
                self.cmds[bg_idx] = DrawCmd::FillRoundedRect {
                    x,
                    y: start_y,
                    w,
                    h: card_h,
                    r: 6.0,
                    color: accent,
                    alpha: if t.is_dark { 24 } else { 20 },
                };
                self.cmds[bar_idx] = DrawCmd::FillRoundedRect {
                    x,
                    y: start_y,
                    w: 4.0,
                    h: card_h,
                    r: 2.0,
                    color: accent,
                    alpha: 255,
                };
                start_y + card_h + 13.0
            }
            BlockKind::List {
                ordered_start,
                items,
            } => {
                let indent = 24.0;
                let mut cy = y;
                for (i, item) in items.iter().enumerate() {
                    if let Some(checked) = item.checked {
                        let bx = x + 3.0;
                        let by = cy + 3.0;
                        let sz = 14.0;
                        if checked {
                            self.cmds.push(DrawCmd::FillRoundedRect {
                                x: bx,
                                y: by,
                                w: sz,
                                h: sz,
                                r: 3.0,
                                color: t.check_done,
                                alpha: 255,
                            });
                            self.cmds.push(DrawCmd::Checkmark {
                                x: bx,
                                y: by,
                                size: sz,
                                color: t.bg,
                            });
                        } else {
                            self.cmds.push(DrawCmd::StrokeRoundedRect {
                                x: bx,
                                y: by,
                                w: sz,
                                h: sz,
                                r: 3.0,
                                width: 1.5,
                                color: t.text_muted,
                            });
                        }
                    } else {
                        let marker = if let Some(start) = ordered_start {
                            format!("{}.", start + i as u64)
                        } else {
                            match depth % 3 {
                                0 => "•".to_string(),
                                1 => "◦".to_string(),
                                _ => "▪".to_string(),
                            }
                        };
                        let (mbuf, _) = self.shape_plain(
                            &marker,
                            indent - 4.0,
                            base,
                            base * 1.5,
                            t.text_muted,
                            false,
                            false,
                        );
                        self.cmds.push(DrawCmd::Text {
                            x: x + 4.0,
                            y: cy,
                            buffer: mbuf,
                            default_color: t.text_muted,
                        });
                    }

                    for inner in &item.blocks {
                        cy = self.layout_block(
                            inner,
                            x + indent,
                            cy,
                            (w - indent).max(80.0),
                            depth + 1,
                        );
                    }
                    cy = (cy - 4.0).max(y);
                }
                cy + 8.0
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
                let col_w = w / ncols as f32;
                let pad_x = 10.0;
                let pad_y = 8.0;
                let cell_size = (base * 0.92).max(12.5);
                let cell_lh = cell_size * 1.42;

                let table_start_y = y;

                // Header row
                if !headers.is_empty() {
                    let mut shaped_cells = Vec::new();
                    let mut row_h = cell_lh;
                    for ci in 0..ncols {
                        let spans = headers.get(ci).map(Vec::as_slice).unwrap_or(&[]);
                        let (buf, h) = self.shape_inlines(
                            spans,
                            (col_w - pad_x * 2.0).max(40.0),
                            cell_size,
                            cell_lh,
                            t.heading,
                            true,
                            false,
                        );
                        row_h = row_h.max(h);
                        shaped_cells.push(buf);
                    }
                    let total_row_h = row_h + pad_y * 2.0;
                    self.cmds.push(DrawCmd::FillRect {
                        x,
                        y,
                        w,
                        h: total_row_h,
                        color: t.table_header_bg,
                        alpha: 255,
                    });
                    for (ci, buf) in shaped_cells.into_iter().enumerate() {
                        let align = alignments.get(ci).copied().unwrap_or(ColAlign::Left);
                        let cx = align_cell_x(x + ci as f32 * col_w, col_w, pad_x, align, &buf);
                        self.cmds.push(DrawCmd::Text {
                            x: cx,
                            y: y + pad_y,
                            buffer: buf,
                            default_color: t.heading,
                        });
                    }
                    y += total_row_h;
                    self.cmds.push(DrawCmd::FillRect {
                        x,
                        y,
                        w,
                        h: 1.0,
                        color: t.border,
                        alpha: 255,
                    });
                }

                // Body rows
                for (ri, row) in rows.iter().enumerate() {
                    let mut shaped_cells = Vec::new();
                    let mut row_h = cell_lh;
                    for ci in 0..ncols {
                        let spans = row.get(ci).map(Vec::as_slice).unwrap_or(&[]);
                        let (buf, h) = self.shape_inlines(
                            spans,
                            (col_w - pad_x * 2.0).max(40.0),
                            cell_size,
                            cell_lh,
                            t.text,
                            false,
                            false,
                        );
                        row_h = row_h.max(h);
                        shaped_cells.push(buf);
                    }
                    let total_row_h = row_h + pad_y * 2.0;
                    if ri % 2 == 1 {
                        self.cmds.push(DrawCmd::FillRect {
                            x,
                            y,
                            w,
                            h: total_row_h,
                            color: t.table_alt_bg,
                            alpha: 255,
                        });
                    }
                    for (ci, buf) in shaped_cells.into_iter().enumerate() {
                        let align = alignments.get(ci).copied().unwrap_or(ColAlign::Left);
                        let cx = align_cell_x(x + ci as f32 * col_w, col_w, pad_x, align, &buf);
                        self.cmds.push(DrawCmd::Text {
                            x: cx,
                            y: y + pad_y,
                            buffer: buf,
                            default_color: t.text,
                        });
                    }
                    y += total_row_h;
                    if ri + 1 < rows.len() {
                        self.cmds.push(DrawCmd::FillRect {
                            x,
                            y,
                            w,
                            h: 1.0,
                            color: t.border,
                            alpha: 140,
                        });
                    }
                }

                // Column dividers + outer rounded border
                let table_h = (y - table_start_y).max(20.0);
                for ci in 1..ncols {
                    self.cmds.push(DrawCmd::FillRect {
                        x: x + ci as f32 * col_w,
                        y: table_start_y,
                        w: 1.0,
                        h: table_h,
                        color: t.border,
                        alpha: 140,
                    });
                }
                self.cmds.push(DrawCmd::StrokeRoundedRect {
                    x,
                    y: table_start_y,
                    w,
                    h: table_h,
                    r: 6.0,
                    width: 1.0,
                    color: t.border,
                });
                y + 14.0
            }
            BlockKind::Image { url, alt } => {
                if let Some((iw, ih, rgba)) =
                    load_image_rgba(url, self.opts.base_dir.as_deref(), w)
                {
                    let draw_x = x + ((w - iw as f32).max(0.0) * 0.5);
                    self.cmds.push(DrawCmd::Image {
                        x: draw_x as i32,
                        y: y as i32,
                        w: iw,
                        h: ih,
                        rgba,
                    });
                    y += ih as f32 + 6.0;
                    if !alt.is_empty() {
                        let (cbuf, ch) = self.shape_plain(
                            alt,
                            w,
                            base * 0.85,
                            base * 1.25,
                            t.text_muted,
                            false,
                            false,
                        );
                        self.cmds.push(DrawCmd::Text {
                            x: draw_x,
                            y,
                            buffer: cbuf,
                            default_color: t.text_muted,
                        });
                        y += ch + 6.0;
                    }
                    y + 8.0
                } else {
                    let label = if alt.is_empty() {
                        format!("[image: {url}]")
                    } else {
                        format!("[image: {alt} ({url})]")
                    };
                    let (buf, h) =
                        self.shape_plain(&label, w, base * 0.9, base * 1.35, t.text_muted, false, false);
                    self.cmds.push(DrawCmd::Text {
                        x,
                        y,
                        buffer: buf,
                        default_color: t.text_muted,
                    });
                    y + h + 10.0
                }
            }
            BlockKind::Rule => {
                y += 8.0;
                self.cmds.push(DrawCmd::FillRect {
                    x,
                    y,
                    w,
                    h: 2.0,
                    color: t.hr,
                    alpha: 255,
                });
                y + 16.0
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn shape_inlines(
        &mut self,
        spans: &[InlineSpan],
        max_w: f32,
        size: f32,
        line_height: f32,
        base_color: Rgb,
        force_bold: bool,
        force_italic: bool,
    ) -> (Buffer, f32) {
        let t = self.theme();
        let mut rich: Vec<(String, Attrs<'static>)> = Vec::with_capacity(spans.len());
        for sp in spans {
            let mut text = if sp.style.math {
                latex_to_unicode(&sp.text)
            } else {
                sp.text.clone()
            };
            let mut attrs = if sp.style.code {
                text = format!(" {text} ");
                Attrs::new()
                    .family(Family::Name(MONO_FAMILY))
                    .color(t.inline_code_fg.to_cosmic())
            } else {
                let col = if sp.style.link.is_some() {
                    t.link
                } else if sp.style.strikethrough {
                    t.text_muted
                } else {
                    base_color
                };
                Attrs::new()
                    .family(Family::Name(SANS_FAMILY))
                    .color(col.to_cosmic())
            };
            if force_bold || sp.style.bold {
                attrs = attrs.weight(Weight::BOLD);
            }
            if force_italic || sp.style.italic {
                attrs = attrs.style(FontStyle::Italic);
            }
            rich.push((text, attrs));
        }
        self.shape_rich(&rich, max_w, size, line_height, base_color)
    }

    #[allow(clippy::too_many_arguments)]
    fn shape_plain(
        &mut self,
        text: &str,
        max_w: f32,
        size: f32,
        line_height: f32,
        color: Rgb,
        bold: bool,
        mono: bool,
    ) -> (Buffer, f32) {
        let mut attrs = Attrs::new()
            .family(Family::Name(if mono { MONO_FAMILY } else { SANS_FAMILY }))
            .color(color.to_cosmic());
        if bold {
            attrs = attrs.weight(Weight::BOLD);
        }
        self.shape_rich(&[(text.to_string(), attrs)], max_w, size, line_height, color)
    }

    fn shape_rich(
        &mut self,
        spans: &[(String, Attrs<'static>)],
        max_w: f32,
        size: f32,
        line_height: f32,
        default_color: Rgb,
    ) -> (Buffer, f32) {
        let mut buf = Buffer::new(&mut self.font_sys, Metrics::new(size, line_height));
        buf.set_wrap(&mut self.font_sys, Wrap::WordOrGlyph);
        buf.set_size(&mut self.font_sys, Some(max_w.max(40.0)), None);
        let default_attrs = Attrs::new()
            .family(Family::Name(SANS_FAMILY))
            .color(default_color.to_cosmic());
        let borrowed: Vec<(&str, Attrs<'_>)> =
            spans.iter().map(|(s, a)| (s.as_str(), *a)).collect();
        buf.set_rich_text(
            &mut self.font_sys,
            borrowed,
            default_attrs,
            Shaping::Advanced,
            None,
        );
        buf.shape_until_scroll(&mut self.font_sys, false);

        let lines = buf.layout_runs().count().max(1);
        let h = lines as f32 * line_height;
        (buf, h)
    }
}

fn align_cell_x(col_x: f32, col_w: f32, pad_x: f32, align: ColAlign, buf: &Buffer) -> f32 {
    if align == ColAlign::Left {
        return col_x + pad_x;
    }
    let max_line_w = buf
        .layout_runs()
        .map(|r| r.line_w)
        .fold(0.0f32, f32::max);
    let inner_w = (col_w - pad_x * 2.0).max(0.0);
    match align {
        ColAlign::Center => col_x + pad_x + ((inner_w - max_line_w).max(0.0) * 0.5),
        ColAlign::Right => col_x + pad_x + (inner_w - max_line_w).max(0.0),
        ColAlign::Left => col_x + pad_x,
    }
}

fn rasterize_svg(svg_str: &str, max_w: f32) -> Option<(u32, u32, Vec<u8>)> {
    let opt = resvg::usvg::Options {
        fontdb: std::sync::Arc::new(svg_fontdb().clone()),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_str(svg_str, &opt).ok()?;
    let sz = tree.size();
    let scale = if sz.width() > max_w && sz.width() > 0.0 {
        max_w / sz.width()
    } else {
        1.0
    };
    let w = (sz.width() * scale).ceil().max(1.0) as u32;
    let h = (sz.height() * scale).ceil().max(1.0) as u32;
    let mut pixmap = Pixmap::new(w, h)?;
    resvg::render(&tree, Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Some((w, h, pixmap.take()))
}

fn load_image_rgba(url: &str, base_dir: Option<&Path>, max_w: f32) -> Option<(u32, u32, Vec<u8>)> {
    if url.starts_with("http://") || url.starts_with("https://") {
        return None;
    }
    let path = {
        let p = Path::new(url);
        if p.is_absolute() {
            p.to_path_buf()
        } else if let Some(base) = base_dir {
            base.join(p)
        } else {
            p.to_path_buf()
        }
    };
    if !path.is_file() {
        return None;
    }
    if path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("svg"))
        .unwrap_or(false)
    {
        let svg = std::fs::read_to_string(&path).ok()?;
        return rasterize_svg(&svg, max_w);
    }
    let img = image::open(&path).ok()?;
    let (ow, oh) = (img.width(), img.height());
    let max_w_u32 = max_w.max(64.0) as u32;
    let resized = if ow > max_w_u32 {
        let nh = ((oh as f64) * (max_w_u32 as f64 / ow as f64)).round().max(1.0) as u32;
        img.resize_exact(max_w_u32, nh, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let rgba = resized.to_rgba8();
    let (w, h) = rgba.dimensions();
    // Premultiply alpha for fast tiny-skia compositing
    let mut premul = rgba.into_raw();
    for px in premul.as_chunks_mut::<4>().0 {
        let a = px[3] as u16;
        if a < 255 {
            px[0] = ((px[0] as u16 * a) / 255) as u8;
            px[1] = ((px[1] as u16 * a) / 255) as u8;
            px[2] = ((px[2] as u16 * a) / 255) as u8;
        }
    }
    Some((w, h, premul))
}

fn execute_cmd(
    pixmap: &mut Pixmap,
    font_sys: &mut FontSystem,
    swash_cache: &mut SwashCache,
    cmd: &DrawCmd,
) {
    match cmd {
        DrawCmd::FillRect {
            x,
            y,
            w,
            h,
            color,
            alpha,
        } => {
            if *w <= 0.0 || *h <= 0.0 || *alpha == 0 {
                return;
            }
            if let Some(rect) = Rect::from_xywh(
                x.round(),
                y.round(),
                w.round().max(1.0),
                h.round().max(1.0),
            ) {
                let mut paint = Paint::default();
                paint.set_color(color.to_skia_alpha(*alpha));
                paint.anti_alias = false;
                pixmap.fill_rect(rect, &paint, Transform::identity(), None);
            }
        }
        DrawCmd::FillRoundedRect {
            x,
            y,
            w,
            h,
            r,
            color,
            alpha,
        } => {
            if *w <= 0.0 || *h <= 0.0 || *alpha == 0 {
                return;
            }
            if let Some(path) = rounded_rect_path(*x, *y, *w, *h, *r) {
                let mut paint = Paint::default();
                paint.set_color(color.to_skia_alpha(*alpha));
                paint.anti_alias = true;
                pixmap.fill_path(&path, &paint, FillRule::Winding, Transform::identity(), None);
            }
        }
        DrawCmd::StrokeRoundedRect {
            x,
            y,
            w,
            h,
            r,
            width,
            color,
        } => {
            if *w <= 2.0 || *h <= 2.0 {
                return;
            }
            if let Some(path) = rounded_rect_path(*x + 0.5, *y + 0.5, *w - 1.0, *h - 1.0, *r) {
                let mut paint = Paint::default();
                paint.set_color(color.to_skia());
                paint.anti_alias = true;
                let stroke = Stroke {
                    width: (*width).max(1.0),
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
        DrawCmd::Checkmark { x, y, size, color } => {
            let mut pb = PathBuilder::new();
            pb.move_to(x + size * 0.22, y + size * 0.52);
            pb.line_to(x + size * 0.43, y + size * 0.74);
            pb.line_to(x + size * 0.80, y + size * 0.28);
            if let Some(path) = pb.finish() {
                let mut paint = Paint::default();
                paint.set_color(color.to_skia());
                paint.anti_alias = true;
                let stroke = Stroke {
                    width: 2.0,
                    ..Default::default()
                };
                pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
            }
        }
        DrawCmd::Text {
            x,
            y,
            buffer,
            default_color,
        } => {
            let pw = pixmap.width() as i32;
            let ph = pixmap.height() as i32;
            let data = pixmap.data_mut();
            let base_x = *x as i32;
            let base_y = *y as i32;

            buffer.draw(
                font_sys,
                swash_cache,
                default_color.to_cosmic(),
                |gx, gy, _gw, _gh, color: CosmicColor| {
                    let px = base_x + gx;
                    let py = base_y + gy;
                    if px < 0 || py < 0 || px >= pw || py >= ph {
                        return;
                    }
                    let a = color.a() as u32;
                    if a == 0 {
                        return;
                    }
                    let idx = ((py as usize) * (pw as usize) + (px as usize)) * 4;
                    if a == 255 {
                        data[idx] = color.r();
                        data[idx + 1] = color.g();
                        data[idx + 2] = color.b();
                        data[idx + 3] = 255;
                    } else {
                        let inv = 255 - a;
                        data[idx] = ((color.r() as u32 * a + data[idx] as u32 * inv) / 255) as u8;
                        data[idx + 1] =
                            ((color.g() as u32 * a + data[idx + 1] as u32 * inv) / 255) as u8;
                        data[idx + 2] =
                            ((color.b() as u32 * a + data[idx + 2] as u32 * inv) / 255) as u8;
                        data[idx + 3] = 255;
                    }
                },
            );
        }
        DrawCmd::Image { x, y, w, h, rgba } => {
            if let Some(src) = tiny_skia::PixmapRef::from_bytes(rgba, *w, *h) {
                pixmap.draw_pixmap(
                    *x,
                    *y,
                    src,
                    &tiny_skia::PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
        }
    }
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    if !x.is_finite() || !y.is_finite() || !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let r = r.min(w * 0.49).min(h * 0.49).max(0.0);
    if r < 0.5 {
        return Rect::from_xywh(x, y, w, h).map(PathBuilder::from_rect);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

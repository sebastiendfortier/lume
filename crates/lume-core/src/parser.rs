use crate::ast::{
    AlertKind, Block, BlockKind, ColAlign, Document, InlineSpan, InlineStyle, ListItem, TocEntry,
};
use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

/// Parse GitHub-Flavored Markdown (plus math, Mermaid, and GitHub Alerts) into a structured Document.
pub fn parse_markdown(input: &str) -> Document {
    let line_starts = compute_line_starts(input);
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_MATH);
    opts.insert(Options::ENABLE_GFM);

    let parser = Parser::new_ext(input, opts).into_offset_iter();
    let events: Vec<(Event<'_>, std::ops::Range<usize>)> = parser.collect();

    let mut idx = 0;
    let mut blocks = parse_blocks(&events, &mut idx, &line_starts, None);
    promote_github_alerts(&mut blocks);

    let mut toc = Vec::new();
    for (bi, blk) in blocks.iter().enumerate() {
        if let BlockKind::Heading { level, plain, .. } = &blk.kind {
            toc.push(TocEntry {
                level: *level,
                title: plain.clone(),
                source_line: blk.source_line,
                block_index: bi,
            });
        }
    }

    Document { blocks, toc }
}

fn compute_line_starts(input: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (i, b) in input.bytes().enumerate() {
        if b == b'\n' {
            starts.push(i + 1);
        }
    }
    starts
}

fn offset_to_line(line_starts: &[usize], offset: usize) -> usize {
    match line_starts.binary_search(&offset) {
        Ok(i) => i + 1,
        Err(i) => i.max(1),
    }
}

fn parse_blocks(
    events: &[(Event<'_>, std::ops::Range<usize>)],
    idx: &mut usize,
    line_starts: &[usize],
    stop_tag: Option<TagEnd>,
) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut loose_spans: Vec<InlineSpan> = Vec::new();
    let mut loose_line: usize = 1;
    let mut inline_style = InlineStyle::default();

    let flush_loose = |blocks: &mut Vec<Block>, loose: &mut Vec<InlineSpan>, line: usize| {
        if !loose.is_empty() {
            let spans = std::mem::take(loose);
            blocks.push(Block {
                source_line: line,
                kind: BlockKind::Paragraph { spans },
            });
        }
    };

    while *idx < events.len() {
        let (ref ev, ref range) = events[*idx];
        let line = offset_to_line(line_starts, range.start);

        if let Some(stop) = stop_tag {
            if matches!(ev, Event::End(end_tag) if *end_tag == stop) {
                *idx += 1;
                break;
            }
        }

        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                flush_loose(&mut blocks, &mut loose_spans, loose_line);
                let lvl = *level as u8;
                *idx += 1;
                let spans = parse_inlines(events, idx, TagEnd::Heading(*level));
                let plain: String = spans.iter().map(|s| s.text.as_str()).collect();
                blocks.push(Block {
                    source_line: line,
                    kind: BlockKind::Heading {
                        level: lvl,
                        spans,
                        plain,
                    },
                });
            }
            Event::Start(Tag::Paragraph) => {
                flush_loose(&mut blocks, &mut loose_spans, loose_line);
                *idx += 1;
                let (spans, standalone_img, standalone_math) = parse_paragraph_inlines(events, idx);
                if let Some((url, alt)) = standalone_img {
                    blocks.push(Block {
                        source_line: line,
                        kind: BlockKind::Image { url, alt },
                    });
                } else if let Some(latex) = standalone_math {
                    blocks.push(Block {
                        source_line: line,
                        kind: BlockKind::DisplayMath { latex },
                    });
                } else if !spans.is_empty() {
                    blocks.push(Block {
                        source_line: line,
                        kind: BlockKind::Paragraph { spans },
                    });
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush_loose(&mut blocks, &mut loose_spans, loose_line);
                let lang = match kind {
                    CodeBlockKind::Fenced(info) => {
                        info.split_whitespace().next().unwrap_or("").to_string()
                    }
                    CodeBlockKind::Indented => String::new(),
                };
                *idx += 1;
                let mut code = String::new();
                while *idx < events.len() {
                    match &events[*idx].0 {
                        Event::End(TagEnd::CodeBlock) => {
                            *idx += 1;
                            break;
                        }
                        Event::Text(t) | Event::Code(t) => {
                            code.push_str(t);
                            *idx += 1;
                        }
                        _ => {
                            *idx += 1;
                        }
                    }
                }
                if code.ends_with('\n') {
                    code.pop();
                }
                let lang_lower = lang.to_lowercase();
                if lang_lower == "mermaid" {
                    blocks.push(Block {
                        source_line: line,
                        kind: BlockKind::Mermaid { code },
                    });
                } else if lang_lower == "math" || lang_lower == "latex" || lang_lower == "tex" {
                    blocks.push(Block {
                        source_line: line,
                        kind: BlockKind::DisplayMath { latex: code },
                    });
                } else {
                    blocks.push(Block {
                        source_line: line,
                        kind: BlockKind::CodeBlock { lang, code },
                    });
                }
            }
            Event::Start(Tag::BlockQuote(bq_kind)) => {
                flush_loose(&mut blocks, &mut loose_spans, loose_line);
                let explicit_alert = bq_kind.map(|k| match k {
                    pulldown_cmark::BlockQuoteKind::Note => AlertKind::Note,
                    pulldown_cmark::BlockQuoteKind::Tip => AlertKind::Tip,
                    pulldown_cmark::BlockQuoteKind::Important => AlertKind::Important,
                    pulldown_cmark::BlockQuoteKind::Warning => AlertKind::Warning,
                    pulldown_cmark::BlockQuoteKind::Caution => AlertKind::Caution,
                });
                let end_tag = TagEnd::BlockQuote(*bq_kind);
                *idx += 1;
                let inner = parse_blocks(events, idx, line_starts, Some(end_tag));
                if let Some(alert_kind) = explicit_alert {
                    blocks.push(Block {
                        source_line: line,
                        kind: BlockKind::Alert {
                            kind: alert_kind,
                            blocks: inner,
                        },
                    });
                } else {
                    blocks.push(Block {
                        source_line: line,
                        kind: BlockKind::BlockQuote { blocks: inner },
                    });
                }
            }
            Event::Start(Tag::List(start_num)) => {
                flush_loose(&mut blocks, &mut loose_spans, loose_line);
                let ordered_start = *start_num;
                let end_tag = TagEnd::List(ordered_start.is_some());
                *idx += 1;
                let mut items = Vec::new();
                while *idx < events.len() {
                    match &events[*idx].0 {
                        Event::End(t) if *t == end_tag => {
                            *idx += 1;
                            break;
                        }
                        Event::Start(Tag::Item) => {
                            *idx += 1;
                            let mut checked = None;
                            if *idx < events.len() {
                                if let Event::TaskListMarker(c) = events[*idx].0 {
                                    checked = Some(c);
                                    *idx += 1;
                                }
                            }
                            let item_blocks =
                                parse_blocks(events, idx, line_starts, Some(TagEnd::Item));
                            items.push(ListItem {
                                checked,
                                blocks: item_blocks,
                            });
                        }
                        _ => {
                            *idx += 1;
                        }
                    }
                }
                blocks.push(Block {
                    source_line: line,
                    kind: BlockKind::List {
                        ordered_start,
                        items,
                    },
                });
            }
            Event::Start(Tag::Table(aligns)) => {
                flush_loose(&mut blocks, &mut loose_spans, loose_line);
                let alignments: Vec<ColAlign> = aligns
                    .iter()
                    .map(|a| match a {
                        Alignment::Center => ColAlign::Center,
                        Alignment::Right => ColAlign::Right,
                        _ => ColAlign::Left,
                    })
                    .collect();
                *idx += 1;
                let mut headers: Vec<Vec<InlineSpan>> = Vec::new();
                let mut rows: Vec<Vec<Vec<InlineSpan>>> = Vec::new();
                let mut in_head = false;
                let mut current_row: Vec<Vec<InlineSpan>> = Vec::new();

                while *idx < events.len() {
                    match &events[*idx].0 {
                        Event::End(TagEnd::Table) => {
                            *idx += 1;
                            break;
                        }
                        Event::Start(Tag::TableHead) => {
                            in_head = true;
                            *idx += 1;
                        }
                        Event::End(TagEnd::TableHead) => {
                            in_head = false;
                            if !current_row.is_empty() {
                                headers = std::mem::take(&mut current_row);
                            }
                            *idx += 1;
                        }
                        Event::Start(Tag::TableRow) => {
                            current_row.clear();
                            *idx += 1;
                        }
                        Event::End(TagEnd::TableRow) => {
                            let r = std::mem::take(&mut current_row);
                            if in_head {
                                headers = r;
                            } else {
                                rows.push(r);
                            }
                            *idx += 1;
                        }
                        Event::Start(Tag::TableCell) => {
                            *idx += 1;
                            let cell = parse_inlines(events, idx, TagEnd::TableCell);
                            current_row.push(cell);
                        }
                        _ => {
                            *idx += 1;
                        }
                    }
                }
                blocks.push(Block {
                    source_line: line,
                    kind: BlockKind::Table {
                        alignments,
                        headers,
                        rows,
                    },
                });
            }
            Event::Rule => {
                flush_loose(&mut blocks, &mut loose_spans, loose_line);
                blocks.push(Block {
                    source_line: line,
                    kind: BlockKind::Rule,
                });
                *idx += 1;
            }
            Event::DisplayMath(latex) => {
                flush_loose(&mut blocks, &mut loose_spans, loose_line);
                blocks.push(Block {
                    source_line: line,
                    kind: BlockKind::DisplayMath {
                        latex: latex.trim().to_string(),
                    },
                });
                *idx += 1;
            }
            // Loose inline content (e.g. tight list items)
            Event::Text(t) => {
                if loose_spans.is_empty() {
                    loose_line = line;
                }
                loose_spans.push(InlineSpan {
                    text: t.to_string(),
                    style: inline_style.clone(),
                });
                *idx += 1;
            }
            Event::Code(c) => {
                if loose_spans.is_empty() {
                    loose_line = line;
                }
                let mut st = inline_style.clone();
                st.code = true;
                loose_spans.push(InlineSpan {
                    text: c.to_string(),
                    style: st,
                });
                *idx += 1;
            }
            Event::InlineMath(m) => {
                if loose_spans.is_empty() {
                    loose_line = line;
                }
                let mut st = inline_style.clone();
                st.math = true;
                loose_spans.push(InlineSpan {
                    text: m.to_string(),
                    style: st,
                });
                *idx += 1;
            }
            Event::SoftBreak => {
                if !loose_spans.is_empty() {
                    loose_spans.push(InlineSpan {
                        text: " ".to_string(),
                        style: inline_style.clone(),
                    });
                }
                *idx += 1;
            }
            Event::HardBreak => {
                loose_spans.push(InlineSpan {
                    text: "\n".to_string(),
                    style: inline_style.clone(),
                });
                *idx += 1;
            }
            Event::Start(Tag::Strong) => {
                inline_style.bold = true;
                *idx += 1;
            }
            Event::End(TagEnd::Strong) => {
                inline_style.bold = false;
                *idx += 1;
            }
            Event::Start(Tag::Emphasis) => {
                inline_style.italic = true;
                *idx += 1;
            }
            Event::End(TagEnd::Emphasis) => {
                inline_style.italic = false;
                *idx += 1;
            }
            Event::Start(Tag::Strikethrough) => {
                inline_style.strikethrough = true;
                *idx += 1;
            }
            Event::End(TagEnd::Strikethrough) => {
                inline_style.strikethrough = false;
                *idx += 1;
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                inline_style.link = Some(dest_url.to_string());
                *idx += 1;
            }
            Event::End(TagEnd::Link) => {
                inline_style.link = None;
                *idx += 1;
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                flush_loose(&mut blocks, &mut loose_spans, loose_line);
                let url = dest_url.to_string();
                *idx += 1;
                let alt_spans = parse_inlines(events, idx, TagEnd::Image);
                let alt: String = alt_spans.iter().map(|s| s.text.as_str()).collect();
                blocks.push(Block {
                    source_line: line,
                    kind: BlockKind::Image { url, alt },
                });
            }
            _ => {
                *idx += 1;
            }
        }
    }

    flush_loose(&mut blocks, &mut loose_spans, loose_line);
    blocks
}

fn parse_paragraph_inlines(
    events: &[(Event<'_>, std::ops::Range<usize>)],
    idx: &mut usize,
) -> (Vec<InlineSpan>, Option<(String, String)>, Option<String>) {
    let mut spans = Vec::new();
    let mut style = InlineStyle::default();
    let mut only_image: Option<(String, String)> = None;
    let mut only_math: Option<String> = None;
    let mut has_other = false;

    while *idx < events.len() {
        match &events[*idx].0 {
            Event::End(TagEnd::Paragraph) => {
                *idx += 1;
                break;
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                let url = dest_url.to_string();
                *idx += 1;
                let alt_spans = parse_inlines(events, idx, TagEnd::Image);
                let alt: String = alt_spans.iter().map(|s| s.text.as_str()).collect();
                if !has_other && only_image.is_none() && only_math.is_none() {
                    only_image = Some((url, alt));
                } else {
                    has_other = true;
                    spans.push(InlineSpan {
                        text: format!("[image: {}]", if alt.is_empty() { &url } else { &alt }),
                        style: InlineStyle {
                            italic: true,
                            link: Some(url),
                            ..Default::default()
                        },
                    });
                }
            }
            Event::DisplayMath(m) => {
                if !has_other && only_image.is_none() && only_math.is_none() {
                    only_math = Some(m.trim().to_string());
                } else {
                    has_other = true;
                    let mut st = style.clone();
                    st.math = true;
                    spans.push(InlineSpan {
                        text: m.to_string(),
                        style: st,
                    });
                }
                *idx += 1;
            }
            _ => {
                if let Some((url, alt)) = only_image.take() {
                    spans.push(InlineSpan {
                        text: format!("[image: {}]", if alt.is_empty() { &url } else { &alt }),
                        style: InlineStyle {
                            italic: true,
                            link: Some(url),
                            ..Default::default()
                        },
                    });
                }
                if let Some(m) = only_math.take() {
                    let mut st = style.clone();
                    st.math = true;
                    spans.push(InlineSpan { text: m, style: st });
                }
                consume_inline_event(&events[*idx].0, &mut style, &mut spans, &mut has_other);
                *idx += 1;
            }
        }
    }

    if has_other {
        (spans, None, None)
    } else {
        (spans, only_image, only_math)
    }
}

fn parse_inlines(
    events: &[(Event<'_>, std::ops::Range<usize>)],
    idx: &mut usize,
    stop: TagEnd,
) -> Vec<InlineSpan> {
    let mut spans = Vec::new();
    let mut style = InlineStyle::default();
    let mut dummy = false;

    while *idx < events.len() {
        if matches!(&events[*idx].0, Event::End(t) if *t == stop) {
            *idx += 1;
            break;
        }
        if let Event::Start(Tag::Image { dest_url, .. }) = &events[*idx].0 {
            let url = dest_url.to_string();
            *idx += 1;
            let alt_spans = parse_inlines(events, idx, TagEnd::Image);
            let alt: String = alt_spans.iter().map(|s| s.text.as_str()).collect();
            spans.push(InlineSpan {
                text: if alt.is_empty() { url.clone() } else { alt },
                style: InlineStyle {
                    link: Some(url),
                    ..style.clone()
                },
            });
            continue;
        }
        consume_inline_event(&events[*idx].0, &mut style, &mut spans, &mut dummy);
        *idx += 1;
    }

    spans
}

fn consume_inline_event(
    ev: &Event<'_>,
    style: &mut InlineStyle,
    spans: &mut Vec<InlineSpan>,
    has_content: &mut bool,
) {
    match ev {
        Event::Text(t) => {
            if !t.trim().is_empty() {
                *has_content = true;
            }
            spans.push(InlineSpan {
                text: t.to_string(),
                style: style.clone(),
            });
        }
        Event::Code(c) => {
            *has_content = true;
            let mut st = style.clone();
            st.code = true;
            spans.push(InlineSpan {
                text: c.to_string(),
                style: st,
            });
        }
        Event::InlineMath(m) => {
            *has_content = true;
            let mut st = style.clone();
            st.math = true;
            spans.push(InlineSpan {
                text: m.to_string(),
                style: st,
            });
        }
        Event::SoftBreak => {
            spans.push(InlineSpan {
                text: " ".to_string(),
                style: style.clone(),
            });
        }
        Event::HardBreak => {
            spans.push(InlineSpan {
                text: "\n".to_string(),
                style: style.clone(),
            });
        }
        Event::Start(Tag::Strong) => style.bold = true,
        Event::End(TagEnd::Strong) => style.bold = false,
        Event::Start(Tag::Emphasis) => style.italic = true,
        Event::End(TagEnd::Emphasis) => style.italic = false,
        Event::Start(Tag::Strikethrough) => style.strikethrough = true,
        Event::End(TagEnd::Strikethrough) => style.strikethrough = false,
        Event::Start(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.to_string()),
        Event::End(TagEnd::Link) => style.link = None,
        Event::FootnoteReference(r) => {
            *has_content = true;
            spans.push(InlineSpan {
                text: format!("[^{r}]"),
                style: style.clone(),
            });
        }
        _ => {}
    }
}

/// Convert `> [!NOTE]` style blockquotes (when not already parsed as GFM BlockQuoteKind) into Alert blocks.
fn promote_github_alerts(blocks: &mut [Block]) {
    for blk in blocks.iter_mut() {
        let maybe_alert = if let BlockKind::BlockQuote { blocks: inner } = &mut blk.kind {
            detect_alert_marker(inner)
        } else {
            None
        };
        if let Some((kind, new_inner)) = maybe_alert {
            blk.kind = BlockKind::Alert {
                kind,
                blocks: new_inner,
            };
        }
    }
}

fn detect_alert_marker(inner: &mut Vec<Block>) -> Option<(AlertKind, Vec<Block>)> {
    let first = inner.first_mut()?;
    let BlockKind::Paragraph { spans } = &mut first.kind else {
        return None;
    };
    let first_span = spans.first_mut()?;
    let trimmed = first_span.text.trim_start();
    let (kind, tag) = [
        (AlertKind::Note, "[!NOTE]"),
        (AlertKind::Tip, "[!TIP]"),
        (AlertKind::Important, "[!IMPORTANT]"),
        (AlertKind::Warning, "[!WARNING]"),
        (AlertKind::Caution, "[!CAUTION]"),
    ]
    .into_iter()
    .find(|(_, marker)| trimmed.to_uppercase().starts_with(marker))?;

    let rest = trimmed[tag.len()..].trim_start().to_string();
    if rest.is_empty() {
        spans.remove(0);
        if spans.first().map(|s| s.text.trim().is_empty()).unwrap_or(false) {
            spans.remove(0);
        }
    } else {
        first_span.text = rest;
    }

    let mut out = std::mem::take(inner);
    if let Some(Block {
        kind: BlockKind::Paragraph { spans },
        ..
    }) = out.first()
    {
        if spans.is_empty() {
            out.remove(0);
        }
    }
    Some((kind, out))
}

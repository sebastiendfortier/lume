use crate::theme::Theme;
use std::collections::{HashMap, HashSet};

/// Render a Mermaid diagram into a crisp, theme-matched SVG string (100% pure Rust, zero Node/Chromium).
/// Supports flowcharts (`graph`/`flowchart` TD/TB/LR/RL/BT), `sequenceDiagram`, and `pie`,
/// with a clean styled card fallback for other diagram types.
pub fn render_mermaid_svg(code: &str, theme: &Theme, max_width: f32) -> String {
    let first_line = code
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("%%"))
        .unwrap_or("");
    let lower = first_line.to_lowercase();

    if lower.starts_with("graph") || lower.starts_with("flowchart") {
        let horizontal = lower.contains(" lr") || lower.contains(" rl");
        if let Some(svg) = render_flowchart_svg(code, theme, max_width, horizontal) {
            return svg;
        }
    } else if lower.starts_with("sequencediagram") {
        if let Some(svg) = render_sequence_svg(code, theme, max_width) {
            return svg;
        }
    } else if lower.starts_with("pie") {
        if let Some(svg) = render_pie_svg(code, theme, max_width) {
            return svg;
        }
    }

    render_fallback_svg(code, theme, max_width)
}

#[derive(Debug, Clone)]
struct FlowNode {
    id: String,
    label: String,
    shape: NodeShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeShape {
    Rect,
    Rounded,
    Diamond,
    Circle,
}

#[derive(Debug, Clone)]
struct FlowEdge {
    from: String,
    to: String,
    label: Option<String>,
    dashed: bool,
}

fn render_flowchart_svg(
    code: &str,
    theme: &Theme,
    max_width: f32,
    horizontal: bool,
) -> Option<String> {
    let mut nodes: HashMap<String, FlowNode> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    let mut edges: Vec<FlowEdge> = Vec::new();

    for raw_line in code.lines().skip(1) {
        let line = raw_line.trim().trim_end_matches(';');
        if line.is_empty()
            || line.starts_with("%%")
            || line.starts_with("subgraph")
            || line == "end"
            || line.starts_with("style ")
            || line.starts_with("classDef ")
            || line.starts_with("class ")
        {
            continue;
        }

        if let Some((lhs, arrow, rhs)) = split_edge(line) {
            let dashed = arrow.contains(".-") || arrow.contains("-.-");
            let (edge_label, rhs_clean) = extract_edge_label(arrow, rhs);
            let n1 = parse_node_spec(lhs.trim());
            let n2 = parse_node_spec(rhs_clean.trim());
            register_node(&mut nodes, &mut order, n1.clone());
            register_node(&mut nodes, &mut order, n2.clone());
            edges.push(FlowEdge {
                from: n1.id,
                to: n2.id,
                label: edge_label,
                dashed,
            });
        } else if !line.contains(' ') || line.contains('[') || line.contains('(') || line.contains('{') {
            let n = parse_node_spec(line);
            if !n.id.is_empty() {
                register_node(&mut nodes, &mut order, n);
            }
        }
    }

    if order.is_empty() {
        return None;
    }

    // Assign topological layers via BFS from roots
    let mut indeg: HashMap<String, usize> = order.iter().map(|id| (id.clone(), 0)).collect();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for e in &edges {
        *indeg.entry(e.to.clone()).or_insert(0) += 1;
        adj.entry(e.from.clone()).or_default().push(e.to.clone());
    }

    let mut layer: HashMap<String, usize> = HashMap::new();
    let mut queue: Vec<String> = order
        .iter()
        .filter(|id| *indeg.get(*id).unwrap_or(&0) == 0)
        .cloned()
        .collect();
    if queue.is_empty() {
        queue.push(order[0].clone());
    }
    for r in &queue {
        layer.insert(r.clone(), 0);
    }

    let mut visited: HashSet<String> = HashSet::new();
    let mut qi = 0;
    while qi < queue.len() {
        let u = queue[qi].clone();
        qi += 1;
        if !visited.insert(u.clone()) {
            continue;
        }
        let lu = *layer.get(&u).unwrap_or(&0);
        if let Some(nbrs) = adj.get(&u) {
            for v in nbrs {
                let entry = layer.entry(v.clone()).or_insert(0);
                if *entry < lu + 1 {
                    *entry = lu + 1;
                }
                if !visited.contains(v) {
                    queue.push(v.clone());
                }
            }
        }
    }
    for id in &order {
        layer.entry(id.clone()).or_insert(0);
    }

    let max_layer = layer.values().copied().max().unwrap_or(0);
    let mut layers: Vec<Vec<String>> = vec![Vec::new(); max_layer + 1];
    for id in &order {
        layers[layer[id]].push(id.clone());
    }

    let node_h = 42.0f32;
    let has_labels = edges.iter().any(|e| e.label.is_some());
    let gap_x = if horizontal && has_labels { 96.0f32 } else { 56.0f32 };
    let gap_y = if !horizontal && has_labels { 64.0f32 } else { 52.0f32 };
    let pad = 28.0f32;

    let node_widths: HashMap<String, f32> = nodes
        .iter()
        .map(|(id, n)| {
            let w = (n.label.chars().count() as f32 * 8.2 + 32.0).clamp(90.0, 240.0);
            (id.clone(), w)
        })
        .collect();

    let mut pos: HashMap<String, (f32, f32, f32, f32)> = HashMap::new();
    let (svg_w, svg_h) = if horizontal {
        let mut x = pad;
        let mut max_h = 0.0f32;
        for col in &layers {
            let col_w = col
                .iter()
                .map(|id| node_widths[id])
                .fold(90.0f32, f32::max);
            let col_h = col.len() as f32 * node_h + (col.len().saturating_sub(1)) as f32 * gap_y;
            max_h = max_h.max(col_h);
            for (ri, id) in col.iter().enumerate() {
                let w = node_widths[id];
                let y = pad + ri as f32 * (node_h + gap_y);
                pos.insert(id.clone(), (x + (col_w - w) * 0.5, y, w, node_h));
            }
            x += col_w + gap_x;
        }
        ((x - gap_x + pad).max(260.0), (max_h + pad * 2.0).max(120.0))
    } else {
        let max_row_w = layers
            .iter()
            .map(|row| {
                row.iter().map(|id| node_widths[id]).sum::<f32>()
                    + row.len().saturating_sub(1) as f32 * gap_x
            })
            .fold(240.0f32, f32::max);
        let total_w = (max_row_w + pad * 2.0).min(max_width.max(360.0));
        let mut y = pad;
        for row in &layers {
            let row_w = row.iter().map(|id| node_widths[id]).sum::<f32>()
                + row.len().saturating_sub(1) as f32 * gap_x;
            let mut x = ((total_w - row_w) * 0.5).max(pad);
            for id in row {
                let w = node_widths[id];
                pos.insert(id.clone(), (x, y, w, node_h));
                x += w + gap_x;
            }
            y += node_h + gap_y;
        }
        (total_w, (y - gap_y + pad).max(120.0))
    };

    let bg = theme.surface.to_hex();
    let border = theme.border.to_hex();
    let node_fill = theme.code_header_bg.to_hex();
    let accent = theme.accent_note.to_hex();
    let text = theme.text.to_hex();
    let muted = theme.text_muted.to_hex();

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{svg_w:.0}" height="{svg_h:.0}" viewBox="0 0 {svg_w:.0} {svg_h:.0}">
<defs>
  <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 0 1 L 10 5 L 0 9 z" fill="{accent}"/>
  </marker>
</defs>
<rect x="0.5" y="0.5" width="{bw:.0}" height="{bh:.0}" rx="8" fill="{bg}" stroke="{border}" stroke-width="1"/>
"##,
        bw = svg_w - 1.0,
        bh = svg_h - 1.0,
    );

    // Draw edges first
    for e in &edges {
        if let (Some(&(x1, y1, w1, h1)), Some(&(x2, y2, w2, h2))) =
            (pos.get(&e.from), pos.get(&e.to))
        {
            let (sx, sy, ex, ey) = if horizontal {
                (x1 + w1, y1 + h1 * 0.5, x2, y2 + h2 * 0.5)
            } else {
                (x1 + w1 * 0.5, y1 + h1, x2 + w2 * 0.5, y2)
            };
            let dash = if e.dashed { r#" stroke-dasharray="5,4""# } else { "" };
            let (cx1, cy1, cx2, cy2) = if horizontal {
                let mx = (sx + ex) * 0.5;
                (mx, sy, mx, ey)
            } else {
                let my = (sy + ey) * 0.5;
                (sx, my, ex, my)
            };
            svg.push_str(&format!(
                r#"<path d="M {sx:.1} {sy:.1} C {cx1:.1} {cy1:.1}, {cx2:.1} {cy2:.1}, {ex:.1} {ey:.1}" fill="none" stroke="{accent}" stroke-width="1.8"{dash} marker-end="url(#arrow)"/>"#
            ));
            if let Some(ref lbl) = e.label {
                let mx = (sx + ex) * 0.5;
                let my = (sy + ey) * 0.5 - 4.0;
                let lw = (lbl.chars().count() as f32 * 6.5 + 10.0).max(24.0);
                svg.push_str(&format!(
                    r#"<rect x="{rx:.1}" y="{ry:.1}" width="{lw:.1}" height="16" rx="4" fill="{bg}" opacity="0.92"/>"#,
                    rx = mx - lw * 0.5,
                    ry = my - 11.5
                ));
                svg.push_str(&format!(
                    r#"<text x="{mx:.1}" y="{my:.1}" text-anchor="middle" font-family="Liberation Sans, sans-serif" font-size="11" fill="{muted}">{}</text>"#,
                    xml_escape(lbl)
                ));
            }
        }
    }

    // Draw nodes
    for id in &order {
        let n = &nodes[id];
        let (x, y, w, h) = pos[id];
        let cx = x + w * 0.5;
        let cy = y + h * 0.5;
        match n.shape {
            NodeShape::Diamond => {
                svg.push_str(&format!(
                    r#"<polygon points="{cx:.1},{y:.1} {rx:.1},{cy:.1} {cx:.1},{by:.1} {x:.1},{cy:.1}" fill="{node_fill}" stroke="{accent}" stroke-width="1.6"/>"#,
                    rx = x + w,
                    by = y + h
                ));
            }
            NodeShape::Circle => {
                let rx = w * 0.5;
                let ry = h * 0.55;
                svg.push_str(&format!(
                    r#"<ellipse cx="{cx:.1}" cy="{cy:.1}" rx="{rx:.1}" ry="{ry:.1}" fill="{node_fill}" stroke="{accent}" stroke-width="1.6"/>"#
                ));
            }
            NodeShape::Rounded => {
                svg.push_str(&format!(
                    r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="18" fill="{node_fill}" stroke="{accent}" stroke-width="1.6"/>"#
                ));
            }
            NodeShape::Rect => {
                svg.push_str(&format!(
                    r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="6" fill="{node_fill}" stroke="{accent}" stroke-width="1.6"/>"#
                ));
            }
        }
        svg.push_str(&format!(
            r#"<text x="{cx:.1}" y="{ty:.1}" text-anchor="middle" font-family="Liberation Sans, sans-serif" font-size="13" font-weight="600" fill="{text}">{}</text>"#,
            xml_escape(&n.label),
            ty = cy + 4.5
        ));
    }

    svg.push_str("</svg>");
    Some(svg)
}

fn render_sequence_svg(code: &str, theme: &Theme, max_width: f32) -> Option<String> {
    let mut actors: Vec<String> = Vec::new();
    let mut msgs: Vec<(String, String, String, bool)> = Vec::new();

    for raw in code.lines().skip(1) {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("%%") {
            continue;
        }
        if let Some(rest) = line.strip_prefix("participant ").or_else(|| line.strip_prefix("actor ")) {
            let name = rest.split(" as ").next().unwrap_or(rest).trim().to_string();
            if !actors.contains(&name) {
                actors.push(name);
            }
            continue;
        }
        for arrow in ["-->>", "->>", "-->", "->"] {
            if let Some(pos) = line.find(arrow) {
                let from = line[..pos].trim().to_string();
                let after = &line[pos + arrow.len()..];
                let (to, msg) = match after.split_once(':') {
                    Some((t, m)) => (t.trim().to_string(), m.trim().to_string()),
                    None => (after.trim().to_string(), String::new()),
                };
                if !actors.contains(&from) {
                    actors.push(from.clone());
                }
                if !actors.contains(&to) {
                    actors.push(to.clone());
                }
                msgs.push((from, to, msg, arrow.starts_with("--")));
                break;
            }
        }
    }

    if actors.is_empty() {
        return None;
    }

    let pad = 28.0f32;
    let col_gap = ((max_width - pad * 2.0) / actors.len().max(2) as f32).clamp(110.0, 200.0);
    let svg_w = (actors.len() as f32 * col_gap + pad * 2.0).max(280.0);
    let top_y = 24.0f32;
    let box_h = 34.0f32;
    let msg_step = 42.0f32;
    let svg_h = top_y + box_h + 24.0 + msgs.len() as f32 * msg_step + 32.0;

    let bg = theme.surface.to_hex();
    let border = theme.border.to_hex();
    let fill = theme.code_header_bg.to_hex();
    let accent = theme.accent_note.to_hex();
    let text = theme.text.to_hex();
    let muted = theme.text_muted.to_hex();

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{svg_w:.0}" height="{svg_h:.0}" viewBox="0 0 {svg_w:.0} {svg_h:.0}">
<defs>
  <marker id="sarrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 0 1 L 10 5 L 0 9 z" fill="{accent}"/>
  </marker>
</defs>
<rect x="0.5" y="0.5" width="{bw:.0}" height="{bh:.0}" rx="8" fill="{bg}" stroke="{border}" stroke-width="1"/>
"##,
        bw = svg_w - 1.0,
        bh = svg_h - 1.0,
    );

    let mut xs: HashMap<String, f32> = HashMap::new();
    for (i, a) in actors.iter().enumerate() {
        let cx = pad + col_gap * 0.5 + i as f32 * col_gap;
        xs.insert(a.clone(), cx);
        let bw = (a.chars().count() as f32 * 8.0 + 28.0).clamp(80.0, col_gap - 16.0);
        svg.push_str(&format!(
            r#"<line x1="{cx:.1}" y1="{ly1:.1}" x2="{cx:.1}" y2="{ly2:.1}" stroke="{border}" stroke-width="1.4" stroke-dasharray="4,4"/>"#,
            ly1 = top_y + box_h,
            ly2 = svg_h - 18.0
        ));
        svg.push_str(&format!(
            r#"<rect x="{bx:.1}" y="{top_y:.1}" width="{bw:.1}" height="{box_h:.1}" rx="6" fill="{fill}" stroke="{accent}" stroke-width="1.5"/>"#,
            bx = cx - bw * 0.5
        ));
        svg.push_str(&format!(
            r#"<text x="{cx:.1}" y="{ty:.1}" text-anchor="middle" font-family="Liberation Sans, sans-serif" font-size="13" font-weight="600" fill="{text}">{}</text>"#,
            xml_escape(a),
            ty = top_y + 21.5
        ));
    }

    for (mi, (from, to, msg, dashed)) in msgs.iter().enumerate() {
        let y = top_y + box_h + 32.0 + mi as f32 * msg_step;
        let x1 = xs[from];
        let x2 = xs[to];
        let dash = if *dashed { r#" stroke-dasharray="5,4""# } else { "" };
        svg.push_str(&format!(
            r#"<line x1="{x1:.1}" y1="{y:.1}" x2="{x2:.1}" y2="{y:.1}" stroke="{accent}" stroke-width="1.6"{dash} marker-end="url(#sarrow)"/>"#
        ));
        let mx = (x1 + x2) * 0.5;
        svg.push_str(&format!(
            r#"<text x="{mx:.1}" y="{ty:.1}" text-anchor="middle" font-family="Liberation Sans, sans-serif" font-size="12" fill="{muted}">{}</text>"#,
            xml_escape(msg),
            ty = y - 6.0
        ));
    }

    svg.push_str("</svg>");
    Some(svg)
}

fn render_pie_svg(code: &str, theme: &Theme, _max_width: f32) -> Option<String> {
    let mut slices: Vec<(String, f64)> = Vec::new();
    let mut title = String::new();
    for raw in code.lines().skip(1) {
        let l = raw.trim();
        if let Some(t) = l.strip_prefix("title ") {
            title = t.trim().to_string();
        } else if let Some((k, v)) = l.split_once(':') {
            let label = k.trim().trim_matches('"').to_string();
            if let Ok(val) = v.trim().parse::<f64>() {
                if val > 0.0 {
                    slices.push((label, val));
                }
            }
        }
    }
    if slices.is_empty() {
        return None;
    }
    let total: f64 = slices.iter().map(|(_, v)| *v).sum();
    let colors = [
        theme.accent_note.to_hex(),
        theme.accent_tip.to_hex(),
        theme.accent_important.to_hex(),
        theme.accent_warning.to_hex(),
        theme.accent_caution.to_hex(),
    ];
    let svg_w = 460.0f32;
    let svg_h = 220.0f32;
    let cx = 120.0f64;
    let cy = 115.0f64;
    let r = 74.0f64;

    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{svg_w:.0}" height="{svg_h:.0}" viewBox="0 0 {svg_w:.0} {svg_h:.0}">
<rect x="0.5" y="0.5" width="{bw:.0}" height="{bh:.0}" rx="8" fill="{bg}" stroke="{border}" stroke-width="1"/>
"##,
        bw = svg_w - 1.0,
        bh = svg_h - 1.0,
        bg = theme.surface.to_hex(),
        border = theme.border.to_hex(),
    );

    if !title.is_empty() {
        svg.push_str(&format!(
            r#"<text x="230" y="24" text-anchor="middle" font-family="Liberation Sans, sans-serif" font-size="14" font-weight="700" fill="{}">{}</text>"#,
            theme.text.to_hex(),
            xml_escape(&title)
        ));
    }

    let mut angle = -std::f64::consts::FRAC_PI_2;
    for (i, (label, val)) in slices.iter().enumerate() {
        let frac = *val / total;
        let next_angle = angle + frac * std::f64::consts::TAU;
        let x1 = cx + r * angle.cos();
        let y1 = cy + r * angle.sin();
        let x2 = cx + r * next_angle.cos();
        let y2 = cy + r * next_angle.sin();
        let large = if frac > 0.5 { 1 } else { 0 };
        let col = &colors[i % colors.len()];
        svg.push_str(&format!(
            r#"<path d="M {cx:.1} {cy:.1} L {x1:.1} {y1:.1} A {r:.1} {r:.1} 0 {large} 1 {x2:.1} {y2:.1} Z" fill="{col}" stroke="{}" stroke-width="1.5"/>"#,
            theme.bg.to_hex()
        ));
        let ly = 54.0 + i as f32 * 24.0;
        svg.push_str(&format!(
            r#"<rect x="230" y="{ry:.1}" width="12" height="12" rx="3" fill="{col}"/>
<text x="250" y="{ly:.1}" font-family="Liberation Sans, sans-serif" font-size="12" fill="{}">{} ({:.0}%)</text>"#,
            theme.text.to_hex(),
            xml_escape(label),
            frac * 100.0,
            ry = ly - 10.0
        ));
        angle = next_angle;
    }

    svg.push_str("</svg>");
    Some(svg)
}

fn render_fallback_svg(code: &str, theme: &Theme, max_width: f32) -> String {
    let lines: Vec<&str> = code.lines().take(24).collect();
    let w = max_width.clamp(320.0, 720.0);
    let h = (lines.len() as f32 * 18.0 + 48.0).max(80.0);
    let mut svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" viewBox="0 0 {w:.0} {h:.0}">
<rect x="0.5" y="0.5" width="{bw:.0}" height="{bh:.0}" rx="8" fill="{bg}" stroke="{border}" stroke-width="1"/>
<text x="14" y="22" font-family="Liberation Sans, sans-serif" font-size="11" font-weight="700" fill="{accent}">MERMAID DIAGRAM</text>
"##,
        bw = w - 1.0,
        bh = h - 1.0,
        bg = theme.surface.to_hex(),
        border = theme.border.to_hex(),
        accent = theme.accent_important.to_hex(),
    );
    for (i, l) in lines.iter().enumerate() {
        let y = 44.0 + i as f32 * 18.0;
        svg.push_str(&format!(
            r#"<text x="16" y="{y:.1}" font-family="DejaVu Sans Mono, monospace" font-size="12" fill="{}">{}</text>"#,
            theme.text.to_hex(),
            xml_escape(l)
        ));
    }
    svg.push_str("</svg>");
    svg
}

fn split_edge(line: &str) -> Option<(&str, &str, &str)> {
    for arrow in ["-.->", "==>", "-->", "---", "->"] {
        if let Some(pos) = line.find(arrow) {
            return Some((&line[..pos], arrow, &line[pos + arrow.len()..]));
        }
    }
    None
}

fn extract_edge_label<'a>(_arrow: &str, rhs: &'a str) -> (Option<String>, &'a str) {
    let trimmed = rhs.trim_start();
    if let Some(rest) = trimmed.strip_prefix('|') {
        if let Some(end) = rest.find('|') {
            return (Some(rest[..end].trim().to_string()), rest[end + 1..].trim_start());
        }
    }
    (None, trimmed)
}

fn parse_node_spec(spec: &str) -> FlowNode {
    let s = spec.trim();
    for (open, close, shape) in [
        ("((", "))", NodeShape::Circle),
        ("([", "])", NodeShape::Rounded),
        ("(", ")", NodeShape::Rounded),
        ("{", "}", NodeShape::Diamond),
        ("[", "]", NodeShape::Rect),
    ] {
        if let Some(start) = s.find(open) {
            if let Some(end) = s.rfind(close) {
                if end > start + open.len() {
                    let id = s[..start].trim().to_string();
                    let label = s[start + open.len()..end]
                        .trim()
                        .trim_matches('"')
                        .to_string();
                    return FlowNode {
                        id: if id.is_empty() { label.clone() } else { id },
                        label,
                        shape,
                    };
                }
            }
        }
    }
    let id = s.split_whitespace().next().unwrap_or(s).to_string();
    FlowNode {
        label: id.clone(),
        id,
        shape: NodeShape::Rect,
    }
}

fn register_node(nodes: &mut HashMap<String, FlowNode>, order: &mut Vec<String>, node: FlowNode) {
    if !nodes.contains_key(&node.id) {
        order.push(node.id.clone());
        nodes.insert(node.id.clone(), node);
    } else if let Some(existing) = nodes.get_mut(&node.id) {
        if existing.label == existing.id && node.label != node.id {
            existing.label = node.label;
            existing.shape = node.shape;
        }
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

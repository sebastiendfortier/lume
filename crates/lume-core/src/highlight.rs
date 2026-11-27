use crate::theme::{Rgb, Theme};
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::{FontStyle, ThemeSet};
use syntect::parsing::SyntaxSet;

#[derive(Debug, Clone)]
pub struct StyledToken {
    pub text: String,
    pub fg: Rgb,
    pub bold: bool,
    pub italic: bool,
}

static SYNTAX_SET: OnceLock<SyntaxSet> = OnceLock::new();
static THEME_SET: OnceLock<ThemeSet> = OnceLock::new();

fn syntax_set() -> &'static SyntaxSet {
    SYNTAX_SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    THEME_SET.get_or_init(ThemeSet::load_defaults)
}

/// Highlight a code block into lines of colored tokens.
pub fn highlight_code(code: &str, lang: &str, theme: &Theme) -> Vec<Vec<StyledToken>> {
    let ss = syntax_set();
    let ts = theme_set();

    let syntax = if lang.is_empty() {
        ss.find_syntax_plain_text()
    } else {
        ss.find_syntax_by_token(lang)
            .or_else(|| ss.find_syntax_by_extension(lang))
            .or_else(|| {
                // Common aliases
                match lang.to_lowercase().as_str() {
                    "rs" | "rust" => ss.find_syntax_by_extension("rs"),
                    "py" | "python" | "python3" => ss.find_syntax_by_extension("py"),
                    "sh" | "bash" | "zsh" | "shell" => ss.find_syntax_by_extension("sh"),
                    "js" | "javascript" | "jsx" => ss.find_syntax_by_extension("js"),
                    "ts" | "typescript" | "tsx" => ss.find_syntax_by_extension("js"),
                    "yml" | "yaml" => ss.find_syntax_by_extension("yaml"),
                    "toml" => ss.find_syntax_by_token("TOML").or_else(|| ss.find_syntax_by_extension("ini")),
                    "json" | "jsonc" => ss.find_syntax_by_extension("json"),
                    "cpp" | "c++" | "cxx" => ss.find_syntax_by_extension("cpp"),
                    "md" | "markdown" => ss.find_syntax_by_extension("md"),
                    _ => None,
                }
            })
            .unwrap_or_else(|| ss.find_syntax_plain_text())
    };

    let syn_theme = ts
        .themes
        .get(theme.syntect_theme)
        .or_else(|| ts.themes.values().next())
        .expect("default syntect themes loaded");

    let mut h = HighlightLines::new(syntax, syn_theme);
    let mut out = Vec::new();

    for line in code.lines() {
        let with_nl = format!("{line}\n");
        let tokens = match h.highlight_line(&with_nl, ss) {
            Ok(ranges) => ranges
                .into_iter()
                .filter_map(|(style, slice)| {
                    let clean = slice.trim_end_matches(['\n', '\r']);
                    if clean.is_empty() {
                        return None;
                    }
                    Some(StyledToken {
                        text: clean.to_string(),
                        fg: Rgb(style.foreground.r, style.foreground.g, style.foreground.b),
                        bold: style.font_style.contains(FontStyle::BOLD),
                        italic: style.font_style.contains(FontStyle::ITALIC),
                    })
                })
                .collect(),
            Err(_) => vec![StyledToken {
                text: line.to_string(),
                fg: theme.text,
                bold: false,
                italic: false,
            }],
        };
        out.push(tokens);
    }

    if out.is_empty() {
        out.push(Vec::new());
    }
    out
}

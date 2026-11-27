use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn to_rgba(self, a: u8) -> [u8; 4] {
        [self.0, self.1, self.2, a]
    }

    pub fn to_skia(self) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.0, self.1, self.2, 255)
    }

    pub fn to_skia_alpha(self, a: u8) -> tiny_skia::Color {
        tiny_skia::Color::from_rgba8(self.0, self.1, self.2, a)
    }

    pub fn to_cosmic(self) -> cosmic_text::Color {
        cosmic_text::Color::rgb(self.0, self.1, self.2)
    }

    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    pub fn ansi_fg(self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.0, self.1, self.2)
    }

    pub fn ansi_bg(self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.0, self.1, self.2)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemePreset {
    #[default]
    GithubDark,
    GithubLight,
    VscodeDark,
    CatppuccinMocha,
}

impl ThemePreset {
    pub fn parse(name: &str) -> Option<Self> {
        match name.trim().to_lowercase().as_str() {
            "github-dark" | "github" | "dark" | "gh-dark" => Some(Self::GithubDark),
            "github-light" | "light" | "gh-light" => Some(Self::GithubLight),
            "vscode-dark" | "vscode" | "cursor" => Some(Self::VscodeDark),
            "catppuccin" | "catppuccin-mocha" | "mocha" => Some(Self::CatppuccinMocha),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::GithubDark => "github-dark",
            Self::GithubLight => "github-light",
            Self::VscodeDark => "vscode-dark",
            Self::CatppuccinMocha => "catppuccin-mocha",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::GithubDark => Self::VscodeDark,
            Self::VscodeDark => Self::CatppuccinMocha,
            Self::CatppuccinMocha => Self::GithubLight,
            Self::GithubLight => Self::GithubDark,
        }
    }

    pub fn theme(self) -> Theme {
        match self {
            Self::GithubDark => Theme {
                preset: self,
                is_dark: true,
                bg: Rgb(13, 17, 23),              // #0d1117
                surface: Rgb(22, 27, 34),         // #161b22
                code_bg: Rgb(22, 27, 34),         // #161b22
                code_header_bg: Rgb(33, 38, 45),  // #21262d
                inline_code_bg: Rgb(48, 54, 61),  // #30363d
                table_header_bg: Rgb(22, 27, 34), // #161b22
                table_alt_bg: Rgb(18, 23, 30),    // #12171e
                border: Rgb(48, 54, 61),          // #30363d
                hr: Rgb(33, 38, 45),              // #21262d
                text: Rgb(230, 237, 243),         // #e6edf3
                text_muted: Rgb(139, 148, 158),   // #8b949e
                heading: Rgb(240, 246, 252),      // #f0f6fc
                link: Rgb(88, 166, 255),          // #58a6ff
                inline_code_fg: Rgb(230, 237, 243),
                quote_bar: Rgb(61, 68, 77),
                accent_note: Rgb(88, 166, 255),
                accent_tip: Rgb(63, 185, 80),
                accent_important: Rgb(163, 113, 247),
                accent_warning: Rgb(210, 153, 34),
                accent_caution: Rgb(248, 81, 73),
                check_done: Rgb(63, 185, 80),
                syntect_theme: "base16-ocean.dark",
            },
            Self::GithubLight => Theme {
                preset: self,
                is_dark: false,
                bg: Rgb(255, 255, 255),
                surface: Rgb(246, 248, 250),
                code_bg: Rgb(246, 248, 250),
                code_header_bg: Rgb(234, 238, 242),
                inline_code_bg: Rgb(239, 241, 243),
                table_header_bg: Rgb(246, 248, 250),
                table_alt_bg: Rgb(249, 250, 251),
                border: Rgb(208, 215, 222),
                hr: Rgb(216, 222, 228),
                text: Rgb(31, 35, 40),
                text_muted: Rgb(101, 109, 118),
                heading: Rgb(31, 35, 40),
                link: Rgb(9, 105, 218),
                inline_code_fg: Rgb(31, 35, 40),
                quote_bar: Rgb(208, 215, 222),
                accent_note: Rgb(9, 105, 218),
                accent_tip: Rgb(26, 127, 55),
                accent_important: Rgb(130, 80, 223),
                accent_warning: Rgb(154, 103, 0),
                accent_caution: Rgb(207, 34, 46),
                check_done: Rgb(26, 127, 55),
                syntect_theme: "InspiredGitHub",
            },
            Self::VscodeDark => Theme {
                preset: self,
                is_dark: true,
                bg: Rgb(30, 30, 30),
                surface: Rgb(37, 37, 38),
                code_bg: Rgb(24, 24, 24),
                code_header_bg: Rgb(45, 45, 45),
                inline_code_bg: Rgb(55, 55, 58),
                table_header_bg: Rgb(37, 37, 38),
                table_alt_bg: Rgb(33, 33, 35),
                border: Rgb(62, 62, 66),
                hr: Rgb(60, 60, 60),
                text: Rgb(212, 212, 212),
                text_muted: Rgb(140, 140, 140),
                heading: Rgb(238, 238, 238),
                link: Rgb(79, 193, 255),
                inline_code_fg: Rgb(206, 145, 120),
                quote_bar: Rgb(0, 122, 204),
                accent_note: Rgb(79, 193, 255),
                accent_tip: Rgb(78, 201, 176),
                accent_important: Rgb(197, 134, 192),
                accent_warning: Rgb(220, 220, 170),
                accent_caution: Rgb(244, 71, 71),
                check_done: Rgb(78, 201, 176),
                syntect_theme: "base16-eighties.dark",
            },
            Self::CatppuccinMocha => Theme {
                preset: self,
                is_dark: true,
                bg: Rgb(30, 30, 46),
                surface: Rgb(24, 24, 37),
                code_bg: Rgb(24, 24, 37),
                code_header_bg: Rgb(49, 50, 68),
                inline_code_bg: Rgb(49, 50, 68),
                table_header_bg: Rgb(36, 39, 58),
                table_alt_bg: Rgb(26, 26, 40),
                border: Rgb(69, 71, 90),
                hr: Rgb(69, 71, 90),
                text: Rgb(205, 214, 244),
                text_muted: Rgb(166, 173, 200),
                heading: Rgb(203, 166, 247),
                link: Rgb(137, 180, 250),
                inline_code_fg: Rgb(245, 194, 231),
                quote_bar: Rgb(137, 180, 250),
                accent_note: Rgb(137, 180, 250),
                accent_tip: Rgb(166, 227, 161),
                accent_important: Rgb(203, 166, 247),
                accent_warning: Rgb(249, 226, 175),
                accent_caution: Rgb(243, 139, 168),
                check_done: Rgb(166, 227, 161),
                syntect_theme: "base16-mocha.dark",
            },
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub preset: ThemePreset,
    pub is_dark: bool,
    pub bg: Rgb,
    pub surface: Rgb,
    pub code_bg: Rgb,
    pub code_header_bg: Rgb,
    pub inline_code_bg: Rgb,
    pub table_header_bg: Rgb,
    pub table_alt_bg: Rgb,
    pub border: Rgb,
    pub hr: Rgb,
    pub text: Rgb,
    pub text_muted: Rgb,
    pub heading: Rgb,
    pub link: Rgb,
    pub inline_code_fg: Rgb,
    pub quote_bar: Rgb,
    pub accent_note: Rgb,
    pub accent_tip: Rgb,
    pub accent_important: Rgb,
    pub accent_warning: Rgb,
    pub accent_caution: Rgb,
    pub check_done: Rgb,
    pub syntect_theme: &'static str,
}

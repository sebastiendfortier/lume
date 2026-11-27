use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default)]
pub struct Document {
    pub blocks: Vec<Block>,
    pub toc: Vec<TocEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TocEntry {
    pub level: u8,
    pub title: String,
    pub source_line: usize,
    pub block_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl AlertKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Note => "Note",
            Self::Tip => "Tip",
            Self::Important => "Important",
            Self::Warning => "Warning",
            Self::Caution => "Caution",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::Note => "i",
            Self::Tip => "*",
            Self::Important => "!",
            Self::Warning => "!",
            Self::Caution => "x",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColAlign {
    Left,
    Center,
    Right,
}

#[derive(Debug, Clone, Default)]
pub struct InlineStyle {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub code: bool,
    pub math: bool,
    pub link: Option<String>,
}

#[derive(Debug, Clone)]
pub struct InlineSpan {
    pub text: String,
    pub style: InlineStyle,
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub source_line: usize,
    pub kind: BlockKind,
}

#[derive(Debug, Clone)]
pub enum BlockKind {
    Heading {
        level: u8,
        spans: Vec<InlineSpan>,
        plain: String,
    },
    Paragraph {
        spans: Vec<InlineSpan>,
    },
    CodeBlock {
        lang: String,
        code: String,
    },
    Mermaid {
        code: String,
    },
    DisplayMath {
        latex: String,
    },
    BlockQuote {
        blocks: Vec<Block>,
    },
    Alert {
        kind: AlertKind,
        blocks: Vec<Block>,
    },
    List {
        ordered_start: Option<u64>,
        items: Vec<ListItem>,
    },
    Table {
        alignments: Vec<ColAlign>,
        headers: Vec<Vec<InlineSpan>>,
        rows: Vec<Vec<Vec<InlineSpan>>>,
    },
    Image {
        url: String,
        alt: String,
    },
    Rule,
}

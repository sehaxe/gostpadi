use std::fmt;

/// Вид отказа: от него зависят CLI-коды, HTTP-ответы и «empty» на сайте.
/// Сообщение — для человека, вид — для кода.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    Syntax,
    Empty,
    Unsupported,
}

impl ParseErrorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ParseErrorKind::Syntax => "syntax",
            ParseErrorKind::Empty => "empty",
            ParseErrorKind::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub msg: String,
    pub line: Option<usize>,
    pub col: Option<usize>,
    pub src: Option<String>,
}

impl ParseError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            kind: ParseErrorKind::Syntax,
            msg: msg.into(),
            line: None,
            col: None,
            src: None,
        }
    }

    /// Вход разобран, но рисовать нечего (пустое тело, одни return).
    pub fn empty(msg: impl Into<String>) -> Self {
        Self::new(msg).with_kind(ParseErrorKind::Empty)
    }

    pub fn with_kind(mut self, kind: ParseErrorKind) -> Self {
        self.kind = kind;
        self
    }
    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
    pub fn with_col(mut self, col: usize) -> Self {
        self.col = Some(col);
        self
    }
    pub fn with_src(mut self, src: impl Into<String>) -> Self {
        self.src = Some(src.into());
        self
    }
    pub fn locate(mut self, src_text: &str) -> Self {
        if self.line.is_some() && self.src.is_none() {
            if let Some(ln) = self.line {
                let lines: Vec<&str> = src_text.lines().collect();
                if ln >= 1 && ln <= lines.len() {
                    self.src = Some(lines[ln - 1].to_string());
                }
            }
        }
        self
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(l) = self.line {
            if let Some(c) = self.col {
                write!(f, "{}:{}: {}", l, c, self.msg)?;
            } else {
                write!(f, "{}: {}", l, self.msg)?;
            }
        } else {
            write!(f, "{}", self.msg)?;
        }
        if let Some(s) = &self.src {
            write!(f, " ({})", s.trim())?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

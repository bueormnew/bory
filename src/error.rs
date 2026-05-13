use std::fmt;

use crate::span::Span;

#[derive(Debug, Clone)]
pub enum BoryErrorKind {
    Lex,
    Parse,
    Runtime,
    Io,
}

#[derive(Debug, Clone)]
pub struct BoryError {
    pub kind: BoryErrorKind,
    pub code: Option<String>,
    pub message: String,
    pub span: Option<Span>,
    pub source_name: Option<String>,
    pub source_code: Option<String>,
    pub hint: Option<String>,
    pub notes: Vec<String>,
    pub trace: Vec<String>,
}

impl BoryError {
    pub fn lex(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: BoryErrorKind::Lex,
            code: Some("LEX001".to_string()),
            message: message.into(),
            span: Some(span),
            source_name: None,
            source_code: None,
            hint: None,
            notes: Vec::new(),
            trace: Vec::new(),
        }
    }

    pub fn parse(message: impl Into<String>, span: Span) -> Self {
        Self {
            kind: BoryErrorKind::Parse,
            code: Some("PARSE001".to_string()),
            message: message.into(),
            span: Some(span),
            source_name: None,
            source_code: None,
            hint: None,
            notes: Vec::new(),
            trace: Vec::new(),
        }
    }

    pub fn runtime(message: impl Into<String>, span: Option<Span>) -> Self {
        Self {
            kind: BoryErrorKind::Runtime,
            code: Some("RUNTIME001".to_string()),
            message: message.into(),
            span,
            source_name: None,
            source_code: None,
            hint: None,
            notes: Vec::new(),
            trace: Vec::new(),
        }
    }

    pub fn io(message: impl Into<String>) -> Self {
        Self {
            kind: BoryErrorKind::Io,
            code: Some("IO001".to_string()),
            message: message.into(),
            span: None,
            source_name: None,
            source_code: None,
            hint: None,
            notes: Vec::new(),
            trace: Vec::new(),
        }
    }

    pub fn with_source(mut self, source_name: impl Into<String>) -> Self {
        self.source_name = Some(source_name.into());
        self
    }

    pub fn with_source_context(
        mut self,
        source_name: impl Into<String>,
        source_code: impl Into<String>,
    ) -> Self {
        self.source_name = Some(source_name.into());
        self.source_code = Some(source_code.into());
        self
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_span_if_missing(mut self, span: Span) -> Self {
        if self.span.is_none() {
            self.span = Some(span);
        }
        self
    }

    pub fn push_trace(mut self, frame: impl Into<String>) -> Self {
        self.trace.push(frame.into());
        self
    }
}

impl fmt::Display for BoryErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Lex => "lexer",
            Self::Parse => "parser",
            Self::Runtime => "runtime",
            Self::Io => "io",
        };
        write!(f, "{label}")
    }
}

impl fmt::Display for BoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}", self.kind)?;
        if let Some(code) = &self.code {
            write!(f, ":{code}")?;
        }
        write!(f, "]")?;
        if let Some(source_name) = &self.source_name {
            write!(f, " {source_name}")?;
        }
        if let Some(span) = self.span {
            write!(f, " {span}")?;
        }
        write!(f, " {}", self.message)?;

        if let (Some(span), Some(source_code)) = (self.span, &self.source_code) {
            if let Some((line_number, line_text)) = source_line(source_code, span.line) {
                write!(f, "\n{:>4} | {}", line_number, line_text)?;
                write!(f, "\n     | ")?;
                for _ in 1..span.column {
                    write!(f, " ")?;
                }
                write!(f, "^")?;
            }
        }

        if let Some(hint) = &self.hint {
            write!(f, "\nhint: {hint}")?;
        }

        for note in &self.notes {
            write!(f, "\nnote: {note}")?;
        }

        if !self.trace.is_empty() {
            write!(f, "\ntrace:")?;
            for frame in self.trace.iter().rev() {
                write!(f, "\n  at {frame}")?;
            }
        }

        Ok(())
    }
}

impl std::error::Error for BoryError {}

fn source_line(source_code: &str, target_line: usize) -> Option<(usize, String)> {
    let line = source_code.lines().nth(target_line.saturating_sub(1))?;
    Some((target_line, line.replace('\t', "    ")))
}

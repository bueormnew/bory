use crate::error::BoryError;
use crate::span::Span;
use crate::token::{Token, TokenKind};

pub fn tokenize(source: &str) -> Result<Vec<Token>, BoryError> {
    Lexer::new(source).tokenize()
}

struct Lexer {
    chars: Vec<char>,
    current: usize,
    line: usize,
    column: usize,
    line_start: bool,
    indent_stack: Vec<usize>,
}

impl Lexer {
    fn new(source: &str) -> Self {
        Self {
            chars: source.chars().collect(),
            current: 0,
            line: 1,
            column: 1,
            line_start: true,
            indent_stack: vec![0],
        }
    }

    fn tokenize(&mut self) -> Result<Vec<Token>, BoryError> {
        let mut tokens = Vec::new();

        while !self.is_at_end() {
            if self.line_start {
                self.handle_line_start(&mut tokens)?;
                if self.is_at_end() {
                    break;
                }
            }

            let span = Span::new(self.line, self.column);
            let ch = self.advance();

            match ch {
                ' ' | '\t' | '\r' => {}
                '\n' => {
                    tokens.push(Token::new(TokenKind::Newline, "\n", span));
                    self.line_start = true;
                }
                '#' => self.skip_comment(),
                '(' => tokens.push(Token::new(TokenKind::LeftParen, "(", span)),
                ')' => tokens.push(Token::new(TokenKind::RightParen, ")", span)),
                '{' => tokens.push(Token::new(TokenKind::LeftBrace, "{", span)),
                '}' => tokens.push(Token::new(TokenKind::RightBrace, "}", span)),
                '[' => tokens.push(Token::new(TokenKind::LeftBracket, "[", span)),
                ']' => tokens.push(Token::new(TokenKind::RightBracket, "]", span)),
                ',' => tokens.push(Token::new(TokenKind::Comma, ",", span)),
                '.' => tokens.push(Token::new(TokenKind::Dot, ".", span)),
                ':' => tokens.push(Token::new(TokenKind::Colon, ":", span)),
                ';' => tokens.push(Token::new(TokenKind::Semicolon, ";", span)),
                '+' => {
                    if self.match_char('=') {
                        tokens.push(Token::new(TokenKind::PlusEqual, "+=", span));
                    } else {
                        tokens.push(Token::new(TokenKind::Plus, "+", span));
                    }
                }
                '-' => {
                    if self.match_char('=') {
                        tokens.push(Token::new(TokenKind::MinusEqual, "-=", span));
                    } else if self.match_char('>') {
                        tokens.push(Token::new(TokenKind::Arrow, "->", span));
                    } else {
                        tokens.push(Token::new(TokenKind::Minus, "-", span));
                    }
                }
                '*' => {
                    if self.match_char('=') {
                        tokens.push(Token::new(TokenKind::StarEqual, "*=", span));
                    } else {
                        tokens.push(Token::new(TokenKind::Star, "*", span));
                    }
                }
                '/' => {
                    if self.match_char('=') {
                        tokens.push(Token::new(TokenKind::SlashEqual, "/=", span));
                    } else {
                        tokens.push(Token::new(TokenKind::Slash, "/", span));
                    }
                }
                '%' => {
                    if self.match_char('=') {
                        tokens.push(Token::new(TokenKind::PercentEqual, "%=", span));
                    } else {
                        tokens.push(Token::new(TokenKind::Percent, "%", span));
                    }
                }
                '^' => tokens.push(Token::new(TokenKind::Caret, "^", span)),
                '=' => {
                    if self.match_char('=') {
                        tokens.push(Token::new(TokenKind::EqualEqual, "==", span));
                    } else if self.match_char('>') {
                        tokens.push(Token::new(TokenKind::FatArrow, "=>", span));
                    } else {
                        tokens.push(Token::new(TokenKind::Equal, "=", span));
                    }
                }
                '!' => {
                    if self.match_char('=') {
                        tokens.push(Token::new(TokenKind::BangEqual, "!=", span));
                    } else {
                        return Err(BoryError::lex(
                            "Expected '=' after '!' to form '!='",
                            span,
                        ));
                    }
                }
                '>' => {
                    if self.match_char('=') {
                        tokens.push(Token::new(TokenKind::GreaterEqual, ">=", span));
                    } else {
                        tokens.push(Token::new(TokenKind::Greater, ">", span));
                    }
                }
                '<' => {
                    if self.match_char('=') {
                        tokens.push(Token::new(TokenKind::LessEqual, "<=", span));
                    } else {
                        tokens.push(Token::new(TokenKind::Less, "<", span));
                    }
                }
                '"' | '\'' => tokens.push(self.string(ch, span)?),
                ch if ch.is_ascii_digit() => tokens.push(self.number(ch, span)),
                ch if is_identifier_start(ch) => tokens.push(self.identifier(ch, span)),
                other => {
                    return Err(BoryError::lex(
                        format!("Unexpected character '{other}'"),
                        span,
                    ));
                }
            }
        }

        while self.indent_stack.len() > 1 {
            self.indent_stack.pop();
            tokens.push(Token::new(
                TokenKind::Dedent,
                "<dedent>",
                Span::new(self.line, self.column),
            ));
        }

        tokens.push(Token::new(
            TokenKind::Eof,
            "",
            Span::new(self.line, self.column),
        ));

        Ok(tokens)
    }

    fn handle_line_start(&mut self, tokens: &mut Vec<Token>) -> Result<(), BoryError> {
        self.line_start = false;
        let mut indent = 0usize;

        while let Some(ch) = self.peek() {
            match ch {
                ' ' => {
                    self.advance();
                    indent += 1;
                }
                '\t' => {
                    self.advance();
                    indent += 4;
                }
                '\r' => {
                    self.advance();
                }
                _ => break,
            }
        }

        if self.is_at_end() || self.peek() == Some('\n') || self.peek() == Some('#') {
            return Ok(());
        }

        let current_indent = *self.indent_stack.last().unwrap_or(&0);
        if indent > current_indent {
            self.indent_stack.push(indent);
            tokens.push(Token::new(
                TokenKind::Indent,
                "<indent>",
                Span::new(self.line, 1),
            ));
            return Ok(());
        }

        if indent < current_indent {
            while indent < *self.indent_stack.last().unwrap_or(&0) {
                self.indent_stack.pop();
                tokens.push(Token::new(
                    TokenKind::Dedent,
                    "<dedent>",
                    Span::new(self.line, 1),
                ));
            }

            if indent != *self.indent_stack.last().unwrap_or(&0) {
                return Err(BoryError::lex(
                    "Inconsistent indentation level",
                    Span::new(self.line, 1),
                ));
            }
        }

        Ok(())
    }

    fn string(&mut self, quote: char, span: Span) -> Result<Token, BoryError> {
        let mut value = String::new();

        while let Some(ch) = self.peek() {
            if ch == quote {
                self.advance();
                return Ok(Token::new(TokenKind::String, value, span));
            }

            if ch == '\\' {
                self.advance();
                let escaped = self.peek().ok_or_else(|| {
                    BoryError::lex("Unterminated string", Span::new(self.line, self.column))
                })?;
                self.advance();
                value.push(match escaped {
                    'n' => '\n',
                    'r' => '\r',
                    't' => '\t',
                    '\\' => '\\',
                    '"' => '"',
                    '\'' => '\'',
                    other => other,
                });
            } else {
                value.push(self.advance());
            }
        }

        Err(BoryError::lex("Unterminated string", span))
    }

    fn number(&mut self, first: char, span: Span) -> Token {
        let mut text = String::from(first);

        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                text.push(self.advance());
            } else {
                break;
            }
        }

        if self.peek() == Some('.') && self.peek_next().is_some_and(|ch| ch.is_ascii_digit()) {
            text.push(self.advance());
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    text.push(self.advance());
                } else {
                    break;
                }
            }
        }

        Token::new(TokenKind::Number, text, span)
    }

    fn identifier(&mut self, first: char, span: Span) -> Token {
        let mut text = String::from(first);

        while let Some(ch) = self.peek() {
            if is_identifier_continue(ch) {
                text.push(self.advance());
            } else {
                break;
            }
        }

        let kind = match text.as_str() {
            "and" => TokenKind::And,
            "or" => TokenKind::Or,
            "not" => TokenKind::Not,
            "if" => TokenKind::If,
            "elif" => TokenKind::Elif,
            "else" => TokenKind::Else,
            "while" => TokenKind::While,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "from" => TokenKind::From,
            "to" => TokenKind::To,
            "step" => TokenKind::Step,
            "task" => TokenKind::Task,
            "struct" => TokenKind::Struct,
            "class" => TokenKind::Class,
            "use" => TokenKind::Use,
            "as" => TokenKind::As,
            "give" => TokenKind::Give,
            "end" => TokenKind::End,
            "var" => TokenKind::Var,
            "stop" => TokenKind::Stop,
            "skip" => TokenKind::Skip,
            "load" => TokenKind::Load,
            "yes" => TokenKind::Yes,
            "no" => TokenKind::No,
            "nil" => TokenKind::Nil,
            _ => TokenKind::Identifier,
        };

        Token::new(kind, text, span)
    }

    fn skip_comment(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn is_at_end(&self) -> bool {
        self.current >= self.chars.len()
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.current).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.current + 1).copied()
    }

    fn advance(&mut self) -> char {
        let ch = self.chars[self.current];
        self.current += 1;

        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }

        ch
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }
}

fn is_identifier_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_identifier_continue(ch: char) -> bool {
    is_identifier_start(ch) || ch.is_ascii_digit()
}

//! Subset lexer and parser for webgl-mock.
//!
//! The accepted grammar is intentionally small. Khronos glslang remains the
//! accept/reject reference for real ESSL 3.00; this parser does not call it,
//! and Naga's desktop GLSL frontend is not used.

use std::error::Error;
use std::fmt;

/// Byte range into the original ESSL source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    pub fn slice(self, source: &str) -> Option<&str> {
        source.get(self.start..self.end)
    }

    /// 1-based line and character column of `start`.
    ///
    /// Columns count Unicode scalar values, matching the editor caret. A start
    /// index inside a multibyte character moves back to that character.
    pub fn line_column(self, source: &str) -> (u32, u32) {
        let mut start = self.start.min(source.len());
        while start > 0 && !source.is_char_boundary(start) {
            start -= 1;
        }
        let prefix = &source[..start];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() as u32 + 1;
        let line_start = prefix.rfind('\n').map(|index| index + 1).unwrap_or(0);
        let column = source[line_start..start].chars().count() as u32 + 1;
        (line, column)
    }
}

/// Lex or parse failure. `span` is the source map hook for later diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontendError {
    pub message: String,
    pub span: SourceSpan,
}

impl fmt::Display for FrontendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} (bytes {}..{})",
            self.message, self.span.start, self.span.end
        )
    }
}

impl Error for FrontendError {}

#[derive(Clone, Debug)]
pub struct TranslationUnit {
    pub version_span: SourceSpan,
    pub uniforms: Vec<InterfaceSymbol>,
    pub inputs: Vec<InterfaceSymbol>,
    pub outputs: Vec<InterfaceSymbol>,
    pub functions: Vec<Function>,
}

#[derive(Clone, Debug)]
pub struct InterfaceSymbol {
    pub name: String,
    pub span: SourceSpan,
}

#[derive(Clone, Debug)]
pub struct Function {
    pub name: String,
    pub span: SourceSpan,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug)]
pub enum Statement {
    Declare {
        name: String,
        value: Expr,
        span: SourceSpan,
    },
    Assign {
        name: String,
        value: Expr,
        span: SourceSpan,
    },
}

#[derive(Clone, Debug)]
pub enum Expr {
    Float {
        bits: u32,
        span: SourceSpan,
    },
    Name {
        name: String,
        span: SourceSpan,
    },
    /// One multiplication. The operands are a name or a float literal.
    Mul {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: SourceSpan,
    },
}

/// Parse the CPU ESSL subset.
///
/// ```text
/// #version 300 es
/// uniform float u_scale;
/// in float v_in;
/// out float v_out;
/// void main() {
///   float x = a_x * u_scale;
///   v_clip_x = x;
/// }
/// ```
///
/// One scalar multiplication is accepted. Constructors, swizzles, `+`,
/// control flow, and any version other than `300 es` are rejected.
pub fn parse_essl_subset(source: &str) -> Result<TranslationUnit, FrontendError> {
    let tokens = lex(source)?;
    Parser::new(tokens).parse_unit()
}

const KEYWORDS: &[&str] = &["void", "float", "uniform", "in", "out", "version"];

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    span: SourceSpan,
}

#[derive(Clone, Debug)]
enum TokenKind {
    Ident(String),
    Int(i64),
    Float(f32),
    Hash,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Semicolon,
    Equals,
    Star,
    Newline,
    Eof,
}

struct Lexer<'a> {
    source: &'a str,
    index: usize,
}

fn lex(source: &str) -> Result<Vec<Token>, FrontendError> {
    let mut lexer = Lexer { source, index: 0 };
    let mut tokens = Vec::new();
    loop {
        lexer.skip_ignorable()?;
        if lexer.eof() {
            tokens.push(Token {
                kind: TokenKind::Eof,
                span: SourceSpan {
                    start: lexer.index,
                    end: lexer.index,
                },
            });
            break;
        }
        if lexer.starts_with('\n') || lexer.starts_with('\r') {
            let start = lexer.index;
            lexer.consume_newline();
            tokens.push(Token {
                kind: TokenKind::Newline,
                span: SourceSpan {
                    start,
                    end: lexer.index,
                },
            });
            continue;
        }
        tokens.push(lexer.next_token()?);
    }
    Ok(tokens)
}

impl<'a> Lexer<'a> {
    fn eof(&self) -> bool {
        self.index >= self.source.len()
    }

    fn rest(&self) -> &'a str {
        &self.source[self.index..]
    }

    fn starts_with(&self, needle: char) -> bool {
        self.rest().starts_with(needle)
    }

    fn skip_ignorable(&mut self) -> Result<(), FrontendError> {
        loop {
            let rest = self.rest();
            if rest.starts_with("//") {
                self.index += 2;
                while !self.eof() && !self.starts_with('\n') && !self.starts_with('\r') {
                    self.bump_char();
                }
                continue;
            }
            if rest.starts_with("/*") {
                let start = self.index;
                self.index += 2;
                let Some(relative) = self.rest().find("*/") else {
                    return Err(FrontendError {
                        message: "unterminated block comment".to_string(),
                        span: SourceSpan {
                            start,
                            end: self.source.len(),
                        },
                    });
                };
                self.index += relative + 2;
                continue;
            }
            if self.starts_with(' ') || self.starts_with('\t') {
                self.bump_char();
                continue;
            }
            break;
        }
        Ok(())
    }

    fn consume_newline(&mut self) {
        if self.rest().starts_with("\r\n") {
            self.index += 2;
        } else if self.starts_with('\n') || self.starts_with('\r') {
            self.bump_char();
        }
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.rest().chars().next()?;
        self.index += ch.len_utf8();
        Some(ch)
    }

    fn next_token(&mut self) -> Result<Token, FrontendError> {
        let start = self.index;
        let Some(ch) = self.rest().chars().next() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                span: SourceSpan { start, end: start },
            });
        };
        let kind = match ch {
            '#' => {
                self.bump_char();
                TokenKind::Hash
            }
            '(' => {
                self.bump_char();
                TokenKind::LParen
            }
            ')' => {
                self.bump_char();
                TokenKind::RParen
            }
            '{' => {
                self.bump_char();
                TokenKind::LBrace
            }
            '}' => {
                self.bump_char();
                TokenKind::RBrace
            }
            ';' => {
                self.bump_char();
                TokenKind::Semicolon
            }
            '=' => {
                self.bump_char();
                TokenKind::Equals
            }
            '*' => {
                self.bump_char();
                TokenKind::Star
            }
            '0'..='9' | '.' => return self.lex_number(start),
            'A'..='Z' | 'a'..='z' | '_' => return Ok(self.lex_ident(start)),
            _ => {
                return Err(FrontendError {
                    message: format!("character {ch:?} is outside the ESSL subset lexer"),
                    span: SourceSpan {
                        start,
                        end: start + ch.len_utf8(),
                    },
                });
            }
        };
        Ok(Token {
            kind,
            span: SourceSpan {
                start,
                end: self.index,
            },
        })
    }

    fn lex_ident(&mut self, start: usize) -> Token {
        while self
            .rest()
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        {
            self.bump_char();
        }
        Token {
            kind: TokenKind::Ident(self.source[start..self.index].to_string()),
            span: SourceSpan {
                start,
                end: self.index,
            },
        }
    }

    fn lex_number(&mut self, start: usize) -> Result<Token, FrontendError> {
        let bytes = self.source.as_bytes();
        let mut index = self.index;
        let mut saw_dot = false;
        let mut saw_exp = false;
        if bytes.get(index) == Some(&b'.') {
            if !bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
                return Err(FrontendError {
                    message: "'.' is outside the ESSL subset lexer".to_string(),
                    span: SourceSpan {
                        start,
                        end: start + 1,
                    },
                });
            }
            saw_dot = true;
            index += 1;
        }
        while bytes.get(index).is_some_and(u8::is_ascii_digit) {
            index += 1;
        }
        if !saw_dot && bytes.get(index) == Some(&b'.') {
            saw_dot = true;
            index += 1;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
        }
        if bytes
            .get(index)
            .is_some_and(|byte| *byte == b'e' || *byte == b'E')
        {
            let exponent = index;
            index += 1;
            if bytes
                .get(index)
                .is_some_and(|byte| *byte == b'+' || *byte == b'-')
            {
                index += 1;
            }
            let digits = index;
            while bytes.get(index).is_some_and(u8::is_ascii_digit) {
                index += 1;
            }
            if index == digits {
                return Err(FrontendError {
                    message: "float literal is missing an exponent".to_string(),
                    span: SourceSpan {
                        start,
                        end: exponent + 1,
                    },
                });
            }
            saw_exp = true;
        }
        let text = &self.source[start..index];
        self.index = index;
        let span = SourceSpan { start, end: index };
        if saw_dot || saw_exp {
            let value: f32 = text.parse().map_err(|_| FrontendError {
                message: format!("cannot parse float literal `{text}`"),
                span,
            })?;
            if !value.is_finite() {
                return Err(FrontendError {
                    message: format!("float literal `{text}` is not finite"),
                    span,
                });
            }
            Ok(Token {
                kind: TokenKind::Float(value),
                span,
            })
        } else {
            let value: i64 = text.parse().map_err(|_| FrontendError {
                message: format!("cannot parse integer literal `{text}`"),
                span,
            })?;
            Ok(Token {
                kind: TokenKind::Int(value),
                span,
            })
        }
    }
}

struct Parser {
    tokens: Vec<Token>,
    cursor: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, cursor: 0 }
    }

    fn parse_unit(&mut self) -> Result<TranslationUnit, FrontendError> {
        self.skip_newlines();
        let version_span = self.parse_version()?;
        let mut uniforms = Vec::new();
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut functions = Vec::new();
        let mut seen_function = false;
        loop {
            self.skip_newlines();
            if matches!(self.peek().kind, TokenKind::Eof) {
                break;
            }
            let word = self
                .peek_ident()
                .ok_or_else(|| self.error("expected uniform, in, out, or void"))?
                .to_string();
            match word.as_str() {
                "uniform" | "in" | "out" if seen_function => {
                    return Err(self.error(
                        "interface declarations in this subset must come before functions",
                    ));
                }
                "uniform" => uniforms.push(self.parse_interface()?),
                "in" => inputs.push(self.parse_interface()?),
                "out" => outputs.push(self.parse_interface()?),
                "void" => {
                    seen_function = true;
                    functions.push(self.parse_function()?);
                }
                _ => {
                    return Err(self.error("expected uniform, in, out, or void"));
                }
            }
        }
        if functions.is_empty() {
            return Err(self.error("expected at least one void function"));
        }
        check_unique_names(&uniforms, &inputs, &outputs, &functions)?;
        Ok(TranslationUnit {
            version_span,
            uniforms,
            inputs,
            outputs,
            functions,
        })
    }

    fn parse_version(&mut self) -> Result<SourceSpan, FrontendError> {
        let start = self.peek().span.start;
        if !matches!(self.peek().kind, TokenKind::Hash) {
            return Err(self.error("ESSL subset requires #version 300 es before any declaration"));
        }
        self.bump();
        self.expect_word("version")?;
        self.expect_int(300)?;
        let es = self.expect_word("es")?;
        match self.peek().kind {
            TokenKind::Newline => {
                self.bump();
            }
            TokenKind::Eof => {}
            _ => {
                return Err(self.error("trailing tokens after #version 300 es"));
            }
        }
        Ok(SourceSpan {
            start,
            end: es.span.end,
        })
    }

    fn parse_interface(&mut self) -> Result<InterfaceSymbol, FrontendError> {
        let start = self.peek().span.start;
        self.bump();
        self.expect_word("float")?;
        let name = self.expect_name()?;
        let semi = self.expect_semicolon()?;
        Ok(InterfaceSymbol {
            name,
            span: SourceSpan {
                start,
                end: semi.span.end,
            },
        })
    }

    fn parse_function(&mut self) -> Result<Function, FrontendError> {
        let start = self.peek().span.start;
        self.expect_word("void")?;
        let name = self.expect_name()?;
        self.expect_kind(TokenKind::LParen, "`(`")?;
        self.expect_kind(TokenKind::RParen, "`)`")?;
        self.expect_kind(TokenKind::LBrace, "`{`")?;
        self.skip_newlines();
        let mut body = Vec::new();
        while !matches!(self.peek().kind, TokenKind::RBrace | TokenKind::Eof) {
            body.push(self.parse_statement()?);
            self.skip_newlines();
        }
        let end = self.expect_kind(TokenKind::RBrace, "`}`")?;
        Ok(Function {
            name,
            span: SourceSpan {
                start,
                end: end.span.end,
            },
            body,
        })
    }

    fn parse_statement(&mut self) -> Result<Statement, FrontendError> {
        let start = self.peek().span.start;
        if self.peek_word("float") {
            self.bump();
            let name = self.expect_name()?;
            self.expect_kind(TokenKind::Equals, "`=`")?;
            let value = self.parse_expr()?;
            let semi = self.expect_semicolon()?;
            return Ok(Statement::Declare {
                name,
                value,
                span: SourceSpan {
                    start,
                    end: semi.span.end,
                },
            });
        }
        let name = self.expect_name()?;
        self.expect_kind(TokenKind::Equals, "`=`")?;
        let value = self.parse_expr()?;
        let semi = self.expect_semicolon()?;
        Ok(Statement::Assign {
            name,
            value,
            span: SourceSpan {
                start,
                end: semi.span.end,
            },
        })
    }

    fn parse_expr(&mut self) -> Result<Expr, FrontendError> {
        let lhs = self.parse_primary()?;
        if !matches!(self.peek().kind, TokenKind::Star) {
            return Ok(lhs);
        }
        self.bump();
        let rhs = self.parse_primary()?;
        if matches!(self.peek().kind, TokenKind::Star) {
            return Err(self.error("only one multiplication is in this subset"));
        }
        let span = SourceSpan {
            start: expr_start(&lhs),
            end: expr_end(&rhs),
        };
        Ok(Expr::Mul {
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            span,
        })
    }

    fn parse_primary(&mut self) -> Result<Expr, FrontendError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Float(value) => {
                self.bump();
                Ok(Expr::Float {
                    bits: value.to_bits(),
                    span: token.span,
                })
            }
            TokenKind::Ident(name) if !KEYWORDS.contains(&name.as_str()) => {
                self.bump();
                Ok(Expr::Name {
                    name,
                    span: token.span,
                })
            }
            _ => Err(self.error("expected a float literal or a name")),
        }
    }

    fn expect_word(&mut self, word: &str) -> Result<Token, FrontendError> {
        if self.peek_word(word) {
            Ok(self.bump())
        } else {
            Err(self.error(format!("expected `{word}`")))
        }
    }

    fn expect_name(&mut self) -> Result<String, FrontendError> {
        let token = self.peek().clone();
        match token.kind {
            TokenKind::Ident(name) if !KEYWORDS.contains(&name.as_str()) => {
                self.bump();
                Ok(name)
            }
            TokenKind::Ident(_) => Err(self.error("expected a name, found a keyword")),
            _ => Err(self.error("expected a name")),
        }
    }

    fn expect_int(&mut self, expected: i64) -> Result<(), FrontendError> {
        match &self.peek().kind {
            TokenKind::Int(value) if *value == expected => {
                self.bump();
                Ok(())
            }
            TokenKind::Int(value) => {
                let value = *value;
                Err(self.error(format!("expected {expected}, found {value}")))
            }
            _ => Err(self.error(format!("expected integer {expected}"))),
        }
    }

    fn expect_semicolon(&mut self) -> Result<Token, FrontendError> {
        self.expect_kind(TokenKind::Semicolon, "`;`")
    }

    fn expect_kind(&mut self, kind: TokenKind, label: &str) -> Result<Token, FrontendError> {
        if token_kind_eq(&self.peek().kind, &kind) {
            Ok(self.bump())
        } else {
            Err(self.error(format!("expected {label}")))
        }
    }

    fn peek_word(&self, word: &str) -> bool {
        matches!(&self.peek().kind, TokenKind::Ident(name) if name == word)
    }

    fn peek_ident(&self) -> Option<&str> {
        match &self.peek().kind {
            TokenKind::Ident(name) => Some(name),
            _ => None,
        }
    }

    fn skip_newlines(&mut self) {
        while matches!(self.peek().kind, TokenKind::Newline) {
            self.bump();
        }
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.cursor]
    }

    fn bump(&mut self) -> Token {
        let token = self.peek().clone();
        if !matches!(token.kind, TokenKind::Eof) {
            self.cursor += 1;
        }
        token
    }

    fn error(&self, message: impl Into<String>) -> FrontendError {
        FrontendError {
            message: message.into(),
            span: self.peek().span,
        }
    }
}

fn token_kind_eq(left: &TokenKind, right: &TokenKind) -> bool {
    matches!(
        (left, right),
        (TokenKind::Hash, TokenKind::Hash)
            | (TokenKind::LParen, TokenKind::LParen)
            | (TokenKind::RParen, TokenKind::RParen)
            | (TokenKind::LBrace, TokenKind::LBrace)
            | (TokenKind::RBrace, TokenKind::RBrace)
            | (TokenKind::Semicolon, TokenKind::Semicolon)
            | (TokenKind::Equals, TokenKind::Equals)
            | (TokenKind::Star, TokenKind::Star)
            | (TokenKind::Newline, TokenKind::Newline)
            | (TokenKind::Eof, TokenKind::Eof)
    )
}

fn expr_start(expr: &Expr) -> usize {
    match expr {
        Expr::Float { span, .. } | Expr::Name { span, .. } | Expr::Mul { span, .. } => span.start,
    }
}

fn expr_end(expr: &Expr) -> usize {
    match expr {
        Expr::Float { span, .. } | Expr::Name { span, .. } | Expr::Mul { span, .. } => span.end,
    }
}

fn check_unique_names(
    uniforms: &[InterfaceSymbol],
    inputs: &[InterfaceSymbol],
    outputs: &[InterfaceSymbol],
    functions: &[Function],
) -> Result<(), FrontendError> {
    let mut names = Vec::<(&str, SourceSpan)>::new();
    for symbol in uniforms.iter().chain(inputs).chain(outputs) {
        if names.iter().any(|(name, _)| *name == symbol.name) {
            return Err(FrontendError {
                message: format!("duplicate interface name `{}`", symbol.name),
                span: symbol.span,
            });
        }
        names.push((&symbol.name, symbol.span));
    }
    let mut function_names = Vec::new();
    for function in functions {
        if function_names.contains(&function.name.as_str()) {
            return Err(FrontendError {
                message: format!("duplicate function `{}`", function.name),
                span: function.span,
            });
        }
        function_names.push(function.name.as_str());
        let mut locals = Vec::new();
        for statement in &function.body {
            if let Statement::Declare { name, span, .. } = statement {
                if names.iter().any(|(existing, _)| *existing == name) || locals.contains(name) {
                    return Err(FrontendError {
                        message: format!("duplicate name `{name}`"),
                        span: *span,
                    });
                }
                locals.push(name.clone());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../examples/webgl2-triangle/float_local.essl");

    #[test]
    fn parses_the_demo_fixture_with_source_spans() {
        let unit = parse_essl_subset(SAMPLE).expect("fixture parses");
        assert_eq!(unit.version_span.slice(SAMPLE), Some("#version 300 es"));
        assert_eq!(unit.uniforms.len(), 1);
        assert_eq!(unit.inputs.len(), 1);
        assert_eq!(unit.outputs.len(), 1);
        assert_eq!(
            unit.uniforms[0].span.slice(SAMPLE),
            Some("uniform float u_scale;")
        );
        assert_eq!(unit.functions[0].name, "main");
        assert_eq!(unit.functions[0].body.len(), 2);
        let Statement::Declare { span, .. } = &unit.functions[0].body[0] else {
            panic!("first statement should declare a local");
        };
        assert_eq!(span.slice(SAMPLE), Some("float x = 1.0;"));
    }

    #[test]
    fn comments_are_ignored_around_an_empty_main() {
        let source = "#version 300 es\n// keep\nvoid main() {\n  /* nothing */\n}\n";
        let unit = parse_essl_subset(source).expect("comments");
        assert!(unit.functions[0].body.is_empty());
    }

    #[test]
    fn line_column_counts_unicode_scalars_on_the_source_line() {
        let source = "a\n中x";
        let span = SourceSpan { start: 5, end: 6 };

        assert_eq!(span.line_column(source), (2, 2));
    }

    #[test]
    fn rejects_a_version_other_than_300_es() {
        let error = parse_essl_subset("#version 330\nvoid main() {}\n").expect_err("desktop glsl");
        assert!(error.message.contains("300"));
        let error = parse_essl_subset("#version 300\nvoid main() {}\n").expect_err("missing es");
        assert!(error.message.contains("es"));
        let error = parse_essl_subset("void main() {}\n").expect_err("missing version");
        assert!(error.message.contains("#version 300 es"));
    }

    #[test]
    fn one_multiplication_keeps_a_source_span() {
        let source = "#version 300 es\nvoid main() { float x = 2.0 * 3.0; }\n";
        let unit = parse_essl_subset(source).expect("mul");
        let Statement::Declare { value, .. } = &unit.functions[0].body[0] else {
            panic!("declaration");
        };
        let Expr::Mul { span, .. } = value else {
            panic!("multiplication");
        };
        assert_eq!(span.slice(source), Some("2.0 * 3.0"));
        let (line, column) = span.line_column(source);
        assert_eq!(line, 2);
        let line_text = source.lines().nth(1).expect("source line");
        let start = column as usize - 1;
        assert_eq!(&line_text[start..start + 3], "2.0");
        let chained = "#version 300 es\nvoid main() { float x = 2.0 * 3.0 * 4.0; }\n";
        let error = parse_essl_subset(chained).expect_err("two muls");
        assert!(error.message.contains("one multiplication"));
    }
}

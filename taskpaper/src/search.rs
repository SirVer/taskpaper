//! Implements a simple parser and interpreter for search DSL for tasks.
//!
//! expression => or;
//! or         => and ( "or" and )*;
//! and        => comparison ( "and" comparison )*;
//! comparison => unary ( ("==" | "!=" | "<" | "<=" | ">" | ">=") unary )*
//! unary      => "not" unary
//!             | primary;
//! primary    => STRING | "false" | "true" | "(" expression ")";

use crate::{Error, Result, Tags};

// NOCOM(#sirver): remove panics here

// TODO(sirver): No support for ordering or project limiting as of now.
#[derive(Debug, PartialEq, Clone)]
enum TokenKind {
    /// A Tag, optionally with a value
    Tag(String),
    LeftParen,
    RightParen,

    /// One or two character tokens.
    BangEqual,
    Equal,
    EqualEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,

    /// Literals
    String(String),

    // Keywords
    // TODO(sirver): Do we require false and true besides for testing?
    Not,
    And,
    Or,
    True,
    False,

    Eof,
}

#[derive(Debug, PartialEq)]
struct Token {
    kind: TokenKind,

    /// Byte offset where this starts in the lexed input string.
    offset: usize,

    /// Len in bytes of this token in the lexed input string.
    len: usize,
}

impl Token {
    fn new(kind: TokenKind, offset: usize, len: usize) -> Self {
        Token { kind, offset, len }
    }
}

#[derive(Debug)]
pub enum Expr {
    Tag(String),
    Grouping(Box<Expr>),

    NotEqual(Box<Expr>, Box<Expr>),
    Equal(Box<Expr>, Box<Expr>),
    Greater(Box<Expr>, Box<Expr>),
    GreaterEqual(Box<Expr>, Box<Expr>),
    Less(Box<Expr>, Box<Expr>),
    LessEqual(Box<Expr>, Box<Expr>),

    String(String),

    Not(Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    True,
    False,
}

#[derive(Debug, PartialEq)]
pub enum Value {
    Undefined, // Missing tag
    Bool(bool),
    String(String),
}

// NOCOM(#sirver): should not panic, but return a meaningful error
impl Value {
    fn not(&self) -> Value {
        match self {
            Value::Undefined => Value::Bool(true),
            Value::String(_) => Value::Bool(false),
            Value::Bool(a) => Value::Bool(!a),
        }
    }

    pub fn is_truish(&self) -> bool {
        match self {
            Value::Undefined => false,
            Value::Bool(b) => *b,
            Value::String(_) => true,
        }
    }

    fn equal(&self, o: &Value) -> Value {
        Value::Bool(*self == *o)
    }

    fn or(self, o: Value) -> Value {
        if self.is_truish() {
            self
        } else if o.is_truish() {
            o
        } else {
            Value::Bool(false)
        }
    }

    fn and(self, o: Value) -> Value {
        if !self.is_truish() {
            self
        } else if o.is_truish() {
            o
        } else {
            Value::Bool(false)
        }
    }

    fn less(self, o: Value) -> Value {
        match (self, o) {
            (_, Value::Undefined) | (Value::Undefined, _) => Value::Undefined,
            (Value::Bool(_), Value::String(_)) => Value::Undefined,
            (Value::String(_), Value::Bool(_)) => Value::Undefined,
            (Value::Bool(a), Value::Bool(b)) => Value::Bool(a < b),
            (Value::String(a), Value::String(b)) => Value::Bool(a < b),
        }
    }

    fn less_equal(self, o: Value) -> Value {
        match (self, o) {
            (_, Value::Undefined) | (Value::Undefined, _) => Value::Undefined,
            (Value::Bool(_), Value::String(_)) => Value::Undefined,
            (Value::String(_), Value::Bool(_)) => Value::Undefined,
            (Value::Bool(a), Value::Bool(b)) => Value::Bool(a <= b),
            (Value::String(a), Value::String(b)) => Value::Bool(a <= b),
        }
    }

    fn greater(self, o: Value) -> Value {
        match (self, o) {
            (_, Value::Undefined) | (Value::Undefined, _) => Value::Undefined,
            (Value::Bool(_), Value::String(_)) => Value::Undefined,
            (Value::String(_), Value::Bool(_)) => Value::Undefined,
            (Value::Bool(a), Value::Bool(b)) => Value::Bool(a > b),
            (Value::String(a), Value::String(b)) => Value::Bool(a > b),
        }
    }

    fn greater_equal(self, o: Value) -> Value {
        match (self, o) {
            (_, Value::Undefined) | (Value::Undefined, _) => Value::Undefined,
            (Value::Bool(_), Value::String(_)) => Value::Undefined,
            (Value::String(_), Value::Bool(_)) => Value::Undefined,
            (Value::Bool(a), Value::Bool(b)) => Value::Bool(a >= b),
            (Value::String(a), Value::String(b)) => Value::Bool(a >= b),
        }
    }
}

impl Expr {
    pub fn parse(text: &str) -> Result<Expr> {
        let tokens = lex(text)?;
        let mut parser = Parser::new(tokens);
        Ok(*parser.expression())
    }

    pub fn evaluate(&self, tags: &Tags) -> Value {
        match self {
            Expr::Tag(name) => match tags.get(name) {
                Some(tag) => match tag.value {
                    Some(value) => Value::String(value),
                    None => Value::Bool(true),
                },
                None => Value::Undefined,
            },
            Expr::String(name) => Value::String(name.to_string()),
            Expr::Grouping(inner) => inner.evaluate(tags),
            Expr::NotEqual(l, r) => l.evaluate(tags).equal(&r.evaluate(tags)).not(),
            Expr::Equal(l, r) => l.evaluate(tags).equal(&r.evaluate(tags)),
            Expr::Greater(l, r) => l.evaluate(tags).greater(r.evaluate(tags)),
            Expr::GreaterEqual(l, r) => l.evaluate(tags).greater_equal(r.evaluate(tags)),
            Expr::Less(l, r) => l.evaluate(tags).less(r.evaluate(tags)),
            Expr::LessEqual(l, r) => l.evaluate(tags).less_equal(r.evaluate(tags)),
            Expr::Not(e) => e.evaluate(tags).not(),
            Expr::And(l, r) => l.evaluate(tags).and(r.evaluate(tags)),
            Expr::Or(l, r) => l.evaluate(tags).or(r.evaluate(tags)),
            Expr::True => Value::Bool(true),
            Expr::False => Value::Bool(false),
        }
    }
}

pub struct Parser {
    current: usize,
    tokens: Vec<Token>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, current: 0 }
    }

    fn expression(&mut self) -> Box<Expr> {
        self.or()
    }

    fn or(&mut self) -> Box<Expr> {
        let mut expr = self.and();
        while self.match_oneof(&[TokenKind::Or]) {
            let right = self.and();
            expr = Box::new(Expr::Or(expr, right));
        }
        expr
    }

    fn and(&mut self) -> Box<Expr> {
        let mut expr = self.comparison();
        while self.match_oneof(&[TokenKind::And]) {
            let right = self.comparison();
            expr = Box::new(Expr::And(expr, right));
        }
        expr
    }

    fn comparison(&mut self) -> Box<Expr> {
        let mut expr = self.unary();
        while self.match_oneof(&[
            TokenKind::BangEqual,
            TokenKind::Equal,
            TokenKind::EqualEqual,
            TokenKind::Greater,
            TokenKind::GreaterEqual,
            TokenKind::Less,
            TokenKind::LessEqual,
        ]) {
            // TODO(sirver): This is fairly ugly and requires me to keep a copy. It would be better
            // to pass ownership in advance() and previous()
            let prev = self.previous().kind.clone();
            let right = self.unary();
            expr = match prev {
                TokenKind::BangEqual => Box::new(Expr::NotEqual(expr, right)),
                TokenKind::Equal | TokenKind::EqualEqual => Box::new(Expr::Equal(expr, right)),
                TokenKind::Greater => Box::new(Expr::Greater(expr, right)),
                TokenKind::GreaterEqual => Box::new(Expr::GreaterEqual(expr, right)),
                TokenKind::Less => Box::new(Expr::Less(expr, right)),
                TokenKind::LessEqual => Box::new(Expr::LessEqual(expr, right)),
                c => unreachable!("{:?}", c),
            }
        }
        expr
    }

    fn unary(&mut self) -> Box<Expr> {
        if self.match_oneof(&[TokenKind::Not]) {
            let right = self.unary();
            return Box::new(Expr::Not(right));
        }
        self.primary()
    }

    fn primary(&mut self) -> Box<Expr> {
        let token = self.advance();
        match &token.kind {
            TokenKind::False => Box::new(Expr::False),
            TokenKind::True => Box::new(Expr::True),
            TokenKind::Tag(name) => Box::new(Expr::Tag(name.clone())),
            TokenKind::String(string) => Box::new(Expr::String(string.clone())),
            TokenKind::LeftParen => {
                let expr = self.expression();
                if !self.check(&TokenKind::RightParen) {
                    panic!("Expect ')' after expression.");
                };
                self.advance();
                Box::new(Expr::Grouping(expr))
            }
            _ => panic!("Invalid token: {:?}", token.kind),
        }
    }

    fn match_oneof(&mut self, tokens: &[TokenKind]) -> bool {
        for t in tokens.iter() {
            if self.check(t) {
                self.advance();
                return true;
            }
        }
        false
    }

    fn check(&mut self, t: &TokenKind) -> bool {
        if self.is_at_end() {
            false
        } else {
            self.peek().kind == *t
        }
    }

    fn advance(&mut self) -> &Token {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }

    fn is_at_end(&self) -> bool {
        self.peek().kind == TokenKind::Eof
    }

    fn peek(&self) -> &Token {
        &self.tokens[self.current]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.current - 1]
    }
}

#[derive(Debug)]
pub struct CharStream {
    indices: Vec<(usize, char)>,
    current: usize,
}

impl CharStream {
    pub fn new(text: &str) -> Self {
        let indices = text.char_indices().collect();
        CharStream {
            current: 0,
            indices,
        }
    }

    pub fn peek(&mut self) -> Option<char> {
        self.indices.get(self.current).map(|e| e.1)
    }

    pub fn is_next(&mut self, c: char) -> bool {
        if self.peek() == Some(c) {
            self.advance();
            true
        } else {
            false
        }
    }

    pub fn position(&self) -> usize {
        if self.is_at_end() {
            self.indices[self.indices.len() - 1].0 + 1
        } else {
            self.indices[self.current].0
        }
    }

    pub fn advance(&mut self) -> char {
        self.current += 1;
        self.indices[self.current - 1].1
    }

    pub fn is_at_end(&self) -> bool {
        self.current >= self.indices.len()
    }
}

fn is_alpha_numeric(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn lex_keyword(text: &str, start: usize, stream: &mut CharStream) -> Result<Token> {
    loop {
        match stream.peek() {
            Some(c) if is_alpha_numeric(c) => stream.advance(),
            _ => break,
        };
    }

    let len = stream.position() - start;
    let identifier = &text[start..start + len];
    let kind = match identifier {
        "and" => TokenKind::And,
        "false" => TokenKind::False,
        "not" => TokenKind::Not,
        "or" => TokenKind::Or,
        "true" => TokenKind::True,
        _ => {
            return Err(Error::misc(format!(
                "Unexpected identifier: '{}'.",
                identifier
            )))
        }
    };
    Ok(Token::new(kind, start, len))
}

fn lex_string(text: &str, start: usize, stream: &mut CharStream) -> Result<Token> {
    loop {
        match stream.peek() {
            Some(c) if c != '"' => stream.advance(),
            _ => break,
        };
    }

    if stream.is_at_end() {
        return Err(Error::misc("Unterminated string."));
    }

    stream.advance(); // Consumes '"'
    let len = stream.position() - start;
    Ok(Token::new(
        TokenKind::String(text[start + 1..start + len - 1].to_string()),
        start,
        len,
    ))
}

fn lex_tag(text: &str, start: usize, stream: &mut CharStream) -> Result<Token> {
    loop {
        match stream.peek() {
            Some(c) if is_alpha_numeric(c) => stream.advance(),
            _ => break,
        };
    }

    let len = stream.position() - start;
    let identifier = text[start + 1..start + len].to_string();
    Ok(Token::new(TokenKind::Tag(identifier), start, len))
}

fn lex(input: &str) -> Result<Vec<Token>> {
    let mut stream = CharStream::new(input);

    use self::TokenKind::*;

    let mut tokens = Vec::new();
    while !stream.is_at_end() {
        let position = stream.position();
        match stream.advance() {
            '"' => tokens.push(lex_string(input, position, &mut stream)?),
            '@' => tokens.push(lex_tag(input, position, &mut stream)?),
            '(' => tokens.push(Token::new(LeftParen, position, 1)),
            ')' => tokens.push(Token::new(RightParen, position, 1)),
            ' ' | '\t' => (),
            'a'..='z' | 'A'..='Z' => tokens.push(lex_keyword(input, position, &mut stream)?),
            '!' => {
                if stream.is_next('=') {
                    tokens.push(Token::new(BangEqual, position, 2));
                } else {
                    return Err(Error::misc(format!(
                        "Unexpected token: '!'. String continues with: '{}'",
                        &input[position..]
                    )));
                }
            }
            '=' => {
                if stream.is_next('=') {
                    tokens.push(Token::new(EqualEqual, position, 2));
                } else {
                    tokens.push(Token::new(Equal, position, 1));
                }
            }
            '>' => {
                if stream.is_next('=') {
                    tokens.push(Token::new(GreaterEqual, position, 2));
                } else {
                    tokens.push(Token::new(Greater, position, 1));
                }
            }
            '<' => {
                if stream.is_next('=') {
                    tokens.push(Token::new(LessEqual, position, 2));
                } else {
                    tokens.push(Token::new(Less, position, 1));
                }
            }
            c => {
                return Err(Error::misc(format!(
                    "Unexpected token: '{}'. String continues with: '{}'",
                    c,
                    &input[position..]
                )))
            }
        }
    }

    tokens.push(Token::new(Eof, stream.position(), 0));
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::TokenKind::*;
    use super::*;
    use pretty_assertions::assert_eq;

    fn tok(kind: TokenKind) -> Token {
        Token {
            kind,
            offset: 0,
            len: 1,
        }
    }

    #[test]
    fn test_lex() {
        assert_eq!(
            lex("true or false").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(Or, 5, 2),
                Token::new(False, 8, 5),
                Token::new(Eof, 13, 0),
            ]
        );

        assert_eq!(
            lex("true   or (true and ( not false))").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(Or, 7, 2),
                Token::new(LeftParen, 10, 1),
                Token::new(True, 11, 4),
                Token::new(And, 16, 3),
                Token::new(LeftParen, 20, 1),
                Token::new(Not, 22, 3),
                Token::new(False, 26, 5),
                Token::new(RightParen, 31, 1),
                Token::new(RightParen, 32, 1),
                Token::new(Eof, 33, 0),
            ]
        );

        assert_eq!(
            lex("true != true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(BangEqual, 5, 2),
                Token::new(True, 8, 4),
                Token::new(Eof, 12, 0)
            ]
        );
        assert_eq!(
            lex("true = true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(Equal, 5, 1),
                Token::new(True, 7, 4),
                Token::new(Eof, 11, 0)
            ]
        );
        assert_eq!(
            lex("true == true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(EqualEqual, 5, 2),
                Token::new(True, 8, 4),
                Token::new(Eof, 12, 0)
            ]
        );
        assert_eq!(
            lex("true > true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(Greater, 5, 1),
                Token::new(True, 7, 4),
                Token::new(Eof, 11, 0)
            ]
        );
        assert_eq!(
            lex("true >= true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(GreaterEqual, 5, 2),
                Token::new(True, 8, 4),
                Token::new(Eof, 12, 0)
            ]
        );
        assert_eq!(
            lex("true < true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(Less, 5, 1),
                Token::new(True, 7, 4),
                Token::new(Eof, 11, 0)
            ]
        );
        assert_eq!(
            lex("true <= true").unwrap(),
            vec![
                Token::new(True, 0, 4),
                Token::new(LessEqual, 5, 2),
                Token::new(True, 8, 4),
                Token::new(Eof, 12, 0)
            ]
        );

        assert_eq!(
            lex("\"foo\" <= \"super callifragi_listic  \"").unwrap(),
            vec![
                Token::new(String("foo".into()), 0, 5),
                Token::new(LessEqual, 6, 2),
                Token::new(String("super callifragi_listic  ".into()), 9, 27),
                Token::new(Eof, 36, 0)
            ]
        );

        assert_eq!(
            lex("@blub <= @bla").unwrap(),
            vec![
                Token::new(Tag("blub".to_string()), 0, 5),
                Token::new(LessEqual, 6, 2),
                Token::new(Tag("bla".to_string()), 9, 4),
                Token::new(Eof, 13, 0)
            ]
        );
    }

    #[test]
    fn test_expr_or() {
        let tags = Tags::new();
        assert_eq!(
            Value::Bool(false),
            Parser::new(vec![tok(False), tok(Or), tok(False), tok(Eof)])
                .or()
                .evaluate(&tags)
        );
        assert_eq!(
            Value::Bool(true),
            Parser::new(vec![tok(True), tok(Or), tok(False), tok(Eof)])
                .or()
                .evaluate(&tags)
        );
        assert_eq!(
            Value::Bool(true),
            Parser::new(vec![tok(False), tok(Or), tok(True), tok(Eof)])
                .or()
                .evaluate(&tags)
        );
        assert_eq!(
            Value::Bool(true),
            Parser::new(vec![tok(True), tok(Or), tok(True), tok(Eof)])
                .or()
                .evaluate(&tags)
        );
    }

    #[test]
    fn test_grouping() {
        let expr = Expr::parse("false or ((false and true) or true)").unwrap();
        let tags = Tags::new();
        assert_eq!(Value::Bool(true), expr.evaluate(&tags));
    }

    #[test]
    fn test_mixing_string_bool() {
        let expr = Expr::parse("false or \"foo\"").unwrap();
        let tags = Tags::new();
        assert_eq!(Value::String("foo".into()), expr.evaluate(&tags));

        let expr = Expr::parse("true and \"foo\"").unwrap();
        let tags = Tags::new();
        assert_eq!(Value::String("foo".into()), expr.evaluate(&tags));

        let expr = Expr::parse("\"foo\" and true").unwrap();
        let tags = Tags::new();
        assert_eq!(Value::Bool(true), expr.evaluate(&tags));
    }

    #[test]
    fn test_tag_insertion() {
        use crate::Tag;
        let expr = Expr::parse("@foo or (@bar = \"any\")").unwrap();

        {
            let tags = Tags::new();
            assert_eq!(Value::Bool(false), expr.evaluate(&tags));
        }

        {
            let mut tags = Tags::new();
            tags.insert(Tag::new("foo".to_string(), None));
            assert_eq!(Value::Bool(true), expr.evaluate(&tags));
        }

        {
            let mut tags = Tags::new();
            tags.insert(Tag::new("bar".to_string(), Some("something".to_string())));
            assert_eq!(Value::Bool(false), expr.evaluate(&tags));
        }

        {
            let mut tags = Tags::new();
            tags.insert(Tag::new("bar".to_string(), Some("any".to_string())));
            assert_eq!(Value::Bool(true), expr.evaluate(&tags));
        }
    }
}
